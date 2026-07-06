//! pi0 native addon — keylogger, screenshots, and data store for the pi0
//! Electron app. macOS-only (IOKit HID + NSWorkspace + ScreenCaptureKit).
//!
//! Threading: HID capture runs on a dedicated thread with its own `CFRunLoop`
//! (see [`engine`]); the `NSWorkspace` observer and every `#[napi]` entry point
//! run on the JS main thread. `!Send` objc state never crosses threads — only
//! the `Send` cells in [`state::Shared`] do.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use napi::bindgen_prelude::{AsyncTask, Error, Result};
use napi::{Env, Task};
use napi_derive::napi;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};

mod app_monitor;
mod callbacks;
mod capture;
mod clock;
mod context_store;
mod db;
mod engine;
mod keymap;
mod ocr;
mod paths;
mod state;
mod writer;

use engine::EngineHandle;
use state::{AppName, Shared};
use writer::Record;

// ---- process-global engine state ------------------------------------------

/// The live engine (HID thread + handles), or `None` when stopped. `Send`, so
/// it lives in a process-global mutex.
fn engine_slot() -> &'static Mutex<Option<EngineHandle>> {
    static SLOT: OnceLock<Mutex<Option<EngineHandle>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

thread_local! {
    /// The NSWorkspace activation observer token. `!Send`, so it lives in a
    /// main-thread thread-local; `start`/`stop` are always called on that thread.
    static OBSERVER: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        const { RefCell::new(None) };
}

fn remove_observer() {
    OBSERVER.with(|slot| {
        if let Some(token) = slot.borrow_mut().take() {
            app_monitor::remove(&token);
        }
    });
}

// ---- napi value types ------------------------------------------------------

/// Capture configuration passed from the main process at `start`. The JS side
/// drives the (now adaptive) capture timer, reading `last_activity_ms` to choose
/// the interval, so no cadence is passed down here.
#[napi(object)]
pub struct EngineConfig {
    /// Absolute data directory (e.g. `<userData>/pi0-data`).
    pub data_dir: String,
}

/// A keystroke record returned by `query_text` (maps to zod `TextRecordSchema`).
#[napi(object)]
pub struct TextRecord {
    /// Epoch milliseconds (JS number) — the UTC instant.
    pub ts: f64,
    /// Local wall-clock at `ts`, ISO-8601 without offset.
    pub local_time: String,
    /// IANA timezone name the record was captured in.
    pub tz_name: String,
    pub app: String,
    pub app_raw: String,
    pub text: String,
}

impl From<Record> for TextRecord {
    fn from(r: Record) -> Self {
        Self {
            ts: r.ts as f64,
            local_time: r.local_time,
            tz_name: r.tz_name,
            app: r.app,
            app_raw: r.app_raw,
            text: r.text,
        }
    }
}

/// Parameters for a time-range text query.
#[napi(object)]
pub struct QueryParams {
    pub start_ms: f64,
    pub end_ms: f64,
}

/// macOS TCC permission status for the two grants pi0 needs.
#[napi(object)]
pub struct PermissionStatus {
    pub input_monitoring: bool,
    pub screen_recording: bool,
}

// ---- lifecycle -------------------------------------------------------------

/// Start capture: install the main-thread NSWorkspace observer and spawn the
/// HID thread. Returns an error (with a TCC hint) if Input Monitoring is denied.
/// Idempotent: a no-op if already running.
#[napi]
pub fn start(config: EngineConfig) -> Result<()> {
    if is_running() {
        return Ok(());
    }

    let data_dir = PathBuf::from(&config.data_dir);
    let shared = Shared::new();

    // Observer lives on the main thread; keep its token for removal on stop.
    let token = app_monitor::install(Arc::clone(&shared));
    OBSERVER.with(|slot| *slot.borrow_mut() = Some(token));

    match engine::spawn(data_dir.clone(), Arc::clone(&shared)) {
        Ok(handle) => {
            *engine_slot().lock().unwrap() = Some(handle);
            // Contextualise any screenshots a previous run left behind (crash,
            // hard quit): every picture on disk is pending OCR by definition.
            ocr::enqueue_sweep(&data_dir);
            Ok(())
        }
        Err(err) => {
            remove_observer();
            Err(Error::from_reason(format!("{err:#}")))
        }
    }
}

/// Stop capture: stop the HID run loop, join the thread (flushing buffered
/// keystrokes), and remove the observer. Idempotent.
#[napi]
pub fn stop() -> Result<()> {
    let handle = engine_slot().lock().unwrap().take();
    if let Some(handle) = handle {
        handle.stop();
    }
    remove_observer();
    Ok(())
}

/// Whether capture is currently running (engine present and its HID run loop live).
#[napi]
pub fn is_running() -> bool {
    engine_slot()
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |h| h.is_running())
}

/// Epoch milliseconds of the most recent input activity — a keystroke, pointer
/// or scroll-wheel motion, or a button press — seen by the HID thread, or `0`
/// when the engine isn't running. The
/// JS side compares this against its idle-timeout window to pick the adaptive
/// capture interval (active vs idle). Returned as an f64 for JS number parity.
#[napi]
pub fn last_activity_ms() -> f64 {
    engine_slot()
        .lock()
        .unwrap()
        .as_ref()
        .map_or(0.0, |h| h.shared.last_activity_ms() as f64)
}

// ---- database (encrypted store) --------------------------------------------

/// Whether an encrypted pi0 database already exists under `dataDir` (i.e. this
/// is not first run — the UI shows "unlock" instead of "create").
#[napi]
pub fn db_exists(data_dir: String) -> bool {
    db::exists(Path::new(&data_dir))
}

/// Open (or create, on first run) the encrypted database with `password`.
/// Returns whether it was newly created. Rejects an incorrect password.
#[napi]
pub fn open_db(data_dir: String, password: String) -> Result<bool> {
    db::open(Path::new(&data_dir), &password).map_err(|e| Error::from_reason(format!("{e:#}")))
}

/// Whether the database is currently unlocked.
#[napi]
pub fn is_db_open() -> bool {
    db::is_open()
}

/// Change the database password. Verifies `current` against the password the DB
/// was unlocked with, then re-encrypts in place.
#[napi]
pub fn change_password(current: String, new_password: String) -> Result<()> {
    db::change_password(&current, &new_password).map_err(|e| Error::from_reason(format!("{e:#}")))
}

/// The MCP access token (minted and stored on first call). Requires an unlocked DB.
#[napi]
pub fn mcp_token() -> Result<String> {
    db::mcp_token().map_err(|e| Error::from_reason(format!("{e:#}")))
}

/// Checkpoint and close the database (called before quit).
#[napi]
pub fn close_db() {
    db::close();
}

// ---- screenshots (ScreenCaptureKit) ----------------------------------------

/// Async ScreenCaptureKit capture running on a libuv worker thread. Reads the
/// current app name up front (on the caller's thread) so the worker never
/// touches objc/main-thread state.
pub struct CaptureTask {
    data_dir: PathBuf,
    app: AppName,
}

impl Task for CaptureTask {
    type Output = Vec<String>;
    type JsValue = Vec<String>;

    fn compute(&mut self) -> Result<Self::Output> {
        let shots = capture::capture_to_file(&self.data_dir, &self.app)
            .map_err(|e| Error::from_reason(format!("{e:#}")))?;
        // Hand every written PNG to the OCR worker, which contextualises it
        // into the store and deletes the picture afterwards.
        let written = shots
            .iter()
            .map(|s| s.path.to_string_lossy().into_owned())
            .collect();
        for shot in shots {
            ocr::enqueue_shot(ocr::PendingShot {
                png_path: shot.path,
                ts: shot.ts,
                local_time: shot.local_time,
                tz_name: shot.tz_name,
                app: self.app.sanitized.clone(),
                app_raw: self.app.raw.clone(),
                display: shot.display,
            });
        }
        Ok(written)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

/// Capture a screenshot of the main display into the active app's `shots/`
/// folder. Returns a Promise resolving to the written PNG path. Requires a
/// running engine (for the active-app name + data dir).
#[napi]
pub fn capture_snapshot() -> Result<AsyncTask<CaptureTask>> {
    let slot = engine_slot().lock().unwrap();
    let handle = slot
        .as_ref()
        .ok_or_else(|| Error::from_reason("capture_snapshot: engine is not running"))?;
    let app = (*handle.shared.current_app()).clone();
    let data_dir = handle.data_dir.clone();
    Ok(AsyncTask::new(CaptureTask { data_dir, app }))
}

// ---- data query ------------------------------------------------------------

/// Async time-range keystroke query against the store. Runs on a libuv worker.
pub struct QueryTask {
    start_ms: i64,
    end_ms: i64,
}

impl Task for QueryTask {
    type Output = Vec<Record>;
    type JsValue = Vec<TextRecord>;

    fn compute(&mut self) -> Result<Self::Output> {
        db::query_text(self.start_ms, self.end_ms).map_err(|e| Error::from_reason(format!("{e:#}")))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into_iter().map(TextRecord::from).collect())
    }
}

/// Query recorded keystroke text within `[startMs, endMs]`. Returns a Promise.
#[napi]
pub fn query_text(params: QueryParams) -> AsyncTask<QueryTask> {
    AsyncTask::new(QueryTask {
        start_ms: params.start_ms as i64,
        end_ms: params.end_ms as i64,
    })
}

// ---- activity timeline queries (powers the MCP server) ---------------------

/// One OCR'd text line; coordinates are normalised to `[0, 1]` relative to the
/// captured display (x/y = top-left of the box, w/h = its size).
#[napi(object)]
pub struct OcrItem {
    pub text: String,
    /// Recognition confidence in `[0, 1]`.
    pub score: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One entry in the merged activity timeline (maps to zod `TimelineRecordSchema`).
/// `kind` is `"ocr"` or `"keys"`; only that kind's fields are populated.
#[napi(object)]
pub struct TimelineRecord {
    /// Epoch milliseconds — the screenshot instant (OCR) or buffer start (keys).
    pub ts: f64,
    /// Local wall-clock at `ts`, ISO-8601 without offset.
    pub local_time: String,
    /// IANA timezone name the record was captured in.
    pub tz_name: String,
    pub app: String,
    pub app_raw: String,
    /// `"ocr"` (screen context) or `"keys"` (keystroke record).
    pub kind: String,
    /// OCR only: display index the shot came from (0 = main display).
    pub display: Option<u32>,
    /// OCR only: recognised text lines with normalised coordinates.
    pub items: Option<Vec<OcrItem>>,
    /// Keystrokes only: the raw captured text for this buffer.
    pub text: Option<String>,
}

impl From<context_store::TimelineRecord> for TimelineRecord {
    fn from(r: context_store::TimelineRecord) -> Self {
        use context_store::TimelineKind;
        let (kind, items) = match r.kind {
            TimelineKind::Ocr => (
                "ocr".to_string(),
                Some(
                    r.items
                        .into_iter()
                        .map(|i| OcrItem {
                            text: i.text,
                            score: i.score,
                            x: i.x,
                            y: i.y,
                            w: i.w,
                            h: i.h,
                        })
                        .collect(),
                ),
            ),
            TimelineKind::Keys => ("keys".to_string(), None),
        };
        Self {
            ts: r.ts as f64,
            local_time: r.local_time,
            tz_name: r.tz_name,
            app: r.app,
            app_raw: r.app_raw,
            kind,
            display: r.display,
            items,
            text: r.text,
        }
    }
}

/// Parameters for a paginated context query.
#[napi(object)]
pub struct ContextQueryParams {
    pub start_ms: f64,
    pub end_ms: f64,
    /// Optional app filter (matches sanitized or raw name, case-insensitive).
    pub app: Option<String>,
    /// Records to skip (pagination).
    pub offset: u32,
    /// Max records to return.
    pub limit: u32,
}

/// One page of timeline records plus the range's total match count.
#[napi(object)]
pub struct TimelinePage {
    pub total: u32,
    pub records: Vec<TimelineRecord>,
}

/// Async paginated timeline query against the store. Runs on a libuv worker.
pub struct TimelineQueryTask {
    params: ContextQueryParams,
}

impl Task for TimelineQueryTask {
    type Output = context_store::TimelinePage;
    type JsValue = TimelinePage;

    fn compute(&mut self) -> Result<Self::Output> {
        db::query_timeline(
            self.params.start_ms as i64,
            self.params.end_ms as i64,
            self.params.app.as_deref(),
            self.params.offset as usize,
            self.params.limit as usize,
        )
        .map_err(|e| Error::from_reason(format!("{e:#}")))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(TimelinePage {
            total: output.total,
            records: output
                .records
                .into_iter()
                .map(TimelineRecord::from)
                .collect(),
        })
    }
}

/// Query the merged activity timeline (OCR contexts + keystroke records) within
/// `[startMs, endMs]`, optionally filtered by app, returning the
/// `[offset, offset + limit)` page plus the total match count.
#[napi]
pub fn query_timeline(params: ContextQueryParams) -> AsyncTask<TimelineQueryTask> {
    AsyncTask::new(TimelineQueryTask { params })
}

/// Per-app usage aggregate over a time range (maps to zod `AppUsageSchema`).
#[napi(object)]
pub struct AppUsage {
    pub app: String,
    pub app_raw: String,
    pub first_ts: f64,
    pub last_ts: f64,
    /// Number of keystroke records in the range.
    pub text_records: u32,
    /// Number of OCR'd screenshot contexts in the range.
    pub context_records: u32,
}

/// Async aggregation of per-app usage across both stores. Runs on a libuv worker.
pub struct AppsQueryTask {
    start_ms: i64,
    end_ms: i64,
}

impl Task for AppsQueryTask {
    type Output = Vec<context_store::AppUsage>;
    type JsValue = Vec<AppUsage>;

    fn compute(&mut self) -> Result<Self::Output> {
        db::query_apps(self.start_ms, self.end_ms).map_err(|e| Error::from_reason(format!("{e:#}")))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output
            .into_iter()
            .map(|u| AppUsage {
                app: u.app,
                app_raw: u.app_raw,
                first_ts: u.first_ts as f64,
                last_ts: u.last_ts as f64,
                text_records: u.text_records,
                context_records: u.context_records,
            })
            .collect())
    }
}

/// Aggregate which apps were used within `[startMs, endMs]` (keystroke records
/// + OCR contexts), most recently active first.
#[napi]
pub fn query_apps(params: QueryParams) -> AsyncTask<AppsQueryTask> {
    AsyncTask::new(AppsQueryTask {
        start_ms: params.start_ms as i64,
        end_ms: params.end_ms as i64,
    })
}

// ---- permissions -----------------------------------------------------------

/// Best-effort macOS TCC status probe.
#[napi]
pub fn permissions_status() -> PermissionStatus {
    PermissionStatus {
        input_monitoring: input_monitoring_granted(),
        screen_recording: capture::screen_recording_granted(),
    }
}

fn input_monitoring_granted() -> bool {
    use objc2_io_kit::{IOHIDAccessType, IOHIDCheckAccess, IOHIDRequestType};
    IOHIDCheckAccess(IOHIDRequestType::ListenEvent) == IOHIDAccessType::Granted
}

/// Trigger the macOS Input Monitoring (IOHID ListenEvent) TCC prompt and return
/// whether access is granted afterwards. The first call registers pi0 in the
/// Input Monitoring list so the user can grant it; later calls just report status.
#[napi]
pub fn request_input_monitoring() -> bool {
    use objc2_io_kit::{IOHIDRequestAccess, IOHIDRequestType};
    IOHIDRequestAccess(IOHIDRequestType::ListenEvent)
}

/// Trigger the macOS Screen Recording TCC prompt and return whether access is
/// granted. The first call registers pi0 in the Screen Recording list; the grant
/// itself typically takes effect only after a relaunch.
#[napi]
pub fn request_screen_recording() -> bool {
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

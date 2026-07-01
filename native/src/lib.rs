//! pi0 native addon — keylogger, screenshots, and data store for the pi0
//! Electron app. macOS-only (IOKit HID + NSWorkspace + ScreenCaptureKit).
//!
//! Threading: HID capture runs on a dedicated thread with its own `CFRunLoop`
//! (see [`engine`]); the `NSWorkspace` observer and every `#[napi]` entry point
//! run on the JS main thread. `!Send` objc state never crosses threads — only
//! the `Send` cells in [`state::Shared`] do.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use napi::bindgen_prelude::{AsyncTask, Error, Result};
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, JsFunction, JsUnknown, Task};
use napi_derive::napi;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};

mod app_monitor;
mod callbacks;
mod capture;
mod engine;
mod keymap;
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

/// Capture configuration passed from the main process at `start`.
#[napi(object)]
pub struct EngineConfig {
    /// Absolute data directory (e.g. `<userData>/pi0-data`).
    pub data_dir: String,
    /// Snapshot interval in ms (the JS side drives the timer; kept for parity).
    pub interval_ms: u32,
    /// Hotkey combo as key tokens, e.g. `["LC", "LS", "S"]`.
    pub hotkey: Vec<String>,
    /// Whether the hotkey should trigger a screenshot.
    pub capture_on_hotkey: bool,
}

/// A keystroke record returned by `query_text` (maps to zod `TextRecordSchema`).
#[napi(object)]
pub struct TextRecord {
    /// Epoch milliseconds (JS number).
    pub ts: f64,
    pub app: String,
    pub app_raw: String,
    pub text: String,
}

impl From<Record> for TextRecord {
    fn from(r: Record) -> Self {
        Self {
            ts: r.ts as f64,
            app: r.app,
            app_raw: r.app_raw,
            text: r.text,
        }
    }
}

/// Parameters for a time-range text query.
#[napi(object)]
pub struct QueryParams {
    /// Absolute data directory to scan.
    pub data_dir: String,
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
pub fn start(config: EngineConfig, on_hotkey: JsFunction) -> Result<()> {
    if is_running() {
        return Ok(());
    }

    let data_dir = PathBuf::from(&config.data_dir);
    let codes = resolve_hotkey(&config.hotkey);
    let shared = Shared::new(codes, config.capture_on_hotkey);

    // Observer lives on the main thread; keep its token for removal on stop.
    let token = app_monitor::install(Arc::clone(&shared));
    OBSERVER.with(|slot| *slot.borrow_mut() = Some(token));

    // Bridge the JS hotkey callback to a Send notifier the HID thread can call.
    let tsfn: ThreadsafeFunction<(), ErrorStrategy::Fatal> = on_hotkey
        .create_threadsafe_function(0, |_ctx: ThreadSafeCallContext<()>| {
            Ok(Vec::<JsUnknown>::new())
        })?;
    let notify: Box<dyn Fn() + Send> = Box::new(move || {
        tsfn.call((), ThreadsafeFunctionCallMode::NonBlocking);
    });

    match engine::spawn(data_dir, Arc::clone(&shared), notify) {
        Ok(handle) => {
            *engine_slot().lock().unwrap() = Some(handle);
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

/// Update the hotkey combo and capture-on-hotkey flag on a live engine. The
/// snapshot interval is driven by the JS `setInterval`, so it is ignored here.
#[napi]
pub fn update_settings(
    _interval_ms: u32,
    hotkey: Vec<String>,
    capture_on_hotkey: bool,
) -> Result<()> {
    if let Some(handle) = engine_slot().lock().unwrap().as_ref() {
        handle.shared.set_hotkey(resolve_hotkey(&hotkey));
        handle.shared.set_capture_on_hotkey(capture_on_hotkey);
    }
    Ok(())
}

fn resolve_hotkey(tokens: &[String]) -> Vec<u32> {
    tokens
        .iter()
        .filter_map(|t| keymap::scancode_for_name(t))
        .collect()
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
        capture::capture_to_file(&self.data_dir, &self.app)
            .map_err(|e| Error::from_reason(format!("{e:#}")))
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

/// Async filesystem scan of `records.jsonl` files in a time range. Runs on a
/// libuv worker thread (no objc state touched).
pub struct QueryTask {
    data_dir: PathBuf,
    start_ms: i64,
    end_ms: i64,
}

impl Task for QueryTask {
    type Output = Vec<Record>;
    type JsValue = Vec<TextRecord>;

    fn compute(&mut self) -> Result<Self::Output> {
        writer::query(&self.data_dir, self.start_ms, self.end_ms)
            .map_err(|e| Error::from_reason(format!("{e:#}")))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into_iter().map(TextRecord::from).collect())
    }
}

/// Query recorded keystroke text within `[startMs, endMs]`. Returns a Promise.
#[napi]
pub fn query_text(params: QueryParams) -> AsyncTask<QueryTask> {
    AsyncTask::new(QueryTask {
        data_dir: PathBuf::from(params.data_dir),
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

//! On-device OCR: contextualise screenshots into text (with normalised
//! coordinates) and delete the picture afterwards.
//!
//! Uses PP-OCRv6 small det/rec MNN models via `ocr-rs`, CPU-only. The model
//! files are embedded in the addon binary (`include_bytes!`), so the app
//! bundle is self-contained — nothing to download or install at runtime.
//!
//! Threading: everything runs on one dedicated `pi0-ocr` thread fed by a
//! channel, mirroring the HID-thread pattern. That keeps the multi-second
//! engine construction and the CPU-heavy inference off both the JS main
//! thread and the libuv worker pool, and serialises inference so concurrent
//! captures can't pile up parallel OCR runs.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Context, Result};
use ocr_rs::{Backend, OcrEngine, OcrEngineConfig};

use crate::context_store::{ContextRecord, OcrItem};
use crate::db;

// PP-OCRv6 small tier: detection + recognition + the tier's charset (the
// charset is tier-specific and mandatory for decoding recognition output).
const DET_MODEL: &[u8] = include_bytes!("../models/PP-OCRv6_small_det.mnn");
const REC_MODEL: &[u8] = include_bytes!("../models/PP-OCRv6_small_rec.mnn");
const CHARSET: &[u8] = include_bytes!("../models/ppocr_keys_v6_small.txt");

/// One written screenshot waiting to be contextualised.
pub struct PendingShot {
    /// The PNG to OCR — deleted once processed, success or not.
    pub png_path: PathBuf,
    /// Epoch ms the screenshot was taken (shared across a multi-display set).
    pub ts: i64,
    /// Sanitized (folder-safe) app name.
    pub app: String,
    /// Original localizedName of the app.
    pub app_raw: String,
    /// Display index (0 = main display).
    pub display: u32,
}

enum Job {
    Shot(PendingShot),
    /// Scan a data dir for stray shot PNGs (crash/quit leftovers) and process
    /// them: any picture on disk is by definition pending contextualisation.
    Sweep(PathBuf),
}

/// Queue a freshly captured screenshot for OCR + deletion.
pub fn enqueue_shot(shot: PendingShot) {
    submit(Job::Shot(shot));
}

/// Queue a sweep for leftover screenshots under `data_dir`.
pub fn enqueue_sweep(data_dir: &Path) {
    submit(Job::Sweep(data_dir.to_path_buf()));
}

fn submit(job: Job) {
    let tx = sender().lock().unwrap();
    if tx.send(job).is_err() {
        eprintln!("[pi0] OCR worker is gone; screenshot left on disk for the next sweep");
    }
}

/// The channel into the lazily spawned `pi0-ocr` worker thread.
fn sender() -> &'static Mutex<Sender<Job>> {
    static TX: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("pi0-ocr".to_string())
            .spawn(move || worker_loop(rx))
            .expect("failed to spawn OCR thread");
        Mutex::new(tx)
    })
}

/// Build the engine from the embedded models. CPU backend is pinned
/// explicitly (a hard M3 requirement — no GPU delegates).
fn build_engine() -> Result<OcrEngine> {
    OcrEngine::from_bytes(
        DET_MODEL,
        REC_MODEL,
        CHARSET,
        Some(OcrEngineConfig::new().with_backend(Backend::CPU)),
    )
    .map_err(|e| anyhow!("OCR engine init failed: {e}"))
}

fn worker_loop(rx: Receiver<Job>) {
    // Built once, on first use: parses the embedded MNN models.
    let engine = match build_engine() {
        Ok(engine) => engine,
        Err(err) => {
            // Leaves every queued send failing loudly; shots stay for a sweep
            // after the underlying problem is fixed.
            eprintln!("[pi0] {err}");
            return;
        }
    };

    while let Ok(job) = rx.recv() {
        match job {
            Job::Shot(shot) => process_shot(&engine, &shot),
            Job::Sweep(data_dir) => {
                for shot in find_stray_shots(&data_dir) {
                    process_shot(&engine, &shot);
                }
            }
        }
    }
}

/// OCR one screenshot into the store, then delete the picture. The image never
/// outlives contextualisation — even when OCR itself fails (the failure is
/// logged and the pixels dropped) — with one exception: if the database is
/// locked the text couldn't be stored, so the picture is kept for the next
/// sweep to retry rather than lost.
fn process_shot(engine: &OcrEngine, shot: &PendingShot) {
    if let Err(err) = contextualise(engine, shot) {
        eprintln!("[pi0] OCR failed for {}: {err:#}", shot.png_path.display());
        if !db::is_open() {
            return; // DB locked — leave the picture for a later sweep.
        }
    }
    if let Err(err) = std::fs::remove_file(&shot.png_path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            eprintln!("[pi0] failed to delete {}: {err}", shot.png_path.display());
        }
    }
    // Drop the per-app folder once it's empty again (fails harmlessly if not).
    if let Some(parent) = shot.png_path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

fn contextualise(engine: &OcrEngine, shot: &PendingShot) -> Result<()> {
    let img = image::open(&shot.png_path)
        .with_context(|| format!("opening {}", shot.png_path.display()))?;
    let (width, height) = (f64::from(img.width()), f64::from(img.height()));

    let mut results = engine
        .recognize(&img)
        .map_err(|e| anyhow!("recognize: {e}"))?;
    // Approximate reading order: top-to-bottom, then left-to-right.
    results.sort_by_key(|r| (r.bbox.rect.top(), r.bbox.rect.left()));

    let items: Vec<OcrItem> = results
        .into_iter()
        .map(|r| OcrItem {
            text: r.text,
            score: f64::from(r.confidence),
            x: (f64::from(r.bbox.rect.left()) / width).clamp(0.0, 1.0),
            y: (f64::from(r.bbox.rect.top()) / height).clamp(0.0, 1.0),
            w: (f64::from(r.bbox.rect.width()) / width).clamp(0.0, 1.0),
            h: (f64::from(r.bbox.rect.height()) / height).clamp(0.0, 1.0),
        })
        .collect();

    // An empty item list is still recorded: "this app was frontmost at ts with
    // nothing readable on screen" is context too.
    db::insert_context(&ContextRecord {
        ts: shot.ts,
        app: shot.app.clone(),
        app_raw: shot.app_raw.clone(),
        display: shot.display,
        items,
    })
}

/// Walk `<data_dir>/<app>/<ts>-<display>.png` for pictures a previous run left
/// behind (crash/quit while queued). The app name comes from the folder (the
/// raw name is unrecoverable, so it doubles as `appRaw`); ts + display index are
/// parsed from the file name.
fn find_stray_shots(data_dir: &Path) -> Vec<PendingShot> {
    let mut out = Vec::new();
    let Ok(apps) = std::fs::read_dir(data_dir) else {
        return out;
    };
    for app_entry in apps.flatten().filter(is_dir) {
        let app = app_entry.file_name().to_string_lossy().into_owned();
        let Ok(shots) = std::fs::read_dir(app_entry.path()) else {
            continue;
        };
        for shot in shots.flatten() {
            let path = shot.path();
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some((ts, display)) = parse_shot_stem(stem) else {
                continue;
            };
            out.push(PendingShot {
                png_path: path,
                ts,
                app: app.clone(),
                app_raw: app.clone(),
                display,
            });
        }
    }
    out
}

fn is_dir(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
}

/// `"1751527334123-2"` → `(1751527334123, 2)`. Rejects anything not shaped like
/// `<ts>-<display>`.
fn parse_shot_stem(stem: &str) -> Option<(i64, u32)> {
    let (ts, display) = stem.rsplit_once('-')?;
    Some((ts.parse().ok()?, display.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shot_stems() {
        assert_eq!(parse_shot_stem("1751527334123-0"), Some((1751527334123, 0)));
        assert_eq!(parse_shot_stem("1751527334123-2"), Some((1751527334123, 2)));
        assert_eq!(parse_shot_stem("1751527334123"), None);
        assert_eq!(parse_shot_stem("not-a-shot"), None);
        assert_eq!(parse_shot_stem("123-x"), None);
    }

    /// Full pipeline smoke test against a real image with text. Opt-in (engine
    /// init + inference are slow):
    /// `PI0_OCR_TEST_IMAGE=/path/to/text.png cargo test -- --ignored --nocapture`
    #[test]
    #[ignore = "needs PI0_OCR_TEST_IMAGE pointing at a PNG containing text"]
    fn ocr_smoke_end_to_end() {
        let src = std::env::var("PI0_OCR_TEST_IMAGE").expect("set PI0_OCR_TEST_IMAGE");
        let data_dir = std::env::temp_dir().join(format!("pi0-ocr-smoke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        db::open(&data_dir, "test-pw").expect("open db");

        let ts = crate::paths::now_ms();
        let app_dir = crate::paths::app_dir(&data_dir, "TestApp");
        std::fs::create_dir_all(&app_dir).unwrap();
        let png = crate::paths::shot_path(&data_dir, "TestApp", ts, 0);
        std::fs::copy(&src, &png).unwrap();

        let engine = build_engine().expect("engine should build from embedded models");
        process_shot(
            &engine,
            &PendingShot {
                png_path: png.clone(),
                ts,
                app: "TestApp".to_string(),
                app_raw: "TestApp".to_string(),
                display: 0,
            },
        );

        assert!(!png.exists(), "picture must be deleted after OCR");

        let page = db::query_contexts(ts - 1, ts + 1, Some("TestApp"), 0, 10).unwrap();
        let record = &page.records[0];
        assert_eq!(record.ts, ts);
        assert!(!record.items.is_empty(), "expected recognised text lines");
        for item in &record.items {
            println!(
                "  [{:.3},{:.3} {:.3}x{:.3}] {:.2} {}",
                item.x, item.y, item.w, item.h, item.score, item.text
            );
            for v in [item.x, item.y, item.w, item.h] {
                assert!((0.0..=1.0).contains(&v), "coordinate out of [0,1]: {v}");
            }
        }

        db::close();
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

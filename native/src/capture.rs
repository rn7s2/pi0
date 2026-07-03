//! Screenshot capture via ScreenCaptureKit (the current, non-deprecated API;
//! `CGDisplayCreateImage` is removed in macOS 15).
//!
//! ScreenCaptureKit is completion-handler based and runs its completions on its
//! own internal queues, so we can kick off the async chain and block the calling
//! (libuv worker) thread on a channel without deadlocking. Isolating all SCK
//! calls here keeps the framework swappable.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_core_graphics::{CGDataProvider, CGImage};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    SCWindow,
};

use crate::paths;
use crate::state::AppName;

/// How long to wait for the async screenshot chain before giving up.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// One written screenshot file plus the metadata the OCR pipeline needs to
/// contextualise (and then delete) it.
pub struct WrittenShot {
    pub path: std::path::PathBuf,
    /// Display index (0 = main display).
    pub display: u32,
    /// Epoch ms of the capture (shared across a multi-display set).
    pub ts: i64,
}

/// Capture **all attached displays** and write a PNG per display under
/// `<data_dir>/<date>/<app>/shots/`, all sharing one `<ts>` so the set is
/// grouped: `<ts>.png` for a single display, or `<ts>-m<i>.png` (main first)
/// when there are several. Returns the written files with their metadata.
pub fn capture_to_file(data_dir: &Path, app: &AppName) -> Result<Vec<WrittenShot>> {
    let shots = capture_all_displays_png()?;
    let ts = paths::now_ms();
    let date = paths::local_date_for_ms(ts);
    let dir = paths::shots_dir(data_dir, &date, &app.sanitized);
    std::fs::create_dir_all(&dir)?;

    let multi = shots.len() > 1;
    let mut written = Vec::with_capacity(shots.len());
    for (index, png) in &shots {
        let name = if multi {
            format!("{ts}-m{index}.png")
        } else {
            format!("{ts}.png")
        };
        let path = dir.join(name);
        std::fs::write(&path, png)?;
        written.push(WrittenShot {
            path,
            display: *index as u32,
            ts,
        });
    }
    Ok(written)
}

/// Whether the process currently has Screen Recording (TCC) access.
pub fn screen_recording_granted() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

/// Messages from the async SCK completion blocks to the blocking collector.
enum Msg {
    /// Number of displays that will be captured.
    Count(usize),
    /// One display's result: `(display index, PNG bytes | error)`.
    Shot(usize, std::result::Result<Vec<u8>, String>),
    /// Fatal failure before any capture was started (e.g. permission denied).
    Fail(String),
}

/// Run the async SCK chain (enumerate content → capture every display) and block
/// until all per-display results arrive or it times out. Returns `(index, png)`
/// pairs sorted with the main display first.
fn capture_all_displays_png() -> Result<Vec<(usize, Vec<u8>)>> {
    let (tx, rx) = mpsc::channel::<Msg>();
    let tx_content = tx.clone();

    // Stage 1: enumerate shareable displays.
    let content_block = RcBlock::new(
        move |content: *mut SCShareableContent, _err: *mut NSError| {
            if content.is_null() {
                let _ = tx_content.send(Msg::Fail(
                    "no shareable content — grant Screen Recording (System Settings → Privacy \
                 & Security → Screen Recording) and relaunch"
                        .to_string(),
                ));
                return;
            }
            let content = unsafe { &*content };
            let display_array = unsafe { content.displays() };
            let displays = order_main_first(&display_array);
            if displays.is_empty() {
                let _ = tx_content.send(Msg::Fail("no displays available to capture".to_string()));
                return;
            }
            let _ = tx_content.send(Msg::Count(displays.len()));

            // Stage 2: capture each display (main display gets index 0).
            for (index, display) in displays.into_iter().enumerate() {
                let empty_windows: Retained<NSArray<SCWindow>> = NSArray::new();
                let filter = unsafe {
                    SCContentFilter::initWithDisplay_excludingWindows(
                        SCContentFilter::alloc(),
                        &display,
                        &empty_windows,
                    )
                };
                let config = unsafe { SCStreamConfiguration::new() };
                unsafe {
                    config.setWidth(display.width().max(0) as usize);
                    config.setHeight(display.height().max(0) as usize);
                }

                let tx_shot = tx_content.clone();
                let shot_block = RcBlock::new(move |image: *mut CGImage, _err: *mut NSError| {
                    let result = if image.is_null() {
                        Err("screenshot returned a null image".to_string())
                    } else {
                        encode_png(unsafe { &*image }).map_err(|e| format!("{e:#}"))
                    };
                    let _ = tx_shot.send(Msg::Shot(index, result));
                });
                unsafe {
                    SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                        &filter,
                        &config,
                        Some(&shot_block),
                    );
                }
            }
        },
    );

    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&content_block) };

    // Collect the display count, then that many per-display results.
    let count = match rx.recv_timeout(CAPTURE_TIMEOUT) {
        Ok(Msg::Count(n)) => n,
        Ok(Msg::Fail(message)) => return Err(anyhow!(message)),
        Ok(_) => return Err(anyhow!("unexpected capture message")),
        Err(_) => return Err(anyhow!("screenshot timed out")),
    };

    let mut shots: Vec<(usize, Vec<u8>)> = Vec::with_capacity(count);
    let mut errors: Vec<String> = Vec::new();
    for _ in 0..count {
        match rx.recv_timeout(CAPTURE_TIMEOUT) {
            Ok(Msg::Shot(index, Ok(bytes))) => shots.push((index, bytes)),
            Ok(Msg::Shot(_, Err(e))) => errors.push(e),
            Ok(_) => {}
            Err(_) => {
                errors.push("timed out".to_string());
                break;
            }
        }
    }

    if shots.is_empty() {
        return Err(anyhow!(
            "all display captures failed: {}",
            errors.join("; ")
        ));
    }
    shots.sort_by_key(|(index, _)| *index);
    Ok(shots)
}

/// Collect the shareable displays with the main display first (index 0).
fn order_main_first(displays: &NSArray<SCDisplay>) -> Vec<Retained<SCDisplay>> {
    let main = objc2_core_graphics::CGMainDisplayID();
    let count = displays.count();
    let mut all: Vec<Retained<SCDisplay>> = Vec::with_capacity(count);
    for i in 0..count {
        all.push(displays.objectAtIndex(i));
    }
    // Stable sort: main display first, others keep enumeration order.
    all.sort_by_key(|d| {
        if unsafe { d.displayID() } == main {
            0u8
        } else {
            1u8
        }
    });
    all
}

/// Encode a ScreenCaptureKit `CGImage` (BGRA8888, SDR) as PNG, honoring row
/// padding and swizzling BGRA → RGBA for the `image` crate.
fn encode_png(image: &CGImage) -> Result<Vec<u8>> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let bytes_per_row = CGImage::bytes_per_row(Some(image));
    if width == 0 || height == 0 {
        return Err(anyhow!("captured image has zero size"));
    }

    let provider =
        CGImage::data_provider(Some(image)).ok_or_else(|| anyhow!("image has no data provider"))?;
    let data = CGDataProvider::data(Some(&provider)).ok_or_else(|| anyhow!("image has no data"))?;
    let bytes = unsafe { data.as_bytes_unchecked() };
    if bytes.len() < height * bytes_per_row {
        return Err(anyhow!("pixel buffer smaller than expected"));
    }

    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row_start = y * bytes_per_row;
        let row = &bytes[row_start..row_start + width * 4];
        for px in row.chunks_exact(4) {
            // BGRA → RGBA.
            rgba.push(px[2]);
            rgba.push(px[1]);
            rgba.push(px[0]);
            rgba.push(px[3]);
        }
    }

    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
        .ok_or_else(|| anyhow!("failed to build RGBA image buffer"))?;
    let mut out = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| anyhow!("PNG encode failed: {e}"))?;
    Ok(out.into_inner())
}

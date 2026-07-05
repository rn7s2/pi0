//! Engine state, split by thread ownership:
//!
//! - [`Shared`] is `Send + Sync` and crosses threads: the main-thread
//!   `NSWorkspace` observer writes it; the HID thread reads it. Access is
//!   lock-light (an `ArcSwap` for the frontmost-app cell).
//! - [`HidState`] lives only on the dedicated HID thread. It owns the keystroke
//!   buffer and caps-lock latch with single-threaded interior mutability
//!   (`RefCell`/`Cell`), exactly like the standalone reference.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::clock::{self, Stamp};
use crate::db;
use crate::writer::Record;

/// Rotate the in-memory keystroke buffer into a new record after it has been
/// open this long, so time-range queries stay meaningful.
const FLUSH_INTERVAL_MS: i64 = 5_000;

/// The frontmost app, both raw (`localizedName`) and sanitized (folder-safe).
#[derive(Clone, Debug)]
pub struct AppName {
    pub raw: String,
    pub sanitized: String,
}

impl Default for AppName {
    fn default() -> Self {
        Self {
            raw: "Unknown".to_string(),
            sanitized: "Unknown".to_string(),
        }
    }
}

/// Cross-thread state. Written by the main/HID threads, read by both. The
/// frontmost-app cell is written by the main-thread observer; the last-activity
/// clock is written by the HID thread (on every keystroke and mouse movement)
/// and read by the JS main thread to choose the adaptive capture interval.
pub struct Shared {
    app_name: ArcSwap<AppName>,
    last_activity_ms: AtomicI64,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            app_name: ArcSwap::from_pointee(AppName::default()),
            // Seed with "now" so capture starts in the active cadence: the user
            // just interacted with the app to start it, so treat that as recent
            // activity rather than forcing the idle interval for the first window.
            last_activity_ms: AtomicI64::new(clock::now_ms()),
        })
    }

    pub fn current_app(&self) -> Arc<AppName> {
        self.app_name.load_full()
    }

    pub fn set_app(&self, name: AppName) {
        self.app_name.store(Arc::new(name));
    }

    /// Record that input (keystroke or mouse movement) happened at `ms`. Called
    /// from the HID thread; a relaxed store is enough — the JS reader only needs
    /// a recent-enough value to decide active vs idle, not exact ordering.
    pub fn mark_activity(&self, ms: i64) {
        self.last_activity_ms.store(ms, Ordering::Relaxed);
    }

    /// Epoch ms of the most recent input activity (or the engine start instant if
    /// none yet). The JS timer compares this against its idle-timeout window.
    pub fn last_activity_ms(&self) -> i64 {
        self.last_activity_ms.load(Ordering::Relaxed)
    }
}

/// An open keystroke buffer for one app, tagged with the stamp of its start
/// instant (UTC ms + local wall-clock + zone name, captured once on rotation).
struct Buffer {
    app: AppName,
    text: String,
    stamp: Stamp,
}

/// HID-thread-only state. Reconstructed from the C callback `context` pointer;
/// only ever touched on the HID thread, so `RefCell`/`Cell` need no locking.
pub struct HidState {
    shared: Arc<Shared>,
    buffer: RefCell<Option<Buffer>>,
    capslock: Cell<bool>,
}

impl HidState {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            buffer: RefCell::new(None),
            capslock: Cell::new(false),
        }
    }

    pub fn capslock(&self) -> bool {
        self.capslock.get()
    }

    pub fn toggle_capslock(&self) {
        self.capslock.set(!self.capslock.get());
    }

    /// Record input activity (keystroke or mouse movement) at `now_ms` on the
    /// shared clock the JS side reads to choose the adaptive capture interval.
    pub fn mark_activity(&self, now_ms: i64) {
        self.shared.mark_activity(now_ms);
    }

    /// Ensure the open buffer targets the current app and time window, flushing
    /// and rotating when the app changed or the window elapsed. `now_ms` is the
    /// cheap clock read used for the rotation *decision*; a fresh full [`Stamp`]
    /// (local time + zone) is resolved only when a new buffer is actually opened.
    pub fn rotate_if_needed(&self, now_ms: i64) {
        let current = self.shared.current_app();
        let rotate = {
            let buf = self.buffer.borrow();
            match buf.as_ref() {
                None => true,
                Some(b) => {
                    b.app.sanitized != current.sanitized || now_ms - b.stamp.ts >= FLUSH_INTERVAL_MS
                }
            }
        };
        if rotate {
            if let Some(old) = self.buffer.borrow_mut().take() {
                self.write_record(old);
            }
            *self.buffer.borrow_mut() = Some(Buffer {
                app: (*current).clone(),
                text: String::new(),
                stamp: clock::stamp(),
            });
        }
    }

    /// Append captured text to the open buffer (no-op if none is open).
    pub fn append_text(&self, text: &str) {
        if let Some(b) = self.buffer.borrow_mut().as_mut() {
            b.text.push_str(text);
        }
    }

    /// Flush and drop the open buffer (called on stop).
    pub fn flush(&self) {
        if let Some(b) = self.buffer.borrow_mut().take() {
            self.write_record(b);
        }
    }

    fn write_record(&self, buffer: Buffer) {
        if buffer.text.is_empty() {
            return;
        }
        let record = Record {
            ts: buffer.stamp.ts,
            local_time: buffer.stamp.local_time,
            tz_name: buffer.stamp.tz_name,
            app: buffer.app.sanitized,
            app_raw: buffer.app.raw,
            text: buffer.text,
        };
        if let Err(err) = db::insert_text_record(&record) {
            eprintln!("[pi0] failed to store text record: {err:#}");
        }
    }
}

//! Engine state, split by thread ownership:
//!
//! - [`Shared`] is `Send + Sync` and crosses threads: the main-thread
//!   `NSWorkspace` observer and `update_settings` write it; the HID thread reads
//!   it. Access is lock-light (`ArcSwap` / atomics / a small `Mutex`).
//! - [`HidState`] lives only on the dedicated HID thread. It owns the keystroke
//!   buffer and caps-lock latch with single-threaded interior mutability
//!   (`RefCell`/`Cell`), exactly like the standalone reference.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::writer::{self, Record};

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

/// Cross-thread state. Written by the main thread, read by the HID thread.
pub struct Shared {
    app_name: ArcSwap<AppName>,
    hotkey: Mutex<Vec<u32>>,
    capture_on_hotkey: AtomicBool,
}

impl Shared {
    pub fn new(hotkey: Vec<u32>, capture_on_hotkey: bool) -> Arc<Self> {
        Arc::new(Self {
            app_name: ArcSwap::from_pointee(AppName::default()),
            hotkey: Mutex::new(hotkey),
            capture_on_hotkey: AtomicBool::new(capture_on_hotkey),
        })
    }

    pub fn current_app(&self) -> Arc<AppName> {
        self.app_name.load_full()
    }

    pub fn set_app(&self, name: AppName) {
        self.app_name.store(Arc::new(name));
    }

    pub fn set_hotkey(&self, codes: Vec<u32>) {
        *self.hotkey.lock().unwrap() = codes;
    }

    pub fn hotkey(&self) -> Vec<u32> {
        self.hotkey.lock().unwrap().clone()
    }

    pub fn set_capture_on_hotkey(&self, on: bool) {
        self.capture_on_hotkey.store(on, Ordering::SeqCst);
    }

    pub fn capture_on_hotkey(&self) -> bool {
        self.capture_on_hotkey.load(Ordering::SeqCst)
    }
}

/// An open keystroke buffer for one app, tagged with its start instant.
struct Buffer {
    app: AppName,
    text: String,
    started_ms: i64,
}

/// HID-thread-only state. Reconstructed from the C callback `context` pointer;
/// only ever touched on the HID thread, so `RefCell`/`Cell` need no locking.
pub struct HidState {
    data_dir: PathBuf,
    shared: Arc<Shared>,
    buffer: RefCell<Option<Buffer>>,
    capslock: Cell<bool>,
    /// Currently-held scancodes, for hotkey-combo detection.
    pressed: RefCell<Vec<u32>>,
    /// Edge latch so a satisfied combo fires once until released.
    hotkey_armed: Cell<bool>,
    /// Invokes the JS hotkey callback (a napi ThreadsafeFunction under the hood).
    notify_hotkey: Box<dyn Fn() + Send>,
}

impl HidState {
    pub fn new(
        data_dir: PathBuf,
        shared: Arc<Shared>,
        notify_hotkey: Box<dyn Fn() + Send>,
    ) -> Self {
        Self {
            data_dir,
            shared,
            buffer: RefCell::new(None),
            capslock: Cell::new(false),
            pressed: RefCell::new(Vec::new()),
            hotkey_armed: Cell::new(false),
            notify_hotkey,
        }
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    pub fn capslock(&self) -> bool {
        self.capslock.get()
    }

    pub fn toggle_capslock(&self) {
        self.capslock.set(!self.capslock.get());
    }

    /// Ensure the open buffer targets the current app and time window, flushing
    /// and rotating when the app changed or the window elapsed.
    pub fn rotate_if_needed(&self, now_ms: i64) {
        let current = self.shared.current_app();
        let rotate = {
            let buf = self.buffer.borrow();
            match buf.as_ref() {
                None => true,
                Some(b) => {
                    b.app.sanitized != current.sanitized
                        || now_ms - b.started_ms >= FLUSH_INTERVAL_MS
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
                started_ms: now_ms,
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
            ts: buffer.started_ms,
            app: buffer.app.sanitized,
            app_raw: buffer.app.raw,
            text: buffer.text,
        };
        if let Err(err) = writer::append_record(&self.data_dir, &record) {
            eprintln!("[pi0] failed to append record: {err:#}");
        }
    }

    // ---- hotkey combo detection -------------------------------------------

    /// Update the held-key set and fire the hotkey callback on the rising edge
    /// of a fully-satisfied combo (when capture-on-hotkey is enabled).
    pub fn update_hotkey(&self, scancode: u32, down: bool) {
        {
            let mut pressed = self.pressed.borrow_mut();
            if down {
                if !pressed.contains(&scancode) {
                    pressed.push(scancode);
                }
            } else {
                pressed.retain(|&c| c != scancode);
            }
        }

        let combo = self.shared.hotkey();
        if combo.is_empty() {
            return;
        }
        let satisfied = {
            let pressed = self.pressed.borrow();
            combo.iter().all(|c| pressed.contains(c))
        };

        if satisfied && !self.hotkey_armed.get() {
            self.hotkey_armed.set(true);
            if self.shared.capture_on_hotkey() {
                (self.notify_hotkey)();
            }
        } else if !satisfied && self.hotkey_armed.get() {
            self.hotkey_armed.set(false);
        }
    }
}

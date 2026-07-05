//! The capture engine: runs the IOKit HID manager on a dedicated thread with
//! its **own** `CFRunLoop`, mirroring the proven libuiohook/uiohook-napi model
//! (dedicated thread → own run loop → `CFRunLoopRun`, with a start/stop
//! handshake). This keeps the blocking run loop off Node's main thread.

use std::ffi::{c_void, CStr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Result};
use objc2_core_foundation::{
    kCFRunLoopCommonModes, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks, CFArray, CFDictionary, CFNumber, CFRetained, CFRunLoop,
    CFString,
};
use objc2_io_kit::{
    kHIDPage_GenericDesktop, kHIDUsage_GD_Keyboard, kHIDUsage_GD_Keypad, kHIDUsage_GD_Mouse,
    kHIDUsage_GD_Pointer, kIOHIDDeviceUsageKey, kIOHIDDeviceUsagePageKey, kIOHIDOptionsTypeNone,
    IOHIDManager,
};

use crate::callbacks;
use crate::state::{HidState, Shared};

/// `kIOReturnSuccess`.
const IO_RETURN_SUCCESS: i32 = 0;

/// A `CFRunLoop` reference that is safe to move to another thread purely to call
/// `CFRunLoopStop` on it (that call is thread-safe).
struct RunLoopHandle(CFRetained<CFRunLoop>);
// SAFETY: only used to invoke CFRunLoopStop, which Apple documents as callable
// from any thread.
unsafe impl Send for RunLoopHandle {}

/// A live capture engine: the HID thread plus the handles needed to stop it and
/// to service `capture_snapshot` (frontmost-app name + data dir).
pub struct EngineHandle {
    join: Option<JoinHandle<()>>,
    run_loop: RunLoopHandle,
    running: Arc<AtomicBool>,
    pub shared: Arc<Shared>,
    pub data_dir: PathBuf,
}

impl EngineHandle {
    /// Stop the HID thread: signal it, stop its run loop, and join.
    pub fn stop(mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.run_loop.0.stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Spawn the HID thread and block briefly until it reports whether the HID
/// manager opened (i.e. whether Input Monitoring is granted). Returns an error
/// with a TCC hint if it did not.
pub fn spawn(data_dir: PathBuf, shared: Arc<Shared>) -> Result<EngineHandle> {
    let running = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::channel::<std::result::Result<RunLoopHandle, String>>();

    let shared_thread = Arc::clone(&shared);
    let running_thread = Arc::clone(&running);

    let join = std::thread::Builder::new()
        .name("pi0-hid".to_string())
        .spawn(move || hid_thread_main(shared_thread, running_thread, tx))?;

    match rx.recv() {
        Ok(Ok(run_loop)) => Ok(EngineHandle {
            join: Some(join),
            run_loop,
            running,
            shared,
            data_dir,
        }),
        Ok(Err(message)) => {
            let _ = join.join();
            Err(anyhow!(message))
        }
        Err(_) => {
            let _ = join.join();
            Err(anyhow!("HID thread exited before signaling readiness"))
        }
    }
}

/// Body of the dedicated HID thread: build the manager, schedule it on this
/// thread's run loop, open it, signal readiness, then run the loop until
/// `CFRunLoopStop`.
fn hid_thread_main(
    shared: Arc<Shared>,
    running: Arc<AtomicBool>,
    tx: mpsc::Sender<std::result::Result<RunLoopHandle, String>>,
) {
    // Boxed so its address is stable for the C callback `context` pointer, and
    // kept alive on this stack for the whole run-loop lifetime.
    let state = Box::new(HidState::new(shared));

    let manager = IOHIDManager::new(None, kIOHIDOptionsTypeNone);

    // Match Generic-Desktop keyboards/keypads (keystrokes) plus mice/pointers
    // (movement deltas → activity signal for the adaptive capture interval).
    let matches = [
        matching_dict(kHIDPage_GenericDesktop, kHIDUsage_GD_Keyboard),
        matching_dict(kHIDPage_GenericDesktop, kHIDUsage_GD_Keypad),
        matching_dict(kHIDPage_GenericDesktop, kHIDUsage_GD_Mouse),
        matching_dict(kHIDPage_GenericDesktop, kHIDUsage_GD_Pointer),
    ];
    let mut match_ptrs: [*const c_void; 4] = [
        (&*matches[0] as *const CFDictionary).cast(),
        (&*matches[1] as *const CFDictionary).cast(),
        (&*matches[2] as *const CFDictionary).cast(),
        (&*matches[3] as *const CFDictionary).cast(),
    ];
    let match_array = unsafe {
        CFArray::new(
            None,
            match_ptrs.as_mut_ptr(),
            match_ptrs.len() as isize,
            &kCFTypeArrayCallBacks,
        )
        .expect("failed to create matching CFArray")
    };
    unsafe { manager.set_device_matching_multiple(Some(&match_array)) };

    let context = (&*state as *const HidState) as *mut c_void;
    unsafe {
        manager.register_input_value_callback(Some(callbacks::input_value), context);
    }

    let run_loop = CFRunLoop::current().expect("no current run loop on HID thread");
    let mode = unsafe { kCFRunLoopCommonModes }.expect("no common run loop mode");
    unsafe { manager.schedule_with_run_loop(&run_loop, mode) };

    let ret = manager.open(kIOHIDOptionsTypeNone);
    if ret != IO_RETURN_SUCCESS {
        let _ = tx.send(Err(format!(
            "IOHIDManagerOpen failed (0x{ret:08x}); grant Input Monitoring \
             (System Settings → Privacy & Security → Input Monitoring) and retry"
        )));
        return;
    }

    // Signal readiness with a stop handle before blocking on the run loop.
    if tx.send(Ok(RunLoopHandle(run_loop.clone()))).is_err() {
        return; // start() gave up; don't run.
    }
    debug_log("HID thread: manager open, entering run loop");

    // Blocks until EngineHandle::stop calls CFRunLoopStop from the main thread.
    // The scheduled HID manager source keeps the loop alive.
    CFRunLoop::run();

    // If the loop returned while we were still meant to be running, the HID
    // source did not keep it alive — surface that (see step-4 spike).
    if running.load(Ordering::SeqCst) {
        debug_log("HID thread: WARNING run loop exited while still running");
    } else {
        debug_log("HID thread: run loop stopped cleanly");
    }
    running.store(false, Ordering::SeqCst);

    // Clean up: flush any buffered keystrokes, unschedule, close.
    state.flush();
    unsafe { manager.unschedule_from_run_loop(&run_loop, mode) };
    let _ = manager.close(kIOHIDOptionsTypeNone);
}

/// Lightweight opt-in tracing for the run-loop spike (enable with `PI0_DEBUG`).
pub(crate) fn debug_enabled() -> bool {
    std::env::var_os("PI0_DEBUG").is_some()
}

fn debug_log(msg: &str) {
    if debug_enabled() {
        eprintln!("[pi0] {msg}");
    }
}

/// Build a `{ DeviceUsagePage: page, DeviceUsage: usage }` matching dictionary.
fn matching_dict(usage_page: u32, usage: u32) -> CFRetained<CFDictionary> {
    let page_key = cfstring(kIOHIDDeviceUsagePageKey);
    let usage_key = cfstring(kIOHIDDeviceUsageKey);
    let page_val = CFNumber::new_i32(usage_page as i32);
    let usage_val = CFNumber::new_i32(usage as i32);

    let mut keys: [*const c_void; 2] = [
        (&*page_key as *const CFString).cast(),
        (&*usage_key as *const CFString).cast(),
    ];
    let mut values: [*const c_void; 2] = [
        (&*page_val as *const CFNumber).cast(),
        (&*usage_val as *const CFNumber).cast(),
    ];

    unsafe {
        CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
        .expect("failed to create matching dictionary")
    }
}

/// Convert an IOKit `&CStr` key constant into a `CFString`.
fn cfstring(key: &CStr) -> CFRetained<CFString> {
    CFString::from_str(
        key.to_str()
            .expect("IOKit key constant was not valid UTF-8"),
    )
}

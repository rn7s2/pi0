//! The IOKit HID `input_value` callback — runs on the dedicated HID thread's
//! run loop. Its signature must match objc2-io-kit's `IOHIDValueCallback`
//! exactly, and because it is invoked from C it must never unwind: the body is
//! wrapped in [`std::panic::catch_unwind`].

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;

use objc2_io_kit::{IOHIDValue, IOReturn};

use crate::keymap::{self, KeyKind};
use crate::paths;
use crate::state::HidState;

/// HID element usage page for keyboard/keypad keys.
const KEYBOARD_USAGE_PAGE: u32 = 0x07;

/// Recover the `HidState` from the C `context` pointer.
///
/// # Safety
/// `context` must be the `&HidState` pointer registered with the HID manager,
/// which the HID thread keeps alive for the lifetime of its run loop.
unsafe fn state_from<'a>(context: *mut c_void) -> &'a HidState {
    debug_assert!(!context.is_null(), "HID callback context was null");
    unsafe { &*(context as *const HidState) }
}

/// `IOHIDValueCallback`: fired for every keyboard input value change.
pub unsafe extern "C-unwind" fn input_value(
    context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
    value: NonNull<IOHIDValue>,
) {
    let state = unsafe { state_from(context) };
    let value = unsafe { value.as_ref() };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        handle_input_value(state, value);
    }));
}

fn handle_input_value(state: &HidState, value: &IOHIDValue) {
    let element = value.element();
    if element.usage_page() != KEYBOARD_USAGE_PAGE {
        return;
    }
    let scancode = element.usage();
    if !(4..=231).contains(&scancode) {
        return;
    }
    let pressed = value.integer_value();
    let down = pressed == 1;

    if crate::engine::debug_enabled() {
        eprintln!("[pi0] key scancode={scancode} down={down}");
    }

    let Some(entry) = keymap::lookup(scancode) else {
        return;
    };

    if down {
        // Rotate the buffer to the current app / time window before appending.
        state.rotate_if_needed(paths::now_ms());
        match entry.kind {
            KeyKind::CapsLock => state.toggle_capslock(),
            KeyKind::Modifier => state.append_text(&format!("{}(", entry.base)),
            KeyKind::Printable => {
                let text = if state.capslock() {
                    entry.shifted
                } else {
                    entry.base
                };
                state.append_text(text);
            }
        }
    } else if entry.kind == KeyKind::Modifier {
        // Modifier released — close the wrapper opened on press.
        state.append_text(")");
    }
}

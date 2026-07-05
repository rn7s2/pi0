//! The IOKit HID `input_value` callback — runs on the dedicated HID thread's
//! run loop. Its signature must match objc2-io-kit's `IOHIDValueCallback`
//! exactly, and because it is invoked from C it must never unwind: the body is
//! wrapped in [`std::panic::catch_unwind`].

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;

use objc2_io_kit::{IOHIDValue, IOReturn};

use crate::clock;
use crate::keymap::{self, KeyKind};
use crate::state::HidState;

/// HID element usage page for keyboard/keypad keys.
const KEYBOARD_USAGE_PAGE: u32 = 0x07;
/// HID Generic Desktop usage page — carries mouse/pointer motion (X/Y).
const GENERIC_DESKTOP_PAGE: u32 = 0x01;
/// Generic Desktop usages for pointer motion deltas.
const USAGE_X: u32 = 0x30;
const USAGE_Y: u32 = 0x31;

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
    let usage_page = element.usage_page();

    // Mouse/pointer movement: a non-zero relative delta on the Generic Desktop
    // X or Y axis. Counts as activity (keeps capture in the active cadence) but
    // is never recorded — only the last-activity clock is bumped.
    if usage_page == GENERIC_DESKTOP_PAGE {
        let usage = element.usage();
        if (usage == USAGE_X || usage == USAGE_Y) && value.integer_value() != 0 {
            state.mark_activity(clock::now_ms());
        }
        return;
    }

    if usage_page != KEYBOARD_USAGE_PAGE {
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
        // A keystroke is activity; bump the clock, then rotate the buffer to the
        // current app / time window before appending.
        let now = clock::now_ms();
        state.mark_activity(now);
        state.rotate_if_needed(now);
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

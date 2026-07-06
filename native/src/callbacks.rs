//! The IOKit HID `input_value` callback — runs on the dedicated HID thread's
//! run loop. Its signature must match objc2-io-kit's `IOHIDValueCallback`
//! exactly, and because it is invoked from C it must never unwind: the body is
//! wrapped in [`std::panic::catch_unwind`].

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;

use objc2_io_kit::{
    kHIDPage_Button, kHIDPage_GenericDesktop, kHIDPage_KeyboardOrKeypad, kHIDUsage_GD_Wheel,
    kHIDUsage_GD_X, kHIDUsage_GD_Y, IOHIDValue, IOReturn,
};

use crate::clock;
use crate::keymap::{self, KeyKind};
use crate::state::HidState;

// HID usage pages / usages, aliased from Apple's IOKit usage-table constants
// (objc2_io_kit re-exports of <IOKit/hid/IOHIDUsageTables.h>, which mirror the
// USB-IF "HID Usage Tables" spec) so the values are traceable, not bare hex.

/// HID element usage page for keyboard/keypad keys.
const KEYBOARD_USAGE_PAGE: u32 = kHIDPage_KeyboardOrKeypad;
/// HID Generic Desktop usage page — carries mouse/pointer motion and the wheel.
const GENERIC_DESKTOP_PAGE: u32 = kHIDPage_GenericDesktop;
/// HID Button usage page — mouse/trackpad button presses.
const BUTTON_USAGE_PAGE: u32 = kHIDPage_Button;
/// Generic Desktop relative-delta usages: pointer X, pointer Y, scroll wheel.
const USAGE_X: u32 = kHIDUsage_GD_X;
const USAGE_Y: u32 = kHIDUsage_GD_Y;
const USAGE_WHEEL: u32 = kHIDUsage_GD_Wheel;

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

    // Pointer motion (mouse or trackpad cursor) and scroll-wheel spins both live
    // on the Generic Desktop page as non-zero relative deltas. Either one counts
    // as activity (keeps capture in the active cadence) but is never recorded —
    // only the last-activity clock is bumped. Zoom/rotate gestures are delivered
    // elsewhere (not Generic Desktop) and are intentionally not tracked here.
    if usage_page == GENERIC_DESKTOP_PAGE {
        let usage = element.usage();
        let is_motion = usage == USAGE_X || usage == USAGE_Y || usage == USAGE_WHEEL;
        if is_motion && value.integer_value() != 0 {
            state.mark_activity(clock::now_ms());
        }
        return;
    }

    // Mouse/trackpad button presses: a click with no cursor motion would
    // otherwise read as idle. Bump the clock on press (non-zero value) only.
    if usage_page == BUTTON_USAGE_PAGE {
        if value.integer_value() != 0 {
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

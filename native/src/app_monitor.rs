//! Active-application tracking via `NSWorkspace` notifications.
//!
//! Runs on the **main thread** (AppKit delivers workspace notifications there,
//! and Electron drives that run loop). On each activation it publishes the
//! frontmost app name into the shared cell the HID thread reads — no direct
//! cross-thread access to HID state.

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::NSNotification;

use crate::paths;
use crate::state::{AppName, Shared};

/// Register an observer for application-activation notifications. The returned
/// token must be kept alive for observation to continue and passed to
/// `removeObserver:` (see `lib::stop`) to unregister.
pub fn install(shared: Arc<Shared>) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();

    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        let notification = unsafe { notification.as_ref() };
        handle_activation(&shared, notification);
    });

    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            None,
            &block,
        )
    }
}

/// Remove a previously-installed activation observer.
pub fn remove(token: &ProtocolObject<dyn NSObjectProtocol>) {
    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    // `removeObserver:` takes an untyped `id`; the token is repr-transparent over
    // the object, so reborrow it as `&AnyObject`.
    let obj: &AnyObject =
        unsafe { &*(token as *const ProtocolObject<dyn NSObjectProtocol> as *const AnyObject) };
    unsafe { center.removeObserver(obj) };
}

fn handle_activation(shared: &Shared, notification: &NSNotification) {
    let Some(info) = notification.userInfo() else {
        return;
    };
    let key = unsafe { NSWorkspaceApplicationKey };
    let Some(app) = info.objectForKey(key) else {
        return;
    };
    let Ok(app) = app.downcast::<NSRunningApplication>() else {
        return;
    };
    let Some(name) = app.localizedName() else {
        return;
    };
    let raw = name.to_string();
    let sanitized = paths::sanitize_app_name(&raw);
    shared.set_app(AppName { raw, sanitized });
}

#![allow(deprecated)]

use tauri::{App, Manager};
use tauri_nspanel::{
    cocoa::appkit::NSWindowCollectionBehavior, panel_delegate, WebviewWindowExt as PanelExt,
};

const NS_FLOAT_WINDOW_LEVEL: i32 = 4;
const NS_WINDOW_STYLE_MASK_NON_ACTIVATING_PANEL: i32 = 1 << 7;

pub fn setup_nspanel(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    app.handle().plugin(tauri_nspanel::init())?;

    let window = app
        .get_webview_window("main")
        .expect("main window not found");
    let panel = window.to_panel()?;

    panel.set_level(NS_FLOAT_WINDOW_LEVEL);
    panel.set_style_mask(NS_WINDOW_STYLE_MASK_NON_ACTIVATING_PANEL);
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces,
    );

    let delegate = panel_delegate!(BlueyPanelDelegate {
        window_did_become_key,
        window_did_resign_key
    });

    delegate.set_listener(Box::new(|event: String| {
        tracing::debug!(event = %event, "panel delegate event");
    }));

    panel.set_delegate(delegate);

    Ok(())
}

/// Codex follow-up: exclude every macOS NSWindow this Tauri app owns
/// from screen-share / screen-recording capture. Walks all webview
/// windows on the AppHandle and sets NSWindow.sharingType = .none.
///
/// macOS API: NSWindow.SharingType.none == 0 == NSWindowSharingNone.
/// The window is excluded from CGDisplayStream / ScreenCaptureKit /
/// screencapture / AirPlay, identical to what the floating overlay
/// already does. Without this, opening the dashboard window during
/// a screen share leaks Bluey to the meeting.
pub fn set_sharing_type_none<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSInteger;

    for (_label, window) in app.webview_windows() {
        let Ok(ns_window_ptr) = window.ns_window() else {
            continue;
        };
        // ns_window() returns *mut std::ffi::c_void pointing at NSWindow.
        let ns_window = ns_window_ptr as *mut AnyObject;
        if ns_window.is_null() {
            continue;
        }
        // SAFETY: Tauri guarantees the pointer is a valid retained
        // NSWindow on macOS. NSWindow.sharingType is a Cocoa property
        // backed by setSharingType:; passing 0 (NSWindowSharingNone)
        // is safe.
        unsafe {
            let _: () = msg_send![ns_window, setSharingType: 0 as NSInteger];
        }
    }
    tracing::info!("dashboard windows set to NSWindowSharingNone (screen-share invisible)");
}

//! Bluey meeting overlay — Tauri shell.
//!
//! The UI is the React "Aurora Glass" app in `ui/`, rendered in a transparent,
//! floating, screen-share-INVISIBLE window — the live meeting surface, distinct
//! from the interview overlay. Tauri does the invisibility (contentProtected +
//! NSWindow.sharingType=.none) and floating-panel behaviour (tauri-nspanel), the
//! same verified mechanism the interview overlay uses.
//!
//! The shell also bridges the UI's MeetingClient to the daemon: the `agent_*`
//! commands speak the daemon's existing agent IPC (the contract the dashboard
//! already uses), and `meeting_ask` streams the answer back to the UI over Tauri
//! events. The daemon socket wiring lives in [`ipc`].
//!
//! Cross-platform: macOS now (sharingType=.none); Windows later via
//! SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE).

use tauri::Manager;

mod commands;
mod ipc;

#[cfg(target_os = "macos")]
mod macos {
    use tauri::Manager;
    use tauri_nspanel::{cocoa::appkit::NSWindowCollectionBehavior, WebviewWindowExt as PanelExt};

    const NS_FLOAT_WINDOW_LEVEL: i32 = 4;
    const NS_WINDOW_STYLE_MASK_BORDERLESS: i32 = 0;
    const NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL: i32 = 1 << 7;
    const NS_OVERLAY_STYLE_MASK: i32 =
        NS_WINDOW_STYLE_MASK_BORDERLESS | NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL;

    /// Float the meeting panel over everything as a non-activating panel (never
    /// steals focus from the meeting app), joining all spaces.
    pub fn setup_panel(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
        app.handle().plugin(tauri_nspanel::init())?;
        let window = app.get_webview_window("meeting").expect("meeting window");
        let panel = window.to_panel()?;
        panel.set_level(NS_FLOAT_WINDOW_LEVEL);
        panel.set_style_mask(NS_OVERLAY_STYLE_MASK);
        panel.set_collection_behaviour(
            NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces,
        );
        Ok(())
    }

    /// Set NSWindow.sharingType = .none (0) on every window — screen-share
    /// invisibility (the verified meeting-tool requirement).
    pub fn set_sharing_none(app: &tauri::AppHandle) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSUInteger;
        for (_label, window) in app.webview_windows() {
            if let Ok(ptr) = window.ns_window() {
                let ns = ptr as *mut AnyObject;
                if !ns.is_null() {
                    unsafe {
                        let _: () = msg_send![ns, setSharingType: 0u64 as NSUInteger];
                    }
                }
            }
        }
    }

    /// Make the NSWindow fully transparent (no frame/border/shadow) so only the
    /// rounded Aurora-Glass content shows — never a window rectangle.
    pub fn clear_window_chrome(app: &tauri::AppHandle) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        for (_label, window) in app.webview_windows() {
            if let Ok(ptr) = window.ns_window() {
                let ns = ptr as *mut AnyObject;
                if ns.is_null() {
                    continue;
                }
                unsafe {
                    let nscolor = objc2::runtime::AnyClass::get("NSColor").unwrap();
                    let clear: *mut AnyObject = msg_send![nscolor, clearColor];
                    let _: () = msg_send![ns, setBackgroundColor: clear];
                    let _: () = msg_send![ns, setOpaque: false];
                    let _: () = msg_send![ns, setHasShadow: false];
                    let _: () = msg_send![ns, invalidateShadow];

                    let content_view: *mut AnyObject = msg_send![ns, contentView];
                    if !content_view.is_null() {
                        let _: () = msg_send![content_view, setWantsLayer: true];
                        let layer: *mut AnyObject = msg_send![content_view, layer];
                        if !layer.is_null() {
                            let _: () = msg_send![layer, setOpaque: false];
                        }
                        clear_webview_background(content_view);
                    }
                }
            }
        }
    }

    /// Recursively disable any WKWebView's opaque backing so only the HTML paints
    /// (the documented transparent-macOS-webview fix). Safe no-op for other views.
    unsafe fn clear_webview_background(view: *mut objc2::runtime::AnyObject) {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        if view.is_null() {
            return;
        }
        if let Some(wk_class) = AnyClass::get("WKWebView") {
            let is_wk: bool = msg_send![view, isKindOfClass: wk_class];
            if is_wk {
                let _: () = msg_send![view, setOpaque: false];
                if let (Some(num_cls), Some(str_cls)) =
                    (AnyClass::get("NSNumber"), AnyClass::get("NSString"))
                {
                    let no: *mut AnyObject = msg_send![num_cls, numberWithBool: false];
                    let key: *mut AnyObject =
                        msg_send![str_cls, stringWithUTF8String: c"drawsBackground".as_ptr()];
                    if !no.is_null() && !key.is_null() {
                        let _: () = msg_send![view, setValue: no, forKey: key];
                    }
                }
                let _: () = msg_send![view, setWantsLayer: true];
                let layer: *mut AnyObject = msg_send![view, layer];
                if !layer.is_null() {
                    let _: () = msg_send![layer, setOpaque: false];
                }
                if let Some(nscolor) = AnyClass::get("NSColor") {
                    let clear: *mut AnyObject = msg_send![nscolor, clearColor];
                    let responds: bool = msg_send![
                        view,
                        respondsToSelector: objc2::sel!(setUnderPageBackgroundColor:)
                    ];
                    if responds {
                        let _: () = msg_send![view, setUnderPageBackgroundColor: clear];
                    }
                }
            }
        }
        let subviews: *mut AnyObject = msg_send![view, subviews];
        if subviews.is_null() {
            return;
        }
        let count: usize = msg_send![subviews, count];
        for i in 0..count {
            let sub: *mut AnyObject = msg_send![subviews, objectAtIndex: i];
            clear_webview_background(sub);
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(ipc::DaemonLink::default())
        .invoke_handler(tauri::generate_handler![
            commands::agent_list,
            commands::agent_attach,
            commands::agent_detach,
            commands::agent_sessions,
            commands::agent_connectors,
            commands::set_agent_session_history,
            commands::meeting_ask,
            commands::meeting_ask_cancel,
        ])
        .setup(|app| {
            // Accessory app — no Dock icon (an invisible meeting overlay must not
            // appear in the Dock).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let win = app.get_webview_window("meeting").expect("meeting window");
            // Capture exclusion — invisible to screen recording/sharing.
            let _ = win.set_content_protected(true);
            #[cfg(target_os = "macos")]
            {
                macos::setup_panel(app)?;
                macos::set_sharing_none(app.handle());
                macos::clear_window_chrome(app.handle());
            }

            // Connect to the daemon IPC socket (when launched by the daemon) and
            // forward its transcript/answer events to the UI.
            ipc::start(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running meeting overlay");
}

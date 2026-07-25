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
// `tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior` is deprecated in
// favour of the objc2-app-kit crate, but tauri-nspanel's panel API still takes
// the cocoa enum. Migrating would mean adding objc2-app-kit and converting types
// across the nspanel boundary; the cocoa binding is still functional and the
// behaviour is verified (screen-share invisibility). Scope the allow to this
// module so the deprecation stays surfaced everywhere else.
#[allow(deprecated)]
mod macos {
    use tauri::Manager;

    pub fn setup_panel(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
        app.handle().plugin(tauri_nspanel::init())?;
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
        .plugin(tauri_plugin_dialog::init())
        .manage(ipc::DaemonLink::default())
        .manage(ipc::EventSender(tokio::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::agent_list,
            commands::agent_attach,
            commands::agent_detach,
            commands::agent_sessions,
            commands::agent_models,
            commands::agent_connectors,
            commands::source_coverage,
            commands::set_agent_session_history,
            commands::calendar_connect,
            commands::calendar_status,
            commands::calendar_disconnect,
            commands::meeting_ask,
            commands::meeting_ask_cancel,
            commands::pick_context_files,
            commands::capture_screenshot,
            commands::hide_banner,
            ipc::overlay_send,
        ])
        .setup(|app| {
            // LOCAL-TEST ESCAPE HATCH ONLY. When BLUEY_MEETING_CAPTURE_VISIBLE=1
            // the overlay is left VISIBLE to screen capture (and shown in the
            // Dock) so a developer can screenshot the UI. This flag must NEVER
            // ship / be set in production — the whole product promise is that the
            // overlay is invisible to Zoom/Teams/screen-share. Default (unset) is
            // the real, invisible behavior.
            let capture_visible = std::env::var("BLUEY_MEETING_CAPTURE_VISIBLE")
                .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
                .unwrap_or(false);

            // Accessory app — no Dock icon (an invisible meeting overlay must not
            // appear in the Dock). In capture-visible test mode, show it as a
            // regular app so it's easy to grab/screenshot.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(if capture_visible {
                tauri::ActivationPolicy::Regular
            } else {
                tauri::ActivationPolicy::Accessory
            });

            let win = app.get_webview_window("meeting").expect("meeting window");
            // The overlay is ALWAYS the same frameless, transparent, floating
            // panel — NO titlebar, NO window chrome, NO box (just like the
            // interview overlay). The ONLY difference in capture-visible test mode
            // is that screen-share invisibility is left OFF so a screenshot can
            // see it. Everything else (NSPanel, transparency, no decorations) is
            // identical, so what you test looks exactly like what ships.
            #[cfg(target_os = "macos")]
            {
                macos::setup_panel(app)?;
                macos::clear_window_chrome(app.handle());
            }
            if !capture_visible {
                // Real mode: invisible to screen recording/sharing.
                let _ = win.set_content_protected(true);
                #[cfg(target_os = "macos")]
                macos::set_sharing_none(app.handle());
            } else {
                eprintln!(
                    "[meeting-overlay] BLUEY_MEETING_CAPTURE_VISIBLE=1 — overlay is \
                     VISIBLE to screen capture (LOCAL TEST ONLY, never ship)"
                );
                #[cfg(debug_assertions)]
                win.open_devtools();
            }

            // Connect to the daemon's Unix socket (when launched by the daemon)
            // and forward its OverlayCommand stream to the UI as `overlay://command`,
            // while pushing UI OverlayEvents back over the same socket. Standalone
            // dev (no socket arg) → no-op; the UI's mock fills in.
            ipc::start(app.handle(), ipc::IpcArgs::from_process());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running meeting overlay");
}

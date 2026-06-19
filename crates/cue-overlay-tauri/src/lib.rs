//! Bluey overlay — Tauri shell. The UI is HTML/CSS (ui/), rendered in a
//! transparent, floating, screen-share-INVISIBLE window. This replaces the
//! native Swift overlay: Tauri does the invisibility (contentProtected +
//! NSWindow.sharingType=.none) and floating-panel behaviour (tauri-nspanel),
//! the same mechanism the dashboard crate already uses and which is verified
//! invisible to QuickTime/ScreenCaptureKit.
//!
//! Cross-platform: macOS now (sharingType=.none); Windows later via
//! SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE).

use tauri::Manager;

mod ipc;

#[cfg(target_os = "macos")]
mod macos {
    use tauri::Manager;
    use tauri_nspanel::{cocoa::appkit::NSWindowCollectionBehavior, WebviewWindowExt as PanelExt};

    const NS_FLOAT_WINDOW_LEVEL: i32 = 4;
    // Borderless (0) | NonactivatingPanel (1<<7). A transparent NSWindow must be
    // borderless — a titled/bordered style mask makes macOS force an opaque
    // backing (the solid rectangle). Borderless is 0 so this equals 1<<7, but we
    // name it explicitly to document intent and guard against a future change.
    const NS_WINDOW_STYLE_MASK_BORDERLESS: i32 = 0;
    const NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL: i32 = 1 << 7;
    const NS_OVERLAY_STYLE_MASK: i32 =
        NS_WINDOW_STYLE_MASK_BORDERLESS | NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL;

    /// Float the overlay over everything as a non-activating panel (doesn't
    /// steal focus from the meeting app), joins all spaces.
    pub fn setup_panel(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
        app.handle().plugin(tauri_nspanel::init())?;
        let window = app.get_webview_window("overlay").expect("overlay window");
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
    /// invisibility (verified working). setSharingType: takes NSUInteger.
    pub fn set_sharing_none(app: &tauri::AppHandle) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSUInteger;
        for (_label, window) in app.webview_windows() {
            if let Ok(ptr) = window.ns_window() {
                let ns = ptr as *mut AnyObject;
                if !ns.is_null() {
                    unsafe { let _: () = msg_send![ns, setSharingType: 0u64 as NSUInteger]; }
                }
            }
        }
    }

    /// Make the NSWindow fully transparent with NO frame/border/shadow — so only
    /// the (rounded) web content shows, never a window rectangle around it.
    pub fn clear_window_chrome(app: &tauri::AppHandle) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        for (_label, window) in app.webview_windows() {
            if let Ok(ptr) = window.ns_window() {
                let ns = ptr as *mut AnyObject;
                if ns.is_null() { continue; }
                unsafe {
                    let nscolor = objc2::runtime::AnyClass::get("NSColor").unwrap();

                    // Window-level transparency: an explicit fully-transparent
                    // (alpha 0) background, non-opaque, no shadow. Done AFTER the
                    // NSPanel conversion (see run()), which can otherwise re-set
                    // these. NOTE: a faint 1px frame edge can still remain at the
                    // window's bottom on macOS — an upstream tauri/macOS quirk for
                    // borderless transparent windows (tauri#14394) that is not
                    // removable from app code; the window is sized with a
                    // transparent margin (OVERLAY_FRAME_MARGIN) so it stays clear
                    // of the visible pill/panel.
                    let clear: *mut AnyObject = msg_send![nscolor, clearColor];
                    let zero_alpha: *mut AnyObject = msg_send![
                        nscolor, colorWithSRGBRed: 0.0f64, green: 0.0f64, blue: 0.0f64, alpha: 0.0f64];
                    let bg = if zero_alpha.is_null() { clear } else { zero_alpha };
                    let _: () = msg_send![ns, setBackgroundColor: bg];
                    let _: () = msg_send![ns, setOpaque: false];
                    let _: () = msg_send![ns, setHasShadow: false];
                    let _: () = msg_send![ns, invalidateShadow];

                    // The content view's backing layer is opaque by default, so the
                    // WKWebView would draw an opaque rectangle behind the web
                    // content. Make it layer-backed and non-opaque.
                    let content_view: *mut AnyObject = msg_send![ns, contentView];
                    if !content_view.is_null() {
                        let _: () = msg_send![content_view, setWantsLayer: true];
                        let layer: *mut AnyObject = msg_send![content_view, layer];
                        if !layer.is_null() {
                            let _: () = msg_send![layer, setOpaque: false];
                        }
                        // 3) THE WKWebView itself. Even with the window + content
                        //    view transparent, WKWebView draws its OWN opaque
                        //    background by default — that is the solid rectangle.
                        //    Recursively find the WKWebView under the content view
                        //    and turn off its drawsBackground + opaque so only the
                        //    HTML pixels paint. This is the documented fix for an
                        //    opaque rectangle on a transparent macOS webview window.
                        clear_webview_background(content_view);
                    }
                }
            }
        }
    }

    /// Recursively walk a view tree and, for any `WKWebView`, disable its opaque
    /// background (`setOpaque:NO` + `setValue:NO forKey:"drawsBackground"`), and
    /// make its layer non-opaque. Safe no-op for non-webview views.
    unsafe fn clear_webview_background(view: *mut objc2::runtime::AnyObject) {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        if view.is_null() {
            return;
        }
        // Is this view a WKWebView?
        if let Some(wk_class) = AnyClass::get("WKWebView") {
            let is_wk: bool = msg_send![view, isKindOfClass: wk_class];
            if is_wk {
                let _: () = msg_send![view, setOpaque: false];
                // WKWebView honours the KVC key "drawsBackground" to stop painting
                // its white backing. Build the NSNumber(false) + NSString key via
                // the runtime (no objc2-foundation type imports needed).
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
                // macOS 12+: WKWebView paints `underPageBackgroundColor` BEHIND
                // the page — it defaults to a system color and shows as a FAINT
                // rectangle even when drawsBackground is off. Set it to clear.
                // (NSColor arg, not CGColor — the safe msg_send path.)
                if let Some(nscolor) = AnyClass::get("NSColor") {
                    let clear: *mut AnyObject = msg_send![nscolor, clearColor];
                    let responds: bool =
                        msg_send![view, respondsToSelector: objc2::sel!(setUnderPageBackgroundColor:)];
                    if responds {
                        let _: () = msg_send![view, setUnderPageBackgroundColor: clear];
                    }
                }
            }
        }
        // Recurse into subviews.
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

/// Resize + reposition the overlay window from JS (reliable, no JS-API guessing).
/// `expanded` = full panel; otherwise = compact pill. Anchored near the top-right.
/// Small transparent margin baked into the window size: keeps the window hugging
/// the pill closely while leaving a hair of clearance so the rounded pill/panel
/// never touches the window's exact edge (and the upstream macOS frame quirk,
/// tauri#14394, isn't right on the pill). Kept tight per design — close to pill.
const OVERLAY_FRAME_MARGIN: f64 = 4.0;

#[tauri::command]
fn set_overlay_mode(window: tauri::WebviewWindow, expanded: bool) {
    use tauri::{LogicalPosition, LogicalSize};
    // Content size + a transparent margin on every side.
    let (cw, ch) = if expanded { (600.0, 720.0) } else { (288.0, 48.0) };
    let m = OVERLAY_FRAME_MARGIN;
    let (w, h) = (cw + m * 2.0, ch + m * 2.0);
    let _ = window.set_size(LogicalSize::new(w, h));
    // keep it pinned near top-right of the primary screen
    if let Ok(Some(mon)) = window.primary_monitor() {
        let sf = mon.scale_factor();
        let mw = mon.size().width as f64 / sf;
        let x = (mw - w - 40.0).max(20.0);
        let _ = window.set_position(LogicalPosition::new(x, 60.0));
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(ipc::EventSender(tokio::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![set_overlay_mode, ipc::overlay_send])
        .setup(|app| {
            // No Dock icon — the overlay is a background/accessory app (like a
            // menu-bar utility). Without this it shows in the Dock, which an
            // invisible meeting overlay must not.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let win = app.get_webview_window("overlay").expect("overlay window");
            // Cross-platform capture exclusion — the overlay must be invisible to
            // screen recording/sharing.
            let _ = win.set_content_protected(true);
            #[cfg(target_os = "macos")]
            {
                macos::setup_panel(app)?;
                macos::set_sharing_none(app.handle());
                macos::clear_window_chrome(app.handle());
            }

            // Connect to the daemon IPC socket (if launched by the daemon) and
            // bridge it to the web UI.
            ipc::start(app.handle(), ipc::IpcArgs::from_process());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running overlay");
}

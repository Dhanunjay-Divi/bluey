#![allow(deprecated)]

use tauri::{App, Manager};
use tauri_nspanel::{
    cocoa::appkit::NSWindowCollectionBehavior, panel_delegate, WebviewWindowExt as PanelExt,
};

const NS_FLOAT_WINDOW_LEVEL: i32 = 4;
const NS_WINDOW_STYLE_MASK_NON_ACTIVATING_PANEL: i32 = 1 << 7;

pub fn setup_nspanel(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    app.handle().plugin(tauri_nspanel::init())?;

    let window = app.get_webview_window("main").expect("main window not found");
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

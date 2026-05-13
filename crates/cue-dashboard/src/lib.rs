mod commands;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::Mutex;

use cue_daemon::db::Database;
use tauri::Manager;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// Shared database state accessible from Tauri commands.
pub struct DbState(pub Mutex<Database>);

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::list_sessions,
            commands::create_session,
            commands::get_session,
            commands::archive_session,
            commands::delete_session,
        ])
        .setup(|app| {
            // Open database
            let db_path = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("bluey")
                .join("sessions.db");
            let db = Database::open(db_path.to_str().unwrap_or("bluey.db"))
                .expect("failed to open database");
            app.manage(DbState(Mutex::new(db)));

            // Register global shortcut
            register_global_shortcut(app)?;

            #[cfg(target_os = "macos")]
            macos::setup_nspanel(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_global_shortcut(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::ShortcutState;

    let handle = app.handle().clone();
    app.global_shortcut().on_shortcut(
        if cfg!(target_os = "macos") {
            "CmdOrCtrl+Shift+D"
        } else {
            "Ctrl+Shift+D"
        },
        move |_app, shortcut, event| {
            if event.state == ShortcutState::Pressed {
                tracing::debug!(shortcut = %shortcut, "global shortcut pressed");
                if let Some(window) = handle.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        },
    )?;
    Ok(())
}

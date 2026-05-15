mod commands;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::Mutex;

use cue_daemon::db::Database;
use tauri::{Emitter, Manager};

use crate::commands::ActiveSessionState;
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
            commands::update_session_title,
            commands::get_active_session,
            commands::set_active_session,
            commands::list_turns,
            // R5: STT settings + secure key store
            commands::save_stt_api_key,
            commands::load_stt_api_key,
            commands::list_audio_devices,
            commands::save_settings,
            commands::load_settings,
            // R5: Search, Export, Speakers
            commands::search_transcripts,
            commands::export_session_to_clipboard,
            commands::export_session_to_file,
            commands::set_speaker_name,
            commands::list_speakers,
            // R5: Hotkey commands
            commands::daemon_toggle_listening,
            commands::daemon_set_push_to_talk,
            commands::daemon_toggle_overlay,
        ])
        .setup(|app| {
            // Open database
            let db_path = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("bluey")
                .join("sessions.db");
            let db = Database::open(db_path.to_str().unwrap_or("bluey.db"))
                .expect("failed to open database");
            let restored = db.load_active_session().unwrap_or_else(|e| {
                tracing::warn!(error = %e, "failed to restore active session id");
                None
            });
            app.manage(DbState(Mutex::new(db)));
            app.manage(ActiveSessionState(Mutex::new(restored)));

            // Register global shortcuts
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

    // Toggle dashboard visibility: Cmd/Ctrl+Shift+D
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

    // Toggle listening: Cmd/Ctrl+Shift+L
    let handle2 = app.handle().clone();
    app.global_shortcut().on_shortcut(
        if cfg!(target_os = "macos") {
            "CmdOrCtrl+Shift+L"
        } else {
            "Ctrl+Shift+L"
        },
        move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle2.emit("hotkey_toggle_listening", ());
            }
        },
    )?;

    // Push-to-talk toggle: Cmd/Ctrl+Shift+P
    // Note: tauri-plugin-global-shortcut does not expose distinct press/release
    // events, so we use a toggle approach (each press cycles the state).
    let handle3 = app.handle().clone();
    app.global_shortcut().on_shortcut(
        if cfg!(target_os = "macos") {
            "CmdOrCtrl+Shift+P"
        } else {
            "Ctrl+Shift+P"
        },
        move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle3.emit("hotkey_push_to_talk", ());
            }
        },
    )?;

    // Toggle overlay: Cmd/Ctrl+Shift+H
    let handle4 = app.handle().clone();
    app.global_shortcut().on_shortcut(
        if cfg!(target_os = "macos") {
            "CmdOrCtrl+Shift+H"
        } else {
            "Ctrl+Shift+H"
        },
        move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle4.emit("hotkey_toggle_overlay", ());
            }
        },
    )?;

    Ok(())
}

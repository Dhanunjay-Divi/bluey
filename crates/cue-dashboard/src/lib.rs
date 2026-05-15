mod commands;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::Mutex;

use cue_daemon::db::Database;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

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
        // Auto-update plugin. Endpoint + pubkey configured in tauri.conf.json.
        // PLACEHOLDER: replace pubkey and endpoint URL before production release.
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            // R5: Update check
            commands::check_for_updates,
            // R7: Live Transcript
            commands::get_live_transcripts,
            // R6: Permission UX
            commands::open_privacy_settings,
            commands::emit_permission_denied,
            commands::poll_audio_permission,
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

            // R7: Live transcript poller — reads daemon meeting file and emits
            // Tauri events for new segments.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let mut last_count: usize = 0;
                    let mut last_session_id = String::new();
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        let Ok(paths) = cue_core::app_paths::AppPaths::discover() else {
                            continue;
                        };
                        let Ok(store) = cue_daemon::storage::MeetingStore::new(&paths) else {
                            continue;
                        };
                        let Ok(Some(meeting)) = store.load_active() else {
                            if last_count > 0 {
                                last_count = 0;
                                last_session_id.clear();
                            }
                            continue;
                        };
                        let sid = meeting.id.to_string();
                        if sid != last_session_id {
                            last_count = 0;
                            last_session_id = sid.clone();
                        }
                        let total = meeting.transcript.len();
                        if total <= last_count {
                            continue;
                        }
                        for seg in meeting.transcript.iter().skip(last_count) {
                            let source = match seg.speaker {
                                cue_core::Speaker::System => "system",
                                cue_core::Speaker::User => "microphone",
                                _ => "unknown",
                            };
                            let payload = commands::LiveTranscriptPayload {
                                session_id: sid.clone(),
                                source: source.to_string(),
                                text: seg.text.clone(),
                                is_final: seg.is_final,
                                speaker: None,
                                ts_ms: seg.created_at.parse::<u64>().unwrap_or(0),
                            };
                            let _ = handle.emit("live_transcript", &payload);
                        }
                        last_count = total;
                    }
                });
            }

            // Register global shortcuts
            register_global_shortcut(app)?;

            // Setup system tray
            setup_tray(app)?;

            // Auto-update: silent background check after 30s delay
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(30));
                tauri::async_runtime::block_on(async {
                    match tauri_plugin_updater::UpdaterExt::updater(&handle) {
                        Ok(updater) => match updater.check().await {
                            Ok(Some(update)) => {
                                tracing::info!(version = %update.version, "update available");
                                let _ = handle.emit("update_available", update.version.clone());
                            }
                            Ok(None) => {
                                tracing::debug!("no update available");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "update check failed (network?)");
                            }
                        },
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to create updater");
                        }
                    }
                });
            });

            #[cfg(target_os = "macos")]
            macos::setup_nspanel(app)?;

            Ok(())
        })
        // Intercept window close: hide to tray instead of quitting
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let toggle_listening =
        MenuItemBuilder::with_id("toggle_listening", "Toggle Listening").build(app)?;
    let show_dashboard = MenuItemBuilder::with_id("show_dashboard", "Show Dashboard").build(app)?;
    let toggle_overlay = MenuItemBuilder::with_id("toggle_overlay", "Toggle Overlay").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let check_updates =
        MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_listening)
        .item(&show_dashboard)
        .item(&toggle_overlay)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&settings)
        .item(&check_updates)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&quit)
        .build()?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle_listening" => {
                let _ = app.emit("hotkey_toggle_listening", ());
            }
            "show_dashboard" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "toggle_overlay" => {
                let _ = app.emit("hotkey_toggle_overlay", ());
            }
            "settings" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = app.emit("navigate_to", "/settings");
                }
            }
            "check_updates" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    match tauri_plugin_updater::UpdaterExt::updater(&handle) {
                        Ok(updater) => match updater.check().await {
                            Ok(Some(update)) => {
                                let _ = handle.emit("update_available", update.version.clone());
                            }
                            Ok(None) => {
                                let _ = handle.emit("update_not_available", ());
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "manual update check failed");
                                let _ = handle.emit("update_check_failed", e.to_string());
                            }
                        },
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to create updater");
                        }
                    }
                });
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

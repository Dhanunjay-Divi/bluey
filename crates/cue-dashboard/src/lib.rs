mod commands;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::Mutex;

use cue_daemon::db::Database;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

use crate::commands::ActiveSessionState;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// Shared database state accessible from Tauri commands.
pub struct DbState(pub Mutex<Database>);

pub fn run() {
    tauri::Builder::default()
        .manage(InvisibilityState::default())
        .manage({
            let cfg = AutoDisguiseConfig::default();
            if let Ok(paths) = cue_core::app_paths::AppPaths::discover() {
                let st = cue_core::load_settings(&paths).unwrap_or_default();
                cfg.prompted.store(
                    st.auto_disguise_prompted,
                    std::sync::atomic::Ordering::Relaxed,
                );
                cfg.enabled.store(
                    st.auto_disguise_enabled,
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
            cfg
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Auto-update plugin. Endpoint + pubkey configured in tauri.conf.json.
        // PLACEHOLDER: replace pubkey and endpoint URL before production release.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::get_balance_snapshot,
            commands::account_me,
            commands::billing_portal_url,
            commands::sign_out,
            commands::delete_account_now,
            commands::get_signin_url,
            commands::complete_onboarding,
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
            // R8: Process Masquerading
            commands::set_disguise,
            commands::get_disguise,
            // R9: LLM / Cue
            commands::save_llm_api_key,
            commands::list_llm_providers,
            commands::list_responses,
            commands::set_llm_chain,
            // R9: Mouse passthrough + Keybinds
            commands::set_mouse_passthrough,
            commands::get_mouse_passthrough,
            commands::list_keybinds,
            commands::set_keybind,
            commands::reset_keybinds,
            // R10: Cue AI hotkey
            commands::request_cue,
            commands::auto_recap,
            invisibility_toggle,
            invisibility_state,
            auto_disguise_accept,
            auto_disguise_decline
        ])
        .setup(|app| {
            install_deep_link_handler(app);

            // R10: Install anti-debug protections (best-effort, non-fatal)
            if let Err(e) = cue_stealth::install_anti_debug() {
                tracing::warn!(error = %e, "anti-debug installation failed (degraded mode)");
            }
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

            // R8: Apply process disguise on startup
            {
                let db_state: tauri::State<DbState> = app.state();
                let mode_str = db_state
                    .0
                    .lock()
                    .ok()
                    .and_then(|db| db.load_setting("disguise_mode").ok().flatten())
                    .unwrap_or_else(|| "none".to_string());
                let mode = cue_stealth::DisguiseMode::from_str_loose(&mode_str);
                let req = cue_stealth::build_request(mode, None);
                if let Err(e) = cue_stealth::apply_disguise(&req) {
                    tracing::warn!(error = %e, "failed to apply startup disguise");
                }
                // Re-assertion timers: OS sometimes drifts the process title.
                // Read the CURRENT persisted mode at each tick so rapid user
                // changes are respected (no stale overrides).
                let app_name = req.app_name.clone();
                let db_path = db_path.to_str().unwrap_or("bluey.db").to_owned();
                std::thread::spawn(move || {
                    for delay_ms in [200, 1000, 5000] {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                        let current_mode = cue_daemon::db::Database::open(&db_path)
                            .ok()
                            .and_then(|db| db.load_setting("disguise_mode").ok().flatten())
                            .unwrap_or_else(|| "none".to_string());
                        let re_req = cue_stealth::build_request(
                            cue_stealth::DisguiseMode::from_str_loose(&current_mode),
                            None,
                        );
                        let _ = cue_stealth::apply_disguise(&re_req);
                    }
                });
                // Set window title if disguise is active
                if mode != cue_stealth::DisguiseMode::None {
                    let title = app_name.trim().to_owned();
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.set_title(&title);
                    }
                }
            }

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
                        for (i, seg) in meeting.transcript.iter().enumerate().skip(last_count) {
                            let source = match seg.speaker {
                                cue_core::Speaker::System => "system",
                                cue_core::Speaker::User => "microphone",
                                _ => "unknown",
                            };
                            let payload = commands::LiveTranscriptPayload {
                                index: i,
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

            // Stage 18 auto-disguise watch must be installed in the single
            // effective setup closure. Tauri stores only one setup callback.
            spawn_meeting_watch(app.handle().clone());

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
            #[cfg(target_os = "macos")]
            crate::macos::set_sharing_type_none(app.handle());

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

    // Request cue (AI answer): Cmd/Ctrl+Shift+A
    let handle_a = app.handle().clone();
    app.global_shortcut().on_shortcut(
        if cfg!(target_os = "macos") {
            "CmdOrCtrl+Shift+A"
        } else {
            "Ctrl+Shift+A"
        },
        move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle_a.emit("hotkey_request_cue", ());
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

    // Codex Stage 18 commit 6: F19 system-wide invisibility toggle.
    let handle_f19 = app.handle().clone();
    if let Err(e) = app
        .global_shortcut()
        .on_shortcut("F19", move |_app, _sc, event| {
            if event.state == ShortcutState::Pressed {
                let h = handle_f19.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, InvisibilityState> = h.state();
                    let _ = invisibility_toggle(state, h.clone()).await;
                });
            }
        })
    {
        tracing::warn!(error = %e, "F19 shortcut registration failed (Accessibility permission?)");
    }

    Ok(())
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let toggle_listening =
        MenuItemBuilder::with_id("toggle_listening", "Toggle Listening").build(app)?;
    let show_dashboard = MenuItemBuilder::with_id("show_dashboard", "Show Dashboard").build(app)?;
    let toggle_overlay = MenuItemBuilder::with_id("toggle_overlay", "Toggle Overlay").build(app)?;
    let invisible = MenuItemBuilder::with_id("invisible_toggle", "Invisible (F19)").build(app)?;
    let signin = MenuItemBuilder::with_id("signin", "Sign in / Out").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let check_updates =
        MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let current_disguise =
        crate::commands::get_disguise(app.state()).unwrap_or_else(|_| "activity".to_string());
    let label = |value: &str, name: &str| {
        if value == current_disguise {
            format!("✓ {name}")
        } else {
            format!("  {name}")
        }
    };
    let disguise_submenu = SubmenuBuilder::new(app, "Disguise")
        .item(&MenuItemBuilder::with_id("disguise:none", label("none", "Off")).build(app)?)
        .item(
            &MenuItemBuilder::with_id("disguise:activity", label("activity", "Activity Monitor"))
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("disguise:terminal", label("terminal", "Terminal"))
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("disguise:settings", label("settings", "System Settings"))
                .build(app)?,
        )
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_listening)
        .item(&show_dashboard)
        .item(&toggle_overlay)
        .item(&invisible)
        .item(&disguise_submenu)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&signin)
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
            "invisible_toggle" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, InvisibilityState> = handle.state();
                    let _ = invisibility_toggle(state, handle.clone()).await;
                });
            }
            id if id.starts_with("disguise:") => {
                let mode = id.trim_start_matches("disguise:").to_string();
                if let Err(e) = crate::commands::set_disguise(mode.clone(), app.clone()) {
                    tracing::warn!(error = %e, "set_disguise from tray failed");
                } else {
                    tracing::info!(mode = %mode, "disguise changed via tray");
                }
            }
            "signin" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit("navigate_to", "/onboarding");
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

// ─── Codex Stage 18 commit 2: deep-link onboarding handoff ──────────────

fn install_deep_link_handler(app: &tauri::App) {
    use tauri_plugin_deep_link::DeepLinkExt;

    let app_handle = app.handle().clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            let url_str = url.to_string();
            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                handle_deep_link_url(url_str, handle).await;
            });
        }
    });
}

#[derive(serde::Serialize, Clone)]
struct DeepLinkLoginResult {
    success: bool,
    email: Option<String>,
    error: Option<String>,
}

/// Handle an incoming bluey://link?code=... URL: exchange the one-time
/// code for tokens, persist them in the keyring via CloudClient, and
/// emit a "deep_link_login" event the dashboard subscribes to.
async fn handle_deep_link_url(url: String, app: tauri::AppHandle) {
    use tauri::Emitter;

    let parsed = match url::Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(url = %url, error = %e, "deep link parse failed");
            return;
        }
    };

    if parsed.scheme() != "bluey" {
        tracing::warn!(scheme = %parsed.scheme(), "unexpected deep link scheme");
        return;
    }

    if parsed.host_str() != Some("link") {
        tracing::warn!(host = ?parsed.host_str(), "unexpected deep link host");
        return;
    }

    let code = parsed
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned());
    let Some(code) = code else {
        tracing::warn!("deep link missing code query param");
        let _ = app.emit(
            "deep_link_login",
            DeepLinkLoginResult {
                success: false,
                email: None,
                error: Some("missing code".into()),
            },
        );
        return;
    };

    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "cloud client init failed");
            let _ = app.emit(
                "deep_link_login",
                DeepLinkLoginResult {
                    success: false,
                    email: None,
                    error: Some(format!("client init: {e}")),
                },
            );
            return;
        }
    };

    #[derive(serde::Deserialize)]
    struct LinkExchange {
        access_token: String,
        refresh_token: String,
        account: cue_cloud_client::AuthAccountSummary,
    }

    match client
        .public_post::<_, LinkExchange>("/auth/link/exchange", &serde_json::json!({ "code": code }))
        .await
    {
        Ok(resp) => {
            if let Err(e) = client.save_tokens(cue_cloud_client::Tokens {
                access: resp.access_token,
                refresh: resp.refresh_token,
                email: resp.account.email.clone(),
            }) {
                tracing::warn!(error = %e, "save_tokens failed");
                let _ = app.emit(
                    "deep_link_login",
                    DeepLinkLoginResult {
                        success: false,
                        email: Some(resp.account.email),
                        error: Some(format!("keyring: {e}")),
                    },
                );
                return;
            }
            tracing::info!(email = %resp.account.email, "deep-link login success");
            let _ = app.emit(
                "deep_link_login",
                DeepLinkLoginResult {
                    success: true,
                    email: Some(resp.account.email),
                    error: None,
                },
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, "/auth/link/exchange failed");
            let _ = app.emit(
                "deep_link_login",
                DeepLinkLoginResult {
                    success: false,
                    email: None,
                    error: Some(format!("exchange: {e}")),
                },
            );
        }
    }
}

// ─── Codex Stage 18 commit 4: invisibility state + tray "Invisible" toggle ──

#[derive(Clone, Default)]
struct InvisibilityState {
    invisible: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl InvisibilityState {
    fn is_invisible(&self) -> bool {
        self.invisible.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn set(&self, v: bool) {
        self.invisible
            .store(v, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tauri::command]
async fn invisibility_toggle(
    state: tauri::State<'_, InvisibilityState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    use tauri::Emitter;
    let next = !state.is_invisible();
    state.set(next);
    // Toggle overlay visibility (existing daemon command).
    let _ = app.emit("hotkey_toggle_overlay", next);
    // Toggle disguise on/off (default disguise mode is "activity").
    let mode = if next { "activity" } else { "none" };
    if let Err(e) = crate::commands::set_disguise(mode.to_string(), app.clone()) {
        tracing::warn!(error = %e, "set_disguise failed during invisibility toggle");
    }
    let _ = app.emit("invisibility_changed", next);
    Ok(next)
}

#[tauri::command]
fn invisibility_state(state: tauri::State<'_, InvisibilityState>) -> bool {
    state.is_invisible()
}

// ─── Codex Stage 18 commit 8: meeting-app auto-disguise wiring ──────────

#[derive(Default, Clone)]
struct AutoDisguiseConfig {
    /// Has the customer been asked once already?
    pub prompted: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Did they say yes?
    pub enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[tauri::command]
fn auto_disguise_accept(
    cfg: tauri::State<'_, AutoDisguiseConfig>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    cfg.enabled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    cfg.prompted
        .store(true, std::sync::atomic::Ordering::Relaxed);
    persist_auto_disguise_settings(true, true)?;
    if let Err(e) = crate::commands::set_disguise("activity".to_string(), app) {
        tracing::warn!(error = %e, "set_disguise failed during auto_disguise_accept");
    }
    Ok(())
}

#[tauri::command]
fn auto_disguise_decline(cfg: tauri::State<'_, AutoDisguiseConfig>) -> Result<(), String> {
    cfg.prompted
        .store(true, std::sync::atomic::Ordering::Relaxed);
    persist_auto_disguise_settings(true, false)?;
    Ok(())
}

fn persist_auto_disguise_settings(prompted: bool, enabled: bool) -> Result<(), String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let mut settings = cue_core::load_settings(&paths).map_err(|e| e.to_string())?;
    settings.auto_disguise_prompted = prompted;
    settings.auto_disguise_enabled = enabled;
    settings.touch();
    cue_core::save_settings(&paths, &settings).map_err(|e| e.to_string())
}

fn spawn_meeting_watch(app: tauri::AppHandle) {
    use tauri::Emitter;
    let watcher = cue_daemon::cloud::meeting_detect::MeetingWatch::default();
    let _handle = cue_daemon::cloud::meeting_detect::spawn_loop(watcher.clone());
    let mut rx = watcher.subscribe();
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let cur = rx.borrow().clone();
            if let Some(evt) = cur {
                let cfg: tauri::State<'_, AutoDisguiseConfig> = app.state();
                let prompted = cfg.prompted.load(std::sync::atomic::Ordering::Relaxed);
                let enabled = cfg.enabled.load(std::sync::atomic::Ordering::Relaxed);
                if !prompted {
                    let _ = app.emit("auto_disguise_offer", &evt);
                } else if enabled {
                    let _ = crate::commands::set_disguise("activity".to_string(), app.clone());
                }
            }
        }
    });
}

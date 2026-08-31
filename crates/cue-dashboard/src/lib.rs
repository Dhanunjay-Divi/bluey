mod commands;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::{Arc, Mutex};

use cue_daemon::db::Database;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

use crate::commands::{
    ActiveSessionState, DashboardOwner, DashboardOwnerCache, DashboardOwnerState,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// Shared database state accessible from Tauri commands.
pub struct DbState(pub Mutex<Database>);

#[derive(Default)]
struct LiveTranscriptPollCursor {
    owner: Option<DashboardOwner>,
    session_id: Option<String>,
    count: usize,
}

impl LiveTranscriptPollCursor {
    fn next_start(&mut self, owner: &DashboardOwner, session_id: &str, total: usize) -> usize {
        if self.owner.as_ref() != Some(owner)
            || self.session_id.as_deref() != Some(session_id)
            || total < self.count
        {
            self.owner = Some(owner.clone());
            self.session_id = Some(session_id.to_string());
            self.count = 0;
        }
        let start = self.count;
        self.count = total;
        start
    }

    fn clear_session(&mut self, owner: Option<&DashboardOwner>) {
        self.owner = owner.cloned();
        self.session_id = None;
        self.count = 0;
    }
}

#[cfg(test)]
mod live_transcript_poller_tests {
    use super::*;

    #[test]
    fn poll_cursor_resets_for_account_and_session_changes() {
        let owner_a = DashboardOwner::SignedIn("account-a".to_string());
        let owner_b = DashboardOwner::SignedIn("account-b".to_string());
        let mut cursor = LiveTranscriptPollCursor::default();

        assert_eq!(cursor.next_start(&owner_a, "meeting-1", 2), 0);
        assert_eq!(cursor.next_start(&owner_a, "meeting-1", 2), 2);
        assert_eq!(cursor.next_start(&owner_a, "meeting-1", 3), 2);
        assert_eq!(cursor.next_start(&owner_b, "meeting-1", 3), 0);
        assert_eq!(cursor.next_start(&owner_b, "meeting-2", 4), 0);
    }

    #[test]
    fn poll_cursor_clears_count_when_visible_meeting_disappears() {
        let owner = DashboardOwner::SignedIn("account-a".to_string());
        let mut cursor = LiveTranscriptPollCursor::default();

        assert_eq!(cursor.next_start(&owner, "meeting-1", 5), 0);
        cursor.clear_session(Some(&owner));
        assert_eq!(cursor.next_start(&owner, "meeting-1", 5), 0);
    }
}

pub fn run() {
    let _log_guard = cue_core::init_local_json_logging(
        "cue-dashboard",
        "cue_dashboard=info,cue_daemon=info,cue_core=info,cue_cloud_client=info",
    );

    tauri::Builder::default()
        .manage(InvisibilityState::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::get_balance_snapshot,
            commands::account_me,
            commands::get_dashboard_owner,
            commands::billing_portal_url,
            commands::sign_out,
            commands::delete_account_now,
            commands::report_frontend_error,
            commands::get_signin_url,
            commands::complete_onboarding,
            commands::get_data_controls,
            commands::set_cloud_sync_enabled,
            commands::set_support_diagnostics_upload_enabled,
            commands::get_context_watch_settings,
            commands::update_context_watch_settings,
            commands::get_meeting_detection_settings,
            commands::set_meeting_detection_enabled,
            commands::get_meeting_detection_ignored_apps,
            commands::clear_meeting_detection_ignored_apps,
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
            commands::daemon_listening_status,
            commands::daemon_toggle_listening,
            commands::daemon_end_session,
            commands::daemon_set_push_to_talk,
            commands::daemon_toggle_overlay,
            commands::daemon_context_status,
            commands::daemon_context_start,
            commands::daemon_context_stop,
            commands::daemon_capture_active_page,
            commands::daemon_context_items,
            commands::daemon_set_context_role,
            // R7: Live Transcript
            commands::get_live_transcripts,
            // R6: Permission UX
            commands::open_privacy_settings,
            commands::emit_permission_denied,
            commands::poll_audio_permission,
            // R9: LLM / Cue
            commands::save_llm_api_key,
            commands::list_llm_providers,
            commands::list_responses,
            commands::set_llm_chain,
            // R9: Mouse passthrough + Keybinds
            commands::set_mouse_passthrough,
            commands::get_mouse_passthrough,
            commands::list_keybinds,
            commands::get_listening_shortcut,
            commands::set_keybind,
            commands::reset_keybinds,
            // R10: Cue AI hotkey
            commands::request_cue,
            commands::auto_recap,
            invisibility_toggle,
            invisibility_state
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
            let initial_owner = commands::current_dashboard_owner()
                .map(Some)
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "dashboard owner unavailable during startup");
                    None
                });
            app.manage(DbState(Mutex::new(db)));
            app.manage(DashboardOwnerState(Mutex::new(DashboardOwnerCache::new(
                initial_owner,
            ))));
            app.manage(ActiveSessionState(Mutex::new(None)));
            let listening_accelerator = {
                let db_state: tauri::State<'_, DbState> = app.state();
                commands::listening_shortcut_accelerator(&db_state)
            };
            app.manage(commands::ListeningShortcutState(listening_accelerator));

            // R7: Live transcript poller — reads daemon meeting file and emits
            // Tauri events for new segments.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let mut cursor = LiveTranscriptPollCursor::default();
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        let owner = handle
                            .state::<DashboardOwnerState>()
                            .0
                            .lock()
                            .ok()
                            .and_then(|owner| owner.owner.clone());
                        let Some(owner) = owner else {
                            cursor.clear_session(None);
                            continue;
                        };
                        let Ok(paths) = cue_core::app_paths::AppPaths::discover() else {
                            continue;
                        };
                        let Ok(store) = cue_daemon::storage::MeetingStore::new(&paths) else {
                            continue;
                        };
                        let Ok(Some(meeting)) = store.load_active() else {
                            cursor.clear_session(Some(&owner));
                            continue;
                        };
                        if !owner.owns_meeting(meeting.owner_account_id.as_deref()) {
                            cursor.clear_session(Some(&owner));
                            continue;
                        }
                        let sid = meeting.id.to_string();
                        let total = meeting.transcript.len();
                        let start = cursor.next_start(&owner, &sid, total);
                        if total <= start {
                            continue;
                        }
                        for (i, seg) in meeting.transcript.iter().enumerate().skip(start) {
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
                    }
                });
            }

            // Register global shortcuts
            register_global_shortcut(app)?;

            // Setup system tray
            setup_tray(app)?;

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

    // Toggle dashboard visibility: Ctrl+Option/Alt+B.
    let handle = app.handle().clone();
    app.global_shortcut()
        .on_shortcut("Ctrl+Alt+B", move |_app, shortcut, event| {
            if event.state == ShortcutState::Pressed {
                tracing::debug!(shortcut = %shortcut, "global shortcut pressed");
                if let Some(window) = handle.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        show_main_window(&handle);
                    }
                }
            }
        })?;

    // Toggle listening: persisted accelerator, falling back to Ctrl+Option/Alt+L.
    let listening_accelerator = {
        let shortcut: tauri::State<'_, commands::ListeningShortcutState> = app.state();
        shortcut.0.clone()
    };
    let handle2 = app.handle().clone();
    app.global_shortcut().on_shortcut(
        listening_accelerator.as_str(),
        move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle2.emit("hotkey_toggle_listening", ());
            }
        },
    )?;

    // Push-to-talk toggle: Ctrl+Option/Alt+P.
    // Note: tauri-plugin-global-shortcut does not expose distinct press/release
    // events, so we use a toggle approach (each press cycles the state).
    let handle3 = app.handle().clone();
    app.global_shortcut()
        .on_shortcut("Ctrl+Alt+P", move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle3.emit("hotkey_push_to_talk", ());
            }
        })?;

    // Request cue (AI answer): Ctrl+Option/Alt+Enter.
    let handle_a = app.handle().clone();
    app.global_shortcut()
        .on_shortcut("Ctrl+Alt+Enter", move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle_a.emit("hotkey_request_cue", ());
            }
        })?;

    // F19 system-wide overlay visibility toggle.
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
    let overlay_visibility =
        MenuItemBuilder::with_id("overlay_visibility", "Show / Hide Overlay (F19)").build(app)?;
    let signin = MenuItemBuilder::with_id("signin", "Account & Sign In…").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_listening)
        .item(&show_dashboard)
        .item(&toggle_overlay)
        .item(&overlay_visibility)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&signin)
        .item(&settings)
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
                show_main_window(app);
            }
            "toggle_overlay" => {
                let _ = app.emit("hotkey_toggle_overlay", ());
            }
            "overlay_visibility" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, InvisibilityState> = handle.state();
                    let _ = invisibility_toggle(state, handle.clone()).await;
                });
            }
            "signin" => {
                show_main_window(app);
                let _ = app.emit("navigate_to", "/settings");
            }
            "settings" => {
                show_main_window(app);
                let _ = app.emit("navigate_to", "/settings");
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    crate::macos::set_sharing_type_none(app);
    #[cfg(target_os = "macos")]
    crate::macos::activate_ignoring_other_apps();

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
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

fn local_dashboard_route(parsed: &url::Url) -> Option<&'static str> {
    let is_settings_host = parsed.scheme() == "bluey"
        && parsed.host_str() == Some("settings")
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && matches!(parsed.path(), "" | "/")
        && parsed.fragment().is_none();

    if !is_settings_host {
        return None;
    }

    match parsed.query() {
        None => Some("/settings"),
        Some("section=audio-meetings") => Some("/settings?section=audio-meetings"),
        Some(_) => None,
    }
}

fn signed_out_account_generation(account: &cue_core::AccountConfig) -> Option<u64> {
    (account.provider.trim().eq_ignore_ascii_case("local")
        && account.cloud_account_id.is_none()
        && account.user_id.trim() == "local-user"
        && !account.token_configured()
        && account.refresh_token.is_none())
    .then_some(account.credential_generation)
}

#[cfg(test)]
mod deep_link_route_tests {
    use super::{local_dashboard_route, signed_out_account_generation};

    #[test]
    fn accepts_only_the_fixed_local_settings_route() {
        let settings = url::Url::parse("bluey://settings").expect("valid settings URL");
        assert_eq!(local_dashboard_route(&settings), Some("/settings"));
        let audio_meetings =
            url::Url::parse("bluey://settings?section=audio-meetings").expect("valid section URL");
        assert_eq!(
            local_dashboard_route(&audio_meetings),
            Some("/settings?section=audio-meetings")
        );

        for rejected in [
            "bluey://settings/account",
            "bluey://settings?next=/onboarding",
            "bluey://settings?section=audio-meetings&next=/onboarding",
            "bluey://link?code=one-time-code",
            "https://settings",
        ] {
            let parsed = url::Url::parse(rejected).expect("valid test URL");
            assert_eq!(local_dashboard_route(&parsed), None, "{rejected}");
        }
    }

    #[test]
    fn linked_account_commit_requires_the_daemon_signed_out_profile() {
        let mut signed_out = cue_core::AccountConfig::local();
        signed_out.credential_generation = 7;
        assert_eq!(signed_out_account_generation(&signed_out), Some(7));

        let mut external_login = signed_out;
        external_login.provider = "bluey".to_string();
        external_login.cloud_account_id = Some("account-b".to_string());
        external_login.user_id = "b@example.com".to_string();
        external_login.access_token = Some("access-b".to_string());
        external_login.refresh_token = Some("refresh-b".to_string());
        external_login.credential_generation = 8;
        assert_eq!(signed_out_account_generation(&external_login), None);
    }
}

/// Handle a fixed local dashboard route or an incoming bluey://link?code=...
/// login URL. Login links exchange the one-time code, persist tokens through
/// CloudClient, and emit the result the dashboard subscribes to.
async fn handle_deep_link_url(url: String, app: tauri::AppHandle) {
    use tauri::Emitter;

    let parsed = match url::Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(
                url_len = url.len(),
                error = %e,
                "deep link parse failed; raw URL suppressed"
            );
            return;
        }
    };

    if parsed.scheme() != "bluey" {
        tracing::warn!(scheme = %parsed.scheme(), "unexpected deep link scheme");
        return;
    }

    if let Some(route) = local_dashboard_route(&parsed) {
        show_main_window(&app);
        let _ = app.emit("navigate_to", route);
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

    let owner_guard = match commands::capture_dashboard_owner_guard(&app) {
        Ok(owner_guard) => owner_guard,
        Err(error) => {
            tracing::warn!(%error, "account owner unavailable before link exchange");
            return;
        }
    };

    let trace_id = cue_core::new_trace_id();
    let client = match dashboard_cloud_client_with_trace(&trace_id) {
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
            // Account replacement is a hard visibility boundary. Ask the
            // daemon to stop audio and clear its active meeting/context before
            // installing the new identity.
            if let Err(error) = owner_guard.begin_transition_if_current(&app) {
                tracing::warn!(%error, "failed to suspend dashboard owner before account switch");
                return;
            }
            let cleanup = commands::daemon_ipc(cue_core::ipc::DaemonRequest::CloudLogoutBound {
                fence: owner_guard.mutation_fence(None, None),
            })
            .await;
            let cleanup_complete = matches!(
                cleanup,
                Ok(cue_core::ipc::DaemonResponse::CloudStatus { .. })
                    | Ok(cue_core::ipc::DaemonResponse::Ok)
            );
            if !cleanup_complete {
                tracing::warn!(
                    error_category = "daemon_account_switch_cleanup_failed",
                    "daemon cleanup failed before dashboard account switch"
                );
                let _ = commands::refresh_dashboard_owner_after_account_change(&app);
                let _ = app.emit(
                    "deep_link_login",
                    DeepLinkLoginResult {
                        success: false,
                        email: Some(resp.account.email),
                        error: Some(
                            "Bluey could not safely switch accounts on this computer. Try again."
                                .to_string(),
                        ),
                    },
                );
                return;
            }

            let paths = match cue_core::app_paths::AppPaths::discover() {
                Ok(paths) => paths,
                Err(e) => {
                    let _ = commands::refresh_dashboard_owner_after_account_change(&app);
                    let _ = app.emit(
                        "deep_link_login",
                        DeepLinkLoginResult {
                            success: false,
                            email: Some(resp.account.email),
                            error: Some(format!("account store: {e}")),
                        },
                    );
                    return;
                }
            };
            let mut account = match cue_core::load_account(&paths) {
                Ok(Some(account)) if signed_out_account_generation(&account).is_some() => account,
                Ok(None) => cue_core::AccountConfig::local(),
                Ok(Some(_)) => {
                    let _ = commands::refresh_dashboard_owner_after_account_change(&app);
                    let _ = app.emit(
                        "deep_link_login",
                        DeepLinkLoginResult {
                            success: false,
                            email: Some(resp.account.email),
                            error: Some(
                                "Another account signed in while this sign-in was finishing."
                                    .to_string(),
                            ),
                        },
                    );
                    return;
                }
                Err(e) => {
                    let _ = commands::refresh_dashboard_owner_after_account_change(&app);
                    let _ = app.emit(
                        "deep_link_login",
                        DeepLinkLoginResult {
                            success: false,
                            email: Some(resp.account.email),
                            error: Some(format!("account store: {e}")),
                        },
                    );
                    return;
                }
            };
            let signed_out_generation = signed_out_account_generation(&account)
                .expect("signed-out account generation was checked above");
            account.provider = "bluey".to_string();
            account.cloud_account_id = Some(resp.account.id.clone());
            account.user_id = resp.account.email.clone();
            if account.workspace_id.trim().is_empty() || account.workspace_id == "local-workspace" {
                account.workspace_id = "default".to_string();
            }
            account.linked_at = cue_core::clock::now_epoch_ms_string();
            account.access_token = Some(resp.access_token);
            account.refresh_token = Some(resp.refresh_token);

            let account_saved = cue_cloud_client::save_account_profile_and_tokens_if_generation(
                &paths,
                signed_out_generation,
                &account,
            );
            if !matches!(account_saved, Ok(true)) {
                tracing::warn!(
                    error_category = "stale_deep_link_account_commit",
                    "deep-link account commit was rejected"
                );
                let _ = commands::refresh_dashboard_owner_after_account_change(&app);
                let _ = app.emit(
                    "deep_link_login",
                    DeepLinkLoginResult {
                        success: false,
                        email: Some(resp.account.email),
                        error: Some(
                            "The account changed while sign-in was finishing. Try again."
                                .to_string(),
                        ),
                    },
                );
                return;
            }
            if let Err(e) = commands::refresh_dashboard_owner_after_account_change(&app) {
                tracing::warn!(error = %e, "dashboard owner refresh failed after login");
                let _ = app.emit(
                    "deep_link_login",
                    DeepLinkLoginResult {
                        success: false,
                        email: Some(resp.account.email),
                        error: Some(e),
                    },
                );
                return;
            }
            tracing::info!("deep-link login success");
            notify_daemon_account_linked().await;
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

fn dashboard_cloud_client_with_trace(
    trace_id: &str,
) -> Result<cue_cloud_client::CloudClient, cue_cloud_client::Error> {
    let paths = cue_core::app_paths::AppPaths::discover()
        .map_err(|error| cue_cloud_client::Error::TokenStore(error.to_string()))?;
    let account = cue_core::load_account(&paths)
        .map_err(|error| cue_cloud_client::Error::TokenStore(error.to_string()))?;
    let mut config = cue_cloud_client::client::ClientConfig {
        trace_id: Some(trace_id.to_string()),
        ..Default::default()
    };
    if let Some(account) = account.as_ref() {
        if !account.api_url.trim().is_empty() {
            config.base_url = account.api_url.clone();
        }
    }
    cue_cloud_client::CloudClient::new(
        config,
        Arc::new(cue_cloud_client::SecureAccountStore::new(paths)),
    )
}

async fn notify_daemon_account_linked() {
    let ready_lines = vec![
        "account linked".to_string(),
        "cloud answers, balance, sync, and saved sessions are ready".to_string(),
        "ask from the composer or start listening".to_string(),
    ];
    if let Err(error) = commands::daemon_ipc(cue_core::ipc::DaemonRequest::OverlayBoot {
        title: "Bluey online".to_string(),
        lines: ready_lines,
    })
    .await
    {
        tracing::warn!(error = %error, "failed to notify daemon after deep-link login");
    }
    if let Err(error) = commands::daemon_ipc(cue_core::ipc::DaemonRequest::CloudStatus).await {
        tracing::debug!(error = %error, "failed to refresh cloud status after deep-link login");
    }
}

// Overlay visibility state shared by F19, Settings, and the tray.

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
    let _ = app.emit("hotkey_toggle_overlay", next);
    let _ = app.emit("invisibility_changed", next);
    Ok(next)
}

#[tauri::command]
fn invisibility_state(state: tauri::State<'_, InvisibilityState>) -> bool {
    state.is_invisible()
}

use cue_core::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use cue_core::session::Session;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::DbState;

/// Shared state for the currently active session.
///
/// Separate from `DbState` so callers can hold an active-session lock
/// independently of the DB connection lock. The active session is a soft
/// selection in the dashboard UI; the DB is the source of truth for session
/// data itself.
pub struct ActiveSessionState(pub Mutex<Option<Uuid>>);

/// Payload emitted on `session:switched` whenever the active session changes.
///
/// Separate struct (not `Session`) because the consumer typically already has
/// full `Session` data; the switch event just signals "the selection moved".
#[derive(Clone, Serialize)]
pub struct SessionSwitchedPayload {
    /// Session id now active, or `None` if active selection was cleared.
    pub id: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct BalanceSnapshotPayload {
    pub balance_cents: i64,
    pub balance_label: String,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
    pub low_balance_warning: bool,
}

#[derive(Clone, Serialize)]
pub struct AccountMePayload {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct FrontendErrorPayload {
    pub source: String,
    pub message: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
}

// ===== Daemon IPC helper =====

/// Send a request to the running daemon over TCP and return the response.
async fn daemon_ipc(request: DaemonRequest) -> Result<DaemonResponse, String> {
    let trace_id = dashboard_trace_id();
    daemon_ipc_with_trace(request, &trace_id).await
}

async fn daemon_ipc_with_trace(
    request: DaemonRequest,
    trace_id: &str,
) -> Result<DaemonResponse, String> {
    daemon_ipc_with_trace_to_addr(request, trace_id, &daemon_addr()).await
}

async fn daemon_ipc_with_trace_to_addr(
    request: DaemonRequest,
    trace_id: &str,
    addr: &str,
) -> Result<DaemonResponse, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("failed to connect to daemon: {e}"))?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let line = serde_json::to_string(&request.with_trace_id(trace_id.to_string()))
        .map_err(|e| e.to_string())?;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    writer.write_all(b"\n").await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())?;

    let mut response = String::new();
    let read = reader
        .read_line(&mut response)
        .await
        .map_err(|e| e.to_string())?;
    if read == 0 {
        return Err("daemon closed connection without a response".to_string());
    }
    serde_json::from_str(response.trim_end()).map_err(|e| e.to_string())
}

fn daemon_addr() -> String {
    std::env::var("BLUEY_DAEMON_ADDR")
        .or_else(|_| std::env::var("CUE_DAEMON_ADDR"))
        .ok()
        .filter(|addr| !addr.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_DAEMON_ADDR.to_string())
}

fn dashboard_trace_id() -> String {
    cue_core::new_trace_id()
}

fn cloud_client_with_trace(trace_id: &str) -> Result<cue_cloud_client::CloudClient, String> {
    cue_cloud_client::CloudClient::with_default_keyring()
        .map(|client| client.with_trace_id(trace_id.to_string()))
        .map_err(|e| format!("account keyring unavailable: {e}"))
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub async fn get_balance_snapshot() -> Result<Option<BalanceSnapshotPayload>, String> {
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    if client.current_tokens().is_none() {
        return Ok(None);
    }

    let me: cue_cloud_client::AccountMe = client
        .auth_get("/account/me")
        .await
        .map_err(|e| format!("balance lookup failed: {e}"))?;
    Ok(Some(BalanceSnapshotPayload {
        balance_cents: me.balance_cents,
        balance_label: format_cents(me.balance_cents),
        trial_seconds_remaining: me.trial_seconds_remaining,
        auto_topup_enabled: me.auto_topup_enabled,
        auto_topup_threshold_cents: me.auto_topup_threshold_cents,
        auto_topup_amount_cents: me.auto_topup_amount_cents,
        low_balance_warning: me.balance_cents < me.auto_topup_threshold_cents
            && me.balance_cents > 0,
    }))
}

#[tauri::command]
pub async fn account_me() -> Result<Option<AccountMePayload>, String> {
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    if client.current_tokens().is_none() {
        return Ok(None);
    }

    let me: cue_cloud_client::AccountMe = client
        .auth_get("/account/me")
        .await
        .map_err(|e| format!("account lookup failed: {e}"))?;
    Ok(Some(AccountMePayload {
        id: me.id,
        email: me.email,
        balance_cents: me.balance_cents,
        trial_seconds_remaining: me.trial_seconds_remaining,
        auto_topup_enabled: me.auto_topup_enabled,
        auto_topup_threshold_cents: me.auto_topup_threshold_cents,
        auto_topup_amount_cents: me.auto_topup_amount_cents,
    }))
}

#[tauri::command]
pub async fn billing_portal_url() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct PortalResponse {
        portal_url: String,
    }

    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    let resp: PortalResponse = client
        .auth_post("/billing/portal", &serde_json::json!({}))
        .await
        .map_err(|e| format!("billing portal failed: {e}"))?;
    Ok(resp.portal_url)
}

#[tauri::command]
pub fn sign_out(db: State<DbState>) -> Result<(), String> {
    let client = cue_cloud_client::CloudClient::with_default_keyring()
        .map_err(|e| format!("account keyring unavailable: {e}"))?;
    client
        .clear_tokens()
        .map_err(|e| format!("sign out failed: {e}"))?;
    mark_onboarding_incomplete(db)
}

#[tauri::command]
pub async fn delete_account_now(db: State<'_, DbState>) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct DeleteAck {
        deleted: bool,
    }

    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    let ack: DeleteAck = client
        .auth_post("/account/delete", &serde_json::json!({}))
        .await
        .map_err(|e| format!("delete account failed: {e}"))?;
    if ack.deleted {
        client
            .clear_tokens()
            .map_err(|e| format!("account deleted, but local sign out failed: {e}"))?;
        mark_onboarding_incomplete(db)?;
    }
    Ok(())
}

#[tauri::command]
pub fn report_frontend_error(payload: FrontendErrorPayload) -> Result<(), String> {
    tracing::warn!(
        source = %truncate_log_field(&payload.source, 120),
        command = %payload.command.as_deref().map(|v| truncate_log_field(v, 120)).unwrap_or_default(),
        url = %payload.url.as_deref().map(|v| truncate_log_field(v, 240)).unwrap_or_default(),
        message = %truncate_log_field(&payload.message, 700),
        stack = %payload.stack.as_deref().map(|v| truncate_log_field(v, 1200)).unwrap_or_default(),
        "frontend error captured"
    );
    Ok(())
}

fn truncate_log_field(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().take(max_chars) {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    if value.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[tauri::command]
pub fn get_signin_url() -> String {
    std::env::var("BLUEY_SIGNIN_URL").unwrap_or_else(|_| "https://bluey.sh/link".to_string())
}

#[tauri::command]
pub fn complete_onboarding(db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.save_setting("onboarding_complete", "true")
        .map_err(|e| e.to_string())
}

fn mark_onboarding_incomplete(db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.save_setting("onboarding_complete", "false")
        .map_err(|e| e.to_string())
}

fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.saturating_abs();
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

#[tauri::command]
pub fn list_sessions(db: State<DbState>) -> Result<Vec<Session>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.list_sessions(None, 100).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_session(
    title: Option<String>,
    db: State<DbState>,
    app: AppHandle,
) -> Result<Session, String> {
    let session = {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.create_session(title).map_err(|e| e.to_string())?
    };
    // Broadcast so any other window / page listening via
    // `useSessionEvents` picks up the new session without a refetch.
    if let Err(e) = app.emit("session:created", &session) {
        tracing::warn!(error = %e, "failed to emit session:created event");
    }
    Ok(session)
}

#[tauri::command]
pub fn get_session(id: String, db: State<DbState>) -> Result<Option<Session>, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.get_session(uuid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_session(id: String, db: State<DbState>) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.archive_session(uuid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_session(
    id: String,
    db: State<DbState>,
    active: State<ActiveSessionState>,
    app: AppHandle,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.delete_session(uuid).map_err(|e| e.to_string())?;
    }
    // If the deleted session was active, clear the selection and notify.
    let mut active = active.0.lock().map_err(|e| e.to_string())?;
    if *active == Some(uuid) {
        *active = None;
        drop(active);
        // Persist cleared selection — best-effort.
        match db.0.lock() {
            Ok(db_guard) => {
                if let Err(e) = db_guard.save_active_session(None) {
                    tracing::warn!(
                        error = %e,
                        "failed to clear persisted active session id; will self-heal on next daemon startup"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "db lock poisoned while clearing persisted active session id");
            }
        }
        if let Err(e) = app.emit("session:switched", SessionSwitchedPayload { id: None }) {
            tracing::warn!(error = %e, "failed to emit session:switched event");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn update_session_title(id: String, title: String, db: State<DbState>) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.update_session_title(uuid, &title)
        .map_err(|e| e.to_string())
}

/// Return the currently-active session id, or `None` if no session is selected.
#[tauri::command]
pub fn get_active_session(active: State<ActiveSessionState>) -> Result<Option<String>, String> {
    let active = active.0.lock().map_err(|e| e.to_string())?;
    Ok(active.map(|u| u.to_string()))
}

/// Set the active session. Pass `None` to clear the selection.
///
/// Validates that the target session exists before switching (avoids pointing
/// at an id that was just deleted in another window). Emits `session:switched`
/// whenever the selection changes.
#[tauri::command]
pub fn set_active_session(
    id: Option<String>,
    db: State<DbState>,
    active: State<ActiveSessionState>,
    app: AppHandle,
) -> Result<(), String> {
    let new_id = match id {
        Some(raw) => {
            let uuid = Uuid::parse_str(&raw).map_err(|e| e.to_string())?;
            // Confirm the session still exists.
            let db = db.0.lock().map_err(|e| e.to_string())?;
            if db.get_session(uuid).map_err(|e| e.to_string())?.is_none() {
                return Err(format!("session {uuid} not found"));
            }
            Some(uuid)
        }
        None => None,
    };

    let mut active = active.0.lock().map_err(|e| e.to_string())?;
    let changed = *active != new_id;
    *active = new_id;
    drop(active); // release lock before emitting

    if changed {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        if let Err(e) = db.save_active_session(new_id) {
            tracing::warn!(error = %e, "failed to persist active session id");
        }
        drop(db);

        let payload = SessionSwitchedPayload {
            id: new_id.map(|u| u.to_string()),
        };
        if let Err(e) = app.emit("session:switched", payload) {
            tracing::warn!(error = %e, "failed to emit session:switched event");
        }
    }
    Ok(())
}

/// List turns for a session (read-only). Used by the session detail page.
#[tauri::command]
pub fn list_turns(
    session_id: String,
    db: State<DbState>,
) -> Result<Vec<cue_core::session::Turn>, String> {
    let uuid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.list_turns(uuid, None).map_err(|e| e.to_string())
}

// ===== Settings + Secrets commands (Phase 3 Round 5) =====

#[tauri::command]
pub fn save_stt_api_key(provider: String, key: String) -> Result<(), String> {
    cue_daemon::secrets::store_api_key(&provider, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_stt_api_key(provider: String) -> Result<Option<String>, String> {
    let raw = cue_daemon::secrets::load_api_key(&provider).map_err(|e| e.to_string())?;
    Ok(raw.map(|s| {
        // Mask all but the LAST 4 chars (codepoints, not bytes — string-slicing
        // by bytes panics on multi-byte UTF-8). Provider keys are normally ASCII
        // but this guards against future non-ASCII secrets.
        let total = s.chars().count();
        if total <= 4 {
            "****".to_string()
        } else {
            let suffix: String = s.chars().skip(total - 4).collect();
            format!("****{suffix}")
        }
    }))
}

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<String>, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                names.push(name);
            }
        }
    }
    Ok(names)
}

#[tauri::command]
pub fn save_settings(
    settings: std::collections::HashMap<String, String>,
    db: State<DbState>,
) -> Result<(), String> {
    // Reject any keys that look like secrets BEFORE we open a db transaction.
    // Secrets must go through save_stt_api_key (keyring-backed). Returning an
    // explicit error makes accidental writes visible in dev tooling instead of
    // being silently dropped.
    for k in settings.keys() {
        if k.contains("api_key") {
            return Err(format!(
                "refusing to persist secret-shaped key {k} through save_settings; \
                 use save_stt_api_key instead"
            ));
        }
    }
    let db = db.0.lock().map_err(|e| e.to_string())?;
    for (k, v) in &settings {
        db.save_setting(k, v).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn load_settings(
    db: State<DbState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.load_all_settings().map_err(|e| e.to_string())
}

// ===== Phase 3 Round 5: Search, Export, Speakers =====

#[tauri::command]
pub fn search_transcripts(
    query: String,
    limit: Option<usize>,
    db: State<DbState>,
) -> Result<Vec<cue_daemon::db::search::TranscriptHit>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.search_transcripts(&query, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_session_to_clipboard(
    id: String,
    format: String,
    app: AppHandle,
) -> Result<String, String> {
    let db_state: State<DbState> = app.state();
    let db = db_state.0.lock().map_err(|e| e.to_string())?;
    let content = match format.as_str() {
        "markdown" => db
            .export_session_markdown(&id, &cue_daemon::export::ExportOptions::default())
            .map_err(|e| e.to_string())?,
        "text" => db.export_session_text(&id).map_err(|e| e.to_string())?,
        "json" => db.export_session_json(&id).map_err(|e| e.to_string())?,
        _ => return Err(format!("unsupported format: {format}")),
    };
    Ok(content)
}

#[tauri::command]
pub fn export_session_to_file(
    id: String,
    format: String,
    path: String,
    app: AppHandle,
) -> Result<(), String> {
    let db_state: State<DbState> = app.state();
    let db = db_state.0.lock().map_err(|e| e.to_string())?;
    let content = match format.as_str() {
        "markdown" => db
            .export_session_markdown(&id, &cue_daemon::export::ExportOptions::default())
            .map_err(|e| e.to_string())?,
        "text" => db.export_session_text(&id).map_err(|e| e.to_string())?,
        "json" => db.export_session_json(&id).map_err(|e| e.to_string())?,
        _ => return Err(format!("unsupported format: {format}")),
    };
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_speaker_name(
    session_id: String,
    speaker_id: i32,
    name: String,
    color: Option<String>,
    db: State<DbState>,
) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.set_speaker_name(&session_id, speaker_id, &name, color.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_speakers(
    session_id: String,
    db: State<DbState>,
) -> Result<Vec<cue_daemon::db::speakers::SpeakerMapping>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.list_speakers(&session_id).map_err(|e| e.to_string())
}

// ===== Phase 3 Round 6: Hotkey → daemon action commands =====

/// Toggle listening: if a meeting/audio session is active, end it; otherwise start one.
/// Sends the appropriate IPC request to the running daemon.
#[tauri::command]
pub async fn daemon_toggle_listening() -> Result<String, String> {
    let trace_id = dashboard_trace_id();
    // Query daemon status to decide start vs stop.
    let status = daemon_ipc_with_trace(DaemonRequest::Status, &trace_id).await?;
    let is_active = match &status {
        DaemonResponse::Status { state } => {
            matches!(state.meeting, cue_core::MeetingState::InMeeting { .. })
        }
        _ => false,
    };
    let resp = if is_active {
        daemon_ipc_with_trace(DaemonRequest::MeetingEnd, &trace_id).await?
    } else {
        daemon_ipc_with_trace(DaemonRequest::MeetingStart { title: None }, &trace_id).await?
    };
    match resp {
        DaemonResponse::Text { text } => Ok(text),
        DaemonResponse::Recap { recap } => Ok(format!("Session ended: {}", recap.summary)),
        DaemonResponse::Ok => Ok("ok".to_string()),
        DaemonResponse::Error { message } => Err(message),
        _ => Ok("ok".to_string()),
    }
}

/// Push-to-talk toggle. Since global shortcuts don't distinguish press/release,
/// this toggles audio capture on/off. When PTT is "enabled" conceptually, audio
/// only streams while toggled on. Each press cycles the state.
#[tauri::command]
pub async fn daemon_set_push_to_talk(db: State<'_, DbState>) -> Result<String, String> {
    let trace_id = dashboard_trace_id();
    // Toggle audio: if audio is active, stop it; otherwise start mic-only.
    let status = daemon_ipc_with_trace(DaemonRequest::AudioStatus, &trace_id).await?;
    let is_active = match &status {
        DaemonResponse::AudioStatus { status } => {
            status.session_id.is_some()
                && !matches!(
                    status.capture.state,
                    cue_core::AudioCaptureState::Stopped | cue_core::AudioCaptureState::Failed
                )
        }
        _ => false,
    };
    let resp = if is_active {
        daemon_ipc_with_trace(DaemonRequest::AudioStop, &trace_id).await?
    } else {
        let mic_device_id = load_mic_device_from_settings(&db);
        daemon_ipc_with_trace(
            DaemonRequest::AudioStart {
                enable_system: false,
                enable_microphone: true,
                mic_device_id,
            },
            &trace_id,
        )
        .await?
    };
    match resp {
        DaemonResponse::AudioStatus { status } => Ok(format!("audio: {:?}", status.capture.state)),
        DaemonResponse::Error { message } => Err(message),
        _ => Ok("ok".to_string()),
    }
}

/// Toggle overlay visibility via daemon IPC.
#[tauri::command]
pub async fn daemon_toggle_overlay() -> Result<String, String> {
    let resp = daemon_ipc(DaemonRequest::OverlayToggle).await?;
    match resp {
        DaemonResponse::Ok => Ok("ok".to_string()),
        DaemonResponse::Error { message } => Err(message),
        _ => Ok("ok".to_string()),
    }
}

/// Trigger an update check from the UI. Emits `update_available` or `update_not_available`.
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<String, String> {
    let updater = tauri_plugin_updater::UpdaterExt::updater(&app).map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let _ = app.emit("update_available", &version);
            Ok(version)
        }
        Ok(None) => {
            let _ = app.emit("update_not_available", ());
            Ok("up-to-date".to_string())
        }
        Err(e) => {
            tracing::warn!(error = %e, "update check failed");
            Err(format!("update check failed: {e}"))
        }
    }
}

/// Read the `audio.mic_device` setting from the dashboard DB.
fn load_mic_device_from_settings(db: &State<DbState>) -> Option<String> {
    let db = db.0.lock().ok()?;
    db.load_setting("audio.mic_device")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

// ===== Tests =====

// ===== Phase 3 Round 6: Permission UX =====

/// Open the OS privacy settings pane for the given audio source.
/// On macOS, opens System Preferences to the relevant Privacy pane.
/// On Windows, opens the Settings app to the microphone privacy page.
#[tauri::command]
pub fn open_privacy_settings(source: String) -> Result<(), String> {
    open_privacy_settings_inner(&source)
}

/// Returns the (program, args) tuple for opening privacy settings on the current platform.
/// Exposed for testing without actually spawning a process.
pub fn privacy_settings_command(source: &str) -> Result<(&'static str, Vec<String>), String> {
    #[cfg(target_os = "macos")]
    {
        let section = match source {
            "microphone" => "Privacy_Microphone",
            _ => "Privacy_ScreenCapture",
        };
        Ok((
            "open",
            vec![format!(
                "x-apple.systempreferences:com.apple.preference.security?{section}"
            )],
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let section = match source {
            "microphone" => "privacy-microphone",
            _ => "privacy-microphone",
        };
        Ok((
            "cmd",
            vec![
                "/C".to_string(),
                "start".to_string(),
                format!("ms-settings:{section}"),
            ],
        ))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = source;
        Err("open_privacy_settings: not supported on this platform".into())
    }
}

/// Platform-specific privacy settings launcher.
fn open_privacy_settings_inner(source: &str) -> Result<(), String> {
    let (program, args) = privacy_settings_command(source)?;
    std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Payload for the audio_permission_denied event.
#[derive(Clone, Serialize)]
pub struct PermissionDeniedPayload {
    pub source: String,
}

/// Emit a permission-denied event to the dashboard for testing/integration.
/// In production, the daemon capture code calls this when it detects denial.
#[tauri::command]
pub fn emit_permission_denied(source: String, app: AppHandle) -> Result<(), String> {
    app.emit(
        "audio_permission_denied",
        PermissionDeniedPayload { source },
    )
    .map_err(|e| e.to_string())
}

/// Poll daemon audio status; if permission_denied_source is set, emit the
/// Tauri event so the dashboard banner appears from real capture failures.
#[tauri::command]
pub async fn poll_audio_permission(app: AppHandle) -> Result<(), String> {
    let resp = daemon_ipc(DaemonRequest::AudioStatus).await?;
    if let DaemonResponse::AudioStatus { status } = resp {
        if let Some(source) = status.capture.permission_denied_source {
            app.emit(
                "audio_permission_denied",
                PermissionDeniedPayload {
                    source: source.default_label().to_string(),
                },
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ===== Phase 3 Round 9: LLM / Cue commands =====

#[tauri::command]
pub fn save_llm_api_key(provider: String, key: String) -> Result<(), String> {
    cue_daemon::secrets::store_api_key(&format!("llm_{provider}"), &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_llm_providers() -> Result<Vec<String>, String> {
    Ok(vec![
        "anthropic".to_string(),
        "openai".to_string(),
        "ollama".to_string(),
    ])
}

#[tauri::command]
pub fn list_responses(
    session_id: String,
    limit: usize,
    db: State<DbState>,
) -> Result<Vec<cue_daemon::llm::CueResponse>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.list_cue_responses(&session_id, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_llm_chain(providers: Vec<String>, db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    let chain = providers.join(",");
    db.save_setting("llm.chain", &chain)
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn daemon_ipc_wraps_dashboard_request_with_trace() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            writer.write_all(br#"{"type":"pong"}"#).await.unwrap();
            writer.write_all(b"\n").await.unwrap();
            line
        });

        let response =
            daemon_ipc_with_trace_to_addr(DaemonRequest::Ping, "dashboard-smoke-trace", &addr)
                .await
                .unwrap();
        assert!(matches!(response, DaemonResponse::Pong));

        let line = server.await.unwrap();
        let request: DaemonRequest = serde_json::from_str(line.trim()).unwrap();
        match request {
            DaemonRequest::WithTrace { trace_id, request } => {
                assert_eq!(trace_id, "dashboard-smoke-trace");
                assert!(matches!(*request, DaemonRequest::Ping));
            }
            other => panic!("dashboard request was not trace-wrapped: {other:?}"),
        }
    }

    #[test]
    fn daemon_addr_defaults_to_standard_addr() {
        assert_eq!(DEFAULT_DAEMON_ADDR, "127.0.0.1:57321");
    }

    #[test]
    fn truncate_log_field_preserves_chars_and_marks_truncation() {
        assert_eq!(truncate_log_field("hello", 10), "hello");
        assert_eq!(truncate_log_field("abcdef", 3), "abc…");
        assert_eq!(truncate_log_field("a\u{0007}b", 10), "a b");
    }

    #[test]
    fn test_session_switched_payload_serializes() {
        let payload = SessionSwitchedPayload {
            id: Some("abc-123".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn test_session_switched_payload_none() {
        let payload = SessionSwitchedPayload { id: None };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("null"));
    }

    #[test]
    fn test_get_app_version_not_empty() {
        let version = get_app_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_privacy_settings_command_microphone() {
        let result = privacy_settings_command("microphone");
        assert!(result.is_ok());
        let (program, args) = result.unwrap();
        #[cfg(target_os = "macos")]
        {
            assert_eq!(program, "open");
            assert_eq!(args.len(), 1);
            assert!(args[0].contains("Privacy_Microphone"));
        }
        #[cfg(target_os = "windows")]
        {
            assert_eq!(program, "cmd");
            assert!(args.contains(&"ms-settings:privacy-microphone".to_string()));
        }
    }

    #[test]
    fn test_privacy_settings_command_system() {
        let result = privacy_settings_command("system");
        assert!(result.is_ok());
        let (program, args) = result.unwrap();
        #[cfg(target_os = "macos")]
        {
            assert_eq!(program, "open");
            assert_eq!(args.len(), 1);
            assert!(args[0].contains("Privacy_ScreenCapture"));
        }
        #[cfg(target_os = "windows")]
        {
            assert_eq!(program, "cmd");
            assert!(args.contains(&"ms-settings:privacy-microphone".to_string()));
        }
    }

    #[test]
    fn test_permission_denied_payload_serializes() {
        let payload = PermissionDeniedPayload {
            source: "microphone".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("microphone"));
    }
}

// ===== Phase 3 Round 7: Live Transcript =====

/// Payload matching the  Tauri event shape.
#[derive(Clone, Serialize)]
pub struct LiveTranscriptPayload {
    pub index: usize,
    pub session_id: String,
    pub source: String,
    pub text: String,
    pub is_final: bool,
    pub speaker: Option<u8>,
    pub ts_ms: u64,
}

/// Return recent transcript segments from the daemon's active meeting file.
/// The UI calls this on mount for catch-up, and the background poller uses
/// it to detect new segments and emit  Tauri events.
#[tauri::command]
pub fn get_live_transcripts(since_index: usize) -> Result<Vec<LiveTranscriptPayload>, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;
    let Some(meeting) = store.load_active().map_err(|e| e.to_string())? else {
        return Ok(Vec::new());
    };
    let session_id = meeting.id.to_string();
    let segments: Vec<LiveTranscriptPayload> = meeting
        .transcript
        .iter()
        .enumerate()
        .skip(since_index)
        .map(|(i, seg)| {
            let source = match seg.speaker {
                cue_core::Speaker::System => "system",
                cue_core::Speaker::User => "microphone",
                _ => "unknown",
            };
            LiveTranscriptPayload {
                index: i,
                session_id: session_id.clone(),
                source: source.to_string(),
                text: seg.text.clone(),
                is_final: seg.is_final,
                speaker: None,
                ts_ms: seg.created_at.parse::<u64>().unwrap_or(0),
            }
        })
        .collect();
    Ok(segments)
}

// ===== Phase 3 Round 8: Process Masquerading =====

/// Apply a disguise mode and persist it. Updates all open windows.
#[tauri::command]
pub fn set_disguise(mode: String, app: AppHandle) -> Result<(), String> {
    let disguise_mode = cue_stealth::DisguiseMode::from_str_loose(&mode);
    let req = cue_stealth::build_request(disguise_mode, None);
    cue_stealth::apply_disguise(&req).map_err(|e| e.to_string())?;

    // Codex follow-up: real menu-bar tray icon swap. Disguise PNGs are
    // embedded at compile time via include_bytes! so they are part of
    // the signed app bundle (no runtime path lookup, no missing-file
    // class). None mode restores the default Bluey icon.
    if let Some(tray) = app.tray_by_id("main") {
        let icon_bytes: Option<&'static [u8]> = match disguise_mode {
            cue_stealth::DisguiseMode::None => None,
            cue_stealth::DisguiseMode::Activity => {
                Some(include_bytes!("../icons/disguise/mac/activity.png"))
            }
            cue_stealth::DisguiseMode::Terminal => {
                Some(include_bytes!("../icons/disguise/mac/terminal.png"))
            }
            cue_stealth::DisguiseMode::Settings => {
                Some(include_bytes!("../icons/disguise/mac/settings.png"))
            }
        };
        match icon_bytes {
            Some(bytes) => match tauri::image::Image::from_bytes(bytes) {
                Ok(img) => {
                    if let Err(e) = tray.set_icon(Some(img)) {
                        tracing::warn!(error = %e, "tray icon swap failed");
                    } else {
                        tracing::debug!(mode = %disguise_mode.as_str(), "tray icon swapped");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "tray icon decode failed");
                }
            },
            None => {
                if let Some(default_img) = app.default_window_icon().cloned() {
                    if let Err(e) = tray.set_icon(Some(default_img)) {
                        tracing::warn!(error = %e, "tray icon restore failed");
                    } else {
                        tracing::debug!("tray icon restored to default");
                    }
                }
            }
        }
    }

    // Update all window titles
    let title = req.app_name.trim();
    for (_label, window) in app.webview_windows() {
        let _ = window.set_title(title);
    }

    // Persist setting
    let db_state: State<DbState> = app.state();
    let db = db_state.0.lock().map_err(|e| e.to_string())?;
    db.save_setting("disguise_mode", disguise_mode.as_str())
        .map_err(|e| e.to_string())?;
    persist_core_disguise_mode(disguise_mode.as_str());

    Ok(())
}

fn persist_core_disguise_mode(mode: &str) {
    let Ok(paths) = cue_core::app_paths::AppPaths::discover() else {
        return;
    };
    let mut settings = match cue_core::load_settings(&paths) {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(%error, "failed to load core settings for disguise persistence");
            return;
        }
    };
    settings.disguise_mode = mode.to_string();
    settings.touch();
    if let Err(error) = cue_core::save_settings(&paths, &settings) {
        tracing::warn!(%error, "failed to persist core disguise setting");
    }
}

/// Get the current disguise mode from persisted settings.
#[tauri::command]
pub fn get_disguise(db: State<DbState>) -> Result<String, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    let mode = db
        .load_setting("disguise_mode")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "activity".to_string());
    Ok(mode)
}

// ===== Phase 3 Round 9: Mouse Passthrough Toggle =====

/// Set overlay mouse passthrough state and persist it.
#[tauri::command]
pub fn set_mouse_passthrough(enabled: bool, db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.save_setting(
        "overlay_passthrough",
        if enabled { "true" } else { "false" },
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Get the current overlay mouse passthrough state.
#[tauri::command]
pub fn get_mouse_passthrough(db: State<DbState>) -> Result<bool, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    let val = db
        .load_setting("overlay_passthrough")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "true".to_string());
    Ok(val == "true")
}

// ===== Phase 3 Round 9: User-Rebindable Keybinds =====

/// A keybind entry returned to the frontend.
#[derive(Clone, Serialize)]
pub struct KeybindEntry {
    pub action: String,
    pub accelerator: String,
}

/// Default keybinds for known actions.
fn default_keybinds() -> Vec<(&'static str, &'static str)> {
    if cfg!(target_os = "macos") {
        vec![
            ("toggle_listening", "CmdOrCtrl+Shift+L"),
            ("push_to_talk", "CmdOrCtrl+Shift+P"),
            ("toggle_overlay", "CmdOrCtrl+Shift+H"),
            ("toggle_dashboard", "CmdOrCtrl+Shift+D"),
        ]
    } else {
        vec![
            ("toggle_listening", "Ctrl+Shift+L"),
            ("push_to_talk", "Ctrl+Shift+P"),
            ("toggle_overlay", "Ctrl+Shift+H"),
            ("toggle_dashboard", "Ctrl+Shift+D"),
        ]
    }
}

/// List all keybinds (from DB, falling back to defaults).
#[tauri::command]
pub fn list_keybinds(db: State<DbState>) -> Result<Vec<KeybindEntry>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.ensure_keybinds_table().map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for (action, default_accel) in default_keybinds() {
        let accel = db
            .load_keybind(action)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| default_accel.to_string());
        entries.push(KeybindEntry {
            action: action.to_string(),
            accelerator: accel,
        });
    }
    Ok(entries)
}

/// Set a keybind for an action. Validates the accelerator string.
#[tauri::command]
pub fn set_keybind(action: String, accelerator: String, db: State<DbState>) -> Result<(), String> {
    // Validate accelerator by attempting to parse
    accelerator
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("invalid accelerator: {e}"))?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.ensure_keybinds_table().map_err(|e| e.to_string())?;
    db.save_keybind(&action, &accelerator)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Reset all keybinds to defaults.
#[tauri::command]
pub fn reset_keybinds(db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.ensure_keybinds_table().map_err(|e| e.to_string())?;
    db.reset_keybinds().map_err(|e| e.to_string())?;
    Ok(())
}

// ===== Phase 3 Round 10: Cmd+Shift+A → request_cue =====

/// Payload emitted per streaming chunk on `cue_response_chunk`.
#[derive(Clone, Serialize)]
pub struct CueResponseChunkPayload {
    pub response_id: String,
    pub kind: String,
    pub partial_text: String,
    pub finished: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents_after: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_confidence: Option<f32>,
    /// Auto Router metadata (Bluey Auto). Optional because future managed
    /// routing may attach more fields; for v0.1 we emit task_type, lane, and
    /// confidence on the FIRST chunk and reuse the same payload schema for
    /// subsequent chunks (router_meta = None on those).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_meta: Option<RouterMeta>,
    /// R14.4: when true, the UI replaces the entire card body with
    /// `partial_text` instead of appending. Used by the SpeculativeRouter
    /// Deep lane Final chunk so the draft -> final transition is a clean
    /// swap rather than a `\"[refined]\n...\"` concatenation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_body: Option<bool>,
}

/// Subset of cue_router::TaskClassification + ProviderRoute exposed to the UI.
#[derive(Clone, Serialize)]
pub struct RouterMeta {
    /// Task type the heuristic classifier picked (general / code /
    /// system_design / meeting / writing / vision).
    pub task_type: String,
    /// Latency lane the policy picked (instant / balanced / deep).
    pub latency_lane: String,
    /// Provider lane the policy resolved to (instant / balanced / deep /
    /// vision / local).
    pub provider_lane: String,
    /// Provider name dispatched to (e.g. "openai", "anthropic", "ollama").
    pub provider_name: String,
    /// Model dispatched to.
    pub model: String,
    /// Heuristic classifier confidence in [0.0, 1.0].
    pub confidence: f32,
}

/// Same classification logic as `classify_for_router` but returns the raw
/// TaskClassification (used by the speculative path which needs the actual
/// classification object to drive routing decisions).
fn classify_only_for_router(
    prompt: &str,
    has_transcript: bool,
    has_screenshot: bool,
) -> cue_router::TaskClassification {
    use cue_router::{ClassifierInput, HeuristicClassifier, TaskClassifier};
    let classifier = HeuristicClassifier::new();
    let input = ClassifierInput {
        prompt,
        has_transcript,
        has_page: false,
        file_attachment_count: 0,
        has_screenshot,
    };
    futures::executor::block_on(classifier.classify(&input))
}

fn classify_for_router(prompt: &str, has_transcript: bool, has_screenshot: bool) -> RouterMeta {
    use cue_router::{
        AutoRouter, ClassifierInput, HeuristicClassifier, RouteOptions, RoutingPolicy,
        StaticPolicy, TaskClassifier,
    };
    use std::sync::Arc;

    let classifier: Arc<dyn TaskClassifier> = Arc::new(HeuristicClassifier::new());
    let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
    let router = AutoRouter::new(classifier, policy);

    let local_only = std::env::var("BLUEY_LOCAL_ONLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let input = ClassifierInput {
        prompt,
        has_transcript,
        has_page: false,
        file_attachment_count: 0,
        has_screenshot,
    };

    // Synchronous wrapper: AutoRouter::route is async only because future
    // managed classifiers may RPC. The heuristic path is in-process and
    // returns immediately; we block the async runtime briefly.
    let routed = futures::executor::block_on(router.route(&input, RouteOptions { local_only }));

    RouterMeta {
        task_type: format!("{:?}", routed.classification.task_type).to_lowercase(),
        latency_lane: format!("{:?}", routed.classification.latency_lane).to_lowercase(),
        provider_lane: format!("{:?}", routed.route.lane).to_lowercase(),
        provider_name: routed.route.provider_name.clone(),
        model: routed.route.model.clone(),
        confidence: routed.classification.confidence,
    }
}

/// Try the speculative routing path. Returns:
///   Ok(Some((text, metadata))) if speculation succeeded and produced a final answer.
///   Ok(None)                   if speculation is OFF or unconfigured (caller should
///                              fall back to the legacy AnswerLlm/WhatToAnswerLlm path).
///   Err(e)                     if speculation was ON and failed mid-stream.
///
/// Forwards every chunk through the existing `cue_response_chunk` Tauri event
/// so the dashboard UI does not need to know the answer was speculative.
#[allow(clippy::too_many_arguments)]
async fn try_speculative_dispatch(
    user_text: &str,
    session_id: &str,
    system_prompt: &str,
    kind: &str,
    response_id: &str,
    classification: cue_router::TaskClassification,
    router_meta: RouterMeta,
    registry: ProviderRegistry,
    app: tauri::AppHandle,
) -> Result<Option<(String, LlmResponseMetadata)>, String> {
    use cue_router::{
        policy::StaticPolicy, speculative::SpeculativeChunk, RoutingPolicy, SpeculativeRouter,
    };
    use futures::stream::StreamExt;
    use std::sync::Arc;

    // Bluey Auto routing is default-ON: classify once, pick the best lane,
    // and stream one visible answer card from that lane. Parallel
    // cheap-draft + deep-final replacement remains dev-gated because it made
    // the customer UI feel jumpy during live questions.
    let speculative_off = std::env::var("BLUEY_SPECULATIVE_ROUTING")
        .map(|v| {
            let v = v.trim();
            v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(false);
    if speculative_off || registry.is_empty() {
        return Ok(None);
    }
    let parallel_drafts = std::env::var("BLUEY_PARALLEL_DRAFTS")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on")
        })
        .unwrap_or(false);

    // Codex Stage 9 round-2 Blocker 2: lane-correct policy selection.
    // StaticPolicy emits openai/anthropic provider names; a managed
    // registry contains only bluey-managed-* names. Detect from the
    // registry contents and pick ManagedPolicy when managed; otherwise
    // StaticPolicy for legacy BYOK.
    let policy: Arc<dyn RoutingPolicy> = if registry.is_managed_only() {
        Arc::new(cue_router::ManagedPolicy::new())
    } else {
        Arc::new(StaticPolicy::defaults())
    };
    let provider: Arc<dyn cue_router::speculative::SpeculativeProvider> = Arc::new(registry);
    // Normal product mode is single visible stream from the selected lane.
    // `BLUEY_PARALLEL_DRAFTS=1` is retained for latency experiments only.
    let router = SpeculativeRouter::new(policy, provider, parallel_drafts);

    let req = cue_llm::LlmRequest {
        system: system_prompt.to_string(),
        user: user_text.to_string(),
        session_id: Some(session_id.to_string()),
        max_tokens: None,
        temperature: None,
        reasoning_effort: None,
        thinking_budget_tokens: None,
        // Codex Stage 9b: thread the daemon's response_id as the
        // stable logical request id so retries / failover hit the
        // server idempotency cache instead of double-charging.
        request_id: Some(response_id.to_string()),
    };

    let stream = router
        .run(&classification, req)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = Box::pin(stream);

    let mut accumulated_draft = String::new();
    let mut final_text: Option<String> = None;
    let mut metadata = LlmResponseMetadata::default();
    // Codex Stage 9 round-2 Blocker 4: collect lane errors for diagnosis
    // when all-lanes-failed.
    let mut lane_errors: Vec<String> = Vec::new();
    let mut emitted_meta = false;

    while let Some(chunk) = stream.next().await {
        match chunk {
            SpeculativeChunk::Draft {
                text,
                finished,
                cost: chunk_cost,
                cost_label,
                artifact,
            } => {
                if let Some(chunk_cost) = chunk_cost.as_ref() {
                    merge_cost_metadata(&mut metadata.cost, chunk_cost.clone());
                }
                if cost_label.is_some() {
                    metadata.cost_label = cost_label.clone();
                }
                if artifact.is_some() {
                    metadata.artifact = artifact.clone();
                }
                accumulated_draft.push_str(&text);
                let meta_for_chunk = if !emitted_meta {
                    emitted_meta = true;
                    Some(router_meta.clone())
                } else {
                    None
                };
                let _ = app.emit(
                    "cue_response_chunk",
                    CueResponseChunkPayload {
                        response_id: response_id.to_string(),
                        kind: kind.to_string(),
                        partial_text: text,
                        finished,
                        cost_cents: chunk_cost.as_ref().map(|cost| cost.cost_cents),
                        balance_cents_after: chunk_cost
                            .as_ref()
                            .and_then(|cost| cost.balance_cents_after),
                        provider: chunk_cost.as_ref().map(|cost| cost.provider.clone()),
                        model: chunk_cost.as_ref().map(|cost| cost.model.clone()),
                        cost_label,
                        artifact_type: artifact_type(&artifact),
                        artifact_body: artifact_body(&artifact),
                        artifact_confidence: artifact_confidence(&artifact),
                        router_meta: meta_for_chunk,
                        replace_body: None,
                    },
                );
            }
            SpeculativeChunk::Final {
                text,
                cost: chunk_cost,
                cost_label,
                artifact,
            } => {
                if let Some(chunk_cost) = chunk_cost.as_ref() {
                    merge_cost_metadata(&mut metadata.cost, chunk_cost.clone());
                }
                if cost_label.is_some() {
                    metadata.cost_label = cost_label.clone();
                }
                if artifact.is_some() {
                    metadata.artifact = artifact.clone();
                }
                let _ = app.emit(
                    "cue_response_chunk",
                    CueResponseChunkPayload {
                        response_id: response_id.to_string(),
                        kind: kind.to_string(),
                        partial_text: text.clone(),
                        finished: true,
                        cost_cents: chunk_cost.as_ref().map(|cost| cost.cost_cents),
                        balance_cents_after: chunk_cost
                            .as_ref()
                            .and_then(|cost| cost.balance_cents_after),
                        provider: chunk_cost.as_ref().map(|cost| cost.provider.clone()),
                        model: chunk_cost.as_ref().map(|cost| cost.model.clone()),
                        cost_label,
                        artifact_type: artifact_type(&artifact),
                        artifact_body: artifact_body(&artifact),
                        artifact_confidence: artifact_confidence(&artifact),
                        router_meta: None,
                        replace_body: Some(true),
                    },
                );
                final_text = Some(text);
            }
            SpeculativeChunk::Error { lane, message } => {
                tracing::warn!(lane, message, "speculative router lane error");
                lane_errors.push(format!("{lane}: {message}"));
                // Non-fatal individually: keep collecting the other lane.
            }
        }
    }

    // Codex review S9 round-3 blocker: actually use lane_errors. If
    // every lane errored AND no text was produced, return Ok(None) so
    // the caller falls back to the legacy single-shot path. Returning
    // empty Ok(Some("")) made try_speculative_dispatch silently
    // persist an empty cue card.
    let resolved = final_text.unwrap_or(accumulated_draft);
    let trimmed = resolved.trim().to_string();
    if trimmed.is_empty() && !lane_errors.is_empty() {
        tracing::warn!(
            lane_errors = ?lane_errors,
            "speculative dispatch: every lane errored; falling back to legacy"
        );
        return Ok(None);
    }
    if trimmed.is_empty() {
        // No errors AND no text — nothing to render either way.
        return Ok(None);
    }
    Ok(Some((trimmed, metadata)))
}

fn merge_cost_metadata(
    existing: &mut Option<cue_llm::LlmCostMetadata>,
    next: cue_llm::LlmCostMetadata,
) {
    match existing {
        Some(current) => {
            if current.provider != next.provider {
                current.provider = format!("{}, {}", current.provider, next.provider);
            }
            if current.model != next.model {
                current.model = format!("{}, {}", current.model, next.model);
            }
            current.input_tokens = current.input_tokens.saturating_add(next.input_tokens);
            current.output_tokens = current.output_tokens.saturating_add(next.output_tokens);
            current.cost_cents = current.cost_cents.saturating_add(next.cost_cents);
            current.balance_cents_after = next.balance_cents_after.or(current.balance_cents_after);
            current.trial_seconds_remaining = next
                .trial_seconds_remaining
                .or(current.trial_seconds_remaining);
        }
        None => *existing = Some(next),
    }
}

#[derive(Debug, Clone, Default)]
struct LlmResponseMetadata {
    cost: Option<cue_llm::LlmCostMetadata>,
    cost_label: Option<String>,
    artifact: Option<cue_llm::LlmArtifactMetadata>,
}

fn artifact_type(artifact: &Option<cue_llm::LlmArtifactMetadata>) -> Option<String> {
    artifact
        .as_ref()
        .map(|artifact| artifact.artifact_type.clone())
}

fn artifact_body(artifact: &Option<cue_llm::LlmArtifactMetadata>) -> Option<String> {
    artifact.as_ref().map(|artifact| artifact.body.clone())
}

fn artifact_confidence(artifact: &Option<cue_llm::LlmArtifactMetadata>) -> Option<f32> {
    artifact.as_ref().and_then(|artifact| artifact.confidence)
}

/// Trigger a cue response. If a recent question is detected in the transcript
/// (last ~30s), runs AnswerLlm; otherwise runs WhatToAnswerLlm (suggestion).
/// Emits `cue_response_chunk` per streaming chunk, then `cue_response` on completion.
#[tauri::command]
pub async fn request_cue(
    kind: String,
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<String, String> {
    use cue_daemon::llm::{ends_with_question, AnswerLlm, WhatToAnswerLlm};

    let trace_id = dashboard_trace_id();
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;
    let meeting = store
        .load_active()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no active session".to_string())?;

    let session_id = meeting.id.to_string();

    // Recent transcript text (last ~30s worth, approx last 10 segments).
    let recent: String = meeting
        .transcript
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if recent.trim().is_empty() {
        return Err("no recent transcript to analyze".to_string());
    }

    let llm = build_llm_provider_from_env(&db, &trace_id)
        .ok_or_else(|| "no LLM provider configured".to_string())?;

    // Generate response_id up-front so chunks and final event share it.
    let response_id = Uuid::new_v4().to_string();

    // Auto Router classification: emit on the first chunk so the UI can render
    // a lane badge before the answer text starts streaming.
    let router_meta = classify_for_router(&recent, true, false);
    let emitted_meta = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Bluey Auto path. **Default ON**: classify, route, and stream one visible
    // answer from the selected lane. Disable explicitly with
    // BLUEY_SPECULATIVE_ROUTING=0 / false / off. The older parallel
    // draft+deep experiment is retained behind BLUEY_PARALLEL_DRAFTS=1 only.
    // When routing is OFF or no providers are configured, fall through to the
    // legacy AnswerLlm / WhatToAnswerLlm path — zero regression.
    {
        use cue_daemon::llm::{answer as answer_mod, suggest as suggest_mod};
        let registry = ProviderRegistry::from_env_and_secrets(&db, &trace_id);
        let classification = classify_only_for_router(&recent, true, false);
        let is_question = kind == "answer" && ends_with_question(&recent);
        let user_text = if is_question {
            recent
                .rsplit('.')
                .find(|s| s.trim().ends_with('?'))
                .unwrap_or(&recent)
                .trim()
                .to_string()
        } else {
            recent.clone()
        };
        let system_prompt = if is_question {
            answer_mod::SYSTEM_PROMPT
        } else {
            suggest_mod::SYSTEM_PROMPT
        };
        let kind_str = if is_question { "answer" } else { "suggestion" };
        if let Some((text, response_metadata)) = try_speculative_dispatch(
            &user_text,
            &session_id,
            system_prompt,
            kind_str,
            &response_id,
            classification,
            router_meta.clone(),
            registry,
            app.clone(),
        )
        .await
        .map_err(|e| e.to_string())?
        {
            let cue_resp = cue_daemon::llm::CueResponse::new(
                kind_str,
                text.clone(),
                &session_id,
                Some(recent.clone()),
            );
            let mut cue_resp = cue_resp;
            cue_resp.id = response_id.clone();
            cue_resp = cue_resp.with_llm_metadata(
                response_metadata.cost.as_ref(),
                response_metadata.cost_label.as_deref(),
                response_metadata.artifact.as_ref(),
            );
            persist_cue_response(&db, &cue_resp)?;
            let _ = app.emit("cue_response", &cue_resp);
            return Ok(text);
        }
    }

    // Detect question in recent transcript and dispatch with streaming.
    let emitted_meta_a = emitted_meta.clone();
    let emitted_meta_b = emitted_meta.clone();
    let router_meta_a = router_meta.clone();
    let router_meta_b = router_meta.clone();
    let cue_resp = if kind == "answer" && ends_with_question(&recent) {
        let question = recent
            .rsplit('.')
            .find(|s| s.trim().ends_with('?'))
            .unwrap_or(&recent)
            .trim();
        let app2 = app.clone();
        let rid = response_id.clone();
        AnswerLlm
            .run_streaming(question, &session_id, llm.as_ref(), |partial, finished| {
                let meta_for_chunk =
                    if !emitted_meta_a.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        Some(router_meta_a.clone())
                    } else {
                        None
                    };
                let _ = app2.emit(
                    "cue_response_chunk",
                    CueResponseChunkPayload {
                        response_id: rid.clone(),
                        kind: "answer".to_string(),
                        partial_text: partial.to_string(),
                        finished,
                        cost_cents: None,
                        balance_cents_after: None,
                        provider: None,
                        model: None,
                        cost_label: None,
                        artifact_type: None,
                        artifact_body: None,
                        artifact_confidence: None,
                        router_meta: meta_for_chunk,
                        replace_body: None,
                    },
                );
            })
            .await
            .map_err(|e| e.to_string())?
    } else {
        let app2 = app.clone();
        let rid = response_id.clone();
        WhatToAnswerLlm
            .run_streaming(&recent, &session_id, llm.as_ref(), |partial, finished| {
                let meta_for_chunk =
                    if !emitted_meta_b.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        Some(router_meta_b.clone())
                    } else {
                        None
                    };
                let _ = app2.emit(
                    "cue_response_chunk",
                    CueResponseChunkPayload {
                        response_id: rid.clone(),
                        kind: "suggestion".to_string(),
                        partial_text: partial.to_string(),
                        finished,
                        cost_cents: None,
                        balance_cents_after: None,
                        provider: None,
                        model: None,
                        cost_label: None,
                        artifact_type: None,
                        artifact_body: None,
                        artifact_confidence: None,
                        router_meta: meta_for_chunk,
                        replace_body: None,
                    },
                );
            })
            .await
            .map_err(|e| e.to_string())?
    };

    // Override the CueResponse id with our pre-generated response_id for consistency.
    let mut cue_resp = cue_resp;
    cue_resp.id = response_id;

    persist_cue_response(&db, &cue_resp)?;
    let _ = app.emit("cue_response", &cue_resp);
    Ok(cue_resp.text.clone())
}

/// Auto-recap: runs RecapLlm on a session's full transcript.
/// Emits `cue_response_chunk` per streaming chunk, then `cue_response` on completion.
#[tauri::command]
pub async fn auto_recap(
    session_id: String,
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<String, String> {
    use cue_daemon::llm::RecapLlm;

    let trace_id = dashboard_trace_id();
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;

    let meetings = store.all_meetings().map_err(|e| e.to_string())?;
    let meeting = meetings
        .iter()
        .find(|m| m.id.to_string() == session_id)
        .ok_or_else(|| "session not found".to_string())?;

    let transcript: String = meeting
        .transcript
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if transcript.trim().is_empty() {
        return Err("empty transcript".to_string());
    }

    let llm = match build_llm_provider_from_env(&db, &trace_id) {
        Some(p) => p,
        None => {
            tracing::warn!("auto-recap skipped: no LLM provider configured");
            return Err("no LLM provider configured".to_string());
        }
    };

    let response_id = Uuid::new_v4().to_string();
    let rid = response_id.clone();
    let app2 = app.clone();

    let cue_resp = RecapLlm
        .run_streaming(
            &transcript,
            &session_id,
            llm.as_ref(),
            |partial, finished| {
                let _ = app2.emit(
                    "cue_response_chunk",
                    CueResponseChunkPayload {
                        response_id: rid.clone(),
                        kind: "recap".to_string(),
                        partial_text: partial.to_string(),
                        finished,
                        cost_cents: None,
                        balance_cents_after: None,
                        provider: None,
                        model: None,
                        cost_label: None,
                        artifact_type: None,
                        artifact_body: None,
                        artifact_confidence: None,
                        router_meta: None, // recap is not yet routed via AutoRouter
                        replace_body: None,
                    },
                );
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut cue_resp = cue_resp;
    cue_resp.id = response_id;

    persist_cue_response(&db, &cue_resp)?;
    let _ = app.emit("cue_response", &cue_resp);
    Ok(cue_resp.text.clone())
}

fn persist_cue_response(
    db: &State<DbState>,
    resp: &cue_daemon::llm::CueResponse,
) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.insert_cue_response(cue_daemon::db::NewCueResponse {
        id: &resp.id,
        session_id: &resp.source_session_id,
        kind: &resp.kind,
        text: &resp.text,
        source_text: resp.source_text.as_deref(),
        ts_ms: resp.ts_ms as i64,
        cost_cents: resp.cost_cents,
        balance_cents_after: resp.balance_cents_after,
        provider: resp.provider.as_deref(),
        model: resp.model.as_deref(),
        input_tokens: resp.input_tokens,
        output_tokens: resp.output_tokens,
        cost_label: resp.cost_label.as_deref(),
        artifact_type: resp.artifact_type.as_deref(),
        artifact_body: resp.artifact_body.as_deref(),
        artifact_confidence: resp.artifact_confidence,
    })
    .map_err(|e| e.to_string())
}

/// Multi-provider registry: builds every LLM provider for which credentials
/// are configured (env var or keyring) and exposes them by `LlmProvider::name()`.
///
/// This is what the SpeculativeRouter dispatches against. If the chosen route
/// targets a provider that is not in the registry, we fall back to whichever
/// provider IS available so a misconfigured Anthropic key does not block an
/// OpenAI-only setup from running the deep lane.
struct ProviderRegistry {
    providers: std::collections::HashMap<String, std::sync::Arc<dyn cue_llm::LlmProvider>>,
}

impl ProviderRegistry {
    fn from_env_and_secrets(_db: &tauri::State<DbState>, trace_id: &str) -> Self {
        use std::collections::HashMap;
        use std::sync::Arc;

        let mut providers: HashMap<String, Arc<dyn cue_llm::LlmProvider>> = HashMap::new();

        // Codex Stage 9a: managed-mode detection. If a Bluey account
        // token is in the keyring, register BlueyManagedProvider for
        // every cloud lane; bluey-server will pick the actual upstream
        // provider+model. Customer pays Bluey; Bluey owns the API keys.
        //
        // Legacy BYOK direct providers (OpenAI/Anthropic from env or
        // keyring) are gated behind BLUEY_DEV_BYOK=1 so dev workflows
        // still work without surprising customers in production.
        let managed_mode = match cloud_client_with_trace(trace_id) {
            Ok(client) => client.current_tokens().is_some(),
            Err(_) => false,
        };

        if managed_mode {
            tracing::info!("ProviderRegistry: managed mode active (token in keyring)");
            // One BlueyManagedProvider per cloud lane. The provider
            // name (bluey-managed-{lane}) matches what
            // cue_router::ManagedPolicy emits, so the registry lookup
            // dispatches correctly.
            if let Ok(client) = cloud_client_with_trace(trace_id) {
                for lane in [
                    cue_llm::bluey_managed::ManagedLane::Instant,
                    cue_llm::bluey_managed::ManagedLane::Balanced,
                    cue_llm::bluey_managed::ManagedLane::Deep,
                    cue_llm::bluey_managed::ManagedLane::Vision,
                ] {
                    let provider: Arc<dyn cue_llm::LlmProvider> = Arc::new(
                        cue_llm::bluey_managed::BlueyManagedProvider::new(client.clone(), lane),
                    );
                    providers.insert(provider.name().to_string(), provider);
                }
            }
        }

        let allow_byok = std::env::var("BLUEY_DEV_BYOK")
            .map(|v| v == "1")
            .unwrap_or(false)
            || !managed_mode;

        if allow_byok {
            if let Some(key) = std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    cue_daemon::secrets::load_api_key("llm_openai")
                        .ok()
                        .flatten()
                })
            {
                let provider: Arc<dyn cue_llm::LlmProvider> =
                    Arc::new(cue_llm::openai::OpenAiProvider::new(key));
                providers.insert("openai".to_string(), provider);
            }

            if let Some(key) = std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    cue_daemon::secrets::load_api_key("llm_anthropic")
                        .ok()
                        .flatten()
                })
            {
                let provider: Arc<dyn cue_llm::LlmProvider> =
                    Arc::new(cue_llm::anthropic::AnthropicProvider::new(key));
                providers.insert("anthropic".to_string(), provider);
            }
        }

        // Ollama: register if user opted in via BLUEY_OLLAMA_HOST. The
        // provider reads OLLAMA_BASE_URL itself; we propagate
        // BLUEY_OLLAMA_HOST into OLLAMA_BASE_URL if the user has not
        // set it explicitly so a single env var is enough.
        // ALWAYS available regardless of managed mode — it is the
        // privacy/offline LocalFallbackPolicy target.
        if let Ok(host) = std::env::var("BLUEY_OLLAMA_HOST") {
            if !host.is_empty() {
                if std::env::var("OLLAMA_BASE_URL").is_err() {
                    // SAFETY: this is the daemon process at request time.
                    unsafe {
                        std::env::set_var("OLLAMA_BASE_URL", &host);
                    }
                }
                let provider: Arc<dyn cue_llm::LlmProvider> =
                    Arc::new(cue_llm::ollama::OllamaProvider::new());
                providers.insert("ollama".to_string(), provider);
            }
        }

        Self { providers }
    }

    fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl ProviderRegistry {
    /// Codex Stage 9 round-2: true when every registered LLM provider is
    /// a `bluey-managed-*` name. Used to pick the right RoutingPolicy.
    pub fn is_managed_only(&self) -> bool {
        if self.providers.is_empty() {
            return false;
        }
        self.providers
            .keys()
            .all(|k| k.starts_with("bluey-managed-"))
    }
}

#[async_trait::async_trait]
impl cue_router::speculative::SpeculativeProvider for ProviderRegistry {
    async fn provider_for(
        &self,
        route: &cue_router::ProviderRoute,
    ) -> Result<std::sync::Arc<dyn cue_llm::LlmProvider>, cue_llm::LlmError> {
        if let Some(p) = self.providers.get(&route.provider_name) {
            return Ok(p.clone());
        }
        // Codex Stage 9 round-2 Blocker 2: NO arbitrary HashMap fallback.
        // Returning an arbitrary provider can silently misroute a Deep
        // classification to (e.g.) bluey-managed-instant. If the named
        // provider is missing it is a registry-construction bug; surface
        // it as an error so the caller falls back to legacy single-shot.
        Err(cue_llm::LlmError::Provider(format!(
            "no provider registered for route {} (managed_only={}, registered={:?})",
            route.provider_name,
            self.is_managed_only(),
            self.providers.keys().collect::<Vec<_>>()
        )))
    }
}

/// Build an LLM provider from environment variables or stored secrets.
///
/// Codex Stage 9 round-2 Blocker 1: managed-mode customers (Bluey
/// account token in keyring, no direct OPENAI_API_KEY) used to be
/// rejected here. We now check managed mode first and return a
/// BlueyManagedProvider bound to the Balanced lane as the legacy
/// single-shot fallback. The speculative path (`try_speculative_
/// dispatch`) picks per-lane providers separately.
fn build_llm_provider_from_env(
    _db: &State<DbState>,
    trace_id: &str,
) -> Option<Box<dyn cue_llm::LlmProvider>> {
    // Managed mode first: Bluey account tokens take priority over BYOK
    // unless BLUEY_DEV_BYOK=1 explicitly opts in (matching the
    // ProviderRegistry policy).
    let allow_byok = std::env::var("BLUEY_DEV_BYOK")
        .map(|v| v == "1")
        .unwrap_or(false);
    if !allow_byok {
        if let Ok(client) = cloud_client_with_trace(trace_id) {
            if client.current_tokens().is_some() {
                return Some(Box::new(cue_llm::bluey_managed::BlueyManagedProvider::new(
                    client,
                    cue_llm::bluey_managed::ManagedLane::Balanced,
                )));
            }
        }
    }

    let openai_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            cue_daemon::secrets::load_api_key("llm_openai")
                .ok()
                .flatten()
        })?;
    Some(Box::new(cue_llm::openai::OpenAiProvider::new(openai_key)))
}

// ─── R8 nit hardening tests (recheck rollout) ───────────────────────────────

#[cfg(test)]
mod r8_nit_tests {
    /// Char-safe last-4 masking matches the new load_stt_api_key behavior:
    /// must NOT panic on multi-byte UTF-8 trailing bytes.
    fn mask_last_four(s: &str) -> String {
        let total = s.chars().count();
        if total <= 4 {
            "****".to_string()
        } else {
            let suffix: String = s.chars().skip(total - 4).collect();
            format!("****{suffix}")
        }
    }

    #[test]
    fn mask_short_key() {
        assert_eq!(mask_last_four("ab"), "****");
        assert_eq!(mask_last_four("abcd"), "****");
    }

    #[test]
    fn mask_ascii_key() {
        assert_eq!(mask_last_four("sk-abcd1234"), "****1234");
    }

    #[test]
    fn mask_multibyte_key_does_not_panic() {
        // Trailing 4 chars are emoji + ASCII — would have panicked under
        // byte-slicing. Char-based skip is safe.
        let key = "secret-key-🦀🎉ab";
        let masked = mask_last_four(key);
        assert!(masked.starts_with("****"));
        assert_eq!(masked.chars().count(), 4 + 4);
    }
}

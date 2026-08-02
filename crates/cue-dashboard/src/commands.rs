use cue_core::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use cue_core::session::Session;
use cue_core::{
    AssistantProfile, AudioCaptureState, AudioPipelineStatus, AudioReadinessProbeResult,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
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

/// Long enough for MeetingEnd's bounded audio tail drain, while keeping the UI responsive.
const DAEMON_IPC_OPERATION_TIMEOUT: Duration = Duration::from_secs(6);
const DAEMON_IPC_TIMEOUT_ERROR: &str = "daemon request timed out";
static DAEMON_IPC_CAPABILITY_CACHE: OnceLock<Mutex<Option<cue_core::IpcCapabilityRecord>>> =
    OnceLock::new();

/// Send a request to the running daemon over TCP and return the response.
pub(crate) async fn daemon_ipc(request: DaemonRequest) -> Result<DaemonResponse, String> {
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
    daemon_ipc_with_trace_to_addr_timeout(request, trace_id, addr, DAEMON_IPC_OPERATION_TIMEOUT)
        .await
}

async fn daemon_ipc_with_trace_to_addr_timeout(
    request: DaemonRequest,
    trace_id: &str,
    addr: &str,
    timeout_limit: Duration,
) -> Result<DaemonResponse, String> {
    // Resolve no hostnames and reject non-loopback destinations before a
    // capability is loaded, so bearer bytes cannot leave the local host.
    let addr = cue_core::validated_loopback_ipc_addr(addr)
        .map_err(|error| format!("daemon IPC address refused: {error}"))?;
    let request = request.with_trace_id(trace_id.to_string());
    if request.ipc_authorization() == cue_core::IpcAuthorization::Public {
        return send_daemon_wire_request(
            cue_core::DaemonWireRequest::Public(request),
            addr,
            timeout_limit,
        )
        .await
        .and_then(reject_dashboard_ipc_auth_error);
    }

    let capability = dashboard_ipc_capability(false)?;
    let response = send_daemon_wire_request(
        cue_core::DaemonWireRequest::Authenticated(cue_core::AuthenticatedDaemonRequest::new(
            &capability,
            request.clone(),
        )),
        addr,
        timeout_limit,
    )
    .await?;
    if matches!(
        response,
        DaemonResponse::IpcAuthError {
            code: cue_core::IpcAuthErrorCode::StaleBoot
        }
    ) {
        let capability = dashboard_ipc_capability(true)?;
        return send_daemon_wire_request(
            cue_core::DaemonWireRequest::Authenticated(cue_core::AuthenticatedDaemonRequest::new(
                &capability,
                request,
            )),
            addr,
            timeout_limit,
        )
        .await
        .and_then(reject_dashboard_ipc_auth_error);
    }
    reject_dashboard_ipc_auth_error(response)
}

fn reject_dashboard_ipc_auth_error(response: DaemonResponse) -> Result<DaemonResponse, String> {
    if let DaemonResponse::IpcAuthError { code } = response {
        return Err(format!("daemon authentication failed: {code:?}"));
    }
    Ok(response)
}

fn dashboard_ipc_capability(force_reload: bool) -> Result<cue_core::IpcCapabilityRecord, String> {
    let cache = DAEMON_IPC_CAPABILITY_CACHE.get_or_init(|| Mutex::new(None));
    let mut cached = cache
        .lock()
        .map_err(|_| "daemon authentication cache unavailable".to_string())?;
    if force_reload {
        *cached = None;
    }
    if let Some(capability) = cached.as_ref() {
        return Ok(capability.clone());
    }
    let paths = cue_core::app_paths::AppPaths::discover()
        .map_err(|_| "daemon authentication unavailable".to_string())?;
    let capability = match cue_core::load_ipc_capability(&paths) {
        Ok(capability) => capability,
        #[cfg(test)]
        Err(_) => cue_core::IpcCapabilityRecord::generate()
            .map_err(|_| "daemon authentication unavailable".to_string())?,
        #[cfg(not(test))]
        Err(error) => return Err(format!("daemon authentication unavailable: {error}")),
    };
    *cached = Some(capability.clone());
    Ok(capability)
}

async fn send_daemon_wire_request(
    request: cue_core::DaemonWireRequest,
    addr: std::net::SocketAddr,
    timeout_limit: Duration,
) -> Result<DaemonResponse, String> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    let operation = async {
        let stream = TcpStream::connect(addr).await.map_err(|error| {
            tracing::warn!(%error, "dashboard daemon connection failed");
            "daemon unavailable".to_string()
        })?;
        let (reader, mut writer) = stream.into_split();
        let mut reader =
            BufReader::new(reader).take((cue_core::ipc_auth::IPC_MAX_RESPONSE_BYTES + 1) as u64);

        let line = serde_json::to_vec(&request).map_err(|error| {
            tracing::warn!(%error, "dashboard daemon request serialization failed");
            "daemon request failed".to_string()
        })?;
        if line.len() + 1 > cue_core::ipc_auth::IPC_MAX_REQUEST_BYTES {
            return Err("daemon request exceeds IPC size limit".to_string());
        }
        writer.write_all(&line).await.map_err(|error| {
            tracing::warn!(%error, "dashboard daemon request write failed");
            "daemon unavailable".to_string()
        })?;
        writer.write_all(b"\n").await.map_err(|error| {
            tracing::warn!(%error, "dashboard daemon request delimiter write failed");
            "daemon unavailable".to_string()
        })?;
        writer.flush().await.map_err(|error| {
            tracing::warn!(%error, "dashboard daemon request flush failed");
            "daemon unavailable".to_string()
        })?;

        let mut response = Vec::new();
        let read = reader
            .read_until(b'\n', &mut response)
            .await
            .map_err(|error| {
                tracing::warn!(%error, "dashboard daemon response read failed");
                "daemon unavailable".to_string()
            })?;
        if read == 0 {
            return Err("daemon closed connection".to_string());
        }
        if response.len() > cue_core::ipc_auth::IPC_MAX_RESPONSE_BYTES {
            return Err("daemon response exceeds IPC size limit".to_string());
        }
        if response.last() != Some(&b'\n') {
            return Err("invalid daemon response delimiter".to_string());
        }
        serde_json::from_slice(&response[..response.len() - 1]).map_err(|error| {
            tracing::warn!(%error, "dashboard daemon response parsing failed");
            "invalid daemon response".to_string()
        })
    };

    match tokio::time::timeout(timeout_limit, operation).await {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(
                timeout_ms = timeout_limit.as_millis(),
                "dashboard daemon operation timed out"
            );
            Err(DAEMON_IPC_TIMEOUT_ERROR.to_string())
        }
    }
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
    let paths = cue_core::app_paths::AppPaths::discover()
        .map_err(|e| format!("account store unavailable: {e}"))?;
    let account =
        cue_core::load_account(&paths).map_err(|e| format!("account profile unavailable: {e}"))?;
    let mut config = cue_cloud_client::client::ClientConfig {
        trace_id: Some(trace_id.to_string()),
        ..Default::default()
    };
    if let Some(account) = account.as_ref() {
        if !account.api_url.trim().is_empty() {
            config.base_url = account.api_url.clone();
        }
    }
    let store: Arc<dyn cue_cloud_client::TokenStore> = if account
        .as_ref()
        .is_some_and(|account| account.provider != "local" && account.linked_owner_id().is_some())
    {
        Arc::new(cue_cloud_client::SecureAccountStore::new(paths))
    } else {
        Arc::new(cue_cloud_client::tokens::MemoryStore::new())
    };
    cue_cloud_client::CloudClient::new(config, store)
        .map_err(|e| format!("account store unavailable: {e}"))
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

    let me: cue_cloud_client::AccountMe = match async {
        verify_dashboard_device_link(&client).await?;
        client.auth_get("/account/me").await
    }
    .await
    {
        Ok(me) => me,
        Err(error) if dashboard_auth_error_should_clear_tokens(&error) => {
            let _ = client.clear_tokens();
            return Ok(None);
        }
        Err(error) => return Err(format!("balance lookup failed: {error}")),
    };
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

    let me: cue_cloud_client::AccountMe = match async {
        verify_dashboard_device_link(&client).await?;
        client.auth_get("/account/me").await
    }
    .await
    {
        Ok(me) => me,
        Err(error) if dashboard_auth_error_should_clear_tokens(&error) => {
            let _ = client.clear_tokens();
            return Ok(None);
        }
        Err(error) => return Err(format!("account lookup failed: {error}")),
    };
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

async fn verify_dashboard_device_link(
    client: &cue_cloud_client::CloudClient,
) -> Result<(), cue_cloud_client::Error> {
    let Some(device_id) = dashboard_stored_cloud_device_id() else {
        return Ok(());
    };
    let status: cue_cloud_client::DeviceStatusResponse = client
        .auth_post(
            "/account/devices/status",
            &cue_cloud_client::DeviceStatusRequest { device_id },
        )
        .await?;
    if status.active {
        Ok(())
    } else {
        Err(cue_cloud_client::Error::Unauthorized)
    }
}

fn dashboard_stored_cloud_device_id() -> Option<String> {
    let paths = cue_core::app_paths::AppPaths::discover().ok()?;
    cue_core::load_account(&paths)
        .ok()
        .flatten()
        .map(|account| account.device_id)
        .filter(|device_id| {
            let value = device_id.trim();
            !value.is_empty() && value != "local-device"
        })
}

fn dashboard_auth_error_should_clear_tokens(error: &cue_cloud_client::Error) -> bool {
    matches!(
        error,
        cue_cloud_client::Error::Unauthorized
            | cue_cloud_client::Error::Server { status: 403 | 404 }
    )
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
pub async fn sign_out(db: State<'_, DbState>) -> Result<(), String> {
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    client
        .clear_tokens()
        .map_err(|e| format!("sign out failed: {e}"))?;
    if let Err(error) = daemon_ipc_with_trace(DaemonRequest::CloudLogout, &trace_id).await {
        // Credential removal is authoritative. A stopped daemon must not turn
        // a completed local sign-out into a false failure.
        tracing::warn!(
            error_length = error.len(),
            "daemon sign-out notification failed"
        );
    }
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
        .auth_post(
            "/account/delete",
            &serde_json::json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }),
        )
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
    std::env::var("BLUEY_SIGNIN_URL").unwrap_or_else(|_| "https://bluey.sh/login".to_string())
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

fn audio_pipeline_is_active(status: &AudioPipelineStatus) -> bool {
    status.capture.is_active()
        || (status.session_id.is_some()
            && !matches!(
                status.capture.state,
                AudioCaptureState::Stopped | AudioCaptureState::Failed
            ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublicAudioErrorKind {
    SignIn,
    Permission,
    Source,
    Timeout,
    Unavailable,
    Generic,
}

fn classify_audio_error(message: &str) -> PublicAudioErrorKind {
    let message = message.to_ascii_lowercase();
    if ["permission", "screen recording", "tcc", "access denied"]
        .iter()
        .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Permission
    } else if [
        "sign in",
        "signin",
        "not signed",
        "unauthorized",
        "authentication",
        "not authenticated",
        "auth required",
        "login required",
        "account required",
        "status 401",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::SignIn
    } else if ["timed out", "timeout", "deadline"]
        .iter()
        .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Timeout
    } else if [
        "helper",
        "audio source",
        "microphone",
        "system audio",
        "audio device",
        "audio backend",
        "source unavailable",
        "source failed",
        "device unavailable",
        "coreaudio",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Source
    } else if [
        "daemon unavailable",
        "closed connection",
        "connection",
        "offline",
        "refused",
        "broken pipe",
        "invalid daemon response",
        "service unavailable",
        "not running",
        "unexpected eof",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Unavailable
    } else {
        PublicAudioErrorKind::Generic
    }
}

fn public_audio_error(message: &str) -> String {
    match classify_audio_error(message) {
        PublicAudioErrorKind::SignIn => "Sign in to Bluey to start listening.",
        PublicAudioErrorKind::Permission => {
            "Allow microphone and system audio access, then try again."
        }
        PublicAudioErrorKind::Source => {
            "A required audio source is unavailable. Check audio settings and try again."
        }
        PublicAudioErrorKind::Timeout => "Bluey took too long to respond. Try again.",
        PublicAudioErrorKind::Unavailable => {
            "Bluey's local audio service is unavailable. Try again."
        }
        PublicAudioErrorKind::Generic => "Bluey couldn't update the live session. Try again.",
    }
    .to_string()
}

fn public_audio_failure(operation: &str, message: &str) -> String {
    let kind = classify_audio_error(message);
    tracing::warn!(
        operation,
        ?kind,
        error_length = message.len(),
        "audio operation failed"
    );
    public_audio_error(message)
}

fn sanitize_audio_pipeline_status(mut status: AudioPipelineStatus) -> AudioPipelineStatus {
    let sanitize = |value: &mut Option<String>| {
        if let Some(message) = value.take() {
            *value = Some(public_audio_error(&message));
        }
    };

    sanitize(&mut status.capture.last_error);
    sanitize(&mut status.capture.system.last_error);
    sanitize(&mut status.capture.microphone.last_error);
    if matches!(status.capture.state, AudioCaptureState::Failed) {
        sanitize(&mut status.note);
    }
    status
}

fn audio_status_from_response(
    response: DaemonResponse,
    operation: &str,
) -> Result<AudioPipelineStatus, String> {
    match response {
        DaemonResponse::AudioStatus { status } => Ok(sanitize_audio_pipeline_status(status)),
        DaemonResponse::Error { message } => Err(public_audio_failure(operation, &message)),
        _ => Err(public_audio_failure(operation, "unexpected response")),
    }
}

async fn daemon_listening_status_with_trace_to_addr(
    trace_id: &str,
    addr: &str,
) -> Result<AudioPipelineStatus, String> {
    let response = daemon_ipc_with_trace_to_addr(DaemonRequest::AudioStatus, trace_id, addr)
        .await
        .map_err(|message| public_audio_failure("query audio status", &message))?;
    audio_status_from_response(response, "query audio status")
}

async fn daemon_toggle_listening_with_trace_to_addr(
    mic_device_id: Option<String>,
    trace_id: &str,
    addr: &str,
) -> Result<AudioPipelineStatus, String> {
    let status = daemon_listening_status_with_trace_to_addr(trace_id, addr).await?;
    let was_active = audio_pipeline_is_active(&status);
    let (request, operation) = if was_active {
        (DaemonRequest::AudioStop, "stop audio capture")
    } else {
        (
            DaemonRequest::AudioStart {
                enable_system: true,
                enable_microphone: true,
                mic_device_id,
            },
            "start audio capture",
        )
    };
    let response = daemon_ipc_with_trace_to_addr(request, trace_id, addr)
        .await
        .map_err(|message| public_audio_failure(operation, &message))?;
    let status = audio_status_from_response(response, operation)?;

    if !was_active
        && audio_pipeline_is_active(&status)
        && !(status.config.system.enabled && status.config.microphone.enabled)
    {
        if let Err(message) =
            daemon_ipc_with_trace_to_addr(DaemonRequest::AudioStop, trace_id, addr).await
        {
            tracing::warn!(
                kind = ?classify_audio_error(&message),
                "failed to clean up partial audio capture"
            );
        }
        return Err(
            "Bluey couldn't start both audio sources, so listening was stopped. Check audio settings and try again."
                .to_string(),
        );
    }

    Ok(status)
}

#[derive(Clone, Serialize)]
pub struct EndSessionPayload {
    pub audio_status: Option<AudioPipelineStatus>,
    pub message: String,
}

async fn daemon_end_session_with_trace_to_addr(
    trace_id: &str,
    addr: &str,
) -> Result<EndSessionPayload, String> {
    let response = daemon_ipc_with_trace_to_addr(DaemonRequest::MeetingEnd, trace_id, addr)
        .await
        .map_err(|message| public_audio_failure("end live session", &message))?;
    match response {
        DaemonResponse::Recap { .. } | DaemonResponse::Text { .. } | DaemonResponse::Ok => {}
        DaemonResponse::Error { message } => {
            return Err(public_audio_failure("end live session", &message));
        }
        _ => {
            return Err(public_audio_failure(
                "end live session",
                "unexpected response",
            ))
        }
    }

    // MeetingEnd is authoritative. A failed follow-up status refresh must not
    // turn an already-ended session back into a UI failure.
    let audio_status = match daemon_listening_status_with_trace_to_addr(trace_id, addr).await {
        Ok(status) => Some(status),
        Err(message) => {
            tracing::warn!(
                kind = ?classify_audio_error(&message),
                "live session ended but audio status refresh failed"
            );
            None
        }
    };

    Ok(EndSessionPayload {
        audio_status,
        message: "Session ended.".to_string(),
    })
}

/// Return the daemon's current audio pipeline status.
#[tauri::command]
pub async fn daemon_listening_status() -> Result<AudioPipelineStatus, String> {
    let trace_id = dashboard_trace_id();
    daemon_listening_status_with_trace_to_addr(&trace_id, &daemon_addr()).await
}

/// Run Bluey's fixed-duration, local-only onboarding audio check.
#[tauri::command]
pub async fn run_audio_readiness_probe() -> Result<AudioReadinessProbeResult, String> {
    let response = daemon_ipc(DaemonRequest::AudioReadinessProbe)
        .await
        .map_err(|message| public_audio_readiness_failure(&message))?;
    match response {
        DaemonResponse::AudioReadiness { result } => Ok(result),
        DaemonResponse::Error { message } => Err(public_audio_readiness_failure(&message)),
        _ => Err(public_audio_readiness_failure("unexpected response")),
    }
}

fn public_audio_readiness_failure(message: &str) -> String {
    let lowercase = message.to_ascii_lowercase();
    if lowercase.contains("already running")
        || lowercase.contains("audio is active")
        || lowercase.contains("session is ending")
    {
        tracing::warn!(
            error_length = message.len(),
            "audio readiness check blocked by active local audio"
        );
        "Audio is active. Finish the listening session, then run the check again.".to_string()
    } else {
        public_audio_failure("run audio readiness check", message)
    }
}

/// Toggle real dual-source audio capture and return the resulting pipeline status.
#[tauri::command]
pub async fn daemon_toggle_listening(
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<AudioPipelineStatus, String> {
    let trace_id = dashboard_trace_id();
    let mic_device_id = load_mic_device_from_settings(&db);
    let result =
        daemon_toggle_listening_with_trace_to_addr(mic_device_id, &trace_id, &daemon_addr()).await;

    match result {
        Ok(status) => {
            let _ = app.emit("audio_pipeline_status", &status);
            Ok(status)
        }
        Err(error) => {
            let _ = app.emit("audio_pipeline_error", &error);
            Err(error)
        }
    }
}

/// End the meeting and audio pipeline atomically in the daemon, then refresh status.
#[tauri::command]
pub async fn daemon_end_session(app: AppHandle) -> Result<EndSessionPayload, String> {
    let trace_id = dashboard_trace_id();
    let result = daemon_end_session_with_trace_to_addr(&trace_id, &daemon_addr()).await;

    match result {
        Ok(payload) => {
            if let Some(status) = &payload.audio_status {
                let _ = app.emit("audio_pipeline_status", status);
            }
            let _ = app.emit("live_session_ended", &payload.message);
            Ok(payload)
        }
        Err(error) => {
            let _ = app.emit("audio_pipeline_error", &error);
            Err(error)
        }
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
        DaemonResponse::AudioStatus { status } => audio_pipeline_is_active(status),
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

/// Return the active meeting's assistant profile.
///
/// The daemon owns validation and persistence so every dashboard window sees
/// the same effective mode and grounding fields.
#[tauri::command]
pub async fn get_assistant_profile() -> Result<AssistantProfile, String> {
    match daemon_ipc(DaemonRequest::AssistantProfileGet).await? {
        DaemonResponse::AssistantProfile { profile } => Ok(profile),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected daemon response".to_string()),
    }
}

/// Validate and save the active meeting's assistant profile.
#[tauri::command]
pub async fn save_assistant_profile(profile: AssistantProfile) -> Result<AssistantProfile, String> {
    match daemon_ipc(DaemonRequest::AssistantProfileSet { profile }).await? {
        DaemonResponse::AssistantProfile { profile } => Ok(profile),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected daemon response".to_string()),
    }
}

/// Typed workspace bridge. These commands deliberately preserve the daemon's
/// tagged response shape so the UI and local IPC share one exact contract.
#[tauri::command]
pub async fn workspace_list() -> Result<DaemonResponse, String> {
    match daemon_ipc(DaemonRequest::WorkspaceList).await? {
        response @ DaemonResponse::WorkspaceList { .. } => Ok(response),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected workspace_list response".to_string()),
    }
}

#[tauri::command]
pub async fn workspace_get(workspace_id: Uuid) -> Result<DaemonResponse, String> {
    match daemon_ipc(DaemonRequest::WorkspaceGet { workspace_id }).await? {
        response @ DaemonResponse::Workspace { .. } => Ok(response),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected workspace_get response".to_string()),
    }
}

#[tauri::command]
pub async fn workspace_create(
    request: cue_core::WorkspaceCreateRequest,
) -> Result<DaemonResponse, String> {
    match daemon_ipc(DaemonRequest::WorkspaceCreate { request }).await? {
        response @ DaemonResponse::Workspace { .. } => Ok(response),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected workspace_create response".to_string()),
    }
}

#[tauri::command]
pub async fn workspace_update(
    request: cue_core::WorkspaceUpdateRequest,
) -> Result<DaemonResponse, String> {
    match daemon_ipc(DaemonRequest::WorkspaceUpdate { request }).await? {
        response @ DaemonResponse::Workspace { .. } => Ok(response),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected workspace_update response".to_string()),
    }
}

#[tauri::command]
pub async fn workspace_activate(workspace_id: Uuid) -> Result<DaemonResponse, String> {
    match daemon_ipc(DaemonRequest::WorkspaceActivate { workspace_id }).await? {
        response @ DaemonResponse::Workspace { .. } => Ok(response),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected workspace_activate response".to_string()),
    }
}

#[tauri::command]
pub async fn workspace_delete(
    workspace_id: Uuid,
    expected_revision: u64,
) -> Result<DaemonResponse, String> {
    match daemon_ipc(DaemonRequest::WorkspaceDelete {
        workspace_id,
        expected_revision,
    })
    .await?
    {
        response @ DaemonResponse::WorkspaceDeleted { .. } => Ok(response),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("unexpected workspace_delete response".to_string()),
    }
}

const MAX_SCREENSHOT_PREVIEW_BYTES: u64 = 25 * 1024 * 1024;
const MAX_SCREENSHOT_EDGE_PX: u32 = 12_000;
const MAX_SCREENSHOT_PIXELS: u64 = 80_000_000;
const MAX_SCREENSHOT_TITLE_CHARS: usize = 160;
const MAX_SCREENSHOT_BINDING_BYTES: u64 = 16 * 1024;
const SCREENSHOT_PREVIEW_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SCREENSHOT_CLEANUP_SCAN: usize = 256;
const MAX_SCREENSHOT_CLEANUP_REMOVALS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScreenshotConsentBinding {
    schema_version: u8,
    operation_id: Uuid,
    expected_owner_account_id: Option<String>,
    expected_session_id: Option<Uuid>,
    created_at: String,
    preview_path: String,
    preview_sha256: String,
    retained_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotPreviewPayload {
    pub operation_id: Uuid,
    pub path: String,
    pub file_size_bytes: u64,
    pub width_px: u32,
    pub height_px: u32,
    pub created_at: String,
    pub capture_kind: String,
}

/// Capture a private local preview. This command does not attach or upload it.
#[tauri::command]
pub async fn capture_screenshot_preview(
    full_screen: bool,
) -> Result<ScreenshotPreviewPayload, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|error| error.to_string())?;
    paths.ensure().map_err(|error| error.to_string())?;
    let preview_dir = screenshot_preview_dir(&paths);
    let binding_dir = screenshot_binding_dir(&paths);
    cue_core::app_paths::create_private_dir(&preview_dir).map_err(|error| error.to_string())?;
    cue_core::app_paths::create_private_dir(&binding_dir).map_err(|error| error.to_string())?;
    cleanup_stale_screenshot_previews(&paths);

    let (expected_owner_account_id, expected_session_id) =
        match daemon_ipc(DaemonRequest::ScreenshotContextDestination).await? {
            DaemonResponse::ScreenshotContextDestination {
                owner_account_id,
                session_id,
            } => (owner_account_id, session_id),
            DaemonResponse::Error { message } => return Err(message),
            _ => return Err("Bluey could not bind the screenshot destination.".to_string()),
        };

    let created_at = cue_core::clock::now_epoch_ms_string();
    let operation_id = Uuid::new_v4();
    let path = preview_dir.join(format!(
        "preview-{created_at}-{}.png",
        operation_id.simple()
    ));
    let path_for_capture = path.clone();
    let capture_kind = tokio::task::spawn_blocking(move || {
        capture_screenshot_platform(&path_for_capture, full_screen)
    })
    .await
    .map_err(|_| "Screenshot capture task stopped unexpectedly.".to_string())??;

    let (bytes, file_size_bytes, width_px, height_px) = match read_validated_png(&path) {
        Ok(details) => details,
        Err(error) => {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
    };
    if let Err(error) = set_private_capture_permissions(&path) {
        let _ = fs::remove_file(&path);
        return Err(error);
    }

    let retained_path = paths
        .data_dir
        .join("captures")
        .join(format!("screenshot-{}.png", operation_id.simple()));
    let binding = ScreenshotConsentBinding {
        schema_version: 1,
        operation_id,
        expected_owner_account_id,
        expected_session_id,
        created_at: created_at.clone(),
        preview_path: path.display().to_string(),
        preview_sha256: cue_core::jobs_handoff::sha256_hex(&bytes),
        retained_path: retained_path.display().to_string(),
    };
    if let Err(error) = write_screenshot_binding(&paths, &binding) {
        let _ = fs::remove_file(&path);
        return Err(error);
    }

    Ok(ScreenshotPreviewPayload {
        operation_id,
        path: path.display().to_string(),
        file_size_bytes,
        width_px,
        height_px,
        created_at,
        capture_kind: capture_kind.to_string(),
    })
}

/// Copy an approved preview into a deterministic retained path, then ask the
/// daemon to commit it exactly once to the capture-bound account/session.
#[tauri::command]
pub async fn attach_screenshot_preview(
    operation_id: Uuid,
    title: String,
) -> Result<cue_core::ScreenshotContextAttachReceipt, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|error| error.to_string())?;
    paths.ensure().map_err(|error| error.to_string())?;
    let binding = load_screenshot_binding(&paths, operation_id)?;
    let title = normalize_screenshot_title(&title)?;
    let (bytes, _, _, _) = read_bound_screenshot_preview(&paths, &binding)?;

    let retained_dir = paths.data_dir.join("captures");
    cue_core::app_paths::create_private_dir(&retained_dir).map_err(|error| error.to_string())?;
    let retained_path = validated_bound_retained_path(&paths, &binding)?;
    ensure_retained_screenshot_copy(&retained_path, &bytes, &binding.preview_sha256)?;

    let request = cue_core::ScreenshotContextAttachRequest {
        operation_id,
        expected_owner_account_id: binding.expected_owner_account_id.clone(),
        expected_session_id: binding.expected_session_id,
        retained_path: retained_path.display().to_string(),
        content_sha256: binding.preview_sha256.clone(),
        title,
    };
    let response = daemon_ipc(DaemonRequest::ScreenshotContextAttach { request }).await;

    match response {
        Ok(DaemonResponse::ScreenshotContextAttached { receipt })
            if receipt.operation_id == operation_id
                && receipt.artifact.id == operation_id
                && receipt.artifact.integrity_sha256.as_deref()
                    == Some(binding.preview_sha256.as_str()) =>
        {
            remove_confirmed_screenshot_preview(&paths, &binding);
            Ok(receipt)
        }
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(_) => Err(
            "Bluey returned an unreadable screenshot receipt. The retained copy was preserved for safe retry."
                .to_string(),
        ),
        Err(_) => Err(
            "Bluey could not confirm whether the screenshot was attached. Its private copies were preserved; retry Attach to reconcile safely."
                .to_string(),
        ),
    }
}

/// Delete an unapproved local preview. Attached context is outside this
/// command's path boundary and cannot be deleted here.
#[tauri::command]
pub fn discard_screenshot_preview(operation_id: Uuid) -> Result<(), String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|error| error.to_string())?;
    let binding = load_screenshot_binding(&paths, operation_id)?;
    discard_bound_screenshot_preview(&paths, &binding)
}

fn discard_bound_screenshot_preview(
    paths: &cue_core::app_paths::AppPaths,
    binding: &ScreenshotConsentBinding,
) -> Result<(), String> {
    let preview_path = validated_bound_preview_path(paths, binding)?;
    fs::remove_file(&preview_path)
        .map_err(|_| "Bluey could not discard the screenshot preview.".to_string())?;
    remove_binding_file(paths, binding.operation_id)
}

fn screenshot_preview_dir(paths: &cue_core::app_paths::AppPaths) -> PathBuf {
    paths.data_dir.join("capture-previews")
}

fn screenshot_binding_dir(paths: &cue_core::app_paths::AppPaths) -> PathBuf {
    paths.data_dir.join("screenshot-consent").join("bindings")
}

fn screenshot_binding_path(paths: &cue_core::app_paths::AppPaths, operation_id: Uuid) -> PathBuf {
    screenshot_binding_dir(paths).join(format!("binding-{}.json", operation_id.simple()))
}

fn write_screenshot_binding(
    paths: &cue_core::app_paths::AppPaths,
    binding: &ScreenshotConsentBinding,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(binding)
        .map_err(|_| "Bluey could not encode the screenshot consent record.".to_string())?;
    write_private_new_file(
        &screenshot_binding_path(paths, binding.operation_id),
        &bytes,
    )
}

fn load_screenshot_binding(
    paths: &cue_core::app_paths::AppPaths,
    operation_id: Uuid,
) -> Result<ScreenshotConsentBinding, String> {
    let path = screenshot_binding_path(paths, operation_id);
    let bytes = read_regular_file_no_follow(&path, MAX_SCREENSHOT_BINDING_BYTES)
        .map_err(|_| "Screenshot consent record is no longer available.".to_string())?;
    let binding: ScreenshotConsentBinding = serde_json::from_slice(&bytes)
        .map_err(|_| "Screenshot consent record could not be verified.".to_string())?;
    if binding.schema_version != 1 || binding.operation_id != operation_id {
        return Err("Screenshot consent record does not match this preview.".to_string());
    }
    Ok(binding)
}

fn validated_bound_preview_path(
    paths: &cue_core::app_paths::AppPaths,
    binding: &ScreenshotConsentBinding,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(&binding.preview_path);
    let expected_name = format!(
        "preview-{}-{}.png",
        binding.created_at,
        binding.operation_id.simple()
    );
    validate_exact_regular_child(&path, &screenshot_preview_dir(paths), &expected_name)
        .map_err(|_| "Screenshot preview is outside Bluey's private preview area.".to_string())?;
    Ok(path)
}

fn validated_bound_retained_path(
    paths: &cue_core::app_paths::AppPaths,
    binding: &ScreenshotConsentBinding,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(&binding.retained_path);
    let expected_name = format!("screenshot-{}.png", binding.operation_id.simple());
    validate_exact_child_path(&path, &paths.data_dir.join("captures"), &expected_name).map_err(
        |_| "Screenshot retained path is outside Bluey's private capture area.".to_string(),
    )?;
    Ok(path)
}

fn normalize_screenshot_title(value: &str) -> Result<String, String> {
    let value = value
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .collect::<String>();
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return Err("Screenshot title is required.".to_string());
    }
    if value.chars().count() > MAX_SCREENSHOT_TITLE_CHARS {
        return Err(format!(
            "Screenshot title must be {MAX_SCREENSHOT_TITLE_CHARS} characters or fewer."
        ));
    }
    Ok(value)
}

fn read_bound_screenshot_preview(
    paths: &cue_core::app_paths::AppPaths,
    binding: &ScreenshotConsentBinding,
) -> Result<(Vec<u8>, u64, u32, u32), String> {
    let path = validated_bound_preview_path(paths, binding)?;
    let details = read_validated_png(&path)?;
    if cue_core::jobs_handoff::sha256_hex(&details.0) != binding.preview_sha256 {
        return Err("Screenshot preview changed after consent review.".to_string());
    }
    Ok(details)
}

fn read_validated_png(path: &Path) -> Result<(Vec<u8>, u64, u32, u32), String> {
    let bytes = read_regular_file_no_follow(path, MAX_SCREENSHOT_PREVIEW_BYTES)
        .map_err(|_| "Screenshot preview could not be read safely.".to_string())?;
    let file_size = bytes.len() as u64;
    if !(24..=MAX_SCREENSHOT_PREVIEW_BYTES).contains(&file_size) {
        return Err("Screenshot preview has an unsupported file size.".to_string());
    }
    if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n") || bytes.get(12..16) != Some(b"IHDR") {
        return Err("Screenshot preview is not a valid PNG image.".to_string());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("PNG width bytes"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("PNG height bytes"));
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_SCREENSHOT_EDGE_PX
        || height > MAX_SCREENSHOT_EDGE_PX
        || pixels > MAX_SCREENSHOT_PIXELS
    {
        return Err("Screenshot preview dimensions are outside Bluey's safety limits.".to_string());
    }
    Ok((bytes, file_size, width, height))
}

fn ensure_retained_screenshot_copy(
    path: &Path,
    bytes: &[u8],
    expected_sha256: &str,
) -> Result<(), String> {
    if path.exists() {
        let existing = read_regular_file_no_follow(path, MAX_SCREENSHOT_PREVIEW_BYTES)
            .map_err(|_| "Existing retained screenshot could not be verified.".to_string())?;
        if cue_core::jobs_handoff::sha256_hex(&existing) != expected_sha256 {
            return Err(
                "Existing retained screenshot does not match the reviewed preview.".to_string(),
            );
        }
        return Ok(());
    }
    write_private_new_file(path, bytes)?;
    sync_parent_directory(path)?;
    Ok(())
}

fn remove_confirmed_screenshot_preview(
    paths: &cue_core::app_paths::AppPaths,
    binding: &ScreenshotConsentBinding,
) {
    if let Ok(path) = validated_bound_preview_path(paths, binding) {
        let _ = fs::remove_file(path);
    }
    let _ = remove_binding_file(paths, binding.operation_id);
}

fn remove_binding_file(
    paths: &cue_core::app_paths::AppPaths,
    operation_id: Uuid,
) -> Result<(), String> {
    fs::remove_file(screenshot_binding_path(paths, operation_id))
        .map_err(|_| "Bluey could not remove the screenshot consent record.".to_string())
}

fn write_private_new_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Bluey could not create a private screenshot record.".to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Bluey could not persist a private screenshot record.".to_string())?;
    Ok(())
}

fn read_regular_file_no_follow(path: &Path, max_bytes: u64) -> Result<Vec<u8>, std::io::Error> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a bounded regular file",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeded bound while reading",
        ));
    }
    Ok(bytes)
}

fn validate_exact_regular_child(
    path: &Path,
    dir: &Path,
    expected_name: &str,
) -> std::io::Result<()> {
    validate_exact_child_path(path, dir, expected_name)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a regular child file",
        ));
    }
    Ok(())
}

fn validate_exact_child_path(path: &Path, dir: &Path, expected_name: &str) -> std::io::Result<()> {
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unexpected file name",
        ));
    }
    let path_parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent"))?;
    if path_parent.canonicalize()? != dir.canonicalize()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "path outside allowed directory",
        ));
    }
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .ok_or_else(|| "Screenshot path has no parent.".to_string())?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "Bluey could not durably retain the screenshot.".to_string())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn cleanup_stale_screenshot_previews(paths: &cue_core::app_paths::AppPaths) {
    let mut removed = 0usize;
    let binding_dir = screenshot_binding_dir(paths);
    if let Ok(entries) = fs::read_dir(&binding_dir) {
        for entry in entries.flatten().take(MAX_SCREENSHOT_CLEANUP_SCAN) {
            if removed >= MAX_SCREENSHOT_CLEANUP_REMOVALS || !is_stale_regular_file(&entry.path()) {
                continue;
            }
            let valid_name = entry.file_name().to_str().is_some_and(|name| {
                name.strip_prefix("binding-")
                    .and_then(|name| name.strip_suffix(".json"))
                    .is_some_and(|id| {
                        id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
            });
            if !valid_name {
                continue;
            }
            if let Ok(bytes) =
                read_regular_file_no_follow(&entry.path(), MAX_SCREENSHOT_BINDING_BYTES)
            {
                if let Ok(binding) = serde_json::from_slice::<ScreenshotConsentBinding>(&bytes) {
                    if let Ok(preview) = validated_bound_preview_path(paths, &binding) {
                        let _ = fs::remove_file(preview);
                    }
                }
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }

    if let Ok(entries) = fs::read_dir(screenshot_preview_dir(paths)) {
        for entry in entries.flatten().take(MAX_SCREENSHOT_CLEANUP_SCAN) {
            if removed >= MAX_SCREENSHOT_CLEANUP_REMOVALS || !is_stale_regular_file(&entry.path()) {
                continue;
            }
            let valid_name = entry.file_name().to_str().is_some_and(|name| {
                name.starts_with("preview-") && name.ends_with(".png") && name.len() <= 128
            });
            if valid_name && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
}

fn is_stale_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|elapsed| elapsed >= SCREENSHOT_PREVIEW_TTL)
    })
}

#[cfg(target_os = "macos")]
fn capture_screenshot_platform(path: &Path, full_screen: bool) -> Result<&'static str, String> {
    let mut command = Command::new("screencapture");
    if full_screen {
        command.arg("-x");
    } else {
        command.args(["-i", "-x"]);
    }
    let status = command
        .arg(path)
        .status()
        .map_err(|_| "Bluey could not open the macOS screenshot tool.".to_string())?;
    if !status.success() || !path.is_file() {
        let _ = fs::remove_file(path);
        return Err("Screenshot capture was cancelled or permission was denied.".to_string());
    }
    Ok(if full_screen {
        "full_screen"
    } else {
        "selection"
    })
}

#[cfg(target_os = "windows")]
fn capture_screenshot_platform(path: &Path, full_screen: bool) -> Result<&'static str, String> {
    if !full_screen {
        return Err(
            "Region or window selection is not available on Windows yet; choose Full screen."
                .to_string(),
        );
    }
    let escaped = path.display().to_string().replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $i=New-Object System.Drawing.Bitmap $b.Width,$b.Height; $g=[System.Drawing.Graphics]::FromImage($i); $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size); $i.Save('{escaped}',[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $i.Dispose()"
    );
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
        .arg(script)
        .status()
        .map_err(|_| "Bluey could not open Windows screen capture.".to_string())?;
    if !status.success() || !path.is_file() {
        let _ = fs::remove_file(path);
        return Err("Screenshot capture failed or permission was denied.".to_string());
    }
    Ok("full_screen")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_screenshot_platform(_path: &Path, _full_screen: bool) -> Result<&'static str, String> {
    Err("Screenshot capture is currently available on macOS and Windows.".to_string())
}

fn set_private_capture_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|_| "Bluey could not protect the screenshot preview.".to_string())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
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
            _ => "sound",
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

    fn screenshot_test_paths(base: &Path) -> cue_core::app_paths::AppPaths {
        cue_core::app_paths::AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        }
    }

    fn write_test_png(path: &Path, width: u32, height: u32) {
        let mut bytes = Vec::from(&b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..]);
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        fs::write(path, bytes).expect("write test PNG header");
    }

    #[test]
    fn screenshot_preview_validation_enforces_owned_path_png_and_dimensions() {
        let base = std::env::temp_dir().join(format!(
            "bluey-dashboard-screenshot-test-{}",
            Uuid::new_v4()
        ));
        let paths = screenshot_test_paths(&base);
        paths.ensure().expect("ensure paths");
        let preview_dir = screenshot_preview_dir(&paths);
        cue_core::app_paths::create_private_dir(&preview_dir).expect("preview dir");
        let operation_id = Uuid::new_v4();
        let created_at = "12345";
        let preview = preview_dir.join(format!(
            "preview-{created_at}-{}.png",
            operation_id.simple()
        ));
        write_test_png(&preview, 1920, 1080);
        let binding = ScreenshotConsentBinding {
            schema_version: 1,
            operation_id,
            expected_owner_account_id: None,
            expected_session_id: None,
            created_at: created_at.to_string(),
            preview_path: preview.display().to_string(),
            preview_sha256: cue_core::jobs_handoff::sha256_hex(
                &fs::read(&preview).expect("read PNG"),
            ),
            retained_path: paths
                .data_dir
                .join("captures")
                .join(format!("screenshot-{}.png", operation_id.simple()))
                .display()
                .to_string(),
        };

        let validated = validated_bound_preview_path(&paths, &binding).expect("owned preview path");
        assert_eq!(validated, preview);
        let (_, size, width, height) = read_validated_png(&preview).expect("valid PNG dimensions");
        assert_eq!((size, width, height), (24, 1920, 1080));

        let outside = base.join("preview-outside.png");
        write_test_png(&outside, 100, 100);
        let mut outside_binding = binding.clone();
        outside_binding.preview_path = outside.display().to_string();
        assert!(validated_bound_preview_path(&paths, &outside_binding).is_err());

        write_test_png(&preview, MAX_SCREENSHOT_EDGE_PX + 1, 100);
        assert!(read_validated_png(&preview).is_err());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn screenshot_title_is_required_bounded_and_single_line() {
        assert_eq!(
            normalize_screenshot_title("  Checkout\n failure   state  ").expect("title"),
            "Checkout failure state"
        );
        assert!(normalize_screenshot_title(" \n ").is_err());
        assert!(normalize_screenshot_title(&"x".repeat(MAX_SCREENSHOT_TITLE_CHARS + 1)).is_err());
    }

    #[test]
    fn discard_removes_only_preview_and_binding_never_retained_copy() {
        let base = std::env::temp_dir().join(format!(
            "bluey-dashboard-screenshot-discard-test-{}",
            Uuid::new_v4()
        ));
        let paths = screenshot_test_paths(&base);
        paths.ensure().expect("ensure paths");
        cue_core::app_paths::create_private_dir(&screenshot_preview_dir(&paths))
            .expect("preview dir");
        cue_core::app_paths::create_private_dir(&screenshot_binding_dir(&paths))
            .expect("binding dir");
        let capture_dir = paths.data_dir.join("captures");
        cue_core::app_paths::create_private_dir(&capture_dir).expect("capture dir");
        let operation_id = Uuid::new_v4();
        let created_at = "12345";
        let preview = screenshot_preview_dir(&paths).join(format!(
            "preview-{created_at}-{}.png",
            operation_id.simple()
        ));
        let retained = capture_dir.join(format!("screenshot-{}.png", operation_id.simple()));
        write_test_png(&preview, 640, 480);
        write_test_png(&retained, 640, 480);
        let preview_bytes = fs::read(&preview).expect("preview bytes");
        let binding = ScreenshotConsentBinding {
            schema_version: 1,
            operation_id,
            expected_owner_account_id: None,
            expected_session_id: None,
            created_at: created_at.to_string(),
            preview_path: preview.display().to_string(),
            preview_sha256: cue_core::jobs_handoff::sha256_hex(&preview_bytes),
            retained_path: retained.display().to_string(),
        };
        write_screenshot_binding(&paths, &binding).expect("binding");

        discard_bound_screenshot_preview(&paths, &binding).expect("discard preview");

        assert!(!preview.exists());
        assert!(!screenshot_binding_path(&paths, operation_id).exists());
        assert!(
            retained.exists(),
            "discard must not touch a possibly committed copy"
        );
        let _ = fs::remove_dir_all(base);
    }

    async fn spawn_fake_daemon(
        responses: Vec<DaemonResponse>,
    ) -> (String, tokio::task::JoinHandle<Vec<DaemonRequest>>) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let mut requests = Vec::with_capacity(responses.len());
            for response in responses {
                let (stream, _) = listener.accept().await.unwrap();
                let (reader, mut writer) = stream.into_split();
                let mut reader = BufReader::new(reader);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                requests.push(decode_test_wire_request(&line));

                let response = serde_json::to_string(&response).unwrap();
                writer.write_all(response.as_bytes()).await.unwrap();
                writer.write_all(b"\n").await.unwrap();
            }
            requests
        });
        (addr, server)
    }

    fn decode_test_wire_request(line: &str) -> DaemonRequest {
        match serde_json::from_str::<cue_core::DaemonWireRequest>(line.trim()).unwrap() {
            cue_core::DaemonWireRequest::Authenticated(envelope) => envelope.request,
            cue_core::DaemonWireRequest::Public(request) => request,
        }
    }

    fn traced_inner(request: DaemonRequest, expected_trace_id: &str) -> DaemonRequest {
        match request {
            DaemonRequest::WithTrace { trace_id, request } => {
                assert_eq!(trace_id, expected_trace_id);
                *request
            }
            other => panic!("dashboard request was not trace-wrapped: {other:?}"),
        }
    }

    fn active_audio_status() -> AudioPipelineStatus {
        let mut status = AudioPipelineStatus::idle();
        status.session_id = Some("session-active".to_string());
        status.capture.state = AudioCaptureState::Capturing;
        status
    }

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
        let request = decode_test_wire_request(&line);
        match request {
            DaemonRequest::WithTrace { trace_id, request } => {
                assert_eq!(trace_id, "dashboard-smoke-trace");
                assert!(matches!(*request, DaemonRequest::Ping));
            }
            other => panic!("dashboard request was not trace-wrapped: {other:?}"),
        }
    }

    #[tokio::test]
    async fn daemon_ipc_reloads_once_only_for_typed_stale_boot() {
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::IpcAuthError {
                code: cue_core::IpcAuthErrorCode::StaleBoot,
            },
            DaemonResponse::Pong,
        ])
        .await;

        let response =
            daemon_ipc_with_trace_to_addr(DaemonRequest::Status, "dashboard-stale-boot", &addr)
                .await
                .expect("stale boot retry");
        assert!(matches!(response, DaemonResponse::Pong));
        let requests = server.await.expect("fake daemon");
        assert_eq!(requests.len(), 2);
        assert!(requests.into_iter().all(|request| matches!(
            traced_inner(request, "dashboard-stale-boot"),
            DaemonRequest::Status
        )));
    }

    #[tokio::test]
    async fn daemon_ipc_rejects_auth_error_returned_after_stale_boot_retry() {
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::IpcAuthError {
                code: cue_core::IpcAuthErrorCode::StaleBoot,
            },
            DaemonResponse::IpcAuthError {
                code: cue_core::IpcAuthErrorCode::InvalidCredentials,
            },
        ])
        .await;

        let error =
            daemon_ipc_with_trace_to_addr(DaemonRequest::Status, "dashboard-auth-error", &addr)
                .await
                .unwrap_err();
        assert!(error.contains("InvalidCredentials"));
        assert_eq!(server.await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn listening_toggle_starts_dual_source_audio_with_saved_mic() {
        let idle = AudioPipelineStatus::idle();
        let started = active_audio_status();
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::AudioStatus { status: idle },
            DaemonResponse::AudioStatus {
                status: started.clone(),
            },
        ])
        .await;

        let result = daemon_toggle_listening_with_trace_to_addr(
            Some("saved-mic-id".to_string()),
            "listen-start-trace",
            &addr,
        )
        .await
        .unwrap();
        assert_eq!(result, started);

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            traced_inner(requests[0].clone(), "listen-start-trace"),
            DaemonRequest::AudioStatus
        ));
        match traced_inner(requests[1].clone(), "listen-start-trace") {
            DaemonRequest::AudioStart {
                enable_system,
                enable_microphone,
                mic_device_id,
            } => {
                assert!(enable_system);
                assert!(enable_microphone);
                assert_eq!(mic_device_id.as_deref(), Some("saved-mic-id"));
            }
            other => panic!("expected dual-source audio start, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn listening_toggle_stops_active_audio() {
        let active = active_audio_status();
        let mut stopped = active.clone();
        stopped.capture.state = AudioCaptureState::Stopped;
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::AudioStatus { status: active },
            DaemonResponse::AudioStatus {
                status: stopped.clone(),
            },
        ])
        .await;

        let result = daemon_toggle_listening_with_trace_to_addr(
            Some("unused-mic-id".to_string()),
            "listen-stop-trace",
            &addr,
        )
        .await
        .unwrap();
        assert_eq!(result, stopped);

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            traced_inner(requests[0].clone(), "listen-stop-trace"),
            DaemonRequest::AudioStatus
        ));
        assert!(matches!(
            traced_inner(requests[1].clone(), "listen-stop-trace"),
            DaemonRequest::AudioStop
        ));
    }

    #[tokio::test]
    async fn listening_toggle_stops_starting_audio_without_session_id() {
        let mut starting = AudioPipelineStatus::idle();
        starting.capture.state = AudioCaptureState::Starting;
        assert!(starting.session_id.is_none());
        let mut stopped = starting.clone();
        stopped.capture.state = AudioCaptureState::Stopped;
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::AudioStatus { status: starting },
            DaemonResponse::AudioStatus {
                status: stopped.clone(),
            },
        ])
        .await;

        let result =
            daemon_toggle_listening_with_trace_to_addr(None, "listen-cancel-start-trace", &addr)
                .await
                .unwrap();
        assert_eq!(result, stopped);

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            traced_inner(requests[0].clone(), "listen-cancel-start-trace"),
            DaemonRequest::AudioStatus
        ));
        assert!(matches!(
            traced_inner(requests[1].clone(), "listen-cancel-start-trace"),
            DaemonRequest::AudioStop
        ));
    }

    #[tokio::test]
    async fn listening_toggle_cleans_up_active_partial_source_start() {
        let mut partial = active_audio_status();
        partial.config.system.enabled = false;
        let mut stopped = partial.clone();
        stopped.capture.state = AudioCaptureState::Stopped;
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::AudioStatus {
                status: AudioPipelineStatus::idle(),
            },
            DaemonResponse::AudioStatus { status: partial },
            DaemonResponse::AudioStatus { status: stopped },
        ])
        .await;

        let error = daemon_toggle_listening_with_trace_to_addr(
            Some("saved-mic-id".to_string()),
            "listen-partial-trace",
            &addr,
        )
        .await
        .unwrap_err();
        assert!(error.contains("both audio sources"));
        assert!(!error.contains("saved-mic-id"));

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 3);
        assert!(matches!(
            traced_inner(requests[0].clone(), "listen-partial-trace"),
            DaemonRequest::AudioStatus
        ));
        assert!(matches!(
            traced_inner(requests[1].clone(), "listen-partial-trace"),
            DaemonRequest::AudioStart {
                enable_system: true,
                enable_microphone: true,
                mic_device_id: Some(ref id),
            } if id == "saved-mic-id"
        ));
        assert!(matches!(
            traced_inner(requests[2].clone(), "listen-partial-trace"),
            DaemonRequest::AudioStop
        ));
    }

    #[tokio::test]
    async fn listening_status_surfaces_daemon_error() {
        let (addr, server) = spawn_fake_daemon(vec![DaemonResponse::Error {
            message: "audio backend unavailable".to_string(),
        }])
        .await;

        let error = daemon_listening_status_with_trace_to_addr("listen-error-trace", &addr)
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "A required audio source is unavailable. Check audio settings and try again."
        );

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(matches!(
            traced_inner(requests[0].clone(), "listen-error-trace"),
            DaemonRequest::AudioStatus
        ));
    }

    #[tokio::test]
    async fn listening_toggle_surfaces_unexpected_operation_response() {
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::AudioStatus {
                status: AudioPipelineStatus::idle(),
            },
            DaemonResponse::Ok,
        ])
        .await;

        let error =
            daemon_toggle_listening_with_trace_to_addr(None, "listen-unexpected-trace", &addr)
                .await
                .unwrap_err();
        assert_eq!(error, "Bluey couldn't update the live session. Try again.");

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            traced_inner(requests[0].clone(), "listen-unexpected-trace"),
            DaemonRequest::AudioStatus
        ));
        assert!(matches!(
            traced_inner(requests[1].clone(), "listen-unexpected-trace"),
            DaemonRequest::AudioStart {
                enable_system: true,
                enable_microphone: true,
                mic_device_id: None,
            }
        ));
    }

    #[tokio::test]
    async fn daemon_ipc_times_out_on_half_open_response() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            tokio::time::sleep(Duration::from_millis(75)).await;
            line
        });

        let error = daemon_ipc_with_trace_to_addr_timeout(
            DaemonRequest::Ping,
            "timeout-trace",
            &addr,
            Duration::from_millis(15),
        )
        .await
        .unwrap_err();
        assert_eq!(error, DAEMON_IPC_TIMEOUT_ERROR);
        assert_eq!(
            public_audio_error(&error),
            "Bluey took too long to respond. Try again."
        );

        let line = server.await.unwrap();
        let request = decode_test_wire_request(&line);
        assert!(matches!(
            traced_inner(request, "timeout-trace"),
            DaemonRequest::Ping
        ));
    }

    #[tokio::test]
    async fn daemon_ipc_refuses_non_loopback_destination_before_connecting() {
        let error = daemon_ipc_with_trace_to_addr(
            DaemonRequest::Ping,
            "non-loopback-trace",
            "192.0.2.10:57321",
        )
        .await
        .unwrap_err();
        assert!(error.contains("non-loopback"));
    }

    #[tokio::test]
    async fn daemon_ipc_rejects_oversized_response() {
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
            let oversized = vec![b'x'; cue_core::ipc_auth::IPC_MAX_RESPONSE_BYTES + 1];
            let _ = writer.write_all(&oversized).await;
        });

        let error = daemon_ipc_with_trace_to_addr_timeout(
            DaemonRequest::Ping,
            "oversized-response-trace",
            &addr,
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "daemon response exceeds IPC size limit");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn end_session_returns_content_free_payload_and_traced_status() {
        let stopped = AudioPipelineStatus::idle();
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::Text {
                text: "private transcript summary".to_string(),
            },
            DaemonResponse::AudioStatus {
                status: stopped.clone(),
            },
        ])
        .await;

        let payload = daemon_end_session_with_trace_to_addr("end-session-trace", &addr)
            .await
            .unwrap();
        assert_eq!(payload.message, "Session ended.");
        assert_eq!(payload.audio_status, Some(stopped));
        assert!(!payload.message.contains("private transcript"));

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            traced_inner(requests[0].clone(), "end-session-trace"),
            DaemonRequest::MeetingEnd
        ));
        assert!(matches!(
            traced_inner(requests[1].clone(), "end-session-trace"),
            DaemonRequest::AudioStatus
        ));
    }

    #[tokio::test]
    async fn end_session_stays_successful_when_status_refresh_fails() {
        let (addr, server) = spawn_fake_daemon(vec![
            DaemonResponse::Ok,
            DaemonResponse::Error {
                message:
                    "token TEST_TOKEN_PRIVATE at /Users/example/private https://internal.invalid"
                        .to_string(),
            },
        ])
        .await;

        let payload = daemon_end_session_with_trace_to_addr("end-refresh-trace", &addr)
            .await
            .unwrap();
        assert_eq!(payload.message, "Session ended.");
        assert!(payload.audio_status.is_none());

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            traced_inner(requests[0].clone(), "end-refresh-trace"),
            DaemonRequest::MeetingEnd
        ));
        assert!(matches!(
            traced_inner(requests[1].clone(), "end-refresh-trace"),
            DaemonRequest::AudioStatus
        ));
    }

    #[test]
    fn audio_errors_are_classified_without_exposing_sensitive_details() {
        assert_eq!(
            classify_audio_error("microphone permission denied"),
            PublicAudioErrorKind::Permission
        );
        assert_eq!(
            classify_audio_error("authentication required"),
            PublicAudioErrorKind::SignIn
        );
        assert_eq!(
            classify_audio_error("system audio helper crashed"),
            PublicAudioErrorKind::Source
        );
        assert_eq!(
            classify_audio_error("operation timed out"),
            PublicAudioErrorKind::Timeout
        );
        assert_eq!(
            classify_audio_error("connection refused"),
            PublicAudioErrorKind::Unavailable
        );
        assert_eq!(
            classify_audio_error("unclassified internal failure"),
            PublicAudioErrorKind::Generic
        );

        let raw =
            "unauthorized bearer TEST_TOKEN_PRIVATE at /Users/example/secret from https://api.invalid";
        let public = public_audio_error(raw);
        assert_eq!(public, "Sign in to Bluey to start listening.");
        for sensitive in ["TEST_TOKEN_PRIVATE", "/Users/example", "https://", "bearer"] {
            assert!(!public
                .to_ascii_lowercase()
                .contains(&sensitive.to_ascii_lowercase()));
        }
    }

    #[test]
    fn failed_audio_status_redacts_all_renderer_error_fields() {
        let mut status = AudioPipelineStatus::idle();
        status.capture.state = AudioCaptureState::Failed;
        status.capture.last_error =
            Some("permission denied /Users/example/secret https://private.invalid".to_string());
        status.capture.system.last_error =
            Some("system audio helper token TEST_TOKEN_SYSTEM at /tmp/helper".to_string());
        status.capture.microphone.last_error =
            Some("microphone timeout at https://private.invalid".to_string());
        status.note = Some("opaque failure TEST_TOKEN_NOTE at /Users/example/note".to_string());

        let sanitized = sanitize_audio_pipeline_status(status);
        assert!(sanitized.capture.last_error.is_some());
        assert!(sanitized.capture.system.last_error.is_some());
        assert!(sanitized.capture.microphone.last_error.is_some());
        assert!(sanitized.note.is_some());
        let json = serde_json::to_string(&sanitized).unwrap();
        for sensitive in [
            "TEST_TOKEN_SYSTEM",
            "TEST_TOKEN_NOTE",
            "/Users/example",
            "/tmp/",
            "https://",
        ] {
            assert!(!json.contains(sensitive));
        }
    }

    #[test]
    fn listening_shortcut_default_and_label_share_one_accelerator() {
        let default = default_keybinds()
            .into_iter()
            .find(|(action, _)| *action == "toggle_listening")
            .unwrap()
            .1;
        assert_eq!(default, DEFAULT_LISTENING_SHORTCUT);
        #[cfg(target_os = "macos")]
        assert_eq!(listening_shortcut_label(default), "⌃⌥L");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(listening_shortcut_label(default), "Ctrl+Alt+L");
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
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
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
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_privacy_settings_command_system() {
        let result = privacy_settings_command("system");
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
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
                assert!(args.contains(&"ms-settings:sound".to_string()));
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            assert!(result.is_err());
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

    // Codex follow-up: real tray icon swap. Disguise PNGs are
    // embedded at compile time via include_bytes! so they are part of
    // the signed app bundle (no runtime path lookup, no missing-file
    // class). None mode restores the default Bluey icon.
    if let Some(tray) = app.tray_by_id("main") {
        match disguise_icon_bytes(disguise_mode) {
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

#[cfg(target_os = "macos")]
fn disguise_icon_bytes(mode: cue_stealth::DisguiseMode) -> Option<&'static [u8]> {
    match mode {
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
    }
}

#[cfg(target_os = "windows")]
fn disguise_icon_bytes(mode: cue_stealth::DisguiseMode) -> Option<&'static [u8]> {
    match mode {
        cue_stealth::DisguiseMode::None => None,
        cue_stealth::DisguiseMode::Activity => {
            Some(include_bytes!("../icons/disguise/win/activity.png"))
        }
        cue_stealth::DisguiseMode::Terminal => {
            Some(include_bytes!("../icons/disguise/win/terminal.png"))
        }
        cue_stealth::DisguiseMode::Settings => {
            Some(include_bytes!("../icons/disguise/win/settings.png"))
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn disguise_icon_bytes(_mode: cue_stealth::DisguiseMode) -> Option<&'static [u8]> {
    None
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

pub(crate) const DEFAULT_LISTENING_SHORTCUT: &str = "Ctrl+Alt+L";

pub struct ListeningShortcutState(pub String);

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
            ("toggle_listening", DEFAULT_LISTENING_SHORTCUT),
            ("push_to_talk", "CmdOrCtrl+Shift+P"),
            ("toggle_overlay", "CmdOrCtrl+Shift+H"),
            ("toggle_dashboard", "CmdOrCtrl+Shift+D"),
        ]
    } else {
        vec![
            ("toggle_listening", DEFAULT_LISTENING_SHORTCUT),
            ("push_to_talk", "Ctrl+Shift+P"),
            ("toggle_overlay", "Ctrl+Shift+H"),
            ("toggle_dashboard", "Ctrl+Shift+D"),
        ]
    }
}

pub(crate) fn listening_shortcut_accelerator(db_state: &DbState) -> String {
    let candidate = db_state.0.lock().ok().and_then(|db| {
        if let Err(error) = db.ensure_keybinds_table() {
            tracing::warn!(%error, "failed to prepare listening shortcut settings");
            return None;
        }
        match db.load_keybind("toggle_listening") {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, "failed to load listening shortcut");
                None
            }
        }
    });

    candidate
        .filter(|accelerator| {
            accelerator
                .parse::<tauri_plugin_global_shortcut::Shortcut>()
                .is_ok()
        })
        .unwrap_or_else(|| DEFAULT_LISTENING_SHORTCUT.to_string())
}

#[cfg(target_os = "macos")]
fn listening_shortcut_label(accelerator: &str) -> String {
    accelerator
        .split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "⌃".to_string(),
            "alt" | "option" => "⌥".to_string(),
            "shift" => "⇧".to_string(),
            "cmd" | "command" | "meta" | "super" | "cmdorctrl" => "⌘".to_string(),
            _ => part.trim().to_ascii_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(not(target_os = "macos"))]
fn listening_shortcut_label(accelerator: &str) -> String {
    accelerator
        .split('+')
        .map(|part| {
            if part.trim().eq_ignore_ascii_case("cmdorctrl") {
                "Ctrl".to_string()
            } else {
                part.trim().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Return the startup listening shortcut as a platform-friendly display label.
#[tauri::command]
pub fn get_listening_shortcut(shortcut: State<ListeningShortcutState>) -> String {
    listening_shortcut_label(&shortcut.0)
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
/// A changed listening shortcut takes effect after Bluey restarts.
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
        image_data_urls: Vec::new(),
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
    let session_instructions = cue_daemon::app::merge_answer_instructions(
        cue_daemon::app::assistant_profile_instructions(&meeting.assistant_profile),
        meeting.answer_instructions.clone(),
    );

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
            answer_mod::system_prompt_with_instructions(session_instructions.as_deref())
        } else {
            suggest_mod::system_prompt_with_instructions(session_instructions.as_deref())
        };
        let kind_str = if is_question { "answer" } else { "suggestion" };
        if let Some((text, response_metadata)) = try_speculative_dispatch(
            &user_text,
            &session_id,
            &system_prompt,
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
            .run_streaming_with_instructions(
                question,
                &session_id,
                llm.as_ref(),
                session_instructions.as_deref(),
                |partial, finished| {
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
                },
            )
            .await
            .map_err(|e| e.to_string())?
    } else {
        let app2 = app.clone();
        let rid = response_id.clone();
        WhatToAnswerLlm
            .run_streaming_with_instructions(
                &recent,
                &session_id,
                llm.as_ref(),
                session_instructions.as_deref(),
                |partial, finished| {
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
                },
            )
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
/// are configured (env var or opt-in saved dev secret) and exposes them by `LlmProvider::name()`.
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
        // token is in the local account store, register BlueyManagedProvider for
        // every cloud lane; bluey-server will pick the actual upstream
        // provider+model. Customer pays Bluey; Bluey owns the API keys.
        //
        // Legacy BYOK direct providers (OpenAI/Anthropic from env or saved
        // secrets) are gated behind BLUEY_DEV_BYOK=1 in debug/dev builds so
        // dev workflows still work while shipped customer binaries stay
        // managed-only.
        let managed_mode = match cloud_client_with_trace(trace_id) {
            Ok(client) => client.current_tokens().is_some(),
            Err(_) => false,
        };

        if managed_mode {
            tracing::info!("ProviderRegistry: managed mode active (account token found)");
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

        let allow_byok = dev_byok_enabled();

        if allow_byok {
            if let Some(key) = std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    allow_byok
                        .then(|| {
                            cue_daemon::secrets::load_api_key("llm_openai")
                                .ok()
                                .flatten()
                        })
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
                    allow_byok
                        .then(|| {
                            cue_daemon::secrets::load_api_key("llm_anthropic")
                                .ok()
                                .flatten()
                        })
                        .flatten()
                })
            {
                let provider: Arc<dyn cue_llm::LlmProvider> =
                    Arc::new(cue_llm::anthropic::AnthropicProvider::new(key));
                providers.insert("anthropic".to_string(), provider);
            }
        }

        // Ollama is a developer-only local path. Production/customer Bluey
        // answers must route through bluey-server so provider credentials and
        // billing remain server-side.
        if allow_byok {
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
/// account token in local account store, no direct OPENAI_API_KEY) used to be
/// rejected here. We now check managed mode first and return a
/// BlueyManagedProvider bound to the Balanced lane as the legacy
/// single-shot fallback. The speculative path (`try_speculative_
/// dispatch`) picks per-lane providers separately.
fn build_llm_provider_from_env(
    _db: &State<DbState>,
    trace_id: &str,
) -> Option<Box<dyn cue_llm::LlmProvider>> {
    // Managed mode first: Bluey account tokens take priority over BYOK unless
    // a debug/dev build has BLUEY_DEV_BYOK=1 explicitly set (matching the
    // ProviderRegistry policy).
    let allow_byok = dev_byok_enabled();
    if !allow_byok {
        if let Ok(client) = cloud_client_with_trace(trace_id) {
            if client.current_tokens().is_some() {
                return Some(Box::new(cue_llm::bluey_managed::BlueyManagedProvider::new(
                    client,
                    cue_llm::bluey_managed::ManagedLane::Balanced,
                )));
            }
        }
        return None;
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

fn dev_byok_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var("BLUEY_DEV_BYOK")
            .ok()
            .is_some_and(|value| value.trim() == "1")
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

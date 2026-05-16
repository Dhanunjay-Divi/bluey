use cue_core::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use cue_core::session::Session;
use serde::Serialize;
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

// ===== Daemon IPC helper =====

/// Send a request to the running daemon over TCP and return the response.
async fn daemon_ipc(request: DaemonRequest) -> Result<DaemonResponse, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    let stream = TcpStream::connect(DEFAULT_DAEMON_ADDR)
        .await
        .map_err(|e| format!("failed to connect to daemon: {e}"))?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
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

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
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
        if s.len() <= 4 {
            "****".to_string()
        } else {
            format!("****{}", &s[s.len() - 4..])
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
    // Query daemon status to decide start vs stop.
    let status = daemon_ipc(DaemonRequest::Status).await?;
    let is_active = match &status {
        DaemonResponse::Status { state } => {
            matches!(state.meeting, cue_core::MeetingState::InMeeting { .. })
        }
        _ => false,
    };
    let resp = if is_active {
        daemon_ipc(DaemonRequest::MeetingEnd).await?
    } else {
        daemon_ipc(DaemonRequest::MeetingStart { title: None }).await?
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
    // Toggle audio: if audio is active, stop it; otherwise start mic-only.
    let status = daemon_ipc(DaemonRequest::AudioStatus).await?;
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
        daemon_ipc(DaemonRequest::AudioStop).await?
    } else {
        let mic_device_id = load_mic_device_from_settings(&db);
        daemon_ipc(DaemonRequest::AudioStart {
            enable_system: false,
            enable_microphone: true,
            mic_device_id,
        })
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
        .skip(since_index)
        .map(|seg| {
            let source = match seg.speaker {
                cue_core::Speaker::System => "system",
                cue_core::Speaker::User => "microphone",
                _ => "unknown",
            };
            LiveTranscriptPayload {
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

    Ok(())
}

/// Get the current disguise mode from persisted settings.
#[tauri::command]
pub fn get_disguise(db: State<DbState>) -> Result<String, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    let mode = db
        .load_setting("disguise_mode")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "none".to_string());
    Ok(mode)
}

// ===== Phase 3 Round 9: Mouse Passthrough Toggle =====

/// Set overlay mouse passthrough state and persist it.
#[tauri::command]
pub fn set_mouse_passthrough(enabled: bool, db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.save_setting("overlay_passthrough", if enabled { "true" } else { "false" })
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

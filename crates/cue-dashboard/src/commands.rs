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
        //
        // At this point the DELETE already committed and the in-memory active
        // selection already cleared. A lock poison or SQLite write failure here
        // must NOT turn that into a user-facing error: the delete succeeded.
        // The persisted active_session_id (if any) will be validated on next
        // daemon startup via load_active_session, which returns None for stale
        // ids. Log + continue.
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
        // Persist the new active selection so a daemon restart can restore it.
        // Best-effort: a DB write failure here should not fail the command —
        // the user selection is still correct in-memory and will be persisted
        // on the next switch.
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

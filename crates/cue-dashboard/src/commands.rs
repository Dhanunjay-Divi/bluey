use cue_core::session::Session;
use tauri::State;
use uuid::Uuid;

use crate::DbState;

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
pub fn create_session(title: Option<String>, db: State<DbState>) -> Result<Session, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.create_session(title).map_err(|e| e.to_string())
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
pub fn delete_session(id: String, db: State<DbState>) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.delete_session(uuid).map_err(|e| e.to_string())
}

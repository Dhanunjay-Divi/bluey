pub mod rag;
pub mod search;
pub mod speakers;

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use uuid::Uuid;

use cue_core::session::{Lane, NewTurn, Session, SessionStatus, Turn};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the SQLite database at `path` and run migrations.
    /// Pass ":memory:" for an in-memory database (tests).
    pub fn open(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            let p = Path::new(path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(p)?
        };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<()> {
        const MIGRATION_002: &str = include_str!("../../../../infra/migrations/002_sessions.sql");
        const MIGRATION_003: &str =
            include_str!("../../../../infra/migrations/003_turns_unique_index.sql");
        const MIGRATION_004: &str = include_str!("../../../../infra/migrations/004_app_state.sql");
        const MIGRATION_005: &str = include_str!("../../../../infra/migrations/005_settings.sql");
        const MIGRATION_006: &str =
            include_str!("../../../../infra/migrations/006_transcript_fts.sql");
        const MIGRATION_007: &str = include_str!("../../../../infra/migrations/007_speakers.sql");
        const MIGRATION_008: &str =
            include_str!("../../../../infra/migrations/008_fts_cascade_fix.sql");
        const MIGRATION_009: &str =
            include_str!("../../../../infra/migrations/009_cue_responses.sql");
        self.conn
            .execute_batch(MIGRATION_002)
            .context("failed to run session migration")?;
        self.conn
            .execute_batch(MIGRATION_003)
            .context("failed to run turns unique index migration")?;
        self.conn
            .execute_batch(MIGRATION_004)
            .context("failed to run app_state migration")?;
        self.conn
            .execute_batch(MIGRATION_005)
            .context("failed to run settings migration")?;
        self.conn
            .execute_batch(MIGRATION_006)
            .context("failed to run transcript_fts migration")?;
        self.conn
            .execute_batch(MIGRATION_007)
            .context("failed to run speakers migration")?;
        self.conn
            .execute_batch(MIGRATION_008)
            .context("failed to run fts cascade fix migration")?;
        self.conn
            .execute_batch(MIGRATION_009)
            .context("failed to run cue_responses migration")?;
        Ok(())
    }

    pub fn create_session(&self, title: Option<String>) -> Result<Session> {
        let id = Uuid::new_v4();
        let now = now_ms();
        let title = title.unwrap_or_else(|| "Untitled session".to_string());
        self.conn.execute(
            "INSERT INTO sessions (id, title, status, created_at, updated_at, last_active_at) \
             VALUES (?1, ?2, 'active', ?3, ?3, ?3)",
            params![id.to_string(), title, now],
        )?;
        self.get_session(id)?
            .context("session not found after insert")
    }

    pub fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, status, created_at, updated_at, last_active_at, \
             archived_at, token_count, compressed_summary, active_skill, metadata \
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id.to_string()], row_to_session)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_sessions(&self, status: Option<SessionStatus>, limit: u32) -> Result<Vec<Session>> {
        let (sql, p): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
            Some(s) => (
                "SELECT id, title, status, created_at, updated_at, last_active_at, \
                 archived_at, token_count, compressed_summary, active_skill, metadata \
                 FROM sessions WHERE status = ?1 ORDER BY updated_at DESC LIMIT ?2"
                    .to_string(),
                vec![Box::new(s.as_str().to_string()), Box::new(limit)],
            ),
            None => (
                "SELECT id, title, status, created_at, updated_at, last_active_at, \
                 archived_at, token_count, compressed_summary, active_skill, metadata \
                 FROM sessions ORDER BY updated_at DESC LIMIT ?1"
                    .to_string(),
                vec![Box::new(limit)],
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p.iter()), row_to_session)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn update_session_status(&self, id: Uuid, status: SessionStatus) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE sessions SET status = ?1, updated_at = ?2, \
             archived_at = CASE WHEN ?1 = 'archived' THEN ?2 ELSE NULL END WHERE id = ?3",
            params![status.as_str(), now, id.to_string()],
        )?;
        Ok(())
    }

    pub fn update_session_title(&self, id: Uuid, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(anyhow::anyhow!("session title cannot be empty"));
        }
        let now = now_ms();
        let changed = self.conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id.to_string()],
        )?;
        if changed == 0 {
            return Err(anyhow::anyhow!("session {id} not found"));
        }
        Ok(())
    }

    pub fn archive_session(&self, id: Uuid) -> Result<()> {
        self.update_session_status(id, SessionStatus::Archived)
    }

    pub fn delete_session(&self, id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn append_turn(&self, session_id: Uuid, turn: NewTurn) -> Result<Turn> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.append_turn_inner(session_id, &turn);
        match &result {
            Ok(_) => self.conn.execute_batch("COMMIT")?,
            Err(_) => {
                let _ = self.conn.execute_batch("ROLLBACK");
            }
        }
        result
    }

    fn append_turn_inner(&self, session_id: Uuid, turn: &NewTurn) -> Result<Turn> {
        let id = Uuid::new_v4();
        let turn_index: u32 = self.conn.query_row(
            "SELECT COALESCE(MAX(turn_index) + 1, 0) FROM turns WHERE session_id = ?1",
            params![session_id.to_string()],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "INSERT INTO turns (id, session_id, turn_index, user_message, model_response, \
             lane, provider, model, created_at, duration_ms, input_tokens, output_tokens, \
             cost_cents) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id.to_string(),
                session_id.to_string(),
                turn_index,
                turn.user_message,
                turn.model_response,
                turn.lane.as_str(),
                turn.provider,
                turn.model,
                turn.created_at,
                turn.duration_ms,
                turn.input_tokens,
                turn.output_tokens,
                turn.cost_cents,
            ],
        )?;

        let tokens_added = turn.input_tokens.unwrap_or(0) + turn.output_tokens.unwrap_or(0);
        let now = now_ms();
        self.conn.execute(
            "UPDATE sessions SET token_count = token_count + ?1, \
             last_active_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![tokens_added, now, session_id.to_string()],
        )?;

        Ok(Turn {
            id,
            session_id,
            turn_index,
            user_message: turn.user_message.clone(),
            model_response: turn.model_response.clone(),
            lane: turn.lane,
            provider: turn.provider.clone(),
            model: turn.model.clone(),
            created_at: turn.created_at,
            duration_ms: turn.duration_ms,
            input_tokens: turn.input_tokens,
            output_tokens: turn.output_tokens,
            cost_cents: turn.cost_cents,
        })
    }

    pub fn list_turns(&self, session_id: Uuid, limit: Option<u32>) -> Result<Vec<Turn>> {
        let limit = limit.unwrap_or(1000);
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, turn_index, user_message, model_response, lane, \
             provider, model, created_at, duration_ms, input_tokens, output_tokens, \
             cost_cents FROM turns WHERE session_id = ?1 ORDER BY turn_index ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id.to_string(), limit], row_to_turn)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ===== Key-value app-state (Phase 3 follow-up) =====

    /// Read a single value from the `app_state` key-value table.
    ///
    /// Returns `None` if the key has never been written OR was explicitly
    /// cleared via `set_app_state(key, None)`. Empty string is a valid value
    /// and is returned as `Some(String::new())` — callers that want to treat
    /// empty as unset must do so explicitly.
    pub fn get_app_state(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM app_state WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(row.get::<_, Option<String>>(0)?),
            None => Ok(None),
        }
    }

    /// Upsert a key into the `app_state` table.
    ///
    /// Passing `value = None` clears the stored value but keeps the row, so a
    /// subsequent `get_app_state(key)` returns `Ok(None)`.
    pub fn set_app_state(&self, key: &str, value: Option<&str>) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
                "INSERT INTO app_state (key, value, updated_at) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now],
            )?;
        Ok(())
    }

    /// Convenience: load the persisted active session id, if any, and verify
    /// it still points at a real session. Invalid / stale ids return `None`
    /// so the caller can fall back to "no selection" cleanly.
    pub fn load_active_session(&self) -> Result<Option<Uuid>> {
        let raw = match self.get_app_state("active_session_id")? {
            Some(s) => s,
            None => return Ok(None),
        };
        let Ok(id) = Uuid::parse_str(&raw) else {
            return Ok(None);
        };
        if self.get_session(id)?.is_none() {
            return Ok(None);
        }
        Ok(Some(id))
    }

    /// Persist the active session id. Pass `None` to clear.
    pub fn save_active_session(&self, id: Option<Uuid>) -> Result<()> {
        match id {
            Some(u) => self.set_app_state("active_session_id", Some(&u.to_string())),
            None => self.set_app_state("active_session_id", None),
        }
    }

    // ===== Settings =====

    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn load_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM app_settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub fn load_all_settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM app_settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }
        Ok(map)
    }

    // ===== Phase 3 Round 9: Cue Responses =====

    pub fn insert_cue_response(
        &self,
        id: &str,
        session_id: &str,
        kind: &str,
        text: &str,
        source_text: Option<&str>,
        ts_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cue_responses (id, session_id, kind, text, source_text, ts_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, session_id, kind, text, source_text, ts_ms],
    // ===== Keybinds (Phase 3 Round 9) =====

    /// Ensure the user_keybinds table exists.
    pub fn ensure_keybinds_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS user_keybinds (
                action TEXT PRIMARY KEY,
                accelerator TEXT NOT NULL
            );"
        )?;
        Ok(())
    }

    pub fn list_cue_responses(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<crate::llm::CueResponse>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, kind, text, source_text, ts_ms \
             FROM cue_responses WHERE session_id = ?1 ORDER BY ts_ms DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            Ok(crate::llm::CueResponse {
                id: row.get(0)?,
                source_session_id: row.get(1)?,
                kind: row.get(2)?,
                text: row.get(3)?,
                source_text: row.get(4)?,
                ts_ms: row.get::<_, i64>(5)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    /// Load a keybind for a given action.
    pub fn load_keybind(&self, action: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT accelerator FROM user_keybinds WHERE action = ?1"
        )?;
        let result = stmt
            .query_row(params![action], |row| row.get::<_, String>(0))
            .ok();
        Ok(result)
    }

    /// Save a keybind for a given action (upsert).
    pub fn save_keybind(&self, action: &str, accelerator: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO user_keybinds (action, accelerator) VALUES (?1, ?2)
             ON CONFLICT(action) DO UPDATE SET accelerator = excluded.accelerator",
            params![action, accelerator],
        )?;
        Ok(())
    }

    /// Reset all keybinds (delete all custom entries).
    pub fn reset_keybinds(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM user_keybinds;")?;
        Ok(())
    }
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    let id_str: String = row.get(0)?;
    let status_str: String = row.get(2)?;
    let skill_str: Option<String> = row.get(9)?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let status = SessionStatus::from_str(&status_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            format!("invalid session status: {status_str}").into(),
        )
    })?;
    Ok(Session {
        id,
        title: row.get(1)?,
        status,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        last_active_at: row.get(5)?,
        archived_at: row.get(6)?,
        token_count: row.get::<_, i64>(7)? as u64,
        compressed_summary: row.get(8)?,
        active_skill: skill_str.and_then(|s| serde_json::from_str(&format!("\"{}\"", s)).ok()),
        metadata: row.get(10)?,
    })
}

fn row_to_turn(row: &rusqlite::Row) -> rusqlite::Result<Turn> {
    let id_str: String = row.get(0)?;
    let session_id_str: String = row.get(1)?;
    let lane_str: String = row.get(5)?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let session_id = Uuid::parse_str(&session_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let lane = Lane::from_str(&lane_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            format!("invalid lane: {lane_str}").into(),
        )
    })?;
    Ok(Turn {
        id,
        session_id,
        turn_index: row.get(2)?,
        user_message: row.get(3)?,
        model_response: row.get(4)?,
        lane,
        provider: row.get(6)?,
        model: row.get(7)?,
        created_at: row.get(8)?,
        duration_ms: row.get(9)?,
        input_tokens: row.get(10)?,
        output_tokens: row.get(11)?,
        cost_cents: row.get(12)?,
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::session::{Lane, NewTurn, SessionStatus};

    fn test_db() -> Database {
        Database::open(":memory:").expect("failed to open in-memory db")
    }

    #[test]
    fn test_create_and_get_session() {
        let db = test_db();
        let session = db.create_session(Some("Test Session".into())).unwrap();
        assert_eq!(session.title, "Test Session");
        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.token_count, 0);

        let fetched = db.get_session(session.id).unwrap().unwrap();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.title, "Test Session");
    }

    #[test]
    fn test_list_sessions_by_status() {
        let db = test_db();
        let s1 = db.create_session(Some("Active 1".into())).unwrap();
        let s2 = db.create_session(Some("Active 2".into())).unwrap();
        db.update_session_status(s2.id, SessionStatus::Paused)
            .unwrap();

        let active = db.list_sessions(Some(SessionStatus::Active), 10).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, s1.id);

        let paused = db.list_sessions(Some(SessionStatus::Paused), 10).unwrap();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0].id, s2.id);

        let all = db.list_sessions(None, 10).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_append_turn_and_list_turns() {
        let db = test_db();
        let session = db.create_session(None).unwrap();

        let t1 = db
            .append_turn(
                session.id,
                NewTurn {
                    user_message: "Hello".into(),
                    model_response: "Hi there!".into(),
                    lane: Lane::Snap,
                    provider: "cerebras".into(),
                    model: "deepseek-v3".into(),
                    created_at: 1000,
                    duration_ms: Some(150),
                    input_tokens: Some(10),
                    output_tokens: Some(20),
                    cost_cents: Some(1),
                },
            )
            .unwrap();
        assert_eq!(t1.turn_index, 0);

        let t2 = db
            .append_turn(
                session.id,
                NewTurn {
                    user_message: "Follow up".into(),
                    model_response: "Sure thing".into(),
                    lane: Lane::Solve,
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4.5".into(),
                    created_at: 2000,
                    duration_ms: Some(500),
                    input_tokens: Some(50),
                    output_tokens: Some(100),
                    cost_cents: Some(5),
                },
            )
            .unwrap();
        assert_eq!(t2.turn_index, 1);

        let turns = db.list_turns(session.id, None).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user_message, "Hello");
        assert_eq!(turns[1].lane, Lane::Solve);

        // Verify token_count updated on session
        let updated = db.get_session(session.id).unwrap().unwrap();
        assert_eq!(updated.token_count, 180); // 10+20 + 50+100
    }

    #[test]
    fn test_cascade_delete_session_removes_turns() {
        let db = test_db();
        let session = db.create_session(None).unwrap();
        db.append_turn(
            session.id,
            NewTurn {
                user_message: "msg".into(),
                model_response: "resp".into(),
                lane: Lane::Think,
                provider: "openai".into(),
                model: "gpt-4o".into(),
                created_at: 1000,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                cost_cents: None,
            },
        )
        .unwrap();

        let turns = db.list_turns(session.id, None).unwrap();
        assert_eq!(turns.len(), 1);

        db.delete_session(session.id).unwrap();
        assert!(db.get_session(session.id).unwrap().is_none());

        let turns = db.list_turns(session.id, None).unwrap();
        assert!(turns.is_empty());
    }

    // ===== Phase 2 regression tests =====
    // Cover deferred D0.2 V2 nits + new Phase 2 behavior.

    #[test]
    fn test_update_session_title_works() {
        let db = test_db();
        let s = db.create_session(Some("Old title".into())).unwrap();

        db.update_session_title(s.id, "New title").unwrap();

        let fetched = db.get_session(s.id).unwrap().unwrap();
        assert_eq!(fetched.title, "New title");
        assert!(
            fetched.updated_at >= s.updated_at,
            "updated_at must be refreshed on title change"
        );
    }

    #[test]
    fn test_update_session_title_rejects_empty() {
        let db = test_db();
        let s = db.create_session(Some("Keep".into())).unwrap();

        assert!(db.update_session_title(s.id, "").is_err());
        assert!(db.update_session_title(s.id, "   ").is_err());

        // Title unchanged
        let fetched = db.get_session(s.id).unwrap().unwrap();
        assert_eq!(fetched.title, "Keep");
    }

    #[test]
    fn test_update_session_title_missing_id_errors() {
        let db = test_db();
        let random_id = uuid::Uuid::new_v4();

        let err = db
            .update_session_title(random_id, "Anything")
            .expect_err("must error for missing id");
        assert!(format!("{err}").contains("not found"));
    }

    // D0.2 V2 nit regression: unarchiving a session must clear archived_at.
    #[test]
    fn test_unarchive_clears_archived_at() {
        let db = test_db();
        let s = db.create_session(Some("S".into())).unwrap();

        db.update_session_status(s.id, SessionStatus::Archived)
            .unwrap();
        let archived: Option<i64> = db
            .conn
            .query_row(
                "SELECT archived_at FROM sessions WHERE id = ?1",
                params![s.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(archived.is_some(), "archived_at must be set after archive");

        // Move back to active
        db.update_session_status(s.id, SessionStatus::Active)
            .unwrap();
        let after: Option<i64> = db
            .conn
            .query_row(
                "SELECT archived_at FROM sessions WHERE id = ?1",
                params![s.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(after.is_none(), "archived_at must be cleared on unarchive");
    }

    // D0.2 V2 nit regression: invalid UUID strings in DB rows surface as errors,
    // NOT silently mapped to nil UUID via unwrap_or_default.
    #[test]
    fn test_invalid_uuid_in_db_row_surfaces_error() {
        let db = test_db();
        // Force-insert a row with a malformed id
        db.conn
            .execute(
                "INSERT INTO sessions (id, title, status, created_at, updated_at, token_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params!["not-a-uuid", "broken", "active", 1i64, 1i64, 0i64],
            )
            .unwrap();

        // list_sessions should fail, not return a session with nil UUID
        let result = db.list_sessions(None, 10);
        assert!(
            result.is_err(),
            "list_sessions must surface invalid-UUID errors, got: {:?}",
            result
        );
    }

    // D0.2 V2 nit regression: duplicate (session_id, turn_index) must fail
    // due to the unique index added in migration 003.
    // Phase 3 follow-up: app_state persistence round-trips for active session id
    #[test]
    fn test_app_state_round_trip() {
        let db = test_db();
        assert_eq!(db.get_app_state("missing").unwrap(), None);
        db.set_app_state("foo", Some("bar")).unwrap();
        assert_eq!(db.get_app_state("foo").unwrap(), Some("bar".into()));
        db.set_app_state("foo", Some("baz")).unwrap();
        assert_eq!(db.get_app_state("foo").unwrap(), Some("baz".into()));
        db.set_app_state("foo", None).unwrap();
        assert_eq!(db.get_app_state("foo").unwrap(), None);
    }

    #[test]
    fn test_active_session_persistence_valid() {
        let db = test_db();
        let s = db.create_session(Some("Persist me".into())).unwrap();
        db.save_active_session(Some(s.id)).unwrap();

        let loaded = db.load_active_session().unwrap();
        assert_eq!(loaded, Some(s.id));
    }

    #[test]
    fn test_active_session_persistence_recovers_from_stale_id() {
        let db = test_db();
        let s = db.create_session(None).unwrap();
        db.save_active_session(Some(s.id)).unwrap();
        // Delete the session while it is still the persisted active selection
        db.delete_session(s.id).unwrap();

        // Must not resurrect the stale id — return None, caller starts blank
        let loaded = db.load_active_session().unwrap();
        assert_eq!(loaded, None);

        // And we can clear explicitly
        db.save_active_session(None).unwrap();
        assert_eq!(db.load_active_session().unwrap(), None);
    }

    #[test]
    fn test_duplicate_turn_index_rejected_by_unique_constraint() {
        let db = test_db();
        let s = db.create_session(None).unwrap();

        // Insert first turn via the normal path to establish turn_index=0
        db.append_turn(
            s.id,
            NewTurn {
                user_message: "m0".into(),
                model_response: "r0".into(),
                lane: Lane::Snap,
                provider: "cerebras".into(),
                model: "deepseek-v3".into(),
                created_at: 1000,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                cost_cents: None,
            },
        )
        .unwrap();

        // Now try to force-insert a duplicate turn_index=0 for the same session
        let dup_id = uuid::Uuid::new_v4().to_string();
        let result = db.conn.execute(
            "INSERT INTO turns (id, session_id, turn_index, user_message, model_response, \
             lane, provider, model, created_at) \
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                dup_id,
                s.id.to_string(),
                "dup",
                "dup",
                "snap",
                "cerebras",
                "deepseek-v3",
                2000i64,
            ],
        );
        assert!(
            result.is_err(),
            "unique index on (session_id, turn_index) must reject duplicate"
        );
        let msg = format!("{:?}", result.err().unwrap());
        assert!(
            msg.contains("UNIQUE") || msg.contains("unique") || msg.contains("constraint"),
            "error should mention unique constraint violation, got: {msg}"
        );
    }
}

#[cfg(test)]
mod fts_tests {
    use super::*;

    fn test_db() -> Database {
        Database::open(":memory:").expect("failed to open in-memory db")
    }

    #[test]
    fn test_insert_and_search_transcripts() {
        let db = test_db();
        let session = db.create_session(Some("FTS Test".into())).unwrap();
        let sid = session.id.to_string();
        db.insert_transcript(
            &sid,
            "hello world from the microphone",
            "mic",
            Some(0),
            true,
            1000,
        )
        .unwrap();
        db.insert_transcript(
            &sid,
            "system audio playing music",
            "system",
            None,
            true,
            2000,
        )
        .unwrap();
        db.insert_transcript(&sid, "hello again from speaker", "mic", Some(1), true, 3000)
            .unwrap();
        let hits = db.search_transcripts("hello", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits[0].snippet.contains("hello"));
        assert_eq!(hits[0].session_id, sid);
        let hits = db.search_transcripts("music", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "system");
    }

    #[test]
    fn test_fts_ranking_sanity() {
        let db = test_db();
        let session = db.create_session(Some("Rank Test".into())).unwrap();
        let sid = session.id.to_string();
        db.insert_transcript(&sid, "the quick brown fox", "mic", None, true, 1000)
            .unwrap();
        db.insert_transcript(
            &sid,
            "fox fox fox repeated many times fox",
            "mic",
            None,
            true,
            2000,
        )
        .unwrap();
        let hits = db.search_transcripts("fox", 10).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn test_speaker_set_and_list() {
        let db = test_db();
        let session = db.create_session(Some("Speaker Test".into())).unwrap();
        let sid = session.id.to_string();
        db.set_speaker_name(&sid, 0, "Alice", Some("#ff0000"))
            .unwrap();
        db.set_speaker_name(&sid, 1, "Bob", None).unwrap();
        let speakers = db.list_speakers(&sid).unwrap();
        assert_eq!(speakers.len(), 2);
        assert_eq!(speakers[0].name, "Alice");
        assert_eq!(speakers[0].color, Some("#ff0000".to_string()));
        assert_eq!(speakers[1].name, "Bob");
    }

    #[test]
    fn test_speaker_rename_persists() {
        let db = test_db();
        let session = db.create_session(None).unwrap();
        let sid = session.id.to_string();
        db.set_speaker_name(&sid, 0, "Speaker 0", None).unwrap();
        db.set_speaker_name(&sid, 0, "Alice", None).unwrap();
        let speakers = db.list_speakers(&sid).unwrap();
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].name, "Alice");
    }

    #[test]
    fn test_speaker_color_preserved_on_rename() {
        let db = test_db();
        let session = db.create_session(None).unwrap();
        let sid = session.id.to_string();
        db.set_speaker_name(&sid, 0, "Alice", Some("#00ff00"))
            .unwrap();
        db.set_speaker_name(&sid, 0, "Alicia", None).unwrap();
        let speakers = db.list_speakers(&sid).unwrap();
        assert_eq!(speakers[0].name, "Alicia");
        assert_eq!(speakers[0].color, Some("#00ff00".to_string()));
    }

    #[test]
    fn test_export_markdown() {
        let db = test_db();
        let session = db.create_session(Some("Export Test".into())).unwrap();
        let sid = session.id.to_string();
        db.insert_transcript(&sid, "Hello everyone", "mic", Some(0), true, 1000)
            .unwrap();
        db.set_speaker_name(&sid, 0, "Alice", None).unwrap();
        let md = db
            .export_session_markdown(&sid, &super::search::ExportOptions::default())
            .unwrap();
        assert!(md.contains("# Export Test"));
        assert!(md.contains("Alice"));
        assert!(md.contains("Hello everyone"));
    }

    #[test]
    fn test_export_text() {
        let db = test_db();
        let session = db.create_session(Some("Text Export".into())).unwrap();
        let sid = session.id.to_string();
        db.insert_transcript(&sid, "Test line", "mic", None, true, 1000)
            .unwrap();
        let txt = db.export_session_text(&sid).unwrap();
        assert!(txt.contains("Text Export"));
        assert!(txt.contains("Test line"));
    }

    #[test]
    fn test_export_json() {
        let db = test_db();
        let session = db.create_session(Some("JSON Export".into())).unwrap();
        let sid = session.id.to_string();
        db.insert_transcript(&sid, "JSON test", "mic", Some(0), true, 1000)
            .unwrap();
        db.set_speaker_name(&sid, 0, "Charlie", None).unwrap();
        let json = db.export_session_json(&sid).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["title"], "JSON Export");
        assert_eq!(parsed["transcripts"][0]["text"], "JSON test");
        assert_eq!(parsed["speakers"][0]["name"], "Charlie");
    }

    #[test]
    fn test_settings_round_trip() {
        let db = test_db();
        db.save_setting("key1", "val1").unwrap();
        assert_eq!(db.load_setting("key1").unwrap(), Some("val1".into()));
        let all = db.load_all_settings().unwrap();
        assert_eq!(all.get("key1").unwrap(), "val1");
    }

    #[test]
    fn fts_delete_removes_index_row() {
        let db = test_db();
        let session = db.create_session(Some("Del Test".into())).unwrap();
        let sid = session.id.to_string();
        let tid = db
            .insert_transcript(&sid, "unique deletable phrase", "mic", None, true, 1000)
            .unwrap();
        let hits = db.search_transcripts("deletable", 10).unwrap();
        assert_eq!(hits.len(), 1);
        db.delete_transcript(&tid).unwrap();
        let hits = db.search_transcripts("deletable", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn fts_cascade_delete_clears_session_index() {
        let db = test_db();
        let session = db.create_session(Some("Cascade Test".into())).unwrap();
        let sid = session.id.to_string();
        db.insert_transcript(&sid, "cascade alpha content", "mic", None, true, 1000)
            .unwrap();
        db.insert_transcript(&sid, "cascade beta content", "mic", None, true, 2000)
            .unwrap();
        db.insert_transcript(&sid, "cascade gamma content", "mic", None, true, 3000)
            .unwrap();
        assert_eq!(db.search_transcripts("cascade", 10).unwrap().len(), 3);
        db.delete_session(session.id).unwrap();
        assert!(db.search_transcripts("cascade", 10).unwrap().is_empty());
    }

    #[test]
    fn fts_index_is_consistent_after_mixed_ops() {
        let db = test_db();
        let session = db.create_session(Some("Mixed Ops".into())).unwrap();
        let sid = session.id.to_string();
        let t1 = db
            .insert_transcript(&sid, "mixop first entry", "mic", None, true, 1000)
            .unwrap();
        db.insert_transcript(&sid, "mixop second entry", "mic", None, true, 2000)
            .unwrap();
        assert_eq!(db.search_transcripts("mixop", 10).unwrap().len(), 2);
        db.delete_transcript(&t1).unwrap();
        assert_eq!(db.search_transcripts("mixop", 10).unwrap().len(), 1);
        db.insert_transcript(&sid, "mixop third entry", "mic", None, true, 3000)
            .unwrap();
        let hits = db.search_transcripts("mixop", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.text.contains("second")));
        assert!(hits.iter().any(|h| h.text.contains("third")));
    }

    #[test]
    fn keybind_save_load_roundtrip() {
        let db = test_db();
        db.ensure_keybinds_table().unwrap();
        db.save_keybind("toggle_listening", "CmdOrCtrl+Shift+L").unwrap();
        let loaded = db.load_keybind("toggle_listening").unwrap();
        assert_eq!(loaded, Some("CmdOrCtrl+Shift+L".to_string()));
    }

    #[test]
    fn keybind_upsert_overwrites() {
        let db = test_db();
        db.ensure_keybinds_table().unwrap();
        db.save_keybind("toggle_listening", "CmdOrCtrl+Shift+L").unwrap();
        db.save_keybind("toggle_listening", "CmdOrCtrl+Shift+K").unwrap();
        let loaded = db.load_keybind("toggle_listening").unwrap();
        assert_eq!(loaded, Some("CmdOrCtrl+Shift+K".to_string()));
    }

    #[test]
    fn keybind_load_missing_returns_none() {
        let db = test_db();
        db.ensure_keybinds_table().unwrap();
        assert_eq!(db.load_keybind("nonexistent").unwrap(), None);
    }

    #[test]
    fn keybind_reset_clears_all() {
        let db = test_db();
        db.ensure_keybinds_table().unwrap();
        db.save_keybind("toggle_listening", "CmdOrCtrl+Shift+L").unwrap();
        db.save_keybind("push_to_talk", "CmdOrCtrl+Shift+P").unwrap();
        db.reset_keybinds().unwrap();
        assert_eq!(db.load_keybind("toggle_listening").unwrap(), None);
        assert_eq!(db.load_keybind("push_to_talk").unwrap(), None);
    }

    #[test]
    fn passthrough_setting_persists() {
        let db = test_db();
        db.save_setting("overlay_passthrough", "false").unwrap();
        let val = db.load_setting("overlay_passthrough").unwrap();
        assert_eq!(val, Some("false".to_string()));
        db.save_setting("overlay_passthrough", "true").unwrap();
        let val = db.load_setting("overlay_passthrough").unwrap();
        assert_eq!(val, Some("true".to_string()));
    }

}

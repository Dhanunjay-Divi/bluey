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
        const MIGRATION: &str = include_str!("../../../../infra/migrations/002_sessions.sql");
        self.conn
            .execute_batch(MIGRATION)
            .context("failed to run session migration")?;
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
            user_message: turn.user_message,
            model_response: turn.model_response,
            lane: turn.lane,
            provider: turn.provider,
            model: turn.model,
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
}

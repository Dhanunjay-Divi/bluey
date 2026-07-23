//! In-meeting conversation persistence: the app-owned Q&A turn log
//! (migration 012). Each row is one turn (the user's visible question or the
//! copilot's post-guard answer), ordered by a per-meeting monotonic `turn_idx`.
//! See `012_conversation_turns.sql`.
//!
//! This is the durable source of truth for the running dialogue. The rolling
//! conversation summary (older turns folded down) lives in daemon memory; these
//! rows survive a restart and are re-foldable.

use anyhow::Result;
use rusqlite::params;
use uuid::Uuid;

use cue_core::conversation::{ConvRole, ConvTurn};

use super::{now_ms, Database};

impl Database {
    /// Append one conversation turn for a meeting, assigning the next
    /// `turn_idx` atomically. The caller MUST have ensured a `sessions` row for
    /// `meeting_id` (via [`Database::ensure_meeting_session`]) so the FK holds —
    /// the same discipline the diarization tables use (commit 338f1e9).
    pub fn conv_append(&self, meeting_id: Uuid, role: ConvRole, text: &str) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.conv_append_inner(meeting_id, role, text);
        match &result {
            Ok(_) => self.conn.execute_batch("COMMIT")?,
            Err(_) => {
                let _ = self.conn.execute_batch("ROLLBACK");
            }
        }
        result
    }

    fn conv_append_inner(&self, meeting_id: Uuid, role: ConvRole, text: &str) -> Result<()> {
        let turn_idx: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(turn_idx) + 1, 0) FROM conversation_turns WHERE session_id = ?1",
            params![meeting_id.to_string()],
            |row| row.get(0),
        )?;
        let epoch_secs = now_ms() / 1000;
        self.conn.execute(
            "INSERT INTO conversation_turns (session_id, turn_idx, role, text, epoch_secs) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                meeting_id.to_string(),
                turn_idx,
                role.as_str(),
                text,
                epoch_secs,
            ],
        )?;
        Ok(())
    }

    /// The most recent `limit` turns for a meeting, in chronological order
    /// (newest LAST — ready to feed [`cue_core::conversation::assemble_block`]).
    pub fn conv_turns(&self, meeting_id: Uuid, limit: usize) -> Result<Vec<ConvTurn>> {
        // Take the newest `limit` by ordering DESC + LIMIT, then reverse to
        // chronological so the assembler sees oldest→newest.
        let mut stmt = self.conn.prepare(
            "SELECT role, text, epoch_secs FROM conversation_turns \
             WHERE session_id = ?1 ORDER BY turn_idx DESC LIMIT ?2",
        )?;
        let mut rows: Vec<ConvTurn> = stmt
            .query_map(params![meeting_id.to_string(), limit as i64], |row| {
                let role: String = row.get(0)?;
                let text: String = row.get(1)?;
                let epoch_secs: i64 = row.get(2)?;
                Ok(ConvTurn {
                    role: ConvRole::from_str_lossy(&role),
                    text,
                    epoch_secs: epoch_secs.max(0) as u64,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// Delete the `count` OLDEST turns of a meeting (used after those turns are
    /// folded into the rolling summary, so they aren't summarized twice).
    pub fn conv_delete_oldest(&self, meeting_id: Uuid, count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        self.conn.execute(
            "DELETE FROM conversation_turns WHERE id IN ( \
                 SELECT id FROM conversation_turns WHERE session_id = ?1 \
                 ORDER BY turn_idx ASC LIMIT ?2 )",
            params![meeting_id.to_string(), count as i64],
        )?;
        Ok(())
    }

    /// Prune to at most `max_turns` rows, dropping the oldest overflow. A safety
    /// cap so a marathon meeting can't grow the table unbounded; the assembler's
    /// token budget already bounds what's SENT, this bounds what's STORED.
    pub fn conv_prune(&self, meeting_id: Uuid, max_turns: usize) -> Result<()> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM conversation_turns WHERE session_id = ?1",
            params![meeting_id.to_string()],
            |row| row.get(0),
        )?;
        let excess = n - max_turns as i64;
        if excess > 0 {
            self.conv_delete_oldest(meeting_id, excess as usize)?;
        }
        Ok(())
    }

    /// Drop every conversation turn for a meeting.
    pub fn conv_clear(&self, meeting_id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM conversation_turns WHERE session_id = ?1",
            params![meeting_id.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        let db = Database::open(":memory:").expect("open in-memory db");
        // The `conversation_turns` migration is gated OFF by default in
        // production (see `Database::run_migrations` / `conv_memory_enabled`),
        // so it is not created by `open`. These tests validate the table's SQL
        // logic directly, so we create it explicitly regardless of the flag.
        db.conn
            .execute_batch(include_str!(
                "../../../../infra/migrations/012_conversation_turns.sql"
            ))
            .expect("create conversation_turns table for tests");
        db
    }

    #[test]
    fn conv_append_and_read_round_trips_in_order() {
        let db = db();
        let m = Uuid::new_v4();
        db.ensure_meeting_session(m, None).unwrap();
        db.conv_append(m, ConvRole::User, "what did we decide?")
            .unwrap();
        db.conv_append(m, ConvRole::Assistant, "shard by tenant")
            .unwrap();
        db.conv_append(m, ConvRole::User, "who owns it?").unwrap();

        let turns = db.conv_turns(m, 10).unwrap();
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, ConvRole::User);
        assert_eq!(turns[0].text, "what did we decide?");
        assert_eq!(turns[1].role, ConvRole::Assistant);
        assert_eq!(turns[2].text, "who owns it?");
    }

    #[test]
    fn conv_turns_limit_returns_newest_in_chronological_order() {
        let db = db();
        let m = Uuid::new_v4();
        db.ensure_meeting_session(m, None).unwrap();
        for i in 0..5 {
            db.conv_append(m, ConvRole::User, &format!("q{i}")).unwrap();
        }
        // Newest 2 = q3, q4, returned oldest→newest.
        let turns = db.conv_turns(m, 2).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].text, "q3");
        assert_eq!(turns[1].text, "q4");
    }

    #[test]
    fn conv_delete_oldest_removes_from_the_front() {
        let db = db();
        let m = Uuid::new_v4();
        db.ensure_meeting_session(m, None).unwrap();
        for i in 0..4 {
            db.conv_append(m, ConvRole::User, &format!("q{i}")).unwrap();
        }
        db.conv_delete_oldest(m, 2).unwrap();
        let turns = db.conv_turns(m, 10).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].text, "q2");
        assert_eq!(turns[1].text, "q3");
        // turn_idx keeps climbing — a new append must not collide with a
        // reused index (UNIQUE(session_id, turn_idx) would fail if it did).
        db.conv_append(m, ConvRole::User, "q4").unwrap();
        let turns = db.conv_turns(m, 10).unwrap();
        assert_eq!(turns.last().unwrap().text, "q4");
    }

    #[test]
    fn conv_prune_caps_to_max_turns() {
        let db = db();
        let m = Uuid::new_v4();
        db.ensure_meeting_session(m, None).unwrap();
        for i in 0..10 {
            db.conv_append(m, ConvRole::User, &format!("q{i}")).unwrap();
        }
        db.conv_prune(m, 3).unwrap();
        let turns = db.conv_turns(m, 100).unwrap();
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].text, "q7");
        assert_eq!(turns[2].text, "q9");
        // Prune below the count is a no-op-safe when already under the cap.
        db.conv_prune(m, 100).unwrap();
        assert_eq!(db.conv_turns(m, 100).unwrap().len(), 3);
    }

    #[test]
    fn conv_clear_empties_only_that_meeting() {
        let db = db();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        db.ensure_meeting_session(a, None).unwrap();
        db.ensure_meeting_session(b, None).unwrap();
        db.conv_append(a, ConvRole::User, "a1").unwrap();
        db.conv_append(b, ConvRole::User, "b1").unwrap();
        db.conv_clear(a).unwrap();
        assert!(db.conv_turns(a, 10).unwrap().is_empty());
        assert_eq!(db.conv_turns(b, 10).unwrap().len(), 1);
    }
}

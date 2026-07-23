use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use super::Database;

#[derive(Debug, Clone, Serialize)]
pub struct SpeakerMapping {
    pub session_id: String,
    pub speaker_id: i32,
    pub name: String,
    pub color: Option<String>,
}

impl Database {
    /// AUTO speaker labeling (the diarizer's "Speaker N" fallback or a
    /// cross-meeting voiceprint match). Records `user_set = 0` and, crucially,
    /// does NOT overwrite a row the USER has renamed (`user_set = 1`) — that is
    /// the fix for the rename flip-flop, where the live diarize tick kept
    /// clobbering the typed name. A user rename always wins over an auto label.
    pub fn set_speaker_name(
        &self,
        session_id: &str,
        speaker_id: i32,
        name: &str,
        color: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO speakers (session_id, speaker_id, name, color, user_set) \
             VALUES (?1, ?2, ?3, ?4, 0) \
             ON CONFLICT(session_id, speaker_id) DO UPDATE SET \
                name = excluded.name, \
                color = COALESCE(excluded.color, speakers.color) \
             WHERE speakers.user_set = 0",
            params![session_id, speaker_id, name, color],
        )?;
        Ok(())
    }

    /// USER-assigned speaker name (the "Reassign Speaker" rename). Records
    /// `user_set = 1` and always overwrites, so it wins over any prior auto
    /// label AND locks the row against future auto relabeling.
    pub fn set_speaker_name_by_user(
        &self,
        session_id: &str,
        speaker_id: i32,
        name: &str,
        color: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO speakers (session_id, speaker_id, name, color, user_set) \
             VALUES (?1, ?2, ?3, ?4, 1) \
             ON CONFLICT(session_id, speaker_id) DO UPDATE SET \
                name = excluded.name, \
                color = COALESCE(excluded.color, speakers.color), \
                user_set = 1",
            params![session_id, speaker_id, name, color],
        )?;
        Ok(())
    }

    /// The USER-assigned name for a speaker, if one exists (`user_set = 1`).
    /// Returns `None` for an auto label or an unnamed speaker — so callers can
    /// prefer the user's name over a freshly-computed "Speaker N". Cheap,
    /// prompt-free point lookup; safe on the live path.
    pub fn user_speaker_name(&self, session_id: &str, speaker_id: i32) -> Option<String> {
        self.conn
            .query_row(
                "SELECT name FROM speakers \
                 WHERE session_id = ?1 AND speaker_id = ?2 AND user_set = 1",
                params![session_id, speaker_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
    }

    /// All USER-renamed speakers (`user_set = 1`) for a session, as
    /// `(speaker_id, name)`. Used by the post-process reconciliation to carry a
    /// user's name from the LIVE gid it was typed against onto the authoritative
    /// post-pass id for the same voice (the rename-follows-the-voice fix).
    pub fn user_named_speakers(&self, session_id: &str) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT speaker_id, name FROM speakers \
             WHERE session_id = ?1 AND user_set = 1 ORDER BY speaker_id",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// List all speaker mappings for a session.
    pub fn list_speakers(&self, session_id: &str) -> Result<Vec<SpeakerMapping>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, speaker_id, name, color FROM speakers WHERE session_id = ?1 ORDER BY speaker_id",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(SpeakerMapping {
                session_id: row.get(0)?,
                speaker_id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

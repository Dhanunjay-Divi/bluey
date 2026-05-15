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
    /// Set or update a speaker name for a session.
    pub fn set_speaker_name(
        &self,
        session_id: &str,
        speaker_id: i32,
        name: &str,
        color: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO speakers (session_id, speaker_id, name, color) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(session_id, speaker_id) DO UPDATE SET name = excluded.name, color = COALESCE(excluded.color, speakers.color)",
            params![session_id, speaker_id, name, color],
        )?;
        Ok(())
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

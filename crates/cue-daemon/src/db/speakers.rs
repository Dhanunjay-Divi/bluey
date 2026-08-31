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
        self.set_speaker_name_for_owner(None, session_id, speaker_id, name, color)
    }

    pub fn set_speaker_name_for_owner(
        &self,
        owner_account_id: Option<&str>,
        session_id: &str,
        speaker_id: i32,
        name: &str,
        color: Option<&str>,
    ) -> Result<()> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let changed = self.conn.execute(
            "INSERT INTO speakers (session_id, speaker_id, name, color) \
             SELECT ?1, ?2, ?3, ?4 \
             WHERE EXISTS ( \
                 SELECT 1 FROM sessions \
                 WHERE sessions.id = ?1 AND sessions.owner_account_id IS ?5 \
             ) \
             ON CONFLICT(session_id, speaker_id) DO UPDATE SET \
                 name = excluded.name, \
                 color = COALESCE(excluded.color, speakers.color) \
             WHERE EXISTS ( \
                 SELECT 1 FROM sessions \
                 WHERE sessions.id = speakers.session_id \
                   AND sessions.owner_account_id IS ?5 \
             )",
            params![session_id, speaker_id, name, color, owner_account_id],
        )?;
        if changed == 0 {
            anyhow::bail!("session not found for requested owner scope");
        }
        Ok(())
    }

    /// List all speaker mappings for a session.
    pub fn list_speakers(&self, session_id: &str) -> Result<Vec<SpeakerMapping>> {
        self.list_speakers_for_owner(None, session_id)
    }

    pub fn list_speakers_for_owner(
        &self,
        owner_account_id: Option<&str>,
        session_id: &str,
    ) -> Result<Vec<SpeakerMapping>> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let mut stmt = self.conn.prepare(
            "SELECT speakers.session_id, speakers.speaker_id, speakers.name, speakers.color \
             FROM speakers \
             INNER JOIN sessions ON sessions.id = speakers.session_id \
             WHERE speakers.session_id = ?1 AND sessions.owner_account_id IS ?2 \
             ORDER BY speakers.speaker_id",
        )?;
        let rows = stmt.query_map(params![session_id, owner_account_id], |row| {
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

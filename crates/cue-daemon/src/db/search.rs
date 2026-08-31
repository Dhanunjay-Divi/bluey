use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptHit {
    pub session_id: String,
    pub text: String,
    pub snippet: String,
    pub source: String,
    pub ts: i64,
    pub rank: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptRow {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub source: String,
    pub speaker_id: Option<i32>,
    pub is_final: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub include_partials: bool,
    pub include_timestamps: bool,
    pub include_speakers: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_partials: false,
            include_timestamps: true,
            include_speakers: true,
        }
    }
}

impl Database {
    pub fn insert_transcript(
        &self,
        session_id: &str,
        text: &str,
        source: &str,
        speaker_id: Option<i32>,
        is_final: bool,
        created_at: i64,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO transcripts (id, session_id, text, source, speaker_id, is_final, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, session_id, text, source, speaker_id, is_final as i32, created_at],
        )?;
        Ok(id)
    }

    pub fn search_transcripts(&self, query: &str, limit: usize) -> Result<Vec<TranscriptHit>> {
        self.search_transcripts_for_owner(None, query, limit)
    }

    /// Search only transcript rows whose parent session belongs to the exact
    /// dashboard owner. `None` is the local namespace; it never includes a
    /// signed-in account's rows.
    pub fn search_transcripts_for_owner(
        &self,
        owner_account_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptHit>> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let mut stmt = self.conn.prepare(
            "SELECT transcript_fts.session_id, transcript_fts.text, \
                    snippet(transcript_fts, 2, '\u{00AB}', '\u{00BB}', '\u{2026}', 8), \
                    transcript_fts.source, transcript_fts.ts, transcript_fts.rank \
             FROM transcript_fts \
             INNER JOIN sessions ON sessions.id = transcript_fts.session_id \
             WHERE transcript_fts.text MATCH ?1 AND sessions.owner_account_id IS ?2 \
             ORDER BY transcript_fts.rank LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, owner_account_id, limit as i64], |row| {
            Ok(TranscriptHit {
                session_id: row.get(0)?,
                text: row.get(1)?,
                snippet: row.get(2)?,
                source: row.get(3)?,
                ts: row.get(4)?,
                rank: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn delete_transcript(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM transcripts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn list_transcripts(&self, session_id: &str) -> Result<Vec<TranscriptRow>> {
        self.list_transcripts_for_owner(None, session_id)
    }

    pub fn list_transcripts_for_owner(
        &self,
        owner_account_id: Option<&str>,
        session_id: &str,
    ) -> Result<Vec<TranscriptRow>> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let mut stmt = self.conn.prepare(
            "SELECT transcripts.id, transcripts.session_id, transcripts.text, \
                    transcripts.source, transcripts.speaker_id, transcripts.is_final, \
                    transcripts.created_at \
             FROM transcripts \
             INNER JOIN sessions ON sessions.id = transcripts.session_id \
             WHERE transcripts.session_id = ?1 AND sessions.owner_account_id IS ?2 \
             ORDER BY transcripts.created_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id, owner_account_id], |row| {
            Ok(TranscriptRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                text: row.get(2)?,
                source: row.get(3)?,
                speaker_id: row.get(4)?,
                is_final: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn export_session_markdown(
        &self,
        session_id: &str,
        opts: &ExportOptions,
    ) -> Result<String> {
        self.export_session_markdown_for_owner(None, session_id, opts)
    }

    pub fn export_session_markdown_for_owner(
        &self,
        owner_account_id: Option<&str>,
        session_id: &str,
        opts: &ExportOptions,
    ) -> Result<String> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let session = self
            .get_session_for_owner(owner_account_id, uuid::Uuid::parse_str(session_id)?)
            .context("session lookup")?
            .context("session not found")?;
        let transcripts = self.list_transcripts_for_owner(owner_account_id, session_id)?;
        let speakers = self.list_speakers_for_owner(owner_account_id, session_id)?;

        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", session.title));
        out.push_str(&format!("**Date:** {}\n\n", format_ts(session.created_at)));

        if !transcripts.is_empty() {
            out.push_str("## Transcript\n\n");
            for t in &transcripts {
                if !opts.include_partials && !t.is_final {
                    continue;
                }
                let speaker_label = if opts.include_speakers {
                    speaker_name(t.speaker_id, &speakers)
                } else {
                    String::new()
                };
                let ts_prefix = if opts.include_timestamps {
                    format!("[{}] ", format_ts(t.created_at))
                } else {
                    String::new()
                };
                if speaker_label.is_empty() {
                    out.push_str(&format!("{}{}\n\n", ts_prefix, t.text));
                } else {
                    out.push_str(&format!(
                        "{}**{}** ({}): {}\n\n",
                        ts_prefix, speaker_label, t.source, t.text
                    ));
                }
            }
        }
        Ok(out)
    }

    pub fn export_session_text(&self, session_id: &str) -> Result<String> {
        self.export_session_text_for_owner(None, session_id)
    }

    pub fn export_session_text_for_owner(
        &self,
        owner_account_id: Option<&str>,
        session_id: &str,
    ) -> Result<String> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let session = self
            .get_session_for_owner(owner_account_id, uuid::Uuid::parse_str(session_id)?)
            .context("session lookup")?
            .context("session not found")?;
        let transcripts = self.list_transcripts_for_owner(owner_account_id, session_id)?;
        let speakers = self.list_speakers_for_owner(owner_account_id, session_id)?;

        let mut out = String::new();
        out.push_str(&format!("{}\n", session.title));
        out.push_str(&format!("Date: {}\n\n", format_ts(session.created_at)));
        for t in &transcripts {
            if !t.is_final {
                continue;
            }
            let name = speaker_name(t.speaker_id, &speakers);
            if name.is_empty() {
                out.push_str(&format!("[{}] {}\n", t.source, t.text));
            } else {
                out.push_str(&format!("[{}] {}: {}\n", t.source, name, t.text));
            }
        }
        Ok(out)
    }

    pub fn export_session_json(&self, session_id: &str) -> Result<String> {
        self.export_session_json_for_owner(None, session_id)
    }

    pub fn export_session_json_for_owner(
        &self,
        owner_account_id: Option<&str>,
        session_id: &str,
    ) -> Result<String> {
        let owner_account_id = owner_account_id
            .map(super::validate_cloud_owner_account_id)
            .transpose()?;
        let session = self
            .get_session_for_owner(owner_account_id, uuid::Uuid::parse_str(session_id)?)
            .context("session lookup")?
            .context("session not found")?;
        let transcripts = self.list_transcripts_for_owner(owner_account_id, session_id)?;
        let speakers = self.list_speakers_for_owner(owner_account_id, session_id)?;

        #[derive(Serialize)]
        struct Export {
            title: String,
            created_at: i64,
            transcripts: Vec<TranscriptRow>,
            speakers: Vec<super::speakers::SpeakerMapping>,
        }

        let export = Export {
            title: session.title,
            created_at: session.created_at,
            transcripts,
            speakers,
        };
        serde_json::to_string_pretty(&export).map_err(Into::into)
    }
}

fn speaker_name(speaker_id: Option<i32>, speakers: &[super::speakers::SpeakerMapping]) -> String {
    match speaker_id {
        Some(id) => speakers
            .iter()
            .find(|s| s.speaker_id == id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("Speaker {}", id)),
        None => String::new(),
    }
}

fn format_ts(ms: i64) -> String {
    let secs = (ms / 1000) as u64;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

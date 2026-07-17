//! Diarization persistence: per-utterance voice embeddings + per-meeting
//! resolved speakers (migration 010). Embeddings are stored as little-endian
//! f32 BLOBs. See `010_diarization.sql`.

use anyhow::Result;
use rusqlite::params;

use super::Database;

/// A stored utterance embedding (one speaker's speech span).
#[derive(Debug, Clone)]
pub struct UtteranceRow {
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub source: String,
    pub speaker_prov: Option<i64>,
    pub speaker_final: Option<i64>,
    pub embedding: Vec<f32>,
}

/// Serialize an f32 embedding to a little-endian byte BLOB.
pub fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// Deserialize a little-endian f32 BLOB back to an embedding.
pub fn blob_to_embedding(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Database {
    /// Insert one utterance embedding (live pass). Returns the row id.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_utterance(
        &self,
        session_id: &str,
        source: &str,
        start_ms: i64,
        end_ms: i64,
        speaker_prov: Option<i64>,
        embedding: &[f32],
        created_at: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO utterance
                (session_id, source, start_ms, end_ms, speaker_prov,
                 speaker_final, embed_dim, embedding, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8)",
            params![
                session_id,
                source,
                start_ms,
                end_ms,
                speaker_prov,
                embedding.len() as i64,
                embedding_to_blob(embedding),
                created_at,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Load all utterances for a session, ordered by start time.
    pub fn load_utterances(&self, session_id: &str) -> Result<Vec<UtteranceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_ms, end_ms, source, speaker_prov, speaker_final, embedding
             FROM utterance WHERE session_id = ?1 ORDER BY start_ms",
        )?;
        let rows = stmt
            .query_map(params![session_id], |r| {
                let blob: Vec<u8> = r.get(6)?;
                Ok(UtteranceRow {
                    id: r.get(0)?,
                    start_ms: r.get(1)?,
                    end_ms: r.get(2)?,
                    source: r.get(3)?,
                    speaker_prov: r.get(4)?,
                    speaker_final: r.get(5)?,
                    embedding: blob_to_embedding(&blob),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Set the authoritative (post-pass) speaker id on an utterance.
    pub fn set_utterance_final_speaker(&self, id: i64, speaker_final: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE utterance SET speaker_final = ?2 WHERE id = ?1",
            params![id, speaker_final],
        )?;
        Ok(())
    }

    /// Upsert a per-meeting resolved speaker (centroid + counts).
    pub fn upsert_meeting_speaker(
        &self,
        session_id: &str,
        speaker_final: i64,
        centroid: &[f32],
        n_utterances: i64,
        total_ms: i64,
        updated_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meeting_speaker
                (session_id, speaker_final, centroid, embed_dim, n_utterances, total_ms, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id, speaker_final) DO UPDATE SET
                centroid = excluded.centroid,
                embed_dim = excluded.embed_dim,
                n_utterances = excluded.n_utterances,
                total_ms = excluded.total_ms,
                updated_at = excluded.updated_at",
            params![
                session_id,
                speaker_final,
                embedding_to_blob(centroid),
                centroid.len() as i64,
                n_utterances,
                total_ms,
                updated_at,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let v = vec![0.1_f32, -0.5, 1.0, 0.0];
        assert_eq!(blob_to_embedding(&embedding_to_blob(&v)), v);
    }

    // A meeting id is NOT an agent-session id: meetings are stored as JSON files,
    // never inserted into `sessions`. Persisting diarization for such an id would
    // fail the `utterance` / `meeting_speaker` FK to `sessions(id)` unless a
    // parent row is ensured first. This proves that after `ensure_meeting_session`
    // (idempotent, as the daemon does before persisting), both inserts succeed.
    #[test]
    fn diarize_insert_succeeds_for_non_agent_meeting_id() {
        let db = Database::open(":memory:").expect("open in-memory db");

        // A brand-new meeting id that was never created via `create_session`.
        let meeting_id = uuid::Uuid::new_v4();
        let sid = meeting_id.to_string();
        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4];

        // Sanity: without a parent sessions row the FK must reject the insert.
        assert!(
            db.insert_utterance(&sid, "system", 0, 1000, Some(0), &embedding, 42)
                .is_err(),
            "insert_utterance should fail the FK when no sessions row exists"
        );

        // Ensure the placeholder parent row (idempotent — call twice).
        assert!(
            db.ensure_meeting_session(meeting_id, None)
                .expect("ensure_meeting_session"),
            "first ensure should create the row"
        );
        assert!(
            !db.ensure_meeting_session(meeting_id, None)
                .expect("ensure_meeting_session idempotent"),
            "second ensure must be a no-op, not clobber the row"
        );

        // Now both diarization writes must succeed.
        let row_id = db
            .insert_utterance(&sid, "system", 0, 1000, Some(0), &embedding, 42)
            .expect("insert_utterance should succeed after ensure_meeting_session");
        db.set_utterance_final_speaker(row_id, 0)
            .expect("set_utterance_final_speaker");
        db.upsert_meeting_speaker(&sid, 0, &embedding, 1, 1000, 42)
            .expect("upsert_meeting_speaker should succeed after ensure_meeting_session");

        // Round-trip the utterance back out to confirm it persisted.
        let rows = db.load_utterances(&sid).expect("load_utterances");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].speaker_final, Some(0));
        assert_eq!(rows[0].embedding, embedding);
    }

    // `ensure_meeting_session` must never clobber a real agent session that
    // happens to share the id space (both are UUID strings in `sessions`).
    #[test]
    fn ensure_meeting_session_preserves_existing_session() {
        let db = Database::open(":memory:").expect("open in-memory db");
        let session = db
            .create_session(Some("Agent session".into()))
            .expect("create_session");

        // Ensuring the same id must be a no-op and must NOT rename it.
        assert!(
            !db.ensure_meeting_session(session.id, Some("Meeting"))
                .expect("ensure_meeting_session"),
            "ensure on an existing session id must not insert"
        );
        let fetched = db
            .get_session(session.id)
            .expect("get_session")
            .expect("session still present");
        assert_eq!(fetched.title, "Agent session");
    }
}

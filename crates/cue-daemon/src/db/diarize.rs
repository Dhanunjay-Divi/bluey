//! Diarization persistence: per-utterance voice embeddings + per-meeting
//! resolved speakers (migration 010). Embeddings are stored as little-endian
//! f32 BLOBs. See `010_diarization.sql`.

use anyhow::Result;
use rusqlite::params;

use super::Database;

/// Cap on how many recent named voiceprints cross-meeting matching scans. A
/// user has at most a few dozen recurring named colleagues; bounding the scan
/// (newest-first) keeps matching O(recent) rather than O(all speakers ever), so
/// a heavy user with thousands of past meetings still matches in microseconds.
const MAX_HISTORICAL_VOICEPRINTS: i64 = 200;

/// Minimum speaking time (ms) a prior-meeting speaker must have accrued for its
/// centroid to be a reliable, matchable voiceprint. MEASURED on real audio
/// (VoxConverse, `examples/cross_meeting_real.rs`): speakers with ample speech
/// self-match at 0.87–0.99, but a centroid from only a second or two of speech
/// is noisy and scores 0.2–0.7 even against the same voice — so enrolling it
/// would cause both misses and false matches. 3s mirrors the diarizer's own
/// `min_enroll_secs` (ProfileBank).
const MIN_VOICEPRINT_SPEECH_MS: i64 = 3000;

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

    /// Load all historical speaker centroids that have user-assigned names.
    /// Load REAL named voiceprints from prior meetings, for cross-meeting
    /// matching. Bounded and filtered for production:
    /// - excludes the current session (`session_id != ?1`),
    /// - requires a real user-assigned name — NOT the auto-fallback `Speaker N`
    ///   labels (matching those would propagate meaningless names across
    ///   meetings),
    /// - newest-first and capped at `MAX_HISTORICAL_VOICEPRINTS`, so the scan
    ///   stays O(recent colleagues) instead of O(all speakers ever), which keeps
    ///   a heavy user with thousands of past meetings fast.
    pub fn load_historical_voiceprints(
        &self,
        current_session_id: &str,
    ) -> Result<Vec<HistoricalVoiceprint>> {
        let mut stmt = self.conn.prepare(
            "SELECT ms.session_id, ms.speaker_final, s.name, ms.centroid \
             FROM meeting_speaker ms \
             JOIN speakers s ON ms.session_id = s.session_id AND ms.speaker_final = s.speaker_id \
             WHERE ms.session_id != ?1 \
               AND s.name != '' \
               AND s.name NOT GLOB 'Speaker [0-9]*' \
               AND ms.total_ms >= ?3 \
             ORDER BY ms.updated_at DESC \
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(
                params![
                    current_session_id,
                    MAX_HISTORICAL_VOICEPRINTS,
                    MIN_VOICEPRINT_SPEECH_MS
                ],
                |r| {
                    let blob: Vec<u8> = r.get(3)?;
                    Ok(HistoricalVoiceprint {
                        session_id: r.get(0)?,
                        speaker_id: r.get(1)?,
                        name: r.get(2)?,
                        centroid: blob_to_embedding(&blob),
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Match a voice centroid against historical voiceprints with a similarity threshold.
    pub fn match_speaker_voiceprint(
        &self,
        current_session_id: &str,
        centroid: &[f32],
        threshold: f32,
    ) -> Option<String> {
        let voiceprints = self.load_historical_voiceprints(current_session_id).ok()?;
        let mut best_match: Option<(String, f32)> = None;
        for vp in &voiceprints {
            let sim = cosine_similarity(centroid, &vp.centroid);
            if sim >= threshold {
                match &best_match {
                    Some((_, best_sim)) if sim > *best_sim => {
                        best_match = Some((vp.name.clone(), sim));
                    }
                    None => {
                        best_match = Some((vp.name.clone(), sim));
                    }
                    _ => {}
                }
            }
        }
        best_match.map(|(name, _)| name)
    }

    /// Merge two speaker centroids for a session.
    pub fn merge_speaker_centroids(
        &self,
        session_id: &str,
        source_speaker_id: i64,
        target_speaker_id: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE utterance SET speaker_final = ?3 WHERE session_id = ?1 AND speaker_final = ?2",
            params![session_id, source_speaker_id, target_speaker_id],
        )?;
        self.conn.execute(
            "DELETE FROM meeting_speaker WHERE session_id = ?1 AND speaker_final = ?2",
            params![session_id, source_speaker_id],
        )?;
        Ok(())
    }
}

/// A historical speaker voiceprint record.
#[derive(Debug, Clone)]
pub struct HistoricalVoiceprint {
    pub session_id: String,
    pub speaker_id: i64,
    pub name: String,
    pub centroid: Vec<f32>,
}

/// Compute cosine similarity between two float vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
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

    #[test]
    fn test_cosine_similarity_math() {
        let v1 = vec![1.0_f32, 0.0, 0.0];
        let v2 = vec![1.0_f32, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 1e-5);

        let v3 = vec![0.0_f32, 1.0, 0.0];
        assert!((cosine_similarity(&v1, &v3) - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_cross_meeting_voiceprint_matching() {
        let db = Database::open(":memory:").unwrap();
        let session1 = uuid::Uuid::new_v4();
        let session2 = uuid::Uuid::new_v4();
        let s1 = session1.to_string();
        let s2 = session2.to_string();

        db.ensure_meeting_session(session1, None).unwrap();
        db.ensure_meeting_session(session2, None).unwrap();

        let centroid_sarah = vec![0.8_f32, 0.2, 0.1];
        db.upsert_meeting_speaker(&s1, 1, &centroid_sarah, 10, 5000, 100)
            .unwrap();
        db.set_speaker_name(&s1, 1, "Sarah Jenkins", None).unwrap();

        // Query from session 2 with similar voice vector
        let incoming = vec![0.81_f32, 0.19, 0.1];
        let matched = db.match_speaker_voiceprint(&s2, &incoming, 0.80);
        assert_eq!(matched, Some("Sarah Jenkins".to_string()));
    }

    /// Harder cross-meeting test: several PRIOR meetings each enroll a named
    /// speaker; a new meeting must (1) pick the RIGHT name among distractors,
    /// (2) NOT match an unnamed speaker, (3) NOT match a genuinely different
    /// voice, (4) never match a speaker from the CURRENT session. This exercises
    /// the real failure modes the near-identical single-speaker test can't.
    #[test]
    fn cross_meeting_voiceprint_picks_right_name_among_distractors() {
        let db = Database::open(":memory:").unwrap();
        // A helper to make a normalized-ish 8-dim voiceprint (closer to the real
        // WeSpeaker embedding shape than a 3-vector, so cosine behaves realistically).
        let vp = |seed: [f32; 8]| seed.to_vec();

        let m1 = uuid::Uuid::new_v4();
        let m2 = uuid::Uuid::new_v4();
        let m3 = uuid::Uuid::new_v4();
        let now = uuid::Uuid::new_v4(); // the CURRENT meeting
        for id in [m1, m2, m3, now] {
            db.ensure_meeting_session(id, None).unwrap();
        }
        let (m1, m2, m3, cur) = (
            m1.to_string(),
            m2.to_string(),
            m3.to_string(),
            now.to_string(),
        );

        // Meeting 1: Sarah (named).
        let sarah = vp([0.90, 0.10, 0.05, 0.02, 0.01, 0.00, 0.00, 0.00]);
        db.upsert_meeting_speaker(&m1, 1, &sarah, 12, 6000, 100)
            .unwrap();
        db.set_speaker_name(&m1, 1, "Sarah Jenkins", None).unwrap();

        // Meeting 2: Alex (named), clearly different voice.
        let alex = vp([0.05, 0.05, 0.90, 0.10, 0.02, 0.00, 0.00, 0.00]);
        db.upsert_meeting_speaker(&m2, 1, &alex, 20, 9000, 200)
            .unwrap();
        db.set_speaker_name(&m2, 1, "Alex Vance", None).unwrap();

        // Meeting 3: an UNNAMED speaker (still "Speaker 2" — no real name). Must
        // never be returned even if the voice is close, because the JOIN requires
        // s.name != ''.
        let unnamed = vp([0.88, 0.12, 0.06, 0.03, 0.00, 0.00, 0.00, 0.00]);
        db.upsert_meeting_speaker(&m3, 2, &unnamed, 5, 2500, 300)
            .unwrap();
        // deliberately NOT set_speaker_name → stays unnamed.

        // (1) A voice close to Sarah, queried from the CURRENT meeting → Sarah,
        // NOT the near-identical unnamed speaker from meeting 3.
        let incoming_sarah = vp([0.89, 0.11, 0.05, 0.02, 0.01, 0.00, 0.00, 0.00]);
        assert_eq!(
            db.match_speaker_voiceprint(&cur, &incoming_sarah, 0.80),
            Some("Sarah Jenkins".to_string()),
            "must match the NAMED Sarah, not the unnamed near-identical voice"
        );

        // (2) A voice close to Alex → Alex (right pick among distractors).
        let incoming_alex = vp([0.06, 0.05, 0.89, 0.11, 0.02, 0.00, 0.00, 0.00]);
        assert_eq!(
            db.match_speaker_voiceprint(&cur, &incoming_alex, 0.80),
            Some("Alex Vance".to_string()),
        );

        // (3) A genuinely different voice → no match (below threshold).
        let stranger = vp([0.00, 0.00, 0.00, 0.00, 0.10, 0.90, 0.30, 0.20]);
        assert_eq!(
            db.match_speaker_voiceprint(&cur, &stranger, 0.80),
            None,
            "an unknown voice must not be forced onto a known name"
        );

        // (4) The SAME voice, but querying from Sarah's OWN meeting (m1), must
        // NOT self-match (the query excludes the current session).
        assert_eq!(
            db.match_speaker_voiceprint(&m1, &incoming_sarah, 0.80),
            None,
            "a speaker must not match themselves within their own meeting"
        );
    }

    /// Production guard: a speaker carrying only the AUTO-FALLBACK label
    /// ("Speaker 3") must NOT be matched cross-meeting — inheriting a fallback
    /// label across meetings would propagate meaningless names. Only real,
    /// user-assigned names are matchable.
    #[test]
    fn fallback_speaker_labels_are_never_matched_cross_meeting() {
        let db = Database::open(":memory:").unwrap();
        let prior = uuid::Uuid::new_v4();
        let cur = uuid::Uuid::new_v4();
        db.ensure_meeting_session(prior, None).unwrap();
        db.ensure_meeting_session(cur, None).unwrap();
        let (p, c) = (prior.to_string(), cur.to_string());

        // A prior meeting where the speaker was only auto-labelled (never named
        // by the user) — exactly what the live path writes for unmatched voices.
        let voice = vec![0.90_f32, 0.10, 0.05, 0.02, 0.01, 0.00, 0.00, 0.00];
        db.upsert_meeting_speaker(&p, 3, &voice, 8, 4000, 100)
            .unwrap();
        db.set_speaker_name(&p, 3, "Speaker 4", None).unwrap(); // fallback label

        // The identical voice in a new meeting must NOT inherit "Speaker 4".
        let same = vec![0.90_f32, 0.10, 0.05, 0.02, 0.01, 0.00, 0.00, 0.00];
        assert_eq!(
            db.match_speaker_voiceprint(&c, &same, 0.80),
            None,
            "a fallback 'Speaker N' label must not propagate across meetings"
        );

        // But once the user gives them a REAL name, the same voice matches it.
        db.set_speaker_name(&p, 3, "Jordan Lee", None).unwrap();
        assert_eq!(
            db.match_speaker_voiceprint(&c, &same, 0.80),
            Some("Jordan Lee".to_string()),
            "a real user-assigned name is matchable cross-meeting"
        );
    }

    /// Production guard (MEASURED on real audio): a named speaker backed by too
    /// little speech (< MIN_VOICEPRINT_SPEECH_MS) is NOT matchable — its centroid
    /// is too noisy to trust. Real VoxConverse voiceprints from <3s of speech
    /// self-matched at only 0.2–0.7, so enrolling them causes misses AND false
    /// positives.
    #[test]
    fn thin_voiceprints_are_not_matchable() {
        let db = Database::open(":memory:").unwrap();
        let prior = uuid::Uuid::new_v4();
        let cur = uuid::Uuid::new_v4();
        db.ensure_meeting_session(prior, None).unwrap();
        db.ensure_meeting_session(cur, None).unwrap();
        let (p, c) = (prior.to_string(), cur.to_string());
        let voice = vec![0.90_f32, 0.10, 0.05, 0.02, 0.01, 0.00, 0.00, 0.00];

        // Named, but only 1.5s of speech → below the 3s reliability floor.
        db.upsert_meeting_speaker(&p, 1, &voice, 3, 1500, 100)
            .unwrap();
        db.set_speaker_name(&p, 1, "Casey Morgan", None).unwrap();
        assert_eq!(
            db.match_speaker_voiceprint(&c, &voice, 0.80),
            None,
            "a voiceprint from too little speech must not be matchable"
        );

        // Same speaker later accrues enough speech → now matchable.
        db.upsert_meeting_speaker(&p, 1, &voice, 20, 12000, 200)
            .unwrap();
        assert_eq!(
            db.match_speaker_voiceprint(&c, &voice, 0.80),
            Some("Casey Morgan".to_string()),
            "once backed by enough speech, the voiceprint becomes matchable"
        );
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

-- 010_diarization.sql — on-device speaker diarization: per-utterance voice
-- embeddings + per-meeting resolved speakers.
--
-- The diarization speaker id is a per-session integer, ORTHOGONAL to the coarse
-- mic-vs-system channel tag on transcripts. `transcripts.speaker_id` (added in
-- an earlier migration) holds the resolved id for display/AI-context. These
-- tables hold the evidence (embeddings) + the resolved-speaker centroids so the
-- post-meeting pass can re-cluster and reconcile, and a future cross-meeting
-- gallery can re-identify a returning speaker.
--
-- Conventions match the rest of the schema: TEXT session_id FKs with ON DELETE
-- CASCADE, integer epoch-ms timestamps, embeddings as little-endian f32 BLOBs.

-- One row per diarized utterance (a speaker's continuous speech span).
CREATE TABLE IF NOT EXISTS utterance (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id     TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    source         TEXT NOT NULL,          -- 'system' | 'mic'
    start_ms       INTEGER NOT NULL,       -- relative to meeting start
    end_ms         INTEGER NOT NULL,
    speaker_prov   INTEGER,                -- live (provisional) speaker id
    speaker_final  INTEGER,                -- post-pass authoritative id; NULL until run
    embed_dim      INTEGER NOT NULL,       -- embedding dimensionality (model-dependent)
    embedding      BLOB NOT NULL,          -- embed_dim little-endian f32, L2-normalized
    created_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_utterance_session ON utterance(session_id, start_ms);

-- Per-meeting resolved speaker: the centroid + bookkeeping produced by the
-- backend re-cluster pass. `speaker_final` is the arrival-time-ordered id and is
-- what `transcripts.speaker_id` / `speakers(session_id, speaker_id)` reference.
CREATE TABLE IF NOT EXISTS meeting_speaker (
    session_id     TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    speaker_final  INTEGER NOT NULL,
    centroid       BLOB NOT NULL,          -- embed_dim little-endian f32, L2-normalized
    embed_dim      INTEGER NOT NULL,
    n_utterances   INTEGER NOT NULL DEFAULT 0,
    total_ms       INTEGER NOT NULL DEFAULT 0,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (session_id, speaker_final)
);

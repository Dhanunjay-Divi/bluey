-- Phase 3 Round 5: Full-text search on transcripts.

CREATE TABLE IF NOT EXISTS transcripts (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    text        TEXT NOT NULL,
    source      TEXT NOT NULL DEFAULT 'mic',
    speaker_id  INTEGER,
    is_final    INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_transcripts_session ON transcripts(session_id, created_at);

-- FTS5 virtual table (standalone, not content-synced for simplicity).
CREATE VIRTUAL TABLE IF NOT EXISTS transcript_fts USING fts5(
    session_id UNINDEXED,
    text,
    source UNINDEXED,
    ts UNINDEXED
);

-- Auto-index new transcripts into FTS5.
CREATE TRIGGER IF NOT EXISTS trg_transcripts_ai AFTER INSERT ON transcripts BEGIN
    INSERT INTO transcript_fts(session_id, text, source, ts)
    VALUES (new.session_id, new.text, new.source, new.created_at);
END;

-- Auto-remove from FTS on delete.
CREATE TRIGGER IF NOT EXISTS trg_transcripts_ad AFTER DELETE ON transcripts BEGIN
    DELETE FROM transcript_fts WHERE rowid = old.rowid;
END;

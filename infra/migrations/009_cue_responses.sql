-- Phase 3 Round 9: AI cue responses table.

CREATE TABLE IF NOT EXISTS cue_responses (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,  -- 'answer', 'recap', 'suggestion'
    text            TEXT NOT NULL,
    source_text     TEXT,
    ts_ms           INTEGER NOT NULL,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE INDEX IF NOT EXISTS idx_cue_responses_session ON cue_responses(session_id, ts_ms DESC);

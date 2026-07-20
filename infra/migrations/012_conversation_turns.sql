-- 012_conversation_turns.sql — app-owned in-meeting conversation memory.
--
-- Bluey stores every in-meeting Q&A turn itself (the user's visible question +
-- the copilot's post-guard answer), so it can re-supply the running dialogue to
-- the agent each turn without depending on the agent's own resumable session.
-- This is the foundation for driving agents ephemerally (Wave 3) while keeping
-- "follow up on that" working: the conversation lives in OUR store, not the
-- agent's chat history.
--
-- The rolling conversation SUMMARY (older turns folded down by the stateless
-- one-shot) is held in daemon memory for Wave 1 — the raw turns here are the
-- durable source of truth and are re-foldable after a restart.
--
-- Conventions match the schema: TEXT session_id FK with ON DELETE CASCADE (a
-- meeting id, backed by a placeholder `sessions` row via ensure_meeting_session
-- — the same FK discipline the diarization tables use, per commit 338f1e9),
-- integer epoch-ms timestamps, a per-meeting monotonic turn_idx.

CREATE TABLE IF NOT EXISTS conversation_turns (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_idx     INTEGER NOT NULL,       -- per-meeting monotonic order (0-based)
    role         TEXT NOT NULL,          -- 'user' | 'assistant'
    text         TEXT NOT NULL,
    epoch_secs   INTEGER NOT NULL,       -- wall-clock seconds
    UNIQUE (session_id, turn_idx)
);
CREATE INDEX IF NOT EXISTS idx_conversation_turns_session
    ON conversation_turns(session_id, turn_idx);

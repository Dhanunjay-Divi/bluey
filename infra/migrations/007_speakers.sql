-- Phase 3 Round 5: Speaker name mapping per session.

CREATE TABLE IF NOT EXISTS speakers (
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    speaker_id  INTEGER NOT NULL,
    name        TEXT NOT NULL,
    color       TEXT,
    PRIMARY KEY (session_id, speaker_id)
);

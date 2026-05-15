-- Migration 008: Fix FTS5 delete trigger consistency.
-- The trigger in 006 uses old.rowid which does not match FTS5 standalone rowids.
-- Fix: rebuild FTS table with transcript_id for reliable delete matching.

-- Drop broken triggers from 006
DROP TRIGGER IF EXISTS trg_transcripts_ai;
DROP TRIGGER IF EXISTS trg_transcripts_ad;

-- Recreate FTS table with transcript_id for reliable matching
DROP TABLE IF EXISTS transcript_fts;
CREATE VIRTUAL TABLE IF NOT EXISTS transcript_fts USING fts5(
    transcript_id UNINDEXED,
    session_id UNINDEXED,
    text,
    source UNINDEXED,
    ts UNINDEXED
);

-- Repopulate from existing transcripts
INSERT INTO transcript_fts(transcript_id, session_id, text, source, ts)
SELECT id, session_id, text, source, created_at FROM transcripts;

-- Correct INSERT trigger
CREATE TRIGGER IF NOT EXISTS trg_transcripts_ai AFTER INSERT ON transcripts BEGIN
    INSERT INTO transcript_fts(transcript_id, session_id, text, source, ts)
    VALUES (new.id, new.session_id, new.text, new.source, new.created_at);
END;

-- Correct DELETE trigger using transcript_id for exact match
CREATE TRIGGER IF NOT EXISTS trg_transcripts_ad AFTER DELETE ON transcripts BEGIN
    DELETE FROM transcript_fts WHERE transcript_id = old.id;
END;

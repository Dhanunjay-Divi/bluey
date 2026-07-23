-- Distinguish a USER-assigned speaker name from an AUTO label (the diarizer's
-- "Speaker N" fallback or a cross-meeting voiceprint match). Without this, the
-- live/post-process diarize passes overwrite a name the user typed, causing the
-- rename to flip-flop back to "Speaker N".
--
-- Default 0 (auto): every existing row and every diarizer-written row is auto;
-- only an explicit user rename sets it to 1, and auto passes must not clobber a
-- row where user_set = 1.
ALTER TABLE speakers ADD COLUMN user_set INTEGER NOT NULL DEFAULT 0;

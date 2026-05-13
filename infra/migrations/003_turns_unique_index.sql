-- Enforce uniqueness of (session_id, turn_index) to prevent race conditions.
CREATE UNIQUE INDEX IF NOT EXISTS idx_turns_session_turn_index ON turns(session_id, turn_index);

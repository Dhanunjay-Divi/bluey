-- Scope local session persistence to either a signed-in account or the
-- account-independent local namespace. The owner column is added to existing
-- databases by Database::ensure_session_owner_column before this runs.

CREATE INDEX IF NOT EXISTS idx_sessions_owner_updated
    ON sessions(owner_account_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_sessions_owner_status_updated
    ON sessions(owner_account_id, status, updated_at DESC);

-- All rows that predate account scoping are local, so the old global active
-- pointer belongs to the local namespace as well. Preserve an already-written
-- scoped pointer if one exists.
INSERT OR IGNORE INTO app_state (key, value, updated_at)
SELECT 'active_session_id:local', value, updated_at
FROM app_state
WHERE key = 'active_session_id';

DELETE FROM app_state WHERE key = 'active_session_id';

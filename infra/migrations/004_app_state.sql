-- Phase 3 follow-up: persist daemon-side app state (active session id, etc.)
-- across restarts. Simple key-value shape so this table stays general-purpose;
-- we'll add more keys in future phases (last-opened URL, onboarding flag, etc.)
-- rather than introducing a new table each time.

CREATE TABLE IF NOT EXISTS app_state (
    key         TEXT PRIMARY KEY,
    value       TEXT,
    updated_at  INTEGER NOT NULL
);

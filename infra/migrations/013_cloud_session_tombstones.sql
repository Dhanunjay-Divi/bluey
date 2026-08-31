-- Durable local fence for account-scoped cloud session deletions.
--
-- A cloud tombstone must outlive the immediate MeetingStore cleanup so a late
-- projection write or a restart cannot recreate a deleted account session.

CREATE TABLE IF NOT EXISTS cloud_session_tombstones (
    owner_account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    deleted_at_ms INTEGER NOT NULL,
    PRIMARY KEY (owner_account_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_cloud_session_tombstones_deleted
    ON cloud_session_tombstones(deleted_at_ms DESC);

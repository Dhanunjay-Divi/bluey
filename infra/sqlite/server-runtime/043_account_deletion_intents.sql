-- Target: SQLite
-- Durable account-deletion write fence and upload drain status.
CREATE TABLE IF NOT EXISTS account_deletion_intents (
  account_id             TEXT PRIMARY KEY
    REFERENCES accounts(id) ON DELETE CASCADE,
  requested_at_ms        INTEGER NOT NULL CHECK(requested_at_ms >= 0),
  last_checked_at_ms     INTEGER NOT NULL CHECK(last_checked_at_ms >= 0),
  fresh_upload_cutoff_ms INTEGER NOT NULL,
  fresh_in_flight_puts   INTEGER NOT NULL DEFAULT 0
    CHECK(fresh_in_flight_puts >= 0)
);

-- Target: Postgres
-- Durable, tenant-scoped mailbox ingestion for Bluey Jobs.
--
-- Provider cursors, message content, and processing metadata are encrypted by
-- the Jobs data key before persistence. Only lookup hashes and scheduling
-- fields remain queryable.

CREATE TABLE IF NOT EXISTS jobs_provider_sync_state (
  connection_id       TEXT PRIMARY KEY REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  provider            TEXT NOT NULL,
  sync_json           TEXT NOT NULL,
  next_sync_at_ms     BIGINT NOT NULL,
  last_synced_at_ms   BIGINT,
  lease_owner         TEXT,
  lease_expires_at_ms BIGINT,
  created_at_ms       BIGINT NOT NULL,
  updated_at_ms       BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_provider_sync_due
  ON jobs_provider_sync_state(next_sync_at_ms, lease_expires_at_ms);
CREATE INDEX IF NOT EXISTS idx_jobs_provider_sync_account
  ON jobs_provider_sync_state(account_id, provider, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_provider_messages (
  id                    TEXT PRIMARY KEY,
  account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  connection_id         TEXT NOT NULL REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
  provider              TEXT NOT NULL,
  provider_message_hash TEXT NOT NULL,
  application_id        TEXT REFERENCES jobs_applications(id) ON DELETE SET NULL,
  processing_status     TEXT NOT NULL,
  message_json          TEXT NOT NULL,
  received_at_ms        BIGINT NOT NULL,
  processed_at_ms       BIGINT,
  created_at_ms         BIGINT NOT NULL,
  updated_at_ms         BIGINT NOT NULL,
  UNIQUE(account_id, provider, provider_message_hash)
);

CREATE INDEX IF NOT EXISTS idx_jobs_provider_messages_account
  ON jobs_provider_messages(account_id, received_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_provider_messages_application
  ON jobs_provider_messages(account_id, application_id, received_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_provider_messages_status
  ON jobs_provider_messages(account_id, processing_status, updated_at_ms DESC);

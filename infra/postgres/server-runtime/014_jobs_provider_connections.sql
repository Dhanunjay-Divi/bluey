-- Target: Postgres
-- Server-only OAuth state and provider credentials for Bluey Jobs.
--
-- These records never appear in the customer workspace payload. Both state
-- details and provider credentials are encrypted by the Jobs data key before
-- persistence.

CREATE TABLE IF NOT EXISTS jobs_oauth_states (
  state_hash       TEXT PRIMARY KEY,
  account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  provider         TEXT NOT NULL,
  state_json       TEXT NOT NULL,
  expires_at_ms    BIGINT NOT NULL,
  created_at_ms    BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_oauth_states_expiry
  ON jobs_oauth_states(expires_at_ms);

CREATE TABLE IF NOT EXISTS jobs_provider_credentials (
  connection_id        TEXT PRIMARY KEY REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
  account_id           TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  provider             TEXT NOT NULL,
  provider_subject_hash TEXT NOT NULL,
  credential_json      TEXT NOT NULL,
  created_at_ms        BIGINT NOT NULL,
  updated_at_ms        BIGINT NOT NULL,
  UNIQUE(account_id, provider, provider_subject_hash)
);

CREATE INDEX IF NOT EXISTS idx_jobs_provider_credentials_account
  ON jobs_provider_credentials(account_id, provider, updated_at_ms DESC);

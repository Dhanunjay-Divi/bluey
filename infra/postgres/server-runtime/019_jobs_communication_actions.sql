-- Target: PostgreSQL
-- Approval-gated, replay-safe email replies and calendar actions.
CREATE TABLE IF NOT EXISTS jobs_communication_actions (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id      TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  connection_id       TEXT NOT NULL REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
  source_message_id   TEXT REFERENCES jobs_provider_messages(id) ON DELETE SET NULL,
  kind                TEXT NOT NULL CHECK (kind IN ('reply', 'calendar')),
  provider            TEXT NOT NULL CHECK (
    provider IN ('gmail', 'outlook_email', 'google_calendar', 'outlook_calendar')
  ),
  idempotency_key     TEXT NOT NULL,
  payload_sha256      TEXT NOT NULL,
  status              TEXT NOT NULL CHECK (
    status IN (
      'awaiting_approval', 'approved', 'dispatching', 'sent',
      'calendar_created', 'needs_input', 'failed',
      'side_effect_unknown', 'cancelled'
    )
  ),
  provider_object_id  TEXT,
  action_json         TEXT NOT NULL,
  lease_owner         TEXT,
  lease_token_sha256  TEXT,
  fence               BIGINT NOT NULL DEFAULT 0,
  lease_expires_at_ms BIGINT,
  next_attempt_at_ms  BIGINT NOT NULL,
  attempt_count       BIGINT NOT NULL DEFAULT 0,
  approved_at_ms      BIGINT,
  dispatched_at_ms    BIGINT,
  created_at_ms       BIGINT NOT NULL,
  updated_at_ms       BIGINT NOT NULL,
  UNIQUE(account_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_jobs_communication_actions_account
  ON jobs_communication_actions(account_id, application_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_communication_actions_due
  ON jobs_communication_actions(status, next_attempt_at_ms, lease_expires_at_ms);

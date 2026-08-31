-- Bluey account deletion and support-diagnostic resurrection fences.
-- Target: PostgreSQL 16+.

ALTER TABLE accounts
  ADD COLUMN IF NOT EXISTS deletion_pending_at_ms BIGINT;

CREATE INDEX IF NOT EXISTS idx_accounts_deletion_pending
  ON accounts(deletion_pending_at_ms)
  WHERE deletion_pending_at_ms IS NOT NULL;

CREATE TABLE IF NOT EXISTS support_diagnostic_session_tombstones (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  session_id TEXT NOT NULL,
  deleted_at_ms BIGINT NOT NULL,
  PRIMARY KEY (account_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_support_diagnostic_session_tombstones_deleted
  ON support_diagnostic_session_tombstones(account_id, deleted_at_ms);

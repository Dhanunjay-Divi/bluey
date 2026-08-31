-- Append-only, account-scoped support-diagnostic consent receipts.
-- Target: PostgreSQL 16+.

CREATE TABLE IF NOT EXISTS support_diagnostic_consent_events (
  receipt_id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  revision BIGINT NOT NULL CHECK(revision > 0),
  action TEXT NOT NULL CHECK(action IN ('granted', 'revoked')),
  policy_version TEXT NOT NULL,
  content_policy TEXT NOT NULL CHECK(content_policy = 'metadata_only'),
  recorded_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, revision)
);

CREATE INDEX IF NOT EXISTS idx_support_diagnostic_consent_current
  ON support_diagnostic_consent_events(account_id, revision DESC);

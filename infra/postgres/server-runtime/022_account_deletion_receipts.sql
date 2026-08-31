-- Opaque, bounded account-deletion reconciliation receipts.
-- Target: PostgreSQL server runtime.
--
-- These rows intentionally do not reference `accounts`: the account row and
-- all customer data can be hard-deleted while a short-lived capability holder
-- still confirms the result of a response-lost DELETE operation.
CREATE TABLE IF NOT EXISTS account_deletion_receipts (
  operation_id       TEXT PRIMARY KEY,
  account_binding    TEXT NOT NULL,
  capability_hash    TEXT NOT NULL,
  state              TEXT NOT NULL CHECK (state IN ('prepared', 'pending', 'deleted')),
  created_at_ms      BIGINT NOT NULL,
  updated_at_ms      BIGINT NOT NULL,
  completed_at_ms    BIGINT,
  expires_at_ms      BIGINT NOT NULL,
  CHECK (expires_at_ms > created_at_ms),
  CHECK ((state = 'deleted') = (completed_at_ms IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_account_deletion_receipts_expiry
  ON account_deletion_receipts(expires_at_ms);
CREATE INDEX IF NOT EXISTS idx_account_deletion_receipts_account
  ON account_deletion_receipts(account_binding, updated_at_ms DESC);

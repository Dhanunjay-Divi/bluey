-- Atomic managed-usage reservations for provider-backed requests.
--
-- `accounts.balance_cents` is spendable balance. A reservation moves its
-- ceiling out of that balance and into `accounts.reserved_cents` in the same
-- transaction. Settlement consumes actual FIFO credit and refunds the unused
-- ceiling; release/TTL reconciliation refunds the full ceiling.

CREATE TABLE IF NOT EXISTS usage_reservations (
  account_id                    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  request_id                    TEXT NOT NULL,
  kind                          TEXT NOT NULL,
  status                        TEXT NOT NULL CHECK (status IN ('reserved', 'settled', 'released')),
  attempt                       BIGINT NOT NULL DEFAULT 1 CHECK (attempt > 0),
  estimated_customer_cents      BIGINT NOT NULL CHECK (estimated_customer_cents >= 0),
  estimated_upstream_cents      BIGINT NOT NULL CHECK (estimated_upstream_cents >= 0),
  reserved_cents                BIGINT NOT NULL DEFAULT 0 CHECK (reserved_cents >= 0),
  actual_customer_cents         BIGINT NOT NULL DEFAULT 0 CHECK (actual_customer_cents >= 0),
  settled_cents                 BIGINT NOT NULL DEFAULT 0 CHECK (settled_cents >= 0),
  refunded_cents                BIGINT NOT NULL DEFAULT 0 CHECK (refunded_cents >= 0),
  reserved_trial_seconds        BIGINT NOT NULL DEFAULT 0 CHECK (reserved_trial_seconds >= 0),
  settled_trial_seconds         BIGINT NOT NULL DEFAULT 0 CHECK (settled_trial_seconds >= 0),
  refunded_trial_seconds        BIGINT NOT NULL DEFAULT 0 CHECK (refunded_trial_seconds >= 0),
  created_at_ms                 BIGINT NOT NULL,
  expires_at_ms                 BIGINT NOT NULL,
  settled_at_ms                 BIGINT,
  balance_cents_after           BIGINT,
  trial_seconds_remaining_after BIGINT,
  reservation_reason            TEXT NOT NULL,
  terminal_reason               TEXT,
  PRIMARY KEY (account_id, request_id)
);

CREATE INDEX IF NOT EXISTS idx_usage_reservations_status_expiry
  ON usage_reservations(status, expires_at_ms);
CREATE INDEX IF NOT EXISTS idx_usage_reservations_account_expiry
  ON usage_reservations(account_id, status, expires_at_ms);

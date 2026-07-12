-- Durable Stripe Auto Reload state.
-- A PaymentIntent is persisted while still unconfirmed so every possible
-- charge has a local reconciliation key before Stripe can collect funds.

CREATE TABLE IF NOT EXISTS stripe_auto_reload_attempts (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  amount_cents BIGINT NOT NULL,
  currency TEXT NOT NULL DEFAULT 'usd',
  stripe_customer_id TEXT NOT NULL,
  stripe_payment_method_id TEXT NOT NULL,
  stripe_payment_intent_id TEXT,
  stripe_charge_id TEXT,
  create_idempotency_key TEXT NOT NULL UNIQUE,
  confirm_idempotency_key TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL,
  failure_code TEXT,
  last_event_id TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  payment_intent_created_at TIMESTAMPTZ,
  charged_at TIMESTAMPTZ,
  credited_at TIMESTAMPTZ,
  failed_at TIMESTAMPTZ,
  reversed_at TIMESTAMPTZ,
  last_reconciled_at TIMESTAMPTZ,
  CHECK (amount_cents > 0),
  CHECK (currency = 'usd'),
  CHECK (status IN (
    'reserved', 'requires_confirmation', 'processing',
    'reconciliation_required', 'succeeded', 'failed',
    'canceled', 'reversed'
  ))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_stripe_auto_reload_payment_intent
  ON stripe_auto_reload_attempts(stripe_payment_intent_id)
  WHERE stripe_payment_intent_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_stripe_auto_reload_charge
  ON stripe_auto_reload_attempts(stripe_charge_id)
  WHERE stripe_charge_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_stripe_auto_reload_active_account
  ON stripe_auto_reload_attempts(account_id)
  WHERE status IN (
    'reserved', 'requires_confirmation', 'processing',
    'reconciliation_required'
  );
CREATE INDEX IF NOT EXISTS idx_stripe_auto_reload_account_created
  ON stripe_auto_reload_attempts(account_id, created_at);

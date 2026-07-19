-- Target: Postgres only
-- Atomically reserves one included Jobs packet before managed resume provider
-- dispatch. The reservation is job/idempotency scoped and converts into the
-- ordinary jobs_packet_metering row without charging general chat credit.

CREATE TABLE IF NOT EXISTS usage_cutover_spend_baseline (
  occurred_at TIMESTAMPTZ NOT NULL,
  cost_cents BIGINT NOT NULL CHECK(cost_cents > 0 AND cost_cents <= 100000000)
);
CREATE INDEX IF NOT EXISTS idx_usage_cutover_spend_baseline_time
  ON usage_cutover_spend_baseline(occurred_at);

ALTER TABLE usage_events
  ADD COLUMN IF NOT EXISTS origin TEXT DEFAULT 'legacy_unverified';
BEGIN;
SELECT pg_advisory_xact_lock(hashtextextended('usage-origin-authority-cutover-v1', 0));
CREATE TABLE IF NOT EXISTS bluey_data_migrations (
  name TEXT PRIMARY KEY,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
DO $$
BEGIN
  -- A completed replay is read-only with respect to usage_events. Take the
  -- cutover lock only while a missing marker still requires a snapshot or
  -- relabel, so ordinary restarts do not repeatedly block usage writers.
  IF NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-cutover-spend-baseline-v1'
  ) OR NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-origin-authority-cutover-v1'
  ) OR NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-origin-taxonomy-repair-v1'
  ) THEN
    LOCK TABLE usage_events IN SHARE ROW EXCLUSIVE MODE;
  END IF;
  IF NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-cutover-spend-baseline-v1'
  ) THEN
    -- No account/request/provider identity crosses this boundary. Treat every
    -- old positive cost as spend: conservative over-counting expires with the
    -- rolling window, while under-counting could reopen a hard cap.
    INSERT INTO usage_cutover_spend_baseline(occurred_at, cost_cents)
    SELECT ts, LEAST(GREATEST(cost_cents_to_bluey, 0), 100000000)
      FROM usage_events
     WHERE cost_cents_to_bluey > 0;
    INSERT INTO bluey_data_migrations(name)
      VALUES ('usage-cutover-spend-baseline-v1');
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-origin-authority-cutover-v1'
  ) THEN
    UPDATE usage_events SET origin = 'legacy_unverified';
    INSERT INTO bluey_data_migrations(name) VALUES
      ('usage-origin-authority-cutover-v1'),
      ('usage-origin-taxonomy-repair-v1');
  ELSIF NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-origin-taxonomy-repair-v1'
  ) THEN
    UPDATE usage_events
       SET origin = 'legacy_unverified'
     WHERE origin IS NULL
        OR origin NOT IN ('server', 'client', 'legacy_unverified');
    INSERT INTO bluey_data_migrations(name)
      VALUES ('usage-origin-taxonomy-repair-v1');
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM bluey_data_migrations
     WHERE name = 'usage-reservations-settled-at-repair-v1'
  ) THEN
    UPDATE usage_reservations
       SET settled_at_ms = created_at_ms
     WHERE status = 'settled' AND settled_at_ms IS NULL;
    INSERT INTO bluey_data_migrations(name)
      VALUES ('usage-reservations-settled-at-repair-v1');
  END IF;
END $$;
COMMIT;
ALTER TABLE usage_events
  ALTER COLUMN origin SET DEFAULT 'legacy_unverified';
ALTER TABLE usage_events
  ALTER COLUMN origin SET NOT NULL;
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
     WHERE conname = 'usage_events_origin_taxonomy'
       AND conrelid = 'usage_events'::regclass
  ) THEN
    ALTER TABLE usage_events
      ADD CONSTRAINT usage_events_origin_taxonomy
      CHECK (origin IN ('server', 'client', 'legacy_unverified')) NOT VALID;
  END IF;
END $$;
ALTER TABLE usage_events VALIDATE CONSTRAINT usage_events_origin_taxonomy;
CREATE INDEX IF NOT EXISTS idx_usage_events_server_ts
  ON usage_events(ts) WHERE origin = 'server';
CREATE INDEX IF NOT EXISTS idx_usage_events_server_identity
  ON usage_events(account_id, request_id, kind) WHERE origin = 'server';
CREATE INDEX IF NOT EXISTS idx_usage_reservations_settled_exposure
  ON usage_reservations(settled_at_ms) WHERE status = 'settled';

CREATE TABLE IF NOT EXISTS jobs_generation_allowance_reservations (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  generation_key TEXT NOT NULL,
  reservation_token TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'reserved' CHECK(status IN (
    'reserved', 'released', 'committed'
  )),
  period_start_ms BIGINT NOT NULL,
  application_id TEXT REFERENCES jobs_applications(id) ON DELETE SET NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  PRIMARY KEY (account_id, job_id),
  UNIQUE(account_id, generation_key)
);

CREATE INDEX IF NOT EXISTS idx_jobs_generation_allowance_status
  ON jobs_generation_allowance_reservations(account_id, status, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_provider_cost_holds (
  request_scope_hash TEXT PRIMARY KEY,
  account_scope_hash TEXT NOT NULL,
  generation_scope_hash TEXT NOT NULL,
  root_scope_hash TEXT NOT NULL,
  reservation_token TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  projected_cost_cents BIGINT NOT NULL CHECK (
    projected_cost_cents > 0 AND projected_cost_cents <= 100000000
  ),
  settled_cost_cents BIGINT NOT NULL DEFAULT 0 CHECK (
    settled_cost_cents >= 0 AND settled_cost_cents <= 100000000
  ),
  status TEXT NOT NULL DEFAULT 'held' CHECK(status IN (
    'held', 'settled', 'released'
  )),
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_provider_cost_holds_generation
  ON jobs_provider_cost_holds(
    account_scope_hash, generation_scope_hash, status, updated_at_ms DESC
  );
CREATE INDEX IF NOT EXISTS idx_jobs_provider_cost_holds_root
  ON jobs_provider_cost_holds(
    account_scope_hash, root_scope_hash, status, updated_at_ms DESC
  );
CREATE INDEX IF NOT EXISTS idx_jobs_provider_cost_holds_global
  ON jobs_provider_cost_holds(status, updated_at_ms DESC);

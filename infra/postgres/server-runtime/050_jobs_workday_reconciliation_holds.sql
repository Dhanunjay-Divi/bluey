-- Target: PostgreSQL
-- Storage for Workday protected-history reconciliation. Registered with API
-- startup and the operator migrator; this migration creates no effects,
-- releases, or backfill.

CREATE TABLE IF NOT EXISTS jobs_workday_reconciliation_holds (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL REFERENCES jobs_discovery_sources(id)
    ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  legacy_external_id TEXT NOT NULL CHECK(length(btrim(legacy_external_id)) > 0),
  first_run_id TEXT NOT NULL REFERENCES jobs_discovery_runs(id)
    ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
  first_snapshot_sha256 TEXT NOT NULL
    CHECK(length(first_snapshot_sha256) = 64 AND first_snapshot_sha256 ~ '^[0-9a-f]{64}$'),
  proposed_external_id TEXT NOT NULL
    CHECK(length(btrim(proposed_external_id)) > 0
      AND proposed_external_id <> legacy_external_id),
  reason TEXT NOT NULL CHECK(reason = 'protected_history'),
  created_at_ms BIGINT NOT NULL CHECK(created_at_ms > 0),
  PRIMARY KEY(account_id, source_id, job_id),
  UNIQUE(source_id, legacy_external_id),
  FOREIGN KEY(source_id, legacy_external_id)
    REFERENCES jobs_discovery_memberships(source_id, external_id)
    ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX IF NOT EXISTS idx_jobs_workday_reconciliation_holds_account_job
  ON jobs_workday_reconciliation_holds(account_id, job_id);

CREATE OR REPLACE FUNCTION jobs_workday_reconciliation_hold_validate_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM jobs_discovery_memberships membership
    JOIN jobs_discovery_sources source ON source.id = membership.source_id
    JOIN jobs_postings posting ON posting.id = NEW.job_id
    JOIN jobs_discovery_runs run ON run.id = NEW.first_run_id
    WHERE membership.source_id = NEW.source_id
      AND membership.external_id = NEW.legacy_external_id
      AND membership.account_id = NEW.account_id
      AND membership.job_id = NEW.job_id
      AND source.account_id = NEW.account_id
      AND source.provider = 'workday'
      AND posting.account_id = NEW.account_id
      AND run.account_id = NEW.account_id
      AND run.source_id = NEW.source_id
      AND run.snapshot_hash = NEW.first_snapshot_sha256
  ) THEN
    RAISE EXCEPTION 'workday reconciliation hold insert refused'
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$function$;

CREATE OR REPLACE TRIGGER trg_jobs_workday_hold_insert_guard
BEFORE INSERT ON jobs_workday_reconciliation_holds
FOR EACH ROW EXECUTE FUNCTION jobs_workday_reconciliation_hold_validate_insert();

CREATE OR REPLACE FUNCTION jobs_workday_reconciliation_hold_reject_update()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
  RAISE EXCEPTION 'workday reconciliation hold is immutable'
    USING ERRCODE = '23514';
  RETURN NEW;
END;
$function$;

CREATE OR REPLACE TRIGGER trg_jobs_workday_hold_no_update
BEFORE UPDATE ON jobs_workday_reconciliation_holds
FOR EACH ROW EXECUTE FUNCTION jobs_workday_reconciliation_hold_reject_update();

CREATE TABLE IF NOT EXISTS jobs_workday_reconciliation_hold_observations (
  run_id TEXT NOT NULL REFERENCES jobs_discovery_runs(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  PRIMARY KEY(run_id, account_id, source_id, job_id),
  FOREIGN KEY(account_id, source_id, job_id)
    REFERENCES jobs_workday_reconciliation_holds(account_id, source_id, job_id)
    ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED
);

-- A future writer derives this run's COUNT(*) once in its transaction and
-- requires it to equal discovered_count - upserted_count before commit.
CREATE OR REPLACE FUNCTION jobs_workday_reconciliation_hold_observation_validate_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM jobs_discovery_runs run
    JOIN jobs_discovery_sources source ON source.id = run.source_id
    JOIN jobs_workday_reconciliation_holds hold
      ON hold.account_id = NEW.account_id
     AND hold.source_id = NEW.source_id
     AND hold.job_id = NEW.job_id
    WHERE run.id = NEW.run_id
      AND run.status = 'completed'
      AND run.completed_at_ms IS NOT NULL
      AND source.account_id = run.account_id
      AND source.provider = 'workday'
      AND run.account_id = NEW.account_id
      AND run.source_id = NEW.source_id
      AND run.discovered_count - run.upserted_count BETWEEN 1 AND 10000
  ) THEN
    RAISE EXCEPTION 'workday reconciliation hold observation insert refused'
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$function$;

CREATE OR REPLACE TRIGGER trg_jobs_workday_hold_observation_insert_guard
BEFORE INSERT ON jobs_workday_reconciliation_hold_observations
FOR EACH ROW EXECUTE FUNCTION jobs_workday_reconciliation_hold_observation_validate_insert();

CREATE OR REPLACE FUNCTION jobs_workday_reconciliation_hold_observation_reject_update()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
  RAISE EXCEPTION 'workday reconciliation hold observation is immutable'
    USING ERRCODE = '23514';
  RETURN NEW;
END;
$function$;

CREATE OR REPLACE TRIGGER trg_jobs_workday_hold_observation_no_update
BEFORE UPDATE ON jobs_workday_reconciliation_hold_observations
FOR EACH ROW EXECUTE FUNCTION jobs_workday_reconciliation_hold_observation_reject_update();

-- Direct raw DELETE is unsupported and remains behind a future audited
-- write-control authority.  A portable trigger cannot reliably distinguish
-- account/job/run foreign-key cascades from a direct DELETE while preserving
-- privacy cleanup.  Individual posting deletion with an observation is
-- intentionally fail-closed; audited release/deletion is a future migration.

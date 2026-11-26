-- Target: SQLite
-- Storage for Workday protected-history reconciliation. Registered with API
-- startup; this migration creates no effects, releases, or backfill.
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS jobs_workday_reconciliation_holds (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES jobs_discovery_sources(id)
        ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
    job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
    legacy_external_id TEXT NOT NULL CHECK(length(trim(legacy_external_id)) > 0),
    first_run_id TEXT NOT NULL REFERENCES jobs_discovery_runs(id)
        ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
    first_snapshot_sha256 TEXT NOT NULL
        CHECK(typeof(first_snapshot_sha256) = 'text'
          AND length(first_snapshot_sha256) = 64
          AND length(CAST(first_snapshot_sha256 AS BLOB)) = 64
          AND first_snapshot_sha256 NOT GLOB '*[^0-9a-f]*'),
    proposed_external_id TEXT NOT NULL
        CHECK(length(trim(proposed_external_id)) > 0
          AND proposed_external_id <> legacy_external_id),
    reason TEXT NOT NULL CHECK(reason = 'protected_history'),
    created_at_ms INTEGER NOT NULL CHECK(typeof(created_at_ms) = 'integer' AND created_at_ms > 0),
    PRIMARY KEY(account_id, source_id, job_id),
    UNIQUE(source_id, legacy_external_id),
    FOREIGN KEY(source_id, legacy_external_id)
        REFERENCES jobs_discovery_memberships(source_id, external_id)
        ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX IF NOT EXISTS idx_jobs_workday_reconciliation_holds_account_job
    ON jobs_workday_reconciliation_holds(account_id, job_id);

-- The referenced membership key alone cannot establish tenant ownership or
-- bind the first immutable evidence to the originating Workday snapshot.
CREATE TRIGGER IF NOT EXISTS trg_jobs_workday_hold_insert_guard
BEFORE INSERT ON jobs_workday_reconciliation_holds
WHEN NOT EXISTS (
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
)
BEGIN
    SELECT RAISE(ABORT, 'workday reconciliation hold insert refused');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workday_hold_no_update
BEFORE UPDATE ON jobs_workday_reconciliation_holds
BEGIN
    SELECT RAISE(ABORT, 'workday reconciliation hold is immutable');
END;

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

-- Observations preserve a completed run's view of immutable holds without
-- rewriting the first evidence when a pre-existing hold appears again.
-- A future writer derives this run's COUNT(*) once in its transaction and
-- requires it to equal discovered_count - upserted_count before commit.
CREATE TRIGGER IF NOT EXISTS trg_jobs_workday_hold_observation_insert_guard
BEFORE INSERT ON jobs_workday_reconciliation_hold_observations
WHEN NOT EXISTS (
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
      AND typeof(run.discovered_count) = 'integer'
      AND typeof(run.upserted_count) = 'integer'
      AND run.discovered_count - run.upserted_count BETWEEN 1 AND 10000
)
BEGIN
    SELECT RAISE(ABORT, 'workday reconciliation hold observation insert refused');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workday_hold_observation_no_update
BEFORE UPDATE ON jobs_workday_reconciliation_hold_observations
BEGIN
    SELECT RAISE(ABORT, 'workday reconciliation hold observation is immutable');
END;

-- Direct raw DELETE is unsupported and remains behind a future audited
-- write-control authority.  SQLite cannot distinguish a direct DELETE from
-- an account/job/run foreign-key cascade in a BEFORE DELETE trigger.
-- Account privacy and run-observation cascades must remain available.
-- Individual posting deletion with an observation is intentionally
-- fail-closed; audited release/deletion is a separate future migration.
COMMIT;

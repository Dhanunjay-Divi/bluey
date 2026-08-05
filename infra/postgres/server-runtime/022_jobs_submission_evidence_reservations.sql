-- Target: PostgreSQL
-- Protected quota headroom reserved before an employer-facing Submit click.
CREATE TABLE IF NOT EXISTS jobs_submission_evidence_capacity (
  account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id   TEXT NOT NULL
    REFERENCES jobs_applications(id) ON DELETE CASCADE,
  run_id            TEXT NOT NULL,
  runner            TEXT NOT NULL CHECK(runner IN ('cloud', 'local')),
  reserved_bytes    BIGINT NOT NULL CHECK(reserved_bytes > 0),
  reserved_objects  BIGINT NOT NULL CHECK(reserved_objects > 0),
  consumed_bytes    BIGINT NOT NULL DEFAULT 0 CHECK(consumed_bytes >= 0),
  consumed_objects  BIGINT NOT NULL DEFAULT 0 CHECK(consumed_objects >= 0),
  expires_at_ms     BIGINT NOT NULL CHECK(expires_at_ms > 0),
  state             TEXT NOT NULL
    CHECK(state IN ('active', 'committed', 'released', 'expired')),
  created_at_ms     BIGINT NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms     BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  completed_at_ms   BIGINT,
  PRIMARY KEY(account_id, application_id, run_id),
  CHECK(consumed_bytes <= reserved_bytes),
  CHECK(consumed_objects <= reserved_objects)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_submission_evidence_capacity_active_application
  ON jobs_submission_evidence_capacity(account_id, application_id)
  WHERE state = 'active';

CREATE INDEX IF NOT EXISTS idx_jobs_submission_evidence_capacity_account
  ON jobs_submission_evidence_capacity(account_id, state, expires_at_ms);

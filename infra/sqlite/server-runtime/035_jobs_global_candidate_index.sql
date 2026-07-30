-- Bluey Jobs global candidate-feed staging.
-- Candidate feeds are discovery leads only. Employer-facing actions still
-- require original-source revalidation and the normal eligibility gate.

CREATE TABLE IF NOT EXISTS jobs_global_discovery_sources (
    id                    TEXT PRIMARY KEY,
    provider              TEXT NOT NULL,
    source_key            TEXT NOT NULL,
    source_json           TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'active',
    health                TEXT NOT NULL DEFAULT 'waiting',
    consecutive_failures  INTEGER NOT NULL DEFAULT 0,
    run_interval_ms       INTEGER NOT NULL DEFAULT 14400000,
    next_run_at_ms        INTEGER NOT NULL,
    last_success_at_ms    INTEGER,
    last_failure_at_ms    INTEGER,
    last_error_code       TEXT,
    lease_owner           TEXT,
    lease_token           TEXT,
    lease_expires_at_ms   INTEGER,
    created_at_ms         INTEGER NOT NULL,
    updated_at_ms         INTEGER NOT NULL,
    UNIQUE(provider, source_key)
);
CREATE INDEX IF NOT EXISTS idx_jobs_global_sources_due
    ON jobs_global_discovery_sources(status, next_run_at_ms, lease_expires_at_ms);

CREATE TABLE IF NOT EXISTS jobs_global_ingestion_runs (
    id                    TEXT PRIMARY KEY,
    source_id             TEXT NOT NULL REFERENCES jobs_global_discovery_sources(id) ON DELETE CASCADE,
    replay_key            TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'running',
    expected_rows         INTEGER NOT NULL DEFAULT 0,
    received_rows         INTEGER NOT NULL DEFAULT 0,
    received_batches      INTEGER NOT NULL DEFAULT 0,
    rejected_rows         INTEGER NOT NULL DEFAULT 0,
    rejection_summary_json TEXT NOT NULL DEFAULT '{}',
    expired_count         INTEGER NOT NULL DEFAULT 0,
    artifact_sha256       TEXT NOT NULL DEFAULT '',
    error_code            TEXT,
    started_at_ms         INTEGER NOT NULL,
    completed_at_ms       INTEGER,
    UNIQUE(source_id, replay_key)
);
CREATE INDEX IF NOT EXISTS idx_jobs_global_runs_source
    ON jobs_global_ingestion_runs(source_id, started_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_global_candidates (
    id                    TEXT PRIMARY KEY,
    canonical_key         TEXT NOT NULL UNIQUE,
    candidate_json        TEXT NOT NULL,
    company               TEXT NOT NULL,
    title                 TEXT NOT NULL,
    location              TEXT NOT NULL DEFAULT '',
    workplace             TEXT NOT NULL DEFAULT '',
    canonical_url         TEXT NOT NULL,
    role_family           TEXT NOT NULL DEFAULT 'generic',
    posted_at_ms          INTEGER,
    availability_status   TEXT NOT NULL DEFAULT 'unknown',
    first_seen_at_ms      INTEGER NOT NULL,
    last_seen_at_ms       INTEGER NOT NULL,
    updated_at_ms         INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_global_candidates_recent
    ON jobs_global_candidates(availability_status, posted_at_ms DESC, updated_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_global_candidates_role
    ON jobs_global_candidates(role_family, availability_status, posted_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_global_candidate_memberships (
    source_id             TEXT NOT NULL REFERENCES jobs_global_discovery_sources(id) ON DELETE CASCADE,
    external_id           TEXT NOT NULL,
    candidate_id          TEXT NOT NULL REFERENCES jobs_global_candidates(id) ON DELETE CASCADE,
    content_hash          TEXT NOT NULL,
    first_seen_at_ms      INTEGER NOT NULL,
    last_seen_at_ms       INTEGER NOT NULL,
    last_seen_run_id      TEXT NOT NULL REFERENCES jobs_global_ingestion_runs(id) ON DELETE RESTRICT,
    availability_status   TEXT NOT NULL DEFAULT 'active',
    missing_count         INTEGER NOT NULL DEFAULT 0,
    missing_since_at_ms   INTEGER,
    PRIMARY KEY(source_id, external_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_global_memberships_candidate
    ON jobs_global_candidate_memberships(candidate_id, availability_status, last_seen_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_global_ingestion_batches (
    run_id                TEXT NOT NULL REFERENCES jobs_global_ingestion_runs(id) ON DELETE CASCADE,
    batch_index           INTEGER NOT NULL,
    payload_sha256        TEXT NOT NULL,
    row_count             INTEGER NOT NULL,
    created_at_ms         INTEGER NOT NULL,
    PRIMARY KEY(run_id, batch_index)
);

CREATE TABLE IF NOT EXISTS jobs_global_candidate_materializations (
    account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    candidate_id          TEXT NOT NULL REFERENCES jobs_global_candidates(id) ON DELETE CASCADE,
    job_id                TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
    track_id              TEXT NOT NULL REFERENCES jobs_tracks(id) ON DELETE CASCADE,
    source_updated_at_ms  INTEGER NOT NULL,
    materialized_at_ms    INTEGER NOT NULL,
    PRIMARY KEY(account_id, candidate_id),
    UNIQUE(account_id, job_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_global_materializations_account
    ON jobs_global_candidate_materializations(account_id, materialized_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_global_materialization_state (
    account_id              TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    global_updated_at_ms    INTEGER NOT NULL DEFAULT 0,
    profile_revision_at_ms  INTEGER NOT NULL DEFAULT 0,
    last_run_at_ms          INTEGER NOT NULL DEFAULT 0
);

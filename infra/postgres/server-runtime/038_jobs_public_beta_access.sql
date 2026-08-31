-- Target: PostgreSQL
-- Durable authority for the bounded, first-come Bluey Jobs public beta.

CREATE TABLE IF NOT EXISTS jobs_public_beta_cohorts (
    id                  TEXT PRIMARY KEY CHECK(id = 'public-v1'),
    state               TEXT NOT NULL CHECK(state IN ('draft', 'open', 'closed_to_new', 'suspended')),
    opens_at_ms          BIGINT,
    closes_at_ms         BIGINT,
    hard_cap             BIGINT NOT NULL CHECK(hard_cap >= 0 AND hard_cap <= 10000),
    assigned_count       BIGINT NOT NULL CHECK(assigned_count >= 0 AND assigned_count <= hard_cap),
    revision             BIGINT NOT NULL CHECK(revision >= 1),
    created_at_ms        BIGINT NOT NULL,
    updated_at_ms        BIGINT NOT NULL,
    CHECK((opens_at_ms IS NULL) = (closes_at_ms IS NULL)),
    CHECK(opens_at_ms IS NULL OR (opens_at_ms >= 0 AND closes_at_ms > opens_at_ms)),
    CHECK(opens_at_ms IS NULL OR closes_at_ms - opens_at_ms <= 7776000000)
);

CREATE TABLE IF NOT EXISTS jobs_public_beta_enrollments (
    cohort_id            TEXT NOT NULL REFERENCES jobs_public_beta_cohorts(id) ON DELETE RESTRICT,
    account_id           TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source               TEXT NOT NULL CHECK(source IN ('public_window', 'admin')),
    admitted_at_ms       BIGINT NOT NULL,
    PRIMARY KEY(cohort_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_enrollments_account
    ON jobs_public_beta_enrollments(account_id, cohort_id);
CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_enrollments_source
    ON jobs_public_beta_enrollments(cohort_id, source, admitted_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_public_beta_overrides (
    cohort_id            TEXT NOT NULL REFERENCES jobs_public_beta_cohorts(id) ON DELETE RESTRICT,
    account_id           TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    denied               INTEGER NOT NULL CHECK(denied IN (0, 1)),
    revision             BIGINT NOT NULL CHECK(revision >= 1),
    created_at_ms        BIGINT NOT NULL,
    updated_at_ms        BIGINT NOT NULL,
    PRIMARY KEY(cohort_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_overrides_account
    ON jobs_public_beta_overrides(account_id, cohort_id);
CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_overrides_active
    ON jobs_public_beta_overrides(cohort_id, denied, updated_at_ms DESC)
    WHERE denied = 1;

INSERT INTO jobs_public_beta_cohorts (
    id, state, opens_at_ms, closes_at_ms, hard_cap, assigned_count,
    revision, created_at_ms, updated_at_ms
) VALUES (
    'public-v1', 'draft', NULL, NULL, 0, 0, 1,
    FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
    FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
)
ON CONFLICT (id) DO NOTHING;

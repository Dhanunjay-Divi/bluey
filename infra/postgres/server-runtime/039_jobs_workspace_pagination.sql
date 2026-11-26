-- Target: PostgreSQL
-- Stable keyset order for the bounded interactive Bluey Jobs workspace.
-- Descending score/time are represented as ascending negated expressions so
-- continuation can use one row-value range predicate and the same index seek.

CREATE INDEX IF NOT EXISTS idx_jobs_postings_account_workspace_page
    ON jobs_postings(account_id, (-match_score), (-updated_at_ms), id ASC);
CREATE INDEX IF NOT EXISTS idx_jobs_postings_verified_import_backfill
    ON jobs_postings(account_id, (id COLLATE "C"))
    WHERE RIGHT(source, 7) = '_import';

CREATE TABLE IF NOT EXISTS jobs_verified_import_backfill_checkpoints (
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    migration_version BIGINT NOT NULL CHECK(migration_version = 1),
    after_job_id       TEXT NOT NULL DEFAULT '',
    completed          SMALLINT NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
    lease_owner        TEXT CHECK(lease_owner IS NULL OR length(lease_owner) BETWEEN 1 AND 128),
    lease_token_sha256 TEXT CHECK(
        lease_token_sha256 IS NULL OR lease_token_sha256 ~ '^[0-9a-f]{64}$'
    ),
    lease_expires_at_ms BIGINT CHECK(
        lease_expires_at_ms IS NULL OR lease_expires_at_ms >= 0
    ),
    updated_at_ms      BIGINT NOT NULL,
    PRIMARY KEY(account_id, migration_version),
    CHECK(
        (lease_owner IS NULL AND lease_token_sha256 IS NULL AND lease_expires_at_ms IS NULL)
        OR
        (lease_owner IS NOT NULL AND lease_token_sha256 IS NOT NULL
            AND lease_expires_at_ms IS NOT NULL)
    ),
    CHECK(completed = 0 OR (
        lease_owner IS NULL AND lease_token_sha256 IS NULL AND lease_expires_at_ms IS NULL
    ))
);

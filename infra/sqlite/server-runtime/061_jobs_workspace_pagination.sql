-- Target: SQLite
-- Stable keyset order for the bounded interactive Bluey Jobs workspace.

CREATE INDEX IF NOT EXISTS idx_jobs_postings_account_workspace_page
    ON jobs_postings(account_id, match_score DESC, updated_at_ms DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_jobs_postings_verified_import_backfill
    ON jobs_postings(account_id, id)
    WHERE substr(source, -7) = '_import';

-- Phase 626 also removes the legacy first-workspace-page coupling from verified
-- import repair. The durable cursor lets a small background batch make
-- monotonic progress across restarts without turning an interactive page load
-- into an unbounded migration.
CREATE TABLE IF NOT EXISTS jobs_verified_import_backfill_checkpoints (
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    migration_version INTEGER NOT NULL CHECK(migration_version = 1),
    after_job_id       TEXT NOT NULL DEFAULT '',
    completed          INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
    lease_owner        TEXT CHECK(lease_owner IS NULL OR length(lease_owner) BETWEEN 1 AND 128),
    lease_token_sha256 TEXT CHECK(lease_token_sha256 IS NULL OR (
        length(lease_token_sha256) = 64
        AND lower(lease_token_sha256) = lease_token_sha256
        AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*'
    )),
    lease_expires_at_ms INTEGER CHECK(
        lease_expires_at_ms IS NULL OR lease_expires_at_ms >= 0
    ),
    updated_at_ms      INTEGER NOT NULL,
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

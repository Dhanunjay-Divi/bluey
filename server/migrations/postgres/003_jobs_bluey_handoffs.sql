CREATE TABLE IF NOT EXISTS jobs_bluey_handoffs (
    nonce_hash          TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    application_id      TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
    audience            TEXT NOT NULL,
    snapshot_json       TEXT NOT NULL,
    created_at_ms       BIGINT NOT NULL,
    expires_at_ms       BIGINT NOT NULL,
    consumed_at_ms      BIGINT,
    CHECK(length(audience) BETWEEN 1 AND 64),
    CHECK(expires_at_ms > created_at_ms)
);

CREATE INDEX IF NOT EXISTS idx_jobs_bluey_handoffs_account
    ON jobs_bluey_handoffs(account_id, application_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_bluey_handoffs_expiry
    ON jobs_bluey_handoffs(expires_at_ms, consumed_at_ms);

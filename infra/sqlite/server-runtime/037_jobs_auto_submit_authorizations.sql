CREATE TABLE IF NOT EXISTS jobs_auto_submit_authorizations (
    id                      TEXT PRIMARY KEY,
    account_id              TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    career_track_id         TEXT NOT NULL REFERENCES jobs_tracks(id) ON DELETE CASCADE,
    application_identity_id TEXT NOT NULL,
    source_resume_asset_id  TEXT NOT NULL,
    authority_fingerprint   TEXT NOT NULL,
    revision_no             INTEGER NOT NULL,
    authorized_at_ms        INTEGER NOT NULL,
    revoked_at_ms           INTEGER,
    UNIQUE(account_id, career_track_id, revision_no)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_auto_submit_authorizations_active
    ON jobs_auto_submit_authorizations(account_id, career_track_id)
    WHERE revoked_at_ms IS NULL;

CREATE INDEX IF NOT EXISTS idx_jobs_auto_submit_account
    ON jobs_auto_submit_authorizations(account_id, authorized_at_ms DESC);

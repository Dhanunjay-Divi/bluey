-- target: postgres
-- Account-scoped source resume objects used for exact-template DOCX tailoring.

CREATE TABLE IF NOT EXISTS jobs_resume_source_assets (
    id                TEXT PRIMARY KEY,
    account_id        TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    file_name         TEXT NOT NULL,
    media_type        TEXT NOT NULL,
    file_type         TEXT NOT NULL CHECK(file_type IN ('docx', 'pdf', 'txt')),
    storage_key       TEXT NOT NULL UNIQUE,
    sha256            TEXT NOT NULL,
    size_bytes        BIGINT NOT NULL CHECK(size_bytes > 0),
    page_count        BIGINT,
    template_status   TEXT NOT NULL CHECK(template_status IN ('exact_docx', 'converted_layout', 'text_only')),
    created_at_ms     BIGINT NOT NULL,
    updated_at_ms     BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_resume_source_assets_account
    ON jobs_resume_source_assets(account_id, updated_at_ms DESC);

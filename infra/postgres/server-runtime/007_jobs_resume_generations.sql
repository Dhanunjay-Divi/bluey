-- Target: Postgres only
-- Durable, tenant-scoped idempotency and provenance for one job-specific
-- resume generation. Provider details and upstream cost remain server-side.

CREATE TABLE IF NOT EXISTS jobs_resume_generations (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  generation_key TEXT NOT NULL,
  reservation_token TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'reserved' CHECK(status IN (
    'reserved', 'completed', 'failed'
  )),
  output_json TEXT,
  provider TEXT,
  model TEXT,
  input_tokens BIGINT NOT NULL DEFAULT 0,
  output_tokens BIGINT NOT NULL DEFAULT 0,
  cost_cents_to_bluey BIGINT NOT NULL DEFAULT 0,
  failure_code TEXT,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, generation_key)
);

CREATE INDEX IF NOT EXISTS idx_jobs_resume_generations_account
  ON jobs_resume_generations(account_id, updated_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_resume_generations_job
  ON jobs_resume_generations(account_id, job_id, updated_at_ms DESC);

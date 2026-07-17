-- Append-only candidate feedback, support issues, and confirmed outcomes.
-- Optional notes and reasons are authenticated-encrypted in event_json.

CREATE TABLE IF NOT EXISTS jobs_candidate_events (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL CHECK(event_type IN (
    'match_feedback', 'application_issue', 'application_outcome'
  )),
  job_id TEXT REFERENCES jobs_postings(id) ON DELETE CASCADE,
  application_id TEXT REFERENCES jobs_applications(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  event_json TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_candidate_events_account
  ON jobs_candidate_events(account_id, event_type, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_candidate_events_job
  ON jobs_candidate_events(account_id, job_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_candidate_events_application
  ON jobs_candidate_events(account_id, application_id, created_at_ms DESC);

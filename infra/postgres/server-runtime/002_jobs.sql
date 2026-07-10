-- Bluey Jobs tenant workspace.
--
-- This schema stays separate from meeting/session runtime tables. Every
-- customer-owned row carries account_id and every generated resume version is
-- bound to exactly one canonical job.

CREATE TABLE IF NOT EXISTS jobs_profiles (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  profile_json TEXT NOT NULL DEFAULT '{}',
  onboarding_step BIGINT NOT NULL DEFAULT 0,
  onboarding_complete INTEGER NOT NULL DEFAULT 0,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs_facts (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  category TEXT NOT NULL,
  label TEXT NOT NULL,
  value_json TEXT NOT NULL,
  source TEXT NOT NULL,
  verification_status TEXT NOT NULL DEFAULT 'unverified',
  confirmed_at_ms BIGINT,
  confirmed_by TEXT,
  schema_version BIGINT NOT NULL DEFAULT 1,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_facts_account_category
  ON jobs_facts(account_id, category, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_preferences (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  preferences_json TEXT NOT NULL DEFAULT '{}',
  updated_at_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs_tracks (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  track_json TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_tracks_account
  ON jobs_tracks(account_id, active, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_postings (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  canonical_key TEXT NOT NULL,
  posting_json TEXT NOT NULL,
  source TEXT NOT NULL,
  canonical_url TEXT,
  company TEXT NOT NULL,
  title TEXT NOT NULL,
  location TEXT,
  match_score BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'matched',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, canonical_key)
);
CREATE INDEX IF NOT EXISTS idx_jobs_postings_account_score
  ON jobs_postings(account_id, status, match_score DESC, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_resume_versions (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  version_no BIGINT NOT NULL,
  mode TEXT NOT NULL,
  content_json TEXT NOT NULL,
  diff_json TEXT NOT NULL DEFAULT '{}',
  claim_ids_json TEXT NOT NULL DEFAULT '[]',
  checksum TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, job_id, checksum)
);
CREATE INDEX IF NOT EXISTS idx_jobs_resume_versions_job
  ON jobs_resume_versions(account_id, job_id, version_no DESC);

CREATE TABLE IF NOT EXISTS jobs_applications (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  resume_version_id TEXT REFERENCES jobs_resume_versions(id) ON DELETE SET NULL,
  state TEXT NOT NULL DEFAULT 'matched',
  application_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  submitted_at_ms BIGINT,
  UNIQUE(account_id, job_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_applications_account_state
  ON jobs_applications(account_id, state, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_browser_sessions (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  runner TEXT NOT NULL,
  status TEXT NOT NULL,
  session_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_browser_sessions_account
  ON jobs_browser_sessions(account_id, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_interventions (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id TEXT REFERENCES jobs_applications(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'open',
  intervention_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms BIGINT NOT NULL,
  resolved_at_ms BIGINT
);
CREATE INDEX IF NOT EXISTS idx_jobs_interventions_account_status
  ON jobs_interventions(account_id, status, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_integrations (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'disconnected',
  integration_json TEXT NOT NULL DEFAULT '{}',
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, provider)
);

CREATE TABLE IF NOT EXISTS jobs_entitlements (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  plan TEXT NOT NULL DEFAULT 'free',
  track_limit BIGINT NOT NULL DEFAULT 1,
  monthly_packet_limit BIGINT NOT NULL DEFAULT 5,
  used_packets BIGINT NOT NULL DEFAULT 0,
  period_start_ms BIGINT NOT NULL,
  period_end_ms BIGINT NOT NULL,
  local_browser INTEGER NOT NULL DEFAULT 0,
  cloud_browser INTEGER NOT NULL DEFAULT 0,
  updated_at_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs_run_events (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  run_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  event_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_run_events_account_run
  ON jobs_run_events(account_id, run_id, created_at_ms ASC);

CREATE TABLE IF NOT EXISTS jobs_packet_metering (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  metering_key TEXT NOT NULL,
  included INTEGER NOT NULL,
  amount_cents BIGINT NOT NULL DEFAULT 0,
  created_at_ms BIGINT NOT NULL,
  PRIMARY KEY (account_id, job_id),
  UNIQUE(account_id, metering_key)
);

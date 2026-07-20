-- Target: Postgres
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

CREATE TABLE IF NOT EXISTS jobs_answer_memory (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  scope TEXT NOT NULL,
  scope_id TEXT NOT NULL DEFAULT '',
  question_hash TEXT NOT NULL,
  answer_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, scope, scope_id, question_hash)
);
CREATE INDEX IF NOT EXISTS idx_jobs_answer_memory_account
  ON jobs_answer_memory(account_id, updated_at_ms DESC);

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

CREATE TABLE IF NOT EXISTS jobs_application_identities (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  email_hash TEXT NOT NULL UNIQUE,
  identity_json TEXT NOT NULL,
  verification_status TEXT NOT NULL DEFAULT 'pending',
  is_default INTEGER NOT NULL DEFAULT 0,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_application_identities_account
  ON jobs_application_identities(account_id, is_default DESC, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_identity_verifications (
  identity_id TEXT PRIMARY KEY REFERENCES jobs_application_identities(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  otp_hash TEXT NOT NULL,
  attempts BIGINT NOT NULL DEFAULT 0,
  expires_at_ms BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_identity_verifications_expiry
  ON jobs_identity_verifications(expires_at_ms);

CREATE TABLE IF NOT EXISTS jobs_mailbox_connections (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  provider_subject_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  connection_json TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, provider, provider_subject_hash)
);
CREATE INDEX IF NOT EXISTS idx_jobs_mailbox_connections_account
  ON jobs_mailbox_connections(account_id, status, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_application_evidence (
  id                    TEXT PRIMARY KEY,
  account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id        TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  kind                  TEXT NOT NULL,
  provider_event_hash   TEXT NOT NULL,
  evidence_json         TEXT NOT NULL,
  occurred_at_ms        BIGINT NOT NULL,
  created_at_ms         BIGINT NOT NULL,
  UNIQUE(account_id, provider_event_hash)
);
CREATE INDEX IF NOT EXISTS idx_jobs_application_evidence_application
  ON jobs_application_evidence(account_id, application_id, occurred_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_local_run_tickets (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  ticket_hash TEXT NOT NULL UNIQUE,
  ticket_secret TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'queued',
  expires_at_ms BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_local_run_tickets_expiry
  ON jobs_local_run_tickets(expires_at_ms, status);
CREATE INDEX IF NOT EXISTS idx_jobs_local_run_tickets_account
  ON jobs_local_run_tickets(account_id, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_attempt_reservations (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  company_key TEXT NOT NULL,
  period_key TEXT NOT NULL,
  runner TEXT NOT NULL DEFAULT 'unassigned',
  status TEXT NOT NULL DEFAULT 'reserved',
  reserved_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, application_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_period
  ON jobs_attempt_reservations(account_id, period_key, status, reserved_at_ms);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_active_company
  ON jobs_attempt_reservations(account_id, company_key)
  WHERE status IN ('reserved', 'running', 'side_effect_unknown', 'submitted');

CREATE TABLE IF NOT EXISTS jobs_discovery_sources (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  track_id TEXT NOT NULL DEFAULT '',
  provider TEXT NOT NULL,
  source_key TEXT NOT NULL,
  source_json TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active',
  health TEXT NOT NULL DEFAULT 'waiting',
  consecutive_failures BIGINT NOT NULL DEFAULT 0,
  run_interval_ms BIGINT NOT NULL DEFAULT 900000,
  next_run_at_ms BIGINT NOT NULL,
  last_success_at_ms BIGINT,
  last_failure_at_ms BIGINT,
  last_error_code TEXT,
  lease_owner TEXT,
  lease_token TEXT,
  lease_expires_at_ms BIGINT,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE(account_id, provider, source_key, track_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_discovery_sources_due
  ON jobs_discovery_sources(status, health, next_run_at_ms, lease_expires_at_ms);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_discovery_sources_board_owner
  ON jobs_discovery_sources(account_id, provider, source_key);

CREATE TABLE IF NOT EXISTS jobs_discovery_runs (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL REFERENCES jobs_discovery_sources(id) ON DELETE CASCADE,
  replay_key TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'running',
  discovered_count BIGINT NOT NULL DEFAULT 0,
  upserted_count BIGINT NOT NULL DEFAULT 0,
  closed_count BIGINT NOT NULL DEFAULT 0,
  error_code TEXT,
  snapshot_hash TEXT,
  started_at_ms BIGINT NOT NULL,
  completed_at_ms BIGINT,
  UNIQUE(source_id, replay_key)
);
CREATE INDEX IF NOT EXISTS idx_jobs_discovery_runs_source
  ON jobs_discovery_runs(source_id, started_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_discovery_memberships (
  source_id TEXT NOT NULL REFERENCES jobs_discovery_sources(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  external_id TEXT NOT NULL,
  canonical_key TEXT NOT NULL,
  job_id TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
  content_hash TEXT NOT NULL,
  first_seen_at_ms BIGINT NOT NULL,
  last_seen_at_ms BIGINT NOT NULL,
  last_seen_run_id TEXT NOT NULL,
  availability_status TEXT NOT NULL DEFAULT 'active',
  missing_count BIGINT NOT NULL DEFAULT 0,
  missing_since_at_ms BIGINT,
  PRIMARY KEY(source_id, external_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_discovery_memberships_job
  ON jobs_discovery_memberships(account_id, job_id);

CREATE TABLE IF NOT EXISTS jobs_execution_leases (
  run_id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  browser_profile_id TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  lease_token_sha256 TEXT NOT NULL,
  fence BIGINT NOT NULL CHECK(fence > 0),
  phase TEXT NOT NULL CHECK(phase IN (
    'prepared', 'click_started', 'submitted', 'failed',
    'side_effect_unknown', 'released'
  )),
  lease_expires_at_ms BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  finished_at_ms BIGINT
);
CREATE INDEX IF NOT EXISTS idx_jobs_execution_leases_binding
  ON jobs_execution_leases(account_id, application_id, run_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_application
  ON jobs_execution_leases(application_id)
  WHERE phase IN ('prepared', 'click_started');
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_profile
  ON jobs_execution_leases(browser_profile_id)
  WHERE phase IN ('prepared', 'click_started');

CREATE TABLE IF NOT EXISTS jobs_local_run_resume_actions (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES jobs_local_run_tickets(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  intervention_id TEXT NOT NULL UNIQUE REFERENCES jobs_interventions(id) ON DELETE CASCADE,
  action TEXT NOT NULL CHECK(action = 'approve_submission'),
  status TEXT NOT NULL CHECK(status IN ('approved', 'consumed')),
  expires_at_ms BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  consumed_at_ms BIGINT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_active_run
  ON jobs_local_run_resume_actions(run_id)
  WHERE status = 'approved';
CREATE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_application
  ON jobs_local_run_resume_actions(account_id, application_id, created_at_ms DESC);

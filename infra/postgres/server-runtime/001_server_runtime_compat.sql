-- Bluey server-runtime compatibility schema.
-- Target: Postgres 16+ with pgvector enabled.
--
-- This intentionally mirrors the active SQLite-backed bluey-server schema.
-- It is the safe managed Postgres target while Bluey has no users and before
-- the later normalized cloud schema replaces the runtime tables.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  email_verified_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_login_at TIMESTAMPTZ,
  balance_cents BIGINT NOT NULL DEFAULT 0,
  reserved_cents BIGINT NOT NULL DEFAULT 0,
  trial_seconds_remaining BIGINT NOT NULL DEFAULT 600,
  auto_topup_enabled INTEGER NOT NULL DEFAULT 0,
  auto_topup_threshold_cents BIGINT NOT NULL DEFAULT 1000,
  auto_topup_amount_cents BIGINT NOT NULL DEFAULT 3000,
  stripe_customer_id TEXT,
  stripe_payment_method_id TEXT,
  square_customer_id TEXT,
  square_card_id TEXT,
  square_card_brand TEXT,
  square_card_last4 TEXT,
  is_admin INTEGER NOT NULL DEFAULT 0,
  billing_restricted INTEGER NOT NULL DEFAULT 0,
  billing_restriction_reason TEXT,
  billing_restricted_at TIMESTAMPTZ,
  CHECK (balance_cents >= 0),
  CHECK (reserved_cents >= 0),
  CHECK (trial_seconds_remaining >= 0),
  CHECK (auto_topup_threshold_cents >= 0),
  CHECK (auto_topup_amount_cents >= 0),
  CHECK (auto_topup_amount_cents = 0 OR auto_topup_amount_cents > auto_topup_threshold_cents)
);
CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);

CREATE TABLE IF NOT EXISTS credit_batches (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  amount_cents BIGINT NOT NULL,
  remaining_cents BIGINT NOT NULL,
  purchased_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  stripe_charge_id TEXT,
  expired_at TIMESTAMPTZ,
  CHECK (amount_cents > 0),
  CHECK (remaining_cents >= 0),
  CHECK (remaining_cents <= amount_cents)
);
CREATE INDEX IF NOT EXISTS idx_credit_batches_account
  ON credit_batches(account_id, expires_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_credit_batches_stripe_charge
  ON credit_batches(stripe_charge_id)
  WHERE stripe_charge_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS refresh_tokens (
  token_hash TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  device_label TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_used_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_account
  ON refresh_tokens(account_id);

CREATE TABLE IF NOT EXISTS device_codes (
  device_code TEXT PRIMARY KEY,
  user_code TEXT NOT NULL UNIQUE,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  approved INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_device_codes_user_code
  ON device_codes(user_code);

CREATE TABLE IF NOT EXISTS usage_events (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  request_id TEXT NOT NULL,
  ts TIMESTAMPTZ NOT NULL DEFAULT now(),
  kind TEXT NOT NULL,
  task_type TEXT,
  lane TEXT,
  provider TEXT,
  model TEXT,
  input_tokens BIGINT NOT NULL DEFAULT 0,
  output_tokens BIGINT NOT NULL DEFAULT 0,
  latency_ms BIGINT NOT NULL DEFAULT 0,
  cost_cents_to_bluey BIGINT NOT NULL DEFAULT 0,
  cost_cents_to_customer BIGINT NOT NULL DEFAULT 0,
  was_speculative INTEGER NOT NULL DEFAULT 0,
  was_fallback INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_usage_events_account_ts
  ON usage_events(account_id, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_request
  ON usage_events(request_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_events_dedupe
  ON usage_events(account_id, request_id, kind);

CREATE TABLE IF NOT EXISTS stripe_webhook_events (
  event_id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  processed_at TIMESTAMPTZ,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS request_idempotency (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  request_id TEXT NOT NULL,
  status TEXT NOT NULL,
  response_json TEXT,
  http_status INTEGER,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ,
  PRIMARY KEY (account_id, request_id)
);
CREATE INDEX IF NOT EXISTS idx_request_idempotency_created
  ON request_idempotency(created_at);

CREATE TABLE IF NOT EXISTS email_verification_tokens (
  token_hash TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  consumed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_email_verify_account
  ON email_verification_tokens(account_id);

CREATE TABLE IF NOT EXISTS password_reset_tokens (
  token_hash TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  consumed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_password_reset_account
  ON password_reset_tokens(account_id);

CREATE TABLE IF NOT EXISTS auth_link_codes (
  code_hash TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  access_token TEXT NOT NULL,
  refresh_token TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  consumed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_auth_link_codes_account
  ON auth_link_codes(account_id);

CREATE TABLE IF NOT EXISTS cloud_sessions (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  session_id TEXT NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  last_active_at_ms BIGINT,
  answer_style TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  deleted_at_ms BIGINT,
  PRIMARY KEY (account_id, session_id)
);
CREATE INDEX IF NOT EXISTS idx_cloud_sessions_account_updated
  ON cloud_sessions(account_id, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS cloud_transcript_segments (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  segment_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  speaker TEXT NOT NULL,
  source TEXT NOT NULL,
  text TEXT NOT NULL,
  start_ms BIGINT,
  end_ms BIGINT,
  ts_ms BIGINT NOT NULL,
  is_final INTEGER NOT NULL DEFAULT 1,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  PRIMARY KEY (account_id, segment_id)
);
CREATE INDEX IF NOT EXISTS idx_cloud_transcript_session_ts
  ON cloud_transcript_segments(account_id, session_id, ts_ms);

CREATE TABLE IF NOT EXISTS cloud_cue_responses (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  response_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  source_text TEXT,
  ts_ms BIGINT NOT NULL,
  provider TEXT,
  model TEXT,
  lane TEXT,
  task_type TEXT,
  cost_cents BIGINT,
  balance_cents_after BIGINT,
  cost_label TEXT,
  artifact_type TEXT,
  artifact_body TEXT,
  artifact_confidence DOUBLE PRECISION,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  PRIMARY KEY (account_id, response_id)
);
CREATE INDEX IF NOT EXISTS idx_cloud_responses_session_ts
  ON cloud_cue_responses(account_id, session_id, ts_ms);

CREATE TABLE IF NOT EXISTS cloud_context_artifacts (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  artifact_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  note TEXT,
  source_uri TEXT,
  content_hash TEXT,
  text_preview TEXT,
  created_at_ms BIGINT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  PRIMARY KEY (account_id, artifact_id)
);
CREATE INDEX IF NOT EXISTS idx_cloud_context_session
  ON cloud_context_artifacts(account_id, session_id);

CREATE TABLE IF NOT EXISTS cloud_rag_chunks (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  chunk_id TEXT NOT NULL,
  session_id TEXT,
  source_kind TEXT NOT NULL,
  source_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  text TEXT NOT NULL,
  embedding_json TEXT,
  embedding vector(1536),
  embedding_model TEXT,
  token_count BIGINT,
  content_hash TEXT,
  updated_at_ms BIGINT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  PRIMARY KEY (account_id, chunk_id)
);
CREATE INDEX IF NOT EXISTS idx_cloud_rag_account_source
  ON cloud_rag_chunks(account_id, source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_cloud_rag_session
  ON cloud_rag_chunks(account_id, session_id);
CREATE INDEX IF NOT EXISTS idx_cloud_rag_embedding
  ON cloud_rag_chunks USING ivfflat (embedding vector_cosine_ops)
  WITH (lists = 100)
  WHERE embedding IS NOT NULL;

CREATE TABLE IF NOT EXISTS stt_sessions (
  session_token TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  bluey_session_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  source TEXT NOT NULL,
  mode TEXT NOT NULL,
  max_seconds BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  expires_at_ms BIGINT NOT NULL,
  consumed_seconds BIGINT NOT NULL DEFAULT 0,
  started_at_ms BIGINT,
  ended_at_ms BIGINT,
  relay_close_reason TEXT,
  reserved_cents BIGINT NOT NULL DEFAULT 0,
  settled_cents BIGINT NOT NULL DEFAULT 0,
  refunded_cents BIGINT NOT NULL DEFAULT 0,
  reserved_trial_seconds BIGINT NOT NULL DEFAULT 0,
  settled_trial_seconds BIGINT NOT NULL DEFAULT 0,
  refunded_trial_seconds BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_stt_sessions_account_exp
  ON stt_sessions(account_id, expires_at_ms);

CREATE TABLE IF NOT EXISTS signup_otps (
  email TEXT PRIMARY KEY,
  otp_hash TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  attempts BIGINT NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_signup_otps_expires_at
  ON signup_otps(expires_at);

CREATE TABLE IF NOT EXISTS trial_grants (
  id TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  email_hash TEXT NOT NULL,
  email_domain_hash TEXT,
  ip_hash TEXT,
  device_hash TEXT,
  user_agent_hash TEXT,
  ip_user_agent_hash TEXT,
  granted_seconds BIGINT NOT NULL DEFAULT 600,
  decision TEXT NOT NULL,
  reason TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE trial_grants
  ADD COLUMN IF NOT EXISTS email_domain_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_trial_grants_email
  ON trial_grants(email_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_grants_email_domain
  ON trial_grants(email_domain_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_grants_ip
  ON trial_grants(ip_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_grants_device
  ON trial_grants(device_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_grants_ip_ua
  ON trial_grants(ip_user_agent_hash, created_at);

CREATE TABLE IF NOT EXISTS trial_abuse_events (
  id TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  email_hash TEXT,
  email_domain_hash TEXT,
  ip_hash TEXT,
  device_hash TEXT,
  user_agent_hash TEXT,
  ip_user_agent_hash TEXT,
  event_type TEXT NOT NULL,
  severity BIGINT NOT NULL DEFAULT 1,
  reason TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE trial_abuse_events
  ADD COLUMN IF NOT EXISTS email_domain_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_created
  ON trial_abuse_events(created_at);
CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_email
  ON trial_abuse_events(email_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_email_domain
  ON trial_abuse_events(email_domain_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_ip
  ON trial_abuse_events(ip_hash, created_at);
CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_device
  ON trial_abuse_events(device_hash, created_at);

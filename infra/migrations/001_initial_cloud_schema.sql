-- Bluey Cloud initial schema outline.
-- Target: Postgres 16+ with pgvector enabled.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE users (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  display_name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  disabled_at TIMESTAMPTZ
);

CREATE TABLE workspaces (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  plan TEXT NOT NULL DEFAULT 'free',
  retention_days INTEGER NOT NULL DEFAULT 90,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE workspace_members (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  user_id TEXT NOT NULL REFERENCES users(id),
  role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (workspace_id, user_id)
);

CREATE TABLE devices (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  user_id TEXT NOT NULL REFERENCES users(id),
  platform TEXT NOT NULL CHECK (platform IN ('macos', 'windows')),
  app_version TEXT NOT NULL,
  capabilities JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
  last_seen_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  device_id TEXT NOT NULL REFERENCES devices(id),
  refresh_token_hash TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE settings_profiles (
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
  profile_version BIGINT NOT NULL DEFAULT 1,
  capture JSONB NOT NULL DEFAULT '{}'::jsonb,
  privacy JSONB NOT NULL DEFAULT '{}'::jsonb,
  answering JSONB NOT NULL DEFAULT '{}'::jsonb,
  shortcuts JSONB NOT NULL DEFAULT '{}'::jsonb,
  limits JSONB NOT NULL DEFAULT '{}'::jsonb,
  updated_by_user_id TEXT REFERENCES users(id),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE wallets (
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
  balance_cents BIGINT NOT NULL DEFAULT 0,
  reserved_cents BIGINT NOT NULL DEFAULT 0,
  trial_seconds_remaining BIGINT NOT NULL DEFAULT 900,
  auto_topup_enabled BOOLEAN NOT NULL DEFAULT false,
  auto_topup_threshold_cents BIGINT NOT NULL DEFAULT 500,
  auto_topup_amount_cents BIGINT NOT NULL DEFAULT 1500,
  billing_restricted BOOLEAN NOT NULL DEFAULT false,
  billing_restriction_reason TEXT,
  billing_restricted_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK (balance_cents >= 0),
  CHECK (reserved_cents >= 0),
  CHECK (trial_seconds_remaining >= 0),
  CHECK (auto_topup_amount_cents = 0 OR auto_topup_amount_cents >= 1500),
  CHECK (auto_topup_threshold_cents >= 0),
  CHECK (auto_topup_amount_cents = 0 OR auto_topup_amount_cents > auto_topup_threshold_cents)
);

CREATE TABLE billing_profiles (
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
  active_provider TEXT NOT NULL CHECK (active_provider IN ('square', 'stripe', 'internal')),
  square_customer_id TEXT,
  square_card_id TEXT,
  square_card_brand TEXT,
  square_card_last4 TEXT,
  stripe_customer_id TEXT,
  stripe_payment_method_id TEXT,
  card_saved_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE credit_batches (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  amount_cents BIGINT NOT NULL,
  remaining_cents BIGINT NOT NULL,
  credit_source_type TEXT NOT NULL CHECK (credit_source_type IN ('processor_payment', 'internal_credit')),
  processor TEXT CHECK (processor IN ('square', 'stripe')),
  processor_payment_id TEXT,
  internal_reason TEXT,
  purchased_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  expired_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ,
  revoked_reason TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  CHECK (amount_cents > 0),
  CHECK (remaining_cents >= 0),
  CHECK (remaining_cents <= amount_cents),
  CHECK (
    (credit_source_type = 'processor_payment' AND processor IS NOT NULL AND processor_payment_id IS NOT NULL)
    OR (credit_source_type = 'internal_credit' AND internal_reason IS NOT NULL)
  )
);
CREATE UNIQUE INDEX idx_credit_batches_processor_payment
  ON credit_batches(processor, processor_payment_id)
  WHERE processor_payment_id IS NOT NULL;
CREATE INDEX idx_credit_batches_workspace_expiry
  ON credit_batches(workspace_id, expires_at, purchased_at);

CREATE TABLE payment_events (
  id TEXT PRIMARY KEY,
  workspace_id TEXT REFERENCES workspaces(id),
  provider TEXT NOT NULL CHECK (provider IN ('square', 'stripe')),
  environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
  processor_event_id TEXT NOT NULL,
  processor_payment_id TEXT,
  event_type TEXT NOT NULL,
  status TEXT NOT NULL,
  amount_cents BIGINT,
  currency TEXT,
  raw_object JSONB NOT NULL,
  received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  processed_at TIMESTAMPTZ,
  ignored_at TIMESTAMPTZ,
  error TEXT
);
CREATE UNIQUE INDEX idx_payment_events_provider_event
  ON payment_events(provider, processor_event_id);
CREATE INDEX idx_payment_events_workspace_received
  ON payment_events(workspace_id, received_at DESC);

CREATE TABLE reload_attempts (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  provider TEXT NOT NULL CHECK (provider IN ('square', 'stripe')),
  trigger TEXT NOT NULL CHECK (trigger IN ('manual', 'auto')),
  idempotency_key TEXT NOT NULL,
  amount_cents BIGINT NOT NULL,
  threshold_cents BIGINT,
  status TEXT NOT NULL CHECK (status IN ('started', 'requires_action', 'succeeded', 'failed', 'cancelled')),
  processor_payment_id TEXT,
  processor_checkout_id TEXT,
  failure_code TEXT,
  failure_message TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ,
  CHECK (amount_cents >= 1500)
);
CREATE UNIQUE INDEX idx_reload_attempts_provider_idem
  ON reload_attempts(provider, idempotency_key);
CREATE INDEX idx_reload_attempts_workspace_created
  ON reload_attempts(workspace_id, created_at DESC);

CREATE TABLE request_idempotency (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  request_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('in_progress', 'complete', 'failed')),
  route TEXT,
  response_json JSONB,
  http_status INTEGER,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ,
  PRIMARY KEY (workspace_id, request_id)
);
CREATE INDEX idx_request_idempotency_created
  ON request_idempotency(created_at);

CREATE TABLE usage_events (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  user_id TEXT REFERENCES users(id),
  meeting_id TEXT,
  request_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('llm', 'embed', 'stt', 'vision', 'rerank')),
  task_type TEXT,
  lane TEXT,
  provider TEXT,
  model TEXT,
  input_tokens BIGINT NOT NULL DEFAULT 0,
  output_tokens BIGINT NOT NULL DEFAULT 0,
  audio_seconds NUMERIC(12, 3) NOT NULL DEFAULT 0,
  latency_ms BIGINT NOT NULL DEFAULT 0,
  cost_cents_to_bluey BIGINT NOT NULL DEFAULT 0,
  cost_cents_to_customer BIGINT NOT NULL DEFAULT 0,
  was_speculative BOOLEAN NOT NULL DEFAULT false,
  was_fallback BOOLEAN NOT NULL DEFAULT false,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_usage_events_dedupe
  ON usage_events(workspace_id, request_id, kind);
CREATE INDEX idx_usage_events_workspace_created
  ON usage_events(workspace_id, created_at DESC);

CREATE TABLE stt_sessions (
  session_token_hash TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  meeting_id TEXT,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  source TEXT NOT NULL,
  mode TEXT NOT NULL,
  max_seconds INTEGER NOT NULL,
  reserved_cents BIGINT NOT NULL DEFAULT 0,
  settled_cents BIGINT NOT NULL DEFAULT 0,
  refunded_cents BIGINT NOT NULL DEFAULT 0,
  reserved_trial_seconds BIGINT NOT NULL DEFAULT 0,
  settled_trial_seconds BIGINT NOT NULL DEFAULT 0,
  refunded_trial_seconds BIGINT NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  started_at TIMESTAMPTZ,
  ended_at TIMESTAMPTZ,
  relay_close_reason TEXT,
  CHECK (reserved_cents >= 0),
  CHECK (settled_cents >= 0),
  CHECK (refunded_cents >= 0)
);
CREATE INDEX idx_stt_sessions_workspace_expiry
  ON stt_sessions(workspace_id, expires_at);

CREATE TABLE meetings (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  device_id TEXT REFERENCES devices(id),
  source_meeting_id TEXT NOT NULL,
  title TEXT NOT NULL,
  summary TEXT,
  started_at TIMESTAMPTZ NOT NULL,
  ended_at TIMESTAMPTZ,
  tombstoned_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (workspace_id, source_meeting_id)
);

CREATE TABLE meeting_events (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  meeting_id TEXT NOT NULL REFERENCES meetings(id),
  source_event_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  occurred_at TIMESTAMPTZ NOT NULL,
  payload JSONB NOT NULL,
  tombstoned_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (workspace_id, source_event_id)
);

CREATE TABLE artifacts (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  meeting_id TEXT REFERENCES meetings(id),
  created_by_device_id TEXT REFERENCES devices(id),
  source_event_id TEXT,
  object_key TEXT NOT NULL,
  content_type TEXT NOT NULL,
  byte_size BIGINT NOT NULL,
  sha256 TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'uploaded', 'processed', 'deleted', 'failed')),
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  tombstoned_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memory_chunks (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  meeting_id TEXT REFERENCES meetings(id),
  artifact_id TEXT REFERENCES artifacts(id),
  source_type TEXT NOT NULL,
  source_id TEXT NOT NULL,
  chunk_type TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  embedding VECTOR(1536),
  embedded_model TEXT,
  occurred_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ,
  tombstoned_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE answer_runs (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  meeting_id TEXT REFERENCES meetings(id),
  user_id TEXT REFERENCES users(id),
  question TEXT NOT NULL,
  mode TEXT NOT NULL,
  route TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('started', 'complete', 'failed', 'cancelled')),
  model TEXT,
  provider TEXT,
  latency_ms INTEGER,
  estimated_cost_usd NUMERIC(12, 6),
  final_text TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ
);

CREATE TABLE rag_citations (
  id TEXT PRIMARY KEY,
  answer_run_id TEXT NOT NULL REFERENCES answer_runs(id),
  chunk_id TEXT NOT NULL REFERENCES memory_chunks(id),
  rank INTEGER NOT NULL,
  score DOUBLE PRECISION NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE audit_log (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  actor_user_id TEXT REFERENCES users(id),
  actor_device_id TEXT REFERENCES devices(id),
  action TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE diagnostic_log_chunks (
  id TEXT PRIMARY KEY,
  workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
  user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
  meeting_id TEXT REFERENCES meetings(id) ON DELETE SET NULL,
  session_id TEXT,
  session_code TEXT,
  kind TEXT NOT NULL,
  storage TEXT NOT NULL CHECK (storage IN ('r2', 's3', 'local', 'filesystem')),
  object_key TEXT,
  local_path TEXT,
  byte_size BIGINT NOT NULL DEFAULT 0,
  sha256 TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE export_requests (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  requested_by_user_id TEXT NOT NULL REFERENCES users(id),
  scope TEXT NOT NULL CHECK (scope IN ('user', 'workspace', 'meeting')),
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'complete', 'failed')),
  object_key TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ
);

CREATE TABLE deletion_requests (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  requested_by_user_id TEXT NOT NULL REFERENCES users(id),
  scope TEXT NOT NULL CHECK (scope IN ('user', 'workspace', 'meeting')),
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'complete', 'failed')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ
);

CREATE INDEX idx_meetings_workspace_started ON meetings (workspace_id, started_at DESC);
CREATE INDEX idx_events_workspace_meeting ON meeting_events (workspace_id, meeting_id, occurred_at);
CREATE INDEX idx_artifacts_workspace_meeting ON artifacts (workspace_id, meeting_id);
CREATE INDEX idx_chunks_workspace_meeting ON memory_chunks (workspace_id, meeting_id, created_at DESC);
CREATE INDEX idx_chunks_workspace_expiry ON memory_chunks (workspace_id, expires_at);
CREATE INDEX idx_answer_runs_workspace_created ON answer_runs (workspace_id, created_at DESC);
CREATE INDEX idx_audit_workspace_created ON audit_log (workspace_id, created_at DESC);
CREATE INDEX idx_diagnostic_logs_workspace_created
  ON diagnostic_log_chunks (workspace_id, created_at DESC);
CREATE INDEX idx_diagnostic_logs_meeting_created
  ON diagnostic_log_chunks (workspace_id, meeting_id, created_at DESC);
CREATE INDEX idx_diagnostic_logs_expires
  ON diagnostic_log_chunks (expires_at);

-- Add after embedding dimension and provider are finalized:
-- CREATE INDEX idx_chunks_embedding ON memory_chunks USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

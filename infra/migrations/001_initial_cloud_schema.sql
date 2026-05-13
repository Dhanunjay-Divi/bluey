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

-- Add after embedding dimension and provider are finalized:
-- CREATE INDEX idx_chunks_embedding ON memory_chunks USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);


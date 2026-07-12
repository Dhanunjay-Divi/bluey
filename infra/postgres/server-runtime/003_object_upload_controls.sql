-- Durable, account-scoped controls for customer object uploads.
--
-- Postgres is the source of truth for quota reservations and object lifecycle
-- state. R2/S3 receives only idempotent writes described by this ledger.

CREATE TABLE IF NOT EXISTS object_uploads (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  object_kind TEXT NOT NULL CHECK (object_kind IN ('artifact', 'session_audit')),
  logical_id TEXT NOT NULL,
  session_id TEXT,
  storage_scope TEXT NOT NULL CHECK (storage_scope IN ('artifact', 'audit')),
  object_key TEXT NOT NULL,
  size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
  sha256 TEXT NOT NULL CHECK (char_length(sha256) = 64),
  content_type TEXT NOT NULL,
  expires_at_ms BIGINT NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending'
    CHECK (state IN ('pending', 'ready', 'delete_pending', 'deleted')),
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  uploaded_at_ms BIGINT,
  deleted_at_ms BIGINT,
  UNIQUE (account_id, object_kind, logical_id),
  UNIQUE (storage_scope, object_key)
);

CREATE INDEX IF NOT EXISTS idx_object_uploads_account_state
  ON object_uploads(account_id, state, created_at_ms);
CREATE INDEX IF NOT EXISTS idx_object_uploads_session
  ON object_uploads(account_id, session_id, state);
CREATE INDEX IF NOT EXISTS idx_object_uploads_cleanup
  ON object_uploads(storage_scope, state, expires_at_ms, updated_at_ms);

CREATE TABLE IF NOT EXISTS object_upload_daily_usage (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  day_start_ms BIGINT NOT NULL,
  reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK (reserved_bytes >= 0),
  reserved_objects BIGINT NOT NULL DEFAULT 0 CHECK (reserved_objects >= 0),
  updated_at_ms BIGINT NOT NULL,
  PRIMARY KEY (account_id, day_start_ms)
);

CREATE TABLE IF NOT EXISTS object_storage_outbox (
  id TEXT PRIMARY KEY,
  upload_id TEXT NOT NULL REFERENCES object_uploads(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  operation TEXT NOT NULL CHECK (operation IN ('put', 'delete')),
  state TEXT NOT NULL DEFAULT 'pending'
    CHECK (state IN ('pending', 'processing', 'retry', 'completed', 'abandoned')),
  attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
  next_attempt_at_ms BIGINT NOT NULL,
  last_error TEXT,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  completed_at_ms BIGINT,
  UNIQUE (upload_id, operation)
);

CREATE INDEX IF NOT EXISTS idx_object_storage_outbox_due
  ON object_storage_outbox(operation, state, next_attempt_at_ms, updated_at_ms);
CREATE INDEX IF NOT EXISTS idx_object_storage_outbox_account
  ON object_storage_outbox(account_id, operation, state);

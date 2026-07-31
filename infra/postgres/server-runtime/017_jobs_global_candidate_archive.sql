-- Target: PostgreSQL 14+
-- Stable candidate change detection and verified R2 archival metadata.

ALTER TABLE jobs_global_candidates
  ADD COLUMN IF NOT EXISTS content_hash TEXT NOT NULL DEFAULT '',
  ADD COLUMN IF NOT EXISTS archive_state TEXT NOT NULL DEFAULT 'hot',
  ADD COLUMN IF NOT EXISTS archive_storage_key TEXT,
  ADD COLUMN IF NOT EXISTS archive_sha256 TEXT,
  ADD COLUMN IF NOT EXISTS archive_size_bytes BIGINT,
  ADD COLUMN IF NOT EXISTS archived_at_ms BIGINT,
  ADD COLUMN IF NOT EXISTS archive_attempt_count BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS archive_next_attempt_at_ms BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS archive_lease_owner TEXT,
  ADD COLUMN IF NOT EXISTS archive_lease_expires_at_ms BIGINT;

CREATE INDEX IF NOT EXISTS idx_jobs_global_candidates_archive_due
  ON jobs_global_candidates(
    archive_state, availability_status, updated_at_ms, archive_next_attempt_at_ms
  );

-- Durable metadata for encrypted Bluey Browser profile snapshots.
--
-- Snapshot bytes live in account-scoped object storage. PostgreSQL/SQLite
-- remains the authority for generation fencing and exact runner ownership.
CREATE TABLE IF NOT EXISTS jobs_browser_profile_snapshots (
  account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  browser_profile_id TEXT NOT NULL,
  generation        INTEGER NOT NULL CHECK(generation > 0),
  object_key        TEXT NOT NULL,
  sha256            TEXT NOT NULL CHECK(length(sha256) = 64),
  size_bytes        INTEGER NOT NULL CHECK(size_bytes > 0),
  envelope_version  INTEGER NOT NULL CHECK(envelope_version > 0),
  writer_run_id     TEXT NOT NULL,
  writer_fence      INTEGER NOT NULL CHECK(writer_fence > 0),
  updated_at_ms     INTEGER NOT NULL,
  PRIMARY KEY(account_id, browser_profile_id),
  UNIQUE(object_key)
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_profile_snapshots_writer
  ON jobs_browser_profile_snapshots(account_id, writer_run_id, writer_fence);

-- Target: SQLite
-- Storage only; no delegated read grant is automatically issued.
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS jobs_mcp_read_grants (
    token_sha256 TEXT PRIMARY KEY NOT NULL CHECK(length(token_sha256) = 64
      AND length(CAST(token_sha256 AS BLOB)) = 64
      AND token_sha256 NOT GLOB '*[^0-9a-f]*'),
    account_id TEXT NOT NULL,
    track_id TEXT NOT NULL,
    audience TEXT NOT NULL CHECK(audience = 'bluey-jobs-mcp-stdio-v1'),
    scope TEXT NOT NULL CHECK(scope = 'jobs:matches:read'),
    track_input_generation INTEGER NOT NULL CHECK(typeof(track_input_generation) = 'integer'
      AND track_input_generation > 0),
    track_input_transition_sha256 TEXT NOT NULL CHECK(length(track_input_transition_sha256) = 64
      AND length(CAST(track_input_transition_sha256 AS BLOB)) = 64
      AND track_input_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
    issued_at_ms INTEGER NOT NULL CHECK(typeof(issued_at_ms) = 'integer' AND issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK(typeof(expires_at_ms) = 'integer' AND expires_at_ms > issued_at_ms
      AND expires_at_ms - issued_at_ms <= 900000),
    revoked_at_ms INTEGER CHECK(revoked_at_ms IS NULL OR
      (typeof(revoked_at_ms) = 'integer' AND revoked_at_ms >= issued_at_ms)),
    FOREIGN KEY(account_id, track_id) REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_jobs_mcp_read_grants_account_track
  ON jobs_mcp_read_grants(account_id, track_id);

-- SQLite REPLACE deletes before inserting and does not fire UPDATE triggers.
CREATE TRIGGER IF NOT EXISTS trg_jobs_mcp_read_grant_no_replace
BEFORE INSERT ON jobs_mcp_read_grants
WHEN EXISTS (SELECT 1 FROM jobs_mcp_read_grants WHERE token_sha256 = NEW.token_sha256)
BEGIN
  SELECT RAISE(ABORT, 'Jobs MCP grant tokens cannot be replaced');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_mcp_read_grant_revoke_only
BEFORE UPDATE ON jobs_mcp_read_grants
WHEN NEW.token_sha256 IS NOT OLD.token_sha256
  OR NEW.account_id IS NOT OLD.account_id
  OR NEW.track_id IS NOT OLD.track_id
  OR NEW.audience IS NOT OLD.audience
  OR NEW.scope IS NOT OLD.scope
  OR NEW.track_input_generation IS NOT OLD.track_input_generation
  OR NEW.track_input_transition_sha256 IS NOT OLD.track_input_transition_sha256
  OR NEW.issued_at_ms IS NOT OLD.issued_at_ms
  OR NEW.expires_at_ms IS NOT OLD.expires_at_ms
  OR OLD.revoked_at_ms IS NOT NULL OR NEW.revoked_at_ms IS NULL
BEGIN
  SELECT RAISE(ABORT, 'Jobs MCP grant updates are limited to one-way revocation');
END;

COMMIT;

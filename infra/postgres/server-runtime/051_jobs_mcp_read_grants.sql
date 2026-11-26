-- Target: PostgreSQL
-- Storage for explicitly issued, read-only Jobs MCP capabilities. No grants
-- are created here. The consumer has no issuance or effectful tool authority.
CREATE TABLE IF NOT EXISTS jobs_mcp_read_grants (
    token_sha256 TEXT PRIMARY KEY CHECK(token_sha256 ~ '^[0-9a-f]{64}$'),
    account_id TEXT NOT NULL,
    track_id TEXT NOT NULL,
    audience TEXT NOT NULL CHECK(audience = 'bluey-jobs-mcp-stdio-v1'),
    scope TEXT NOT NULL CHECK(scope = 'jobs:matches:read'),
    track_input_generation BIGINT NOT NULL CHECK(track_input_generation > 0),
    track_input_transition_sha256 TEXT NOT NULL CHECK(track_input_transition_sha256 ~ '^[0-9a-f]{64}$'),
    issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms >= 0),
    expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms > issued_at_ms
      AND expires_at_ms - issued_at_ms <= 900000),
    revoked_at_ms BIGINT CHECK(revoked_at_ms >= issued_at_ms),
    FOREIGN KEY(account_id, track_id) REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_jobs_mcp_read_grants_account_track
  ON jobs_mcp_read_grants(account_id, track_id);

CREATE OR REPLACE FUNCTION jobs_mcp_read_grant_revoke_only()
RETURNS trigger LANGUAGE plpgsql AS $function$
BEGIN
  IF (NEW.token_sha256, NEW.account_id, NEW.track_id, NEW.audience, NEW.scope,
      NEW.track_input_generation, NEW.track_input_transition_sha256,
      NEW.issued_at_ms, NEW.expires_at_ms)
     IS DISTINCT FROM
     (OLD.token_sha256, OLD.account_id, OLD.track_id, OLD.audience, OLD.scope,
      OLD.track_input_generation, OLD.track_input_transition_sha256,
      OLD.issued_at_ms, OLD.expires_at_ms)
     OR OLD.revoked_at_ms IS NOT NULL OR NEW.revoked_at_ms IS NULL THEN
    RAISE EXCEPTION 'Jobs MCP grant updates are limited to one-way revocation'
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$function$;

CREATE OR REPLACE TRIGGER trg_jobs_mcp_read_grant_revoke_only
BEFORE UPDATE ON jobs_mcp_read_grants
FOR EACH ROW EXECUTE FUNCTION jobs_mcp_read_grant_revoke_only();

-- Target: Postgres only
-- Immutable Career Track evidence snapshots and resume-claim provenance.

BEGIN;

CREATE TABLE IF NOT EXISTS jobs_profile_evidence_revisions (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  career_track_id     TEXT NOT NULL REFERENCES jobs_tracks(id) ON DELETE CASCADE,
  revision_no         BIGINT NOT NULL,
  content_hash        TEXT NOT NULL,
  snapshot_json       TEXT NOT NULL,
  created_at_ms       BIGINT NOT NULL,
  UNIQUE(account_id, career_track_id, revision_no),
  UNIQUE(account_id, career_track_id, content_hash)
);

CREATE INDEX IF NOT EXISTS idx_jobs_evidence_revisions_track
  ON jobs_profile_evidence_revisions(account_id, career_track_id, revision_no DESC);

CREATE TABLE IF NOT EXISTS jobs_resume_claim_evidence (
  id                    TEXT PRIMARY KEY,
  account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  resume_version_id     TEXT NOT NULL REFERENCES jobs_resume_versions(id) ON DELETE CASCADE,
  claim_id              TEXT NOT NULL,
  evidence_revision_id TEXT NOT NULL REFERENCES jobs_profile_evidence_revisions(id) ON DELETE RESTRICT,
  source_ids_json       TEXT NOT NULL,
  claim_json            TEXT NOT NULL,
  created_at_ms         BIGINT NOT NULL,
  UNIQUE(account_id, resume_version_id, claim_id)
);

CREATE INDEX IF NOT EXISTS idx_jobs_claim_evidence_revision
  ON jobs_resume_claim_evidence(account_id, evidence_revision_id, created_at_ms DESC);

COMMIT;

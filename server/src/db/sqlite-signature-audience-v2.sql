-- Private Phase657 fragment, not a standalone operator migration.
-- The Rust owner supplies schema validation, transaction/lock and ledger boundaries.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

CREATE TABLE jobs_managed_cloud_signature_sets_phase657 (
  signature_set_sha256 TEXT PRIMARY KEY
    CHECK(length(signature_set_sha256) = 64
      AND lower(signature_set_sha256) = signature_set_sha256
      AND signature_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  signature_set_id TEXT NOT NULL UNIQUE CHECK(length(signature_set_id) > 0),
  trust_generation INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role TEXT NOT NULL CHECK(role IN (
    'general_promotion', 'incident', 'promotion', 'release', 'root'
  )),
  target_audience TEXT NOT NULL CHECK(target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1',
    'bluey-jobs-managed-cloud-activation-v2',
    'bluey-jobs-managed-cloud-activation-v3',
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
    'bluey-jobs-managed-cloud-release-v2',
    'bluey-jobs-managed-cloud-release-v3',
    'bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1',
    'bluey-jobs-managed-cloud-trust-policy-v1',
    'bluey-jobs-managed-cloud-provisioning-v4',
    'bluey-jobs-managed-cloud-preproduction-context-v1',
    'bluey-jobs-managed-cloud-serving-latch-v1',
    'bluey-jobs-managed-cloud-process-attachment-v1'
  )),
  target_sha256 TEXT NOT NULL
    CHECK(length(target_sha256) = 64 AND lower(target_sha256) = target_sha256
      AND target_sha256 NOT GLOB '*[^0-9a-f]*'),
  signed_at_ms INTEGER NOT NULL CHECK(signed_at_ms >= 0),
  signature_count INTEGER NOT NULL CHECK(signature_count BETWEEN 1 AND 32),
  canonical_signature_set_base64url TEXT NOT NULL
    CHECK(length(canonical_signature_set_base64url) > 0),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms >= signed_at_ms),
  UNIQUE(target_audience, target_sha256, trust_generation, role)
);
INSERT INTO jobs_managed_cloud_signature_sets_phase657 (
  signature_set_sha256, signature_set_id, trust_generation, role,
  target_audience, target_sha256, signed_at_ms, signature_count,
  canonical_signature_set_base64url, recorded_by, recorded_at_ms
) SELECT
  signature_set_sha256, signature_set_id, trust_generation, role,
  target_audience, target_sha256, signed_at_ms, signature_count,
  canonical_signature_set_base64url, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_signature_sets;

DROP TRIGGER trg_jobs_managed_cloud_signature_sets_no_update;
DROP TRIGGER trg_jobs_managed_cloud_signature_sets_no_delete;
DROP TABLE jobs_managed_cloud_signature_sets;
ALTER TABLE jobs_managed_cloud_signature_sets_phase657
  RENAME TO jobs_managed_cloud_signature_sets;
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_update
BEFORE UPDATE ON jobs_managed_cloud_signature_sets
BEGIN SELECT RAISE(ABORT, 'managed cloud signature set is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_delete
BEFORE DELETE ON jobs_managed_cloud_signature_sets
BEGIN SELECT RAISE(ABORT, 'managed cloud signature set is immutable'); END;
CREATE INDEX idx_jobs_managed_cloud_signature_sets_target
ON jobs_managed_cloud_signature_sets(target_audience,target_sha256,trust_generation,role);

-- Preserve all pre-existing references; a violation aborts before commit.
CREATE TEMP TABLE phase657_signature_fk_guard (ok INTEGER NOT NULL CHECK(ok = 0));
INSERT INTO phase657_signature_fk_guard SELECT 1 FROM pragma_foreign_key_check LIMIT 1;
DROP TABLE phase657_signature_fk_guard;
COMMIT;
-- The mandatory wrapper restores the *incoming* foreign_keys state after
-- success or rollback, rather than assuming it was enabled on entry.

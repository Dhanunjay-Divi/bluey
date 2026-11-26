-- Target: SQLite
-- One-shot original-source verifier startup authority. No startup or runtime
-- readiness row is seeded by this migration.

-- Migration 058 widened this immutable table through v2. Rebuild it once more
-- so v3 signatures are representable without changing any historical row.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
DROP TABLE IF EXISTS jobs_managed_cloud_signature_sets_phase628_replacement;
CREATE TABLE jobs_managed_cloud_signature_sets_phase628_replacement (
  signature_set_sha256             TEXT PRIMARY KEY
    CHECK(length(signature_set_sha256) = 64
      AND lower(signature_set_sha256) = signature_set_sha256
      AND signature_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  signature_set_id                 TEXT NOT NULL UNIQUE CHECK(length(signature_set_id) > 0),
  trust_generation                 INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role                             TEXT NOT NULL CHECK(role IN (
    'general_promotion', 'incident', 'promotion', 'release', 'root'
  )),
  target_audience                  TEXT NOT NULL CHECK(target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1',
    'bluey-jobs-managed-cloud-activation-v2',
    'bluey-jobs-managed-cloud-activation-v3',
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
    'bluey-jobs-managed-cloud-release-v2',
    'bluey-jobs-managed-cloud-release-v3',
    'bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1',
    'bluey-jobs-managed-cloud-trust-policy-v1'
  )),
  target_sha256                    TEXT NOT NULL
    CHECK(length(target_sha256) = 64 AND lower(target_sha256) = target_sha256
      AND target_sha256 NOT GLOB '*[^0-9a-f]*'),
  signed_at_ms                     INTEGER NOT NULL CHECK(signed_at_ms >= 0),
  signature_count                  INTEGER NOT NULL CHECK(signature_count BETWEEN 1 AND 32),
  canonical_signature_set_base64url TEXT NOT NULL
    CHECK(length(canonical_signature_set_base64url) > 0),
  recorded_by                      TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                   INTEGER NOT NULL CHECK(recorded_at_ms >= signed_at_ms),
  UNIQUE(target_audience, target_sha256, trust_generation, role)
);
INSERT INTO jobs_managed_cloud_signature_sets_phase628_replacement (
  signature_set_sha256, signature_set_id, trust_generation, role,
  target_audience, target_sha256, signed_at_ms, signature_count,
  canonical_signature_set_base64url, recorded_by, recorded_at_ms
)
SELECT
  signature_set_sha256, signature_set_id, trust_generation, role,
  target_audience, target_sha256, signed_at_ms, signature_count,
  canonical_signature_set_base64url, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_signature_sets;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_signature_sets_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_signature_sets_no_delete;
DROP TABLE jobs_managed_cloud_signature_sets;
ALTER TABLE jobs_managed_cloud_signature_sets_phase628_replacement
  RENAME TO jobs_managed_cloud_signature_sets;
CREATE INDEX idx_jobs_managed_cloud_signature_sets_target
  ON jobs_managed_cloud_signature_sets(
    target_audience, target_sha256, trust_generation, role
  );
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_update
BEFORE UPDATE ON jobs_managed_cloud_signature_sets BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature set is immutable');
END;
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_delete
BEFORE DELETE ON jobs_managed_cloud_signature_sets BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature set is immutable');
END;

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_source_verifier_startup_protocols (
  manifest_sha256                 TEXT PRIMARY KEY,
  protocol_id                     TEXT NOT NULL DEFAULT 'source_verifier_startup'
    CHECK(protocol_id = 'source_verifier_startup'),
  protocol_version                INTEGER NOT NULL CHECK(protocol_version = 1),
  schema_sha256                   TEXT NOT NULL
    CHECK(length(schema_sha256) = 64 AND lower(schema_sha256) = schema_sha256
      AND schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  ordinal                         INTEGER NOT NULL CHECK(ordinal = 10),
  UNIQUE(manifest_sha256, protocol_id),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_startup_authorities (
  startup_request_id              TEXT PRIMARY KEY CHECK(
    length(startup_request_id) BETWEEN 20 AND 128
    AND startup_request_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  worker_id                       TEXT NOT NULL,
  runtime_identity_sha256         TEXT NOT NULL
    CHECK(length(runtime_identity_sha256) = 64
      AND lower(runtime_identity_sha256) = runtime_identity_sha256
      AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  request_sha256                  TEXT NOT NULL
    CHECK(length(request_sha256) = 64 AND lower(request_sha256) = request_sha256
      AND request_sha256 NOT GLOB '*[^0-9a-f]*'),
  degraded_heartbeat_sequence     INTEGER NOT NULL
    CHECK(degraded_heartbeat_sequence BETWEEN 1 AND 9007199254740990),
  ready_heartbeat_sequence        INTEGER NOT NULL
    CHECK(ready_heartbeat_sequence = degraded_heartbeat_sequence + 1),
  observed_head_revision          INTEGER NOT NULL
    CHECK(observed_head_revision BETWEEN 1 AND 9007199254740991),
  observed_transition_sha256      TEXT NOT NULL
    CHECK(length(observed_transition_sha256) = 64
      AND lower(observed_transition_sha256) = observed_transition_sha256
      AND observed_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
  artifact_sha256                 TEXT NOT NULL,
  migration_set_sha256            TEXT NOT NULL,
  config_schema_sha256            TEXT NOT NULL,
  protocol_set_sha256             TEXT NOT NULL,
  task_queue_sha256               TEXT NOT NULL,
  failure_converter_sha256        TEXT NOT NULL,
  dependency_evidence_sha256      TEXT NOT NULL,
  probe_result                    TEXT NOT NULL CHECK(probe_result IN ('idle', 'leaseable')),
  startup_authority_sha256        TEXT NOT NULL UNIQUE
    CHECK(length(startup_authority_sha256) = 64
      AND lower(startup_authority_sha256) = startup_authority_sha256
      AND startup_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_response_base64url    TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256                 TEXT NOT NULL UNIQUE
    CHECK(length(response_sha256) = 64 AND lower(response_sha256) = response_sha256
      AND response_sha256 NOT GLOB '*[^0-9a-f]*'),
  startup_at_ms                   INTEGER NOT NULL
    CHECK(startup_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(runtime_instance_id, instance_epoch),
  UNIQUE(startup_request_id, request_sha256),
  FOREIGN KEY(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, observed_head_revision,
    observed_transition_sha256, artifact_sha256, config_schema_sha256,
    migration_set_sha256, protocol_set_sha256, task_queue_sha256,
    failure_converter_sha256, dependency_evidence_sha256
  ) REFERENCES jobs_managed_cloud_original_source_verifier_runtime_instances(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256,
    artifact_sha256, config_schema_sha256, migration_set_sha256,
    protocol_set_sha256, task_queue_sha256, failure_converter_sha256,
    dependency_evidence_sha256
  ) ON DELETE RESTRICT
);

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_startup_protocol_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_source_verifier_startup_protocols BEGIN
  SELECT RAISE(ABORT, 'managed source-verifier startup protocol is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_startup_protocol_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_source_verifier_startup_protocols BEGIN
  SELECT RAISE(ABORT, 'managed source-verifier startup protocol is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_osv_startup_authority_validate_insert
BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_startup_authorities
WHEN NOT EXISTS (
  SELECT 1
    FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats heartbeat
   WHERE heartbeat.runtime_instance_id = NEW.runtime_instance_id
     AND heartbeat.instance_epoch = NEW.instance_epoch
     AND heartbeat.heartbeat_sequence = NEW.degraded_heartbeat_sequence
     AND heartbeat.worker_id = NEW.worker_id
     AND heartbeat.activation_sha256 = NEW.activation_sha256
     AND heartbeat.manifest_sha256 = NEW.manifest_sha256
     AND heartbeat.component_id = NEW.component_id
     AND heartbeat.role = NEW.role
     AND heartbeat.artifact_sha256 = NEW.artifact_sha256
     AND heartbeat.observed_head_revision = NEW.observed_head_revision
     AND heartbeat.observed_transition_sha256 = NEW.observed_transition_sha256
     AND heartbeat.migration_set_sha256 = NEW.migration_set_sha256
     AND heartbeat.config_schema_sha256 = NEW.config_schema_sha256
     AND heartbeat.protocol_set_sha256 = NEW.protocol_set_sha256
     AND heartbeat.task_queue_sha256 = NEW.task_queue_sha256
     AND heartbeat.failure_converter_sha256 = NEW.failure_converter_sha256
     AND heartbeat.dependency_evidence_sha256 = NEW.dependency_evidence_sha256
     AND heartbeat.health_state = 'degraded'
     AND heartbeat.reason_code = 'startup'
) BEGIN
  SELECT RAISE(ABORT, 'source-verifier startup heartbeat binding is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_osv_startup_authority_no_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_startup_authorities BEGIN
  SELECT RAISE(ABORT, 'source-verifier startup authority is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_osv_startup_authority_no_delete
BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_startup_authorities BEGIN
  SELECT RAISE(ABORT, 'source-verifier startup authority is immutable');
END;

DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence
BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN EXISTS (
  SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols protocol
   WHERE protocol.manifest_sha256 = NEW.manifest_sha256
     AND protocol.protocol_id = 'source_verifier_startup'
     AND protocol.protocol_version = 1 AND protocol.ordinal = 10
) AND (
  NEW.heartbeat_sequence <> 1
  OR NEW.health_state <> 'degraded'
  OR NEW.reason_code IS NOT 'startup'
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime first heartbeat must be degraded startup');
END;

DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_fenced_update;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_fenced_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.runtime_instance_id <> OLD.runtime_instance_id
  OR NEW.instance_epoch <> OLD.instance_epoch
  OR NEW.heartbeat_sequence <> OLD.heartbeat_sequence + 1
  OR NEW.heartbeat_at_ms < OLD.heartbeat_at_ms
  OR (EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols protocol
     WHERE protocol.manifest_sha256 = NEW.manifest_sha256
       AND protocol.protocol_id = 'source_verifier_startup'
       AND protocol.protocol_version = 1 AND protocol.ordinal = 10
  ) AND (
    (NEW.health_state = 'ready' AND OLD.health_state IN ('degraded', 'draining')
    AND NOT EXISTS (
      SELECT 1
        FROM jobs_managed_cloud_original_source_verifier_startup_authorities startup
       WHERE startup.runtime_instance_id = NEW.runtime_instance_id
         AND startup.instance_epoch = NEW.instance_epoch
         AND startup.degraded_heartbeat_sequence = OLD.heartbeat_sequence
         AND startup.ready_heartbeat_sequence = NEW.heartbeat_sequence
         AND startup.worker_id = NEW.worker_id
         AND startup.activation_sha256 = NEW.activation_sha256
         AND startup.manifest_sha256 = NEW.manifest_sha256
         AND startup.component_id = NEW.component_id
         AND startup.role = NEW.role
         AND startup.observed_head_revision = NEW.observed_head_revision
         AND startup.observed_transition_sha256 = NEW.observed_transition_sha256
         AND startup.artifact_sha256 = NEW.artifact_sha256
         AND startup.migration_set_sha256 = NEW.migration_set_sha256
         AND startup.config_schema_sha256 = NEW.config_schema_sha256
         AND startup.protocol_set_sha256 = NEW.protocol_set_sha256
         AND startup.task_queue_sha256 = NEW.task_queue_sha256
         AND startup.failure_converter_sha256 = NEW.failure_converter_sha256
         AND startup.dependency_evidence_sha256 = NEW.dependency_evidence_sha256
    ))
    OR (NEW.health_state = 'ready' AND OLD.health_state = 'ready'
    AND NOT EXISTS (
      SELECT 1
        FROM jobs_managed_cloud_original_source_verifier_startup_authorities startup
       WHERE startup.runtime_instance_id = NEW.runtime_instance_id
         AND startup.instance_epoch = NEW.instance_epoch
         AND startup.worker_id = NEW.worker_id
         AND startup.activation_sha256 = NEW.activation_sha256
         AND startup.manifest_sha256 = NEW.manifest_sha256
         AND startup.component_id = NEW.component_id
         AND startup.role = NEW.role
         AND startup.observed_head_revision = NEW.observed_head_revision
         AND startup.observed_transition_sha256 = NEW.observed_transition_sha256
         AND startup.artifact_sha256 = NEW.artifact_sha256
         AND startup.migration_set_sha256 = NEW.migration_set_sha256
         AND startup.config_schema_sha256 = NEW.config_schema_sha256
         AND startup.protocol_set_sha256 = NEW.protocol_set_sha256
         AND startup.task_queue_sha256 = NEW.task_queue_sha256
         AND startup.failure_converter_sha256 = NEW.failure_converter_sha256
         AND startup.dependency_evidence_sha256 = NEW.dependency_evidence_sha256
    ))
  )) BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat readiness fence is invalid');
END;

INSERT OR IGNORE INTO bluey_data_migrations(name)
VALUES ('jobs-original-source-verifier-startup-authority-v1');
COMMIT;
PRAGMA foreign_keys = ON;

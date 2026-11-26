-- Target: SQLite. Prospective boot-scoped heartbeat fence; 064 remains immutable.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
-- Release-v3 has nine runtime roles plus the static portal (ten capabilities).
-- Preserve every historical manifest byte/field while admitting that exact new shape.
DROP TABLE IF EXISTS jobs_managed_cloud_manifests_phase628_boot;
CREATE TABLE jobs_managed_cloud_manifests_phase628_boot (
  manifest_sha256                 TEXT PRIMARY KEY
    CHECK(length(manifest_sha256) = 64 AND lower(manifest_sha256) = manifest_sha256
      AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_id                     TEXT NOT NULL UNIQUE CHECK(length(manifest_id) > 0),
  manifest_generation             INTEGER NOT NULL UNIQUE
    CHECK(manifest_generation BETWEEN 1 AND 9007199254740991),
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  release_id                      TEXT NOT NULL UNIQUE CHECK(length(release_id) > 0),
  release_sequence                INTEGER NOT NULL UNIQUE
    CHECK(release_sequence BETWEEN 1 AND 9007199254740991),
  source_commit                   TEXT NOT NULL
    CHECK(length(source_commit) = 40 AND lower(source_commit) = source_commit
      AND source_commit NOT GLOB '*[^0-9a-f]*'),
  sqlite_migration_head           TEXT NOT NULL CHECK(length(sqlite_migration_head) > 0),
  postgres_migration_head         TEXT NOT NULL CHECK(length(postgres_migration_head) > 0),
  migration_set_sha256            TEXT NOT NULL
    CHECK(length(migration_set_sha256) = 64 AND lower(migration_set_sha256) = migration_set_sha256
      AND migration_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  config_schema_sha256            TEXT NOT NULL
    CHECK(length(config_schema_sha256) = 64 AND lower(config_schema_sha256) = config_schema_sha256
      AND config_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_set_sha256             TEXT NOT NULL
    CHECK(length(protocol_set_sha256) = 64 AND lower(protocol_set_sha256) = protocol_set_sha256
      AND protocol_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  component_set_sha256            TEXT NOT NULL
    CHECK(length(component_set_sha256) = 64 AND lower(component_set_sha256) = component_set_sha256
      AND component_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  feature_authority_sha256        TEXT NOT NULL
    CHECK(length(feature_authority_sha256) = 64
      AND lower(feature_authority_sha256) = feature_authority_sha256
      AND feature_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  cloud_distribution_enabled     INTEGER NOT NULL
    CHECK(cloud_distribution_enabled IN (0, 1)),
  workflow_command_dispatch_enabled INTEGER NOT NULL
    CHECK(workflow_command_dispatch_enabled IN (0, 1)),
  workflow_cleanup_enabled       INTEGER NOT NULL
    CHECK(workflow_cleanup_enabled IN (0, 1)),
  direct_discovery_enabled       INTEGER NOT NULL
    CHECK(direct_discovery_enabled IN (0, 1)),
  global_discovery_enabled       INTEGER NOT NULL
    CHECK(global_discovery_enabled IN (0, 1)),
  source_verification_enabled    INTEGER NOT NULL
    CHECK(source_verification_enabled IN (0, 1)),
  verification_evidence_sha256    TEXT NOT NULL
    CHECK(length(verification_evidence_sha256) = 64
      AND lower(verification_evidence_sha256) = verification_evidence_sha256
      AND verification_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  failure_converter_sha256        TEXT NOT NULL
    CHECK(length(failure_converter_sha256) = 64
      AND lower(failure_converter_sha256) = failure_converter_sha256
      AND failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  component_count                 INTEGER NOT NULL CHECK(component_count = 4),
  capability_count                INTEGER NOT NULL CHECK(capability_count BETWEEN 7 AND 9 OR
      (capability_count = 10 AND direct_discovery_enabled = 1
       AND global_discovery_enabled = 1 AND source_verification_enabled = 1)),
  protocol_count                  INTEGER NOT NULL CHECK(protocol_count = 11),
  canonical_manifest_base64url    TEXT NOT NULL CHECK(length(canonical_manifest_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  published_at_ms                 INTEGER NOT NULL CHECK(published_at_ms >= 0),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= published_at_ms),
  UNIQUE(manifest_sha256, authorization_signature_set_sha256),
  UNIQUE(
    manifest_sha256, authorization_signature_set_sha256, trust_generation,
    feature_authority_sha256
  ),
  UNIQUE(
    manifest_sha256, migration_set_sha256,
    config_schema_sha256, protocol_set_sha256
  ),
  UNIQUE(manifest_sha256, release_id, release_sequence),
  UNIQUE(manifest_sha256, failure_converter_sha256),
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_manifests_phase628_boot (
  manifest_sha256, manifest_id, manifest_generation,
  trust_generation, release_id, release_sequence,
  source_commit, sqlite_migration_head, postgres_migration_head,
  migration_set_sha256, config_schema_sha256, protocol_set_sha256,
  component_set_sha256, feature_authority_sha256, cloud_distribution_enabled,
  workflow_command_dispatch_enabled, workflow_cleanup_enabled, direct_discovery_enabled,
  global_discovery_enabled, source_verification_enabled, verification_evidence_sha256,
  failure_converter_sha256, component_count, capability_count,
  protocol_count, canonical_manifest_base64url, authorization_signature_set_sha256,
  published_at_ms, recorded_by, recorded_at_ms
) SELECT
  manifest_sha256, manifest_id, manifest_generation,
  trust_generation, release_id, release_sequence,
  source_commit, sqlite_migration_head, postgres_migration_head,
  migration_set_sha256, config_schema_sha256, protocol_set_sha256,
  component_set_sha256, feature_authority_sha256, cloud_distribution_enabled,
  workflow_command_dispatch_enabled, workflow_cleanup_enabled, direct_discovery_enabled,
  global_discovery_enabled, source_verification_enabled, verification_evidence_sha256,
  failure_converter_sha256, component_count, capability_count,
  protocol_count, canonical_manifest_base64url, authorization_signature_set_sha256,
  published_at_ms, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_manifests;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_manifests_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_manifests_no_delete;
DROP TABLE jobs_managed_cloud_manifests;
ALTER TABLE jobs_managed_cloud_manifests_phase628_boot RENAME TO jobs_managed_cloud_manifests;
CREATE TRIGGER trg_jobs_managed_cloud_manifests_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifests BEGIN
  SELECT RAISE(ABORT, 'managed cloud manifest is immutable');
END;
CREATE TRIGGER trg_jobs_managed_cloud_manifests_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifests BEGIN
  SELECT RAISE(ABORT, 'managed cloud manifest is immutable');
END;
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_heartbeat_boot_bindings (
  runtime_instance_id TEXT NOT NULL,
  instance_epoch INTEGER NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence INTEGER NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  boot_id TEXT NOT NULL,
  boot_generation INTEGER NOT NULL CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  PRIMARY KEY(runtime_instance_id,instance_epoch,heartbeat_sequence),
  FOREIGN KEY(boot_id) REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS jobs_original_source_verification_attempt_boot_bindings (
  attempt_id TEXT PRIMARY KEY,
  runtime_instance_id TEXT NOT NULL,
  runtime_instance_epoch INTEGER NOT NULL
    CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  boot_id TEXT NOT NULL,
  boot_generation INTEGER NOT NULL
    CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  FOREIGN KEY(attempt_id) REFERENCES jobs_original_source_verification_attempts(attempt_id)
    ON DELETE CASCADE,
  FOREIGN KEY(boot_id) REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id)
    ON DELETE RESTRICT
);
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_attempt_boot_binding_immutable
BEFORE UPDATE ON jobs_original_source_verification_attempt_boot_bindings
BEGIN SELECT RAISE(ABORT, 'source-verifier attempt boot binding is immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_attempt_boot_binding_delete
BEFORE DELETE ON jobs_original_source_verification_attempt_boot_bindings
WHEN EXISTS (SELECT 1 FROM jobs_original_source_verification_attempts attempt
             WHERE attempt.attempt_id=OLD.attempt_id)
BEGIN SELECT RAISE(ABORT, 'source-verifier attempt boot binding is immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_attempt_boot_binding_current
BEFORE INSERT ON jobs_original_source_verification_attempt_boot_bindings
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_original_source_verification_attempts attempt
  JOIN jobs_managed_cloud_source_verifier_boot_registrations registration
    ON registration.boot_id=NEW.boot_id
  JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head
    ON head.runtime_instance_id=registration.runtime_instance_id
   AND head.instance_epoch=registration.instance_epoch
  WHERE attempt.attempt_id=NEW.attempt_id
    AND attempt.runtime_instance_id=NEW.runtime_instance_id
    AND attempt.runtime_instance_epoch=NEW.runtime_instance_epoch
    AND attempt.worker_id=registration.worker_id
    AND registration.runtime_instance_id=NEW.runtime_instance_id
    AND registration.instance_epoch=NEW.runtime_instance_epoch
    AND registration.boot_generation=NEW.boot_generation
    AND head.current_boot_generation=NEW.boot_generation
    AND head.ready_boot_generation=NEW.boot_generation
)
BEGIN SELECT RAISE(ABORT, 'source-verifier attempt boot binding is stale'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_binding_immutable BEFORE UPDATE ON jobs_managed_cloud_source_verifier_heartbeat_boot_bindings BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat boot binding is immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_binding_permanent BEFORE DELETE ON jobs_managed_cloud_source_verifier_heartbeat_boot_bindings BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat boot binding is permanent'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_binding_current BEFORE INSERT ON jobs_managed_cloud_source_verifier_heartbeat_boot_bindings
WHEN NOT EXISTS (SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch WHERE r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND h.current_boot_generation=NEW.boot_generation)
BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat boot binding is stale'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_startup_requires_degraded
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
WHEN NOT EXISTS (
  SELECT 1
    FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats heartbeat
    JOIN jobs_managed_cloud_source_verifier_heartbeat_boot_bindings binding
      ON binding.runtime_instance_id=heartbeat.runtime_instance_id
     AND binding.instance_epoch=heartbeat.instance_epoch
     AND binding.heartbeat_sequence=heartbeat.heartbeat_sequence
    JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head
      ON head.runtime_instance_id=binding.runtime_instance_id AND head.instance_epoch=binding.instance_epoch
   WHERE heartbeat.runtime_instance_id=NEW.runtime_instance_id
     AND heartbeat.instance_epoch=NEW.instance_epoch
     AND heartbeat.heartbeat_sequence=NEW.degraded_heartbeat_sequence
     AND heartbeat.health_state='degraded' AND heartbeat.reason_code='startup'
     AND binding.boot_id=NEW.boot_id AND binding.boot_generation=NEW.boot_generation
     AND head.current_boot_generation=NEW.boot_generation
)
BEGIN SELECT RAISE(ABORT, 'source-verifier boot startup requires current degraded heartbeat'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_heartbeat_insert BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN EXISTS (SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_manifest_source_verifier_boot_protocols p ON p.manifest_sha256=i.manifest_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id)
 AND NOT EXISTS (SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND h.current_boot_generation=b.boot_generation)
BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat requires current boot binding'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_heartbeat_update BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN EXISTS (SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_manifest_source_verifier_boot_protocols p ON p.manifest_sha256=i.manifest_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id)
 AND NOT EXISTS (SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND h.current_boot_generation=b.boot_generation)
BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat requires current boot binding'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_heartbeat_transition BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence)
 AND (((NEW.health_state='ready' AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_startup_authorities s ON s.boot_id=b.boot_id AND s.boot_generation=b.boot_generation JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND s.ready_heartbeat_sequence<=NEW.heartbeat_sequence AND h.ready_boot_generation=b.boot_generation)) OR (NEW.health_state='degraded' AND NEW.reason_code IN ('startup','probe_failed') AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND (h.ready_boot_generation IS NULL OR h.current_boot_generation>h.ready_boot_generation)))) IS NOT TRUE)
BEGIN SELECT RAISE(ABORT,'source-verifier boot heartbeat transition is unauthorized'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_osv_boot_heartbeat_transition_update BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats WHEN EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence) AND (((NEW.health_state='ready' AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_startup_authorities s ON s.boot_id=b.boot_id AND s.boot_generation=b.boot_generation JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND s.ready_heartbeat_sequence<=NEW.heartbeat_sequence AND h.ready_boot_generation=b.boot_generation)) OR (NEW.health_state='degraded' AND NEW.reason_code IN ('startup','probe_failed') AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND (h.ready_boot_generation IS NULL OR h.current_boot_generation>h.ready_boot_generation)))) IS NOT TRUE) BEGIN SELECT RAISE(ABORT,'source-verifier boot heartbeat transition is unauthorized'); END;
-- Keep historical sequence/time and v1 startup checks unchanged. Boot manifests
-- use the independent generation-bound authority above instead of the v1 row.
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
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot
     WHERE boot.manifest_sha256 = NEW.manifest_sha256
       AND boot.protocol_id = 'source_verifier_boot'
       AND boot.protocol_version = 1 AND boot.ordinal = 11
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

INSERT OR IGNORE INTO bluey_data_migrations(name) VALUES ('jobs-source-verifier-heartbeat-boot-bindings-v1');
COMMIT;
PRAGMA foreign_keys = ON;

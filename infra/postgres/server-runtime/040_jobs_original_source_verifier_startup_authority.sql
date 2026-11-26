-- Target: PostgreSQL
-- One-shot original-source verifier startup authority. No startup or runtime
-- readiness row is seeded by this migration.

ALTER TABLE jobs_managed_cloud_signature_sets
  DROP CONSTRAINT IF EXISTS jobs_managed_cloud_signature_sets_target_audience_check;
ALTER TABLE jobs_managed_cloud_signature_sets
  ADD CONSTRAINT jobs_managed_cloud_signature_sets_target_audience_check
  CHECK(target_audience IN (
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
  ));

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_source_verifier_startup_protocols (
  manifest_sha256 TEXT PRIMARY KEY
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT,
  protocol_id TEXT NOT NULL DEFAULT 'source_verifier_startup'
    CHECK(protocol_id = 'source_verifier_startup'),
  protocol_version BIGINT NOT NULL CHECK(protocol_version = 1),
  schema_sha256 TEXT NOT NULL CHECK(schema_sha256 ~ '^[0-9a-f]{64}$'),
  ordinal BIGINT NOT NULL CHECK(ordinal = 10),
  UNIQUE(manifest_sha256, protocol_id),
  UNIQUE(manifest_sha256, ordinal)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_startup_authorities (
  startup_request_id TEXT PRIMARY KEY
    CHECK(startup_request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  runtime_instance_id TEXT NOT NULL,
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  worker_id TEXT NOT NULL,
  runtime_identity_sha256 TEXT NOT NULL CHECK(runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  request_sha256 TEXT NOT NULL CHECK(request_sha256 ~ '^[0-9a-f]{64}$'),
  degraded_heartbeat_sequence BIGINT NOT NULL
    CHECK(degraded_heartbeat_sequence BETWEEN 1 AND 9007199254740990),
  ready_heartbeat_sequence BIGINT NOT NULL
    CHECK(ready_heartbeat_sequence = degraded_heartbeat_sequence + 1),
  observed_head_revision BIGINT NOT NULL
    CHECK(observed_head_revision BETWEEN 1 AND 9007199254740991),
  observed_transition_sha256 TEXT NOT NULL
    CHECK(observed_transition_sha256 ~ '^[0-9a-f]{64}$'),
  activation_sha256 TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL,
  component_id TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role TEXT NOT NULL CHECK(role = 'original_source_verifier'),
  artifact_sha256 TEXT NOT NULL CHECK(artifact_sha256 ~ '^[0-9a-f]{64}$'),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL
    CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  dependency_evidence_sha256 TEXT NOT NULL
    CHECK(dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  probe_result TEXT NOT NULL CHECK(probe_result IN ('idle', 'leaseable')),
  startup_authority_sha256 TEXT NOT NULL UNIQUE
    CHECK(startup_authority_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_response_base64url TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256 TEXT NOT NULL UNIQUE CHECK(response_sha256 ~ '^[0-9a-f]{64}$'),
  startup_at_ms BIGINT NOT NULL CHECK(startup_at_ms BETWEEN 0 AND 9007199254740991),
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

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_startup_protocol_immutable
  ON jobs_managed_cloud_manifest_source_verifier_startup_protocols;
CREATE TRIGGER trg_jobs_managed_source_verifier_startup_protocol_immutable
BEFORE UPDATE OR DELETE ON jobs_managed_cloud_manifest_source_verifier_startup_protocols
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_osv_startup_authority_insert()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF NOT EXISTS (
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
  ) THEN
    RAISE EXCEPTION 'source-verifier startup heartbeat binding is invalid';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_osv_startup_authority_validate_insert
  ON jobs_managed_cloud_original_source_verifier_startup_authorities;
CREATE TRIGGER trg_jobs_managed_osv_startup_authority_validate_insert
BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_startup_authorities
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_guard_osv_startup_authority_insert();
DROP TRIGGER IF EXISTS trg_jobs_managed_osv_startup_authority_immutable
  ON jobs_managed_cloud_original_source_verifier_startup_authorities;
CREATE TRIGGER trg_jobs_managed_osv_startup_authority_immutable
BEFORE UPDATE OR DELETE ON jobs_managed_cloud_original_source_verifier_startup_authorities
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_osv_heartbeat_sequence()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF TG_OP = 'INSERT' AND EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols protocol
     WHERE protocol.manifest_sha256 = NEW.manifest_sha256
       AND protocol.protocol_id = 'source_verifier_startup'
       AND protocol.protocol_version = 1 AND protocol.ordinal = 10
  ) AND (
    NEW.heartbeat_sequence <> 1
    OR NEW.health_state <> 'degraded'
    OR NEW.reason_code IS DISTINCT FROM 'startup'
  ) THEN
    RAISE EXCEPTION 'managed cloud runtime first heartbeat must be degraded startup';
  END IF;
  IF TG_OP = 'UPDATE' AND (
    NEW.runtime_instance_id IS DISTINCT FROM OLD.runtime_instance_id
    OR NEW.instance_epoch IS DISTINCT FROM OLD.instance_epoch
    OR NEW.heartbeat_sequence <> OLD.heartbeat_sequence + 1
    OR NEW.heartbeat_at_ms < OLD.heartbeat_at_ms
  ) THEN
    RAISE EXCEPTION 'managed cloud runtime heartbeat fence is invalid';
  END IF;
  IF TG_OP = 'UPDATE' AND EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols protocol
     WHERE protocol.manifest_sha256 = NEW.manifest_sha256
       AND protocol.protocol_id = 'source_verifier_startup'
       AND protocol.protocol_version = 1 AND protocol.ordinal = 10
  ) AND NEW.health_state = 'ready'
    AND OLD.health_state IN ('degraded', 'draining')
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
    ) THEN
    RAISE EXCEPTION 'managed cloud runtime startup readiness fence is invalid';
  END IF;
  IF TG_OP = 'UPDATE' AND EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols protocol
     WHERE protocol.manifest_sha256 = NEW.manifest_sha256
       AND protocol.protocol_id = 'source_verifier_startup'
       AND protocol.protocol_version = 1 AND protocol.ordinal = 10
  ) AND NEW.health_state = 'ready' AND OLD.health_state = 'ready'
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
    ) THEN
    RAISE EXCEPTION 'managed cloud runtime ready heartbeat lacks startup authority';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence
  ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence
BEFORE INSERT OR UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_guard_osv_heartbeat_sequence();

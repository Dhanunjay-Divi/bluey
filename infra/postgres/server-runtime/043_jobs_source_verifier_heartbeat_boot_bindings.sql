-- Target: PostgreSQL. Paired prospective successor to SQLite 065.
BEGIN;
-- Release-v3 adds both discovery workers and the source verifier. Existing
-- rows retain their historical bytes; only the ten-capability shape is new.
ALTER TABLE jobs_managed_cloud_manifests
  DROP CONSTRAINT IF EXISTS jobs_managed_cloud_manifests_capability_count_check;
ALTER TABLE jobs_managed_cloud_manifests
  ADD CONSTRAINT jobs_managed_cloud_manifests_capability_count_check CHECK (
    capability_count BETWEEN 7 AND 9 OR
    (capability_count = 10 AND direct_discovery_enabled
     AND global_discovery_enabled AND source_verification_enabled)
  );
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_heartbeat_boot_bindings (
 runtime_instance_id TEXT NOT NULL,
 instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
 heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
 boot_id TEXT NOT NULL,
 boot_generation BIGINT NOT NULL CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
 PRIMARY KEY(runtime_instance_id,instance_epoch,heartbeat_sequence),
 FOREIGN KEY(boot_id) REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id) ON DELETE RESTRICT);
CREATE TABLE IF NOT EXISTS jobs_original_source_verification_attempt_boot_bindings (
 attempt_id TEXT PRIMARY KEY,
 runtime_instance_id TEXT NOT NULL,
 runtime_instance_epoch BIGINT NOT NULL CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
 boot_id TEXT NOT NULL,
 boot_generation BIGINT NOT NULL CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
 FOREIGN KEY(attempt_id) REFERENCES jobs_original_source_verification_attempts(attempt_id) ON DELETE CASCADE,
 FOREIGN KEY(boot_id) REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id) ON DELETE RESTRICT);
CREATE OR REPLACE FUNCTION jobs_osv_attempt_boot_binding_guard() RETURNS TRIGGER AS $$ BEGIN
 IF TG_OP = 'DELETE' THEN
   IF EXISTS(SELECT 1 FROM jobs_original_source_verification_attempts attempt WHERE attempt.attempt_id=OLD.attempt_id) THEN RAISE EXCEPTION 'source-verifier attempt boot binding is immutable'; END IF;
   RETURN OLD;
 END IF;
 IF TG_OP <> 'INSERT' THEN RAISE EXCEPTION 'source-verifier attempt boot binding is immutable'; END IF;
 IF NOT EXISTS(SELECT 1 FROM jobs_original_source_verification_attempts attempt JOIN jobs_managed_cloud_source_verifier_boot_registrations registration ON registration.boot_id=NEW.boot_id JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head ON head.runtime_instance_id=registration.runtime_instance_id AND head.instance_epoch=registration.instance_epoch WHERE attempt.attempt_id=NEW.attempt_id AND attempt.runtime_instance_id=NEW.runtime_instance_id AND attempt.runtime_instance_epoch=NEW.runtime_instance_epoch AND attempt.worker_id=registration.worker_id AND registration.runtime_instance_id=NEW.runtime_instance_id AND registration.instance_epoch=NEW.runtime_instance_epoch AND registration.boot_generation=NEW.boot_generation AND head.current_boot_generation=NEW.boot_generation AND head.ready_boot_generation=NEW.boot_generation) THEN RAISE EXCEPTION 'source-verifier attempt boot binding is stale'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_osv_attempt_boot_binding_guard ON jobs_original_source_verification_attempt_boot_bindings;
CREATE TRIGGER trg_jobs_osv_attempt_boot_binding_guard BEFORE INSERT OR UPDATE OR DELETE ON jobs_original_source_verification_attempt_boot_bindings FOR EACH ROW EXECUTE FUNCTION jobs_osv_attempt_boot_binding_guard();
CREATE OR REPLACE FUNCTION jobs_osv_boot_startup_requires_degraded() RETURNS TRIGGER AS $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats heartbeat JOIN jobs_managed_cloud_source_verifier_heartbeat_boot_bindings binding ON binding.runtime_instance_id=heartbeat.runtime_instance_id AND binding.instance_epoch=heartbeat.instance_epoch AND binding.heartbeat_sequence=heartbeat.heartbeat_sequence JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head ON head.runtime_instance_id=binding.runtime_instance_id AND head.instance_epoch=binding.instance_epoch WHERE heartbeat.runtime_instance_id=NEW.runtime_instance_id AND heartbeat.instance_epoch=NEW.instance_epoch AND heartbeat.heartbeat_sequence=NEW.degraded_heartbeat_sequence AND heartbeat.health_state='degraded' AND heartbeat.reason_code='startup' AND binding.boot_id=NEW.boot_id AND binding.boot_generation=NEW.boot_generation AND head.current_boot_generation=NEW.boot_generation) THEN RAISE EXCEPTION 'source-verifier boot startup requires current degraded heartbeat'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_startup_requires_degraded ON jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_osv_boot_startup_requires_degraded BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities FOR EACH ROW EXECUTE FUNCTION jobs_osv_boot_startup_requires_degraded();
CREATE OR REPLACE FUNCTION jobs_osv_boot_binding_guard() RETURNS TRIGGER AS $$ BEGIN
 IF TG_OP <> 'INSERT' THEN RAISE EXCEPTION 'source-verifier heartbeat boot binding is immutable'; END IF;
 IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch WHERE r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND h.current_boot_generation=NEW.boot_generation) THEN RAISE EXCEPTION 'source-verifier heartbeat boot binding is stale'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION jobs_osv_boot_heartbeat_guard() RETURNS TRIGGER AS $$ BEGIN
 IF EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_manifest_source_verifier_boot_protocols p ON p.manifest_sha256=i.manifest_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id) AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND h.current_boot_generation=b.boot_generation) THEN RAISE EXCEPTION 'source-verifier heartbeat requires current boot binding'; END IF;
 IF EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence) AND (((NEW.health_state='ready' AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_startup_authorities s ON s.boot_id=b.boot_id AND s.boot_generation=b.boot_generation JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND s.ready_heartbeat_sequence<=NEW.heartbeat_sequence AND h.ready_boot_generation=b.boot_generation)) OR (NEW.health_state='degraded' AND NEW.reason_code IN ('startup','probe_failed') AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND (h.ready_boot_generation IS NULL OR h.current_boot_generation>h.ready_boot_generation)))) IS NOT TRUE) THEN RAISE EXCEPTION 'source-verifier boot heartbeat transition is unauthorized'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_binding_insert ON jobs_managed_cloud_source_verifier_heartbeat_boot_bindings;
CREATE TRIGGER trg_jobs_osv_boot_binding_insert BEFORE INSERT OR UPDATE OR DELETE ON jobs_managed_cloud_source_verifier_heartbeat_boot_bindings FOR EACH ROW EXECUTE FUNCTION jobs_osv_boot_binding_guard();
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_osv_boot_heartbeat BEFORE INSERT OR UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats FOR EACH ROW EXECUTE FUNCTION jobs_osv_boot_heartbeat_guard();
-- Keep historical sequence/time and v1 startup checks unchanged. Boot manifests
-- use the independent generation-bound authority above instead of the v1 row.
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
  IF TG_OP = 'UPDATE' AND NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot
     WHERE boot.manifest_sha256 = NEW.manifest_sha256
       AND boot.protocol_id = 'source_verifier_boot'
       AND boot.protocol_version = 1 AND boot.ordinal = 11
  ) AND EXISTS (
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
  IF TG_OP = 'UPDATE' AND NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot
     WHERE boot.manifest_sha256 = NEW.manifest_sha256
       AND boot.protocol_id = 'source_verifier_boot'
       AND boot.protocol_version = 1 AND boot.ordinal = 11
  ) AND EXISTS (
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
INSERT INTO bluey_data_migrations(name) VALUES ('jobs-source-verifier-heartbeat-boot-bindings-v1') ON CONFLICT DO NOTHING;
COMMIT;

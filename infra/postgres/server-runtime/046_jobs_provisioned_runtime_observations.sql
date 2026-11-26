-- Target: PostgreSQL. Phase648 C3 DDL CANDIDATE; no writer/admission permission.
-- Existing two runtime families remain authoritative. Hash/typed metadata is NOT
-- proof of signatures, owned resources, real dependency work, or queue isolation.
-- Writers must acquire their agreed H/M -> scope/generation -> instance order,
-- then final DB time; latch, contamination and latest-observation writes serialize.
-- Boot-v2 is disjoint parked storage: no Ready update or leaseability probe.
-- Its real work-free producer/signature/context validation remains writer-owned.
BEGIN;

-- Header is inert until an authenticated latch writer validates the complete
-- concrete membership union, required-role minima, latest/fresh probes and facts.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_bootstrap_sets (
  bootstrap_set_sha256 TEXT NOT NULL CHECK(bootstrap_set_sha256 ~ '^[0-9a-f]{64}$') PRIMARY KEY,
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (context_sha256 ~ '^[0-9a-f]{64}$')),
  queue_generation_id TEXT NOT NULL CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  trust_generation BIGINT NOT NULL CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  channel_sequence BIGINT NOT NULL CHECK(channel_sequence BETWEEN 1 AND 9007199254740991),
  member_count BIGINT NOT NULL CHECK(member_count BETWEEN 1 AND 9007199254740991),
  canonical_bootstrap_base64url TEXT NOT NULL CHECK(octet_length(canonical_bootstrap_base64url) BETWEEN 1 AND 174763 AND canonical_bootstrap_base64url ~ '^[A-Za-z0-9_-]+$'),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256),
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE),
  FOREIGN KEY(activation_sha256,manifest_sha256,environment,region,channel) REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(transition_sha256,environment,region,channel,head_revision,activation_sha256,manifest_sha256,trust_generation,channel_sequence) REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION jobs_mc_bootstrap_header_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND a.purpose IS NOT DISTINCT FROM NEW.purpose AND a.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND a.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND a.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256) THEN RAISE EXCEPTION 'managed cloud bootstrap head binding is invalid'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_bootstrap_header_guard ON jobs_managed_cloud_bootstrap_sets;
CREATE TRIGGER trg_jobs_mc_bootstrap_header_guard BEFORE INSERT ON jobs_managed_cloud_bootstrap_sets
FOR EACH ROW EXECUTE FUNCTION jobs_mc_bootstrap_header_guard();
DROP TRIGGER IF EXISTS trg_jobs_mc_bootstrap_header_immutable ON jobs_managed_cloud_bootstrap_sets;
CREATE TRIGGER trg_jobs_mc_bootstrap_header_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_bootstrap_sets
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_serving_latches (
  latch_sha256 TEXT NOT NULL CHECK(latch_sha256 ~ '^[0-9a-f]{64}$') PRIMARY KEY,
  latch_id TEXT NOT NULL CHECK(latch_id ~ '^[A-Za-z0-9_-]{20,128}$') UNIQUE,
  bootstrap_set_sha256 TEXT NOT NULL CHECK(bootstrap_set_sha256 ~ '^[0-9a-f]{64}$') UNIQUE,
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (context_sha256 ~ '^[0-9a-f]{64}$')),
  queue_generation_id TEXT NOT NULL CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  trust_generation BIGINT NOT NULL CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  channel_sequence BIGINT NOT NULL CHECK(channel_sequence BETWEEN 1 AND 9007199254740991),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT CHECK(authorization_signature_set_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_latch_base64url TEXT NOT NULL CHECK(octet_length(canonical_latch_base64url) BETWEEN 1 AND 174763 AND canonical_latch_base64url ~ '^[A-Za-z0-9_-]+$'),
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms BETWEEN 0 AND 9007199254740991),
  not_before_ms BIGINT NOT NULL CHECK(not_before_ms BETWEEN 0 AND 9007199254740991),
  expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms BETWEEN 1 AND 9007199254740991),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms BETWEEN 0 AND 9007199254740991),
  CHECK(not_before_ms>=issued_at_ms AND expires_at_ms>not_before_ms AND recorded_at_ms>=issued_at_ms),
  UNIQUE(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256),
  UNIQUE(activation_sha256,head_revision,transition_sha256),
  FOREIGN KEY(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT,
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE),
  FOREIGN KEY(activation_sha256,manifest_sha256,environment,region,channel) REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(transition_sha256,environment,region,channel,head_revision,activation_sha256,manifest_sha256,trust_generation,channel_sequence) REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT
);
DROP TRIGGER IF EXISTS trg_jobs_mc_serving_latch_immutable ON jobs_managed_cloud_serving_latches;
CREATE TRIGGER trg_jobs_mc_serving_latch_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_serving_latches
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

-- base: retain every existing legacy row/authority/FK/index and instance immutability.
ALTER TABLE jobs_managed_cloud_runtime_instances
  ADD COLUMN IF NOT EXISTS protocol_version BIGINT NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
  ADD COLUMN IF NOT EXISTS claimed_latch_sha256 TEXT CHECK(claimed_latch_sha256 IS NULL OR (claimed_latch_sha256 ~ '^[0-9a-f]{64}$'));
DO $$ BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_runtime_instances'::regclass AND conname='ck_jobs_mc_base_claim_version') THEN
    ALTER TABLE jobs_managed_cloud_runtime_instances ADD CONSTRAINT ck_jobs_mc_base_claim_version CHECK((protocol_version=1 AND claimed_latch_sha256 IS NULL) OR protocol_version=2);
  END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_runtime_instances'::regclass AND conname='fk_jobs_mc_base_claim_latch') THEN
    ALTER TABLE jobs_managed_cloud_runtime_instances ADD CONSTRAINT fk_jobs_mc_base_claim_latch FOREIGN KEY(claimed_latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_serving_latches(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) ON DELETE RESTRICT;
  END IF;
END $$;
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_mc_base_v2_identity ON jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel);
CREATE OR REPLACE FUNCTION jobs_mc_base_claim_kind_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.claimed_latch_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND NEW.protocol_version=2))) THEN RAISE EXCEPTION 'managed cloud runtime claim protocol kind mismatch'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_base_claim_kind_guard ON jobs_managed_cloud_runtime_instances;
CREATE TRIGGER trg_jobs_mc_base_claim_kind_guard BEFORE INSERT ON jobs_managed_cloud_runtime_instances
FOR EACH ROW EXECUTE FUNCTION jobs_mc_base_claim_kind_guard();

-- osv: retain every existing legacy row/authority/FK/index and instance immutability.
ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_instances
  ADD COLUMN IF NOT EXISTS protocol_version BIGINT NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
  ADD COLUMN IF NOT EXISTS claimed_latch_sha256 TEXT CHECK(claimed_latch_sha256 IS NULL OR (claimed_latch_sha256 ~ '^[0-9a-f]{64}$'));
DO $$ BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_original_source_verifier_runtime_instances'::regclass AND conname='ck_jobs_mc_osv_claim_version') THEN
    ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_instances ADD CONSTRAINT ck_jobs_mc_osv_claim_version CHECK((protocol_version=1 AND claimed_latch_sha256 IS NULL) OR protocol_version=2);
  END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_original_source_verifier_runtime_instances'::regclass AND conname='fk_jobs_mc_osv_claim_latch') THEN
    ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_instances ADD CONSTRAINT fk_jobs_mc_osv_claim_latch FOREIGN KEY(claimed_latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_serving_latches(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) ON DELETE RESTRICT;
  END IF;
END $$;
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_mc_osv_v2_identity ON jobs_managed_cloud_original_source_verifier_runtime_instances(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel);
CREATE OR REPLACE FUNCTION jobs_mc_osv_claim_kind_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.claimed_latch_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND NEW.protocol_version=2))) THEN RAISE EXCEPTION 'managed cloud runtime claim protocol kind mismatch'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_claim_kind_guard ON jobs_managed_cloud_original_source_verifier_runtime_instances;
CREATE TRIGGER trg_jobs_mc_osv_claim_kind_guard BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_instances
FOR EACH ROW EXECUTE FUNCTION jobs_mc_osv_claim_kind_guard();
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_mc_base_contamination_identity ON jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,worker_id,activation_sha256,manifest_sha256,head_revision,transition_sha256,role);

-- Boot registration is the sole observation parent; startup is a receipt child.
-- No observation -> startup FK. V2 stores only dependency_ready success; an
-- unavailable probe has NO startup row/observation/latest mutation.
ALTER TABLE jobs_managed_cloud_source_verifier_boot_registrations ADD COLUMN IF NOT EXISTS protocol_version BIGINT NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2));
DO $$ DECLARE name TEXT; BEGIN
 FOR name IN SELECT conname FROM pg_constraint WHERE conrelid='jobs_managed_cloud_source_verifier_boot_registrations'::regclass AND contype='c'
  AND (pg_get_constraintdef(oid) LIKE '%boot_id%' OR pg_get_constraintdef(oid) LIKE '%registration_request_id%')
 LOOP EXECUTE format('ALTER TABLE jobs_managed_cloud_source_verifier_boot_registrations DROP CONSTRAINT %I',name); END LOOP;
END $$;
ALTER TABLE jobs_managed_cloud_source_verifier_boot_registrations ADD CONSTRAINT ck_jobs_mc_boot_registration_version CHECK(((protocol_version=1 AND boot_id ~ '^[A-Za-z0-9_-]{20,128}$' AND registration_request_id ~ '^[A-Za-z0-9_-]{20,128}$') OR (protocol_version=2 AND boot_id IS NOT NULL AND registration_request_id IS NOT NULL AND boot_id ~ '^[A-Za-z0-9_.:/-]{20,128}$' AND registration_request_id ~ '^[A-Za-z0-9_.:/-]{20,128}$' AND runtime_identity_sha256 ~ '^[0-9a-f]{64}$' AND activation_sha256 ~ '^[0-9a-f]{64}$' AND manifest_sha256 ~ '^[0-9a-f]{64}$' AND registration_request_sha256 ~ '^[0-9a-f]{64}$' AND registration_authority_sha256 ~ '^[0-9a-f]{64}$' AND response_sha256 ~ '^[0-9a-f]{64}$' AND octet_length(canonical_response_base64url) BETWEEN 1 AND 87382 AND canonical_response_base64url ~ '^[A-Za-z0-9_-]+$')) IS TRUE);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_mc_boot_registration_v2 ON jobs_managed_cloud_source_verifier_boot_registrations(runtime_instance_id,instance_epoch,boot_id,boot_generation,protocol_version);

-- Permanent base facts. Canonical blobs MUST come from the closed nonsecret
-- Rust serializers; SQL does not decode base64, verify the digest, or prove work.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_observations (
  runtime_instance_id TEXT NOT NULL CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(observation_sha256 ~ '^[0-9a-f]{64}$') UNIQUE,
  grant_id TEXT NOT NULL CHECK(grant_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  runtime_identity_sha256 TEXT NOT NULL CHECK(runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_version BIGINT NOT NULL DEFAULT 2 CHECK(protocol_version=2),
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (context_sha256 ~ '^[0-9a-f]{64}$')),
  queue_generation_id TEXT NOT NULL CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  component_id TEXT NOT NULL CHECK(component_id IN ('jobs-api','jobs-runner','jobs-workflows')),
  role TEXT NOT NULL CHECK(role IN ('discovery_worker','global_discovery_worker','jobs_api','managed_runner','workflow_command_dispatcher','workflow_cleanup_dispatcher','workflow_gateway','workflow_worker')),
  worker_id TEXT NOT NULL CHECK(worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  artifact_sha256 TEXT NOT NULL CHECK(artifact_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN ('parked','latch_acknowledged','operational')),
  health_state TEXT NOT NULL CHECK(health_state IN ('ready','degraded','draining')),
  reason_code TEXT CHECK(reason_code IS NULL OR reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','draining','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup')),
  acknowledged_latch_sha256 TEXT CHECK(acknowledged_latch_sha256 IS NULL OR (acknowledged_latch_sha256 ~ '^[0-9a-f]{64}$')),
  serving_latch_sha256 TEXT CHECK(serving_latch_sha256 IS NULL OR (serving_latch_sha256 ~ '^[0-9a-f]{64}$')),
  canonical_heartbeat_base64url TEXT NOT NULL CHECK(octet_length(canonical_heartbeat_base64url) BETWEEN 1 AND 87382 AND canonical_heartbeat_base64url ~ '^[A-Za-z0-9_-]+$'),
  dependency_probe_base64url TEXT NOT NULL CHECK(octet_length(dependency_probe_base64url) BETWEEN 1 AND 87382 AND dependency_probe_base64url ~ '^[A-Za-z0-9_-]+$'),
  poller_identity_sha256 TEXT CHECK(poller_identity_sha256 IS NULL OR (poller_identity_sha256 ~ '^[0-9a-f]{64}$')),
  backlog_count BIGINT CHECK(backlog_count IS NULL OR backlog_count BETWEEN 0 AND 9007199254740991),
  delivered_task_count BIGINT CHECK(delivered_task_count IS NULL OR delivered_task_count BETWEEN 0 AND 9007199254740991),
  parked_predecessor_sequence BIGINT CHECK(parked_predecessor_sequence IS NULL OR parked_predecessor_sequence BETWEEN 1 AND 9007199254740991),
  parked_predecessor_sha256 TEXT CHECK(parked_predecessor_sha256 IS NULL OR (parked_predecessor_sha256 ~ '^[0-9a-f]{64}$')),
  heartbeat_at_ms BIGINT NOT NULL CHECK(heartbeat_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(runtime_instance_id,instance_epoch,heartbeat_sequence),
  UNIQUE(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256),
  UNIQUE(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256),
  FOREIGN KEY(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel) REFERENCES jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256) REFERENCES jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,parked_predecessor_sequence,parked_predecessor_sha256) REFERENCES jobs_managed_cloud_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(serving_latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_serving_latches(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) ON DELETE RESTRICT,
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE),
  CHECK(((health_state='ready' AND reason_code IS NULL) OR (health_state='draining' AND reason_code='draining') OR (health_state='degraded' AND reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup'))) IS TRUE),
  CHECK(((lifecycle_state='parked' AND acknowledged_latch_sha256 IS NULL AND health_state<>'ready') OR (lifecycle_state='latch_acknowledged' AND acknowledged_latch_sha256 IS NOT NULL AND acknowledged_latch_sha256=serving_latch_sha256 AND health_state<>'ready') OR (lifecycle_state='operational' AND acknowledged_latch_sha256 IS NOT NULL AND acknowledged_latch_sha256=serving_latch_sha256)) IS TRUE),
  CHECK(((parked_predecessor_sequence IS NULL AND parked_predecessor_sha256 IS NULL) OR (parked_predecessor_sequence IS NOT NULL AND parked_predecessor_sha256 IS NOT NULL AND parked_predecessor_sequence<heartbeat_sequence)) IS TRUE),
  CHECK(health_state<>'ready' OR (parked_predecessor_sequence IS NOT NULL AND parked_predecessor_sha256 IS NOT NULL)),
  CHECK(((poller_identity_sha256 IS NULL AND backlog_count IS NULL AND delivered_task_count IS NULL) OR (role='workflow_worker' AND poller_identity_sha256 IS NOT NULL AND backlog_count IS NOT NULL AND delivered_task_count IS NOT NULL)) IS TRUE)
);
CREATE OR REPLACE FUNCTION jobs_mc_base_observation_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND a.purpose IS NOT DISTINCT FROM NEW.purpose AND a.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND a.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND a.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256) OR (NEW.parked_predecessor_sequence IS NOT NULL AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations p WHERE p.runtime_instance_id=NEW.runtime_instance_id AND p.instance_epoch=NEW.instance_epoch AND p.heartbeat_sequence=NEW.parked_predecessor_sequence AND p.observation_sha256=NEW.parked_predecessor_sha256 AND p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup' AND p.activation_sha256=NEW.activation_sha256 AND p.manifest_sha256=NEW.manifest_sha256 AND p.head_revision=NEW.head_revision AND p.transition_sha256=NEW.transition_sha256 AND p.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND p.purpose IS NOT DISTINCT FROM NEW.purpose AND p.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND p.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND p.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256)) OR (NEW.lifecycle_state='operational' AND NEW.health_state='ready' AND NOT EXISTS(
    -- The observation is inserted BEFORE latest CAS: h must still be the immediate
    -- previous fact. A negative or latch_acknowledged fact breaks the parked chain.
    SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats h
    JOIN jobs_managed_cloud_runtime_observations p
      ON p.runtime_instance_id=h.runtime_instance_id AND p.instance_epoch=h.instance_epoch
      AND p.heartbeat_sequence=h.heartbeat_sequence AND p.observation_sha256=h.observation_sha256
    WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch
      AND h.protocol_version=2 AND p.heartbeat_sequence=NEW.heartbeat_sequence-1
      AND p.activation_sha256 IS NOT DISTINCT FROM NEW.activation_sha256
      AND p.manifest_sha256 IS NOT DISTINCT FROM NEW.manifest_sha256
      AND p.head_revision IS NOT DISTINCT FROM NEW.head_revision
      AND p.transition_sha256 IS NOT DISTINCT FROM NEW.transition_sha256
      AND p.authority_kind IS NOT DISTINCT FROM NEW.authority_kind
      AND p.purpose IS NOT DISTINCT FROM NEW.purpose
      AND p.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256
      AND p.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id
      AND p.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256
      AND ((p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup'
        AND p.heartbeat_sequence=NEW.parked_predecessor_sequence
        AND p.observation_sha256=NEW.parked_predecessor_sha256)
       OR (p.lifecycle_state='operational' AND p.health_state='ready' AND p.reason_code IS NULL
        AND p.serving_latch_sha256=NEW.serving_latch_sha256
        AND p.acknowledged_latch_sha256=NEW.acknowledged_latch_sha256
        AND p.parked_predecessor_sequence=NEW.parked_predecessor_sequence
        AND p.parked_predecessor_sha256=NEW.parked_predecessor_sha256))))
 OR NEW.heartbeat_sequence<>COALESCE((SELECT h.heartbeat_sequence+1 FROM jobs_managed_cloud_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch),1) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch AND h.heartbeat_at_ms>NEW.heartbeat_at_ms) OR NEW.heartbeat_at_ms>floor(extract(epoch FROM clock_timestamp())*1000)::bigint OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND i.claimed_at_ms<=NEW.heartbeat_at_ms) THEN RAISE EXCEPTION 'managed cloud observation identity or predecessor is invalid'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_base_observation_guard ON jobs_managed_cloud_runtime_observations;
CREATE TRIGGER trg_jobs_mc_base_observation_guard BEFORE INSERT ON jobs_managed_cloud_runtime_observations
FOR EACH ROW EXECUTE FUNCTION jobs_mc_base_observation_guard();
DROP TRIGGER IF EXISTS trg_jobs_mc_base_observation_immutable ON jobs_managed_cloud_runtime_observations;
CREATE TRIGGER trg_jobs_mc_base_observation_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_runtime_observations
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();
ALTER TABLE jobs_managed_cloud_runtime_heartbeats ADD COLUMN IF NOT EXISTS protocol_version BIGINT NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)), ADD COLUMN IF NOT EXISTS observation_sha256 TEXT CHECK(observation_sha256 IS NULL OR (observation_sha256 ~ '^[0-9a-f]{64}$'));
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_runtime_heartbeats'::regclass AND conname='fk_jobs_mc_base_latest_observation') THEN
  ALTER TABLE jobs_managed_cloud_runtime_heartbeats ADD CONSTRAINT fk_jobs_mc_base_latest_observation FOREIGN KEY(observation_sha256) REFERENCES jobs_managed_cloud_runtime_observations(observation_sha256) ON DELETE RESTRICT;
 END IF;
END $$;
CREATE OR REPLACE FUNCTION jobs_mc_base_latest_observation_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.observation_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND i.protocol_version=2 AND NEW.protocol_version=2 AND NEW.observation_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations o WHERE o.runtime_instance_id IS NOT DISTINCT FROM NEW.runtime_instance_id AND o.instance_epoch IS NOT DISTINCT FROM NEW.instance_epoch AND o.heartbeat_sequence IS NOT DISTINCT FROM NEW.heartbeat_sequence AND o.activation_sha256 IS NOT DISTINCT FROM NEW.activation_sha256 AND o.manifest_sha256 IS NOT DISTINCT FROM NEW.manifest_sha256 AND o.component_id IS NOT DISTINCT FROM NEW.component_id AND o.role IS NOT DISTINCT FROM NEW.role AND o.worker_id IS NOT DISTINCT FROM NEW.worker_id AND o.artifact_sha256 IS NOT DISTINCT FROM NEW.artifact_sha256 AND o.migration_set_sha256 IS NOT DISTINCT FROM NEW.migration_set_sha256 AND o.config_schema_sha256 IS NOT DISTINCT FROM NEW.config_schema_sha256 AND o.protocol_set_sha256 IS NOT DISTINCT FROM NEW.protocol_set_sha256 AND o.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256 AND o.failure_converter_sha256 IS NOT DISTINCT FROM NEW.failure_converter_sha256 AND o.dependency_evidence_sha256 IS NOT DISTINCT FROM NEW.dependency_evidence_sha256 AND o.health_state IS NOT DISTINCT FROM NEW.health_state AND o.reason_code IS NOT DISTINCT FROM NEW.reason_code AND o.heartbeat_at_ms IS NOT DISTINCT FROM NEW.heartbeat_at_ms AND o.head_revision=NEW.observed_head_revision AND o.transition_sha256=NEW.observed_transition_sha256 AND o.protocol_version=NEW.protocol_version AND o.observation_sha256=NEW.observation_sha256)))) THEN RAISE EXCEPTION 'managed cloud latest heartbeat observation does not match'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_base_latest_observation_guard ON jobs_managed_cloud_runtime_heartbeats;
CREATE TRIGGER trg_jobs_mc_base_latest_observation_guard BEFORE INSERT OR UPDATE ON jobs_managed_cloud_runtime_heartbeats
FOR EACH ROW EXECUTE FUNCTION jobs_mc_base_latest_observation_guard();
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_bootstrap_runtime_members (
  bootstrap_set_sha256 TEXT NOT NULL CHECK(bootstrap_set_sha256 ~ '^[0-9a-f]{64}$'),
  runtime_instance_id TEXT NOT NULL CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(observation_sha256 ~ '^[0-9a-f]{64}$'),
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  PRIMARY KEY(bootstrap_set_sha256,runtime_instance_id,instance_epoch),
  FOREIGN KEY(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION jobs_mc_base_bootstrap_member_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations o JOIN jobs_managed_cloud_bootstrap_sets b ON b.bootstrap_set_sha256=NEW.bootstrap_set_sha256 WHERE o.runtime_instance_id=NEW.runtime_instance_id AND o.instance_epoch=NEW.instance_epoch AND o.heartbeat_sequence=NEW.heartbeat_sequence AND o.observation_sha256=NEW.observation_sha256 AND o.lifecycle_state='parked' AND o.health_state='degraded' AND o.reason_code='startup' AND o.authority_kind IS NOT DISTINCT FROM b.authority_kind AND o.purpose IS NOT DISTINCT FROM b.purpose AND o.context_sha256 IS NOT DISTINCT FROM b.context_sha256 AND o.queue_generation_id IS NOT DISTINCT FROM b.queue_generation_id AND o.task_queue_sha256 IS NOT DISTINCT FROM b.task_queue_sha256) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_serving_latches l WHERE l.bootstrap_set_sha256=NEW.bootstrap_set_sha256) THEN RAISE EXCEPTION 'managed cloud bootstrap member is not an unsealed parked fact'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_base_bootstrap_member_guard ON jobs_managed_cloud_bootstrap_runtime_members;
CREATE TRIGGER trg_jobs_mc_base_bootstrap_member_guard BEFORE INSERT ON jobs_managed_cloud_bootstrap_runtime_members
FOR EACH ROW EXECUTE FUNCTION jobs_mc_base_bootstrap_member_guard();
DROP TRIGGER IF EXISTS trg_jobs_mc_base_bootstrap_member_immutable ON jobs_managed_cloud_bootstrap_runtime_members;
CREATE TRIGGER trg_jobs_mc_base_bootstrap_member_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_bootstrap_runtime_members
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

-- Permanent osv facts. Canonical blobs MUST come from the closed nonsecret
-- Rust serializers; SQL does not decode base64, verify the digest, or prove work.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_osv_runtime_observations (
  runtime_instance_id TEXT NOT NULL CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(observation_sha256 ~ '^[0-9a-f]{64}$') UNIQUE,
  grant_id TEXT NOT NULL CHECK(grant_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  runtime_identity_sha256 TEXT NOT NULL CHECK(runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_version BIGINT NOT NULL DEFAULT 2 CHECK(protocol_version=2),
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (context_sha256 ~ '^[0-9a-f]{64}$')),
  queue_generation_id TEXT NOT NULL CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  component_id TEXT NOT NULL CHECK(component_id IN ('jobs-api','jobs-runner','jobs-workflows')),
  role TEXT NOT NULL CHECK(role='original_source_verifier'),
  worker_id TEXT NOT NULL CHECK(worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  artifact_sha256 TEXT NOT NULL CHECK(artifact_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN ('parked','latch_acknowledged','operational')),
  health_state TEXT NOT NULL CHECK(health_state IN ('ready','degraded','draining')),
  reason_code TEXT CHECK(reason_code IS NULL OR reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','draining','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup')),
  acknowledged_latch_sha256 TEXT CHECK(acknowledged_latch_sha256 IS NULL OR (acknowledged_latch_sha256 ~ '^[0-9a-f]{64}$')),
  serving_latch_sha256 TEXT CHECK(serving_latch_sha256 IS NULL OR (serving_latch_sha256 ~ '^[0-9a-f]{64}$')),
  canonical_heartbeat_base64url TEXT NOT NULL CHECK(octet_length(canonical_heartbeat_base64url) BETWEEN 1 AND 87382 AND canonical_heartbeat_base64url ~ '^[A-Za-z0-9_-]+$'),
  dependency_probe_base64url TEXT NOT NULL CHECK(octet_length(dependency_probe_base64url) BETWEEN 1 AND 87382 AND dependency_probe_base64url ~ '^[A-Za-z0-9_-]+$'),
  poller_identity_sha256 TEXT CHECK(poller_identity_sha256 IS NULL OR (poller_identity_sha256 ~ '^[0-9a-f]{64}$')),
  backlog_count BIGINT CHECK(backlog_count IS NULL OR backlog_count BETWEEN 0 AND 9007199254740991),
  delivered_task_count BIGINT CHECK(delivered_task_count IS NULL OR delivered_task_count BETWEEN 0 AND 9007199254740991),
  parked_predecessor_sequence BIGINT CHECK(parked_predecessor_sequence IS NULL OR parked_predecessor_sequence BETWEEN 1 AND 9007199254740991),
  parked_predecessor_sha256 TEXT CHECK(parked_predecessor_sha256 IS NULL OR (parked_predecessor_sha256 ~ '^[0-9a-f]{64}$')),
  heartbeat_at_ms BIGINT NOT NULL CHECK(heartbeat_at_ms BETWEEN 0 AND 9007199254740991),
  boot_id TEXT NOT NULL CHECK(boot_id ~ '^[A-Za-z0-9_.:/-]{20,128}$'),
  boot_generation BIGINT NOT NULL CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  PRIMARY KEY(runtime_instance_id,instance_epoch,heartbeat_sequence),
  UNIQUE(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256),
  UNIQUE(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256),
  FOREIGN KEY(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel) REFERENCES jobs_managed_cloud_original_source_verifier_runtime_instances(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256) REFERENCES jobs_managed_cloud_original_source_verifier_runtime_instances(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,parked_predecessor_sequence,parked_predecessor_sha256) REFERENCES jobs_managed_cloud_osv_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(serving_latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_serving_latches(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) ON DELETE RESTRICT,
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE),
  CHECK(((health_state='ready' AND reason_code IS NULL) OR (health_state='draining' AND reason_code='draining') OR (health_state='degraded' AND reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup'))) IS TRUE),
  CHECK(((lifecycle_state='parked' AND acknowledged_latch_sha256 IS NULL AND health_state<>'ready') OR (lifecycle_state='latch_acknowledged' AND acknowledged_latch_sha256 IS NOT NULL AND acknowledged_latch_sha256=serving_latch_sha256 AND health_state<>'ready') OR (lifecycle_state='operational' AND acknowledged_latch_sha256 IS NOT NULL AND acknowledged_latch_sha256=serving_latch_sha256)) IS TRUE),
  CHECK(((parked_predecessor_sequence IS NULL AND parked_predecessor_sha256 IS NULL) OR (parked_predecessor_sequence IS NOT NULL AND parked_predecessor_sha256 IS NOT NULL AND parked_predecessor_sequence<heartbeat_sequence)) IS TRUE),
  CHECK(health_state<>'ready' OR (parked_predecessor_sequence IS NOT NULL AND parked_predecessor_sha256 IS NOT NULL)),
  CHECK(((poller_identity_sha256 IS NULL AND backlog_count IS NULL AND delivered_task_count IS NULL) OR (role='workflow_worker' AND poller_identity_sha256 IS NOT NULL AND backlog_count IS NOT NULL AND delivered_task_count IS NOT NULL)) IS TRUE),
  FOREIGN KEY(runtime_instance_id,instance_epoch,boot_id,boot_generation,protocol_version) REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(runtime_instance_id,instance_epoch,boot_id,boot_generation,protocol_version) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION jobs_mc_osv_observation_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND a.purpose IS NOT DISTINCT FROM NEW.purpose AND a.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND a.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND a.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256) OR (NEW.parked_predecessor_sequence IS NOT NULL AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations p WHERE p.runtime_instance_id=NEW.runtime_instance_id AND p.instance_epoch=NEW.instance_epoch AND p.heartbeat_sequence=NEW.parked_predecessor_sequence AND p.observation_sha256=NEW.parked_predecessor_sha256 AND p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup' AND p.activation_sha256=NEW.activation_sha256 AND p.manifest_sha256=NEW.manifest_sha256 AND p.head_revision=NEW.head_revision AND p.transition_sha256=NEW.transition_sha256 AND p.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND p.purpose IS NOT DISTINCT FROM NEW.purpose AND p.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND p.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND p.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256 AND p.boot_id=NEW.boot_id AND p.boot_generation=NEW.boot_generation)) OR (NEW.lifecycle_state='operational' AND NEW.health_state='ready' AND NOT EXISTS(
    -- The observation is inserted BEFORE latest CAS: h must still be the immediate
    -- previous fact. A negative or latch_acknowledged fact breaks the parked chain.
    SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats h
    JOIN jobs_managed_cloud_osv_runtime_observations p
      ON p.runtime_instance_id=h.runtime_instance_id AND p.instance_epoch=h.instance_epoch
      AND p.heartbeat_sequence=h.heartbeat_sequence AND p.observation_sha256=h.observation_sha256
    WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch
      AND h.protocol_version=2 AND p.heartbeat_sequence=NEW.heartbeat_sequence-1
      AND p.activation_sha256 IS NOT DISTINCT FROM NEW.activation_sha256
      AND p.manifest_sha256 IS NOT DISTINCT FROM NEW.manifest_sha256
      AND p.head_revision IS NOT DISTINCT FROM NEW.head_revision
      AND p.transition_sha256 IS NOT DISTINCT FROM NEW.transition_sha256
      AND p.authority_kind IS NOT DISTINCT FROM NEW.authority_kind
      AND p.purpose IS NOT DISTINCT FROM NEW.purpose
      AND p.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256
      AND p.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id
      AND p.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256
      AND p.boot_id=NEW.boot_id AND p.boot_generation=NEW.boot_generation
      AND ((p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup'
        AND p.heartbeat_sequence=NEW.parked_predecessor_sequence
        AND p.observation_sha256=NEW.parked_predecessor_sha256)
       OR (p.lifecycle_state='operational' AND p.health_state='ready' AND p.reason_code IS NULL
        AND p.serving_latch_sha256=NEW.serving_latch_sha256
        AND p.acknowledged_latch_sha256=NEW.acknowledged_latch_sha256
        AND p.parked_predecessor_sequence=NEW.parked_predecessor_sequence
        AND p.parked_predecessor_sha256=NEW.parked_predecessor_sha256))))
 OR NEW.heartbeat_sequence<>COALESCE((SELECT h.heartbeat_sequence+1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch),1) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch AND h.heartbeat_at_ms>NEW.heartbeat_at_ms) OR NEW.heartbeat_at_ms>floor(extract(epoch FROM clock_timestamp())*1000)::bigint OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND i.claimed_at_ms<=NEW.heartbeat_at_ms) OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch WHERE r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND r.worker_id=NEW.worker_id AND h.current_boot_generation=r.boot_generation) OR (NEW.health_state='ready' AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_startup_authorities s WHERE s.protocol_version=2 AND s.boot_id=NEW.boot_id AND s.runtime_instance_id=NEW.runtime_instance_id AND s.instance_epoch=NEW.instance_epoch AND s.boot_generation=NEW.boot_generation AND NEW.parked_predecessor_sequence>=s.parked_heartbeat_sequence)) THEN RAISE EXCEPTION 'managed cloud observation identity or predecessor is invalid'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_observation_guard ON jobs_managed_cloud_osv_runtime_observations;
CREATE TRIGGER trg_jobs_mc_osv_observation_guard BEFORE INSERT ON jobs_managed_cloud_osv_runtime_observations
FOR EACH ROW EXECUTE FUNCTION jobs_mc_osv_observation_guard();
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_observation_immutable ON jobs_managed_cloud_osv_runtime_observations;
CREATE TRIGGER trg_jobs_mc_osv_observation_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_osv_runtime_observations
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();
ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_heartbeats ADD COLUMN IF NOT EXISTS protocol_version BIGINT NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)), ADD COLUMN IF NOT EXISTS observation_sha256 TEXT CHECK(observation_sha256 IS NULL OR (observation_sha256 ~ '^[0-9a-f]{64}$'));
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_original_source_verifier_runtime_heartbeats'::regclass AND conname='fk_jobs_mc_osv_latest_observation') THEN
  ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_heartbeats ADD CONSTRAINT fk_jobs_mc_osv_latest_observation FOREIGN KEY(observation_sha256) REFERENCES jobs_managed_cloud_osv_runtime_observations(observation_sha256) ON DELETE RESTRICT;
 END IF;
END $$;
CREATE OR REPLACE FUNCTION jobs_mc_osv_latest_observation_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.observation_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND i.protocol_version=2 AND NEW.protocol_version=2 AND NEW.observation_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o WHERE o.runtime_instance_id IS NOT DISTINCT FROM NEW.runtime_instance_id AND o.instance_epoch IS NOT DISTINCT FROM NEW.instance_epoch AND o.heartbeat_sequence IS NOT DISTINCT FROM NEW.heartbeat_sequence AND o.activation_sha256 IS NOT DISTINCT FROM NEW.activation_sha256 AND o.manifest_sha256 IS NOT DISTINCT FROM NEW.manifest_sha256 AND o.component_id IS NOT DISTINCT FROM NEW.component_id AND o.role IS NOT DISTINCT FROM NEW.role AND o.worker_id IS NOT DISTINCT FROM NEW.worker_id AND o.artifact_sha256 IS NOT DISTINCT FROM NEW.artifact_sha256 AND o.migration_set_sha256 IS NOT DISTINCT FROM NEW.migration_set_sha256 AND o.config_schema_sha256 IS NOT DISTINCT FROM NEW.config_schema_sha256 AND o.protocol_set_sha256 IS NOT DISTINCT FROM NEW.protocol_set_sha256 AND o.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256 AND o.failure_converter_sha256 IS NOT DISTINCT FROM NEW.failure_converter_sha256 AND o.dependency_evidence_sha256 IS NOT DISTINCT FROM NEW.dependency_evidence_sha256 AND o.health_state IS NOT DISTINCT FROM NEW.health_state AND o.reason_code IS NOT DISTINCT FROM NEW.reason_code AND o.heartbeat_at_ms IS NOT DISTINCT FROM NEW.heartbeat_at_ms AND o.head_revision=NEW.observed_head_revision AND o.transition_sha256=NEW.observed_transition_sha256 AND o.protocol_version=NEW.protocol_version AND o.observation_sha256=NEW.observation_sha256)))) THEN RAISE EXCEPTION 'managed cloud latest heartbeat observation does not match'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_latest_observation_guard ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_mc_osv_latest_observation_guard BEFORE INSERT OR UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW EXECUTE FUNCTION jobs_mc_osv_latest_observation_guard();
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_bootstrap_source_verifier_members (
  bootstrap_set_sha256 TEXT NOT NULL CHECK(bootstrap_set_sha256 ~ '^[0-9a-f]{64}$'),
  runtime_instance_id TEXT NOT NULL CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(observation_sha256 ~ '^[0-9a-f]{64}$'),
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  PRIMARY KEY(bootstrap_set_sha256,runtime_instance_id,instance_epoch),
  FOREIGN KEY(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_osv_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION jobs_mc_osv_bootstrap_member_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o JOIN jobs_managed_cloud_bootstrap_sets b ON b.bootstrap_set_sha256=NEW.bootstrap_set_sha256 WHERE o.runtime_instance_id=NEW.runtime_instance_id AND o.instance_epoch=NEW.instance_epoch AND o.heartbeat_sequence=NEW.heartbeat_sequence AND o.observation_sha256=NEW.observation_sha256 AND o.lifecycle_state='parked' AND o.health_state='degraded' AND o.reason_code='startup' AND o.authority_kind IS NOT DISTINCT FROM b.authority_kind AND o.purpose IS NOT DISTINCT FROM b.purpose AND o.context_sha256 IS NOT DISTINCT FROM b.context_sha256 AND o.queue_generation_id IS NOT DISTINCT FROM b.queue_generation_id AND o.task_queue_sha256 IS NOT DISTINCT FROM b.task_queue_sha256) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_serving_latches l WHERE l.bootstrap_set_sha256=NEW.bootstrap_set_sha256) THEN RAISE EXCEPTION 'managed cloud bootstrap member is not an unsealed parked fact'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_bootstrap_member_guard ON jobs_managed_cloud_bootstrap_source_verifier_members;
CREATE TRIGGER trg_jobs_mc_osv_bootstrap_member_guard BEFORE INSERT ON jobs_managed_cloud_bootstrap_source_verifier_members
FOR EACH ROW EXECUTE FUNCTION jobs_mc_osv_bootstrap_member_guard();
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_bootstrap_member_immutable ON jobs_managed_cloud_bootstrap_source_verifier_members;
CREATE TRIGGER trg_jobs_mc_osv_bootstrap_member_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_bootstrap_source_verifier_members
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_queue_contaminations (
  activation_sha256 TEXT NOT NULL CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (context_sha256 ~ '^[0-9a-f]{64}$')),
  queue_generation_id TEXT NOT NULL CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  runtime_instance_id TEXT NOT NULL CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  worker_id TEXT NOT NULL CHECK(worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  role TEXT NOT NULL DEFAULT 'workflow_worker' CHECK(role='workflow_worker'),
  delivery_identity_sha256 TEXT NOT NULL CHECK(delivery_identity_sha256 ~ '^[0-9a-f]{64}$'),
  reason_code TEXT NOT NULL CHECK(reason_code='unexpected_pre_serving_delivery'),
  observed_at_ms BIGINT NOT NULL CHECK(observed_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(queue_generation_id,runtime_instance_id,instance_epoch,delivery_identity_sha256),
  FOREIGN KEY(runtime_instance_id,instance_epoch,worker_id,activation_sha256,manifest_sha256,head_revision,transition_sha256,role) REFERENCES jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,worker_id,activation_sha256,manifest_sha256,head_revision,transition_sha256,role) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT,
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE)
);
CREATE OR REPLACE FUNCTION jobs_mc_queue_contamination_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND a.purpose IS NOT DISTINCT FROM NEW.purpose AND a.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND a.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND a.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256) THEN RAISE EXCEPTION 'managed cloud contamination head binding is invalid'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_queue_contamination_guard ON jobs_managed_cloud_queue_contaminations;
CREATE TRIGGER trg_jobs_mc_queue_contamination_guard BEFORE INSERT ON jobs_managed_cloud_queue_contaminations
FOR EACH ROW EXECUTE FUNCTION jobs_mc_queue_contamination_guard();
DROP TRIGGER IF EXISTS trg_jobs_mc_queue_contamination_immutable ON jobs_managed_cloud_queue_contaminations;
CREATE TRIGGER trg_jobs_mc_queue_contamination_immutable BEFORE UPDATE OR DELETE ON jobs_managed_cloud_queue_contaminations
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

-- No role-minimum/real-probe/isolation authorization is inferred here. The
-- signed latch writer MUST validate the exact current quorum and signature in
-- the same serialized transaction. These guards only bind stored metadata.
CREATE OR REPLACE FUNCTION jobs_mc_serving_latch_insert_guard() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND a.purpose IS NOT DISTINCT FROM NEW.purpose AND a.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND a.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND a.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256) OR NEW.expires_at_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND NEW.not_before_ms>=a.not_before_ms AND NEW.expires_at_ms<=a.expires_at_ms) OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_bootstrap_sets b WHERE b.bootstrap_set_sha256=NEW.bootstrap_set_sha256 AND b.member_count=((SELECT count(*) FROM jobs_managed_cloud_bootstrap_runtime_members m WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256)+(SELECT count(*) FROM jobs_managed_cloud_bootstrap_source_verifier_members m WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256)) AND b.authority_kind IS NOT DISTINCT FROM NEW.authority_kind AND b.purpose IS NOT DISTINCT FROM NEW.purpose AND b.context_sha256 IS NOT DISTINCT FROM NEW.context_sha256 AND b.queue_generation_id IS NOT DISTINCT FROM NEW.queue_generation_id AND b.task_queue_sha256 IS NOT DISTINCT FROM NEW.task_queue_sha256) OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_signature_sets s WHERE s.signature_set_sha256=NEW.authorization_signature_set_sha256 AND s.target_audience='bluey-jobs-managed-cloud-serving-latch-v1' AND s.target_sha256=NEW.latch_sha256 AND s.trust_generation=NEW.trust_generation) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_queue_contaminations c WHERE c.queue_generation_id=NEW.queue_generation_id) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_bootstrap_runtime_members m LEFT JOIN jobs_managed_cloud_runtime_heartbeats h ON h.runtime_instance_id=m.runtime_instance_id AND h.instance_epoch=m.instance_epoch WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256 AND NOT (h.observation_sha256 IS NOT DISTINCT FROM m.observation_sha256 AND h.protocol_version=2)) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_bootstrap_source_verifier_members m LEFT JOIN jobs_managed_cloud_original_source_verifier_runtime_heartbeats h ON h.runtime_instance_id=m.runtime_instance_id AND h.instance_epoch=m.instance_epoch WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256 AND NOT (h.observation_sha256 IS NOT DISTINCT FROM m.observation_sha256 AND h.protocol_version=2)) THEN RAISE EXCEPTION 'managed cloud serving latch metadata or bootstrap snapshot is invalid'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_serving_latch_insert_guard ON jobs_managed_cloud_serving_latches;
CREATE TRIGGER trg_jobs_mc_serving_latch_insert_guard BEFORE INSERT ON jobs_managed_cloud_serving_latches
FOR EACH ROW EXECUTE FUNCTION jobs_mc_serving_latch_insert_guard();

ALTER TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities
 ADD COLUMN IF NOT EXISTS protocol_version BIGINT NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
 ADD COLUMN IF NOT EXISTS prior_heartbeat_sequence BIGINT,
 ADD COLUMN IF NOT EXISTS parked_heartbeat_sequence BIGINT,
 ADD COLUMN IF NOT EXISTS dependency_result TEXT,
 ADD COLUMN IF NOT EXISTS observation_sha256 TEXT CHECK(observation_sha256 IS NULL OR (observation_sha256 ~ '^[0-9a-f]{64}$')),
 ALTER COLUMN degraded_heartbeat_sequence DROP NOT NULL,
 ALTER COLUMN ready_heartbeat_sequence DROP NOT NULL,
 ALTER COLUMN probe_result DROP NOT NULL,
 ALTER COLUMN startup_authority_sha256 DROP NOT NULL;
DO $$ DECLARE name TEXT; BEGIN
 FOR name IN SELECT conname FROM pg_constraint WHERE conrelid='jobs_managed_cloud_source_verifier_boot_startup_authorities'::regclass AND contype='c'
  AND pg_get_constraintdef(oid) LIKE '%startup_request_id%'
 LOOP EXECUTE format('ALTER TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities DROP CONSTRAINT %I',name); END LOOP;
END $$;
ALTER TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities ADD CONSTRAINT ck_jobs_mc_boot_startup_version CHECK(((protocol_version=1 AND startup_request_id ~ '^[A-Za-z0-9_-]{20,128}$' AND degraded_heartbeat_sequence IS NOT NULL AND ready_heartbeat_sequence IS NOT NULL AND probe_result IS NOT NULL AND startup_authority_sha256 IS NOT NULL AND prior_heartbeat_sequence IS NULL AND parked_heartbeat_sequence IS NULL AND dependency_result IS NULL AND observation_sha256 IS NULL) OR (protocol_version=2 AND startup_request_id IS NOT NULL AND startup_request_id ~ '^[A-Za-z0-9_.:/-]{20,128}$' AND degraded_heartbeat_sequence IS NULL AND ready_heartbeat_sequence IS NULL AND probe_result IS NULL AND startup_authority_sha256 IS NULL AND prior_heartbeat_sequence BETWEEN 1 AND 9007199254740990 AND parked_heartbeat_sequence=prior_heartbeat_sequence+1 AND dependency_result='dependency_ready' AND observation_sha256 IS NOT NULL AND observation_sha256 ~ '^[0-9a-f]{64}$' AND request_sha256 ~ '^[0-9a-f]{64}$' AND response_sha256 ~ '^[0-9a-f]{64}$' AND octet_length(canonical_response_base64url) BETWEEN 1 AND 87382 AND canonical_response_base64url ~ '^[A-Za-z0-9_-]+$')) IS TRUE);
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_source_verifier_boot_startup_authorities'::regclass AND conname='fk_jobs_mc_boot_startup_observation') THEN
  ALTER TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities ADD CONSTRAINT fk_jobs_mc_boot_startup_observation FOREIGN KEY(runtime_instance_id,instance_epoch,parked_heartbeat_sequence,observation_sha256) REFERENCES jobs_managed_cloud_osv_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256) ON DELETE RESTRICT;
 END IF;
END $$;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_startup_requires_degraded ON jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_osv_boot_startup_requires_degraded BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
FOR EACH ROW WHEN (NEW.protocol_version=1) EXECUTE FUNCTION jobs_osv_boot_startup_requires_degraded();
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_osv_boot_heartbeat BEFORE INSERT OR UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW WHEN (NEW.protocol_version=1) EXECUTE FUNCTION jobs_osv_boot_heartbeat_guard();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence BEFORE INSERT OR UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW WHEN (NEW.protocol_version=1) EXECUTE FUNCTION bluey_jobs_guard_osv_heartbeat_sequence();
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_v2_sequence ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_mc_osv_v2_sequence BEFORE INSERT OR UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW WHEN (NEW.protocol_version=2) EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_heartbeat_sequence();
-- Preserve PostgreSQL042's exact legacy ready-lowering body/message. V2
-- negative evidence instead replaces latest through its observation/CAS guards.
DROP TRIGGER IF EXISTS trg_jobs_managed_osv_heartbeat_ready_fence
  ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_managed_osv_heartbeat_ready_fence
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW WHEN (NEW.protocol_version=1)
EXECUTE FUNCTION jobs_managed_osv_heartbeat_ready_fence();
-- Cross-version isolation: legacy effect-binding tables cannot consume v2
-- registration/parked facts. Preserve every existing v1 predicate and message.
CREATE OR REPLACE FUNCTION jobs_osv_attempt_boot_binding_guard() RETURNS TRIGGER AS $$ BEGIN
 IF TG_OP = 'DELETE' THEN
   IF EXISTS(SELECT 1 FROM jobs_original_source_verification_attempts attempt WHERE attempt.attempt_id=OLD.attempt_id) THEN RAISE EXCEPTION 'source-verifier attempt boot binding is immutable'; END IF;
   RETURN OLD;
 END IF;
 IF TG_OP <> 'INSERT' THEN RAISE EXCEPTION 'source-verifier attempt boot binding is immutable'; END IF;
 IF NOT EXISTS(SELECT 1 FROM jobs_original_source_verification_attempts attempt JOIN jobs_managed_cloud_source_verifier_boot_registrations registration ON registration.boot_id=NEW.boot_id JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head ON head.runtime_instance_id=registration.runtime_instance_id AND head.instance_epoch=registration.instance_epoch WHERE registration.protocol_version=1 AND attempt.attempt_id=NEW.attempt_id AND attempt.runtime_instance_id=NEW.runtime_instance_id AND attempt.runtime_instance_epoch=NEW.runtime_instance_epoch AND attempt.worker_id=registration.worker_id AND registration.runtime_instance_id=NEW.runtime_instance_id AND registration.instance_epoch=NEW.runtime_instance_epoch AND registration.boot_generation=NEW.boot_generation AND head.current_boot_generation=NEW.boot_generation AND head.ready_boot_generation=NEW.boot_generation) THEN RAISE EXCEPTION 'source-verifier attempt boot binding is stale'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION jobs_osv_boot_binding_guard() RETURNS TRIGGER AS $$ BEGIN
 IF TG_OP <> 'INSERT' THEN RAISE EXCEPTION 'source-verifier heartbeat boot binding is immutable'; END IF;
 IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch WHERE r.protocol_version=1 AND r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND h.current_boot_generation=NEW.boot_generation) THEN RAISE EXCEPTION 'source-verifier heartbeat boot binding is stale'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_startup_guard()
RETURNS TRIGGER AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations registration
     WHERE registration.protocol_version = NEW.protocol_version
       AND registration.boot_id = NEW.boot_id
       AND registration.runtime_instance_id = NEW.runtime_instance_id
       AND registration.instance_epoch = NEW.instance_epoch
       AND registration.boot_generation = NEW.boot_generation
       AND registration.worker_id = NEW.worker_id
  ) THEN
    RAISE EXCEPTION 'source-verifier boot startup binding is invalid';
  END IF;
  IF EXISTS (
    SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
     WHERE head.runtime_instance_id = NEW.runtime_instance_id
       AND head.instance_epoch = NEW.instance_epoch
       AND head.ready_boot_generation IS NOT NULL
       AND head.ready_boot_generation > NEW.boot_generation
  ) THEN
    RAISE EXCEPTION 'source-verifier boot startup generation is stale';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION jobs_mc_boot_registration_binding_guard() RETURNS trigger AS $$
BEGIN IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND i.worker_id=NEW.worker_id AND i.runtime_identity_sha256=NEW.runtime_identity_sha256 AND i.activation_sha256=NEW.activation_sha256 AND i.manifest_sha256=NEW.manifest_sha256 AND i.role=NEW.role AND i.component_id=NEW.component_id AND ((NEW.protocol_version=1 AND i.protocol_version=1 AND a.authority_kind='legacy_activation') OR (NEW.protocol_version=2 AND i.protocol_version=2 AND a.authority_kind='provisioning_v4'))) THEN RAISE EXCEPTION 'source-verifier boot registration protocol binding is invalid'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_boot_registration_binding_guard ON jobs_managed_cloud_source_verifier_boot_registrations;
CREATE TRIGGER trg_jobs_mc_boot_registration_binding_guard BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_registrations FOR EACH ROW EXECUTE FUNCTION jobs_mc_boot_registration_binding_guard();
CREATE OR REPLACE FUNCTION jobs_mc_boot_startup_v2_guard() RETURNS trigger AS $$
BEGIN IF NEW.protocol_version=2 AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch JOIN jobs_managed_cloud_osv_runtime_observations o ON o.runtime_instance_id=r.runtime_instance_id AND o.instance_epoch=r.instance_epoch AND o.boot_id=r.boot_id AND o.boot_generation=r.boot_generation JOIN jobs_managed_cloud_osv_runtime_observations p ON p.runtime_instance_id=o.runtime_instance_id AND p.instance_epoch=o.instance_epoch AND p.boot_id=o.boot_id AND p.boot_generation=o.boot_generation WHERE r.protocol_version=2 AND r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND r.worker_id=NEW.worker_id AND h.current_boot_generation=r.boot_generation AND (h.ready_boot_generation IS NULL OR h.ready_boot_generation<r.boot_generation) AND o.heartbeat_sequence=NEW.parked_heartbeat_sequence AND o.observation_sha256=NEW.observation_sha256 AND o.worker_id=NEW.worker_id AND o.lifecycle_state='parked' AND o.health_state='degraded' AND o.reason_code='startup' AND o.acknowledged_latch_sha256 IS NULL AND o.poller_identity_sha256 IS NULL AND o.backlog_count IS NULL AND o.delivered_task_count IS NULL AND o.heartbeat_at_ms=NEW.startup_at_ms AND p.heartbeat_sequence=NEW.prior_heartbeat_sequence AND p.lifecycle_state='parked' AND p.health_state='degraded') THEN RAISE EXCEPTION 'source-verifier parked startup receipt binding is invalid'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_boot_startup_v2_guard ON jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_mc_boot_startup_v2_guard BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities FOR EACH ROW EXECUTE FUNCTION jobs_mc_boot_startup_v2_guard();
CREATE OR REPLACE FUNCTION jobs_mc_boot_v2_ready_fact_guard() RETURNS trigger AS $$
BEGIN IF (NEW.ready_boot_generation IS NOT NULL AND NEW.ready_boot_generation IS DISTINCT FROM OLD.ready_boot_generation) AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r WHERE r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.ready_boot_generation AND r.protocol_version=2) AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o JOIN jobs_managed_cloud_source_verifier_boot_registrations r ON r.boot_id=o.boot_id AND r.runtime_instance_id=o.runtime_instance_id AND r.instance_epoch=o.instance_epoch AND r.boot_generation=o.boot_generation AND r.protocol_version=2 WHERE o.runtime_instance_id=NEW.runtime_instance_id AND o.instance_epoch=NEW.instance_epoch AND o.boot_generation=NEW.ready_boot_generation AND o.lifecycle_state='operational' AND o.health_state='ready' AND o.reason_code IS NULL AND o.acknowledged_latch_sha256=o.serving_latch_sha256 AND o.serving_latch_sha256 IS NOT NULL AND o.parked_predecessor_sequence IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_startup_authorities s WHERE s.protocol_version=2 AND s.boot_id=o.boot_id AND s.runtime_instance_id=o.runtime_instance_id AND s.instance_epoch=o.instance_epoch AND s.boot_generation=o.boot_generation AND o.parked_predecessor_sequence>=s.parked_heartbeat_sequence)) THEN RAISE EXCEPTION 'source-verifier ready generation requires an operational latch observation'; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_mc_boot_v2_ready_fact_guard ON jobs_managed_cloud_source_verifier_boot_generation_heads;
CREATE TRIGGER trg_jobs_mc_boot_v2_ready_fact_guard BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_generation_heads FOR EACH ROW EXECUTE FUNCTION jobs_mc_boot_v2_ready_fact_guard();

DO $$ BEGIN IF EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE (a.authority_kind='legacy_activation' AND (i.protocol_version<>1 OR i.claimed_latch_sha256 IS NOT NULL)) OR (a.authority_kind='provisioning_v4' AND i.protocol_version<>2)) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE (a.authority_kind='legacy_activation' AND (i.protocol_version<>1 OR i.claimed_latch_sha256 IS NOT NULL)) OR (a.authority_kind='provisioning_v4' AND i.protocol_version<>2)) THEN RAISE EXCEPTION 'existing runtime protocol kind mismatch'; END IF; END $$;
DO $migration$
DECLARE
  ledger text := current_setting('bluey.migration_table', true);
BEGIN
  IF ledger IS NULL THEN
    ledger := 'bluey_schema_migrations';
  END IF;
  IF octet_length(ledger) > 63 OR ledger COLLATE "C" !~ '^[a-z_][a-z0-9_]*$' THEN
    RAISE EXCEPTION 'invalid migration ledger identifier';
  END IF;
  EXECUTE format('INSERT INTO %I(version) VALUES ($1) ON CONFLICT(version) DO NOTHING', ledger)
    USING '046_jobs_provisioned_runtime_observations.sql';
END;
$migration$;
COMMIT;

-- Target: SQLite. Phase648 C3 DDL CANDIDATE; no writer/admission permission.
-- Existing two runtime families remain authoritative. Hash/typed metadata is NOT
-- proof of signatures, owned resources, real dependency work, or queue isolation.
-- Writers must acquire their agreed H/M -> scope/generation -> instance order,
-- then final DB time; latch, contamination and latest-observation writes serialize.
-- Boot-v2 is disjoint parked storage: no Ready update or leaseability probe.
-- Its real work-free producer/signature/context validation remains writer-owned.
-- Run only through the guarded 068 one-shot executor. On repeat startup,
-- the data-migration marker MUST skip all rebuild/ADD COLUMN statements.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

-- Introduce latest-row protocol fields before restoring any version-gated
-- trigger. SQLite reparses the entire schema during subsequent ALTERs.
ALTER TABLE jobs_managed_cloud_runtime_heartbeats ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2));
ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_heartbeats ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2));

-- Header is inert until an authenticated latch writer validates the complete
-- concrete membership union, required-role minima, latest/fresh probes and facts.
CREATE TABLE jobs_managed_cloud_bootstrap_sets (
  bootstrap_set_sha256 TEXT NOT NULL CHECK(length(CAST(bootstrap_set_sha256 AS BLOB))=64 AND length(bootstrap_set_sha256)=64 AND instr(bootstrap_set_sha256,char(0))=0 AND bootstrap_set_sha256 NOT GLOB '*[^0-9a-f]*') PRIMARY KEY,
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(length(CAST(region AS BLOB))=length(region) AND instr(region,char(0))=0 AND length(region) BETWEEN 1 AND 64 AND region NOT GLOB '*[^a-z0-9-]*' AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (length(CAST(context_sha256 AS BLOB))=64 AND length(context_sha256)=64 AND instr(context_sha256,char(0))=0 AND context_sha256 NOT GLOB '*[^0-9a-f]*')),
  queue_generation_id TEXT NOT NULL CHECK(length(queue_generation_id) BETWEEN 20 AND 128 AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id) AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  task_queue_sha256 TEXT NOT NULL CHECK(length(CAST(task_queue_sha256 AS BLOB))=64 AND length(task_queue_sha256)=64 AND instr(task_queue_sha256,char(0))=0 AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  trust_generation INTEGER NOT NULL CHECK(typeof(trust_generation)='integer' AND trust_generation BETWEEN 1 AND 9007199254740991),
  channel_sequence INTEGER NOT NULL CHECK(typeof(channel_sequence)='integer' AND channel_sequence BETWEEN 1 AND 9007199254740991),
  member_count INTEGER NOT NULL CHECK(typeof(member_count)='integer' AND member_count BETWEEN 1 AND 9007199254740991),
  canonical_bootstrap_base64url TEXT NOT NULL CHECK(length(CAST(canonical_bootstrap_base64url AS BLOB)) BETWEEN 1 AND 174763 AND length(CAST(canonical_bootstrap_base64url AS BLOB))=length(canonical_bootstrap_base64url) AND instr(canonical_bootstrap_base64url,char(0))=0 AND canonical_bootstrap_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  recorded_at_ms INTEGER NOT NULL CHECK(typeof(recorded_at_ms)='integer' AND recorded_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256),
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE),
  FOREIGN KEY(activation_sha256,manifest_sha256,environment,region,channel) REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(transition_sha256,environment,region,channel,head_revision,activation_sha256,manifest_sha256,trust_generation,channel_sequence) REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT
);
CREATE TRIGGER trg_jobs_mc_bootstrap_header_0_guard BEFORE INSERT ON jobs_managed_cloud_bootstrap_sets
BEGIN SELECT RAISE(ABORT,'managed cloud bootstrap head binding is invalid') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NEW.authority_kind AND a.purpose IS NEW.purpose AND a.context_sha256 IS NEW.context_sha256 AND a.queue_generation_id IS NEW.queue_generation_id AND a.task_queue_sha256 IS NEW.task_queue_sha256); END;
CREATE TRIGGER trg_jobs_mc_bootstrap_header_no_update BEFORE UPDATE ON jobs_managed_cloud_bootstrap_sets
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_bootstrap_header_no_delete BEFORE DELETE ON jobs_managed_cloud_bootstrap_sets
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;
CREATE TABLE jobs_managed_cloud_serving_latches (
  latch_sha256 TEXT NOT NULL CHECK(length(CAST(latch_sha256 AS BLOB))=64 AND length(latch_sha256)=64 AND instr(latch_sha256,char(0))=0 AND latch_sha256 NOT GLOB '*[^0-9a-f]*') PRIMARY KEY,
  latch_id TEXT NOT NULL CHECK(length(latch_id) BETWEEN 20 AND 128 AND length(CAST(latch_id AS BLOB))=length(latch_id) AND latch_id NOT GLOB '*[^A-Za-z0-9_-]*') UNIQUE,
  bootstrap_set_sha256 TEXT NOT NULL CHECK(length(CAST(bootstrap_set_sha256 AS BLOB))=64 AND length(bootstrap_set_sha256)=64 AND instr(bootstrap_set_sha256,char(0))=0 AND bootstrap_set_sha256 NOT GLOB '*[^0-9a-f]*') UNIQUE,
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(length(CAST(region AS BLOB))=length(region) AND instr(region,char(0))=0 AND length(region) BETWEEN 1 AND 64 AND region NOT GLOB '*[^a-z0-9-]*' AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (length(CAST(context_sha256 AS BLOB))=64 AND length(context_sha256)=64 AND instr(context_sha256,char(0))=0 AND context_sha256 NOT GLOB '*[^0-9a-f]*')),
  queue_generation_id TEXT NOT NULL CHECK(length(queue_generation_id) BETWEEN 20 AND 128 AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id) AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  task_queue_sha256 TEXT NOT NULL CHECK(length(CAST(task_queue_sha256 AS BLOB))=64 AND length(task_queue_sha256)=64 AND instr(task_queue_sha256,char(0))=0 AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  trust_generation INTEGER NOT NULL CHECK(typeof(trust_generation)='integer' AND trust_generation BETWEEN 1 AND 9007199254740991),
  channel_sequence INTEGER NOT NULL CHECK(typeof(channel_sequence)='integer' AND channel_sequence BETWEEN 1 AND 9007199254740991),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT CHECK(length(CAST(authorization_signature_set_sha256 AS BLOB))=64 AND length(authorization_signature_set_sha256)=64 AND instr(authorization_signature_set_sha256,char(0))=0 AND authorization_signature_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_latch_base64url TEXT NOT NULL CHECK(length(CAST(canonical_latch_base64url AS BLOB)) BETWEEN 1 AND 174763 AND length(CAST(canonical_latch_base64url AS BLOB))=length(canonical_latch_base64url) AND instr(canonical_latch_base64url,char(0))=0 AND canonical_latch_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  issued_at_ms INTEGER NOT NULL CHECK(typeof(issued_at_ms)='integer' AND issued_at_ms BETWEEN 0 AND 9007199254740991),
  not_before_ms INTEGER NOT NULL CHECK(typeof(not_before_ms)='integer' AND not_before_ms BETWEEN 0 AND 9007199254740991),
  expires_at_ms INTEGER NOT NULL CHECK(typeof(expires_at_ms)='integer' AND expires_at_ms BETWEEN 1 AND 9007199254740991),
  recorded_at_ms INTEGER NOT NULL CHECK(typeof(recorded_at_ms)='integer' AND recorded_at_ms BETWEEN 0 AND 9007199254740991),
  CHECK(not_before_ms>=issued_at_ms AND expires_at_ms>not_before_ms AND recorded_at_ms>=issued_at_ms),
  UNIQUE(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256),
  UNIQUE(activation_sha256,head_revision,transition_sha256),
  FOREIGN KEY(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT,
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE),
  FOREIGN KEY(activation_sha256,manifest_sha256,environment,region,channel) REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(transition_sha256,environment,region,channel,head_revision,activation_sha256,manifest_sha256,trust_generation,channel_sequence) REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT
);
CREATE TRIGGER trg_jobs_mc_serving_latch_no_update BEFORE UPDATE ON jobs_managed_cloud_serving_latches
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_serving_latch_no_delete BEFORE DELETE ON jobs_managed_cloud_serving_latches
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;

-- base: retain every existing legacy row/authority/FK/index and instance immutability.
CREATE TABLE jobs_managed_cloud_runtime_instances_c3 (
  grant_id                        TEXT PRIMARY KEY
    REFERENCES jobs_managed_cloud_runtime_grants(grant_id) ON DELETE RESTRICT,
  runtime_instance_id             TEXT NOT NULL UNIQUE CHECK(
    length(runtime_instance_id) BETWEEN 20 AND 128
    AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  runtime_identity_sha256         TEXT NOT NULL
    CHECK(length(runtime_identity_sha256) = 64 AND lower(runtime_identity_sha256) = runtime_identity_sha256
      AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  worker_id                       TEXT NOT NULL CHECK(
    length(worker_id) BETWEEN 20 AND 128
    AND worker_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  session_proof_hmac_sha256       TEXT NOT NULL UNIQUE
    CHECK(length(session_proof_hmac_sha256) = 64
      AND lower(session_proof_hmac_sha256) = session_proof_hmac_sha256
      AND session_proof_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment                     TEXT NOT NULL,
  region                          TEXT NOT NULL CHECK(
    length(region) BETWEEN 1 AND 64
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region, 1, 1) GLOB '[a-z0-9]'
    AND substr(region, -1, 1) GLOB '[a-z0-9]'
  ),
  channel                         TEXT NOT NULL,
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL,
  role                            TEXT NOT NULL,
  head_revision                   INTEGER NOT NULL,
  transition_sha256               TEXT NOT NULL,
  artifact_sha256                 TEXT NOT NULL,
  config_schema_sha256            TEXT NOT NULL,
  migration_set_sha256            TEXT NOT NULL,
  protocol_set_sha256             TEXT NOT NULL,
  task_queue_sha256               TEXT NOT NULL,
  failure_converter_sha256        TEXT NOT NULL,
  dependency_evidence_sha256      TEXT NOT NULL,
  activation_expires_at_ms        INTEGER NOT NULL CHECK(activation_expires_at_ms > 0),
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  claimed_at_ms                   INTEGER NOT NULL CHECK(claimed_at_ms >= 0),
  protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
  claimed_latch_sha256 TEXT CHECK(claimed_latch_sha256 IS NULL OR (length(CAST(claimed_latch_sha256 AS BLOB))=64 AND length(claimed_latch_sha256)=64 AND instr(claimed_latch_sha256,char(0))=0 AND claimed_latch_sha256 NOT GLOB '*[^0-9a-f]*')),
  UNIQUE(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256, dependency_evidence_sha256
  ),
  UNIQUE(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256, dependency_evidence_sha256,
    activation_expires_at_ms
  ),
  FOREIGN KEY(
    grant_id, environment, region, channel, activation_sha256, manifest_sha256,
    component_id, role, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256,
    dependency_evidence_sha256,
    runtime_identity_sha256, worker_id, activation_expires_at_ms
  ) REFERENCES jobs_managed_cloud_runtime_grants(
    grant_id, environment, region, channel, activation_sha256, manifest_sha256,
    component_id, role, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256,
    expected_dependency_evidence_sha256,
    expected_runtime_identity_sha256, expected_worker_id, activation_expires_at_ms
  ) ON DELETE RESTRICT,
  CHECK((protocol_version=1 AND claimed_latch_sha256 IS NULL) OR protocol_version=2),
  FOREIGN KEY(claimed_latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256)
    REFERENCES jobs_managed_cloud_serving_latches(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_runtime_instances_c3(grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,session_proof_hmac_sha256,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,activation_expires_at_ms,instance_epoch,claimed_at_ms) SELECT grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,session_proof_hmac_sha256,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,activation_expires_at_ms,instance_epoch,claimed_at_ms FROM jobs_managed_cloud_runtime_instances;
-- DB bootstrap also installs these cross-table guards on jobs_execution_leases.
-- SQLite reparses them during RENAME, so preserve both across the parent gap.
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_execution_lease_insert;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_execution_lease_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_instances_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_instances_no_delete;
DROP TABLE jobs_managed_cloud_runtime_instances;
ALTER TABLE jobs_managed_cloud_runtime_instances_c3 RENAME TO jobs_managed_cloud_runtime_instances;
CREATE UNIQUE INDEX IF NOT EXISTS
           idx_jobs_managed_cloud_runtime_instance_epoch_worker
           ON jobs_managed_cloud_runtime_instances(
             runtime_instance_id, instance_epoch, worker_id);
CREATE TRIGGER trg_jobs_managed_cloud_runtime_instances_no_update BEFORE UPDATE ON jobs_managed_cloud_runtime_instances
BEGIN SELECT RAISE(ABORT,'managed cloud runtime instance is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_instances_no_delete BEFORE DELETE ON jobs_managed_cloud_runtime_instances
BEGIN SELECT RAISE(ABORT,'managed cloud runtime instance is immutable'); END;
-- Restore the bootstrap-owned index and exact execution-lease trigger bodies
-- from server/src/db/mod.rs after their ordinary runtime parent exists again.
CREATE UNIQUE INDEX IF NOT EXISTS
           idx_jobs_managed_cloud_runtime_instance_epoch
           ON jobs_managed_cloud_runtime_instances(runtime_instance_id, instance_epoch);
CREATE TRIGGER trg_jobs_managed_cloud_execution_lease_insert
         BEFORE INSERT ON jobs_execution_leases
         WHEN
           ((NEW.managed_cloud_workflow_request_id IS NULL)
             + (NEW.managed_cloud_request_command_id IS NULL)
             + (NEW.managed_cloud_execution_command_id IS NULL)
             + (NEW.managed_cloud_binding_sha256 IS NULL)
             + (NEW.managed_cloud_release_memo_base64url IS NULL)
             + (NEW.managed_cloud_release_sha256 IS NULL)
             + (NEW.managed_cloud_runtime_instance_id IS NULL)
             + (NEW.managed_cloud_runtime_instance_epoch IS NULL)
             + (NEW.managed_cloud_worker_id IS NULL)
             + (NEW.managed_cloud_gateway_authority_base64url IS NULL)
             + (NEW.managed_cloud_gateway_authority_sha256 IS NULL)
             + (NEW.managed_cloud_lease_authority_sha256 IS NULL)) NOT IN (0, 12)
           OR (NEW.managed_cloud_lease_authority_sha256 IS NOT NULL AND (
             NOT EXISTS(
               SELECT 1 FROM jobs_workflow_commands command
                WHERE command.id = NEW.managed_cloud_request_command_id
                  AND command.request_id = NEW.managed_cloud_workflow_request_id
                  AND command.account_id = NEW.account_id
                  AND command.application_id = NEW.application_id
                  AND command.run_id = NEW.run_id
                  AND command.managed_cloud_authority_required = 1
                  AND command.command_kind IN ('start', 'resume')
             )
             OR NOT EXISTS(
               SELECT 1 FROM jobs_managed_cloud_workflow_bindings binding
                WHERE binding.command_id = NEW.managed_cloud_execution_command_id
                  AND binding.binding_sha256 = NEW.managed_cloud_binding_sha256
                  AND binding.release_memo_base64url =
                      NEW.managed_cloud_release_memo_base64url
                  AND binding.release_memo_sha256 = NEW.managed_cloud_release_sha256
             )
             OR NOT EXISTS(
               SELECT 1 FROM jobs_managed_cloud_runtime_instances instance
                WHERE instance.runtime_instance_id =
                      NEW.managed_cloud_runtime_instance_id
                  AND instance.instance_epoch =
                      NEW.managed_cloud_runtime_instance_epoch
                  AND instance.worker_id = NEW.managed_cloud_worker_id
             )
           ))
         BEGIN
           SELECT RAISE(ABORT, 'managed-cloud execution lease authority is invalid');
         END;
CREATE TRIGGER trg_jobs_managed_cloud_execution_lease_update
         BEFORE UPDATE OF fence, phase, managed_cloud_workflow_request_id,
           managed_cloud_request_command_id, managed_cloud_execution_command_id,
           managed_cloud_binding_sha256, managed_cloud_release_memo_base64url,
           managed_cloud_release_sha256, managed_cloud_runtime_instance_id,
           managed_cloud_runtime_instance_epoch, managed_cloud_worker_id,
           managed_cloud_gateway_authority_base64url,
           managed_cloud_gateway_authority_sha256,
           managed_cloud_lease_authority_sha256
         ON jobs_execution_leases
         WHEN
           ((NEW.managed_cloud_workflow_request_id IS NULL)
             + (NEW.managed_cloud_request_command_id IS NULL)
             + (NEW.managed_cloud_execution_command_id IS NULL)
             + (NEW.managed_cloud_binding_sha256 IS NULL)
             + (NEW.managed_cloud_release_memo_base64url IS NULL)
             + (NEW.managed_cloud_release_sha256 IS NULL)
             + (NEW.managed_cloud_runtime_instance_id IS NULL)
             + (NEW.managed_cloud_runtime_instance_epoch IS NULL)
             + (NEW.managed_cloud_worker_id IS NULL)
             + (NEW.managed_cloud_gateway_authority_base64url IS NULL)
             + (NEW.managed_cloud_gateway_authority_sha256 IS NULL)
             + (NEW.managed_cloud_lease_authority_sha256 IS NULL)) NOT IN (0, 12)
           OR (OLD.managed_cloud_lease_authority_sha256 IS NOT NULL
             AND NEW.managed_cloud_lease_authority_sha256 IS NULL)
           OR (OLD.managed_cloud_lease_authority_sha256 IS NOT NULL
             AND NEW.fence <> OLD.fence
             AND NEW.managed_cloud_lease_authority_sha256 =
                 OLD.managed_cloud_lease_authority_sha256)
           OR ((NEW.managed_cloud_workflow_request_id
                  IS NOT OLD.managed_cloud_workflow_request_id
             OR NEW.managed_cloud_request_command_id
                  IS NOT OLD.managed_cloud_request_command_id
             OR NEW.managed_cloud_execution_command_id
                  IS NOT OLD.managed_cloud_execution_command_id
             OR NEW.managed_cloud_binding_sha256
                  IS NOT OLD.managed_cloud_binding_sha256
             OR NEW.managed_cloud_release_memo_base64url
                  IS NOT OLD.managed_cloud_release_memo_base64url
             OR NEW.managed_cloud_release_sha256
                  IS NOT OLD.managed_cloud_release_sha256
             OR NEW.managed_cloud_runtime_instance_id
                  IS NOT OLD.managed_cloud_runtime_instance_id
             OR NEW.managed_cloud_runtime_instance_epoch
                  IS NOT OLD.managed_cloud_runtime_instance_epoch
             OR NEW.managed_cloud_worker_id IS NOT OLD.managed_cloud_worker_id
             OR NEW.managed_cloud_gateway_authority_base64url
                  IS NOT OLD.managed_cloud_gateway_authority_base64url
             OR NEW.managed_cloud_gateway_authority_sha256
                  IS NOT OLD.managed_cloud_gateway_authority_sha256
             OR NEW.managed_cloud_lease_authority_sha256
                  IS NOT OLD.managed_cloud_lease_authority_sha256)
             AND NOT (OLD.phase = 'prepared' AND NEW.phase = 'prepared'
               AND NEW.fence > OLD.fence))
           OR (NEW.managed_cloud_lease_authority_sha256 IS NOT NULL AND (
             NOT EXISTS(
               SELECT 1 FROM jobs_workflow_commands command
                WHERE command.id = NEW.managed_cloud_request_command_id
                  AND command.request_id = NEW.managed_cloud_workflow_request_id
                  AND command.account_id = NEW.account_id
                  AND command.application_id = NEW.application_id
                  AND command.run_id = NEW.run_id
                  AND command.managed_cloud_authority_required = 1
                  AND command.command_kind IN ('start', 'resume')
             )
             OR NOT EXISTS(
               SELECT 1 FROM jobs_managed_cloud_workflow_bindings binding
                WHERE binding.command_id = NEW.managed_cloud_execution_command_id
                  AND binding.binding_sha256 = NEW.managed_cloud_binding_sha256
                  AND binding.release_memo_base64url =
                      NEW.managed_cloud_release_memo_base64url
                  AND binding.release_memo_sha256 = NEW.managed_cloud_release_sha256
             )
             OR NOT EXISTS(
               SELECT 1 FROM jobs_managed_cloud_runtime_instances instance
                WHERE instance.runtime_instance_id =
                      NEW.managed_cloud_runtime_instance_id
                  AND instance.instance_epoch =
                      NEW.managed_cloud_runtime_instance_epoch
                  AND instance.worker_id = NEW.managed_cloud_worker_id
             )
           ))
         BEGIN
           SELECT RAISE(ABORT, 'managed-cloud execution lease authority is frozen');
         END;
CREATE UNIQUE INDEX idx_jobs_mc_base_v2_identity ON jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel);
CREATE TRIGGER trg_jobs_mc_base_claim_kind_0_guard BEFORE INSERT ON jobs_managed_cloud_runtime_instances
BEGIN SELECT RAISE(ABORT,'managed cloud runtime claim protocol kind mismatch') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.claimed_latch_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND NEW.protocol_version=2))); END;

-- osv: retain every existing legacy row/authority/FK/index and instance immutability.
-- Preserve finite cross-table triggers across the temporary parent-table gap.
DROP TRIGGER IF EXISTS trg_jobs_original_source_receipt_binding_validate;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat_insert;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat_update;
CREATE TABLE jobs_managed_cloud_original_source_verifier_runtime_instances_c3 (
  grant_id                        TEXT PRIMARY KEY
    REFERENCES jobs_managed_cloud_original_source_verifier_runtime_grants(grant_id) ON DELETE RESTRICT,
  runtime_instance_id             TEXT NOT NULL UNIQUE CHECK(
    length(runtime_instance_id) BETWEEN 20 AND 128
    AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  runtime_identity_sha256         TEXT NOT NULL
    CHECK(length(runtime_identity_sha256) = 64 AND lower(runtime_identity_sha256) = runtime_identity_sha256
      AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  worker_id                       TEXT NOT NULL CHECK(
    length(worker_id) BETWEEN 20 AND 128
    AND worker_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  session_proof_hmac_sha256       TEXT NOT NULL UNIQUE
    CHECK(length(session_proof_hmac_sha256) = 64
      AND lower(session_proof_hmac_sha256) = session_proof_hmac_sha256
      AND session_proof_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment                     TEXT NOT NULL,
  region                          TEXT NOT NULL CHECK(
    length(region) BETWEEN 1 AND 64
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region, 1, 1) GLOB '[a-z0-9]'
    AND substr(region, -1, 1) GLOB '[a-z0-9]'
  ),
  channel                         TEXT NOT NULL,
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
  head_revision                   INTEGER NOT NULL,
  transition_sha256               TEXT NOT NULL,
  artifact_sha256                 TEXT NOT NULL,
  config_schema_sha256            TEXT NOT NULL,
  migration_set_sha256            TEXT NOT NULL,
  protocol_set_sha256             TEXT NOT NULL,
  task_queue_sha256               TEXT NOT NULL,
  failure_converter_sha256        TEXT NOT NULL,
  dependency_evidence_sha256      TEXT NOT NULL,
  activation_expires_at_ms        INTEGER NOT NULL CHECK(activation_expires_at_ms > 0),
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  claimed_at_ms                   INTEGER NOT NULL CHECK(claimed_at_ms >= 0),
  protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
  claimed_latch_sha256 TEXT CHECK(claimed_latch_sha256 IS NULL OR (length(CAST(claimed_latch_sha256 AS BLOB))=64 AND length(claimed_latch_sha256)=64 AND instr(claimed_latch_sha256,char(0))=0 AND claimed_latch_sha256 NOT GLOB '*[^0-9a-f]*')),
  UNIQUE(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256, dependency_evidence_sha256
  ),
  UNIQUE(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256, dependency_evidence_sha256,
    activation_expires_at_ms
  ),
  FOREIGN KEY(
    grant_id, environment, region, channel, activation_sha256, manifest_sha256,
    component_id, role, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256,
    dependency_evidence_sha256,
    runtime_identity_sha256, worker_id, activation_expires_at_ms
  ) REFERENCES jobs_managed_cloud_original_source_verifier_runtime_grants(
    grant_id, environment, region, channel, activation_sha256, manifest_sha256,
    component_id, role, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256,
    expected_dependency_evidence_sha256,
    expected_runtime_identity_sha256, expected_worker_id, activation_expires_at_ms
  ) ON DELETE RESTRICT,
  CHECK((protocol_version=1 AND claimed_latch_sha256 IS NULL) OR protocol_version=2),
  FOREIGN KEY(claimed_latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256)
    REFERENCES jobs_managed_cloud_serving_latches(latch_sha256,activation_sha256,manifest_sha256,environment,region,channel,head_revision,transition_sha256) ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_original_source_verifier_runtime_instances_c3(grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,session_proof_hmac_sha256,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,activation_expires_at_ms,instance_epoch,claimed_at_ms) SELECT grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,session_proof_hmac_sha256,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,activation_expires_at_ms,instance_epoch,claimed_at_ms FROM jobs_managed_cloud_original_source_verifier_runtime_instances;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_instances_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_instances_no_delete;
DROP TABLE jobs_managed_cloud_original_source_verifier_runtime_instances;
ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_instances_c3 RENAME TO jobs_managed_cloud_original_source_verifier_runtime_instances;
CREATE UNIQUE INDEX idx_jobs_managed_cloud_original_source_verifier_runtime_instance_epoch_worker ON jobs_managed_cloud_original_source_verifier_runtime_instances(runtime_instance_id,instance_epoch,worker_id);
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_instances_no_update BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_instances
BEGIN SELECT RAISE(ABORT,'managed cloud runtime instance is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_instances_no_delete BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_runtime_instances
BEGIN SELECT RAISE(ABORT,'managed cloud runtime instance is immutable'); END;
CREATE UNIQUE INDEX idx_jobs_mc_osv_v2_identity ON jobs_managed_cloud_original_source_verifier_runtime_instances(runtime_instance_id,instance_epoch,grant_id,runtime_identity_sha256,protocol_version,environment,region,channel);
CREATE TRIGGER trg_jobs_original_source_receipt_binding_validate
BEFORE INSERT ON jobs_original_source_verification_receipts
BEGIN
  SELECT CASE WHEN NOT EXISTS(
    SELECT 1 FROM jobs_original_source_verification_assignments assignment
     WHERE assignment.assignment_id=NEW.assignment_id
       AND assignment.account_id=NEW.account_id AND assignment.job_id=NEW.job_id
       AND assignment.subject_sha256=NEW.subject_sha256
       AND assignment.assignment_generation=NEW.assignment_generation
       AND assignment.assignment_sha256=NEW.assignment_sha256
       AND assignment.managed_authority_sha256=NEW.managed_authority_sha256
       AND assignment.canonical_managed_authority_json=NEW.canonical_managed_authority_json
       AND assignment.managed_environment=NEW.managed_environment
       AND assignment.managed_region=NEW.managed_region
       AND assignment.managed_channel=NEW.managed_channel
       AND assignment.managed_head_revision=NEW.managed_head_revision
       AND assignment.managed_transition_sha256=NEW.managed_transition_sha256
       AND assignment.managed_activation_sha256=NEW.managed_activation_sha256
       AND assignment.managed_manifest_sha256=NEW.managed_manifest_sha256
       AND assignment.managed_source_protocol_schema_sha256=
           NEW.managed_source_protocol_schema_sha256
       AND assignment.managed_runtime_identity_sha256=NEW.managed_runtime_identity_sha256
  ) THEN RAISE(ABORT, 'original-source receipt assignment authority mismatch') END;
  SELECT CASE WHEN NOT EXISTS(
    SELECT 1 FROM jobs_original_source_verification_observations observation
     WHERE observation.observation_id=NEW.observation_id
       AND observation.assignment_id=NEW.assignment_id
       AND observation.attempt_id=NEW.attempt_id
       AND observation.fence=NEW.fence
       AND observation.runtime_instance_id=NEW.runtime_instance_id
       AND observation.runtime_instance_epoch=NEW.runtime_instance_epoch
       AND observation.runtime_authority_sha256=NEW.runtime_authority_sha256
       AND observation.worker_runtime_identity_sha256=
           NEW.worker_runtime_identity_sha256
       AND observation.canonical_application_url IS NEW.canonical_application_url
       AND observation.application_domain IS NEW.application_domain
  ) THEN RAISE(ABORT, 'original-source receipt observation authority mismatch') END;
  SELECT CASE WHEN NOT EXISTS(
    SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances runtime
     WHERE runtime.grant_id=NEW.runtime_grant_id
       AND runtime.runtime_instance_id=NEW.runtime_instance_id
       AND runtime.instance_epoch=NEW.runtime_instance_epoch
       AND runtime.runtime_identity_sha256=NEW.managed_runtime_identity_sha256
       AND runtime.environment=NEW.managed_environment
       AND runtime.region=NEW.managed_region AND runtime.channel=NEW.managed_channel
       AND runtime.activation_sha256=NEW.managed_activation_sha256
       AND runtime.manifest_sha256=NEW.managed_manifest_sha256
       AND runtime.head_revision=NEW.managed_head_revision
       AND runtime.transition_sha256=NEW.managed_transition_sha256
  ) THEN RAISE(ABORT, 'original-source receipt runtime authority mismatch') END;
END;
CREATE TRIGGER trg_jobs_osv_boot_heartbeat_insert BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=1 AND EXISTS (SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_manifest_source_verifier_boot_protocols p ON p.manifest_sha256=i.manifest_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id)
 AND NOT EXISTS (SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND h.current_boot_generation=b.boot_generation)
BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat requires current boot binding'); END;
CREATE TRIGGER trg_jobs_osv_boot_heartbeat_update BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=1 AND EXISTS (SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_manifest_source_verifier_boot_protocols p ON p.manifest_sha256=i.manifest_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id)
 AND NOT EXISTS (SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND h.current_boot_generation=b.boot_generation)
BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat requires current boot binding'); END;
CREATE TRIGGER trg_jobs_mc_osv_claim_kind_0_guard BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_instances
BEGIN SELECT RAISE(ABORT,'managed cloud runtime claim protocol kind mismatch') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.claimed_latch_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND NEW.protocol_version=2))); END;
CREATE UNIQUE INDEX idx_jobs_mc_base_contamination_identity ON jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,worker_id,activation_sha256,manifest_sha256,head_revision,transition_sha256,role);

-- Boot registration is the sole observation parent; startup is a receipt child.
-- No observation -> startup FK. V2 stores only dependency_ready success; an
-- unavailable probe has NO startup row/observation/latest mutation.
-- Remove only the finite cross-table triggers during the table gap; restore
-- exact legacy bodies below. Do not use writable_schema/legacy_alter_table.
DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_startup_binding;
DROP TRIGGER IF EXISTS trg_jobs_osv_attempt_boot_binding_current;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_binding_current;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat_transition;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat_transition_update;
CREATE TABLE jobs_managed_cloud_source_verifier_boot_registrations_c3 (
  boot_id TEXT PRIMARY KEY,
  registration_request_id TEXT NOT NULL,
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  boot_generation                 INTEGER NOT NULL
    CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  worker_id                       TEXT NOT NULL,
  runtime_identity_sha256         TEXT NOT NULL
    CHECK(length(runtime_identity_sha256) = 64
      AND lower(runtime_identity_sha256) = runtime_identity_sha256
      AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
  registration_request_sha256     TEXT NOT NULL
    CHECK(length(registration_request_sha256) = 64
      AND lower(registration_request_sha256) = registration_request_sha256
      AND registration_request_sha256 NOT GLOB '*[^0-9a-f]*'),
  registration_authority_sha256   TEXT NOT NULL UNIQUE
    CHECK(length(registration_authority_sha256) = 64
      AND lower(registration_authority_sha256) = registration_authority_sha256
      AND registration_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_response_base64url    TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256                 TEXT NOT NULL UNIQUE
    CHECK(length(response_sha256) = 64 AND lower(response_sha256) = response_sha256
      AND response_sha256 NOT GLOB '*[^0-9a-f]*'),
  registered_at_ms                INTEGER NOT NULL
    CHECK(registered_at_ms BETWEEN 0 AND 9007199254740991),
  protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
  CHECK(((protocol_version=1 AND length(boot_id) BETWEEN 20 AND 128 AND length(CAST(boot_id AS BLOB))=length(boot_id) AND boot_id NOT GLOB '*[^A-Za-z0-9_-]*' AND length(registration_request_id) BETWEEN 20 AND 128 AND length(CAST(registration_request_id AS BLOB))=length(registration_request_id) AND registration_request_id NOT GLOB '*[^A-Za-z0-9_-]*') OR (protocol_version=2 AND boot_id IS NOT NULL AND registration_request_id IS NOT NULL AND length(boot_id) BETWEEN 20 AND 128 AND length(CAST(boot_id AS BLOB))=length(boot_id) AND boot_id NOT GLOB '*[^A-Za-z0-9_.:/-]*' AND length(registration_request_id) BETWEEN 20 AND 128 AND length(CAST(registration_request_id AS BLOB))=length(registration_request_id) AND registration_request_id NOT GLOB '*[^A-Za-z0-9_.:/-]*' AND length(CAST(runtime_identity_sha256 AS BLOB))=64 AND length(runtime_identity_sha256)=64 AND instr(runtime_identity_sha256,char(0))=0 AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(registration_request_sha256 AS BLOB))=64 AND length(registration_request_sha256)=64 AND instr(registration_request_sha256,char(0))=0 AND registration_request_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(registration_authority_sha256 AS BLOB))=64 AND length(registration_authority_sha256)=64 AND instr(registration_authority_sha256,char(0))=0 AND registration_authority_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(response_sha256 AS BLOB))=64 AND length(response_sha256)=64 AND instr(response_sha256,char(0))=0 AND response_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(canonical_response_base64url AS BLOB)) BETWEEN 1 AND 87382 AND length(CAST(canonical_response_base64url AS BLOB))=length(canonical_response_base64url) AND instr(canonical_response_base64url,char(0))=0 AND canonical_response_base64url NOT GLOB '*[^A-Za-z0-9_-]*')) IS TRUE),
  UNIQUE(runtime_instance_id, instance_epoch, boot_generation),
  UNIQUE(registration_request_id, registration_request_sha256),
  FOREIGN KEY(runtime_instance_id, instance_epoch)
    REFERENCES jobs_managed_cloud_source_verifier_boot_generation_heads(
      runtime_instance_id, instance_epoch
    ) ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_source_verifier_boot_registrations_c3(boot_id,registration_request_id,runtime_instance_id,instance_epoch,boot_generation,worker_id,runtime_identity_sha256,activation_sha256,manifest_sha256,component_id,role,registration_request_sha256,registration_authority_sha256,canonical_response_base64url,response_sha256,registered_at_ms) SELECT boot_id,registration_request_id,runtime_instance_id,instance_epoch,boot_generation,worker_id,runtime_identity_sha256,activation_sha256,manifest_sha256,component_id,role,registration_request_sha256,registration_authority_sha256,canonical_response_base64url,response_sha256,registered_at_ms FROM jobs_managed_cloud_source_verifier_boot_registrations;
DROP TABLE jobs_managed_cloud_source_verifier_boot_registrations;
ALTER TABLE jobs_managed_cloud_source_verifier_boot_registrations_c3 RENAME TO jobs_managed_cloud_source_verifier_boot_registrations;
CREATE INDEX idx_jobs_managed_source_verifier_boot_registrations_instance ON jobs_managed_cloud_source_verifier_boot_registrations(runtime_instance_id,instance_epoch,boot_generation DESC);
CREATE UNIQUE INDEX idx_jobs_mc_boot_registration_v2 ON jobs_managed_cloud_source_verifier_boot_registrations(runtime_instance_id,instance_epoch,boot_id,boot_generation,protocol_version);
CREATE TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities_c3 (
  startup_request_id TEXT PRIMARY KEY,
  boot_id                         TEXT NOT NULL UNIQUE,
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  boot_generation                 INTEGER NOT NULL
    CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  worker_id                       TEXT NOT NULL,
  request_sha256                  TEXT NOT NULL
    CHECK(length(request_sha256) = 64 AND lower(request_sha256) = request_sha256
      AND request_sha256 NOT GLOB '*[^0-9a-f]*'),
  degraded_heartbeat_sequence     INTEGER
    CHECK(degraded_heartbeat_sequence BETWEEN 1 AND 9007199254740990),
  ready_heartbeat_sequence        INTEGER
    CHECK(ready_heartbeat_sequence = degraded_heartbeat_sequence + 1),
  probe_result                    TEXT CHECK(probe_result IN ('idle', 'leaseable')),
  startup_authority_sha256        TEXT UNIQUE
    CHECK(length(startup_authority_sha256) = 64
      AND lower(startup_authority_sha256) = startup_authority_sha256
      AND startup_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_response_base64url    TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256                 TEXT NOT NULL UNIQUE
    CHECK(length(response_sha256) = 64 AND lower(response_sha256) = response_sha256
      AND response_sha256 NOT GLOB '*[^0-9a-f]*'),
  startup_at_ms                   INTEGER NOT NULL
    CHECK(startup_at_ms BETWEEN 0 AND 9007199254740991),
  protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)),
  prior_heartbeat_sequence INTEGER,
  parked_heartbeat_sequence INTEGER,
  dependency_result TEXT,
  observation_sha256 TEXT CHECK(observation_sha256 IS NULL OR (length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*')),
  CHECK(((protocol_version=1 AND length(startup_request_id) BETWEEN 20 AND 128 AND length(CAST(startup_request_id AS BLOB))=length(startup_request_id) AND startup_request_id NOT GLOB '*[^A-Za-z0-9_-]*' AND degraded_heartbeat_sequence IS NOT NULL AND ready_heartbeat_sequence IS NOT NULL AND probe_result IS NOT NULL AND startup_authority_sha256 IS NOT NULL AND prior_heartbeat_sequence IS NULL AND parked_heartbeat_sequence IS NULL AND dependency_result IS NULL AND observation_sha256 IS NULL) OR (protocol_version=2 AND startup_request_id IS NOT NULL AND length(startup_request_id) BETWEEN 20 AND 128 AND length(CAST(startup_request_id AS BLOB))=length(startup_request_id) AND startup_request_id NOT GLOB '*[^A-Za-z0-9_.:/-]*' AND degraded_heartbeat_sequence IS NULL AND ready_heartbeat_sequence IS NULL AND probe_result IS NULL AND startup_authority_sha256 IS NULL AND typeof(prior_heartbeat_sequence)='integer' AND typeof(parked_heartbeat_sequence)='integer' AND typeof(startup_at_ms)='integer' AND prior_heartbeat_sequence BETWEEN 1 AND 9007199254740990 AND parked_heartbeat_sequence=prior_heartbeat_sequence+1 AND dependency_result='dependency_ready' AND observation_sha256 IS NOT NULL AND length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(request_sha256 AS BLOB))=64 AND length(request_sha256)=64 AND instr(request_sha256,char(0))=0 AND request_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(response_sha256 AS BLOB))=64 AND length(response_sha256)=64 AND instr(response_sha256,char(0))=0 AND response_sha256 NOT GLOB '*[^0-9a-f]*' AND length(CAST(canonical_response_base64url AS BLOB)) BETWEEN 1 AND 87382 AND length(CAST(canonical_response_base64url AS BLOB))=length(canonical_response_base64url) AND instr(canonical_response_base64url,char(0))=0 AND canonical_response_base64url NOT GLOB '*[^A-Za-z0-9_-]*')) IS TRUE),
  FOREIGN KEY(runtime_instance_id,instance_epoch,parked_heartbeat_sequence,observation_sha256) REFERENCES jobs_managed_cloud_osv_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256) ON DELETE RESTRICT,
  UNIQUE(runtime_instance_id, instance_epoch, boot_generation),
  UNIQUE(startup_request_id, request_sha256),
  FOREIGN KEY(boot_id)
    REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id)
    ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities_c3(startup_request_id,boot_id,runtime_instance_id,instance_epoch,boot_generation,worker_id,request_sha256,degraded_heartbeat_sequence,ready_heartbeat_sequence,probe_result,startup_authority_sha256,canonical_response_base64url,response_sha256,startup_at_ms) SELECT startup_request_id,boot_id,runtime_instance_id,instance_epoch,boot_generation,worker_id,request_sha256,degraded_heartbeat_sequence,ready_heartbeat_sequence,probe_result,startup_authority_sha256,canonical_response_base64url,response_sha256,startup_at_ms FROM jobs_managed_cloud_source_verifier_boot_startup_authorities;
DROP TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities;
ALTER TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities_c3 RENAME TO jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_registration_no_update
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_registrations BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot registration is immutable');
END;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_registration_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_registrations BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot registration is immutable');
END;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_registration_generation
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_registrations
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
   WHERE head.runtime_instance_id = NEW.runtime_instance_id
     AND head.instance_epoch = NEW.instance_epoch
     AND head.current_boot_generation = NEW.boot_generation
) BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot registration generation is not the assigned head');
END;
CREATE TRIGGER trg_jobs_osv_attempt_boot_binding_current
BEFORE INSERT ON jobs_original_source_verification_attempt_boot_bindings
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_original_source_verification_attempts attempt
  JOIN jobs_managed_cloud_source_verifier_boot_registrations registration
    ON registration.boot_id=NEW.boot_id
  JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head
    ON head.runtime_instance_id=registration.runtime_instance_id
   AND head.instance_epoch=registration.instance_epoch
  WHERE registration.protocol_version=1
    AND attempt.attempt_id=NEW.attempt_id
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
CREATE TRIGGER trg_jobs_osv_boot_binding_current BEFORE INSERT ON jobs_managed_cloud_source_verifier_heartbeat_boot_bindings
WHEN NOT EXISTS (SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch WHERE r.protocol_version=1 AND r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND h.current_boot_generation=NEW.boot_generation)
BEGIN SELECT RAISE(ABORT,'source-verifier heartbeat boot binding is stale'); END;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_no_update
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_startup_authorities BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup authority is immutable');
END;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_startup_authorities BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup authority is immutable');
END;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_binding
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations registration
   WHERE registration.protocol_version = NEW.protocol_version
     AND registration.boot_id = NEW.boot_id
     AND registration.runtime_instance_id = NEW.runtime_instance_id
     AND registration.instance_epoch = NEW.instance_epoch
     AND registration.boot_generation = NEW.boot_generation
     AND registration.worker_id = NEW.worker_id
) BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup binding is invalid');
END;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_not_stale
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
WHEN EXISTS (
  SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
   WHERE head.runtime_instance_id = NEW.runtime_instance_id
     AND head.instance_epoch = NEW.instance_epoch
     AND head.ready_boot_generation IS NOT NULL
     AND head.ready_boot_generation > NEW.boot_generation
) BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup generation is stale');
END;

-- Permanent base facts. Canonical blobs MUST come from the closed nonsecret
-- Rust serializers; SQL does not decode base64, verify the digest, or prove work.
CREATE TABLE jobs_managed_cloud_runtime_observations (
  runtime_instance_id TEXT NOT NULL CHECK(length(runtime_instance_id) BETWEEN 20 AND 128 AND length(CAST(runtime_instance_id AS BLOB))=length(runtime_instance_id) AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  instance_epoch INTEGER NOT NULL CHECK(typeof(instance_epoch)='integer' AND instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence INTEGER NOT NULL CHECK(typeof(heartbeat_sequence)='integer' AND heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*') UNIQUE,
  grant_id TEXT NOT NULL CHECK(length(grant_id) BETWEEN 20 AND 128 AND length(CAST(grant_id AS BLOB))=length(grant_id) AND grant_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  runtime_identity_sha256 TEXT NOT NULL CHECK(length(CAST(runtime_identity_sha256 AS BLOB))=64 AND length(runtime_identity_sha256)=64 AND instr(runtime_identity_sha256,char(0))=0 AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_version INTEGER NOT NULL DEFAULT 2 CHECK(protocol_version=2),
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(length(CAST(region AS BLOB))=length(region) AND instr(region,char(0))=0 AND length(region) BETWEEN 1 AND 64 AND region NOT GLOB '*[^a-z0-9-]*' AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (length(CAST(context_sha256 AS BLOB))=64 AND length(context_sha256)=64 AND instr(context_sha256,char(0))=0 AND context_sha256 NOT GLOB '*[^0-9a-f]*')),
  queue_generation_id TEXT NOT NULL CHECK(length(queue_generation_id) BETWEEN 20 AND 128 AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id) AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  task_queue_sha256 TEXT NOT NULL CHECK(length(CAST(task_queue_sha256 AS BLOB))=64 AND length(task_queue_sha256)=64 AND instr(task_queue_sha256,char(0))=0 AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  component_id TEXT NOT NULL CHECK(component_id IN ('jobs-api','jobs-runner','jobs-workflows')),
  role TEXT NOT NULL CHECK(role IN ('discovery_worker','global_discovery_worker','jobs_api','managed_runner','workflow_command_dispatcher','workflow_cleanup_dispatcher','workflow_gateway','workflow_worker')),
  worker_id TEXT NOT NULL CHECK(length(worker_id) BETWEEN 20 AND 128 AND length(CAST(worker_id AS BLOB))=length(worker_id) AND worker_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  artifact_sha256 TEXT NOT NULL CHECK(length(CAST(artifact_sha256 AS BLOB))=64 AND length(artifact_sha256)=64 AND instr(artifact_sha256,char(0))=0 AND artifact_sha256 NOT GLOB '*[^0-9a-f]*'),
  config_schema_sha256 TEXT NOT NULL CHECK(length(CAST(config_schema_sha256 AS BLOB))=64 AND length(config_schema_sha256)=64 AND instr(config_schema_sha256,char(0))=0 AND config_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  migration_set_sha256 TEXT NOT NULL CHECK(length(CAST(migration_set_sha256 AS BLOB))=64 AND length(migration_set_sha256)=64 AND instr(migration_set_sha256,char(0))=0 AND migration_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_set_sha256 TEXT NOT NULL CHECK(length(CAST(protocol_set_sha256 AS BLOB))=64 AND length(protocol_set_sha256)=64 AND instr(protocol_set_sha256,char(0))=0 AND protocol_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  failure_converter_sha256 TEXT NOT NULL CHECK(length(CAST(failure_converter_sha256 AS BLOB))=64 AND length(failure_converter_sha256)=64 AND instr(failure_converter_sha256,char(0))=0 AND failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(length(CAST(dependency_evidence_sha256 AS BLOB))=64 AND length(dependency_evidence_sha256)=64 AND instr(dependency_evidence_sha256,char(0))=0 AND dependency_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN ('parked','latch_acknowledged','operational')),
  health_state TEXT NOT NULL CHECK(health_state IN ('ready','degraded','draining')),
  reason_code TEXT CHECK(reason_code IS NULL OR reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','draining','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup')),
  acknowledged_latch_sha256 TEXT CHECK(acknowledged_latch_sha256 IS NULL OR (length(CAST(acknowledged_latch_sha256 AS BLOB))=64 AND length(acknowledged_latch_sha256)=64 AND instr(acknowledged_latch_sha256,char(0))=0 AND acknowledged_latch_sha256 NOT GLOB '*[^0-9a-f]*')),
  serving_latch_sha256 TEXT CHECK(serving_latch_sha256 IS NULL OR (length(CAST(serving_latch_sha256 AS BLOB))=64 AND length(serving_latch_sha256)=64 AND instr(serving_latch_sha256,char(0))=0 AND serving_latch_sha256 NOT GLOB '*[^0-9a-f]*')),
  canonical_heartbeat_base64url TEXT NOT NULL CHECK(length(CAST(canonical_heartbeat_base64url AS BLOB)) BETWEEN 1 AND 87382 AND length(CAST(canonical_heartbeat_base64url AS BLOB))=length(canonical_heartbeat_base64url) AND instr(canonical_heartbeat_base64url,char(0))=0 AND canonical_heartbeat_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  dependency_probe_base64url TEXT NOT NULL CHECK(length(CAST(dependency_probe_base64url AS BLOB)) BETWEEN 1 AND 87382 AND length(CAST(dependency_probe_base64url AS BLOB))=length(dependency_probe_base64url) AND instr(dependency_probe_base64url,char(0))=0 AND dependency_probe_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  poller_identity_sha256 TEXT CHECK(poller_identity_sha256 IS NULL OR (length(CAST(poller_identity_sha256 AS BLOB))=64 AND length(poller_identity_sha256)=64 AND instr(poller_identity_sha256,char(0))=0 AND poller_identity_sha256 NOT GLOB '*[^0-9a-f]*')),
  backlog_count INTEGER CHECK(backlog_count IS NULL OR (typeof(backlog_count)='integer' AND backlog_count BETWEEN 0 AND 9007199254740991)),
  delivered_task_count INTEGER CHECK(delivered_task_count IS NULL OR (typeof(delivered_task_count)='integer' AND delivered_task_count BETWEEN 0 AND 9007199254740991)),
  parked_predecessor_sequence INTEGER CHECK(parked_predecessor_sequence IS NULL OR (typeof(parked_predecessor_sequence)='integer' AND parked_predecessor_sequence BETWEEN 1 AND 9007199254740991)),
  parked_predecessor_sha256 TEXT CHECK(parked_predecessor_sha256 IS NULL OR (length(CAST(parked_predecessor_sha256 AS BLOB))=64 AND length(parked_predecessor_sha256)=64 AND instr(parked_predecessor_sha256,char(0))=0 AND parked_predecessor_sha256 NOT GLOB '*[^0-9a-f]*')),
  heartbeat_at_ms INTEGER NOT NULL CHECK(typeof(heartbeat_at_ms)='integer' AND heartbeat_at_ms BETWEEN 0 AND 9007199254740991),
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
-- The observation guard reads the prior latest observation before latest CAS.
ALTER TABLE jobs_managed_cloud_runtime_heartbeats ADD COLUMN observation_sha256 TEXT REFERENCES jobs_managed_cloud_runtime_observations(observation_sha256) ON DELETE RESTRICT CHECK(observation_sha256 IS NULL OR (length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*'));
CREATE TRIGGER trg_jobs_mc_base_observation_0_guard BEFORE INSERT ON jobs_managed_cloud_runtime_observations
BEGIN SELECT RAISE(ABORT,'managed cloud observation identity or predecessor is invalid') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NEW.authority_kind AND a.purpose IS NEW.purpose AND a.context_sha256 IS NEW.context_sha256 AND a.queue_generation_id IS NEW.queue_generation_id AND a.task_queue_sha256 IS NEW.task_queue_sha256) OR (NEW.parked_predecessor_sequence IS NOT NULL AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations p WHERE p.runtime_instance_id=NEW.runtime_instance_id AND p.instance_epoch=NEW.instance_epoch AND p.heartbeat_sequence=NEW.parked_predecessor_sequence AND p.observation_sha256=NEW.parked_predecessor_sha256 AND p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup' AND p.activation_sha256=NEW.activation_sha256 AND p.manifest_sha256=NEW.manifest_sha256 AND p.head_revision=NEW.head_revision AND p.transition_sha256=NEW.transition_sha256 AND p.authority_kind IS NEW.authority_kind AND p.purpose IS NEW.purpose AND p.context_sha256 IS NEW.context_sha256 AND p.queue_generation_id IS NEW.queue_generation_id AND p.task_queue_sha256 IS NEW.task_queue_sha256)) OR (NEW.lifecycle_state='operational' AND NEW.health_state='ready' AND NOT EXISTS(
    -- The observation is inserted BEFORE latest CAS: h must still be the immediate
    -- previous fact. A negative or latch_acknowledged fact breaks the parked chain.
    SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats h
    JOIN jobs_managed_cloud_runtime_observations p
      ON p.runtime_instance_id=h.runtime_instance_id AND p.instance_epoch=h.instance_epoch
      AND p.heartbeat_sequence=h.heartbeat_sequence AND p.observation_sha256=h.observation_sha256
    WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch
      AND h.protocol_version=2 AND p.heartbeat_sequence=NEW.heartbeat_sequence-1
      AND p.activation_sha256 IS NEW.activation_sha256
      AND p.manifest_sha256 IS NEW.manifest_sha256
      AND p.head_revision IS NEW.head_revision
      AND p.transition_sha256 IS NEW.transition_sha256
      AND p.authority_kind IS NEW.authority_kind
      AND p.purpose IS NEW.purpose
      AND p.context_sha256 IS NEW.context_sha256
      AND p.queue_generation_id IS NEW.queue_generation_id
      AND p.task_queue_sha256 IS NEW.task_queue_sha256
      AND ((p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup'
        AND p.heartbeat_sequence=NEW.parked_predecessor_sequence
        AND p.observation_sha256=NEW.parked_predecessor_sha256)
       OR (p.lifecycle_state='operational' AND p.health_state='ready' AND p.reason_code IS NULL
        AND p.serving_latch_sha256=NEW.serving_latch_sha256
        AND p.acknowledged_latch_sha256=NEW.acknowledged_latch_sha256
        AND p.parked_predecessor_sequence=NEW.parked_predecessor_sequence
        AND p.parked_predecessor_sha256=NEW.parked_predecessor_sha256))))
 OR NEW.heartbeat_sequence<>COALESCE((SELECT h.heartbeat_sequence+1 FROM jobs_managed_cloud_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch),1) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch AND h.heartbeat_at_ms>NEW.heartbeat_at_ms) OR NEW.heartbeat_at_ms>CAST(strftime('%s','now') AS INTEGER)*1000 OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND i.claimed_at_ms<=NEW.heartbeat_at_ms); END;
CREATE TRIGGER trg_jobs_mc_base_observation_no_update BEFORE UPDATE ON jobs_managed_cloud_runtime_observations
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_base_observation_no_delete BEFORE DELETE ON jobs_managed_cloud_runtime_observations
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;
CREATE TRIGGER trg_jobs_mc_base_latest_observation_0_guard BEFORE INSERT ON jobs_managed_cloud_runtime_heartbeats
BEGIN SELECT RAISE(ABORT,'managed cloud latest heartbeat observation does not match') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.observation_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND i.protocol_version=2 AND NEW.protocol_version=2 AND NEW.observation_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations o WHERE o.runtime_instance_id IS NEW.runtime_instance_id AND o.instance_epoch IS NEW.instance_epoch AND o.heartbeat_sequence IS NEW.heartbeat_sequence AND o.activation_sha256 IS NEW.activation_sha256 AND o.manifest_sha256 IS NEW.manifest_sha256 AND o.component_id IS NEW.component_id AND o.role IS NEW.role AND o.worker_id IS NEW.worker_id AND o.artifact_sha256 IS NEW.artifact_sha256 AND o.migration_set_sha256 IS NEW.migration_set_sha256 AND o.config_schema_sha256 IS NEW.config_schema_sha256 AND o.protocol_set_sha256 IS NEW.protocol_set_sha256 AND o.task_queue_sha256 IS NEW.task_queue_sha256 AND o.failure_converter_sha256 IS NEW.failure_converter_sha256 AND o.dependency_evidence_sha256 IS NEW.dependency_evidence_sha256 AND o.health_state IS NEW.health_state AND o.reason_code IS NEW.reason_code AND o.heartbeat_at_ms IS NEW.heartbeat_at_ms AND o.head_revision=NEW.observed_head_revision AND o.transition_sha256=NEW.observed_transition_sha256 AND o.protocol_version=NEW.protocol_version AND o.observation_sha256=NEW.observation_sha256)))); END;
CREATE TRIGGER trg_jobs_mc_base_latest_observation_1_guard BEFORE UPDATE ON jobs_managed_cloud_runtime_heartbeats
BEGIN SELECT RAISE(ABORT,'managed cloud latest heartbeat observation does not match') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.observation_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND i.protocol_version=2 AND NEW.protocol_version=2 AND NEW.observation_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations o WHERE o.runtime_instance_id IS NEW.runtime_instance_id AND o.instance_epoch IS NEW.instance_epoch AND o.heartbeat_sequence IS NEW.heartbeat_sequence AND o.activation_sha256 IS NEW.activation_sha256 AND o.manifest_sha256 IS NEW.manifest_sha256 AND o.component_id IS NEW.component_id AND o.role IS NEW.role AND o.worker_id IS NEW.worker_id AND o.artifact_sha256 IS NEW.artifact_sha256 AND o.migration_set_sha256 IS NEW.migration_set_sha256 AND o.config_schema_sha256 IS NEW.config_schema_sha256 AND o.protocol_set_sha256 IS NEW.protocol_set_sha256 AND o.task_queue_sha256 IS NEW.task_queue_sha256 AND o.failure_converter_sha256 IS NEW.failure_converter_sha256 AND o.dependency_evidence_sha256 IS NEW.dependency_evidence_sha256 AND o.health_state IS NEW.health_state AND o.reason_code IS NEW.reason_code AND o.heartbeat_at_ms IS NEW.heartbeat_at_ms AND o.head_revision=NEW.observed_head_revision AND o.transition_sha256=NEW.observed_transition_sha256 AND o.protocol_version=NEW.protocol_version AND o.observation_sha256=NEW.observation_sha256)))); END;
CREATE TABLE jobs_managed_cloud_bootstrap_runtime_members (
  bootstrap_set_sha256 TEXT NOT NULL CHECK(length(CAST(bootstrap_set_sha256 AS BLOB))=64 AND length(bootstrap_set_sha256)=64 AND instr(bootstrap_set_sha256,char(0))=0 AND bootstrap_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_instance_id TEXT NOT NULL CHECK(length(runtime_instance_id) BETWEEN 20 AND 128 AND length(CAST(runtime_instance_id AS BLOB))=length(runtime_instance_id) AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  instance_epoch INTEGER NOT NULL CHECK(typeof(instance_epoch)='integer' AND instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence INTEGER NOT NULL CHECK(typeof(heartbeat_sequence)='integer' AND heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*'),
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  PRIMARY KEY(bootstrap_set_sha256,runtime_instance_id,instance_epoch),
  FOREIGN KEY(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT
);
CREATE TRIGGER trg_jobs_mc_base_bootstrap_member_0_guard BEFORE INSERT ON jobs_managed_cloud_bootstrap_runtime_members
BEGIN SELECT RAISE(ABORT,'managed cloud bootstrap member is not an unsealed parked fact') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_observations o JOIN jobs_managed_cloud_bootstrap_sets b ON b.bootstrap_set_sha256=NEW.bootstrap_set_sha256 WHERE o.runtime_instance_id=NEW.runtime_instance_id AND o.instance_epoch=NEW.instance_epoch AND o.heartbeat_sequence=NEW.heartbeat_sequence AND o.observation_sha256=NEW.observation_sha256 AND o.lifecycle_state='parked' AND o.health_state='degraded' AND o.reason_code='startup' AND o.authority_kind IS b.authority_kind AND o.purpose IS b.purpose AND o.context_sha256 IS b.context_sha256 AND o.queue_generation_id IS b.queue_generation_id AND o.task_queue_sha256 IS b.task_queue_sha256) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_serving_latches l WHERE l.bootstrap_set_sha256=NEW.bootstrap_set_sha256); END;
CREATE TRIGGER trg_jobs_mc_base_bootstrap_member_no_update BEFORE UPDATE ON jobs_managed_cloud_bootstrap_runtime_members
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_base_bootstrap_member_no_delete BEFORE DELETE ON jobs_managed_cloud_bootstrap_runtime_members
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;

-- Permanent osv facts. Canonical blobs MUST come from the closed nonsecret
-- Rust serializers; SQL does not decode base64, verify the digest, or prove work.
CREATE TABLE jobs_managed_cloud_osv_runtime_observations (
  runtime_instance_id TEXT NOT NULL CHECK(length(runtime_instance_id) BETWEEN 20 AND 128 AND length(CAST(runtime_instance_id AS BLOB))=length(runtime_instance_id) AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  instance_epoch INTEGER NOT NULL CHECK(typeof(instance_epoch)='integer' AND instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence INTEGER NOT NULL CHECK(typeof(heartbeat_sequence)='integer' AND heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*') UNIQUE,
  grant_id TEXT NOT NULL CHECK(length(grant_id) BETWEEN 20 AND 128 AND length(CAST(grant_id AS BLOB))=length(grant_id) AND grant_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  runtime_identity_sha256 TEXT NOT NULL CHECK(length(CAST(runtime_identity_sha256 AS BLOB))=64 AND length(runtime_identity_sha256)=64 AND instr(runtime_identity_sha256,char(0))=0 AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_version INTEGER NOT NULL DEFAULT 2 CHECK(protocol_version=2),
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(length(CAST(region AS BLOB))=length(region) AND instr(region,char(0))=0 AND length(region) BETWEEN 1 AND 64 AND region NOT GLOB '*[^a-z0-9-]*' AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (length(CAST(context_sha256 AS BLOB))=64 AND length(context_sha256)=64 AND instr(context_sha256,char(0))=0 AND context_sha256 NOT GLOB '*[^0-9a-f]*')),
  queue_generation_id TEXT NOT NULL CHECK(length(queue_generation_id) BETWEEN 20 AND 128 AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id) AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  task_queue_sha256 TEXT NOT NULL CHECK(length(CAST(task_queue_sha256 AS BLOB))=64 AND length(task_queue_sha256)=64 AND instr(task_queue_sha256,char(0))=0 AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  component_id TEXT NOT NULL CHECK(component_id IN ('jobs-api','jobs-runner','jobs-workflows')),
  role TEXT NOT NULL CHECK(role='original_source_verifier'),
  worker_id TEXT NOT NULL CHECK(length(worker_id) BETWEEN 20 AND 128 AND length(CAST(worker_id AS BLOB))=length(worker_id) AND worker_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  artifact_sha256 TEXT NOT NULL CHECK(length(CAST(artifact_sha256 AS BLOB))=64 AND length(artifact_sha256)=64 AND instr(artifact_sha256,char(0))=0 AND artifact_sha256 NOT GLOB '*[^0-9a-f]*'),
  config_schema_sha256 TEXT NOT NULL CHECK(length(CAST(config_schema_sha256 AS BLOB))=64 AND length(config_schema_sha256)=64 AND instr(config_schema_sha256,char(0))=0 AND config_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  migration_set_sha256 TEXT NOT NULL CHECK(length(CAST(migration_set_sha256 AS BLOB))=64 AND length(migration_set_sha256)=64 AND instr(migration_set_sha256,char(0))=0 AND migration_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_set_sha256 TEXT NOT NULL CHECK(length(CAST(protocol_set_sha256 AS BLOB))=64 AND length(protocol_set_sha256)=64 AND instr(protocol_set_sha256,char(0))=0 AND protocol_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  failure_converter_sha256 TEXT NOT NULL CHECK(length(CAST(failure_converter_sha256 AS BLOB))=64 AND length(failure_converter_sha256)=64 AND instr(failure_converter_sha256,char(0))=0 AND failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(length(CAST(dependency_evidence_sha256 AS BLOB))=64 AND length(dependency_evidence_sha256)=64 AND instr(dependency_evidence_sha256,char(0))=0 AND dependency_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  lifecycle_state TEXT NOT NULL CHECK(lifecycle_state IN ('parked','latch_acknowledged','operational')),
  health_state TEXT NOT NULL CHECK(health_state IN ('ready','degraded','draining')),
  reason_code TEXT CHECK(reason_code IS NULL OR reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','draining','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup')),
  acknowledged_latch_sha256 TEXT CHECK(acknowledged_latch_sha256 IS NULL OR (length(CAST(acknowledged_latch_sha256 AS BLOB))=64 AND length(acknowledged_latch_sha256)=64 AND instr(acknowledged_latch_sha256,char(0))=0 AND acknowledged_latch_sha256 NOT GLOB '*[^0-9a-f]*')),
  serving_latch_sha256 TEXT CHECK(serving_latch_sha256 IS NULL OR (length(CAST(serving_latch_sha256 AS BLOB))=64 AND length(serving_latch_sha256)=64 AND instr(serving_latch_sha256,char(0))=0 AND serving_latch_sha256 NOT GLOB '*[^0-9a-f]*')),
  canonical_heartbeat_base64url TEXT NOT NULL CHECK(length(CAST(canonical_heartbeat_base64url AS BLOB)) BETWEEN 1 AND 87382 AND length(CAST(canonical_heartbeat_base64url AS BLOB))=length(canonical_heartbeat_base64url) AND instr(canonical_heartbeat_base64url,char(0))=0 AND canonical_heartbeat_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  dependency_probe_base64url TEXT NOT NULL CHECK(length(CAST(dependency_probe_base64url AS BLOB)) BETWEEN 1 AND 87382 AND length(CAST(dependency_probe_base64url AS BLOB))=length(dependency_probe_base64url) AND instr(dependency_probe_base64url,char(0))=0 AND dependency_probe_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  poller_identity_sha256 TEXT CHECK(poller_identity_sha256 IS NULL OR (length(CAST(poller_identity_sha256 AS BLOB))=64 AND length(poller_identity_sha256)=64 AND instr(poller_identity_sha256,char(0))=0 AND poller_identity_sha256 NOT GLOB '*[^0-9a-f]*')),
  backlog_count INTEGER CHECK(backlog_count IS NULL OR (typeof(backlog_count)='integer' AND backlog_count BETWEEN 0 AND 9007199254740991)),
  delivered_task_count INTEGER CHECK(delivered_task_count IS NULL OR (typeof(delivered_task_count)='integer' AND delivered_task_count BETWEEN 0 AND 9007199254740991)),
  parked_predecessor_sequence INTEGER CHECK(parked_predecessor_sequence IS NULL OR (typeof(parked_predecessor_sequence)='integer' AND parked_predecessor_sequence BETWEEN 1 AND 9007199254740991)),
  parked_predecessor_sha256 TEXT CHECK(parked_predecessor_sha256 IS NULL OR (length(CAST(parked_predecessor_sha256 AS BLOB))=64 AND length(parked_predecessor_sha256)=64 AND instr(parked_predecessor_sha256,char(0))=0 AND parked_predecessor_sha256 NOT GLOB '*[^0-9a-f]*')),
  heartbeat_at_ms INTEGER NOT NULL CHECK(typeof(heartbeat_at_ms)='integer' AND heartbeat_at_ms BETWEEN 0 AND 9007199254740991),
  boot_id TEXT NOT NULL CHECK(length(boot_id) BETWEEN 20 AND 128 AND length(CAST(boot_id AS BLOB))=length(boot_id) AND boot_id NOT GLOB '*[^A-Za-z0-9_.:/-]*'),
  boot_generation INTEGER NOT NULL CHECK(typeof(boot_generation)='integer' AND boot_generation BETWEEN 1 AND 9007199254740991),
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
-- The observation guard reads the prior latest observation before latest CAS.
ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_heartbeats ADD COLUMN observation_sha256 TEXT REFERENCES jobs_managed_cloud_osv_runtime_observations(observation_sha256) ON DELETE RESTRICT CHECK(observation_sha256 IS NULL OR (length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*'));
CREATE TRIGGER trg_jobs_mc_osv_observation_0_guard BEFORE INSERT ON jobs_managed_cloud_osv_runtime_observations
BEGIN SELECT RAISE(ABORT,'managed cloud observation identity or predecessor is invalid') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NEW.authority_kind AND a.purpose IS NEW.purpose AND a.context_sha256 IS NEW.context_sha256 AND a.queue_generation_id IS NEW.queue_generation_id AND a.task_queue_sha256 IS NEW.task_queue_sha256) OR (NEW.parked_predecessor_sequence IS NOT NULL AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations p WHERE p.runtime_instance_id=NEW.runtime_instance_id AND p.instance_epoch=NEW.instance_epoch AND p.heartbeat_sequence=NEW.parked_predecessor_sequence AND p.observation_sha256=NEW.parked_predecessor_sha256 AND p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup' AND p.activation_sha256=NEW.activation_sha256 AND p.manifest_sha256=NEW.manifest_sha256 AND p.head_revision=NEW.head_revision AND p.transition_sha256=NEW.transition_sha256 AND p.authority_kind IS NEW.authority_kind AND p.purpose IS NEW.purpose AND p.context_sha256 IS NEW.context_sha256 AND p.queue_generation_id IS NEW.queue_generation_id AND p.task_queue_sha256 IS NEW.task_queue_sha256 AND p.boot_id=NEW.boot_id AND p.boot_generation=NEW.boot_generation)) OR (NEW.lifecycle_state='operational' AND NEW.health_state='ready' AND NOT EXISTS(
    -- The observation is inserted BEFORE latest CAS: h must still be the immediate
    -- previous fact. A negative or latch_acknowledged fact breaks the parked chain.
    SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats h
    JOIN jobs_managed_cloud_osv_runtime_observations p
      ON p.runtime_instance_id=h.runtime_instance_id AND p.instance_epoch=h.instance_epoch
      AND p.heartbeat_sequence=h.heartbeat_sequence AND p.observation_sha256=h.observation_sha256
    WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch
      AND h.protocol_version=2 AND p.heartbeat_sequence=NEW.heartbeat_sequence-1
      AND p.activation_sha256 IS NEW.activation_sha256
      AND p.manifest_sha256 IS NEW.manifest_sha256
      AND p.head_revision IS NEW.head_revision
      AND p.transition_sha256 IS NEW.transition_sha256
      AND p.authority_kind IS NEW.authority_kind
      AND p.purpose IS NEW.purpose
      AND p.context_sha256 IS NEW.context_sha256
      AND p.queue_generation_id IS NEW.queue_generation_id
      AND p.task_queue_sha256 IS NEW.task_queue_sha256
      AND p.boot_id=NEW.boot_id AND p.boot_generation=NEW.boot_generation
      AND ((p.lifecycle_state='parked' AND p.health_state='degraded' AND p.reason_code='startup'
        AND p.heartbeat_sequence=NEW.parked_predecessor_sequence
        AND p.observation_sha256=NEW.parked_predecessor_sha256)
       OR (p.lifecycle_state='operational' AND p.health_state='ready' AND p.reason_code IS NULL
        AND p.serving_latch_sha256=NEW.serving_latch_sha256
        AND p.acknowledged_latch_sha256=NEW.acknowledged_latch_sha256
        AND p.parked_predecessor_sequence=NEW.parked_predecessor_sequence
        AND p.parked_predecessor_sha256=NEW.parked_predecessor_sha256))))
 OR NEW.heartbeat_sequence<>COALESCE((SELECT h.heartbeat_sequence+1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch),1) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats h WHERE h.runtime_instance_id=NEW.runtime_instance_id AND h.instance_epoch=NEW.instance_epoch AND h.heartbeat_at_ms>NEW.heartbeat_at_ms) OR NEW.heartbeat_at_ms>CAST(strftime('%s','now') AS INTEGER)*1000 OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND i.claimed_at_ms<=NEW.heartbeat_at_ms) OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch WHERE r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND r.worker_id=NEW.worker_id AND h.current_boot_generation=r.boot_generation) OR (NEW.health_state='ready' AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_startup_authorities s WHERE s.protocol_version=2 AND s.boot_id=NEW.boot_id AND s.runtime_instance_id=NEW.runtime_instance_id AND s.instance_epoch=NEW.instance_epoch AND s.boot_generation=NEW.boot_generation AND NEW.parked_predecessor_sequence>=s.parked_heartbeat_sequence)); END;
CREATE TRIGGER trg_jobs_mc_osv_observation_no_update BEFORE UPDATE ON jobs_managed_cloud_osv_runtime_observations
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_osv_observation_no_delete BEFORE DELETE ON jobs_managed_cloud_osv_runtime_observations
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;
CREATE TRIGGER trg_jobs_mc_osv_latest_observation_0_guard BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
BEGIN SELECT RAISE(ABORT,'managed cloud latest heartbeat observation does not match') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.observation_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND i.protocol_version=2 AND NEW.protocol_version=2 AND NEW.observation_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o WHERE o.runtime_instance_id IS NEW.runtime_instance_id AND o.instance_epoch IS NEW.instance_epoch AND o.heartbeat_sequence IS NEW.heartbeat_sequence AND o.activation_sha256 IS NEW.activation_sha256 AND o.manifest_sha256 IS NEW.manifest_sha256 AND o.component_id IS NEW.component_id AND o.role IS NEW.role AND o.worker_id IS NEW.worker_id AND o.artifact_sha256 IS NEW.artifact_sha256 AND o.migration_set_sha256 IS NEW.migration_set_sha256 AND o.config_schema_sha256 IS NEW.config_schema_sha256 AND o.protocol_set_sha256 IS NEW.protocol_set_sha256 AND o.task_queue_sha256 IS NEW.task_queue_sha256 AND o.failure_converter_sha256 IS NEW.failure_converter_sha256 AND o.dependency_evidence_sha256 IS NEW.dependency_evidence_sha256 AND o.health_state IS NEW.health_state AND o.reason_code IS NEW.reason_code AND o.heartbeat_at_ms IS NEW.heartbeat_at_ms AND o.head_revision=NEW.observed_head_revision AND o.transition_sha256=NEW.observed_transition_sha256 AND o.protocol_version=NEW.protocol_version AND o.observation_sha256=NEW.observation_sha256)))); END;
CREATE TRIGGER trg_jobs_mc_osv_latest_observation_1_guard BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
BEGIN SELECT RAISE(ABORT,'managed cloud latest heartbeat observation does not match') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND ((a.authority_kind='legacy_activation' AND NEW.protocol_version=1 AND NEW.observation_sha256 IS NULL) OR (a.authority_kind='provisioning_v4' AND i.protocol_version=2 AND NEW.protocol_version=2 AND NEW.observation_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o WHERE o.runtime_instance_id IS NEW.runtime_instance_id AND o.instance_epoch IS NEW.instance_epoch AND o.heartbeat_sequence IS NEW.heartbeat_sequence AND o.activation_sha256 IS NEW.activation_sha256 AND o.manifest_sha256 IS NEW.manifest_sha256 AND o.component_id IS NEW.component_id AND o.role IS NEW.role AND o.worker_id IS NEW.worker_id AND o.artifact_sha256 IS NEW.artifact_sha256 AND o.migration_set_sha256 IS NEW.migration_set_sha256 AND o.config_schema_sha256 IS NEW.config_schema_sha256 AND o.protocol_set_sha256 IS NEW.protocol_set_sha256 AND o.task_queue_sha256 IS NEW.task_queue_sha256 AND o.failure_converter_sha256 IS NEW.failure_converter_sha256 AND o.dependency_evidence_sha256 IS NEW.dependency_evidence_sha256 AND o.health_state IS NEW.health_state AND o.reason_code IS NEW.reason_code AND o.heartbeat_at_ms IS NEW.heartbeat_at_ms AND o.head_revision=NEW.observed_head_revision AND o.transition_sha256=NEW.observed_transition_sha256 AND o.protocol_version=NEW.protocol_version AND o.observation_sha256=NEW.observation_sha256)))); END;
CREATE TABLE jobs_managed_cloud_bootstrap_source_verifier_members (
  bootstrap_set_sha256 TEXT NOT NULL CHECK(length(CAST(bootstrap_set_sha256 AS BLOB))=64 AND length(bootstrap_set_sha256)=64 AND instr(bootstrap_set_sha256,char(0))=0 AND bootstrap_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_instance_id TEXT NOT NULL CHECK(length(runtime_instance_id) BETWEEN 20 AND 128 AND length(CAST(runtime_instance_id AS BLOB))=length(runtime_instance_id) AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  instance_epoch INTEGER NOT NULL CHECK(typeof(instance_epoch)='integer' AND instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence INTEGER NOT NULL CHECK(typeof(heartbeat_sequence)='integer' AND heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  observation_sha256 TEXT NOT NULL CHECK(length(CAST(observation_sha256 AS BLOB))=64 AND length(observation_sha256)=64 AND instr(observation_sha256,char(0))=0 AND observation_sha256 NOT GLOB '*[^0-9a-f]*'),
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  PRIMARY KEY(bootstrap_set_sha256,runtime_instance_id,instance_epoch),
  FOREIGN KEY(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) REFERENCES jobs_managed_cloud_osv_runtime_observations(runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256) ON DELETE RESTRICT
);
CREATE TRIGGER trg_jobs_mc_osv_bootstrap_member_0_guard BEFORE INSERT ON jobs_managed_cloud_bootstrap_source_verifier_members
BEGIN SELECT RAISE(ABORT,'managed cloud bootstrap member is not an unsealed parked fact') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o JOIN jobs_managed_cloud_bootstrap_sets b ON b.bootstrap_set_sha256=NEW.bootstrap_set_sha256 WHERE o.runtime_instance_id=NEW.runtime_instance_id AND o.instance_epoch=NEW.instance_epoch AND o.heartbeat_sequence=NEW.heartbeat_sequence AND o.observation_sha256=NEW.observation_sha256 AND o.lifecycle_state='parked' AND o.health_state='degraded' AND o.reason_code='startup' AND o.authority_kind IS b.authority_kind AND o.purpose IS b.purpose AND o.context_sha256 IS b.context_sha256 AND o.queue_generation_id IS b.queue_generation_id AND o.task_queue_sha256 IS b.task_queue_sha256) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_serving_latches l WHERE l.bootstrap_set_sha256=NEW.bootstrap_set_sha256); END;
CREATE TRIGGER trg_jobs_mc_osv_bootstrap_member_no_update BEFORE UPDATE ON jobs_managed_cloud_bootstrap_source_verifier_members
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_osv_bootstrap_member_no_delete BEFORE DELETE ON jobs_managed_cloud_bootstrap_source_verifier_members
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;
CREATE TABLE jobs_managed_cloud_queue_contaminations (
  activation_sha256 TEXT NOT NULL CHECK(length(CAST(activation_sha256 AS BLOB))=64 AND length(activation_sha256)=64 AND instr(activation_sha256,char(0))=0 AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  manifest_sha256 TEXT NOT NULL CHECK(length(CAST(manifest_sha256 AS BLOB))=64 AND length(manifest_sha256)=64 AND instr(manifest_sha256,char(0))=0 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(length(CAST(region AS BLOB))=length(region) AND instr(region,char(0))=0 AND length(region) BETWEEN 1 AND 64 AND region NOT GLOB '*[^a-z0-9-]*' AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  authority_kind TEXT NOT NULL DEFAULT 'provisioning_v4' CHECK(authority_kind='provisioning_v4'),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR (length(CAST(context_sha256 AS BLOB))=64 AND length(context_sha256)=64 AND instr(context_sha256,char(0))=0 AND context_sha256 NOT GLOB '*[^0-9a-f]*')),
  queue_generation_id TEXT NOT NULL CHECK(length(queue_generation_id) BETWEEN 20 AND 128 AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id) AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  task_queue_sha256 TEXT NOT NULL CHECK(length(CAST(task_queue_sha256 AS BLOB))=64 AND length(task_queue_sha256)=64 AND instr(task_queue_sha256,char(0))=0 AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision INTEGER NOT NULL CHECK(typeof(head_revision)='integer' AND head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256 TEXT NOT NULL CHECK(length(CAST(transition_sha256 AS BLOB))=64 AND length(transition_sha256)=64 AND instr(transition_sha256,char(0))=0 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_instance_id TEXT NOT NULL CHECK(length(runtime_instance_id) BETWEEN 20 AND 128 AND length(CAST(runtime_instance_id AS BLOB))=length(runtime_instance_id) AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  instance_epoch INTEGER NOT NULL CHECK(typeof(instance_epoch)='integer' AND instance_epoch BETWEEN 1 AND 9007199254740991),
  worker_id TEXT NOT NULL CHECK(length(worker_id) BETWEEN 20 AND 128 AND length(CAST(worker_id AS BLOB))=length(worker_id) AND worker_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  role TEXT NOT NULL DEFAULT 'workflow_worker' CHECK(role='workflow_worker'),
  delivery_identity_sha256 TEXT NOT NULL CHECK(length(CAST(delivery_identity_sha256 AS BLOB))=64 AND length(delivery_identity_sha256)=64 AND instr(delivery_identity_sha256,char(0))=0 AND delivery_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  reason_code TEXT NOT NULL CHECK(reason_code='unexpected_pre_serving_delivery'),
  observed_at_ms INTEGER NOT NULL CHECK(typeof(observed_at_ms)='integer' AND observed_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(queue_generation_id,runtime_instance_id,instance_epoch,delivery_identity_sha256),
  FOREIGN KEY(runtime_instance_id,instance_epoch,worker_id,activation_sha256,manifest_sha256,head_revision,transition_sha256,role) REFERENCES jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,worker_id,activation_sha256,manifest_sha256,head_revision,transition_sha256,role) ON DELETE RESTRICT,
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) REFERENCES jobs_managed_cloud_queue_generations(queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT,
  CHECK(((purpose='production' AND context_sha256 IS NULL) OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL AND environment='staging' AND channel='canary')) IS TRUE)
);
CREATE TRIGGER trg_jobs_mc_queue_contamination_0_guard BEFORE INSERT ON jobs_managed_cloud_queue_contaminations
BEGIN SELECT RAISE(ABORT,'managed cloud contamination head binding is invalid') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NEW.authority_kind AND a.purpose IS NEW.purpose AND a.context_sha256 IS NEW.context_sha256 AND a.queue_generation_id IS NEW.queue_generation_id AND a.task_queue_sha256 IS NEW.task_queue_sha256); END;
CREATE TRIGGER trg_jobs_mc_queue_contamination_no_update BEFORE UPDATE ON jobs_managed_cloud_queue_contaminations
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is immutable'); END;
CREATE TRIGGER trg_jobs_mc_queue_contamination_no_delete BEFORE DELETE ON jobs_managed_cloud_queue_contaminations
BEGIN SELECT RAISE(ABORT,'managed cloud observation authority is permanent'); END;

-- No role-minimum/real-probe/isolation authorization is inferred here. The
-- signed latch writer MUST validate the exact current quorum and signature in
-- the same serialized transaction. These guards only bind stored metadata.
CREATE TRIGGER trg_jobs_mc_serving_latch_insert_0_guard BEFORE INSERT ON jobs_managed_cloud_serving_latches
BEGIN SELECT RAISE(ABORT,'managed cloud serving latch metadata or bootstrap snapshot is invalid') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND a.manifest_sha256=NEW.manifest_sha256 AND a.environment=NEW.environment AND a.region=NEW.region AND a.channel=NEW.channel AND a.authority_kind IS NEW.authority_kind AND a.purpose IS NEW.purpose AND a.context_sha256 IS NEW.context_sha256 AND a.queue_generation_id IS NEW.queue_generation_id AND a.task_queue_sha256 IS NEW.task_queue_sha256) OR NEW.expires_at_ms<=CAST(strftime('%s','now') AS INTEGER)*1000 OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations a WHERE a.activation_sha256=NEW.activation_sha256 AND NEW.not_before_ms>=a.not_before_ms AND NEW.expires_at_ms<=a.expires_at_ms) OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_bootstrap_sets b WHERE b.bootstrap_set_sha256=NEW.bootstrap_set_sha256 AND b.member_count=((SELECT count(*) FROM jobs_managed_cloud_bootstrap_runtime_members m WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256)+(SELECT count(*) FROM jobs_managed_cloud_bootstrap_source_verifier_members m WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256)) AND b.authority_kind IS NEW.authority_kind AND b.purpose IS NEW.purpose AND b.context_sha256 IS NEW.context_sha256 AND b.queue_generation_id IS NEW.queue_generation_id AND b.task_queue_sha256 IS NEW.task_queue_sha256) OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_signature_sets s WHERE s.signature_set_sha256=NEW.authorization_signature_set_sha256 AND s.target_audience='bluey-jobs-managed-cloud-serving-latch-v1' AND s.target_sha256=NEW.latch_sha256 AND s.trust_generation=NEW.trust_generation) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_queue_contaminations c WHERE c.queue_generation_id=NEW.queue_generation_id) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_bootstrap_runtime_members m LEFT JOIN jobs_managed_cloud_runtime_heartbeats h ON h.runtime_instance_id=m.runtime_instance_id AND h.instance_epoch=m.instance_epoch WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256 AND NOT (h.observation_sha256 IS m.observation_sha256 AND h.protocol_version=2)) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_bootstrap_source_verifier_members m LEFT JOIN jobs_managed_cloud_original_source_verifier_runtime_heartbeats h ON h.runtime_instance_id=m.runtime_instance_id AND h.instance_epoch=m.instance_epoch WHERE m.bootstrap_set_sha256=NEW.bootstrap_set_sha256 AND NOT (h.observation_sha256 IS m.observation_sha256 AND h.protocol_version=2)); END;

DROP TRIGGER IF EXISTS trg_jobs_osv_boot_startup_requires_degraded;
CREATE TRIGGER trg_jobs_osv_boot_startup_requires_degraded
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
WHEN NEW.protocol_version=1 AND (NOT EXISTS (
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
))
BEGIN SELECT RAISE(ABORT, 'source-verifier boot startup requires current degraded heartbeat'); END;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat_transition;
CREATE TRIGGER trg_jobs_osv_boot_heartbeat_transition BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=1 AND (EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence)
 AND (((NEW.health_state='ready' AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_startup_authorities s ON s.boot_id=b.boot_id AND s.boot_generation=b.boot_generation JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND s.ready_heartbeat_sequence<=NEW.heartbeat_sequence AND h.ready_boot_generation=b.boot_generation)) OR (NEW.health_state='degraded' AND NEW.reason_code IN ('startup','probe_failed') AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND (h.ready_boot_generation IS NULL OR h.current_boot_generation>h.ready_boot_generation)))) IS NOT TRUE))
BEGIN SELECT RAISE(ABORT,'source-verifier boot heartbeat transition is unauthorized'); END;
DROP TRIGGER IF EXISTS trg_jobs_osv_boot_heartbeat_transition_update;
CREATE TRIGGER trg_jobs_osv_boot_heartbeat_transition_update BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats WHEN NEW.protocol_version=1 AND (EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence) AND (((NEW.health_state='ready' AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_startup_authorities s ON s.boot_id=b.boot_id AND s.boot_generation=b.boot_generation JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND s.ready_heartbeat_sequence<=NEW.heartbeat_sequence AND h.ready_boot_generation=b.boot_generation)) OR (NEW.health_state='degraded' AND NEW.reason_code IN ('startup','probe_failed') AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_heartbeat_boot_bindings b JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=b.runtime_instance_id AND h.instance_epoch=b.instance_epoch WHERE b.runtime_instance_id=NEW.runtime_instance_id AND b.instance_epoch=NEW.instance_epoch AND b.heartbeat_sequence=NEW.heartbeat_sequence AND (h.ready_boot_generation IS NULL OR h.current_boot_generation>h.ready_boot_generation)))) IS NOT TRUE))
BEGIN SELECT RAISE(ABORT,'source-verifier boot heartbeat transition is unauthorized'); END;
CREATE TRIGGER trg_jobs_mc_boot_registration_binding_guard BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_registrations
BEGIN SELECT RAISE(ABORT,'source-verifier boot registration protocol binding is invalid') WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE i.runtime_instance_id=NEW.runtime_instance_id AND i.instance_epoch=NEW.instance_epoch AND i.worker_id=NEW.worker_id AND i.runtime_identity_sha256=NEW.runtime_identity_sha256 AND i.activation_sha256=NEW.activation_sha256 AND i.manifest_sha256=NEW.manifest_sha256 AND i.role=NEW.role AND i.component_id=NEW.component_id AND ((NEW.protocol_version=1 AND i.protocol_version=1 AND a.authority_kind='legacy_activation') OR (NEW.protocol_version=2 AND i.protocol_version=2 AND a.authority_kind='provisioning_v4'))); END;
CREATE TRIGGER trg_jobs_mc_boot_startup_v2_guard BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
BEGIN SELECT RAISE(ABORT,'source-verifier parked startup receipt binding is invalid') WHERE NEW.protocol_version=2 AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch JOIN jobs_managed_cloud_osv_runtime_observations o ON o.runtime_instance_id=r.runtime_instance_id AND o.instance_epoch=r.instance_epoch AND o.boot_id=r.boot_id AND o.boot_generation=r.boot_generation JOIN jobs_managed_cloud_osv_runtime_observations p ON p.runtime_instance_id=o.runtime_instance_id AND p.instance_epoch=o.instance_epoch AND p.boot_id=o.boot_id AND p.boot_generation=o.boot_generation WHERE r.protocol_version=2 AND r.boot_id=NEW.boot_id AND r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.boot_generation AND r.worker_id=NEW.worker_id AND h.current_boot_generation=r.boot_generation AND (h.ready_boot_generation IS NULL OR h.ready_boot_generation<r.boot_generation) AND o.heartbeat_sequence=NEW.parked_heartbeat_sequence AND o.observation_sha256=NEW.observation_sha256 AND o.worker_id=NEW.worker_id AND o.lifecycle_state='parked' AND o.health_state='degraded' AND o.reason_code='startup' AND o.acknowledged_latch_sha256 IS NULL AND o.poller_identity_sha256 IS NULL AND o.backlog_count IS NULL AND o.delivered_task_count IS NULL AND o.heartbeat_at_ms=NEW.startup_at_ms AND p.heartbeat_sequence=NEW.prior_heartbeat_sequence AND p.lifecycle_state='parked' AND p.health_state='degraded'); END;
CREATE TRIGGER trg_jobs_mc_boot_v2_ready_fact_guard BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_generation_heads
BEGIN SELECT RAISE(ABORT,'source-verifier ready generation requires an operational latch observation') WHERE (NEW.ready_boot_generation IS NOT NULL AND NEW.ready_boot_generation IS NOT OLD.ready_boot_generation) AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations r WHERE r.runtime_instance_id=NEW.runtime_instance_id AND r.instance_epoch=NEW.instance_epoch AND r.boot_generation=NEW.ready_boot_generation AND r.protocol_version=2) AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_osv_runtime_observations o JOIN jobs_managed_cloud_source_verifier_boot_registrations r ON r.boot_id=o.boot_id AND r.runtime_instance_id=o.runtime_instance_id AND r.instance_epoch=o.instance_epoch AND r.boot_generation=o.boot_generation AND r.protocol_version=2 WHERE o.runtime_instance_id=NEW.runtime_instance_id AND o.instance_epoch=NEW.instance_epoch AND o.boot_generation=NEW.ready_boot_generation AND o.lifecycle_state='operational' AND o.health_state='ready' AND o.reason_code IS NULL AND o.acknowledged_latch_sha256=o.serving_latch_sha256 AND o.serving_latch_sha256 IS NOT NULL AND o.parked_predecessor_sequence IS NOT NULL AND EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_startup_authorities s WHERE s.protocol_version=2 AND s.boot_id=o.boot_id AND s.runtime_instance_id=o.runtime_instance_id AND s.instance_epoch=o.instance_epoch AND s.boot_generation=o.boot_generation AND o.parked_predecessor_sequence>=s.parked_heartbeat_sequence)); END;


-- C3_REPLAY_SAFE_BOOT_GUARDS_BEGIN
-- The guarded 068 executor must refresh ONLY this finite suffix on restart:
-- older 062/064/065 replay recreates these three legacy predicates. Original bodies
-- remain unchanged under protocol_version=1; v2 uses its independent CAS guard.
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence
BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=1 AND (EXISTS (
  SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols protocol
   WHERE protocol.manifest_sha256 = NEW.manifest_sha256
     AND protocol.protocol_id = 'source_verifier_startup'
     AND protocol.protocol_version = 1 AND protocol.ordinal = 10
) AND (
  NEW.heartbeat_sequence <> 1
  OR NEW.health_state <> 'degraded'
  OR NEW.reason_code IS NOT 'startup'
))
BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime first heartbeat must be degraded startup');
END;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_fenced_update;
CREATE TRIGGER trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_fenced_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=1 AND (NEW.runtime_instance_id <> OLD.runtime_instance_id
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
  )))
BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat readiness fence is invalid');
END;
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_v2_sequence_insert;
CREATE TRIGGER trg_jobs_mc_osv_v2_sequence_insert BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=2 AND NEW.heartbeat_sequence<>1
BEGIN SELECT RAISE(ABORT,'managed cloud runtime heartbeat sequence is invalid'); END;
DROP TRIGGER IF EXISTS trg_jobs_mc_osv_v2_sequence_update;
CREATE TRIGGER trg_jobs_mc_osv_v2_sequence_update BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=2 AND (NEW.runtime_instance_id<>OLD.runtime_instance_id OR NEW.instance_epoch<>OLD.instance_epoch OR NEW.heartbeat_sequence<>OLD.heartbeat_sequence+1 OR NEW.heartbeat_at_ms<OLD.heartbeat_at_ms)
BEGIN SELECT RAISE(ABORT,'managed cloud runtime heartbeat fence is invalid'); END;
-- Preserve SQLite064's exact legacy condition/body/message, scoped to v1.
DROP TRIGGER IF EXISTS trg_jobs_managed_osv_heartbeat_ready_fence;
CREATE TRIGGER trg_jobs_managed_osv_heartbeat_ready_fence
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.protocol_version=1 AND NEW.health_state <> 'ready'
  AND EXISTS (
    SELECT 1
      FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
     WHERE head.runtime_instance_id = NEW.runtime_instance_id
       AND head.instance_epoch = NEW.instance_epoch
       AND head.ready_boot_generation IS NOT NULL
       AND head.current_boot_generation <= head.ready_boot_generation
  )
BEGIN
  SELECT RAISE(ABORT, 'source-verifier committed readiness cannot be lowered');
END;
-- C3_REPLAY_SAFE_BOOT_GUARDS_END
CREATE TEMP TABLE jobs_c3_observation_migration_guard(violations INTEGER NOT NULL CHECK(violations=0));
INSERT INTO jobs_c3_observation_migration_guard SELECT CASE WHEN EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE (a.authority_kind='legacy_activation' AND (i.protocol_version<>1 OR i.claimed_latch_sha256 IS NOT NULL)) OR (a.authority_kind='provisioning_v4' AND i.protocol_version<>2)) OR EXISTS(SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_instances i JOIN jobs_managed_cloud_activations a ON a.activation_sha256=i.activation_sha256 WHERE (a.authority_kind='legacy_activation' AND (i.protocol_version<>1 OR i.claimed_latch_sha256 IS NOT NULL)) OR (a.authority_kind='provisioning_v4' AND i.protocol_version<>2)) THEN 1 ELSE 0 END;
INSERT INTO jobs_c3_observation_migration_guard SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE jobs_c3_observation_migration_guard;
INSERT OR IGNORE INTO bluey_data_migrations(name) VALUES('jobs-provisioned-runtime-observations-storage-v1');
COMMIT;
PRAGMA foreign_keys = ON;

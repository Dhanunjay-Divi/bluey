-- Target: SQLite. Phase648 C1 storage only; no positive proof or runtime writer.
-- Forward-only after the first v4 authority; never coerce v4 rows into legacy.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
CREATE TABLE jobs_managed_cloud_signature_sets_phase648 (
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
    'bluey-jobs-managed-cloud-trust-policy-v1',
    'bluey-jobs-managed-cloud-provisioning-v4',
    'bluey-jobs-managed-cloud-preproduction-context-v1',
    'bluey-jobs-managed-cloud-serving-latch-v1'
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
INSERT INTO jobs_managed_cloud_signature_sets_phase648 (
  signature_set_sha256, signature_set_id, trust_generation, role, target_audience, target_sha256, signed_at_ms, signature_count, canonical_signature_set_base64url, recorded_by, recorded_at_ms
) SELECT
  signature_set_sha256, signature_set_id, trust_generation, role, target_audience, target_sha256, signed_at_ms, signature_count, canonical_signature_set_base64url, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_signature_sets;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_signature_sets_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_signature_sets_no_delete;
DROP TABLE jobs_managed_cloud_signature_sets;
ALTER TABLE jobs_managed_cloud_signature_sets_phase648 RENAME TO jobs_managed_cloud_signature_sets;
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_update BEFORE UPDATE ON jobs_managed_cloud_signature_sets
BEGIN SELECT RAISE(ABORT, 'managed cloud signature set is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_delete BEFORE DELETE ON jobs_managed_cloud_signature_sets
BEGIN SELECT RAISE(ABORT, 'managed cloud signature set is immutable'); END;
CREATE INDEX idx_jobs_managed_cloud_signature_sets_target
ON jobs_managed_cloud_signature_sets(target_audience,target_sha256,trust_generation,role);

-- These are immutable allocation/context METADATA, not validated physical evidence.
-- No API writer or positive context/resource/queue proof reader is added by this slice.
CREATE TABLE jobs_managed_cloud_queue_generations (
  queue_generation_id TEXT NOT NULL PRIMARY KEY CHECK(length(queue_generation_id) BETWEEN 20 AND 128
    AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id)
    AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  issuance_ref TEXT NOT NULL UNIQUE CHECK(length(issuance_ref) BETWEEN 20 AND 128
    AND length(CAST(issuance_ref AS BLOB))=length(issuance_ref)
    AND issuance_ref NOT GLOB '*[^A-Za-z0-9_-]*'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(length(region) BETWEEN 1 AND 64 AND instr(region,char(0))=0
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  namespace TEXT NOT NULL CHECK(length(CAST(namespace AS BLOB)) BETWEEN 1 AND 240
    AND instr(namespace,char(0))=0),
  task_queue TEXT NOT NULL CHECK(length(CAST(task_queue AS BLOB)) BETWEEN 1 AND 240
    AND instr(task_queue,char(0))=0),
  task_queue_sha256 TEXT NOT NULL UNIQUE CHECK(length(CAST(task_queue_sha256 AS BLOB))=64
    AND instr(task_queue_sha256,char(0))=0
    AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(namespace,task_queue),
  UNIQUE(queue_generation_id,environment,region,channel,purpose,task_queue_sha256),
  CHECK(purpose='production' OR (environment='staging' AND channel='canary'))
);
CREATE TABLE jobs_managed_cloud_preproduction_contexts (
  context_sha256 TEXT NOT NULL PRIMARY KEY CHECK(length(CAST(context_sha256 AS BLOB))=64
    AND instr(context_sha256,char(0))=0
    AND context_sha256 NOT GLOB '*[^0-9a-f]*'),
  context_id TEXT NOT NULL UNIQUE CHECK(length(context_id) BETWEEN 20 AND 128
    AND length(CAST(context_id AS BLOB))=length(context_id)
    AND context_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  trust_generation INTEGER NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation)
    ON DELETE RESTRICT CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  environment TEXT NOT NULL CHECK(environment='staging'),
  region TEXT NOT NULL CHECK(length(region) BETWEEN 1 AND 64 AND instr(region,char(0))=0
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region,1,1) GLOB '[a-z0-9]' AND substr(region,-1,1) GLOB '[a-z0-9]'),
  channel TEXT NOT NULL CHECK(channel='canary'),
  purpose TEXT NOT NULL DEFAULT 'nonproduction_synthetic' CHECK(purpose='nonproduction_synthetic'),
  source_commit TEXT NOT NULL CHECK(length(CAST(source_commit AS BLOB))=40
    AND instr(source_commit,char(0))=0
    AND source_commit NOT GLOB '*[^0-9a-f]*'
    AND source_commit<>'0000000000000000000000000000000000000000'),
  component_set_sha256 TEXT NOT NULL CHECK(length(CAST(component_set_sha256 AS BLOB))=64
    AND instr(component_set_sha256,char(0))=0
    AND component_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  db_instance_sha256 TEXT NOT NULL CHECK(length(CAST(db_instance_sha256 AS BLOB))=64
    AND instr(db_instance_sha256,char(0))=0
    AND db_instance_sha256 NOT GLOB '*[^0-9a-f]*'),
  private_resource_set_sha256 TEXT NOT NULL CHECK(length(CAST(private_resource_set_sha256 AS BLOB))=64
    AND instr(private_resource_set_sha256,char(0))=0
    AND private_resource_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  namespace TEXT NOT NULL CHECK(length(CAST(namespace AS BLOB)) BETWEEN 1 AND 240
    AND instr(namespace,char(0))=0),
  task_queue TEXT NOT NULL CHECK(length(CAST(task_queue AS BLOB)) BETWEEN 1 AND 240
    AND instr(task_queue,char(0))=0),
  queue_generation_id TEXT NOT NULL CHECK(length(queue_generation_id) BETWEEN 20 AND 128
    AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id)
    AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  task_queue_sha256 TEXT NOT NULL CHECK(length(CAST(task_queue_sha256 AS BLOB))=64
    AND instr(task_queue_sha256,char(0))=0 AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  egress_policy_sha256 TEXT NOT NULL CHECK(length(CAST(egress_policy_sha256 AS BLOB))=64
    AND instr(egress_policy_sha256,char(0))=0
    AND egress_policy_sha256 NOT GLOB '*[^0-9a-f]*'),
  cohort_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_cohorts(cohort_sha256) ON DELETE RESTRICT
    CHECK(length(CAST(cohort_sha256 AS BLOB))=64 AND instr(cohort_sha256,char(0))=0
      AND cohort_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_context_base64url TEXT NOT NULL CHECK(length(canonical_context_base64url) BETWEEN 1 AND 174763
    AND length(CAST(canonical_context_base64url AS BLOB))=length(canonical_context_base64url)
    AND canonical_context_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  authorization_signature_set_sha256 TEXT NOT NULL
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
    CHECK(length(CAST(authorization_signature_set_sha256 AS BLOB))=64
      AND instr(authorization_signature_set_sha256,char(0))=0
      AND authorization_signature_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms BETWEEN 0 AND 9007199254740991),
  not_before_ms INTEGER NOT NULL CHECK(not_before_ms BETWEEN issued_at_ms AND 9007199254740991),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms BETWEEN 1 AND 9007199254740991
    AND expires_at_ms>not_before_ms AND expires_at_ms-issued_at_ms<=86400000),
  recorded_by TEXT NOT NULL CHECK(length(CAST(recorded_by AS BLOB)) BETWEEN 1 AND 240
    AND instr(recorded_by,char(0))=0),
  recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms BETWEEN issued_at_ms AND 9007199254740991),
  UNIQUE(context_sha256,environment,region,channel,queue_generation_id,cohort_sha256),
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256)
    REFERENCES jobs_managed_cloud_queue_generations(
      queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT
);
CREATE TRIGGER trg_jobs_managed_cloud_context_insert BEFORE INSERT
ON jobs_managed_cloud_preproduction_contexts BEGIN
  SELECT RAISE(ABORT,'invalid managed cloud context metadata')
  WHERE NEW.expires_at_ms<=CAST(strftime('%s','now') AS INTEGER)*1000
      +CAST(substr(strftime('%f','now'),4,3) AS INTEGER)
     OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_queue_generations queue
       WHERE queue.queue_generation_id=NEW.queue_generation_id
         AND queue.namespace=NEW.namespace AND queue.task_queue=NEW.task_queue)
     OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_cohorts cohort
       WHERE cohort.cohort_sha256=NEW.cohort_sha256 AND cohort.trust_generation=NEW.trust_generation
         AND cohort.environment=NEW.environment AND cohort.region=NEW.region AND cohort.channel=NEW.channel)
     OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_signature_sets signature
       WHERE signature.signature_set_sha256=NEW.authorization_signature_set_sha256
         AND signature.target_audience='bluey-jobs-managed-cloud-preproduction-context-v1'
         AND signature.target_sha256=NEW.context_sha256 AND signature.trust_generation=NEW.trust_generation);
END;

CREATE TABLE jobs_managed_cloud_activations_phase648 (
  activation_sha256               TEXT PRIMARY KEY
    CHECK(length(activation_sha256) = 64 AND lower(activation_sha256) = activation_sha256
      AND activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  activation_id                   TEXT NOT NULL UNIQUE CHECK(length(activation_id) > 0),
  activation_generation           INTEGER NOT NULL
    CHECK(activation_generation BETWEEN 1 AND 9007199254740991),
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  environment                     TEXT NOT NULL CHECK(environment IN ('production', 'staging')),
  region                          TEXT NOT NULL CHECK(
    length(region) BETWEEN 1 AND 64
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region, 1, 1) GLOB '[a-z0-9]'
    AND substr(region, -1, 1) GLOB '[a-z0-9]'
  ),
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general', 'shadow')),
  channel_sequence                INTEGER NOT NULL
    CHECK(channel_sequence BETWEEN 1 AND 9007199254740991),
  expected_head_revision          INTEGER NOT NULL CHECK(expected_head_revision >= 0),
  expected_transition_sha256      TEXT,
  predecessor_activation_sha256   TEXT,
  manifest_sha256                 TEXT NOT NULL,
  manifest_signature_set_sha256   TEXT NOT NULL,
  cohort_sha256                   TEXT NOT NULL,
  cohort_signature_set_sha256     TEXT NOT NULL,
  feature_authority_sha256        TEXT NOT NULL
    CHECK(length(feature_authority_sha256) = 64
      AND lower(feature_authority_sha256) = feature_authority_sha256
      AND feature_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  cloud_distribution_enabled      INTEGER NOT NULL
    CHECK(cloud_distribution_enabled IN (0, 1)),
  workflow_command_dispatch_enabled INTEGER NOT NULL
    CHECK(workflow_command_dispatch_enabled IN (0, 1)),
  workflow_cleanup_enabled        INTEGER NOT NULL
    CHECK(workflow_cleanup_enabled IN (0, 1)),
  direct_discovery_enabled        INTEGER NOT NULL
    CHECK(direct_discovery_enabled IN (0, 1)),
  global_discovery_enabled        INTEGER NOT NULL
    CHECK(global_discovery_enabled IN (0, 1)),
  source_verification_enabled     INTEGER NOT NULL
    CHECK(source_verification_enabled IN (0, 1)),
  runner_fleet_evidence_sha256    TEXT
    CHECK(length(runner_fleet_evidence_sha256) = 64
      AND lower(runner_fleet_evidence_sha256) = runner_fleet_evidence_sha256
      AND runner_fleet_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  cleanup_authority_sha256        TEXT
    CHECK(length(cleanup_authority_sha256) = 64
      AND lower(cleanup_authority_sha256) = cleanup_authority_sha256
      AND cleanup_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  temporal_namespace_sha256       TEXT NOT NULL
    CHECK(length(temporal_namespace_sha256) = 64
      AND lower(temporal_namespace_sha256) = temporal_namespace_sha256
      AND temporal_namespace_sha256 NOT GLOB '*[^0-9a-f]*'),
  storage_config_sha256           TEXT NOT NULL
    CHECK(length(storage_config_sha256) = 64
      AND lower(storage_config_sha256) = storage_config_sha256
      AND storage_config_sha256 NOT GLOB '*[^0-9a-f]*'),
  task_queue_sha256               TEXT NOT NULL
    CHECK(length(task_queue_sha256) = 64 AND lower(task_queue_sha256) = task_queue_sha256
      AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  failure_converter_sha256        TEXT NOT NULL
    CHECK(length(failure_converter_sha256) = 64
      AND lower(failure_converter_sha256) = failure_converter_sha256
      AND failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  canary_evidence_sha256          TEXT
    CHECK(length(canary_evidence_sha256) = 64
      AND lower(canary_evidence_sha256) = canary_evidence_sha256
      AND canary_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  portal_readback_evidence_sha256 TEXT
    CHECK(length(portal_readback_evidence_sha256) = 64
      AND lower(portal_readback_evidence_sha256) = portal_readback_evidence_sha256
      AND portal_readback_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  portal_readback_at_ms           INTEGER CHECK(portal_readback_at_ms >= 0),
  portal_readback_ttl_ms          INTEGER
    CHECK(portal_readback_ttl_ms BETWEEN 60000 AND 2592000000),
  maximum_inflight                INTEGER NOT NULL CHECK(maximum_inflight BETWEEN 0 AND 100000),
  maximum_daily_admissions        INTEGER NOT NULL CHECK(maximum_daily_admissions BETWEEN 0 AND 1000000),
  requirement_count               INTEGER NOT NULL CHECK(requirement_count BETWEEN 6 AND 9),
  heartbeat_ttl_ms                INTEGER NOT NULL CHECK(heartbeat_ttl_ms BETWEEN 5000 AND 300000),
  recovery_acceptance_count       INTEGER NOT NULL CHECK(recovery_acceptance_count BETWEEN 0 AND 32),
  canonical_activation_base64url  TEXT NOT NULL CHECK(length(canonical_activation_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  not_before_ms                   INTEGER NOT NULL CHECK(not_before_ms >= issued_at_ms),
  expires_at_ms                   INTEGER NOT NULL CHECK(expires_at_ms > not_before_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  authority_kind TEXT NOT NULL DEFAULT 'legacy_activation'
    CHECK(authority_kind IN ('legacy_activation','provisioning_v4')),
  purpose TEXT NOT NULL DEFAULT 'production'
    CHECK(purpose IN ('production','nonproduction_synthetic')),
  context_sha256 TEXT CHECK(context_sha256 IS NULL OR
    (length(CAST(context_sha256 AS BLOB))=64 AND instr(context_sha256,char(0))=0
      AND context_sha256 NOT GLOB '*[^0-9a-f]*')),
  queue_generation_id TEXT CHECK(queue_generation_id IS NULL OR
    (length(queue_generation_id) BETWEEN 20 AND 128
      AND length(CAST(queue_generation_id AS BLOB))=length(queue_generation_id)
      AND queue_generation_id NOT GLOB '*[^A-Za-z0-9_-]*')),
  UNIQUE(environment, region, channel, trust_generation, channel_sequence),
  UNIQUE(activation_sha256, manifest_sha256),
  UNIQUE(activation_sha256, manifest_sha256, expires_at_ms),
  UNIQUE(
    activation_sha256, manifest_sha256,
    task_queue_sha256, failure_converter_sha256
  ),
  UNIQUE(activation_sha256, manifest_sha256, environment, region, channel),
  UNIQUE(
    activation_sha256, manifest_sha256, environment, region, channel,
    task_queue_sha256, failure_converter_sha256
  ),
  UNIQUE(activation_sha256, manifest_sha256, cohort_sha256, environment, region, channel),
  UNIQUE(activation_sha256, manifest_sha256, cohort_sha256),
  CHECK(
    (channel = 'shadow' AND maximum_inflight = 0 AND maximum_daily_admissions = 0)
    OR (channel IN ('canary', 'general') AND maximum_inflight > 0
      AND maximum_daily_admissions > 0)
  ),
  CHECK((expected_head_revision = 0 AND expected_transition_sha256 IS NULL)
    OR (expected_head_revision > 0 AND expected_transition_sha256 IS NOT NULL
      AND length(expected_transition_sha256) = 64
      AND lower(expected_transition_sha256) = expected_transition_sha256
      AND expected_transition_sha256 NOT GLOB '*[^0-9a-f]*')),
  CHECK((expected_head_revision = 0 AND predecessor_activation_sha256 IS NULL)
    OR (expected_head_revision > 0 AND predecessor_activation_sha256 IS NOT NULL)),
  CHECK((
    (authority_kind='legacy_activation' AND purpose='production'
      AND context_sha256 IS NULL AND queue_generation_id IS NULL
      AND runner_fleet_evidence_sha256 IS NOT NULL AND cleanup_authority_sha256 IS NOT NULL
      AND canary_evidence_sha256 IS NOT NULL AND portal_readback_evidence_sha256 IS NOT NULL
      AND portal_readback_at_ms IS NOT NULL AND portal_readback_ttl_ms IS NOT NULL
      AND portal_readback_at_ms<=issued_at_ms
      AND expires_at_ms<=portal_readback_at_ms+portal_readback_ttl_ms)
    OR (authority_kind='provisioning_v4' AND queue_generation_id IS NOT NULL AND requirement_count=9
      AND ((purpose='production' AND context_sha256 IS NULL)
        OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL
          AND environment='staging' AND channel='canary'))
      AND runner_fleet_evidence_sha256 IS NULL AND cleanup_authority_sha256 IS NULL
      AND canary_evidence_sha256 IS NULL AND portal_readback_evidence_sha256 IS NULL
      AND portal_readback_at_ms IS NULL AND portal_readback_ttl_ms IS NULL
      AND cloud_distribution_enabled=0 AND workflow_command_dispatch_enabled=0
      AND workflow_cleanup_enabled=0 AND direct_discovery_enabled=0
      AND global_discovery_enabled=0 AND source_verification_enabled=0)
  ) IS TRUE),
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256)
    REFERENCES jobs_managed_cloud_queue_generations(
      queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(context_sha256,environment,region,channel,queue_generation_id,cohort_sha256)
    REFERENCES jobs_managed_cloud_preproduction_contexts(
      context_sha256,environment,region,channel,queue_generation_id,cohort_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(predecessor_activation_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(
    manifest_sha256, manifest_signature_set_sha256, trust_generation,
    feature_authority_sha256
  )
    REFERENCES jobs_managed_cloud_manifests(
      manifest_sha256, authorization_signature_set_sha256, trust_generation,
      feature_authority_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(
    cohort_sha256, cohort_signature_set_sha256, trust_generation,
    environment, region, channel
  )
    REFERENCES jobs_managed_cloud_cohorts(
      cohort_sha256, authorization_signature_set_sha256, trust_generation,
      environment, region, channel
    ) ON DELETE RESTRICT,
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, failure_converter_sha256)
    REFERENCES jobs_managed_cloud_manifests(
      manifest_sha256, failure_converter_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_activations_phase648 (
  activation_sha256, activation_id, activation_generation, trust_generation, environment, region, channel, channel_sequence, expected_head_revision, expected_transition_sha256, predecessor_activation_sha256, manifest_sha256, manifest_signature_set_sha256, cohort_sha256, cohort_signature_set_sha256, feature_authority_sha256, cloud_distribution_enabled, workflow_command_dispatch_enabled, workflow_cleanup_enabled, direct_discovery_enabled, global_discovery_enabled, source_verification_enabled, runner_fleet_evidence_sha256, cleanup_authority_sha256, temporal_namespace_sha256, storage_config_sha256, task_queue_sha256, failure_converter_sha256, canary_evidence_sha256, portal_readback_evidence_sha256, portal_readback_at_ms, portal_readback_ttl_ms, maximum_inflight, maximum_daily_admissions, requirement_count, heartbeat_ttl_ms, recovery_acceptance_count, canonical_activation_base64url, authorization_signature_set_sha256, issued_at_ms, not_before_ms, expires_at_ms, recorded_by, recorded_at_ms
) SELECT
  activation_sha256, activation_id, activation_generation, trust_generation, environment, region, channel, channel_sequence, expected_head_revision, expected_transition_sha256, predecessor_activation_sha256, manifest_sha256, manifest_signature_set_sha256, cohort_sha256, cohort_signature_set_sha256, feature_authority_sha256, cloud_distribution_enabled, workflow_command_dispatch_enabled, workflow_cleanup_enabled, direct_discovery_enabled, global_discovery_enabled, source_verification_enabled, runner_fleet_evidence_sha256, cleanup_authority_sha256, temporal_namespace_sha256, storage_config_sha256, task_queue_sha256, failure_converter_sha256, canary_evidence_sha256, portal_readback_evidence_sha256, portal_readback_at_ms, portal_readback_ttl_ms, maximum_inflight, maximum_daily_admissions, requirement_count, heartbeat_ttl_ms, recovery_acceptance_count, canonical_activation_base64url, authorization_signature_set_sha256, issued_at_ms, not_before_ms, expires_at_ms, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_activations;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_activations_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_activations_no_delete;
DROP TABLE jobs_managed_cloud_activations;
ALTER TABLE jobs_managed_cloud_activations_phase648 RENAME TO jobs_managed_cloud_activations;
CREATE TRIGGER trg_jobs_managed_cloud_activations_no_update BEFORE UPDATE ON jobs_managed_cloud_activations
BEGIN SELECT RAISE(ABORT, 'managed cloud activation is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_activations_no_delete BEFORE DELETE ON jobs_managed_cloud_activations
BEGIN SELECT RAISE(ABORT, 'managed cloud activation is immutable'); END;
CREATE UNIQUE INDEX idx_jobs_managed_cloud_activation_queue_generation
ON jobs_managed_cloud_activations(queue_generation_id) WHERE queue_generation_id IS NOT NULL;
CREATE TRIGGER trg_jobs_managed_cloud_provisioning_insert BEFORE INSERT
ON jobs_managed_cloud_activations WHEN NEW.authority_kind='provisioning_v4' BEGIN
  SELECT RAISE(ABORT,'invalid managed cloud provisioning metadata')
  WHERE NEW.expires_at_ms<=CAST(strftime('%s','now') AS INTEGER)*1000
      +CAST(substr(strftime('%f','now'),4,3) AS INTEGER)
     OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_signature_sets signature
       WHERE signature.signature_set_sha256=NEW.authorization_signature_set_sha256
         AND signature.target_audience='bluey-jobs-managed-cloud-provisioning-v4'
         AND signature.target_sha256=NEW.activation_sha256
         AND signature.trust_generation=NEW.trust_generation)
     OR (NEW.context_sha256 IS NOT NULL AND NOT EXISTS(
       SELECT 1 FROM jobs_managed_cloud_preproduction_contexts context
        WHERE context.context_sha256=NEW.context_sha256 AND context.not_before_ms<=NEW.not_before_ms
          AND context.expires_at_ms>=NEW.expires_at_ms));
END;

CREATE TABLE jobs_managed_cloud_head_transitions_phase648 (
  transition_sha256               TEXT PRIMARY KEY
    CHECK(length(transition_sha256) = 64 AND lower(transition_sha256) = transition_sha256
      AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment                     TEXT NOT NULL CHECK(environment IN ('production', 'staging')),
  region                          TEXT NOT NULL CHECK(
    length(region) BETWEEN 1 AND 64
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region, 1, 1) GLOB '[a-z0-9]'
    AND substr(region, -1, 1) GLOB '[a-z0-9]'
  ),
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general', 'shadow')),
  head_revision                   INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision          INTEGER NOT NULL CHECK(previous_head_revision >= 0),
  previous_transition_sha256      TEXT,
  previous_activation_sha256      TEXT,
  previous_manifest_sha256        TEXT,
  previous_trust_generation       INTEGER,
  previous_channel_sequence       INTEGER,
  next_activation_sha256          TEXT NOT NULL,
  next_manifest_sha256            TEXT NOT NULL,
  next_trust_generation           INTEGER NOT NULL
    CHECK(next_trust_generation BETWEEN 1 AND 9007199254740991),
  next_channel_sequence           INTEGER NOT NULL
    CHECK(next_channel_sequence BETWEEN 1 AND 9007199254740991),
  transition_kind                 TEXT NOT NULL CHECK(transition_kind IN ('activation', 'rollback', 'provisioning')),
  authority_sha256                TEXT NOT NULL
    CHECK(length(authority_sha256) = 64 AND lower(authority_sha256) = authority_sha256
      AND authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  rollback_authority_sha256       TEXT,
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(environment, region, channel, head_revision),
  UNIQUE(environment, region, channel, previous_head_revision),
  UNIQUE(authority_sha256),
  UNIQUE(
    transition_sha256, environment, region, channel, head_revision,
    next_activation_sha256, next_manifest_sha256, next_trust_generation,
    next_channel_sequence
  ),
  UNIQUE(
    transition_sha256, environment, region, channel, head_revision,
    next_activation_sha256, next_manifest_sha256
  ),
  CHECK(head_revision = previous_head_revision + 1),
  CHECK(
    (head_revision = 1 AND previous_head_revision = 0
      AND previous_transition_sha256 IS NULL
      AND previous_activation_sha256 IS NULL AND previous_manifest_sha256 IS NULL
      AND previous_trust_generation IS NULL AND previous_channel_sequence IS NULL
      AND transition_kind IN ('activation', 'provisioning'))
    OR (head_revision > 1 AND previous_transition_sha256 IS NOT NULL
      AND previous_activation_sha256 IS NOT NULL AND previous_manifest_sha256 IS NOT NULL
      AND previous_trust_generation IS NOT NULL AND previous_channel_sequence IS NOT NULL)
  ),
  CHECK(
    (transition_kind IN ('activation', 'provisioning') AND authority_sha256 = next_activation_sha256
      AND rollback_authority_sha256 IS NULL)
    OR (transition_kind = 'rollback' AND rollback_authority_sha256 = authority_sha256)
  ),
  CHECK(
    head_revision = 1 OR next_trust_generation > previous_trust_generation
    OR (next_trust_generation = previous_trust_generation
      AND next_channel_sequence > previous_channel_sequence)
  ),
  FOREIGN KEY(
    previous_transition_sha256, environment, region, channel, previous_head_revision,
    previous_activation_sha256, previous_manifest_sha256,
    previous_trust_generation, previous_channel_sequence
  ) REFERENCES jobs_managed_cloud_head_transitions(
    transition_sha256, environment, region, channel, head_revision,
    next_activation_sha256, next_manifest_sha256,
    next_trust_generation, next_channel_sequence
  ) ON DELETE RESTRICT,
  FOREIGN KEY(next_activation_sha256, next_manifest_sha256, environment, region, channel)
    REFERENCES jobs_managed_cloud_activations(
      activation_sha256, manifest_sha256, environment, region, channel
    ) ON DELETE RESTRICT,
  FOREIGN KEY(
    rollback_authority_sha256, environment, region, channel,
    previous_activation_sha256, previous_manifest_sha256,
    next_activation_sha256, next_manifest_sha256
  ) REFERENCES jobs_managed_cloud_rollbacks(
    rollback_sha256, environment, region, channel,
    from_activation_sha256, from_manifest_sha256,
    to_activation_sha256, to_manifest_sha256
  ) ON DELETE RESTRICT
);
INSERT INTO jobs_managed_cloud_head_transitions_phase648 (
  transition_sha256, environment, region, channel, head_revision, previous_head_revision, previous_transition_sha256, previous_activation_sha256, previous_manifest_sha256, previous_trust_generation, previous_channel_sequence, next_activation_sha256, next_manifest_sha256, next_trust_generation, next_channel_sequence, transition_kind, authority_sha256, rollback_authority_sha256, recorded_by, recorded_at_ms
) SELECT
  transition_sha256, environment, region, channel, head_revision, previous_head_revision, previous_transition_sha256, previous_activation_sha256, previous_manifest_sha256, previous_trust_generation, previous_channel_sequence, next_activation_sha256, next_manifest_sha256, next_trust_generation, next_channel_sequence, transition_kind, authority_sha256, rollback_authority_sha256, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_head_transitions;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_transitions_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_transitions_no_delete;
DROP TABLE jobs_managed_cloud_head_transitions;
ALTER TABLE jobs_managed_cloud_head_transitions_phase648 RENAME TO jobs_managed_cloud_head_transitions;
CREATE TRIGGER trg_jobs_managed_cloud_transitions_no_update BEFORE UPDATE ON jobs_managed_cloud_head_transitions
BEGIN SELECT RAISE(ABORT, 'managed cloud head transition is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_transitions_no_delete BEFORE DELETE ON jobs_managed_cloud_head_transitions
BEGIN SELECT RAISE(ABORT, 'managed cloud head transition is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_transition_kind BEFORE INSERT
ON jobs_managed_cloud_head_transitions BEGIN
  SELECT RAISE(ABORT,'managed cloud transition authority kind mismatch')
  WHERE NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations target
    WHERE target.activation_sha256=NEW.next_activation_sha256
      AND ((NEW.transition_kind='provisioning' AND target.authority_kind='provisioning_v4')
        OR (NEW.transition_kind IN ('activation','rollback') AND target.authority_kind='legacy_activation')))
    OR (NEW.transition_kind<>'provisioning' AND EXISTS(
      SELECT 1 FROM jobs_managed_cloud_activations previous
       WHERE previous.activation_sha256=NEW.previous_activation_sha256
         AND previous.authority_kind='provisioning_v4'));
END;
CREATE TRIGGER trg_jobs_managed_cloud_head_kind BEFORE UPDATE ON jobs_managed_cloud_heads
WHEN EXISTS(SELECT 1 FROM jobs_managed_cloud_activations previous
  WHERE previous.activation_sha256=OLD.current_activation_sha256 AND previous.authority_kind='provisioning_v4')
AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations target
  WHERE target.activation_sha256=NEW.current_activation_sha256 AND target.authority_kind='provisioning_v4')
BEGIN SELECT RAISE(ABORT,'managed cloud v4 authority cannot downgrade'); END;

CREATE TRIGGER trg_jobs_managed_cloud_queue_generations_no_update BEFORE UPDATE ON jobs_managed_cloud_queue_generations
BEGIN SELECT RAISE(ABORT,'managed cloud provisioning metadata is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_queue_generations_no_delete BEFORE DELETE ON jobs_managed_cloud_queue_generations
BEGIN SELECT RAISE(ABORT,'managed cloud provisioning metadata is permanent'); END;
CREATE TRIGGER trg_jobs_managed_cloud_preproduction_contexts_no_update BEFORE UPDATE ON jobs_managed_cloud_preproduction_contexts
BEGIN SELECT RAISE(ABORT,'managed cloud provisioning metadata is immutable'); END;
CREATE TRIGGER trg_jobs_managed_cloud_preproduction_contexts_no_delete BEFORE DELETE ON jobs_managed_cloud_preproduction_contexts
BEGIN SELECT RAISE(ABORT,'managed cloud provisioning metadata is permanent'); END;
-- Check while rollback can still restore every old table and FK target.
CREATE TEMP TABLE jobs_phase648_foreign_key_guard(violations INTEGER CHECK(violations=0));
INSERT INTO jobs_phase648_foreign_key_guard SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE jobs_phase648_foreign_key_guard;
INSERT OR IGNORE INTO bluey_data_migrations(name)
VALUES('jobs-provisioned-runtime-startup-storage-v1');
COMMIT;
PRAGMA foreign_keys = ON;

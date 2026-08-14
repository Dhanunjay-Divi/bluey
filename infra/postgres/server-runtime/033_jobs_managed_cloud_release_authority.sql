-- Target: PostgreSQL
-- Dialect pair for SQLite migration 055. Missing policy/head/runtime evidence
-- is intentionally unavailable; this migration seeds no release authority.

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_signature_sets (
  signature_set_sha256 TEXT PRIMARY KEY CHECK(signature_set_sha256 ~ '^[0-9a-f]{64}$'),
  signature_set_id TEXT NOT NULL UNIQUE CHECK(length(signature_set_id)>0),
  trust_generation BIGINT NOT NULL CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role TEXT NOT NULL CHECK(role IN ('general_promotion','incident','promotion','release','root')),
  target_audience TEXT NOT NULL CHECK(target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1','bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1','bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1','bluey-jobs-managed-cloud-trust-policy-v1')),
  target_sha256 TEXT NOT NULL CHECK(target_sha256 ~ '^[0-9a-f]{64}$'),
  signed_at_ms BIGINT NOT NULL CHECK(signed_at_ms>=0),
  signature_count BIGINT NOT NULL CHECK(signature_count BETWEEN 1 AND 32),
  canonical_signature_set_base64url TEXT NOT NULL CHECK(length(canonical_signature_set_base64url)>0),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=signed_at_ms),
  UNIQUE(target_audience,target_sha256,trust_generation,role)
);
CREATE INDEX IF NOT EXISTS idx_jobs_managed_cloud_signature_sets_target
  ON jobs_managed_cloud_signature_sets(target_audience,target_sha256,trust_generation,role);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_signatures (
  signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  key_id TEXT NOT NULL CHECK(length(key_id)>0),
  signature_base64url TEXT NOT NULL CHECK(length(signature_base64url)=86),
  PRIMARY KEY(signature_set_sha256,key_id)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_trust_policies (
  policy_sha256 TEXT PRIMARY KEY CHECK(policy_sha256 ~ '^[0-9a-f]{64}$'),
  policy_id TEXT NOT NULL UNIQUE CHECK(length(policy_id)>0),
  trust_generation BIGINT NOT NULL UNIQUE CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256 TEXT,
  predecessor_trust_generation BIGINT NOT NULL CHECK(predecessor_trust_generation>=0),
  root_threshold BIGINT NOT NULL CHECK(root_threshold BETWEEN 1 AND 32),
  release_threshold BIGINT NOT NULL CHECK(release_threshold BETWEEN 1 AND 32),
  promotion_threshold BIGINT NOT NULL CHECK(promotion_threshold BETWEEN 1 AND 32),
  general_promotion_threshold BIGINT NOT NULL CHECK(general_promotion_threshold BETWEEN 1 AND 32),
  incident_threshold BIGINT NOT NULL CHECK(incident_threshold BETWEEN 1 AND 32),
  key_count BIGINT NOT NULL CHECK(key_count BETWEEN 5 AND 64),
  canonical_policy_base64url TEXT NOT NULL CHECK(length(canonical_policy_base64url)>0),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms>=0),
  valid_from_ms BIGINT NOT NULL CHECK(valid_from_ms>=0),
  expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms>issued_at_ms),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=issued_at_ms),
  UNIQUE(policy_sha256,trust_generation),
  CHECK(valid_from_ms<=issued_at_ms),
  CHECK((trust_generation=1 AND predecessor_trust_generation=0 AND predecessor_policy_sha256 IS NULL)
    OR (trust_generation>1 AND predecessor_trust_generation=trust_generation-1
      AND predecessor_policy_sha256 IS NOT NULL
      AND predecessor_policy_sha256 ~ '^[0-9a-f]{64}$')),
  FOREIGN KEY(predecessor_policy_sha256,predecessor_trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(policy_sha256,trust_generation) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_trust_keys (
  policy_sha256 TEXT NOT NULL,
  trust_generation BIGINT NOT NULL CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  key_id TEXT NOT NULL CHECK(length(key_id)>0),
  role TEXT NOT NULL CHECK(role IN ('general_promotion','incident','promotion','release','root')),
  public_key_base64url TEXT NOT NULL CHECK(length(public_key_base64url)=43),
  state TEXT NOT NULL CHECK(state IN ('active','retired','revoked')),
  valid_from_ms BIGINT NOT NULL CHECK(valid_from_ms>=0),
  valid_until_ms BIGINT NOT NULL CHECK(valid_until_ms>valid_from_ms),
  minimum_trust_generation BIGINT NOT NULL CHECK(minimum_trust_generation BETWEEN 1 AND 9007199254740991),
  maximum_trust_generation BIGINT NOT NULL CHECK(maximum_trust_generation BETWEEN 1 AND 9007199254740991),
  PRIMARY KEY(policy_sha256,key_id), UNIQUE(policy_sha256,public_key_base64url),
  CHECK(maximum_trust_generation>=minimum_trust_generation),
  FOREIGN KEY(policy_sha256,trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(policy_sha256,trust_generation) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifests (
  manifest_sha256 TEXT PRIMARY KEY CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
  manifest_id TEXT NOT NULL UNIQUE CHECK(length(manifest_id)>0),
  manifest_generation BIGINT NOT NULL UNIQUE CHECK(manifest_generation BETWEEN 1 AND 9007199254740991),
  trust_generation BIGINT NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  release_id TEXT NOT NULL UNIQUE CHECK(length(release_id)>0),
  release_sequence BIGINT NOT NULL UNIQUE CHECK(release_sequence BETWEEN 1 AND 9007199254740991),
  source_commit TEXT NOT NULL CHECK(source_commit ~ '^[0-9a-f]{40}$'),
  sqlite_migration_head TEXT NOT NULL CHECK(length(sqlite_migration_head)>0),
  postgres_migration_head TEXT NOT NULL CHECK(length(postgres_migration_head)>0),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  component_set_sha256 TEXT NOT NULL CHECK(component_set_sha256 ~ '^[0-9a-f]{64}$'),
  feature_authority_sha256 TEXT NOT NULL CHECK(feature_authority_sha256 ~ '^[0-9a-f]{64}$'),
  cloud_distribution_enabled BOOLEAN NOT NULL,
  workflow_command_dispatch_enabled BOOLEAN NOT NULL,
  workflow_cleanup_enabled BOOLEAN NOT NULL,
  direct_discovery_enabled BOOLEAN NOT NULL,
  global_discovery_enabled BOOLEAN NOT NULL,
  source_verification_enabled BOOLEAN NOT NULL,
  verification_evidence_sha256 TEXT NOT NULL CHECK(verification_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  component_count BIGINT NOT NULL CHECK(component_count=4),
  capability_count BIGINT NOT NULL CHECK(capability_count BETWEEN 7 AND 9),
  protocol_count BIGINT NOT NULL CHECK(protocol_count=11),
  canonical_manifest_base64url TEXT NOT NULL CHECK(length(canonical_manifest_base64url)>0),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  published_at_ms BIGINT NOT NULL CHECK(published_at_ms>=0),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=published_at_ms),
  UNIQUE(manifest_sha256,authorization_signature_set_sha256),
  UNIQUE(manifest_sha256,authorization_signature_set_sha256,trust_generation,feature_authority_sha256),
  UNIQUE(manifest_sha256,migration_set_sha256,config_schema_sha256,protocol_set_sha256),
  UNIQUE(manifest_sha256,release_id,release_sequence)
  ,UNIQUE(manifest_sha256,failure_converter_sha256)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_components (
  manifest_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT,
  component_id TEXT NOT NULL CHECK(length(component_id)>0),
  artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('binary','oci_image','static_bundle')),
  artifact_ref TEXT NOT NULL CHECK(length(artifact_ref) BETWEEN 1 AND 2048),
  artifact_sha256 TEXT NOT NULL CHECK(artifact_sha256 ~ '^[0-9a-f]{64}$'),
  build_id TEXT NOT NULL CHECK(length(build_id)>0),
  source_commit TEXT NOT NULL CHECK(source_commit ~ '^[0-9a-f]{40}$'),
  platform TEXT NOT NULL CHECK(platform IN ('linux','web')),
  architecture TEXT NOT NULL CHECK(architecture IN ('arm64','x86_64','wasm')),
  sbom_sha256 TEXT NOT NULL CHECK(sbom_sha256 ~ '^[0-9a-f]{64}$'),
  provenance_sha256 TEXT NOT NULL CHECK(provenance_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  runtime_measurement_sha256 TEXT CHECK(
    runtime_measurement_sha256 IS NULL OR runtime_measurement_sha256 ~ '^[0-9a-f]{64}$'),
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(manifest_sha256,component_id), UNIQUE(manifest_sha256,ordinal),
  UNIQUE(manifest_sha256,component_id,artifact_sha256,config_schema_sha256),
  UNIQUE(manifest_sha256,component_id,runtime_measurement_sha256),
  CHECK((component_id='jobs-portal')=(runtime_measurement_sha256 IS NULL))
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_capabilities (
  manifest_sha256 TEXT NOT NULL, component_id TEXT NOT NULL,
  capability TEXT NOT NULL CHECK(capability IN ('discovery_worker','global_discovery_worker','jobs_api','managed_runner','original_source_verifier','portal_static','workflow_command_dispatcher','workflow_cleanup_dispatcher','workflow_gateway','workflow_worker')),
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 9),
  PRIMARY KEY(manifest_sha256,capability), UNIQUE(manifest_sha256,component_id,capability),
  UNIQUE(manifest_sha256,ordinal),
  FOREIGN KEY(manifest_sha256,component_id)
    REFERENCES jobs_managed_cloud_manifest_components(manifest_sha256,component_id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_runtime_identities (
  manifest_sha256 TEXT NOT NULL,
  component_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('discovery_worker','global_discovery_worker','jobs_api','managed_runner','workflow_command_dispatcher','workflow_cleanup_dispatcher','workflow_gateway','workflow_worker')),
  runtime_measurement_sha256 TEXT NOT NULL CHECK(runtime_measurement_sha256 ~ '^[0-9a-f]{64}$'),
  runtime_identity_sha256 TEXT NOT NULL CHECK(runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 7),
  PRIMARY KEY(manifest_sha256,component_id,role),
  UNIQUE(manifest_sha256,role),
  UNIQUE(manifest_sha256,runtime_identity_sha256),
  UNIQUE(manifest_sha256,component_id,ordinal),
  UNIQUE(manifest_sha256,component_id,role,runtime_identity_sha256),
  FOREIGN KEY(manifest_sha256,component_id,runtime_measurement_sha256)
    REFERENCES jobs_managed_cloud_manifest_components(manifest_sha256,component_id,runtime_measurement_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,component_id,role)
    REFERENCES jobs_managed_cloud_manifest_capabilities(manifest_sha256,component_id,capability) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_protocols (
  manifest_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT,
  protocol_id TEXT NOT NULL CHECK(protocol_id IN ('ats_certification','execution_lease','gateway_command','managed_cloud_release','object_evidence','runner_checkpoint','runner_profile_snapshot','runner_result','runtime_heartbeat','workflow_cleanup','workflow_command')),
  protocol_version BIGINT NOT NULL CHECK(protocol_version BETWEEN 1 AND 9007199254740991),
  schema_sha256 TEXT NOT NULL CHECK(schema_sha256 ~ '^[0-9a-f]{64}$'),
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(manifest_sha256,protocol_id), UNIQUE(manifest_sha256,ordinal)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_cohorts (
  cohort_sha256 TEXT PRIMARY KEY CHECK(cohort_sha256 ~ '^[0-9a-f]{64}$'),
  cohort_id TEXT NOT NULL UNIQUE CHECK(length(cohort_id)>0),
  cohort_generation BIGINT NOT NULL CHECK(cohort_generation BETWEEN 1 AND 9007199254740991),
  trust_generation BIGINT NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  rollout_mode TEXT NOT NULL CHECK(rollout_mode IN ('allowlist','all_eligible_accounts','none')),
  member_count BIGINT NOT NULL CHECK(member_count BETWEEN 0 AND 512),
  approval_ref TEXT NOT NULL CHECK(length(approval_ref)>0),
  canonical_cohort_base64url TEXT NOT NULL CHECK(length(canonical_cohort_base64url)>0),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms>=0), not_before_ms BIGINT NOT NULL CHECK(not_before_ms>=issued_at_ms),
  expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms>not_before_ms),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0), recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=issued_at_ms),
  UNIQUE(environment,region,channel,cohort_generation), UNIQUE(cohort_sha256,authorization_signature_set_sha256),
  UNIQUE(cohort_sha256,authorization_signature_set_sha256,trust_generation,environment,region,channel),
  CHECK((channel='canary' AND rollout_mode='allowlist' AND member_count>0)
    OR (channel='general' AND rollout_mode='all_eligible_accounts' AND member_count=0)
    OR (channel='shadow' AND rollout_mode='none' AND member_count=0))
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_cohort_members (
  cohort_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_cohorts(cohort_sha256) ON DELETE RESTRICT,
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 511),
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  account_id_sha256 TEXT NOT NULL CHECK(account_id_sha256 ~ '^[0-9a-f]{64}$'),
  PRIMARY KEY(cohort_sha256,ordinal), UNIQUE(cohort_sha256,account_id), UNIQUE(cohort_sha256,account_id_sha256)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_activations (
  activation_sha256 TEXT PRIMARY KEY CHECK(activation_sha256 ~ '^[0-9a-f]{64}$'),
  activation_id TEXT NOT NULL UNIQUE CHECK(length(activation_id)>0),
  activation_generation BIGINT NOT NULL CHECK(activation_generation BETWEEN 1 AND 9007199254740991),
  trust_generation BIGINT NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  channel_sequence BIGINT NOT NULL CHECK(channel_sequence BETWEEN 1 AND 9007199254740991),
  expected_head_revision BIGINT NOT NULL CHECK(expected_head_revision>=0),
  expected_transition_sha256 TEXT,
  predecessor_activation_sha256 TEXT REFERENCES jobs_managed_cloud_activations(activation_sha256) ON DELETE RESTRICT,
  manifest_sha256 TEXT NOT NULL, manifest_signature_set_sha256 TEXT NOT NULL,
  cohort_sha256 TEXT NOT NULL, cohort_signature_set_sha256 TEXT NOT NULL,
  feature_authority_sha256 TEXT NOT NULL CHECK(feature_authority_sha256 ~ '^[0-9a-f]{64}$'),
  cloud_distribution_enabled BOOLEAN NOT NULL,
  workflow_command_dispatch_enabled BOOLEAN NOT NULL,
  workflow_cleanup_enabled BOOLEAN NOT NULL,
  direct_discovery_enabled BOOLEAN NOT NULL,
  global_discovery_enabled BOOLEAN NOT NULL,
  source_verification_enabled BOOLEAN NOT NULL,
  runner_fleet_evidence_sha256 TEXT NOT NULL CHECK(runner_fleet_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  cleanup_authority_sha256 TEXT NOT NULL CHECK(cleanup_authority_sha256 ~ '^[0-9a-f]{64}$'),
  temporal_namespace_sha256 TEXT NOT NULL CHECK(temporal_namespace_sha256 ~ '^[0-9a-f]{64}$'),
  storage_config_sha256 TEXT NOT NULL CHECK(storage_config_sha256 ~ '^[0-9a-f]{64}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  canary_evidence_sha256 TEXT NOT NULL CHECK(canary_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  portal_readback_evidence_sha256 TEXT NOT NULL CHECK(portal_readback_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  portal_readback_at_ms BIGINT NOT NULL CHECK(portal_readback_at_ms>=0),
  portal_readback_ttl_ms BIGINT NOT NULL CHECK(portal_readback_ttl_ms BETWEEN 60000 AND 2592000000),
  maximum_inflight BIGINT NOT NULL CHECK(maximum_inflight BETWEEN 0 AND 100000),
  maximum_daily_admissions BIGINT NOT NULL CHECK(maximum_daily_admissions BETWEEN 0 AND 1000000),
  requirement_count BIGINT NOT NULL CHECK(requirement_count BETWEEN 6 AND 9),
  heartbeat_ttl_ms BIGINT NOT NULL CHECK(heartbeat_ttl_ms BETWEEN 5000 AND 300000),
  recovery_acceptance_count BIGINT NOT NULL CHECK(recovery_acceptance_count BETWEEN 0 AND 32),
  canonical_activation_base64url TEXT NOT NULL CHECK(length(canonical_activation_base64url)>0),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms>=0), not_before_ms BIGINT NOT NULL CHECK(not_before_ms>=issued_at_ms),
  expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms>not_before_ms),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0), recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=issued_at_ms),
  UNIQUE(environment,region,channel,trust_generation,channel_sequence), UNIQUE(activation_sha256,manifest_sha256),
  UNIQUE(activation_sha256,manifest_sha256,expires_at_ms),
  UNIQUE(activation_sha256,manifest_sha256,task_queue_sha256,failure_converter_sha256),
  UNIQUE(activation_sha256,manifest_sha256,environment,region,channel),
  UNIQUE(activation_sha256,manifest_sha256,environment,region,channel,task_queue_sha256,failure_converter_sha256),
  UNIQUE(activation_sha256,manifest_sha256,cohort_sha256,environment,region,channel),
  UNIQUE(activation_sha256,manifest_sha256,cohort_sha256),
  CHECK((channel='shadow' AND maximum_inflight=0 AND maximum_daily_admissions=0)
    OR (channel IN ('canary','general') AND maximum_inflight>0 AND maximum_daily_admissions>0)),
  CHECK((expected_head_revision=0 AND expected_transition_sha256 IS NULL)
    OR (expected_head_revision>0 AND expected_transition_sha256 IS NOT NULL
      AND expected_transition_sha256 ~ '^[0-9a-f]{64}$')),
  CHECK((expected_head_revision=0 AND predecessor_activation_sha256 IS NULL)
    OR (expected_head_revision>0 AND predecessor_activation_sha256 IS NOT NULL)),
  CHECK(portal_readback_at_ms<=issued_at_ms),
  CHECK(expires_at_ms<=portal_readback_at_ms+portal_readback_ttl_ms),
  FOREIGN KEY(manifest_sha256,manifest_signature_set_sha256,trust_generation,feature_authority_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256,authorization_signature_set_sha256,trust_generation,feature_authority_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(cohort_sha256,cohort_signature_set_sha256,trust_generation,environment,region,channel)
    REFERENCES jobs_managed_cloud_cohorts(cohort_sha256,authorization_signature_set_sha256,trust_generation,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,failure_converter_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256,failure_converter_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_activation_requirements (
  activation_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_activations(activation_sha256) ON DELETE RESTRICT,
  role TEXT NOT NULL CHECK(role IN ('discovery_worker','global_discovery_worker','jobs_api','managed_runner','original_source_verifier','workflow_command_dispatcher','workflow_cleanup_dispatcher','workflow_gateway','workflow_worker')),
  minimum_ready_instances BIGINT NOT NULL CHECK(minimum_ready_instances=1),
  heartbeat_ttl_ms BIGINT NOT NULL CHECK(heartbeat_ttl_ms BETWEEN 5000 AND 300000),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 8), PRIMARY KEY(activation_sha256,role), UNIQUE(activation_sha256,ordinal)
  ,UNIQUE(activation_sha256,role,dependency_evidence_sha256)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_activation_recovery_acceptances (
  activation_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_activations(activation_sha256) ON DELETE RESTRICT,
  recovery_activation_sha256 TEXT NOT NULL, recovery_manifest_sha256 TEXT NOT NULL,
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(activation_sha256,recovery_activation_sha256,recovery_manifest_sha256),
  UNIQUE(activation_sha256,ordinal), CHECK(activation_sha256<>recovery_activation_sha256),
  FOREIGN KEY(recovery_activation_sha256,recovery_manifest_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_rollbacks (
  rollback_sha256 TEXT PRIMARY KEY CHECK(rollback_sha256 ~ '^[0-9a-f]{64}$'),
  rollback_id TEXT NOT NULL UNIQUE CHECK(length(rollback_id)>0),
  rollback_generation BIGINT NOT NULL CHECK(rollback_generation BETWEEN 1 AND 9007199254740991),
  trust_generation BIGINT NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  expected_head_revision BIGINT NOT NULL CHECK(expected_head_revision>=1),
  expected_transition_sha256 TEXT NOT NULL CHECK(expected_transition_sha256 ~ '^[0-9a-f]{64}$'),
  from_activation_sha256 TEXT NOT NULL, from_manifest_sha256 TEXT NOT NULL,
  to_activation_sha256 TEXT NOT NULL, to_manifest_sha256 TEXT NOT NULL,
  evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256 ~ '^[0-9a-f]{64}$'),
  reason_ref TEXT NOT NULL CHECK(length(reason_ref)>0),
  canonical_rollback_base64url TEXT NOT NULL CHECK(length(canonical_rollback_base64url)>0),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms>=0), recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=issued_at_ms),
  UNIQUE(trust_generation,rollback_generation),
  UNIQUE(rollback_sha256,environment,region,channel,from_activation_sha256,from_manifest_sha256,to_activation_sha256,to_manifest_sha256),
  CHECK(from_activation_sha256<>to_activation_sha256),
  FOREIGN KEY(from_activation_sha256,from_manifest_sha256,environment,region,channel)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(to_activation_sha256,to_manifest_sha256,environment,region,channel)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_revocations (
  revocation_sha256 TEXT PRIMARY KEY CHECK(revocation_sha256 ~ '^[0-9a-f]{64}$'),
  revocation_id TEXT NOT NULL UNIQUE CHECK(length(revocation_id)>0),
  revocation_generation BIGINT NOT NULL CHECK(revocation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_revocation_sha256 TEXT,
  trust_generation BIGINT NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  subject_kind TEXT NOT NULL CHECK(subject_kind IN ('activation','cohort','component','manifest','release','rollback','runtime_grant','runtime_instance','signing_key','trust_policy')),
  subject_id TEXT NOT NULL CHECK(length(subject_id)>0),
  subject_sha256 TEXT NOT NULL CHECK(subject_sha256 ~ '^[0-9a-f]{64}$'),
  reason_ref TEXT NOT NULL CHECK(length(reason_ref)>0),
  canonical_revocation_base64url TEXT NOT NULL CHECK(length(canonical_revocation_base64url)>0),
  authorization_signature_set_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT,
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms>=0), effective_at_ms BIGINT NOT NULL CHECK(effective_at_ms>=issued_at_ms),
  recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0), recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=issued_at_ms),
  UNIQUE(trust_generation,revocation_generation),
  UNIQUE(revocation_sha256,trust_generation),
  UNIQUE(predecessor_revocation_sha256,trust_generation),
  UNIQUE(subject_kind,subject_id,subject_sha256),
  CHECK((revocation_generation=1 AND predecessor_revocation_sha256 IS NULL)
    OR (revocation_generation>1 AND predecessor_revocation_sha256 IS NOT NULL)),
  FOREIGN KEY(predecessor_revocation_sha256,trust_generation)
    REFERENCES jobs_managed_cloud_revocations(revocation_sha256,trust_generation)
    ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_head_transitions (
  transition_sha256 TEXT PRIMARY KEY CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision BIGINT NOT NULL CHECK(previous_head_revision>=0),
  previous_transition_sha256 TEXT, previous_activation_sha256 TEXT, previous_manifest_sha256 TEXT,
  previous_trust_generation BIGINT, previous_channel_sequence BIGINT,
  next_activation_sha256 TEXT NOT NULL, next_manifest_sha256 TEXT NOT NULL,
  next_trust_generation BIGINT NOT NULL CHECK(next_trust_generation BETWEEN 1 AND 9007199254740991),
  next_channel_sequence BIGINT NOT NULL CHECK(next_channel_sequence BETWEEN 1 AND 9007199254740991),
  transition_kind TEXT NOT NULL CHECK(transition_kind IN ('activation','rollback')),
  authority_sha256 TEXT NOT NULL CHECK(authority_sha256 ~ '^[0-9a-f]{64}$'),
  rollback_authority_sha256 TEXT, recorded_by TEXT NOT NULL CHECK(length(recorded_by)>0),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms>=0),
  UNIQUE(environment,region,channel,head_revision), UNIQUE(environment,region,channel,previous_head_revision),
  UNIQUE(authority_sha256),
  UNIQUE(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256),
  UNIQUE(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence),
  CHECK(head_revision=previous_head_revision+1),
  CHECK((head_revision=1 AND previous_head_revision=0 AND previous_transition_sha256 IS NULL
      AND previous_activation_sha256 IS NULL AND previous_manifest_sha256 IS NULL
      AND previous_trust_generation IS NULL AND previous_channel_sequence IS NULL AND transition_kind='activation')
    OR (head_revision>1 AND previous_transition_sha256 IS NOT NULL AND previous_activation_sha256 IS NOT NULL
      AND previous_manifest_sha256 IS NOT NULL AND previous_trust_generation IS NOT NULL AND previous_channel_sequence IS NOT NULL)),
  CHECK((transition_kind='activation' AND authority_sha256=next_activation_sha256 AND rollback_authority_sha256 IS NULL)
    OR (transition_kind='rollback' AND rollback_authority_sha256=authority_sha256)),
  CHECK(head_revision=1 OR next_trust_generation>previous_trust_generation
    OR (next_trust_generation=previous_trust_generation AND next_channel_sequence>previous_channel_sequence)),
  FOREIGN KEY(previous_transition_sha256,environment,region,channel,previous_head_revision,previous_activation_sha256,previous_manifest_sha256,previous_trust_generation,previous_channel_sequence)
    REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT,
  FOREIGN KEY(next_activation_sha256,next_manifest_sha256,environment,region,channel)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel) ON DELETE RESTRICT,
  FOREIGN KEY(rollback_authority_sha256,environment,region,channel,previous_activation_sha256,previous_manifest_sha256,next_activation_sha256,next_manifest_sha256)
    REFERENCES jobs_managed_cloud_rollbacks(rollback_sha256,environment,region,channel,from_activation_sha256,from_manifest_sha256,to_activation_sha256,to_manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_heads (
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  head_revision BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  current_transition_sha256 TEXT NOT NULL, current_activation_sha256 TEXT NOT NULL, current_manifest_sha256 TEXT NOT NULL,
  current_trust_generation BIGINT NOT NULL CHECK(current_trust_generation BETWEEN 1 AND 9007199254740991),
  current_channel_sequence BIGINT NOT NULL CHECK(current_channel_sequence BETWEEN 1 AND 9007199254740991),
  updated_at_ms BIGINT NOT NULL CHECK(updated_at_ms>=0), PRIMARY KEY(environment,region,channel),
  FOREIGN KEY(current_transition_sha256,environment,region,channel,head_revision,current_activation_sha256,current_manifest_sha256,current_trust_generation,current_channel_sequence)
    REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_grants (
  grant_id TEXT PRIMARY KEY CHECK(grant_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  token_sha256 TEXT NOT NULL UNIQUE CHECK(token_sha256 ~ '^[0-9a-f]{64}$'),
  grant_token_ciphertext TEXT CHECK(grant_token_ciphertext IS NULL OR (
    length(grant_token_ciphertext) BETWEEN 32 AND 1024
    AND grant_token_ciphertext LIKE 'bluey-jobs:v1:%')),
  issuance_ref TEXT NOT NULL UNIQUE CHECK(issuance_ref ~ '^[A-Za-z0-9_-]{20,128}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  activation_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL, component_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('discovery_worker','global_discovery_worker','jobs_api','managed_runner','original_source_verifier','workflow_command_dispatcher','workflow_cleanup_dispatcher','workflow_gateway','workflow_worker')),
  head_revision BIGINT NOT NULL CHECK(head_revision>=1),
  transition_sha256 TEXT NOT NULL CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  artifact_sha256 TEXT NOT NULL CHECK(artifact_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  expected_dependency_evidence_sha256 TEXT NOT NULL CHECK(expected_dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  expected_runtime_identity_sha256 TEXT NOT NULL CHECK(expected_runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  expected_worker_id TEXT NOT NULL CHECK(expected_worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  authorization_ref TEXT NOT NULL CHECK(authorization_ref ~ '^[A-Za-z0-9_-]{20,128}$'),
  created_by TEXT NOT NULL CHECK(length(created_by) BETWEEN 1 AND 240),
  activation_expires_at_ms BIGINT NOT NULL CHECK(activation_expires_at_ms>0),
  expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms>=0), created_at_ms BIGINT NOT NULL CHECK(created_at_ms>=0),
  CHECK(expires_at_ms>created_at_ms), CHECK(expires_at_ms<=activation_expires_at_ms),
  UNIQUE(grant_id,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,expected_dependency_evidence_sha256,expected_runtime_identity_sha256,expected_worker_id,activation_expires_at_ms),
  FOREIGN KEY(activation_sha256,manifest_sha256,environment,region,channel,task_queue_sha256,failure_converter_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,environment,region,channel,task_queue_sha256,failure_converter_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256,manifest_sha256,activation_expires_at_ms)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,expires_at_ms) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,component_id,artifact_sha256,config_schema_sha256)
    REFERENCES jobs_managed_cloud_manifest_components(manifest_sha256,component_id,artifact_sha256,config_schema_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,component_id,role)
    REFERENCES jobs_managed_cloud_manifest_capabilities(manifest_sha256,component_id,capability) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,migration_set_sha256,config_schema_sha256,protocol_set_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256,migration_set_sha256,config_schema_sha256,protocol_set_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(transition_sha256,environment,region,channel,head_revision,activation_sha256,manifest_sha256)
    REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256,role,expected_dependency_evidence_sha256)
    REFERENCES jobs_managed_cloud_activation_requirements(activation_sha256,role,dependency_evidence_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,component_id,role,expected_runtime_identity_sha256)
    REFERENCES jobs_managed_cloud_manifest_runtime_identities(manifest_sha256,component_id,role,runtime_identity_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_grant_revocations (
  grant_id TEXT PRIMARY KEY REFERENCES jobs_managed_cloud_runtime_grants(grant_id) ON DELETE RESTRICT,
  reason_ref TEXT NOT NULL CHECK(length(reason_ref)>0), revoked_by TEXT NOT NULL CHECK(length(revoked_by)>0),
  revoked_at_ms BIGINT NOT NULL CHECK(revoked_at_ms>=0)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_instances (
  grant_id TEXT PRIMARY KEY REFERENCES jobs_managed_cloud_runtime_grants(grant_id) ON DELETE RESTRICT,
  runtime_instance_id TEXT NOT NULL UNIQUE CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  runtime_identity_sha256 TEXT NOT NULL CHECK(runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  worker_id TEXT NOT NULL CHECK(worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  session_proof_hmac_sha256 TEXT NOT NULL UNIQUE CHECK(session_proof_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  activation_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL, component_id TEXT NOT NULL, role TEXT NOT NULL,
  head_revision BIGINT NOT NULL, transition_sha256 TEXT NOT NULL,
  artifact_sha256 TEXT NOT NULL, config_schema_sha256 TEXT NOT NULL,
  migration_set_sha256 TEXT NOT NULL, protocol_set_sha256 TEXT NOT NULL,
  task_queue_sha256 TEXT NOT NULL, failure_converter_sha256 TEXT NOT NULL,
  dependency_evidence_sha256 TEXT NOT NULL,
  activation_expires_at_ms BIGINT NOT NULL CHECK(activation_expires_at_ms>0),
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  claimed_at_ms BIGINT NOT NULL CHECK(claimed_at_ms>=0),
  UNIQUE(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256),
  UNIQUE(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,activation_expires_at_ms),
  FOREIGN KEY(grant_id,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,runtime_identity_sha256,worker_id,activation_expires_at_ms)
    REFERENCES jobs_managed_cloud_runtime_grants(grant_id,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,expected_dependency_evidence_sha256,expected_runtime_identity_sha256,expected_worker_id,activation_expires_at_ms) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_heartbeats (
  runtime_instance_id TEXT NOT NULL, instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  activation_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL, component_id TEXT NOT NULL, role TEXT NOT NULL,
  worker_id TEXT NOT NULL,
  artifact_sha256 TEXT NOT NULL, observed_head_revision BIGINT NOT NULL CHECK(observed_head_revision BETWEEN 1 AND 9007199254740991),
  observed_transition_sha256 TEXT NOT NULL CHECK(observed_transition_sha256 ~ '^[0-9a-f]{64}$'),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  health_state TEXT NOT NULL CHECK(health_state IN ('degraded','draining','ready')),
  reason_code TEXT CHECK(reason_code IS NULL OR reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','draining','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup')),
  heartbeat_at_ms BIGINT NOT NULL CHECK(heartbeat_at_ms>=0),
  PRIMARY KEY(runtime_instance_id,instance_epoch),
  FOREIGN KEY(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,observed_head_revision,observed_transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256)
    REFERENCES jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,activation_sha256,manifest_sha256,component_id,role,worker_id,head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256) ON DELETE RESTRICT,
  CHECK((health_state='ready' AND reason_code IS NULL)
    OR (health_state='draining' AND reason_code='draining')
    OR (health_state='degraded' AND reason_code IS NOT NULL AND reason_code<>'draining'))
);
CREATE INDEX IF NOT EXISTS idx_jobs_managed_cloud_runtime_heartbeat_readiness
  ON jobs_managed_cloud_runtime_heartbeats(activation_sha256,role,health_state,heartbeat_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_heartbeat_audit (
  runtime_instance_id TEXT NOT NULL,
  instance_epoch BIGINT NOT NULL CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence BIGINT NOT NULL CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  activation_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL,
  component_id TEXT NOT NULL, role TEXT NOT NULL, worker_id TEXT NOT NULL,
  artifact_sha256 TEXT NOT NULL,
  observed_head_revision BIGINT NOT NULL CHECK(observed_head_revision BETWEEN 1 AND 9007199254740991),
  observed_transition_sha256 TEXT NOT NULL CHECK(observed_transition_sha256 ~ '^[0-9a-f]{64}$'),
  migration_set_sha256 TEXT NOT NULL CHECK(migration_set_sha256 ~ '^[0-9a-f]{64}$'),
  config_schema_sha256 TEXT NOT NULL CHECK(config_schema_sha256 ~ '^[0-9a-f]{64}$'),
  protocol_set_sha256 TEXT NOT NULL CHECK(protocol_set_sha256 ~ '^[0-9a-f]{64}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  dependency_evidence_sha256 TEXT NOT NULL CHECK(dependency_evidence_sha256 ~ '^[0-9a-f]{64}$'),
  health_state TEXT NOT NULL CHECK(health_state IN ('degraded','draining','ready')),
  reason_code TEXT,
  heartbeat_at_ms BIGINT NOT NULL CHECK(heartbeat_at_ms>=0),
  PRIMARY KEY(runtime_instance_id,instance_epoch,heartbeat_sequence),
  FOREIGN KEY(runtime_instance_id,instance_epoch)
    REFERENCES jobs_managed_cloud_runtime_heartbeats(runtime_instance_id,instance_epoch)
    ON DELETE RESTRICT,
  CHECK((health_state='ready' AND reason_code IS NULL)
    OR (health_state='draining' AND reason_code='draining')
    OR (health_state='degraded' AND reason_code IN ('artifact_mismatch','config_mismatch','dependency_unavailable','head_mismatch','migration_mismatch','probe_failed','protocol_mismatch','startup')))
);

ALTER TABLE jobs_workflow_commands
  ADD COLUMN IF NOT EXISTS managed_cloud_authority_required BOOLEAN
    NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_workflow_bindings (
  command_id TEXT PRIMARY KEY CHECK(command_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  binding_sha256 TEXT NOT NULL UNIQUE CHECK(binding_sha256 ~ '^[0-9a-f]{64}$'),
  account_id_hmac_sha256 TEXT NOT NULL CHECK(account_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  application_id_hmac_sha256 TEXT NOT NULL CHECK(application_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  run_id_hmac_sha256 TEXT NOT NULL CHECK(run_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  workflow_id_hmac_sha256 TEXT NOT NULL CHECK(workflow_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general')),
  head_revision BIGINT NOT NULL CHECK(head_revision>=1), transition_sha256 TEXT NOT NULL,
  activation_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL, cohort_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_cohorts(cohort_sha256) ON DELETE RESTRICT,
  trust_generation BIGINT NOT NULL CHECK(trust_generation>=1), channel_sequence BIGINT NOT NULL CHECK(channel_sequence>=1),
  release_id TEXT NOT NULL CHECK(length(release_id)>0), release_sequence BIGINT NOT NULL CHECK(release_sequence>=1),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  failure_converter_sha256 TEXT NOT NULL CHECK(failure_converter_sha256 ~ '^[0-9a-f]{64}$'),
  readiness_sha256 TEXT NOT NULL CHECK(readiness_sha256 ~ '^[0-9a-f]{64}$'),
  resolved_at_ms BIGINT NOT NULL CHECK(resolved_at_ms>=0),
  release_memo_base64url TEXT NOT NULL CHECK(
    length(release_memo_base64url) BETWEEN 1 AND 174763
    AND release_memo_base64url ~ '^[A-Za-z0-9_-]+$'),
  release_memo_sha256 TEXT NOT NULL CHECK(release_memo_sha256 ~ '^[0-9a-f]{64}$'),
  activation_expires_at_ms BIGINT NOT NULL CHECK(activation_expires_at_ms>0),
  bound_at_ms BIGINT NOT NULL CHECK(bound_at_ms>=resolved_at_ms),
  CHECK(resolved_at_ms<activation_expires_at_ms),
  UNIQUE(account_id_hmac_sha256,command_id),
  UNIQUE(command_id,binding_sha256),
  UNIQUE(command_id,binding_sha256,release_memo_sha256),
  UNIQUE(command_id,binding_sha256,release_memo_base64url,release_memo_sha256),
  FOREIGN KEY(command_id)
    REFERENCES jobs_workflow_commands(id) ON DELETE CASCADE,
  FOREIGN KEY(transition_sha256,environment,region,channel,head_revision,activation_sha256,manifest_sha256,trust_generation,channel_sequence)
    REFERENCES jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256,manifest_sha256,task_queue_sha256,failure_converter_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,task_queue_sha256,failure_converter_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256,manifest_sha256,cohort_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256,manifest_sha256,cohort_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256,release_id,release_sequence)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256,release_id,release_sequence) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_request_start_authorities (
  attempt_id TEXT PRIMARY KEY CHECK(attempt_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  account_id TEXT NOT NULL,
  command_id TEXT NOT NULL CHECK(command_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  fence BIGINT NOT NULL CHECK(fence BETWEEN 1 AND 9007199254740991),
  event_phase TEXT NOT NULL DEFAULT 'request_started'
    CHECK(event_phase='request_started'),
  binding_sha256 TEXT NOT NULL CHECK(binding_sha256 ~ '^[0-9a-f]{64}$'),
  gateway_authority_base64url TEXT NOT NULL CHECK(
    length(gateway_authority_base64url) BETWEEN 1 AND 174763
    AND gateway_authority_base64url ~ '^[A-Za-z0-9_-]+$'),
  gateway_authority_sha256 TEXT NOT NULL
    CHECK(gateway_authority_sha256 ~ '^[0-9a-f]{64}$'),
  authorized_at_ms BIGINT NOT NULL
    CHECK(authorized_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(command_id),
  UNIQUE(command_id,fence),
  UNIQUE(attempt_id,account_id,command_id,fence),
  FOREIGN KEY(attempt_id,account_id,command_id,fence)
    REFERENCES jobs_workflow_command_attempts(id,account_id,command_id,fence)
    ON DELETE CASCADE,
  FOREIGN KEY(attempt_id,event_phase)
    REFERENCES jobs_workflow_command_attempt_events(attempt_id,event_phase)
    ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY(command_id,binding_sha256)
    REFERENCES jobs_managed_cloud_workflow_bindings(command_id,binding_sha256)
    ON DELETE CASCADE
);

ALTER TABLE jobs_workflow_cleanup_targets
  ADD COLUMN managed_cloud_binding_sha256 TEXT
    CHECK(managed_cloud_binding_sha256 IS NULL
      OR managed_cloud_binding_sha256 ~ '^[0-9a-f]{64}$'),
  ADD COLUMN managed_cloud_release_memo_base64url TEXT
    CHECK(managed_cloud_release_memo_base64url IS NULL OR (
      length(managed_cloud_release_memo_base64url) BETWEEN 1 AND 174763
      AND managed_cloud_release_memo_base64url ~ '^[A-Za-z0-9_-]+$')),
  ADD COLUMN managed_cloud_release_memo_sha256 TEXT
    CHECK(managed_cloud_release_memo_sha256 IS NULL
      OR managed_cloud_release_memo_sha256 ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT fk_jobs_cleanup_target_managed_cloud_binding
    FOREIGN KEY(start_command_id,managed_cloud_binding_sha256,
      managed_cloud_release_memo_sha256)
    REFERENCES jobs_managed_cloud_workflow_bindings(
      command_id,binding_sha256,release_memo_sha256) ON DELETE RESTRICT,
  ADD CONSTRAINT ck_jobs_cleanup_target_managed_cloud_binding_complete CHECK(
    (managed_cloud_binding_sha256 IS NULL
      AND managed_cloud_release_memo_base64url IS NULL
      AND managed_cloud_release_memo_sha256 IS NULL)
    OR (managed_cloud_binding_sha256 IS NOT NULL
      AND managed_cloud_release_memo_base64url IS NOT NULL
      AND managed_cloud_release_memo_sha256 IS NOT NULL)
  );

CREATE UNIQUE INDEX IF NOT EXISTS
  idx_jobs_workflow_commands_managed_cloud_request_identity
  ON jobs_workflow_commands(id,request_id);
CREATE UNIQUE INDEX IF NOT EXISTS
  idx_jobs_managed_cloud_runtime_instance_epoch
  ON jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch);
CREATE UNIQUE INDEX IF NOT EXISTS
  idx_jobs_managed_cloud_runtime_instance_epoch_worker
  ON jobs_managed_cloud_runtime_instances(runtime_instance_id,instance_epoch,worker_id);

ALTER TABLE jobs_execution_leases
  ADD COLUMN managed_cloud_workflow_request_id TEXT CHECK(
    managed_cloud_workflow_request_id IS NULL OR
    managed_cloud_workflow_request_id ~
      '^wfreq-v2-[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'),
  ADD COLUMN managed_cloud_request_command_id TEXT CHECK(
    managed_cloud_request_command_id IS NULL OR
    managed_cloud_request_command_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  ADD COLUMN managed_cloud_execution_command_id TEXT CHECK(
    managed_cloud_execution_command_id IS NULL OR
    managed_cloud_execution_command_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  ADD COLUMN managed_cloud_binding_sha256 TEXT CHECK(
    managed_cloud_binding_sha256 IS NULL OR
    managed_cloud_binding_sha256 ~ '^[0-9a-f]{64}$'),
  ADD COLUMN managed_cloud_release_memo_base64url TEXT CHECK(
    managed_cloud_release_memo_base64url IS NULL OR (
      length(managed_cloud_release_memo_base64url) BETWEEN 1 AND 174763
      AND managed_cloud_release_memo_base64url ~ '^[A-Za-z0-9_-]+$')),
  ADD COLUMN managed_cloud_release_sha256 TEXT CHECK(
    managed_cloud_release_sha256 IS NULL OR
    managed_cloud_release_sha256 ~ '^[0-9a-f]{64}$'),
  ADD COLUMN managed_cloud_runtime_instance_id TEXT CHECK(
    managed_cloud_runtime_instance_id IS NULL OR
    managed_cloud_runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  ADD COLUMN managed_cloud_runtime_instance_epoch BIGINT CHECK(
    managed_cloud_runtime_instance_epoch IS NULL OR
    managed_cloud_runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  ADD COLUMN managed_cloud_worker_id TEXT CHECK(
    managed_cloud_worker_id IS NULL OR
    managed_cloud_worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  ADD COLUMN managed_cloud_gateway_authority_base64url TEXT CHECK(
    managed_cloud_gateway_authority_base64url IS NULL OR (
      length(managed_cloud_gateway_authority_base64url) BETWEEN 1 AND 174763
      AND managed_cloud_gateway_authority_base64url ~ '^[A-Za-z0-9_-]+$')),
  ADD COLUMN managed_cloud_gateway_authority_sha256 TEXT CHECK(
    managed_cloud_gateway_authority_sha256 IS NULL OR
    managed_cloud_gateway_authority_sha256 ~ '^[0-9a-f]{64}$'),
  ADD COLUMN managed_cloud_lease_authority_sha256 TEXT CHECK(
    managed_cloud_lease_authority_sha256 IS NULL OR
    managed_cloud_lease_authority_sha256 ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT ck_jobs_execution_lease_managed_cloud_complete CHECK(
    num_nonnulls(
      managed_cloud_workflow_request_id,managed_cloud_request_command_id,
      managed_cloud_execution_command_id,managed_cloud_binding_sha256,
      managed_cloud_release_memo_base64url,managed_cloud_release_sha256,
      managed_cloud_runtime_instance_id,managed_cloud_runtime_instance_epoch,
      managed_cloud_worker_id,
      managed_cloud_gateway_authority_base64url,
      managed_cloud_gateway_authority_sha256,
      managed_cloud_lease_authority_sha256
    ) IN (0,12)),
  ADD CONSTRAINT fk_jobs_execution_lease_managed_cloud_request
    FOREIGN KEY(managed_cloud_request_command_id,managed_cloud_workflow_request_id)
    REFERENCES jobs_workflow_commands(id,request_id) ON DELETE CASCADE,
  ADD CONSTRAINT fk_jobs_execution_lease_managed_cloud_binding
    FOREIGN KEY(managed_cloud_execution_command_id,managed_cloud_binding_sha256,
      managed_cloud_release_memo_base64url,managed_cloud_release_sha256)
    REFERENCES jobs_managed_cloud_workflow_bindings(
      command_id,binding_sha256,release_memo_base64url,release_memo_sha256)
    ON DELETE CASCADE,
  ADD CONSTRAINT fk_jobs_execution_lease_managed_cloud_runtime
    FOREIGN KEY(managed_cloud_runtime_instance_id,managed_cloud_runtime_instance_epoch,
      managed_cloud_worker_id)
    REFERENCES jobs_managed_cloud_runtime_instances(
      runtime_instance_id,instance_epoch,worker_id)
    ON DELETE RESTRICT;

-- A single immutable receipt freezes the exact latest effect authority at the
-- prepared -> click_started boundary. Response-loss replay reads these bytes
-- before consulting any mutable release state.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_irreversible_effect_receipts (
  run_id TEXT NOT NULL CHECK(run_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  fence BIGINT NOT NULL CHECK(fence BETWEEN 1 AND 9007199254740991),
  account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 240),
  application_id TEXT NOT NULL CHECK(length(application_id) BETWEEN 1 AND 240),
  workflow_request_id TEXT NOT NULL CHECK(workflow_request_id ~
    '^wfreq-v2-[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'),
  request_command_id TEXT NOT NULL CHECK(request_command_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  execution_command_id TEXT NOT NULL CHECK(execution_command_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  binding_sha256 TEXT NOT NULL CHECK(binding_sha256 ~ '^[0-9a-f]{64}$'),
  release_memo_base64url TEXT NOT NULL CHECK(
    length(release_memo_base64url) BETWEEN 1 AND 174763
    AND release_memo_base64url ~ '^[A-Za-z0-9_-]+$'),
  release_sha256 TEXT NOT NULL CHECK(release_sha256 ~ '^[0-9a-f]{64}$'),
  runtime_instance_id TEXT NOT NULL CHECK(runtime_instance_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  runtime_instance_epoch BIGINT NOT NULL
    CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  worker_id TEXT NOT NULL CHECK(worker_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  gateway_authority_base64url TEXT NOT NULL CHECK(
    length(gateway_authority_base64url) BETWEEN 1 AND 174763
    AND gateway_authority_base64url ~ '^[A-Za-z0-9_-]+$'),
  gateway_authority_sha256 TEXT NOT NULL CHECK(
    gateway_authority_sha256 ~ '^[0-9a-f]{64}$'),
  receipt_sha256 TEXT NOT NULL UNIQUE CHECK(receipt_sha256 ~ '^[0-9a-f]{64}$'),
  committed_at_ms BIGINT NOT NULL CHECK(
    committed_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(run_id,fence),
  FOREIGN KEY(run_id) REFERENCES jobs_execution_leases(run_id) ON DELETE CASCADE,
  FOREIGN KEY(request_command_id,workflow_request_id)
    REFERENCES jobs_workflow_commands(id,request_id) ON DELETE CASCADE,
  FOREIGN KEY(execution_command_id,binding_sha256,
    release_memo_base64url,release_sha256)
    REFERENCES jobs_managed_cloud_workflow_bindings(
      command_id,binding_sha256,release_memo_base64url,release_memo_sha256)
    ON DELETE CASCADE,
  FOREIGN KEY(runtime_instance_id,runtime_instance_epoch,worker_id)
    REFERENCES jobs_managed_cloud_runtime_instances(
      runtime_instance_id,instance_epoch,worker_id) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_irreversible_receipt()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF TG_OP='UPDATE' THEN
    RAISE EXCEPTION 'managed cloud irreversible receipt is immutable';
  END IF;
  IF TG_OP='DELETE' THEN
    IF NOT EXISTS(
      SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
       WHERE token.account_id=OLD.account_id
         AND NOT EXISTS(
           SELECT 1 FROM accounts account WHERE account.id=OLD.account_id)
    ) THEN
      RAISE EXCEPTION 'managed cloud irreversible receipt is immutable';
    END IF;
    RETURN OLD;
  END IF;
  IF NOT EXISTS(
    SELECT 1 FROM jobs_execution_leases lease
     WHERE lease.run_id=NEW.run_id AND lease.fence=NEW.fence
       AND lease.account_id=NEW.account_id
       AND lease.application_id=NEW.application_id
       AND lease.phase='click_started'
       AND lease.managed_cloud_lease_authority_sha256 IS NOT NULL
       AND lease.managed_cloud_runtime_instance_id=NEW.runtime_instance_id
       AND lease.managed_cloud_runtime_instance_epoch=NEW.runtime_instance_epoch
       AND lease.managed_cloud_worker_id=NEW.worker_id
  ) OR NOT EXISTS(
    SELECT 1 FROM jobs_workflow_commands command
     WHERE command.id=NEW.request_command_id
       AND command.request_id=NEW.workflow_request_id
       AND command.account_id=NEW.account_id
       AND command.application_id=NEW.application_id
       AND command.run_id=NEW.run_id
       AND command.command_kind IN ('start','resume')
       AND command.managed_cloud_authority_required
       AND command.first_request_started_at_ms IS NOT NULL
       AND EXISTS(
         SELECT 1 FROM jobs_workflow_command_attempt_events event
          WHERE event.command_id=command.id AND event.event_kind='request_started')
  ) THEN
    RAISE EXCEPTION 'managed cloud irreversible receipt is invalid';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_irreversible_receipt_insert
  ON jobs_managed_cloud_irreversible_effect_receipts;
CREATE TRIGGER trg_jobs_managed_cloud_irreversible_receipt_insert
BEFORE INSERT ON jobs_managed_cloud_irreversible_effect_receipts FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_irreversible_receipt();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_irreversible_receipt_update
  ON jobs_managed_cloud_irreversible_effect_receipts;
CREATE TRIGGER trg_jobs_managed_cloud_irreversible_receipt_update
BEFORE UPDATE ON jobs_managed_cloud_irreversible_effect_receipts FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_irreversible_receipt();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_irreversible_receipt_delete
  ON jobs_managed_cloud_irreversible_effect_receipts;
CREATE TRIGGER trg_jobs_managed_cloud_irreversible_receipt_delete
BEFORE DELETE ON jobs_managed_cloud_irreversible_effect_receipts FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_irreversible_receipt();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_execution_lease()
RETURNS trigger LANGUAGE plpgsql AS $fn$
DECLARE
  old_managed BOOLEAN := OLD.managed_cloud_lease_authority_sha256 IS NOT NULL;
  new_managed BOOLEAN := NEW.managed_cloud_lease_authority_sha256 IS NOT NULL;
  authority_changed BOOLEAN :=
    NEW.managed_cloud_workflow_request_id IS DISTINCT FROM
      OLD.managed_cloud_workflow_request_id
    OR NEW.managed_cloud_request_command_id IS DISTINCT FROM
      OLD.managed_cloud_request_command_id
    OR NEW.managed_cloud_execution_command_id IS DISTINCT FROM
      OLD.managed_cloud_execution_command_id
    OR NEW.managed_cloud_binding_sha256 IS DISTINCT FROM
      OLD.managed_cloud_binding_sha256
    OR NEW.managed_cloud_release_memo_base64url IS DISTINCT FROM
      OLD.managed_cloud_release_memo_base64url
    OR NEW.managed_cloud_release_sha256 IS DISTINCT FROM
      OLD.managed_cloud_release_sha256
    OR NEW.managed_cloud_runtime_instance_id IS DISTINCT FROM
      OLD.managed_cloud_runtime_instance_id
    OR NEW.managed_cloud_runtime_instance_epoch IS DISTINCT FROM
      OLD.managed_cloud_runtime_instance_epoch
    OR NEW.managed_cloud_worker_id IS DISTINCT FROM OLD.managed_cloud_worker_id
    OR NEW.managed_cloud_gateway_authority_base64url IS DISTINCT FROM
      OLD.managed_cloud_gateway_authority_base64url
    OR NEW.managed_cloud_gateway_authority_sha256 IS DISTINCT FROM
      OLD.managed_cloud_gateway_authority_sha256
    OR NEW.managed_cloud_lease_authority_sha256 IS DISTINCT FROM
      OLD.managed_cloud_lease_authority_sha256;
BEGIN
  IF old_managed AND NOT new_managed THEN
    RAISE EXCEPTION 'managed-cloud execution lease authority cannot be removed';
  END IF;
  IF old_managed AND NEW.fence<>OLD.fence
     AND NEW.managed_cloud_lease_authority_sha256=
         OLD.managed_cloud_lease_authority_sha256 THEN
    RAISE EXCEPTION 'managed-cloud execution lease fence requires new authority';
  END IF;
  IF authority_changed AND NOT (
    OLD.phase='prepared' AND NEW.phase='prepared' AND NEW.fence>OLD.fence
  ) THEN
    RAISE EXCEPTION 'managed-cloud execution lease authority is frozen';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_execution_lease
  ON jobs_execution_leases;
CREATE TRIGGER trg_jobs_managed_cloud_execution_lease
BEFORE UPDATE OF fence,phase,managed_cloud_workflow_request_id,
  managed_cloud_request_command_id,managed_cloud_execution_command_id,
  managed_cloud_binding_sha256,managed_cloud_release_memo_base64url,
  managed_cloud_release_sha256,managed_cloud_runtime_instance_id,
  managed_cloud_runtime_instance_epoch,managed_cloud_worker_id,
  managed_cloud_gateway_authority_base64url,
  managed_cloud_gateway_authority_sha256,managed_cloud_lease_authority_sha256
ON jobs_execution_leases FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_execution_lease();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_cleanup_target_binding()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.managed_cloud_binding_sha256 IS DISTINCT FROM OLD.managed_cloud_binding_sha256
      OR NEW.managed_cloud_release_memo_base64url
        IS DISTINCT FROM OLD.managed_cloud_release_memo_base64url
      OR NEW.managed_cloud_release_memo_sha256
        IS DISTINCT FROM OLD.managed_cloud_release_memo_sha256
    THEN
      RAISE EXCEPTION 'workflow cleanup managed-cloud memo authority is immutable';
    END IF;
    RETURN NEW;
  END IF;
  IF NEW.managed_cloud_binding_sha256 IS NULL THEN
    IF EXISTS(
      SELECT 1 FROM jobs_managed_cloud_workflow_bindings binding
       WHERE binding.command_id=NEW.start_command_id
    ) THEN
      RAISE EXCEPTION 'bound workflow cleanup target requires managed-cloud memo authority';
    END IF;
  ELSIF NOT EXISTS(
    SELECT 1 FROM jobs_managed_cloud_workflow_bindings binding
     WHERE binding.command_id=NEW.start_command_id
       AND binding.binding_sha256=NEW.managed_cloud_binding_sha256
       AND binding.release_memo_base64url=NEW.managed_cloud_release_memo_base64url
       AND binding.release_memo_sha256=NEW.managed_cloud_release_memo_sha256
  ) THEN
    RAISE EXCEPTION 'workflow cleanup managed-cloud memo authority is invalid';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_cleanup_target_binding
  ON jobs_workflow_cleanup_targets;
CREATE TRIGGER trg_jobs_managed_cloud_cleanup_target_binding
BEFORE INSERT OR UPDATE OF managed_cloud_binding_sha256,
  managed_cloud_release_memo_base64url,managed_cloud_release_memo_sha256
ON jobs_workflow_cleanup_targets FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_cleanup_target_binding();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_binding_retrofit()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF NOT EXISTS(
    SELECT 1 FROM jobs_workflow_commands command
     WHERE command.id=NEW.command_id
       AND command.managed_cloud_authority_required
  ) THEN
    RAISE EXCEPTION 'managed-cloud binding requires command authority marker';
  END IF;
  IF EXISTS(
    SELECT 1 FROM jobs_workflow_cleanup_targets target
     WHERE target.start_command_id=NEW.command_id
       AND (target.managed_cloud_binding_sha256 IS NULL
         OR target.managed_cloud_binding_sha256<>NEW.binding_sha256
         OR target.managed_cloud_release_memo_base64url<>NEW.release_memo_base64url
         OR target.managed_cloud_release_memo_sha256<>NEW.release_memo_sha256)
  ) THEN
    RAISE EXCEPTION 'managed-cloud binding cannot retrofit cleanup memo authority';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_binding_no_historical_retrofit
  ON jobs_managed_cloud_workflow_bindings;
CREATE TRIGGER trg_jobs_managed_cloud_binding_no_historical_retrofit
BEFORE INSERT ON jobs_managed_cloud_workflow_bindings FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_binding_retrofit();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_command_authority_marker()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF NEW.managed_cloud_authority_required IS DISTINCT FROM
      OLD.managed_cloud_authority_required THEN
    RAISE EXCEPTION 'managed-cloud command authority marker is immutable';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_command_authority_marker
  ON jobs_workflow_commands;
CREATE TRIGGER trg_jobs_managed_cloud_command_authority_marker
BEFORE UPDATE OF managed_cloud_authority_required ON jobs_workflow_commands
FOR EACH ROW EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_command_authority_marker();

CREATE OR REPLACE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  RAISE EXCEPTION 'managed cloud authority is immutable';
END;
$fn$;

-- Install identical update/delete guards on append-only global authority and
-- runtime evidence. Dynamic DDL keeps the paired table list reviewable.
DO $body$
DECLARE
  authority_table TEXT;
  trigger_name TEXT;
BEGIN
  FOREACH authority_table IN ARRAY ARRAY[
    'jobs_managed_cloud_signature_sets', 'jobs_managed_cloud_signatures',
    'jobs_managed_cloud_trust_policies', 'jobs_managed_cloud_trust_keys',
    'jobs_managed_cloud_manifests', 'jobs_managed_cloud_manifest_components',
    'jobs_managed_cloud_manifest_capabilities',
    'jobs_managed_cloud_manifest_runtime_identities',
    'jobs_managed_cloud_manifest_protocols',
    'jobs_managed_cloud_cohorts', 'jobs_managed_cloud_activations',
    'jobs_managed_cloud_activation_requirements',
    'jobs_managed_cloud_activation_recovery_acceptances',
    'jobs_managed_cloud_rollbacks', 'jobs_managed_cloud_revocations',
    'jobs_managed_cloud_head_transitions',
    'jobs_managed_cloud_runtime_grant_revocations',
    'jobs_managed_cloud_runtime_instances',
    'jobs_managed_cloud_workflow_bindings',
    'jobs_managed_cloud_request_start_authorities'
  ]
  LOOP
    trigger_name := 'trg_mc_' || substr(md5(authority_table), 1, 16) || '_u';
    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', trigger_name, authority_table);
    EXECUTE format(
      'CREATE TRIGGER %I BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation()',
      trigger_name, authority_table
    );
    trigger_name := 'trg_mc_' || substr(md5(authority_table), 1, 16) || '_d';
    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', trigger_name, authority_table);
    IF authority_table NOT IN (
      'jobs_managed_cloud_workflow_bindings',
      'jobs_managed_cloud_request_start_authorities'
    ) THEN
      EXECUTE format(
        'CREATE TRIGGER %I BEFORE DELETE ON %I FOR EACH ROW EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation()',
        trigger_name, authority_table
      );
    END IF;
  END LOOP;
END;
$body$;

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_binding_delete()
RETURNS trigger LANGUAGE plpgsql AS $fn$
DECLARE
  bound_account_id TEXT;
BEGIN
  SELECT command.account_id INTO bound_account_id
    FROM jobs_workflow_commands command WHERE command.id=OLD.command_id;
  IF bound_account_id IS NULL OR NOT EXISTS(
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id=bound_account_id
       AND NOT EXISTS(
         SELECT 1 FROM accounts account WHERE account.id=bound_account_id)
  ) THEN
    RAISE EXCEPTION 'managed cloud workflow binding is immutable';
  END IF;
  RETURN OLD;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_workflow_bindings_delete_guard
  ON jobs_managed_cloud_workflow_bindings;
CREATE TRIGGER trg_jobs_managed_cloud_workflow_bindings_delete_guard
BEFORE DELETE ON jobs_managed_cloud_workflow_bindings FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_binding_delete();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_request_start_delete()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF NOT EXISTS(
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id=OLD.account_id
       AND NOT EXISTS(
         SELECT 1 FROM accounts account WHERE account.id=OLD.account_id)
  ) THEN
    RAISE EXCEPTION 'managed cloud request-start authority is immutable';
  END IF;
  RETURN OLD;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_request_start_delete_guard
  ON jobs_managed_cloud_request_start_authorities;
CREATE TRIGGER trg_jobs_managed_cloud_request_start_delete_guard
BEFORE DELETE ON jobs_managed_cloud_request_start_authorities FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_request_start_delete();

CREATE OR REPLACE FUNCTION bluey_jobs_delete_managed_cloud_command_authority()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF EXISTS(
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id=OLD.account_id
       AND NOT EXISTS(
         SELECT 1 FROM accounts account WHERE account.id=OLD.account_id)
  ) THEN
    DELETE FROM jobs_execution_leases
     WHERE account_id=OLD.account_id
       AND (managed_cloud_request_command_id=OLD.id
         OR managed_cloud_execution_command_id=OLD.id);
    DELETE FROM jobs_workflow_cleanup_targets
     WHERE account_id=OLD.account_id AND start_command_id=OLD.id;
    DELETE FROM jobs_managed_cloud_request_start_authorities
     WHERE account_id=OLD.account_id AND command_id=OLD.id;
    DELETE FROM jobs_managed_cloud_workflow_bindings WHERE command_id=OLD.id;
  END IF;
  RETURN OLD;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_command_hard_delete
  ON jobs_workflow_commands;
CREATE TRIGGER trg_jobs_managed_cloud_command_hard_delete
BEFORE DELETE ON jobs_workflow_commands FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_delete_managed_cloud_command_authority();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_runtime_grant_update()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF (to_jsonb(NEW) - 'grant_token_ciphertext')
       IS DISTINCT FROM (to_jsonb(OLD) - 'grant_token_ciphertext')
    OR OLD.grant_token_ciphertext IS NULL
    OR NEW.grant_token_ciphertext IS NOT NULL
  THEN
    RAISE EXCEPTION 'managed cloud runtime grant secret can only be scrubbed';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_grants_secret_scrub
  ON jobs_managed_cloud_runtime_grants;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_grants_secret_scrub
BEFORE UPDATE ON jobs_managed_cloud_runtime_grants FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_runtime_grant_update();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_head_update()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF NEW.environment IS DISTINCT FROM OLD.environment
    OR NEW.region IS DISTINCT FROM OLD.region
    OR NEW.channel IS DISTINCT FROM OLD.channel
    OR NEW.head_revision <> OLD.head_revision + 1
    OR NEW.current_transition_sha256 = OLD.current_transition_sha256
  THEN
    RAISE EXCEPTION 'managed cloud head must advance in its immutable scope';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_heads_monotonic ON jobs_managed_cloud_heads;
CREATE TRIGGER trg_jobs_managed_cloud_heads_monotonic
BEFORE UPDATE ON jobs_managed_cloud_heads FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_head_update();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_heads_no_delete ON jobs_managed_cloud_heads;
CREATE TRIGGER trg_jobs_managed_cloud_heads_no_delete
BEFORE DELETE ON jobs_managed_cloud_heads FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_heartbeat_sequence()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF TG_OP = 'INSERT' AND NEW.heartbeat_sequence <> 1 THEN
    RAISE EXCEPTION 'managed cloud runtime heartbeat sequence is invalid';
  END IF;
  IF TG_OP = 'UPDATE' AND (
    NEW.runtime_instance_id IS DISTINCT FROM OLD.runtime_instance_id
    OR NEW.instance_epoch IS DISTINCT FROM OLD.instance_epoch
    OR NEW.heartbeat_sequence <> OLD.heartbeat_sequence + 1
    OR NEW.heartbeat_at_ms < OLD.heartbeat_at_ms
  ) THEN
    RAISE EXCEPTION 'managed cloud runtime heartbeat fence is invalid';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_heartbeats_sequence
  ON jobs_managed_cloud_runtime_heartbeats;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_heartbeats_sequence
BEFORE INSERT OR UPDATE ON jobs_managed_cloud_runtime_heartbeats FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_heartbeat_sequence();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_heartbeats_no_delete
  ON jobs_managed_cloud_runtime_heartbeats;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_heartbeats_no_delete
BEFORE DELETE ON jobs_managed_cloud_runtime_heartbeats FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_heartbeat_audit()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF TG_OP = 'INSERT' AND NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats current
     WHERE current.runtime_instance_id = NEW.runtime_instance_id
       AND current.instance_epoch = NEW.instance_epoch
       AND current.heartbeat_sequence = NEW.heartbeat_sequence
       AND current.activation_sha256 = NEW.activation_sha256
       AND current.manifest_sha256 = NEW.manifest_sha256
       AND current.component_id = NEW.component_id
       AND current.role = NEW.role
       AND current.worker_id = NEW.worker_id
       AND current.artifact_sha256 = NEW.artifact_sha256
       AND current.observed_head_revision = NEW.observed_head_revision
       AND current.observed_transition_sha256 = NEW.observed_transition_sha256
       AND current.migration_set_sha256 = NEW.migration_set_sha256
       AND current.config_schema_sha256 = NEW.config_schema_sha256
       AND current.protocol_set_sha256 = NEW.protocol_set_sha256
       AND current.task_queue_sha256 = NEW.task_queue_sha256
       AND current.failure_converter_sha256 = NEW.failure_converter_sha256
       AND current.dependency_evidence_sha256 = NEW.dependency_evidence_sha256
       AND current.health_state = NEW.health_state
       AND current.reason_code IS NOT DISTINCT FROM NEW.reason_code
       AND current.heartbeat_at_ms = NEW.heartbeat_at_ms
  ) THEN
    RAISE EXCEPTION 'managed cloud heartbeat audit does not match current fence';
  END IF;
  IF TG_OP = 'DELETE' AND NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats current
     WHERE current.runtime_instance_id = OLD.runtime_instance_id
       AND current.instance_epoch = OLD.instance_epoch
       AND OLD.heartbeat_sequence <= current.heartbeat_sequence - 64
  ) THEN
    RAISE EXCEPTION 'managed cloud heartbeat audit pruning is unsafe';
  END IF;
  RETURN COALESCE(NEW, OLD);
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_heartbeat_audit_insert
  ON jobs_managed_cloud_runtime_heartbeat_audit;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_heartbeat_audit_insert
BEFORE INSERT ON jobs_managed_cloud_runtime_heartbeat_audit FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_heartbeat_audit();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_heartbeat_audit_no_update
  ON jobs_managed_cloud_runtime_heartbeat_audit;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_heartbeat_audit_no_update
BEFORE UPDATE ON jobs_managed_cloud_runtime_heartbeat_audit FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_runtime_heartbeat_audit_prune
  ON jobs_managed_cloud_runtime_heartbeat_audit;
CREATE TRIGGER trg_jobs_managed_cloud_runtime_heartbeat_audit_prune
BEFORE DELETE ON jobs_managed_cloud_runtime_heartbeat_audit FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_heartbeat_audit();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_revocation_chain()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF (NEW.revocation_generation = 1 AND (
      NEW.predecessor_revocation_sha256 IS NOT NULL
      OR EXISTS(
        SELECT 1 FROM jobs_managed_cloud_revocations existing
         WHERE existing.trust_generation = NEW.trust_generation)))
    OR (NEW.revocation_generation > 1 AND NOT EXISTS(
      SELECT 1 FROM jobs_managed_cloud_revocations predecessor
       WHERE predecessor.revocation_sha256 = NEW.predecessor_revocation_sha256
         AND predecessor.revocation_generation = NEW.revocation_generation - 1
         AND predecessor.trust_generation = NEW.trust_generation))
  THEN
    RAISE EXCEPTION 'managed cloud revocation chain is invalid';
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_revocations_chain
  ON jobs_managed_cloud_revocations;
CREATE TRIGGER trg_jobs_managed_cloud_revocations_chain
BEFORE INSERT ON jobs_managed_cloud_revocations FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_revocation_chain();

CREATE OR REPLACE FUNCTION bluey_jobs_guard_managed_cloud_cohort_member_delete()
RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
  IF EXISTS(SELECT 1 FROM accounts WHERE id = OLD.account_id) THEN
    RAISE EXCEPTION 'managed cloud cohort member deletion requires account deletion';
  END IF;
  RETURN OLD;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_cohort_members_no_update
  ON jobs_managed_cloud_cohort_members;
CREATE TRIGGER trg_jobs_managed_cloud_cohort_members_no_update
BEFORE UPDATE ON jobs_managed_cloud_cohort_members FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_reject_managed_cloud_authority_mutation();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_cohort_members_delete_guard
  ON jobs_managed_cloud_cohort_members;
CREATE TRIGGER trg_jobs_managed_cloud_cohort_members_delete_guard
BEFORE DELETE ON jobs_managed_cloud_cohort_members FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_guard_managed_cloud_cohort_member_delete();

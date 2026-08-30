-- Target: SQLite
-- Signed managed-cloud stack releases, explicit scoped-head CAS, deployment
-- runtime evidence, and immutable workflow admission bindings. No authority is
-- seeded: a missing signed head is intentionally unavailable.

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_signature_sets (
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
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
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

CREATE INDEX IF NOT EXISTS idx_jobs_managed_cloud_signature_sets_target
  ON jobs_managed_cloud_signature_sets(
    target_audience, target_sha256, trust_generation, role
  );

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_signatures (
  signature_set_sha256             TEXT NOT NULL,
  key_id                           TEXT NOT NULL CHECK(length(key_id) > 0),
  signature_base64url              TEXT NOT NULL CHECK(length(signature_base64url) = 86),
  PRIMARY KEY(signature_set_sha256, key_id),
  FOREIGN KEY(signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_trust_policies (
  policy_sha256                    TEXT PRIMARY KEY
    CHECK(length(policy_sha256) = 64 AND lower(policy_sha256) = policy_sha256
      AND policy_sha256 NOT GLOB '*[^0-9a-f]*'),
  policy_id                        TEXT NOT NULL UNIQUE CHECK(length(policy_id) > 0),
  trust_generation                INTEGER NOT NULL UNIQUE
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256       TEXT,
  predecessor_trust_generation    INTEGER NOT NULL CHECK(predecessor_trust_generation >= 0),
  root_threshold                  INTEGER NOT NULL CHECK(root_threshold BETWEEN 1 AND 32),
  release_threshold               INTEGER NOT NULL CHECK(release_threshold BETWEEN 1 AND 32),
  promotion_threshold             INTEGER NOT NULL CHECK(promotion_threshold BETWEEN 1 AND 32),
  general_promotion_threshold     INTEGER NOT NULL CHECK(general_promotion_threshold BETWEEN 1 AND 32),
  incident_threshold              INTEGER NOT NULL CHECK(incident_threshold BETWEEN 1 AND 32),
  key_count                       INTEGER NOT NULL CHECK(key_count BETWEEN 5 AND 64),
  canonical_policy_base64url      TEXT NOT NULL CHECK(length(canonical_policy_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  valid_from_ms                   INTEGER NOT NULL CHECK(valid_from_ms >= 0),
  expires_at_ms                   INTEGER NOT NULL CHECK(expires_at_ms > issued_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(policy_sha256, trust_generation),
  CHECK(valid_from_ms <= issued_at_ms),
  CHECK(
    (trust_generation = 1 AND predecessor_trust_generation = 0
      AND predecessor_policy_sha256 IS NULL)
    OR (trust_generation > 1
      AND predecessor_trust_generation = trust_generation - 1
      AND predecessor_policy_sha256 IS NOT NULL
      AND length(predecessor_policy_sha256) = 64
      AND predecessor_policy_sha256 NOT GLOB '*[^0-9a-f]*')
  ),
  FOREIGN KEY(predecessor_policy_sha256, predecessor_trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(policy_sha256, trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_trust_keys (
  policy_sha256                    TEXT NOT NULL,
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  key_id                          TEXT NOT NULL CHECK(length(key_id) > 0),
  role                            TEXT NOT NULL CHECK(role IN (
    'general_promotion', 'incident', 'promotion', 'release', 'root'
  )),
  public_key_base64url            TEXT NOT NULL CHECK(length(public_key_base64url) = 43),
  state                           TEXT NOT NULL CHECK(state IN ('active', 'retired', 'revoked')),
  valid_from_ms                   INTEGER NOT NULL CHECK(valid_from_ms >= 0),
  valid_until_ms                  INTEGER NOT NULL CHECK(valid_until_ms > valid_from_ms),
  minimum_trust_generation        INTEGER NOT NULL
    CHECK(minimum_trust_generation BETWEEN 1 AND 9007199254740991),
  maximum_trust_generation        INTEGER NOT NULL
    CHECK(maximum_trust_generation BETWEEN 1 AND 9007199254740991),
  PRIMARY KEY(policy_sha256, key_id),
  UNIQUE(policy_sha256, public_key_base64url),
  CHECK(maximum_trust_generation >= minimum_trust_generation),
  FOREIGN KEY(policy_sha256, trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(policy_sha256, trust_generation)
    ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifests (
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
  capability_count                INTEGER NOT NULL CHECK(capability_count BETWEEN 7 AND 9),
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

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_components (
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(length(component_id) > 0),
  artifact_kind                   TEXT NOT NULL CHECK(artifact_kind IN ('binary', 'oci_image', 'static_bundle')),
  artifact_ref                    TEXT NOT NULL CHECK(length(artifact_ref) BETWEEN 1 AND 2048),
  artifact_sha256                 TEXT NOT NULL
    CHECK(length(artifact_sha256) = 64 AND lower(artifact_sha256) = artifact_sha256
      AND artifact_sha256 NOT GLOB '*[^0-9a-f]*'),
  build_id                        TEXT NOT NULL CHECK(length(build_id) > 0),
  source_commit                   TEXT NOT NULL
    CHECK(length(source_commit) = 40 AND lower(source_commit) = source_commit),
  platform                        TEXT NOT NULL CHECK(platform IN ('linux', 'web')),
  architecture                    TEXT NOT NULL CHECK(architecture IN ('arm64', 'x86_64', 'wasm')),
  sbom_sha256                     TEXT NOT NULL
    CHECK(length(sbom_sha256) = 64 AND lower(sbom_sha256) = sbom_sha256
      AND sbom_sha256 NOT GLOB '*[^0-9a-f]*'),
  provenance_sha256               TEXT NOT NULL
    CHECK(length(provenance_sha256) = 64 AND lower(provenance_sha256) = provenance_sha256
      AND provenance_sha256 NOT GLOB '*[^0-9a-f]*'),
  config_schema_sha256            TEXT NOT NULL
    CHECK(length(config_schema_sha256) = 64 AND lower(config_schema_sha256) = config_schema_sha256
      AND config_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_measurement_sha256      TEXT CHECK(
    runtime_measurement_sha256 IS NULL OR (
      length(runtime_measurement_sha256) = 64
      AND lower(runtime_measurement_sha256) = runtime_measurement_sha256
      AND runtime_measurement_sha256 NOT GLOB '*[^0-9a-f]*'
    )
  ),
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(manifest_sha256, component_id),
  UNIQUE(manifest_sha256, ordinal),
  UNIQUE(manifest_sha256, component_id, artifact_sha256, config_schema_sha256),
  UNIQUE(manifest_sha256, component_id, runtime_measurement_sha256),
  CHECK((component_id = 'jobs-portal') = (runtime_measurement_sha256 IS NULL)),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_runtime_identities (
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL,
  role                            TEXT NOT NULL CHECK(role IN (
    'discovery_worker', 'global_discovery_worker', 'jobs_api', 'managed_runner',
    'workflow_command_dispatcher', 'workflow_cleanup_dispatcher',
    'workflow_gateway', 'workflow_worker'
  )),
  runtime_measurement_sha256      TEXT NOT NULL CHECK(
    length(runtime_measurement_sha256) = 64
    AND runtime_measurement_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  runtime_identity_sha256         TEXT NOT NULL CHECK(
    length(runtime_identity_sha256) = 64
    AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 7),
  PRIMARY KEY(manifest_sha256, component_id, role),
  UNIQUE(manifest_sha256, role),
  UNIQUE(manifest_sha256, runtime_identity_sha256),
  UNIQUE(manifest_sha256, component_id, ordinal),
  UNIQUE(manifest_sha256, component_id, role, runtime_identity_sha256),
  FOREIGN KEY(manifest_sha256, component_id, runtime_measurement_sha256)
    REFERENCES jobs_managed_cloud_manifest_components(
      manifest_sha256, component_id, runtime_measurement_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, component_id, role)
    REFERENCES jobs_managed_cloud_manifest_capabilities(
      manifest_sha256, component_id, capability
    ) ON DELETE RESTRICT
);

-- Capabilities are distinct from deployable artifacts. One Jobs API artifact
-- may serve the API and both server-side dispatch loops; a portal artifact is
-- read back as static content and is never a heartbeat role.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_capabilities (
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL,
  capability                     TEXT NOT NULL CHECK(capability IN (
    'discovery_worker', 'global_discovery_worker', 'jobs_api', 'managed_runner',
    'original_source_verifier', 'portal_static', 'workflow_command_dispatcher',
    'workflow_cleanup_dispatcher', 'workflow_gateway', 'workflow_worker'
  )),
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 9),
  PRIMARY KEY(manifest_sha256, capability),
  UNIQUE(manifest_sha256, component_id, capability),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256, component_id)
    REFERENCES jobs_managed_cloud_manifest_components(manifest_sha256, component_id)
    ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_protocols (
  manifest_sha256                 TEXT NOT NULL,
  protocol_id                     TEXT NOT NULL CHECK(protocol_id IN (
    'ats_certification', 'execution_lease', 'gateway_command',
    'managed_cloud_release', 'object_evidence', 'runner_checkpoint',
    'runner_profile_snapshot', 'runner_result', 'runtime_heartbeat',
    'workflow_cleanup', 'workflow_command'
  )),
  protocol_version                INTEGER NOT NULL
    CHECK(protocol_version BETWEEN 1 AND 9007199254740991),
  schema_sha256                   TEXT NOT NULL
    CHECK(length(schema_sha256) = 64 AND lower(schema_sha256) = schema_sha256
      AND schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(manifest_sha256, protocol_id),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_cohorts (
  cohort_sha256                   TEXT PRIMARY KEY
    CHECK(length(cohort_sha256) = 64 AND lower(cohort_sha256) = cohort_sha256
      AND cohort_sha256 NOT GLOB '*[^0-9a-f]*'),
  cohort_id                       TEXT NOT NULL UNIQUE CHECK(length(cohort_id) > 0),
  cohort_generation               INTEGER NOT NULL
    CHECK(cohort_generation BETWEEN 1 AND 9007199254740991),
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
  rollout_mode                    TEXT NOT NULL CHECK(rollout_mode IN ('allowlist', 'all_eligible_accounts', 'none')),
  member_count                    INTEGER NOT NULL CHECK(member_count BETWEEN 0 AND 512),
  approval_ref                    TEXT NOT NULL CHECK(length(approval_ref) > 0),
  canonical_cohort_base64url      TEXT NOT NULL CHECK(length(canonical_cohort_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  not_before_ms                   INTEGER NOT NULL CHECK(not_before_ms >= issued_at_ms),
  expires_at_ms                   INTEGER NOT NULL CHECK(expires_at_ms > not_before_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(environment, region, channel, cohort_generation),
  UNIQUE(cohort_sha256, authorization_signature_set_sha256),
  UNIQUE(
    cohort_sha256, authorization_signature_set_sha256, trust_generation,
    environment, region, channel
  ),
  CHECK(
    (channel = 'canary' AND rollout_mode = 'allowlist' AND member_count > 0)
    OR (channel = 'general' AND rollout_mode = 'all_eligible_accounts' AND member_count = 0)
    OR (channel = 'shadow' AND rollout_mode = 'none' AND member_count = 0)
  ),
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_cohort_members (
  cohort_sha256                   TEXT NOT NULL,
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 511),
  account_id                      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  account_id_sha256               TEXT NOT NULL
    CHECK(length(account_id_sha256) = 64 AND lower(account_id_sha256) = account_id_sha256
      AND account_id_sha256 NOT GLOB '*[^0-9a-f]*'),
  PRIMARY KEY(cohort_sha256, ordinal),
  UNIQUE(cohort_sha256, account_id),
  UNIQUE(cohort_sha256, account_id_sha256),
  FOREIGN KEY(cohort_sha256)
    REFERENCES jobs_managed_cloud_cohorts(cohort_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_activations (
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
  runner_fleet_evidence_sha256    TEXT NOT NULL
    CHECK(length(runner_fleet_evidence_sha256) = 64
      AND lower(runner_fleet_evidence_sha256) = runner_fleet_evidence_sha256
      AND runner_fleet_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  cleanup_authority_sha256        TEXT NOT NULL
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
  canary_evidence_sha256          TEXT NOT NULL
    CHECK(length(canary_evidence_sha256) = 64
      AND lower(canary_evidence_sha256) = canary_evidence_sha256
      AND canary_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  portal_readback_evidence_sha256 TEXT NOT NULL
    CHECK(length(portal_readback_evidence_sha256) = 64
      AND lower(portal_readback_evidence_sha256) = portal_readback_evidence_sha256
      AND portal_readback_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  portal_readback_at_ms           INTEGER NOT NULL CHECK(portal_readback_at_ms >= 0),
  portal_readback_ttl_ms          INTEGER NOT NULL
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
  CHECK(portal_readback_at_ms <= issued_at_ms),
  CHECK(expires_at_ms <= portal_readback_at_ms + portal_readback_ttl_ms),
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

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_activation_requirements (
  activation_sha256               TEXT NOT NULL,
  role                            TEXT NOT NULL CHECK(role IN (
    'discovery_worker', 'global_discovery_worker', 'jobs_api', 'managed_runner',
    'original_source_verifier', 'workflow_command_dispatcher',
    'workflow_cleanup_dispatcher', 'workflow_gateway', 'workflow_worker'
  )),
  minimum_ready_instances         INTEGER NOT NULL CHECK(minimum_ready_instances = 1),
  heartbeat_ttl_ms                INTEGER NOT NULL CHECK(heartbeat_ttl_ms BETWEEN 5000 AND 300000),
  dependency_evidence_sha256      TEXT NOT NULL CHECK(
    length(dependency_evidence_sha256) = 64
    AND dependency_evidence_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 8),
  PRIMARY KEY(activation_sha256, role),
  UNIQUE(activation_sha256, ordinal),
  UNIQUE(activation_sha256, role, dependency_evidence_sha256),
  FOREIGN KEY(activation_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256) ON DELETE RESTRICT
);

-- A successor may execute an already-frozen command only when its signed
-- activation explicitly accepts the exact predecessor pair for recovery.
-- This does not make the predecessor admission-active again.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_activation_recovery_acceptances (
  activation_sha256               TEXT NOT NULL,
  recovery_activation_sha256      TEXT NOT NULL,
  recovery_manifest_sha256        TEXT NOT NULL,
  ordinal                         INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(activation_sha256, recovery_activation_sha256, recovery_manifest_sha256),
  UNIQUE(activation_sha256, ordinal),
  CHECK(activation_sha256 <> recovery_activation_sha256),
  FOREIGN KEY(activation_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(recovery_activation_sha256, recovery_manifest_sha256)
    REFERENCES jobs_managed_cloud_activations(activation_sha256, manifest_sha256)
    ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_rollbacks (
  rollback_sha256                 TEXT PRIMARY KEY
    CHECK(length(rollback_sha256) = 64 AND lower(rollback_sha256) = rollback_sha256
      AND rollback_sha256 NOT GLOB '*[^0-9a-f]*'),
  rollback_id                     TEXT NOT NULL UNIQUE CHECK(length(rollback_id) > 0),
  rollback_generation             INTEGER NOT NULL
    CHECK(rollback_generation BETWEEN 1 AND 9007199254740991),
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
  expected_head_revision          INTEGER NOT NULL CHECK(expected_head_revision >= 1),
  expected_transition_sha256      TEXT NOT NULL
    CHECK(length(expected_transition_sha256) = 64
      AND lower(expected_transition_sha256) = expected_transition_sha256
      AND expected_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  from_activation_sha256          TEXT NOT NULL,
  from_manifest_sha256            TEXT NOT NULL,
  to_activation_sha256            TEXT NOT NULL,
  to_manifest_sha256              TEXT NOT NULL,
  evidence_sha256                 TEXT NOT NULL
    CHECK(length(evidence_sha256) = 64 AND lower(evidence_sha256) = evidence_sha256
      AND evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  canonical_rollback_base64url    TEXT NOT NULL CHECK(length(canonical_rollback_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(trust_generation, rollback_generation),
  UNIQUE(
    rollback_sha256, environment, region, channel,
    from_activation_sha256, from_manifest_sha256,
    to_activation_sha256, to_manifest_sha256
  ),
  CHECK(from_activation_sha256 <> to_activation_sha256),
  FOREIGN KEY(from_activation_sha256, from_manifest_sha256, environment, region, channel)
    REFERENCES jobs_managed_cloud_activations(
      activation_sha256, manifest_sha256, environment, region, channel
    ) ON DELETE RESTRICT,
  FOREIGN KEY(to_activation_sha256, to_manifest_sha256, environment, region, channel)
    REFERENCES jobs_managed_cloud_activations(
      activation_sha256, manifest_sha256, environment, region, channel
    ) ON DELETE RESTRICT,
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_revocations (
  revocation_sha256               TEXT PRIMARY KEY
    CHECK(length(revocation_sha256) = 64 AND lower(revocation_sha256) = revocation_sha256
      AND revocation_sha256 NOT GLOB '*[^0-9a-f]*'),
  revocation_id                   TEXT NOT NULL UNIQUE CHECK(length(revocation_id) > 0),
  revocation_generation           INTEGER NOT NULL
    CHECK(revocation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_revocation_sha256   TEXT,
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  subject_kind                    TEXT NOT NULL CHECK(subject_kind IN (
    'activation', 'cohort', 'component', 'manifest', 'release', 'rollback',
    'runtime_grant', 'runtime_instance', 'signing_key', 'trust_policy'
  )),
  subject_id                      TEXT NOT NULL CHECK(length(subject_id) > 0),
  subject_sha256                  TEXT NOT NULL
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  canonical_revocation_base64url  TEXT NOT NULL CHECK(length(canonical_revocation_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  effective_at_ms                 INTEGER NOT NULL CHECK(effective_at_ms >= issued_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(trust_generation, revocation_generation),
  UNIQUE(revocation_sha256, trust_generation),
  UNIQUE(predecessor_revocation_sha256, trust_generation),
  UNIQUE(subject_kind, subject_id, subject_sha256),
  CHECK(
    (revocation_generation = 1 AND predecessor_revocation_sha256 IS NULL)
    OR (revocation_generation > 1 AND predecessor_revocation_sha256 IS NOT NULL)
  ),
  FOREIGN KEY(predecessor_revocation_sha256, trust_generation)
    REFERENCES jobs_managed_cloud_revocations(revocation_sha256, trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_managed_cloud_trust_policies(trust_generation) ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_head_transitions (
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
  transition_kind                 TEXT NOT NULL CHECK(transition_kind IN ('activation', 'rollback')),
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
      AND transition_kind = 'activation')
    OR (head_revision > 1 AND previous_transition_sha256 IS NOT NULL
      AND previous_activation_sha256 IS NOT NULL AND previous_manifest_sha256 IS NOT NULL
      AND previous_trust_generation IS NOT NULL AND previous_channel_sequence IS NOT NULL)
  ),
  CHECK(
    (transition_kind = 'activation' AND authority_sha256 = next_activation_sha256
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

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_heads (
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
  current_transition_sha256       TEXT NOT NULL,
  current_activation_sha256       TEXT NOT NULL,
  current_manifest_sha256         TEXT NOT NULL,
  current_trust_generation        INTEGER NOT NULL
    CHECK(current_trust_generation BETWEEN 1 AND 9007199254740991),
  current_channel_sequence        INTEGER NOT NULL
    CHECK(current_channel_sequence BETWEEN 1 AND 9007199254740991),
  updated_at_ms                   INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  PRIMARY KEY(environment, region, channel),
  FOREIGN KEY(
    current_transition_sha256, environment, region, channel, head_revision,
    current_activation_sha256, current_manifest_sha256,
    current_trust_generation, current_channel_sequence
  ) REFERENCES jobs_managed_cloud_head_transitions(
    transition_sha256, environment, region, channel, head_revision,
    next_activation_sha256, next_manifest_sha256,
    next_trust_generation, next_channel_sequence
  ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_grants (
  grant_id                        TEXT PRIMARY KEY CHECK(
    length(grant_id) BETWEEN 20 AND 128
    AND grant_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  token_sha256                    TEXT NOT NULL UNIQUE
    CHECK(length(token_sha256) = 64 AND lower(token_sha256) = token_sha256
      AND token_sha256 NOT GLOB '*[^0-9a-f]*'),
  grant_token_ciphertext          TEXT CHECK(
    grant_token_ciphertext IS NULL
    OR (length(grant_token_ciphertext) BETWEEN 32 AND 1024
      AND grant_token_ciphertext LIKE 'bluey-jobs:v1:%')
  ),
  issuance_ref                    TEXT NOT NULL UNIQUE CHECK(
    length(issuance_ref) BETWEEN 20 AND 128
    AND issuance_ref NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  environment                     TEXT NOT NULL CHECK(environment IN ('production', 'staging')),
  region                          TEXT NOT NULL CHECK(
    length(region) BETWEEN 1 AND 64
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region, 1, 1) GLOB '[a-z0-9]'
    AND substr(region, -1, 1) GLOB '[a-z0-9]'
  ),
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general', 'shadow')),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL,
  role                            TEXT NOT NULL CHECK(role IN (
    'discovery_worker', 'global_discovery_worker', 'jobs_api', 'managed_runner',
    'original_source_verifier', 'workflow_command_dispatcher',
    'workflow_cleanup_dispatcher', 'workflow_gateway', 'workflow_worker'
  )),
  head_revision                   INTEGER NOT NULL CHECK(head_revision >= 1),
  transition_sha256               TEXT NOT NULL
    CHECK(length(transition_sha256) = 64 AND lower(transition_sha256) = transition_sha256
      AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  artifact_sha256                 TEXT NOT NULL
    CHECK(length(artifact_sha256) = 64 AND lower(artifact_sha256) = artifact_sha256
      AND artifact_sha256 NOT GLOB '*[^0-9a-f]*'),
  config_schema_sha256            TEXT NOT NULL
    CHECK(length(config_schema_sha256) = 64
      AND lower(config_schema_sha256) = config_schema_sha256
      AND config_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  migration_set_sha256            TEXT NOT NULL
    CHECK(length(migration_set_sha256) = 64
      AND lower(migration_set_sha256) = migration_set_sha256
      AND migration_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_set_sha256             TEXT NOT NULL
    CHECK(length(protocol_set_sha256) = 64
      AND lower(protocol_set_sha256) = protocol_set_sha256
      AND protocol_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  task_queue_sha256               TEXT NOT NULL
    CHECK(length(task_queue_sha256) = 64 AND lower(task_queue_sha256) = task_queue_sha256
      AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  failure_converter_sha256        TEXT NOT NULL
    CHECK(length(failure_converter_sha256) = 64
      AND lower(failure_converter_sha256) = failure_converter_sha256
      AND failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  expected_dependency_evidence_sha256 TEXT NOT NULL CHECK(
    length(expected_dependency_evidence_sha256) = 64
    AND expected_dependency_evidence_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  expected_runtime_identity_sha256 TEXT NOT NULL
    CHECK(length(expected_runtime_identity_sha256) = 64
      AND lower(expected_runtime_identity_sha256) = expected_runtime_identity_sha256
      AND expected_runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  expected_worker_id              TEXT NOT NULL CHECK(
    length(expected_worker_id) BETWEEN 20 AND 128
    AND expected_worker_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  authorization_ref               TEXT NOT NULL CHECK(
    length(authorization_ref) BETWEEN 20 AND 128
    AND authorization_ref NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  created_by                      TEXT NOT NULL CHECK(length(created_by) BETWEEN 1 AND 240),
  activation_expires_at_ms        INTEGER NOT NULL CHECK(activation_expires_at_ms > 0),
  expires_at_ms                   INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  created_at_ms                   INTEGER NOT NULL CHECK(created_at_ms >= 0),
  CHECK(expires_at_ms > created_at_ms),
  CHECK(expires_at_ms <= activation_expires_at_ms),
  UNIQUE(
    grant_id, environment, region, channel, activation_sha256, manifest_sha256,
    component_id, role, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256,
    expected_dependency_evidence_sha256,
    expected_runtime_identity_sha256, expected_worker_id, activation_expires_at_ms
  ),
  FOREIGN KEY(
    activation_sha256, manifest_sha256, environment, region, channel,
    task_queue_sha256, failure_converter_sha256
  )
    REFERENCES jobs_managed_cloud_activations(
      activation_sha256, manifest_sha256, environment, region, channel,
      task_queue_sha256, failure_converter_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256, manifest_sha256, activation_expires_at_ms)
    REFERENCES jobs_managed_cloud_activations(
      activation_sha256, manifest_sha256, expires_at_ms
    ) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, component_id, artifact_sha256, config_schema_sha256)
    REFERENCES jobs_managed_cloud_manifest_components(
      manifest_sha256, component_id, artifact_sha256, config_schema_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, component_id, role)
    REFERENCES jobs_managed_cloud_manifest_capabilities(
      manifest_sha256, component_id, capability
    ) ON DELETE RESTRICT,
  FOREIGN KEY(
    manifest_sha256, migration_set_sha256,
    config_schema_sha256, protocol_set_sha256
  ) REFERENCES jobs_managed_cloud_manifests(
    manifest_sha256, migration_set_sha256,
    config_schema_sha256, protocol_set_sha256
  ) ON DELETE RESTRICT,
  FOREIGN KEY(
    transition_sha256, environment, region, channel, head_revision,
    activation_sha256, manifest_sha256
  ) REFERENCES jobs_managed_cloud_head_transitions(
    transition_sha256, environment, region, channel, head_revision,
    next_activation_sha256, next_manifest_sha256
  ) ON DELETE RESTRICT,
  FOREIGN KEY(
    activation_sha256, role, expected_dependency_evidence_sha256
  ) REFERENCES jobs_managed_cloud_activation_requirements(
    activation_sha256, role, dependency_evidence_sha256
  ) ON DELETE RESTRICT,
  FOREIGN KEY(
    manifest_sha256, component_id, role, expected_runtime_identity_sha256
  ) REFERENCES jobs_managed_cloud_manifest_runtime_identities(
    manifest_sha256, component_id, role, runtime_identity_sha256
  ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_grant_revocations (
  grant_id                        TEXT PRIMARY KEY
    REFERENCES jobs_managed_cloud_runtime_grants(grant_id) ON DELETE RESTRICT,
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  revoked_by                      TEXT NOT NULL CHECK(length(revoked_by) > 0),
  revoked_at_ms                   INTEGER NOT NULL CHECK(revoked_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_instances (
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
  ) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX IF NOT EXISTS
  idx_jobs_managed_cloud_runtime_instance_epoch_worker
  ON jobs_managed_cloud_runtime_instances(
    runtime_instance_id, instance_epoch, worker_id);

-- Readiness reads only this fenced current row. Exact heartbeat replay never
-- updates it, so replay cannot extend freshness. A new sequence is an exact
-- old+1 CAS written with database time by the registry transaction.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_heartbeats (
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence              INTEGER NOT NULL
    CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL,
  role                            TEXT NOT NULL,
  worker_id                       TEXT NOT NULL,
  artifact_sha256                 TEXT NOT NULL,
  observed_head_revision          INTEGER NOT NULL
    CHECK(observed_head_revision BETWEEN 1 AND 9007199254740991),
  observed_transition_sha256      TEXT NOT NULL
    CHECK(length(observed_transition_sha256) = 64
      AND lower(observed_transition_sha256) = observed_transition_sha256
      AND observed_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  migration_set_sha256            TEXT NOT NULL
    CHECK(length(migration_set_sha256) = 64 AND lower(migration_set_sha256) = migration_set_sha256
      AND migration_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  config_schema_sha256            TEXT NOT NULL
    CHECK(length(config_schema_sha256) = 64 AND lower(config_schema_sha256) = config_schema_sha256
      AND config_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  protocol_set_sha256             TEXT NOT NULL
    CHECK(length(protocol_set_sha256) = 64 AND lower(protocol_set_sha256) = protocol_set_sha256
      AND protocol_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  task_queue_sha256               TEXT NOT NULL
    CHECK(length(task_queue_sha256) = 64 AND lower(task_queue_sha256) = task_queue_sha256
      AND task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  failure_converter_sha256        TEXT NOT NULL
    CHECK(length(failure_converter_sha256) = 64
      AND lower(failure_converter_sha256) = failure_converter_sha256
      AND failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  dependency_evidence_sha256      TEXT NOT NULL
    CHECK(length(dependency_evidence_sha256) = 64
      AND lower(dependency_evidence_sha256) = dependency_evidence_sha256
      AND dependency_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  health_state                    TEXT NOT NULL CHECK(health_state IN ('degraded', 'draining', 'ready')),
  reason_code                     TEXT CHECK(reason_code IS NULL OR reason_code IN (
    'artifact_mismatch', 'config_mismatch', 'dependency_unavailable', 'draining',
    'head_mismatch', 'migration_mismatch', 'probe_failed', 'protocol_mismatch', 'startup'
  )),
  heartbeat_at_ms                 INTEGER NOT NULL CHECK(heartbeat_at_ms >= 0),
  PRIMARY KEY(runtime_instance_id, instance_epoch),
  FOREIGN KEY(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, observed_head_revision, observed_transition_sha256,
    artifact_sha256, config_schema_sha256, migration_set_sha256,
    protocol_set_sha256, task_queue_sha256, failure_converter_sha256,
    dependency_evidence_sha256
  ) REFERENCES jobs_managed_cloud_runtime_instances(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256, dependency_evidence_sha256
  ) ON DELETE RESTRICT,
  CHECK((health_state = 'ready' AND reason_code IS NULL)
    OR (health_state = 'draining' AND reason_code = 'draining')
    OR (health_state = 'degraded' AND reason_code IS NOT NULL AND reason_code <> 'draining'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_managed_cloud_runtime_heartbeat_readiness
  ON jobs_managed_cloud_runtime_heartbeats(
    activation_sha256, role, health_state, heartbeat_at_ms DESC
  );

-- This bounded audit retains only the newest 64 committed transitions for an
-- instance epoch. Its guarded pruning can never affect readiness or resurrect
-- an older observation.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_runtime_heartbeat_audit (
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence              INTEGER NOT NULL
    CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL,
  role                            TEXT NOT NULL,
  worker_id                       TEXT NOT NULL,
  artifact_sha256                 TEXT NOT NULL,
  observed_head_revision          INTEGER NOT NULL
    CHECK(observed_head_revision BETWEEN 1 AND 9007199254740991),
  observed_transition_sha256      TEXT NOT NULL,
  migration_set_sha256            TEXT NOT NULL,
  config_schema_sha256            TEXT NOT NULL,
  protocol_set_sha256             TEXT NOT NULL,
  task_queue_sha256               TEXT NOT NULL,
  failure_converter_sha256        TEXT NOT NULL,
  dependency_evidence_sha256      TEXT NOT NULL,
  health_state                    TEXT NOT NULL CHECK(health_state IN ('degraded', 'draining', 'ready')),
  reason_code                     TEXT,
  heartbeat_at_ms                 INTEGER NOT NULL CHECK(heartbeat_at_ms >= 0),
  PRIMARY KEY(runtime_instance_id, instance_epoch, heartbeat_sequence),
  CHECK((health_state = 'ready' AND reason_code IS NULL)
    OR (health_state = 'draining' AND reason_code = 'draining')
    OR (health_state = 'degraded' AND reason_code IN (
      'artifact_mismatch', 'config_mismatch', 'dependency_unavailable',
      'head_mismatch', 'migration_mismatch', 'probe_failed',
      'protocol_mismatch', 'startup'
    ))),
  FOREIGN KEY(runtime_instance_id, instance_epoch)
    REFERENCES jobs_managed_cloud_runtime_heartbeats(runtime_instance_id, instance_epoch)
    ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_workflow_bindings (
  command_id                      TEXT PRIMARY KEY CHECK(
    length(command_id) BETWEEN 20 AND 128
    AND command_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  binding_sha256                  TEXT NOT NULL UNIQUE
    CHECK(length(binding_sha256) = 64 AND lower(binding_sha256) = binding_sha256
      AND binding_sha256 NOT GLOB '*[^0-9a-f]*'),
  account_id_hmac_sha256          TEXT NOT NULL
    CHECK(length(account_id_hmac_sha256) = 64 AND lower(account_id_hmac_sha256) = account_id_hmac_sha256
      AND account_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  application_id_hmac_sha256      TEXT NOT NULL
    CHECK(length(application_id_hmac_sha256) = 64 AND lower(application_id_hmac_sha256) = application_id_hmac_sha256
      AND application_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  run_id_hmac_sha256              TEXT NOT NULL
    CHECK(length(run_id_hmac_sha256) = 64 AND lower(run_id_hmac_sha256) = run_id_hmac_sha256
      AND run_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  workflow_id_hmac_sha256         TEXT NOT NULL
    CHECK(length(workflow_id_hmac_sha256) = 64 AND lower(workflow_id_hmac_sha256) = workflow_id_hmac_sha256
      AND workflow_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  environment                     TEXT NOT NULL,
  region                          TEXT NOT NULL CHECK(
    length(region) BETWEEN 1 AND 64
    AND region NOT GLOB '*[^a-z0-9-]*'
    AND substr(region, 1, 1) GLOB '[a-z0-9]'
    AND substr(region, -1, 1) GLOB '[a-z0-9]'
  ),
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general')),
  head_revision                   INTEGER NOT NULL CHECK(head_revision >= 1),
  transition_sha256               TEXT NOT NULL,
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  cohort_sha256                   TEXT NOT NULL,
  trust_generation                INTEGER NOT NULL CHECK(trust_generation >= 1),
  channel_sequence                INTEGER NOT NULL CHECK(channel_sequence >= 1),
  release_id                      TEXT NOT NULL CHECK(length(release_id) > 0),
  release_sequence                INTEGER NOT NULL CHECK(release_sequence >= 1),
  task_queue_sha256               TEXT NOT NULL,
  failure_converter_sha256        TEXT NOT NULL,
  readiness_sha256               TEXT NOT NULL
    CHECK(length(readiness_sha256) = 64 AND lower(readiness_sha256) = readiness_sha256
      AND readiness_sha256 NOT GLOB '*[^0-9a-f]*'),
  resolved_at_ms                 INTEGER NOT NULL CHECK(resolved_at_ms >= 0),
  release_memo_base64url          TEXT NOT NULL CHECK(
    length(release_memo_base64url) BETWEEN 1 AND 174763
    AND release_memo_base64url NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  release_memo_sha256             TEXT NOT NULL CHECK(
    length(release_memo_sha256) = 64
    AND release_memo_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  activation_expires_at_ms        INTEGER NOT NULL CHECK(activation_expires_at_ms > 0),
  bound_at_ms                     INTEGER NOT NULL CHECK(bound_at_ms >= 0),
  CHECK(resolved_at_ms < activation_expires_at_ms),
  CHECK(bound_at_ms >= resolved_at_ms),
  UNIQUE(account_id_hmac_sha256, command_id),
  UNIQUE(command_id, binding_sha256),
  UNIQUE(command_id, binding_sha256, release_memo_sha256),
  UNIQUE(
    command_id, binding_sha256, release_memo_base64url, release_memo_sha256
  ),
  FOREIGN KEY(command_id)
    REFERENCES jobs_workflow_commands(id) ON DELETE CASCADE,
  FOREIGN KEY(
    transition_sha256, environment, region, channel, head_revision,
    activation_sha256, manifest_sha256, trust_generation, channel_sequence
  ) REFERENCES jobs_managed_cloud_head_transitions(
    transition_sha256, environment, region, channel, head_revision,
    next_activation_sha256, next_manifest_sha256,
    next_trust_generation, next_channel_sequence
  ) ON DELETE RESTRICT,
  FOREIGN KEY(cohort_sha256)
    REFERENCES jobs_managed_cloud_cohorts(cohort_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(
    activation_sha256, manifest_sha256,
    task_queue_sha256, failure_converter_sha256
  ) REFERENCES jobs_managed_cloud_activations(
    activation_sha256, manifest_sha256,
    task_queue_sha256, failure_converter_sha256
  ) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256, manifest_sha256, cohort_sha256)
    REFERENCES jobs_managed_cloud_activations(
      activation_sha256, manifest_sha256, cohort_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, release_id, release_sequence)
    REFERENCES jobs_managed_cloud_manifests(
      manifest_sha256, release_id, release_sequence
    ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_request_start_authorities (
  attempt_id                       TEXT PRIMARY KEY CHECK(
    length(attempt_id) BETWEEN 20 AND 128
    AND attempt_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  account_id                       TEXT NOT NULL,
  command_id                       TEXT NOT NULL CHECK(
    length(command_id) BETWEEN 20 AND 128
    AND command_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  fence                            INTEGER NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  event_phase                      TEXT NOT NULL DEFAULT 'request_started'
    CHECK(event_phase = 'request_started'),
  binding_sha256                   TEXT NOT NULL CHECK(
    length(binding_sha256) = 64 AND lower(binding_sha256) = binding_sha256
    AND binding_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  gateway_authority_base64url      TEXT NOT NULL CHECK(
    length(gateway_authority_base64url) BETWEEN 1 AND 174763
    AND gateway_authority_base64url NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  gateway_authority_sha256         TEXT NOT NULL CHECK(
    length(gateway_authority_sha256) = 64
    AND lower(gateway_authority_sha256) = gateway_authority_sha256
    AND gateway_authority_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  authorized_at_ms                 INTEGER NOT NULL
    CHECK(authorized_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(command_id),
  UNIQUE(command_id, fence),
  UNIQUE(attempt_id, account_id, command_id, fence),
  FOREIGN KEY(attempt_id, account_id, command_id, fence)
    REFERENCES jobs_workflow_command_attempts(id, account_id, command_id, fence)
    ON DELETE CASCADE,
  FOREIGN KEY(attempt_id, event_phase)
    REFERENCES jobs_workflow_command_attempt_events(attempt_id, event_phase)
    ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY(command_id, binding_sha256)
    REFERENCES jobs_managed_cloud_workflow_bindings(command_id, binding_sha256)
    ON DELETE CASCADE
);

-- One immutable receipt is committed with the irreversible prepared ->
-- click_started boundary. It preserves the exact latest request authority so
-- response-loss replay never needs to consult a later mutable release head.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_irreversible_effect_receipts (
  run_id                           TEXT NOT NULL CHECK(
    length(run_id) BETWEEN 20 AND 128
    AND run_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  fence                            INTEGER NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  account_id                       TEXT NOT NULL CHECK(
    length(account_id) BETWEEN 1 AND 240),
  application_id                   TEXT NOT NULL CHECK(
    length(application_id) BETWEEN 1 AND 240),
  workflow_request_id              TEXT NOT NULL CHECK(
    length(workflow_request_id) = 45
    AND substr(workflow_request_id, 1, 9) = 'wfreq-v2-'
    AND substr(workflow_request_id, 10) NOT GLOB '*[^0-9a-f-]*'
    AND length(replace(substr(workflow_request_id, 10), '-', '')) = 32
    AND substr(workflow_request_id, 18, 1) = '-'
    AND substr(workflow_request_id, 23, 1) = '-'
    AND substr(workflow_request_id, 24, 1) = '5'
    AND substr(workflow_request_id, 28, 1) = '-'
    AND substr(workflow_request_id, 29, 1) GLOB '[89ab]'
    AND substr(workflow_request_id, 33, 1) = '-'),
  request_command_id               TEXT NOT NULL CHECK(
    length(request_command_id) BETWEEN 20 AND 128
    AND request_command_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  execution_command_id             TEXT NOT NULL CHECK(
    length(execution_command_id) BETWEEN 20 AND 128
    AND execution_command_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  binding_sha256                   TEXT NOT NULL CHECK(
    length(binding_sha256) = 64 AND lower(binding_sha256) = binding_sha256
    AND binding_sha256 NOT GLOB '*[^0-9a-f]*'),
  release_memo_base64url           TEXT NOT NULL CHECK(
    length(release_memo_base64url) BETWEEN 1 AND 174763
    AND release_memo_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  release_sha256                   TEXT NOT NULL CHECK(
    length(release_sha256) = 64 AND lower(release_sha256) = release_sha256
    AND release_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_instance_id              TEXT NOT NULL CHECK(
    length(runtime_instance_id) BETWEEN 20 AND 128
    AND runtime_instance_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  runtime_instance_epoch           INTEGER NOT NULL
    CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  worker_id                        TEXT NOT NULL CHECK(
    length(worker_id) BETWEEN 20 AND 128
    AND worker_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  gateway_authority_base64url      TEXT NOT NULL CHECK(
    length(gateway_authority_base64url) BETWEEN 1 AND 174763
    AND gateway_authority_base64url NOT GLOB '*[^A-Za-z0-9_-]*'),
  gateway_authority_sha256         TEXT NOT NULL CHECK(
    length(gateway_authority_sha256) = 64
    AND lower(gateway_authority_sha256) = gateway_authority_sha256
    AND gateway_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  receipt_sha256                   TEXT NOT NULL UNIQUE CHECK(
    length(receipt_sha256) = 64 AND lower(receipt_sha256) = receipt_sha256
    AND receipt_sha256 NOT GLOB '*[^0-9a-f]*'),
  committed_at_ms                  INTEGER NOT NULL
    CHECK(committed_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(run_id, fence),
  FOREIGN KEY(run_id) REFERENCES jobs_execution_leases(run_id) ON DELETE CASCADE,
  FOREIGN KEY(request_command_id)
    REFERENCES jobs_workflow_commands(id) ON DELETE CASCADE,
  FOREIGN KEY(execution_command_id, binding_sha256,
    release_memo_base64url, release_sha256)
    REFERENCES jobs_managed_cloud_workflow_bindings(
      command_id, binding_sha256, release_memo_base64url, release_memo_sha256)
    ON DELETE CASCADE,
  FOREIGN KEY(runtime_instance_id, runtime_instance_epoch, worker_id)
    REFERENCES jobs_managed_cloud_runtime_instances(
      runtime_instance_id, instance_epoch, worker_id) ON DELETE RESTRICT
);

DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_irreversible_receipt_insert;
CREATE TRIGGER trg_jobs_managed_cloud_irreversible_receipt_insert
BEFORE INSERT ON jobs_managed_cloud_irreversible_effect_receipts
WHEN NOT EXISTS(
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
     AND command.managed_cloud_authority_required=1
     AND command.first_request_started_at_ms IS NOT NULL
     AND EXISTS(
       SELECT 1 FROM jobs_workflow_command_attempt_events event
        WHERE event.command_id=command.id AND event.event_kind='request_started')
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud irreversible receipt is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_irreversible_receipt_no_update
BEFORE UPDATE ON jobs_managed_cloud_irreversible_effect_receipts BEGIN
  SELECT RAISE(ABORT, 'managed cloud irreversible receipt is immutable');
END;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_irreversible_receipt_no_delete;
CREATE TRIGGER trg_jobs_managed_cloud_irreversible_receipt_no_delete
BEFORE DELETE ON jobs_managed_cloud_irreversible_effect_receipts
WHEN NOT EXISTS(
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id=OLD.account_id
     AND NOT EXISTS(SELECT 1 FROM accounts account WHERE account.id=OLD.account_id)
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud irreversible receipt is immutable');
END;

-- Immutable authorities and runtime evidence. Heads are the sole mutable
-- release objects and may only advance one exact revision at a time.
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_signature_sets_no_update
BEFORE UPDATE ON jobs_managed_cloud_signature_sets BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature set is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_signature_sets_no_delete
BEFORE DELETE ON jobs_managed_cloud_signature_sets BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature set is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_signatures_no_update
BEFORE UPDATE ON jobs_managed_cloud_signatures BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_signatures_no_delete
BEFORE DELETE ON jobs_managed_cloud_signatures BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_trust_policies_no_update
BEFORE UPDATE ON jobs_managed_cloud_trust_policies BEGIN
  SELECT RAISE(ABORT, 'managed cloud trust policy is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_trust_policies_no_delete
BEFORE DELETE ON jobs_managed_cloud_trust_policies BEGIN
  SELECT RAISE(ABORT, 'managed cloud trust policy is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_trust_keys_no_update
BEFORE UPDATE ON jobs_managed_cloud_trust_keys BEGIN
  SELECT RAISE(ABORT, 'managed cloud trust key is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_trust_keys_no_delete
BEFORE DELETE ON jobs_managed_cloud_trust_keys BEGIN
  SELECT RAISE(ABORT, 'managed cloud trust key is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_manifests_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifests BEGIN
  SELECT RAISE(ABORT, 'managed cloud manifest is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_manifests_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifests BEGIN
  SELECT RAISE(ABORT, 'managed cloud manifest is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_components_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_components BEGIN
  SELECT RAISE(ABORT, 'managed cloud component is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_components_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_components BEGIN
  SELECT RAISE(ABORT, 'managed cloud component is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_capabilities_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_capabilities BEGIN
  SELECT RAISE(ABORT, 'managed cloud component capability is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_capabilities_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_capabilities BEGIN
  SELECT RAISE(ABORT, 'managed cloud component capability is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_identities_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_runtime_identities BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime identity is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_identities_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_runtime_identities BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime identity is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_protocols_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_protocols BEGIN
  SELECT RAISE(ABORT, 'managed cloud protocol is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_protocols_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_protocols BEGIN
  SELECT RAISE(ABORT, 'managed cloud protocol is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_cohorts_no_update
BEFORE UPDATE ON jobs_managed_cloud_cohorts BEGIN
  SELECT RAISE(ABORT, 'managed cloud cohort is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_cohorts_no_delete
BEFORE DELETE ON jobs_managed_cloud_cohorts BEGIN
  SELECT RAISE(ABORT, 'managed cloud cohort is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_cohort_members_no_update
BEFORE UPDATE ON jobs_managed_cloud_cohort_members BEGIN
  SELECT RAISE(ABORT, 'managed cloud cohort member is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_cohort_members_delete_guard
BEFORE DELETE ON jobs_managed_cloud_cohort_members
WHEN EXISTS(SELECT 1 FROM accounts WHERE id = OLD.account_id) BEGIN
  SELECT RAISE(ABORT, 'managed cloud cohort member deletion requires account deletion');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_activations_no_update
BEFORE UPDATE ON jobs_managed_cloud_activations BEGIN
  SELECT RAISE(ABORT, 'managed cloud activation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_activations_no_delete
BEFORE DELETE ON jobs_managed_cloud_activations BEGIN
  SELECT RAISE(ABORT, 'managed cloud activation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_requirements_no_update
BEFORE UPDATE ON jobs_managed_cloud_activation_requirements BEGIN
  SELECT RAISE(ABORT, 'managed cloud activation requirement is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_requirements_no_delete
BEFORE DELETE ON jobs_managed_cloud_activation_requirements BEGIN
  SELECT RAISE(ABORT, 'managed cloud activation requirement is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_recovery_acceptances_no_update
BEFORE UPDATE ON jobs_managed_cloud_activation_recovery_acceptances BEGIN
  SELECT RAISE(ABORT, 'managed cloud recovery acceptance is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_recovery_acceptances_no_delete
BEFORE DELETE ON jobs_managed_cloud_activation_recovery_acceptances BEGIN
  SELECT RAISE(ABORT, 'managed cloud recovery acceptance is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_rollbacks_no_update
BEFORE UPDATE ON jobs_managed_cloud_rollbacks BEGIN
  SELECT RAISE(ABORT, 'managed cloud rollback is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_rollbacks_no_delete
BEFORE DELETE ON jobs_managed_cloud_rollbacks BEGIN
  SELECT RAISE(ABORT, 'managed cloud rollback is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_revocations_chain
BEFORE INSERT ON jobs_managed_cloud_revocations
WHEN
  (NEW.revocation_generation = 1 AND (
    NEW.predecessor_revocation_sha256 IS NOT NULL
    OR EXISTS(
      SELECT 1 FROM jobs_managed_cloud_revocations existing
       WHERE existing.trust_generation = NEW.trust_generation
    )
  ))
  OR (NEW.revocation_generation > 1 AND NOT EXISTS(
    SELECT 1 FROM jobs_managed_cloud_revocations predecessor
     WHERE predecessor.revocation_sha256 = NEW.predecessor_revocation_sha256
       AND predecessor.revocation_generation = NEW.revocation_generation - 1
       AND predecessor.trust_generation = NEW.trust_generation
  ))
BEGIN
  SELECT RAISE(ABORT, 'managed cloud revocation chain is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_revocations_no_update
BEFORE UPDATE ON jobs_managed_cloud_revocations BEGIN
  SELECT RAISE(ABORT, 'managed cloud revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_revocations_no_delete
BEFORE DELETE ON jobs_managed_cloud_revocations BEGIN
  SELECT RAISE(ABORT, 'managed cloud revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_transitions_no_update
BEFORE UPDATE ON jobs_managed_cloud_head_transitions BEGIN
  SELECT RAISE(ABORT, 'managed cloud head transition is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_transitions_no_delete
BEFORE DELETE ON jobs_managed_cloud_head_transitions BEGIN
  SELECT RAISE(ABORT, 'managed cloud head transition is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_heads_monotonic
BEFORE UPDATE ON jobs_managed_cloud_heads
WHEN NEW.head_revision <> OLD.head_revision + 1
  OR NEW.current_transition_sha256 = OLD.current_transition_sha256
  OR NEW.environment <> OLD.environment OR NEW.region <> OLD.region
  OR NEW.channel <> OLD.channel BEGIN
  SELECT RAISE(ABORT, 'managed cloud head must advance monotonically');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_heads_no_delete
BEFORE DELETE ON jobs_managed_cloud_heads BEGIN
  SELECT RAISE(ABORT, 'managed cloud head cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_grants_immutable_fields
BEFORE UPDATE OF
  grant_id, token_sha256, issuance_ref, environment, region, channel,
  activation_sha256, manifest_sha256, component_id, role, head_revision,
  transition_sha256, artifact_sha256, config_schema_sha256, migration_set_sha256,
  protocol_set_sha256, task_queue_sha256, failure_converter_sha256,
  expected_dependency_evidence_sha256, expected_runtime_identity_sha256,
  expected_worker_id, authorization_ref, created_by, activation_expires_at_ms,
  expires_at_ms, created_at_ms
ON jobs_managed_cloud_runtime_grants BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant authority is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_grants_secret_scrub
BEFORE UPDATE OF grant_token_ciphertext ON jobs_managed_cloud_runtime_grants
WHEN OLD.grant_token_ciphertext IS NULL OR NEW.grant_token_ciphertext IS NOT NULL BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant secret can only be scrubbed');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_grants_no_delete
BEFORE DELETE ON jobs_managed_cloud_runtime_grants BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_grant_revocations_no_update
BEFORE UPDATE ON jobs_managed_cloud_runtime_grant_revocations BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_grant_revocations_no_delete
BEFORE DELETE ON jobs_managed_cloud_runtime_grant_revocations BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_instances_no_update
BEFORE UPDATE ON jobs_managed_cloud_runtime_instances BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime instance is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_instances_no_delete
BEFORE DELETE ON jobs_managed_cloud_runtime_instances BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime instance is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_heartbeats_fenced_update
BEFORE UPDATE ON jobs_managed_cloud_runtime_heartbeats
WHEN NEW.runtime_instance_id <> OLD.runtime_instance_id
  OR NEW.instance_epoch <> OLD.instance_epoch
  OR NEW.heartbeat_sequence <> OLD.heartbeat_sequence + 1
  OR NEW.heartbeat_at_ms < OLD.heartbeat_at_ms BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat fence is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_heartbeats_no_delete
BEFORE DELETE ON jobs_managed_cloud_runtime_heartbeats BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_heartbeats_sequence
BEFORE INSERT ON jobs_managed_cloud_runtime_heartbeats
WHEN NEW.heartbeat_sequence <> 1 BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat sequence is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_heartbeat_audit_matches_current
BEFORE INSERT ON jobs_managed_cloud_runtime_heartbeat_audit
WHEN NOT EXISTS (
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
     AND current.reason_code IS NEW.reason_code
     AND current.heartbeat_at_ms = NEW.heartbeat_at_ms
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud heartbeat audit does not match current fence');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_heartbeat_audit_no_update
BEFORE UPDATE ON jobs_managed_cloud_runtime_heartbeat_audit BEGIN
  SELECT RAISE(ABORT, 'managed cloud heartbeat audit is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_runtime_heartbeat_audit_bounded_delete
BEFORE DELETE ON jobs_managed_cloud_runtime_heartbeat_audit
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats current
   WHERE current.runtime_instance_id = OLD.runtime_instance_id
     AND current.instance_epoch = OLD.instance_epoch
     AND OLD.heartbeat_sequence <= current.heartbeat_sequence - 64
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud heartbeat audit pruning is unsafe');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_workflow_bindings_no_update
BEFORE UPDATE ON jobs_managed_cloud_workflow_bindings BEGIN
  SELECT RAISE(ABORT, 'managed cloud workflow binding is immutable');
END;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_workflow_bindings_no_delete;
CREATE TRIGGER trg_jobs_managed_cloud_workflow_bindings_no_delete
BEFORE DELETE ON jobs_managed_cloud_workflow_bindings
WHEN NOT EXISTS(
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   JOIN jobs_workflow_commands command ON command.id=OLD.command_id
   WHERE token.account_id=command.account_id
     AND NOT EXISTS(SELECT 1 FROM accounts account WHERE account.id=command.account_id)
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud workflow binding is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_request_start_authorities_no_update
BEFORE UPDATE ON jobs_managed_cloud_request_start_authorities BEGIN
  SELECT RAISE(ABORT, 'managed cloud request-start authority is immutable');
END;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_request_start_authorities_no_delete;
CREATE TRIGGER trg_jobs_managed_cloud_request_start_authorities_no_delete
BEFORE DELETE ON jobs_managed_cloud_request_start_authorities
WHEN NOT EXISTS(
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id=OLD.account_id
     AND NOT EXISTS(SELECT 1 FROM accounts account WHERE account.id=OLD.account_id)
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud request-start authority is immutable');
END;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_command_hard_delete;
CREATE TRIGGER trg_jobs_managed_cloud_command_hard_delete
BEFORE DELETE ON jobs_workflow_commands
WHEN EXISTS(
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id=OLD.account_id
     AND NOT EXISTS(SELECT 1 FROM accounts account WHERE account.id=OLD.account_id)
)
BEGIN
  DELETE FROM jobs_execution_leases
   WHERE account_id=OLD.account_id
     AND (managed_cloud_request_command_id=OLD.id
       OR managed_cloud_execution_command_id=OLD.id);
  DELETE FROM jobs_workflow_cleanup_targets
   WHERE account_id=OLD.account_id AND start_command_id=OLD.id;
  DELETE FROM jobs_managed_cloud_request_start_authorities
   WHERE account_id=OLD.account_id AND command_id=OLD.id;
  DELETE FROM jobs_managed_cloud_workflow_bindings WHERE command_id=OLD.id;
END;

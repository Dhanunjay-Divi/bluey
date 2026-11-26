-- Target: SQLite
-- Replay-safe, tenant-bound original-source verification authority. This
-- migration deliberately seeds no assignment, receipt, or current authority.

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_postings_account_id_unique
  ON jobs_postings(account_id, id);

-- Additive managed-cloud v2 extensions preserve every v1 cardinality and
-- CHECK constraint. A v2 manifest carries exactly one extra protocol and
-- exactly one verifier runtime identity in these immutable companion tables.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_source_verification_protocols (
  manifest_sha256                 TEXT PRIMARY KEY,
  protocol_id                     TEXT NOT NULL DEFAULT 'source_verification'
    CHECK(protocol_id = 'source_verification'),
  protocol_version                INTEGER NOT NULL CHECK(protocol_version = 1),
  schema_sha256                   TEXT NOT NULL
    CHECK(length(schema_sha256) = 64
      AND schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  ordinal                         INTEGER NOT NULL CHECK(ordinal = 9),
  UNIQUE(manifest_sha256, protocol_id),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_original_source_verifier_identities (
  manifest_sha256                 TEXT PRIMARY KEY,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL DEFAULT 'original_source_verifier'
    CHECK(role = 'original_source_verifier'),
  runtime_measurement_sha256      TEXT NOT NULL
    CHECK(length(runtime_measurement_sha256) = 64
      AND runtime_measurement_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_identity_sha256         TEXT NOT NULL
    CHECK(length(runtime_identity_sha256) = 64
      AND runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  ordinal                         INTEGER NOT NULL CHECK(ordinal = 0),
  UNIQUE(manifest_sha256, component_id, role),
  UNIQUE(manifest_sha256, runtime_identity_sha256),
  UNIQUE(manifest_sha256, component_id, role, runtime_identity_sha256),
  FOREIGN KEY(manifest_sha256, component_id, runtime_measurement_sha256)
    REFERENCES jobs_managed_cloud_manifest_components(
      manifest_sha256, component_id, runtime_measurement_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, component_id, role)
    REFERENCES jobs_managed_cloud_manifest_capabilities(
      manifest_sha256, component_id, capability) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_runtime_grants (
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
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
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
  ) REFERENCES jobs_managed_cloud_manifest_original_source_verifier_identities(
    manifest_sha256, component_id, role, runtime_identity_sha256
  ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_grant_revocations (
  grant_id                        TEXT PRIMARY KEY
    REFERENCES jobs_managed_cloud_original_source_verifier_runtime_grants(grant_id) ON DELETE RESTRICT,
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  revoked_by                      TEXT NOT NULL CHECK(length(revoked_by) > 0),
  revoked_at_ms                   INTEGER NOT NULL CHECK(revoked_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_runtime_instances (
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
  ) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX IF NOT EXISTS
  idx_jobs_managed_cloud_original_source_verifier_runtime_instance_epoch_worker
  ON jobs_managed_cloud_original_source_verifier_runtime_instances(
    runtime_instance_id, instance_epoch, worker_id);

-- Readiness reads only this fenced current row. Exact heartbeat replay never
-- updates it, so replay cannot extend freshness. A new sequence is an exact
-- old+1 CAS written with database time by the registry transaction.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_runtime_heartbeats (
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence              INTEGER NOT NULL
    CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
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
  ) REFERENCES jobs_managed_cloud_original_source_verifier_runtime_instances(
    runtime_instance_id, instance_epoch, activation_sha256, manifest_sha256,
    component_id, role, worker_id, head_revision, transition_sha256, artifact_sha256,
    config_schema_sha256, migration_set_sha256, protocol_set_sha256,
    task_queue_sha256, failure_converter_sha256, dependency_evidence_sha256
  ) ON DELETE RESTRICT,
  CHECK((health_state = 'ready' AND reason_code IS NULL)
    OR (health_state = 'draining' AND reason_code = 'draining')
    OR (health_state = 'degraded' AND reason_code IS NOT NULL AND reason_code <> 'draining'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_managed_cloud_original_source_verifier_runtime_heartbeat_readiness
  ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats(
    activation_sha256, role, health_state, heartbeat_at_ms DESC
  );

-- This bounded audit retains only the newest 64 committed transitions for an
-- instance epoch. Its guarded pruning can never affect readiness or resurrect
-- an older observation.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit (
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  heartbeat_sequence              INTEGER NOT NULL
    CHECK(heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
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
    REFERENCES jobs_managed_cloud_original_source_verifier_runtime_heartbeats(runtime_instance_id, instance_epoch)
    ON DELETE RESTRICT
);


CREATE TABLE IF NOT EXISTS jobs_original_source_verification_assignments (
  assignment_id                    TEXT PRIMARY KEY
    CHECK(length(assignment_id) BETWEEN 20 AND 128
      AND assignment_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  account_id                       TEXT NOT NULL,
  job_id                           TEXT NOT NULL,
  subject_schema_version           INTEGER NOT NULL DEFAULT 1
    CHECK(subject_schema_version = 1),
  assignment_generation            INTEGER NOT NULL
    CHECK(assignment_generation BETWEEN 1 AND 9007199254740991),
  assignment_sha256                TEXT NOT NULL UNIQUE
    CHECK(length(assignment_sha256) = 64
      AND assignment_sha256 NOT GLOB '*[^0-9a-f]*'),
  predecessor_assignment_sha256    TEXT
    CHECK(predecessor_assignment_sha256 IS NULL OR
      (length(predecessor_assignment_sha256) = 64
        AND predecessor_assignment_sha256 NOT GLOB '*[^0-9a-f]*')),
  canonical_assignment_json        TEXT NOT NULL
    CHECK(length(CAST(canonical_assignment_json AS BLOB)) BETWEEN 2 AND 65536
      AND json_valid(canonical_assignment_json)),
  managed_authority_sha256         TEXT NOT NULL
    CHECK(length(managed_authority_sha256) = 64
      AND managed_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_managed_authority_json TEXT NOT NULL
    CHECK(length(CAST(canonical_managed_authority_json AS BLOB)) BETWEEN 2 AND 65536
      AND json_valid(canonical_managed_authority_json)),
  managed_environment              TEXT NOT NULL CHECK(managed_environment IN ('production')),
  managed_region                   TEXT NOT NULL CHECK(length(managed_region) BETWEEN 1 AND 64),
  managed_channel                  TEXT NOT NULL CHECK(managed_channel IN ('canary', 'general')),
  managed_head_revision            INTEGER NOT NULL
    CHECK(managed_head_revision BETWEEN 1 AND 9007199254740991),
  managed_transition_sha256        TEXT NOT NULL
    CHECK(length(managed_transition_sha256) = 64
      AND managed_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_activation_sha256        TEXT NOT NULL
    CHECK(length(managed_activation_sha256) = 64
      AND managed_activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_manifest_sha256          TEXT NOT NULL
    CHECK(length(managed_manifest_sha256) = 64
      AND managed_manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_cohort_sha256            TEXT NOT NULL
    CHECK(length(managed_cohort_sha256) = 64
      AND managed_cohort_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_trust_generation         INTEGER NOT NULL
    CHECK(managed_trust_generation BETWEEN 1 AND 9007199254740991),
  managed_channel_sequence         INTEGER NOT NULL
    CHECK(managed_channel_sequence BETWEEN 1 AND 9007199254740991),
  managed_release_id               TEXT NOT NULL CHECK(length(managed_release_id) BETWEEN 1 AND 128),
  managed_release_sequence         INTEGER NOT NULL
    CHECK(managed_release_sequence BETWEEN 1 AND 9007199254740991),
  managed_task_queue_sha256        TEXT NOT NULL
    CHECK(length(managed_task_queue_sha256) = 64
      AND managed_task_queue_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_failure_converter_sha256 TEXT NOT NULL
    CHECK(length(managed_failure_converter_sha256) = 64
      AND managed_failure_converter_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_activation_expires_at_ms INTEGER NOT NULL
    CHECK(managed_activation_expires_at_ms BETWEEN 1 AND 9007199254740991),
  managed_source_protocol_schema_sha256 TEXT NOT NULL
    CHECK(length(managed_source_protocol_schema_sha256) = 64
      AND managed_source_protocol_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_runtime_identity_sha256  TEXT NOT NULL
    CHECK(length(managed_runtime_identity_sha256) = 64
      AND managed_runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_dependency_evidence_sha256 TEXT NOT NULL
    CHECK(length(managed_dependency_evidence_sha256) = 64
      AND managed_dependency_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_heartbeat_ttl_ms         INTEGER NOT NULL
    CHECK(managed_heartbeat_ttl_ms BETWEEN 1 AND 9007199254740991),
  subject_sha256                   TEXT NOT NULL
    CHECK(length(subject_sha256) = 64
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_subject_json           TEXT NOT NULL
    CHECK(length(CAST(canonical_subject_json AS BLOB)) BETWEEN 2 AND 131072
      AND json_valid(canonical_subject_json)),
  target_kind                      TEXT NOT NULL DEFAULT 'original_source'
    CHECK(target_kind = 'original_source'),
  state                            TEXT NOT NULL
    CHECK(state IN ('pending', 'leased', 'retry_wait', 'idle',
                    'quarantined', 'superseded', 'cancelled')),
  attempt_count                    INTEGER NOT NULL DEFAULT 0
    CHECK(attempt_count BETWEEN 0 AND 9007199254740991),
  active_attempt_id                TEXT,
  lease_owner                      TEXT,
  lease_token_sha256               TEXT
    CHECK(lease_token_sha256 IS NULL OR
      (length(lease_token_sha256) = 64
        AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*')),
  lease_expires_at_ms              INTEGER
    CHECK(lease_expires_at_ms IS NULL OR
      lease_expires_at_ms BETWEEN 0 AND 9007199254740991),
  hard_deadline_at_ms              INTEGER
    CHECK(hard_deadline_at_ms IS NULL OR
      hard_deadline_at_ms BETWEEN 0 AND 9007199254740991),
  heartbeat_sequence               INTEGER NOT NULL DEFAULT 0
    CHECK(heartbeat_sequence BETWEEN 0 AND 9007199254740991),
  consecutive_failures             INTEGER NOT NULL DEFAULT 0
    CHECK(consecutive_failures BETWEEN 0 AND 9007199254740991),
  circuit_state                    TEXT NOT NULL DEFAULT 'closed'
    CHECK(circuit_state IN ('closed', 'open', 'half_open')),
  circuit_open_until_ms            INTEGER
    CHECK(circuit_open_until_ms IS NULL OR
      circuit_open_until_ms BETWEEN 0 AND 9007199254740991),
  next_attempt_at_ms               INTEGER NOT NULL
    CHECK(next_attempt_at_ms BETWEEN 0 AND 9007199254740991),
  not_before_at_ms                 INTEGER NOT NULL
    CHECK(not_before_at_ms BETWEEN 0 AND 9007199254740991),
  expires_at_ms                    INTEGER NOT NULL
    CHECK(expires_at_ms BETWEEN 0 AND 9007199254740991),
  attempt_budget                   INTEGER NOT NULL
    CHECK(attempt_budget BETWEEN 1 AND 9007199254740991),
  last_error_code                  TEXT
    CHECK(last_error_code IS NULL OR length(last_error_code) BETWEEN 1 AND 64),
  current_event_sequence           INTEGER NOT NULL DEFAULT 0
    CHECK(current_event_sequence BETWEEN 0 AND 9007199254740991),
  current_event_sha256             TEXT
    CHECK(current_event_sha256 IS NULL OR
      (length(current_event_sha256) = 64
        AND current_event_sha256 NOT GLOB '*[^0-9a-f]*')),
  created_at_ms                    INTEGER NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                    INTEGER NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, job_id, assignment_generation),
  UNIQUE(account_id, job_id, assignment_generation, assignment_sha256),
  UNIQUE(account_id, job_id, assignment_sha256),
  UNIQUE(assignment_id, account_id, job_id, subject_sha256, assignment_generation,
         assignment_sha256, managed_authority_sha256),
  UNIQUE(assignment_id, account_id, job_id, subject_sha256),
  FOREIGN KEY(account_id, job_id)
    REFERENCES jobs_postings(account_id, id) ON DELETE CASCADE,
  FOREIGN KEY(account_id, job_id, predecessor_assignment_sha256)
    REFERENCES jobs_original_source_verification_assignments(
      account_id, job_id, assignment_sha256)
      ON DELETE CASCADE,
  CHECK((assignment_generation = 1 AND predecessor_assignment_sha256 IS NULL)
    OR (assignment_generation > 1 AND predecessor_assignment_sha256 IS NOT NULL)),
  CHECK(not_before_at_ms < expires_at_ms),
  CHECK(expires_at_ms <= managed_activation_expires_at_ms),
  CHECK(next_attempt_at_ms >= not_before_at_ms),
  CHECK(attempt_count <= attempt_budget),
  CHECK(updated_at_ms >= created_at_ms),
  CHECK(
    (state = 'leased'
      AND active_attempt_id IS NOT NULL AND lease_owner IS NOT NULL
      AND lease_token_sha256 IS NOT NULL AND lease_expires_at_ms IS NOT NULL
      AND hard_deadline_at_ms IS NOT NULL
      AND lease_expires_at_ms <= hard_deadline_at_ms)
    OR (state <> 'leased' AND active_attempt_id IS NULL AND lease_owner IS NULL
      AND lease_token_sha256 IS NULL AND lease_expires_at_ms IS NULL
      AND hard_deadline_at_ms IS NULL AND heartbeat_sequence = 0)
  ),
  CHECK((circuit_state = 'open' AND circuit_open_until_ms IS NOT NULL)
    OR (circuit_state <> 'open'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_original_source_assignment_active
  ON jobs_original_source_verification_assignments(account_id, job_id)
  WHERE state NOT IN ('superseded', 'cancelled');
CREATE INDEX IF NOT EXISTS idx_jobs_original_source_assignment_due
  ON jobs_original_source_verification_assignments(
    state, next_attempt_at_ms, circuit_open_until_ms, created_at_ms);

CREATE TABLE IF NOT EXISTS jobs_original_source_verification_attempts (
  attempt_id                       TEXT PRIMARY KEY
    CHECK(length(attempt_id) BETWEEN 20 AND 128
      AND attempt_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  assignment_id                    TEXT NOT NULL,
  account_id                       TEXT NOT NULL,
  job_id                           TEXT NOT NULL,
  subject_sha256                   TEXT NOT NULL
    CHECK(length(subject_sha256) = 64
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  attempt_no                       INTEGER NOT NULL
    CHECK(attempt_no BETWEEN 1 AND 9007199254740991),
  worker_id                        TEXT NOT NULL
    CHECK(length(worker_id) BETWEEN 1 AND 128),
  runtime_instance_id              TEXT NOT NULL
    CHECK(length(runtime_instance_id) BETWEEN 1 AND 128),
  runtime_instance_epoch           INTEGER NOT NULL
    CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  runtime_authority_sha256         TEXT NOT NULL
    CHECK(length(runtime_authority_sha256) = 64
      AND runtime_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_session_token_sha256     TEXT NOT NULL
    CHECK(length(runtime_session_token_sha256) = 64
      AND runtime_session_token_sha256 NOT GLOB '*[^0-9a-f]*'),
  lease_token_sha256               TEXT NOT NULL
    CHECK(length(lease_token_sha256) = 64
      AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*'),
  claimed_at_ms                    INTEGER NOT NULL
    CHECK(claimed_at_ms BETWEEN 0 AND 9007199254740991),
  initial_lease_expires_at_ms      INTEGER NOT NULL
    CHECK(initial_lease_expires_at_ms BETWEEN 0 AND 9007199254740991),
  hard_deadline_at_ms              INTEGER NOT NULL
    CHECK(hard_deadline_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(assignment_id, attempt_no),
  UNIQUE(attempt_id, assignment_id, attempt_no),
  UNIQUE(attempt_id, assignment_id),
  UNIQUE(attempt_id, assignment_id, account_id, job_id, subject_sha256,
         attempt_no, worker_id, runtime_instance_id, runtime_instance_epoch,
         runtime_authority_sha256, runtime_session_token_sha256,
         lease_token_sha256),
  FOREIGN KEY(assignment_id, account_id, job_id, subject_sha256)
    REFERENCES jobs_original_source_verification_assignments(
      assignment_id, account_id, job_id, subject_sha256) ON DELETE CASCADE,
  CHECK(claimed_at_ms < initial_lease_expires_at_ms
    AND initial_lease_expires_at_ms <= hard_deadline_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_original_source_verification_events (
  event_id                         TEXT PRIMARY KEY
    CHECK(length(event_id) BETWEEN 20 AND 128
      AND event_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  assignment_id                    TEXT NOT NULL,
  attempt_id                       TEXT,
  event_sequence                   INTEGER NOT NULL
    CHECK(event_sequence BETWEEN 1 AND 9007199254740991),
  predecessor_event_sha256         TEXT,
  event_kind                       TEXT NOT NULL CHECK(event_kind IN (
    'scheduled', 'claimed', 'heartbeat', 'verified', 'failed',
    'lease_expired', 'circuit_opened', 'circuit_half_opened',
    'circuit_closed', 'quarantined', 'released', 'superseded'
  )),
  heartbeat_sequence               INTEGER
    CHECK(heartbeat_sequence IS NULL OR
      heartbeat_sequence BETWEEN 1 AND 9007199254740991),
  lease_expires_at_ms              INTEGER
    CHECK(lease_expires_at_ms IS NULL OR
      lease_expires_at_ms BETWEEN 0 AND 9007199254740991),
  completion_request_id            TEXT
    CHECK(completion_request_id IS NULL OR
      length(completion_request_id) BETWEEN 1 AND 128),
  completion_request_sha256        TEXT
    CHECK(completion_request_sha256 IS NULL OR
      (length(completion_request_sha256) = 64
        AND completion_request_sha256 NOT GLOB '*[^0-9a-f]*')),
  receipt_sha256                   TEXT
    CHECK(receipt_sha256 IS NULL OR
      (length(receipt_sha256) = 64
        AND receipt_sha256 NOT GLOB '*[^0-9a-f]*')),
  reason_code                      TEXT
    CHECK(reason_code IS NULL OR length(reason_code) BETWEEN 1 AND 64),
  canonical_event_json             TEXT NOT NULL
    CHECK(length(CAST(canonical_event_json AS BLOB)) BETWEEN 2 AND 65536
      AND json_valid(canonical_event_json)),
  event_sha256                     TEXT NOT NULL UNIQUE
    CHECK(length(event_sha256) = 64
      AND event_sha256 NOT GLOB '*[^0-9a-f]*'),
  occurred_at_ms                   INTEGER NOT NULL
    CHECK(occurred_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(assignment_id, event_sequence),
  UNIQUE(assignment_id, event_sequence, event_sha256),
  FOREIGN KEY(assignment_id)
    REFERENCES jobs_original_source_verification_assignments(assignment_id)
      ON DELETE CASCADE,
  FOREIGN KEY(attempt_id, assignment_id)
    REFERENCES jobs_original_source_verification_attempts(attempt_id, assignment_id)
      ON DELETE CASCADE,
  CHECK((completion_request_id IS NULL) = (completion_request_sha256 IS NULL)),
  CHECK((event_sequence = 1 AND predecessor_event_sha256 IS NULL)
    OR (event_sequence > 1 AND predecessor_event_sha256 IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_original_source_event_request
  ON jobs_original_source_verification_events(assignment_id, completion_request_id)
  WHERE completion_request_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS jobs_original_source_verification_observations (
  observation_id                   TEXT PRIMARY KEY
    CHECK(length(observation_id) BETWEEN 20 AND 128
      AND observation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  observation_sha256               TEXT NOT NULL
    CHECK(length(observation_sha256) = 64
      AND observation_sha256 NOT GLOB '*[^0-9a-f]*'),
  assignment_id                    TEXT NOT NULL,
  attempt_id                       TEXT NOT NULL,
  account_id                       TEXT NOT NULL,
  job_id                           TEXT NOT NULL,
  subject_sha256                   TEXT NOT NULL
    CHECK(length(subject_sha256) = 64
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  attempt_no                       INTEGER NOT NULL
    CHECK(attempt_no BETWEEN 1 AND 9007199254740991),
  fence                            INTEGER NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  completion_request_id            TEXT NOT NULL
    CHECK(length(completion_request_id) BETWEEN 1 AND 128),
  completion_request_sha256        TEXT NOT NULL
    CHECK(length(completion_request_sha256) = 64
      AND completion_request_sha256 NOT GLOB '*[^0-9a-f]*'),
  worker_id                        TEXT NOT NULL CHECK(length(worker_id) BETWEEN 1 AND 128),
  runtime_instance_id              TEXT NOT NULL
    CHECK(length(runtime_instance_id) BETWEEN 1 AND 128),
  runtime_instance_epoch           INTEGER NOT NULL
    CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  runtime_authority_sha256         TEXT NOT NULL
    CHECK(length(runtime_authority_sha256) = 64
      AND runtime_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_session_token_sha256     TEXT NOT NULL
    CHECK(length(runtime_session_token_sha256) = 64
      AND runtime_session_token_sha256 NOT GLOB '*[^0-9a-f]*'),
  lease_token_sha256               TEXT NOT NULL
    CHECK(length(lease_token_sha256) = 64
      AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*'),
  assurance                        TEXT NOT NULL CHECK(assurance = 'original_verified'),
  result                           TEXT NOT NULL
    CHECK(result IN ('open', 'closed', 'mismatch', 'quarantined', 'indeterminate')),
  error_code                       TEXT
    CHECK(error_code IS NULL OR length(error_code) BETWEEN 1 AND 64),
  requested_url                    TEXT
    CHECK(requested_url IS NULL OR length(requested_url) BETWEEN 8 AND 4096),
  canonical_observed_url           TEXT
    CHECK(canonical_observed_url IS NULL OR
      length(canonical_observed_url) BETWEEN 8 AND 4096),
  canonical_application_url        TEXT
    CHECK(canonical_application_url IS NULL OR
      length(canonical_application_url) BETWEEN 8 AND 4096),
  application_domain               TEXT
    CHECK(application_domain IS NULL OR (
      length(application_domain) BETWEEN 1 AND 253
      AND application_domain = lower(application_domain)
      AND application_domain NOT GLOB '*[^a-z0-9.-]*'
      AND application_domain NOT LIKE '.%'
      AND application_domain NOT LIKE '%.'
      AND application_domain NOT LIKE '%..%')),
  provider_record_id               TEXT
    CHECK(provider_record_id IS NULL OR length(provider_record_id) BETWEEN 1 AND 512),
  retrieval_status                 TEXT NOT NULL CHECK(retrieval_status IN (
    'observed', 'absent', 'not_found', 'gone', 'unreachable',
    'preflight_rejected')),
  http_status                      INTEGER
    CHECK(http_status IS NULL OR http_status BETWEEN 100 AND 599),
  http_semantics_digest            TEXT NOT NULL
    CHECK(length(http_semantics_digest) = 64
      AND http_semantics_digest NOT GLOB '*[^0-9a-f]*'),
  redirect_chain_digest            TEXT NOT NULL
    CHECK(length(redirect_chain_digest) = 64
      AND redirect_chain_digest NOT GLOB '*[^0-9a-f]*'),
  headers_digest                   TEXT NOT NULL
    CHECK(length(headers_digest) = 64
      AND headers_digest NOT GLOB '*[^0-9a-f]*'),
  content_digest                   TEXT NOT NULL
    CHECK(length(content_digest) = 64
      AND content_digest NOT GLOB '*[^0-9a-f]*'),
  parser_version                   TEXT NOT NULL
    CHECK(length(parser_version) BETWEEN 1 AND 128),
  parser_digest                    TEXT NOT NULL
    CHECK(length(parser_digest) = 64
      AND parser_digest NOT GLOB '*[^0-9a-f]*'),
  worker_runtime_identity_sha256   TEXT NOT NULL
    CHECK(length(worker_runtime_identity_sha256) = 64
      AND worker_runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_observation_json       TEXT NOT NULL
    CHECK(length(CAST(canonical_observation_json AS BLOB)) BETWEEN 2 AND 131072
      AND json_valid(canonical_observation_json)),
  observed_at_ms                   INTEGER NOT NULL
    CHECK(observed_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(assignment_id, completion_request_id),
  UNIQUE(assignment_id, observation_id, observation_sha256, attempt_id,
         account_id, job_id, subject_sha256, attempt_no,
         completion_request_id, completion_request_sha256, assurance, result,
         observed_at_ms),
  FOREIGN KEY(attempt_id, assignment_id, account_id, job_id, subject_sha256,
              attempt_no, worker_id, runtime_instance_id,
              runtime_instance_epoch, runtime_authority_sha256,
              runtime_session_token_sha256, lease_token_sha256)
    REFERENCES jobs_original_source_verification_attempts(
      attempt_id, assignment_id, account_id, job_id, subject_sha256,
      attempt_no, worker_id, runtime_instance_id, runtime_instance_epoch,
      runtime_authority_sha256, runtime_session_token_sha256,
      lease_token_sha256) ON DELETE CASCADE,
  CHECK(fence = attempt_no),
  CHECK((retrieval_status = 'preflight_rejected'
      AND requested_url IS NULL AND http_status IS NULL)
    OR (retrieval_status = 'unreachable'
      AND requested_url IS NOT NULL AND http_status IS NULL)
    OR (retrieval_status NOT IN ('preflight_rejected', 'unreachable')
      AND requested_url IS NOT NULL AND http_status IS NOT NULL)),
  CHECK((retrieval_status = 'not_found' AND http_status = 404)
    OR (retrieval_status = 'gone' AND http_status = 410)
    OR retrieval_status NOT IN ('not_found', 'gone')),
  CHECK((error_code IS NULL AND result <> 'indeterminate')
    OR (error_code IS NOT NULL AND result IN ('indeterminate', 'quarantined'))),
  CHECK((canonical_application_url IS NULL) = (application_domain IS NULL))
);

CREATE TABLE IF NOT EXISTS jobs_original_source_verification_receipts (
  receipt_id                       TEXT PRIMARY KEY
    CHECK(length(receipt_id) BETWEEN 20 AND 128
      AND receipt_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  receipt_sha256                   TEXT NOT NULL UNIQUE
    CHECK(length(receipt_sha256) = 64
      AND receipt_sha256 NOT GLOB '*[^0-9a-f]*'),
  assignment_id                    TEXT NOT NULL,
  attempt_id                       TEXT NOT NULL,
  account_id                       TEXT NOT NULL,
  job_id                           TEXT NOT NULL,
  subject_sha256                   TEXT NOT NULL,
  attempt_no                       INTEGER NOT NULL
    CHECK(attempt_no BETWEEN 1 AND 9007199254740991),
  assignment_generation           INTEGER NOT NULL
    CHECK(assignment_generation BETWEEN 1 AND 9007199254740991),
  assignment_sha256               TEXT NOT NULL
    CHECK(length(assignment_sha256) = 64
      AND assignment_sha256 NOT GLOB '*[^0-9a-f]*'),
  fence                            INTEGER NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  managed_authority_sha256         TEXT NOT NULL
    CHECK(length(managed_authority_sha256) = 64
      AND managed_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_managed_authority_json TEXT NOT NULL
    CHECK(length(CAST(canonical_managed_authority_json AS BLOB)) BETWEEN 2 AND 65536
      AND json_valid(canonical_managed_authority_json)),
  managed_environment              TEXT NOT NULL CHECK(managed_environment = 'production'),
  managed_region                   TEXT NOT NULL CHECK(length(managed_region) BETWEEN 1 AND 64),
  managed_channel                  TEXT NOT NULL CHECK(managed_channel IN ('canary','general')),
  managed_head_revision            INTEGER NOT NULL
    CHECK(managed_head_revision BETWEEN 1 AND 9007199254740991),
  managed_transition_sha256        TEXT NOT NULL
    CHECK(length(managed_transition_sha256) = 64
      AND managed_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_activation_sha256        TEXT NOT NULL
    CHECK(length(managed_activation_sha256) = 64
      AND managed_activation_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_manifest_sha256          TEXT NOT NULL
    CHECK(length(managed_manifest_sha256) = 64
      AND managed_manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_source_protocol_schema_sha256 TEXT NOT NULL
    CHECK(length(managed_source_protocol_schema_sha256) = 64
      AND managed_source_protocol_schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  managed_runtime_identity_sha256  TEXT NOT NULL
    CHECK(length(managed_runtime_identity_sha256) = 64
      AND managed_runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  runtime_grant_id                 TEXT NOT NULL CHECK(length(runtime_grant_id) BETWEEN 20 AND 128),
  runtime_instance_id              TEXT NOT NULL CHECK(length(runtime_instance_id) BETWEEN 20 AND 128),
  runtime_instance_epoch           INTEGER NOT NULL
    CHECK(runtime_instance_epoch BETWEEN 1 AND 9007199254740991),
  runtime_authority_sha256         TEXT NOT NULL
    CHECK(length(runtime_authority_sha256) = 64
      AND runtime_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  worker_runtime_identity_sha256   TEXT NOT NULL
    CHECK(length(worker_runtime_identity_sha256) = 64
      AND worker_runtime_identity_sha256 NOT GLOB '*[^0-9a-f]*'),
  completion_request_id            TEXT NOT NULL
    CHECK(length(completion_request_id) BETWEEN 1 AND 128),
  completion_request_sha256        TEXT NOT NULL
    CHECK(length(completion_request_sha256) = 64
      AND completion_request_sha256 NOT GLOB '*[^0-9a-f]*'),
  observation_id                   TEXT NOT NULL,
  observation_sha256               TEXT NOT NULL
    CHECK(length(observation_sha256) = 64
      AND observation_sha256 NOT GLOB '*[^0-9a-f]*'),
  observation_result               TEXT NOT NULL CHECK(observation_result IN (
    'open', 'closed', 'mismatch', 'quarantined', 'indeterminate')),
  canonical_application_url        TEXT
    CHECK(canonical_application_url IS NULL OR
      length(canonical_application_url) BETWEEN 8 AND 4096),
  application_domain               TEXT
    CHECK(application_domain IS NULL OR (
      length(application_domain) BETWEEN 1 AND 253
      AND application_domain = lower(application_domain)
      AND application_domain NOT GLOB '*[^a-z0-9.-]*'
      AND application_domain NOT LIKE '.%'
      AND application_domain NOT LIKE '%.'
      AND application_domain NOT LIKE '%..%')),
  assurance                        TEXT NOT NULL
    CHECK(assurance IN ('ats_snapshot', 'original_verified')),
  result                           TEXT NOT NULL
    CHECK(result IN (
      'verified_open', 'closed', 'redirected_to_unknown', 'identity_mismatch',
      'materially_changed', 'source_untrusted', 'expired', 'unknown',
      'unreachable', 'rate_limited', 'auth_required', 'captcha_required',
      'parse_ambiguous', 'provider_unavailable')),
  evidence_sha256                  TEXT NOT NULL
    CHECK(length(evidence_sha256) = 64
      AND evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
  material_sha256                  TEXT NOT NULL
    CHECK(length(material_sha256) = 64
      AND material_sha256 NOT GLOB '*[^0-9a-f]*'),
  source_risk_status               TEXT NOT NULL CHECK(source_risk_status = 'provider_observed'),
  employer_verification_status     TEXT NOT NULL CHECK(employer_verification_status = 'unverified'),
  scam_risk_status                 TEXT NOT NULL CHECK(scam_risk_status = 'review_required'),
  execution_capability             TEXT NOT NULL CHECK(execution_capability = 'review_only'),
  canonical_receipt_json           TEXT NOT NULL
    CHECK(length(CAST(canonical_receipt_json AS BLOB)) BETWEEN 2 AND 131072
      AND json_valid(canonical_receipt_json)),
  checked_at_ms                    INTEGER NOT NULL
    CHECK(checked_at_ms BETWEEN 0 AND 9007199254740991),
  expires_at_ms                    INTEGER NOT NULL
    CHECK(expires_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(assignment_id, completion_request_id),
  UNIQUE(assignment_id, receipt_id, receipt_sha256),
  UNIQUE(assignment_id, receipt_id, receipt_sha256, account_id, job_id,
         subject_sha256, material_sha256, assurance, result,
         checked_at_ms, expires_at_ms),
  UNIQUE(assignment_id, receipt_id, receipt_sha256, account_id, job_id,
         subject_sha256, observation_id, observation_sha256,
         material_sha256, assurance, result,
         checked_at_ms, expires_at_ms),
  FOREIGN KEY(assignment_id, account_id, job_id, subject_sha256)
    REFERENCES jobs_original_source_verification_assignments(
      assignment_id, account_id, job_id, subject_sha256) ON DELETE CASCADE,
  FOREIGN KEY(assignment_id, account_id, job_id, subject_sha256,
              assignment_generation, assignment_sha256, managed_authority_sha256)
    REFERENCES jobs_original_source_verification_assignments(
      assignment_id, account_id, job_id, subject_sha256,
      assignment_generation, assignment_sha256, managed_authority_sha256)
      ON DELETE CASCADE,
  FOREIGN KEY(runtime_grant_id)
    REFERENCES jobs_managed_cloud_original_source_verifier_runtime_instances(grant_id)
      ON DELETE RESTRICT,
  FOREIGN KEY(attempt_id, assignment_id, attempt_no)
    REFERENCES jobs_original_source_verification_attempts(
      attempt_id, assignment_id, attempt_no) ON DELETE CASCADE,
  FOREIGN KEY(assignment_id, observation_id, observation_sha256, attempt_id,
              account_id, job_id, subject_sha256, attempt_no,
              completion_request_id, completion_request_sha256, assurance,
              observation_result, checked_at_ms)
    REFERENCES jobs_original_source_verification_observations(
      assignment_id, observation_id, observation_sha256, attempt_id,
      account_id, job_id, subject_sha256, attempt_no,
      completion_request_id, completion_request_sha256, assurance,
      result, observed_at_ms) ON DELETE CASCADE,
  CHECK(evidence_sha256 = observation_sha256),
  CHECK(fence = attempt_no),
  CHECK(worker_runtime_identity_sha256 = managed_runtime_identity_sha256),
  CHECK(expires_at_ms > checked_at_ms),
  CHECK((canonical_application_url IS NULL) = (application_domain IS NULL)),
  CHECK((observation_result = 'open' AND canonical_application_url IS NOT NULL)
    OR observation_result = 'mismatch'
    OR (observation_result NOT IN ('open','mismatch')
      AND canonical_application_url IS NULL))
);

CREATE TABLE IF NOT EXISTS jobs_original_source_verification_transitions (
  transition_id                    TEXT PRIMARY KEY
    CHECK(length(transition_id) BETWEEN 20 AND 128
      AND transition_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  transition_sha256                TEXT NOT NULL UNIQUE
    CHECK(length(transition_sha256) = 64
      AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  account_id                       TEXT NOT NULL,
  job_id                           TEXT NOT NULL,
  head_revision                    INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision           INTEGER NOT NULL
    CHECK(previous_head_revision BETWEEN 0 AND 9007199254740991),
  predecessor_transition_sha256    TEXT,
  material_generation              INTEGER NOT NULL
    CHECK(material_generation BETWEEN 1 AND 9007199254740991),
  previous_material_generation     INTEGER NOT NULL
    CHECK(previous_material_generation BETWEEN 0 AND 9007199254740991),
  classification                   TEXT NOT NULL CHECK(classification IN (
    'initial', 'unchanged', 'material_change', 'closed', 'reopened',
    'mismatch', 'quarantined', 'indeterminate'
  )),
  assignment_id                    TEXT NOT NULL,
  receipt_id                       TEXT NOT NULL,
  receipt_sha256                   TEXT NOT NULL,
  subject_sha256                   TEXT NOT NULL,
  material_sha256                  TEXT NOT NULL,
  assurance                        TEXT NOT NULL,
  result                           TEXT NOT NULL CHECK(result IN (
    'verified_open', 'closed', 'redirected_to_unknown', 'identity_mismatch',
    'materially_changed', 'source_untrusted', 'expired', 'unknown',
    'unreachable', 'rate_limited', 'auth_required', 'captcha_required',
    'parse_ambiguous', 'provider_unavailable')),
  checked_at_ms                    INTEGER NOT NULL,
  expires_at_ms                    INTEGER NOT NULL,
  created_at_ms                    INTEGER NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, job_id, head_revision),
  UNIQUE(account_id, job_id, head_revision, transition_sha256),
  UNIQUE(account_id, job_id, head_revision, transition_id, transition_sha256),
  UNIQUE(account_id, job_id, head_revision, transition_sha256,
         material_generation),
  UNIQUE(account_id, job_id, head_revision, transition_id, transition_sha256,
         material_generation, assignment_id, receipt_id, receipt_sha256,
         subject_sha256, material_sha256, assurance, result,
         checked_at_ms, expires_at_ms),
  FOREIGN KEY(account_id, job_id)
    REFERENCES jobs_postings(account_id, id) ON DELETE CASCADE,
  FOREIGN KEY(assignment_id, receipt_id, receipt_sha256)
    REFERENCES jobs_original_source_verification_receipts(
      assignment_id, receipt_id, receipt_sha256) ON DELETE CASCADE,
  FOREIGN KEY(assignment_id, receipt_id, receipt_sha256, account_id, job_id,
              subject_sha256, material_sha256, assurance, result,
              checked_at_ms, expires_at_ms)
    REFERENCES jobs_original_source_verification_receipts(
      assignment_id, receipt_id, receipt_sha256, account_id, job_id,
      subject_sha256, material_sha256, assurance, result,
      checked_at_ms, expires_at_ms) ON DELETE CASCADE,
  FOREIGN KEY(account_id, job_id, previous_head_revision,
              predecessor_transition_sha256)
    REFERENCES jobs_original_source_verification_transitions(
      account_id, job_id, head_revision, transition_sha256) ON DELETE CASCADE,
  FOREIGN KEY(account_id, job_id, previous_head_revision,
              predecessor_transition_sha256, previous_material_generation)
    REFERENCES jobs_original_source_verification_transitions(
      account_id, job_id, head_revision, transition_sha256,
      material_generation) ON DELETE CASCADE,
  CHECK((head_revision = 1 AND previous_head_revision = 0
      AND predecessor_transition_sha256 IS NULL
      AND previous_material_generation = 0 AND classification = 'initial')
    OR (head_revision > 1 AND previous_head_revision = head_revision - 1
      AND predecessor_transition_sha256 IS NOT NULL)),
  CHECK((classification = 'unchanged'
      AND material_generation = previous_material_generation)
    OR (classification <> 'unchanged'
      AND material_generation = previous_material_generation + 1)),
  CHECK(created_at_ms = checked_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_original_source_verification_heads (
  account_id                       TEXT NOT NULL,
  job_id                           TEXT NOT NULL,
  head_revision                    INTEGER NOT NULL,
  transition_id                    TEXT NOT NULL,
  transition_sha256                TEXT NOT NULL,
  material_generation              INTEGER NOT NULL,
  assignment_id                    TEXT NOT NULL,
  receipt_id                       TEXT NOT NULL,
  receipt_sha256                   TEXT NOT NULL,
  subject_sha256                   TEXT NOT NULL,
  material_sha256                  TEXT NOT NULL,
  assurance                        TEXT NOT NULL,
  result                           TEXT NOT NULL CHECK(result IN (
    'verified_open', 'closed', 'redirected_to_unknown', 'identity_mismatch',
    'materially_changed', 'source_untrusted', 'expired', 'unknown',
    'unreachable', 'rate_limited', 'auth_required', 'captcha_required',
    'parse_ambiguous', 'provider_unavailable')),
  checked_at_ms                    INTEGER NOT NULL,
  expires_at_ms                    INTEGER NOT NULL,
  updated_at_ms                    INTEGER NOT NULL,
  PRIMARY KEY(account_id, job_id),
  FOREIGN KEY(account_id, job_id, head_revision, transition_id,
              transition_sha256, material_generation, assignment_id,
              receipt_id, receipt_sha256, subject_sha256, material_sha256,
              assurance, result, checked_at_ms, expires_at_ms)
    REFERENCES jobs_original_source_verification_transitions(
      account_id, job_id, head_revision, transition_id, transition_sha256,
      material_generation, assignment_id, receipt_id, receipt_sha256,
      subject_sha256, material_sha256, assurance, result,
      checked_at_ms, expires_at_ms) ON DELETE CASCADE,
  CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  CHECK(material_generation BETWEEN 1 AND 9007199254740991),
  CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  CHECK(updated_at_ms = checked_at_ms)
);

CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_assignment_predecessor_validate
BEFORE INSERT ON jobs_original_source_verification_assignments
WHEN NEW.assignment_generation > 1
BEGIN
  SELECT CASE WHEN NOT EXISTS(
    SELECT 1 FROM jobs_original_source_verification_assignments predecessor
     WHERE predecessor.account_id = NEW.account_id
       AND predecessor.job_id = NEW.job_id
       AND predecessor.assignment_generation = NEW.assignment_generation - 1
       AND predecessor.assignment_sha256 = NEW.predecessor_assignment_sha256
  ) THEN RAISE(ABORT, 'original-source assignment predecessor mismatch') END;
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_receipt_binding_validate
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
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_assignment_identity_no_update
BEFORE UPDATE OF assignment_id, account_id, job_id, subject_schema_version,
  assignment_generation, assignment_sha256, predecessor_assignment_sha256,
  canonical_assignment_json, managed_authority_sha256,
  canonical_managed_authority_json, managed_environment, managed_region,
  managed_channel, managed_head_revision, managed_transition_sha256,
  managed_activation_sha256, managed_manifest_sha256, managed_cohort_sha256,
  managed_trust_generation, managed_channel_sequence, managed_release_id,
  managed_release_sequence, managed_task_queue_sha256,
  managed_failure_converter_sha256, managed_activation_expires_at_ms,
  managed_source_protocol_schema_sha256, managed_runtime_identity_sha256,
  managed_dependency_evidence_sha256, managed_heartbeat_ttl_ms,
  subject_sha256, canonical_subject_json,
  target_kind, not_before_at_ms, expires_at_ms, attempt_budget, created_at_ms
ON jobs_original_source_verification_assignments
BEGIN
  SELECT RAISE(ABORT, 'original-source assignment identity is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_assignment_no_delete
BEFORE DELETE ON jobs_original_source_verification_assignments
WHEN EXISTS(SELECT 1 FROM jobs_postings
  WHERE account_id = OLD.account_id AND id = OLD.job_id)
BEGIN SELECT RAISE(ABORT, 'original-source assignments cannot be deleted'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_assignment_event_cas
BEFORE UPDATE ON jobs_original_source_verification_assignments
BEGIN
  SELECT CASE WHEN NOT (
    (NEW.current_event_sequence = OLD.current_event_sequence
      AND NEW.current_event_sha256 IS OLD.current_event_sha256)
    OR (NEW.current_event_sequence = OLD.current_event_sequence + 1
      AND EXISTS(
        SELECT 1 FROM jobs_original_source_verification_events event
         WHERE event.assignment_id = NEW.assignment_id
           AND event.event_sequence = NEW.current_event_sequence
           AND event.event_sha256 = NEW.current_event_sha256
           AND event.predecessor_event_sha256 IS OLD.current_event_sha256))
  ) THEN RAISE(ABORT, 'original-source assignment event head must advance by exact CAS') END;
  SELECT CASE WHEN NEW.state = 'leased' AND NOT EXISTS(
    SELECT 1 FROM jobs_original_source_verification_attempts attempt
     WHERE attempt.assignment_id = NEW.assignment_id
       AND attempt.attempt_id = NEW.active_attempt_id
       AND attempt.attempt_no = NEW.attempt_count
       AND attempt.lease_token_sha256 = NEW.lease_token_sha256
       AND attempt.worker_id = NEW.lease_owner
       AND attempt.hard_deadline_at_ms = NEW.hard_deadline_at_ms
  ) THEN RAISE(ABORT, 'original-source assignment lease projection is invalid') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_grants_immutable_fields
BEFORE UPDATE OF
  grant_id, token_sha256, issuance_ref, environment, region, channel,
  activation_sha256, manifest_sha256, component_id, role, head_revision,
  transition_sha256, artifact_sha256, config_schema_sha256, migration_set_sha256,
  protocol_set_sha256, task_queue_sha256, failure_converter_sha256,
  expected_dependency_evidence_sha256, expected_runtime_identity_sha256,
  expected_worker_id, authorization_ref, created_by, activation_expires_at_ms,
  expires_at_ms, created_at_ms
ON jobs_managed_cloud_original_source_verifier_runtime_grants BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant authority is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_grants_secret_scrub
BEFORE UPDATE OF grant_token_ciphertext ON jobs_managed_cloud_original_source_verifier_runtime_grants
WHEN OLD.grant_token_ciphertext IS NULL OR NEW.grant_token_ciphertext IS NOT NULL BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant secret can only be scrubbed');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_grants_no_delete
BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_runtime_grants BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_grant_revocations_no_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_grant_revocations BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_grant_revocations_no_delete
BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_grant_revocations BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime grant revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_instances_no_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_instances BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime instance is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_instances_no_delete
BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_runtime_instances BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime instance is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_fenced_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.runtime_instance_id <> OLD.runtime_instance_id
  OR NEW.instance_epoch <> OLD.instance_epoch
  OR NEW.heartbeat_sequence <> OLD.heartbeat_sequence + 1
  OR NEW.heartbeat_at_ms < OLD.heartbeat_at_ms BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat fence is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_no_delete
BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_sequence
BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.heartbeat_sequence <> 1 BEGIN
  SELECT RAISE(ABORT, 'managed cloud runtime heartbeat sequence is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit_matches_current
BEFORE INSERT ON jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats current
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
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit_no_update
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit BEGIN
  SELECT RAISE(ABORT, 'managed cloud heartbeat audit is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit_bounded_delete
BEFORE DELETE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats current
   WHERE current.runtime_instance_id = OLD.runtime_instance_id
     AND current.instance_epoch = OLD.instance_epoch
     AND OLD.heartbeat_sequence <= current.heartbeat_sequence - 64
) BEGIN
  SELECT RAISE(ABORT, 'managed cloud heartbeat audit pruning is unsafe');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verification_protocol_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_source_verification_protocols
BEGIN SELECT RAISE(ABORT, 'managed source-verification protocols are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verification_protocol_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_source_verification_protocols
BEGIN SELECT RAISE(ABORT, 'managed source-verification protocols are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_original_source_identity_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_original_source_verifier_identities
BEGIN SELECT RAISE(ABORT, 'managed original-source identities are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_original_source_identity_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_original_source_verifier_identities
BEGIN SELECT RAISE(ABORT, 'managed original-source identities are immutable'); END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_attempt_no_update
BEFORE UPDATE ON jobs_original_source_verification_attempts
BEGIN SELECT RAISE(ABORT, 'original-source attempts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_attempt_no_delete
BEFORE DELETE ON jobs_original_source_verification_attempts
WHEN EXISTS(SELECT 1 FROM jobs_postings
  WHERE account_id = OLD.account_id AND id = OLD.job_id)
BEGIN SELECT RAISE(ABORT, 'original-source attempts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_event_no_update
BEFORE UPDATE ON jobs_original_source_verification_events
BEGIN SELECT RAISE(ABORT, 'original-source events are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_event_no_delete
BEFORE DELETE ON jobs_original_source_verification_events
WHEN EXISTS(
  SELECT 1 FROM jobs_original_source_verification_assignments assignment
  JOIN jobs_postings posting ON posting.account_id = assignment.account_id
    AND posting.id = assignment.job_id
  WHERE assignment.assignment_id = OLD.assignment_id)
BEGIN SELECT RAISE(ABORT, 'original-source events are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_observation_no_update
BEFORE UPDATE ON jobs_original_source_verification_observations
BEGIN SELECT RAISE(ABORT, 'original-source observations are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_observation_no_delete
BEFORE DELETE ON jobs_original_source_verification_observations
WHEN EXISTS(SELECT 1 FROM jobs_postings
  WHERE account_id = OLD.account_id AND id = OLD.job_id)
BEGIN SELECT RAISE(ABORT, 'original-source observations are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_receipt_no_update
BEFORE UPDATE ON jobs_original_source_verification_receipts
BEGIN SELECT RAISE(ABORT, 'original-source receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_receipt_no_delete
BEFORE DELETE ON jobs_original_source_verification_receipts
WHEN EXISTS(SELECT 1 FROM jobs_postings
  WHERE account_id = OLD.account_id AND id = OLD.job_id)
BEGIN SELECT RAISE(ABORT, 'original-source receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_transition_no_update
BEFORE UPDATE ON jobs_original_source_verification_transitions
BEGIN SELECT RAISE(ABORT, 'original-source transitions are immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_transition_no_delete
BEFORE DELETE ON jobs_original_source_verification_transitions
WHEN EXISTS(SELECT 1 FROM jobs_postings
  WHERE account_id = OLD.account_id AND id = OLD.job_id)
BEGIN SELECT RAISE(ABORT, 'original-source transitions are immutable'); END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_transition_predecessor_validate
BEFORE INSERT ON jobs_original_source_verification_transitions
WHEN NEW.head_revision > 1
BEGIN
  SELECT CASE WHEN NOT EXISTS(
    SELECT 1 FROM jobs_original_source_verification_transitions predecessor
     WHERE predecessor.account_id = NEW.account_id
       AND predecessor.job_id = NEW.job_id
       AND predecessor.head_revision = NEW.previous_head_revision
       AND predecessor.transition_sha256 = NEW.predecessor_transition_sha256
       AND predecessor.material_generation = NEW.previous_material_generation
       AND predecessor.checked_at_ms <= NEW.checked_at_ms
       AND predecessor.created_at_ms <= NEW.created_at_ms
  ) THEN RAISE(ABORT, 'original-source transition predecessor mismatch') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_head_initial_insert
BEFORE INSERT ON jobs_original_source_verification_heads
WHEN NEW.head_revision <> 1
BEGIN SELECT RAISE(ABORT, 'original-source initial head revision must be one'); END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_head_cas_update
BEFORE UPDATE ON jobs_original_source_verification_heads
BEGIN
  SELECT CASE WHEN NEW.head_revision <> OLD.head_revision + 1
    THEN RAISE(ABORT, 'original-source head must advance by exact CAS') END;
  SELECT CASE WHEN NEW.account_id <> OLD.account_id OR NEW.job_id <> OLD.job_id
    THEN RAISE(ABORT, 'original-source head identity is immutable') END;
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_original_source_head_no_delete
BEFORE DELETE ON jobs_original_source_verification_heads
WHEN EXISTS(SELECT 1 FROM jobs_postings
  WHERE account_id = OLD.account_id AND id = OLD.job_id)
BEGIN SELECT RAISE(ABORT, 'original-source heads cannot be deleted'); END;

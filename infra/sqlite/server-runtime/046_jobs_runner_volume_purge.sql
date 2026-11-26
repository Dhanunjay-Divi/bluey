-- Target: SQLite
-- Signed managed-runner volume identity, immutable key epochs, purge fan-out,
-- independently authorized destruction evidence, and restore tombstones.
CREATE TABLE IF NOT EXISTS jobs_runner_legacy_inventory_authorities (
  authority_id                    TEXT PRIMARY KEY,
  reconciliation_id               TEXT NOT NULL,
  authority_generation            INTEGER NOT NULL UNIQUE
    CHECK(authority_generation >= 1),
  authority_state                 TEXT NOT NULL
    CHECK(authority_state IN ('reconciling', 'ready')),
  predecessor_generation          INTEGER NOT NULL CHECK(predecessor_generation >= 0),
  predecessor_authority_id        TEXT,
  predecessor_authority_sha256    TEXT,
  root_count                      INTEGER NOT NULL CHECK(root_count >= 0),
  root_set_sha256                 TEXT NOT NULL CHECK(length(root_set_sha256) = 64),
  scope_ref                       TEXT NOT NULL CHECK(length(scope_ref) > 0),
  evidence_ref                    TEXT NOT NULL CHECK(length(evidence_ref) > 0),
  evidence_sha256                 TEXT NOT NULL CHECK(length(evidence_sha256) = 64),
  authorized_by                   TEXT NOT NULL CHECK(length(authorized_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
  authority_sha256                TEXT NOT NULL UNIQUE CHECK(length(authority_sha256) = 64),
  UNIQUE(reconciliation_id, authority_state),
  UNIQUE(authority_id, authority_generation, authority_sha256),
  CHECK(root_count <> 0 OR root_set_sha256 =
    '1d657a1abf311316ec1f0390d2fd76239cd5b06280325b670b4295a57096045c'),
  CHECK(
    (predecessor_generation = 0 AND predecessor_authority_id IS NULL
      AND predecessor_authority_sha256 IS NULL)
    OR
    (predecessor_generation >= 1 AND predecessor_authority_id IS NOT NULL
      AND predecessor_authority_sha256 IS NOT NULL
      AND length(predecessor_authority_sha256) = 64)
  ),
  CHECK(authority_state <> 'ready' OR predecessor_generation >= 1),
  FOREIGN KEY(
    predecessor_authority_id, predecessor_generation, predecessor_authority_sha256
  ) REFERENCES jobs_runner_legacy_inventory_authorities(
    authority_id, authority_generation, authority_sha256
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_legacy_inventory_authorities_reconciliation
  ON jobs_runner_legacy_inventory_authorities(reconciliation_id, authority_generation, authority_state);

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_legacy_inventory_authorities_no_update
BEFORE UPDATE ON jobs_runner_legacy_inventory_authorities
BEGIN
  SELECT RAISE(ABORT, 'runner legacy inventory authority is append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_legacy_inventory_authorities_no_delete
BEFORE DELETE ON jobs_runner_legacy_inventory_authorities
BEGIN
  SELECT RAISE(ABORT, 'runner legacy inventory authority is append-only');
END;

CREATE TABLE IF NOT EXISTS jobs_runner_volume_fleet_state (
  singleton_id                    INTEGER PRIMARY KEY CHECK(singleton_id = 1),
  enrollment_generation           INTEGER NOT NULL DEFAULT 0
    CHECK(enrollment_generation >= 0),
  purge_generation                INTEGER NOT NULL DEFAULT 0
    CHECK(purge_generation >= 0),
  tombstone_generation            INTEGER NOT NULL DEFAULT 0
    CHECK(tombstone_generation >= 0),
  destruction_generation          INTEGER NOT NULL DEFAULT 0
    CHECK(destruction_generation >= 0),
  legacy_reconciliation_generation INTEGER NOT NULL DEFAULT 0
    CHECK(legacy_reconciliation_generation >= 0),
  storage_attestation_generation  INTEGER NOT NULL DEFAULT 0
    CHECK(storage_attestation_generation >= 0),
  storage_attestation_count       INTEGER NOT NULL DEFAULT 0
    CHECK(storage_attestation_count >= 0),
  storage_attestation_set_sha256  TEXT NOT NULL DEFAULT
    '76f460646d1be73e8796ece72560c26b7be276c4183fe569d1763b59f89ce1e9'
    CHECK(length(storage_attestation_set_sha256) = 64),
  legacy_inventory_state           TEXT NOT NULL DEFAULT 'unknown'
    CHECK(legacy_inventory_state IN ('unknown', 'reconciling', 'ready')),
  legacy_inventory_generation      INTEGER NOT NULL DEFAULT 0
    CHECK(legacy_inventory_generation >= 0),
  legacy_inventory_reconciliation_id TEXT,
  legacy_inventory_authority_id    TEXT,
  legacy_inventory_authority_sha256 TEXT,
  legacy_inventory_root_count      INTEGER CHECK(legacy_inventory_root_count >= 0),
  legacy_inventory_root_set_sha256 TEXT,
  cutover_state                   TEXT NOT NULL DEFAULT 'pre_cutover'
    CHECK(cutover_state IN ('pre_cutover', 'reconciling', 'ready')),
  unresolved_legacy_volume_count  INTEGER NOT NULL DEFAULT 0
    CHECK(unresolved_legacy_volume_count >= 0),
  cutover_enrollment_generation   INTEGER CHECK(cutover_enrollment_generation >= 0),
  cutover_purge_generation        INTEGER CHECK(cutover_purge_generation >= 0),
  cutover_tombstone_generation    INTEGER CHECK(cutover_tombstone_generation >= 0),
  cutover_destruction_generation  INTEGER CHECK(cutover_destruction_generation >= 0),
  cutover_legacy_reconciliation_generation INTEGER
    CHECK(cutover_legacy_reconciliation_generation >= 0),
  cutover_storage_attestation_generation INTEGER
    CHECK(cutover_storage_attestation_generation >= 0),
  cutover_storage_attestation_count INTEGER
    CHECK(cutover_storage_attestation_count >= 0),
  cutover_storage_attestation_set_sha256 TEXT,
  cutover_legacy_inventory_generation INTEGER
    CHECK(cutover_legacy_inventory_generation >= 1),
  cutover_legacy_inventory_reconciliation_id TEXT,
  cutover_legacy_inventory_authority_id TEXT,
  cutover_legacy_inventory_authority_sha256 TEXT,
  cutover_legacy_inventory_root_count INTEGER
    CHECK(cutover_legacy_inventory_root_count >= 0),
  cutover_legacy_inventory_root_set_sha256 TEXT,
  cutover_non_destroyed_volume_count INTEGER CHECK(cutover_non_destroyed_volume_count >= 0),
  cutover_destruction_count       INTEGER CHECK(cutover_destruction_count >= 0),
  cutover_unresolved_legacy_volume_count INTEGER
    CHECK(cutover_unresolved_legacy_volume_count >= 0),
  cutover_evidence_ref            TEXT,
  cutover_evidence_sha256         TEXT,
  cutover_authorized_by           TEXT,
  cutover_at_ms                   INTEGER,
  updated_at_ms                   INTEGER NOT NULL DEFAULT 0 CHECK(updated_at_ms >= 0),
  CHECK(
    (legacy_inventory_state = 'unknown'
      AND legacy_inventory_generation = 0
      AND legacy_inventory_reconciliation_id IS NULL
      AND legacy_inventory_authority_id IS NULL
      AND legacy_inventory_authority_sha256 IS NULL
      AND legacy_inventory_root_count IS NULL
      AND legacy_inventory_root_set_sha256 IS NULL)
    OR
    (legacy_inventory_state IN ('reconciling', 'ready')
      AND legacy_inventory_generation >= 1
      AND legacy_inventory_reconciliation_id IS NOT NULL
      AND legacy_inventory_authority_id IS NOT NULL
      AND legacy_inventory_authority_sha256 IS NOT NULL
      AND length(legacy_inventory_authority_sha256) = 64
      AND legacy_inventory_root_count IS NOT NULL
      AND legacy_inventory_root_set_sha256 IS NOT NULL
      AND length(legacy_inventory_root_set_sha256) = 64
      AND (legacy_inventory_root_count <> 0 OR legacy_inventory_root_set_sha256 =
        '1d657a1abf311316ec1f0390d2fd76239cd5b06280325b670b4295a57096045c'))
  ),
  CHECK(cutover_state <> 'ready' OR (
    legacy_inventory_state = 'ready'
    AND unresolved_legacy_volume_count = 0
    AND cutover_unresolved_legacy_volume_count = 0
    AND cutover_enrollment_generation = enrollment_generation
    AND cutover_purge_generation = purge_generation
    AND cutover_tombstone_generation = tombstone_generation
    AND cutover_destruction_generation = destruction_generation
    AND cutover_legacy_reconciliation_generation = legacy_reconciliation_generation
    AND cutover_storage_attestation_generation = storage_attestation_generation
    AND cutover_storage_attestation_count = storage_attestation_count
    AND cutover_storage_attestation_set_sha256 = storage_attestation_set_sha256
    AND cutover_legacy_inventory_generation = legacy_inventory_generation
    AND cutover_legacy_inventory_reconciliation_id = legacy_inventory_reconciliation_id
    AND cutover_legacy_inventory_authority_id = legacy_inventory_authority_id
    AND cutover_legacy_inventory_authority_sha256 = legacy_inventory_authority_sha256
    AND cutover_legacy_inventory_root_count = legacy_inventory_root_count
    AND cutover_legacy_inventory_root_set_sha256 = legacy_inventory_root_set_sha256
  )),
  CHECK(
    (cutover_state = 'pre_cutover'
      AND cutover_enrollment_generation IS NULL
      AND cutover_purge_generation IS NULL
      AND cutover_tombstone_generation IS NULL
      AND cutover_destruction_generation IS NULL
      AND cutover_legacy_reconciliation_generation IS NULL
      AND cutover_storage_attestation_generation IS NULL
      AND cutover_storage_attestation_count IS NULL
      AND cutover_storage_attestation_set_sha256 IS NULL
      AND cutover_legacy_inventory_generation IS NULL
      AND cutover_legacy_inventory_reconciliation_id IS NULL
      AND cutover_legacy_inventory_authority_id IS NULL
      AND cutover_legacy_inventory_authority_sha256 IS NULL
      AND cutover_legacy_inventory_root_count IS NULL
      AND cutover_legacy_inventory_root_set_sha256 IS NULL
      AND cutover_non_destroyed_volume_count IS NULL
      AND cutover_destruction_count IS NULL
      AND cutover_unresolved_legacy_volume_count IS NULL
      AND cutover_evidence_ref IS NULL
      AND cutover_evidence_sha256 IS NULL AND cutover_authorized_by IS NULL
      AND cutover_at_ms IS NULL)
    OR (cutover_state = 'reconciling' AND (
      (cutover_enrollment_generation IS NULL
        AND cutover_purge_generation IS NULL
        AND cutover_tombstone_generation IS NULL
        AND cutover_destruction_generation IS NULL
        AND cutover_legacy_reconciliation_generation IS NULL
        AND cutover_storage_attestation_generation IS NULL
        AND cutover_storage_attestation_count IS NULL
        AND cutover_storage_attestation_set_sha256 IS NULL
        AND cutover_legacy_inventory_generation IS NULL
        AND cutover_legacy_inventory_reconciliation_id IS NULL
        AND cutover_legacy_inventory_authority_id IS NULL
        AND cutover_legacy_inventory_authority_sha256 IS NULL
        AND cutover_legacy_inventory_root_count IS NULL
        AND cutover_legacy_inventory_root_set_sha256 IS NULL
        AND cutover_non_destroyed_volume_count IS NULL
        AND cutover_destruction_count IS NULL
        AND cutover_unresolved_legacy_volume_count IS NULL
        AND cutover_evidence_ref IS NULL AND cutover_evidence_sha256 IS NULL
        AND cutover_authorized_by IS NULL AND cutover_at_ms IS NULL)
      OR
      (cutover_enrollment_generation IS NOT NULL
        AND cutover_purge_generation IS NOT NULL
        AND cutover_tombstone_generation IS NOT NULL
        AND cutover_destruction_generation IS NOT NULL
        AND cutover_legacy_reconciliation_generation IS NOT NULL
        AND cutover_storage_attestation_generation IS NOT NULL
        AND cutover_storage_attestation_count IS NOT NULL
        AND cutover_storage_attestation_set_sha256 IS NOT NULL
        AND length(cutover_storage_attestation_set_sha256) = 64
        AND cutover_legacy_inventory_generation IS NOT NULL
        AND cutover_legacy_inventory_reconciliation_id IS NOT NULL
        AND cutover_legacy_inventory_authority_id IS NOT NULL
        AND cutover_legacy_inventory_authority_sha256 IS NOT NULL
        AND length(cutover_legacy_inventory_authority_sha256) = 64
        AND cutover_legacy_inventory_root_count IS NOT NULL
        AND cutover_legacy_inventory_root_set_sha256 IS NOT NULL
        AND length(cutover_legacy_inventory_root_set_sha256) = 64
        AND cutover_non_destroyed_volume_count IS NOT NULL
        AND cutover_destruction_count IS NOT NULL
        AND cutover_unresolved_legacy_volume_count IS NOT NULL
        AND cutover_evidence_ref IS NOT NULL
        AND cutover_evidence_sha256 IS NOT NULL
        AND length(cutover_evidence_sha256) = 64
        AND cutover_authorized_by IS NOT NULL AND cutover_at_ms IS NOT NULL)
    ))
    OR
    (cutover_state = 'ready'
      AND cutover_enrollment_generation IS NOT NULL
      AND cutover_purge_generation IS NOT NULL
      AND cutover_tombstone_generation IS NOT NULL
      AND cutover_destruction_generation IS NOT NULL
      AND cutover_legacy_reconciliation_generation IS NOT NULL
      AND cutover_storage_attestation_generation IS NOT NULL
      AND cutover_storage_attestation_count IS NOT NULL
      AND cutover_storage_attestation_set_sha256 IS NOT NULL
      AND length(cutover_storage_attestation_set_sha256) = 64
      AND cutover_legacy_inventory_generation IS NOT NULL
      AND cutover_legacy_inventory_reconciliation_id IS NOT NULL
      AND cutover_legacy_inventory_authority_id IS NOT NULL
      AND cutover_legacy_inventory_authority_sha256 IS NOT NULL
      AND length(cutover_legacy_inventory_authority_sha256) = 64
      AND cutover_legacy_inventory_root_count IS NOT NULL
      AND cutover_legacy_inventory_root_set_sha256 IS NOT NULL
      AND length(cutover_legacy_inventory_root_set_sha256) = 64
      AND cutover_non_destroyed_volume_count IS NOT NULL
      AND cutover_destruction_count IS NOT NULL
      AND cutover_unresolved_legacy_volume_count IS NOT NULL
      AND cutover_evidence_ref IS NOT NULL
      AND cutover_evidence_sha256 IS NOT NULL
      AND length(cutover_evidence_sha256) = 64
      AND cutover_authorized_by IS NOT NULL AND cutover_at_ms IS NOT NULL)
  ),
  FOREIGN KEY(
    legacy_inventory_authority_id, legacy_inventory_generation,
    legacy_inventory_authority_sha256
  ) REFERENCES jobs_runner_legacy_inventory_authorities(
    authority_id, authority_generation, authority_sha256
  )
);

INSERT OR IGNORE INTO jobs_runner_volume_fleet_state (
  singleton_id, enrollment_generation, purge_generation, tombstone_generation,
  destruction_generation, legacy_reconciliation_generation, cutover_state,
  unresolved_legacy_volume_count, cutover_evidence_ref,
  cutover_evidence_sha256, cutover_authorized_by, cutover_at_ms, updated_at_ms
) VALUES (1, 0, 0, 0, 0, 0, 'pre_cutover', 0, NULL, NULL, NULL, NULL, 0);

CREATE TABLE IF NOT EXISTS jobs_runner_volume_admission_grants (
  grant_id                 TEXT PRIMARY KEY,
  token_sha256             TEXT NOT NULL UNIQUE CHECK(length(token_sha256) = 64),
  expected_worker_id       TEXT NOT NULL,
  provider                 TEXT NOT NULL,
  provider_resource_id     TEXT NOT NULL,
  resource_fingerprint     TEXT NOT NULL CHECK(length(resource_fingerprint) = 64),
  authorization_ref        TEXT NOT NULL,
  created_by               TEXT NOT NULL,
  issued_fleet_generation  INTEGER NOT NULL CHECK(issued_fleet_generation >= 0),
  expires_at_ms            INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  created_at_ms            INTEGER NOT NULL CHECK(created_at_ms >= 0),
  consumed_volume_id       TEXT,
  consumed_at_ms           INTEGER,
  CHECK((consumed_volume_id IS NULL) = (consumed_at_ms IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_volume_grants_expiry
  ON jobs_runner_volume_admission_grants(expires_at_ms, consumed_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_volumes (
  volume_id                     TEXT PRIMARY KEY,
  worker_id                     TEXT NOT NULL,
  provider                      TEXT NOT NULL,
  provider_resource_id          TEXT NOT NULL,
  resource_fingerprint          TEXT NOT NULL UNIQUE CHECK(length(resource_fingerprint) = 64),
  current_epoch                 INTEGER NOT NULL CHECK(current_epoch >= 1),
  enrollment_generation         INTEGER NOT NULL CHECK(enrollment_generation >= 1),
  required_tombstone_generation INTEGER NOT NULL CHECK(required_tombstone_generation >= 0),
  reconciled_tombstone_generation INTEGER NOT NULL DEFAULT 0
    CHECK(reconciled_tombstone_generation >= 0),
  status                        TEXT NOT NULL DEFAULT 'reconciling'
    CHECK(status IN ('reconciling', 'active', 'suspended', 'retired', 'destroyed')),
  active_instance_id            TEXT,
  instance_lease_expires_at_ms  INTEGER,
  legacy_artifact_count         INTEGER NOT NULL DEFAULT 0
    CHECK(legacy_artifact_count >= 0),
  admission_grant_id            TEXT NOT NULL UNIQUE
    REFERENCES jobs_runner_volume_admission_grants(grant_id),
  enrolled_at_ms                INTEGER NOT NULL CHECK(enrolled_at_ms >= 0),
  last_seen_at_ms               INTEGER NOT NULL CHECK(last_seen_at_ms >= 0),
  updated_at_ms                 INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  UNIQUE(provider, provider_resource_id),
  UNIQUE(enrollment_generation),
  CHECK((active_instance_id IS NULL) = (instance_lease_expires_at_ms IS NULL)),
  CHECK(status <> 'destroyed' OR active_instance_id IS NULL),
  CHECK(reconciled_tombstone_generation <= required_tombstone_generation),
  CHECK(status <> 'active' OR (
    reconciled_tombstone_generation = required_tombstone_generation
  ))
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_volumes_worker_status
  ON jobs_runner_volumes(worker_id, status, updated_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_volume_keys (
  volume_id              TEXT NOT NULL REFERENCES jobs_runner_volumes(volume_id),
  enrollment_epoch       INTEGER NOT NULL CHECK(enrollment_epoch >= 1),
  public_key_base64url   TEXT NOT NULL CHECK(length(public_key_base64url) = 43),
  key_fingerprint        TEXT NOT NULL UNIQUE CHECK(length(key_fingerprint) = 64),
  activated_at_ms        INTEGER NOT NULL CHECK(activated_at_ms >= 0),
  retired_at_ms          INTEGER,
  PRIMARY KEY(volume_id, enrollment_epoch),
  UNIQUE(volume_id, enrollment_epoch, key_fingerprint),
  CHECK(retired_at_ms IS NULL OR retired_at_ms >= activated_at_ms)
);

-- Immutable, signed proof of the current closed-world storage state. Historical
-- enrollment inventory remains untouched; activation is authorized only by the
-- latest attestation for the current key epoch and process lease.
CREATE TABLE IF NOT EXISTS jobs_runner_volume_storage_attestations (
  volume_id                              TEXT NOT NULL,
  enrollment_epoch                       INTEGER NOT NULL CHECK(enrollment_epoch >= 1),
  volume_key_fingerprint                 TEXT NOT NULL CHECK(length(volume_key_fingerprint) = 64),
  attestation_generation                 INTEGER NOT NULL CHECK(attestation_generation >= 1),
  fleet_attestation_generation           INTEGER NOT NULL UNIQUE
    CHECK(fleet_attestation_generation >= 1),
  version                                INTEGER NOT NULL CHECK(version = 1),
  audience                               TEXT NOT NULL
    CHECK(audience = 'bluey-jobs-runner-volume-storage-attestation-v1'),
  attestation_id                         TEXT NOT NULL,
  resource_fingerprint                   TEXT NOT NULL CHECK(length(resource_fingerprint) = 64),
  enrollment_generation                  INTEGER NOT NULL CHECK(enrollment_generation >= 1),
  process_instance_id                    TEXT NOT NULL,
  predecessor_attestation_generation     INTEGER NOT NULL
    CHECK(predecessor_attestation_generation >= 0),
  predecessor_attestation_sha256         TEXT NOT NULL
    CHECK(length(predecessor_attestation_sha256) = 64),
  required_tombstone_generation          INTEGER NOT NULL
    CHECK(required_tombstone_generation >= 0),
  reconciled_tombstone_generation        INTEGER NOT NULL
    CHECK(reconciled_tombstone_generation >= 0),
  storage_evidence_version               INTEGER NOT NULL CHECK(storage_evidence_version = 2),
  subject_storage_layout_version         INTEGER NOT NULL
    CHECK(subject_storage_layout_version = 2),
  root_device_id                         TEXT NOT NULL,
  root_link_count                        INTEGER NOT NULL CHECK(root_link_count >= 1),
  root_entry_count                       INTEGER NOT NULL CHECK(root_entry_count >= 0),
  root_file_bytes                        TEXT NOT NULL,
  root_sha256                            TEXT NOT NULL CHECK(length(root_sha256) = 64),
  subject_storage_subject_count          INTEGER NOT NULL
    CHECK(subject_storage_subject_count >= 0),
  subject_storage_subject_set_sha256     TEXT NOT NULL
    CHECK(length(subject_storage_subject_set_sha256) = 64),
  subject_storage_scope_count            INTEGER NOT NULL
    CHECK(subject_storage_scope_count >= 0),
  subject_storage_complete_root_entry_count INTEGER NOT NULL
    CHECK(subject_storage_complete_root_entry_count >= 0),
  subject_storage_complete_root_file_bytes TEXT NOT NULL,
  subject_storage_complete_root_sha256   TEXT NOT NULL
    CHECK(length(subject_storage_complete_root_sha256) = 64),
  locator_count                          INTEGER NOT NULL CHECK(locator_count >= 0),
  resident_locator_count                 INTEGER NOT NULL CHECK(resident_locator_count >= 0),
  locator_set_sha256                     TEXT NOT NULL CHECK(length(locator_set_sha256) = 64),
  legacy_inventory_version               INTEGER NOT NULL CHECK(legacy_inventory_version = 1),
  legacy_artifact_count                  INTEGER NOT NULL CHECK(legacy_artifact_count = 0),
  legacy_artifact_bytes                  TEXT NOT NULL CHECK(legacy_artifact_bytes = '0'),
  legacy_artifact_set_sha256             TEXT NOT NULL CHECK(legacy_artifact_set_sha256 =
    'b414032476a3243362451c3a47d7989a9d84881436cb2f32e85b90098a4a0a1e'),
  unclassified_root_count                INTEGER NOT NULL CHECK(unclassified_root_count = 0),
  runner_build_id                        TEXT NOT NULL,
  observed_at_ms                         INTEGER NOT NULL CHECK(observed_at_ms >= 0),
  signature                              TEXT NOT NULL,
  canonical_unsigned_sha256              TEXT NOT NULL
    CHECK(length(canonical_unsigned_sha256) = 64),
  attestation_sha256                     TEXT NOT NULL CHECK(length(attestation_sha256) = 64),
  canonical_json                         TEXT NOT NULL,
  received_at_ms                         INTEGER NOT NULL CHECK(received_at_ms >= 0),
  PRIMARY KEY(volume_id, enrollment_epoch, attestation_generation),
  UNIQUE(volume_id, enrollment_epoch, attestation_id),
  UNIQUE(volume_id, enrollment_epoch, attestation_generation, attestation_sha256),
  FOREIGN KEY(volume_id, enrollment_epoch, volume_key_fingerprint)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch, key_fingerprint),
  CHECK(required_tombstone_generation = reconciled_tombstone_generation),
  CHECK(subject_storage_scope_count = resident_locator_count),
  CHECK(resident_locator_count <= locator_count),
  CHECK(
    (predecessor_attestation_generation = 0 AND predecessor_attestation_sha256 =
      'af14da54b5862fedcda27f7b1ca9ccd2be4271870efa7707d6f2e308efd874c2')
    OR predecessor_attestation_generation >= 1
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_volume_storage_attestations_latest
  ON jobs_runner_volume_storage_attestations(volume_id, enrollment_epoch, attestation_generation DESC);

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_volume_storage_attestations_no_update
BEFORE UPDATE ON jobs_runner_volume_storage_attestations
BEGIN
  SELECT RAISE(ABORT, 'runner volume storage attestation is append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_volume_storage_attestations_no_delete
BEFORE DELETE ON jobs_runner_volume_storage_attestations
BEGIN
  SELECT RAISE(ABORT, 'runner volume storage attestation is append-only');
END;

CREATE TABLE IF NOT EXISTS jobs_runner_volume_authority_uses (
  volume_id              TEXT NOT NULL,
  enrollment_epoch       INTEGER NOT NULL CHECK(enrollment_epoch >= 1),
  request_id              TEXT NOT NULL,
  operation               TEXT NOT NULL,
  payload_sha256          TEXT NOT NULL CHECK(length(payload_sha256) = 64),
  process_instance_id     TEXT NOT NULL,
  issued_at_ms            INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  consumed_at_ms          INTEGER NOT NULL CHECK(consumed_at_ms >= 0),
  PRIMARY KEY(volume_id, enrollment_epoch, request_id),
  FOREIGN KEY(volume_id, enrollment_epoch)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch)
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_volume_authority_uses_consumed
  ON jobs_runner_volume_authority_uses(consumed_at_ms);

CREATE TABLE IF NOT EXISTS jobs_execution_lease_volume_bindings (
  run_id                  TEXT PRIMARY KEY
    REFERENCES jobs_execution_leases(run_id) ON DELETE CASCADE,
  volume_id               TEXT NOT NULL,
  volume_epoch            INTEGER NOT NULL CHECK(volume_epoch >= 1),
  process_instance_id     TEXT NOT NULL,
  purge_subject_sha256    TEXT NOT NULL CHECK(length(purge_subject_sha256) = 64),
  bound_at_ms             INTEGER NOT NULL CHECK(bound_at_ms >= 0),
  FOREIGN KEY(volume_id, volume_epoch)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch)
);

CREATE INDEX IF NOT EXISTS idx_jobs_execution_lease_volume_bindings_volume
  ON jobs_execution_lease_volume_bindings(volume_id, volume_epoch, bound_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_account_subjects (
  account_id          TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  purge_subject       TEXT NOT NULL UNIQUE CHECK(length(purge_subject) = 43),
  legacy_unresolved   INTEGER NOT NULL DEFAULT 0 CHECK(legacy_unresolved IN (0, 1)),
  created_at_ms       INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms       INTEGER NOT NULL CHECK(updated_at_ms >= 0)
);

-- Deliberately no account FK: opaque subject history survives account-row
-- deletion so restored data can never silently become live again.
CREATE TABLE IF NOT EXISTS jobs_runner_volume_residencies (
  purge_subject         TEXT NOT NULL CHECK(length(purge_subject) = 43),
  volume_id             TEXT NOT NULL,
  volume_epoch          INTEGER NOT NULL CHECK(volume_epoch >= 1),
  state                 TEXT NOT NULL DEFAULT 'resident'
    CHECK(state IN ('resident', 'purged')),
  purge_generation      INTEGER NOT NULL DEFAULT 0 CHECK(purge_generation >= 0),
  first_recorded_at_ms  INTEGER NOT NULL CHECK(first_recorded_at_ms >= 0),
  last_recorded_at_ms   INTEGER NOT NULL CHECK(last_recorded_at_ms >= 0),
  PRIMARY KEY(purge_subject, volume_id, volume_epoch),
  FOREIGN KEY(volume_id, volume_epoch)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch)
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_residencies_subject_state
  ON jobs_runner_volume_residencies(purge_subject, state, volume_id, volume_epoch);
CREATE INDEX IF NOT EXISTS idx_jobs_runner_residencies_volume_state
  ON jobs_runner_volume_residencies(volume_id, volume_epoch, state, last_recorded_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_volume_destructions (
  destruction_id             TEXT PRIMARY KEY,
  volume_id                  TEXT NOT NULL,
  volume_epoch               INTEGER NOT NULL CHECK(volume_epoch >= 1),
  volume_key_fingerprint     TEXT NOT NULL CHECK(length(volume_key_fingerprint) = 64),
  provider                   TEXT NOT NULL,
  provider_resource_id       TEXT NOT NULL,
  resource_fingerprint       TEXT NOT NULL CHECK(length(resource_fingerprint) = 64),
  evidence_type              TEXT NOT NULL
    CHECK(evidence_type IN ('provider_volume_destroyed', 'physical_device_destroyed')),
  snapshot_inventory_sha256  TEXT NOT NULL CHECK(length(snapshot_inventory_sha256) = 64),
  evidence_sha256            TEXT NOT NULL UNIQUE CHECK(length(evidence_sha256) = 64),
  authorization_ref          TEXT NOT NULL,
  authorized_by              TEXT NOT NULL,
  occurred_at_ms             INTEGER NOT NULL CHECK(occurred_at_ms >= 0),
  recorded_at_ms             INTEGER NOT NULL CHECK(recorded_at_ms >= occurred_at_ms),
  details_json               TEXT NOT NULL,
  UNIQUE(volume_id, volume_epoch),
  FOREIGN KEY(volume_id, volume_epoch, volume_key_fingerprint)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch, key_fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_volume_destructions_volume
  ON jobs_runner_volume_destructions(volume_id, volume_epoch, recorded_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_purge_requests (
  request_id                 TEXT PRIMARY KEY,
  deletion_request_id        TEXT NOT NULL,
  predecessor_request_id     TEXT REFERENCES jobs_runner_purge_requests(request_id),
  account_id                 TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  purge_subject              TEXT NOT NULL CHECK(length(purge_subject) = 43),
  purge_generation           INTEGER NOT NULL UNIQUE CHECK(purge_generation >= 1),
  legacy_inventory_generation INTEGER NOT NULL CHECK(legacy_inventory_generation >= 1),
  legacy_inventory_reconciliation_id TEXT NOT NULL,
  legacy_inventory_authority_id TEXT NOT NULL,
  legacy_inventory_authority_sha256 TEXT NOT NULL
    CHECK(length(legacy_inventory_authority_sha256) = 64),
  state                      TEXT NOT NULL CHECK(state IN ('pending', 'complete', 'superseded')),
  legacy_unresolved_count    INTEGER NOT NULL DEFAULT 0
    CHECK(legacy_unresolved_count >= 0),
  required_target_count      INTEGER NOT NULL DEFAULT 0
    CHECK(required_target_count >= 0),
  resolved_target_count      INTEGER NOT NULL DEFAULT 0
    CHECK(resolved_target_count >= 0),
  target_set_sha256          TEXT NOT NULL CHECK(length(target_set_sha256) = 64),
  legacy_resolution_ref      TEXT,
  legacy_resolution_sha256   TEXT,
  legacy_resolved_by         TEXT,
  legacy_resolved_at_ms      INTEGER,
  created_at_ms              INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms              INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  completed_at_ms            INTEGER,
  superseded_at_ms           INTEGER,
  CHECK(resolved_target_count <= required_target_count),
  CHECK(
    (state = 'pending' AND completed_at_ms IS NULL AND superseded_at_ms IS NULL)
    OR
    (state = 'complete' AND completed_at_ms IS NOT NULL
      AND superseded_at_ms IS NULL
      AND legacy_unresolved_count = 0
      AND resolved_target_count = required_target_count)
    OR
    (state = 'superseded' AND completed_at_ms IS NULL
      AND superseded_at_ms IS NOT NULL)
  ),
  CHECK(
    (legacy_resolution_ref IS NULL AND legacy_resolution_sha256 IS NULL
      AND legacy_resolved_by IS NULL AND legacy_resolved_at_ms IS NULL)
    OR
    (legacy_resolution_ref IS NOT NULL AND legacy_resolution_sha256 IS NOT NULL
      AND length(legacy_resolution_sha256) = 64
      AND legacy_resolved_by IS NOT NULL AND legacy_resolved_at_ms IS NOT NULL)
  ),
  UNIQUE(request_id, purge_generation),
  UNIQUE(account_id, deletion_request_id, legacy_inventory_generation),
  UNIQUE(
    request_id, purge_generation, purge_subject, target_set_sha256, required_target_count
  ),
  FOREIGN KEY(
    legacy_inventory_authority_id, legacy_inventory_generation,
    legacy_inventory_authority_sha256
  ) REFERENCES jobs_runner_legacy_inventory_authorities(
    authority_id, authority_generation, authority_sha256
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_purge_requests_account_state
  ON jobs_runner_purge_requests(account_id, state, updated_at_ms);
CREATE INDEX IF NOT EXISTS idx_jobs_runner_purge_requests_deletion_attempt
  ON jobs_runner_purge_requests(deletion_request_id, purge_generation, updated_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_purge_targets (
  request_id              TEXT NOT NULL
    REFERENCES jobs_runner_purge_requests(request_id) ON DELETE CASCADE,
  volume_id               TEXT NOT NULL,
  volume_epoch            INTEGER NOT NULL CHECK(volume_epoch >= 1),
  volume_key_fingerprint  TEXT NOT NULL CHECK(length(volume_key_fingerprint) = 64),
  command_id              TEXT NOT NULL UNIQUE,
  command_json            TEXT NOT NULL,
  command_sha256          TEXT NOT NULL CHECK(length(command_sha256) = 64),
  server_key_id           TEXT NOT NULL,
  server_signature        TEXT NOT NULL CHECK(length(server_signature) = 86),
  state                   TEXT NOT NULL DEFAULT 'pending'
    CHECK(state IN ('pending', 'acknowledged', 'destroyed')),
  destruction_id          TEXT,
  ack_json                TEXT,
  ack_sha256              TEXT,
  ack_signature           TEXT,
  ack_process_instance_id TEXT,
  ack_command_sha256      TEXT,
  ack_inventory_before_count INTEGER,
  ack_inventory_before_sha256 TEXT,
  ack_inventory_after_count  INTEGER,
  ack_inventory_after_sha256 TEXT,
  ack_removed_entry_count INTEGER,
  ack_runner_build_id     TEXT,
  ack_completed_at_ms     INTEGER,
  created_at_ms           INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms           INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  acknowledged_at_ms      INTEGER,
  PRIMARY KEY(request_id, volume_id, volume_epoch),
  FOREIGN KEY(volume_id, volume_epoch, volume_key_fingerprint)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch, key_fingerprint),
  FOREIGN KEY(destruction_id)
    REFERENCES jobs_runner_volume_destructions(destruction_id),
  CHECK(
    (state = 'pending' AND destruction_id IS NULL AND ack_json IS NULL
      AND ack_sha256 IS NULL AND ack_signature IS NULL
      AND ack_process_instance_id IS NULL AND ack_command_sha256 IS NULL
      AND ack_inventory_before_count IS NULL AND ack_inventory_before_sha256 IS NULL
      AND ack_inventory_after_count IS NULL AND ack_inventory_after_sha256 IS NULL
      AND ack_removed_entry_count IS NULL AND ack_runner_build_id IS NULL
      AND ack_completed_at_ms IS NULL AND acknowledged_at_ms IS NULL)
    OR
    (state = 'acknowledged' AND destruction_id IS NULL AND ack_json IS NOT NULL
      AND ack_sha256 IS NOT NULL AND length(ack_sha256) = 64
      AND ack_signature IS NOT NULL AND length(ack_signature) = 86
      AND ack_process_instance_id IS NOT NULL
      AND ack_command_sha256 = command_sha256
      AND ack_inventory_before_count IS NOT NULL AND ack_inventory_before_count >= 0
      AND ack_inventory_before_sha256 IS NOT NULL
      AND length(ack_inventory_before_sha256) = 64
      AND ack_inventory_after_count = 0
      AND ack_inventory_after_sha256 =
        'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
      AND ack_removed_entry_count = ack_inventory_before_count
      AND ack_runner_build_id IS NOT NULL AND ack_completed_at_ms IS NOT NULL
      AND acknowledged_at_ms IS NOT NULL)
    OR
    (state = 'destroyed' AND destruction_id IS NOT NULL AND ack_json IS NULL
      AND ack_sha256 IS NULL AND ack_signature IS NULL
      AND ack_process_instance_id IS NULL AND ack_command_sha256 IS NULL
      AND ack_inventory_before_count IS NULL AND ack_inventory_before_sha256 IS NULL
      AND ack_inventory_after_count IS NULL AND ack_inventory_after_sha256 IS NULL
      AND ack_removed_entry_count IS NULL AND ack_runner_build_id IS NULL
      AND ack_completed_at_ms IS NULL AND acknowledged_at_ms IS NULL)
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_purge_targets_volume_state
  ON jobs_runner_purge_targets(volume_id, volume_epoch, state, updated_at_ms);
CREATE INDEX IF NOT EXISTS idx_jobs_runner_purge_targets_request_state
  ON jobs_runner_purge_targets(request_id, state, updated_at_ms);

CREATE TABLE IF NOT EXISTS jobs_runner_purge_tombstones (
  purge_subject          TEXT NOT NULL CHECK(length(purge_subject) = 43),
  request_id             TEXT PRIMARY KEY
    REFERENCES jobs_runner_purge_requests(request_id),
  purge_generation       INTEGER NOT NULL CHECK(purge_generation >= 1),
  tombstone_generation   INTEGER NOT NULL UNIQUE CHECK(tombstone_generation >= 1),
  target_set_sha256      TEXT NOT NULL CHECK(length(target_set_sha256) = 64),
  required_target_count  INTEGER NOT NULL CHECK(required_target_count >= 0),
  completed_at_ms        INTEGER NOT NULL CHECK(completed_at_ms >= 0),
  retention_policy       TEXT NOT NULL DEFAULT 'indefinite_managed_restore_safety'
    CHECK(retention_policy = 'indefinite_managed_restore_safety'),
  FOREIGN KEY(request_id, purge_generation)
    REFERENCES jobs_runner_purge_requests(request_id, purge_generation),
  FOREIGN KEY(
    request_id, purge_generation, purge_subject, target_set_sha256, required_target_count
  ) REFERENCES jobs_runner_purge_requests(
    request_id, purge_generation, purge_subject, target_set_sha256, required_target_count
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_purge_tombstones_request
  ON jobs_runner_purge_tombstones(request_id, completed_at_ms);

-- Post-completion commands never change the frozen required target set. They
-- keep a restored or newly enrolled key epoch in reconciliation until it has
-- enforced every retained pseudonymous tombstone.
CREATE TABLE IF NOT EXISTS jobs_runner_purge_enforcements (
  request_id              TEXT NOT NULL
    REFERENCES jobs_runner_purge_tombstones(request_id),
  volume_id               TEXT NOT NULL,
  volume_epoch            INTEGER NOT NULL CHECK(volume_epoch >= 1),
  volume_key_fingerprint  TEXT NOT NULL CHECK(length(volume_key_fingerprint) = 64),
  command_id              TEXT NOT NULL UNIQUE,
  command_json            TEXT NOT NULL,
  command_sha256          TEXT NOT NULL CHECK(length(command_sha256) = 64),
  server_key_id           TEXT NOT NULL,
  server_signature        TEXT NOT NULL CHECK(length(server_signature) = 86),
  state                   TEXT NOT NULL DEFAULT 'pending'
    CHECK(state IN ('pending', 'acknowledged')),
  ack_json                TEXT,
  ack_sha256              TEXT,
  ack_signature           TEXT,
  ack_process_instance_id TEXT,
  ack_command_sha256      TEXT,
  ack_inventory_before_count INTEGER,
  ack_inventory_before_sha256 TEXT,
  ack_inventory_after_count  INTEGER,
  ack_inventory_after_sha256 TEXT,
  ack_removed_entry_count INTEGER,
  ack_runner_build_id     TEXT,
  ack_completed_at_ms     INTEGER,
  created_at_ms           INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms           INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  acknowledged_at_ms      INTEGER,
  PRIMARY KEY(request_id, volume_id, volume_epoch),
  FOREIGN KEY(volume_id, volume_epoch, volume_key_fingerprint)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch, key_fingerprint),
  CHECK(
    (state = 'pending' AND ack_json IS NULL AND ack_sha256 IS NULL
      AND ack_signature IS NULL AND ack_process_instance_id IS NULL
      AND ack_command_sha256 IS NULL AND ack_inventory_before_count IS NULL
      AND ack_inventory_before_sha256 IS NULL AND ack_inventory_after_count IS NULL
      AND ack_inventory_after_sha256 IS NULL AND ack_removed_entry_count IS NULL
      AND ack_runner_build_id IS NULL AND ack_completed_at_ms IS NULL
      AND acknowledged_at_ms IS NULL)
    OR
    (state = 'acknowledged' AND ack_json IS NOT NULL AND ack_sha256 IS NOT NULL
      AND length(ack_sha256) = 64 AND ack_signature IS NOT NULL
      AND length(ack_signature) = 86 AND ack_process_instance_id IS NOT NULL
      AND ack_command_sha256 = command_sha256
      AND ack_inventory_before_count IS NOT NULL AND ack_inventory_before_count >= 0
      AND ack_inventory_before_sha256 IS NOT NULL
      AND length(ack_inventory_before_sha256) = 64
      AND ack_inventory_after_count = 0
      AND ack_inventory_after_sha256 =
        'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
      AND ack_removed_entry_count = ack_inventory_before_count
      AND ack_runner_build_id IS NOT NULL AND ack_completed_at_ms IS NOT NULL
      AND acknowledged_at_ms IS NOT NULL)
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_purge_enforcements_volume_state
  ON jobs_runner_purge_enforcements(volume_id, volume_epoch, state, updated_at_ms);

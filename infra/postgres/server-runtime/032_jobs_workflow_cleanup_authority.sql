-- Target: PostgreSQL
-- Phase 610: durable global Temporal-v1 inventory and account-bound workflow cleanup.
-- Legacy Temporal identifiers and continuation tokens, plus companion copies
-- of v2 known-run identifiers, are encrypted. Retained Phase609 v2 workflow
-- and first-run authority is bounded provider-opaque data and is deleted only
-- by the authorized account cascade.

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_inventory_generations (
  generation                       BIGINT PRIMARY KEY
    CHECK(generation BETWEEN 1 AND 9007199254740991),
  inventory_generation_id          TEXT NOT NULL UNIQUE
    CHECK(inventory_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  namespace_ciphertext             TEXT NOT NULL
    CHECK(length(namespace_ciphertext) BETWEEN 32 AND 16777216
      AND substring(namespace_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:'),
  namespace_hmac_sha256            TEXT NOT NULL
    CHECK(namespace_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  workflow_type                    TEXT NOT NULL CHECK(workflow_type = 'applicationWorkflow'),
  visibility_cutoff_ms             BIGINT NOT NULL
    CHECK(visibility_cutoff_ms BETWEEN 0 AND 253402300799999),
  confirmation_age_ms              BIGINT NOT NULL
    CHECK(confirmation_age_ms BETWEEN 1000 AND 600000),
  visibility_query_ciphertext      TEXT NOT NULL
    CHECK(length(visibility_query_ciphertext) BETWEEN 32 AND 16777216
      AND substring(visibility_query_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:'),
  query_digest_sha256              TEXT NOT NULL
    CHECK(query_digest_sha256 ~ '^[0-9a-f]{64}$'),
  query_hmac_sha256                TEXT NOT NULL
    CHECK(query_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  state                            TEXT NOT NULL CHECK(state IN (
    'scanning', 'draining', 'awaiting_second_scan', 'complete', 'identity_conflict'
  )),
  scan_pass                        BIGINT NOT NULL CHECK(scan_pass IN (1, 2)),
  page_index                       BIGINT NOT NULL
    CHECK(page_index BETWEEN 0 AND 4095),
  predecessor_page_digest_sha256   TEXT NOT NULL
    CHECK(predecessor_page_digest_sha256 ~ '^[0-9a-f]{64}$'),
  page_token_ciphertext            TEXT
    CHECK(page_token_ciphertext IS NULL OR (
      length(page_token_ciphertext) BETWEEN 32 AND 16777216
      AND substring(page_token_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  page_token_hmac_sha256           TEXT
    CHECK(page_token_hmac_sha256 IS NULL OR page_token_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  request_epoch                    BIGINT NOT NULL DEFAULT 0
    CHECK(request_epoch BETWEEN 0 AND 9007199254740991),
  fence                            BIGINT NOT NULL DEFAULT 0
    CHECK(fence BETWEEN 0 AND 9007199254740991),
  request_id                       TEXT
    CHECK(request_id IS NULL OR request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  first_request_started_at_ms      BIGINT,
  last_outcome_code                TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'page_recorded', 'transport_unknown', 'gateway_unavailable', 'identity_conflict'
  )),
  lease_owner                      TEXT,
  lease_token_sha256               TEXT,
  lease_expires_at_ms              BIGINT,
  next_attempt_at_ms               BIGINT NOT NULL
    CHECK(next_attempt_at_ms BETWEEN 0 AND 9007199254740991),
  completion_epoch                 BIGINT NOT NULL DEFAULT 1
    CHECK(completion_epoch BETWEEN 1 AND 9007199254740991),
  first_zero_observed_at_ms        BIGINT,
  completed_at_ms                  BIGINT,
  completion_digest_sha256         TEXT,
  revalidate_after_ms              BIGINT,
  created_at_ms                    BIGINT NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                    BIGINT NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(generation, query_digest_sha256),
  UNIQUE(generation, inventory_generation_id, query_digest_sha256),
  UNIQUE(namespace_hmac_sha256, workflow_type, visibility_cutoff_ms, query_digest_sha256),
  CHECK((page_token_ciphertext IS NULL) = (page_token_hmac_sha256 IS NULL)),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND request_id IS NULL)
    OR (lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 ~ '^[0-9a-f]{64}$'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND request_id IS NOT NULL)
  ),
  CHECK((state = 'awaiting_second_scan') = (first_zero_observed_at_ms IS NOT NULL)),
  CHECK(state <> 'complete' OR (
    scan_pass = 2 AND completed_at_ms IS NOT NULL
      AND revalidate_after_ms IS NOT NULL AND revalidate_after_ms > completed_at_ms
      AND completion_digest_sha256 ~ '^[0-9a-f]{64}$'
  )),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_inventory_head (
  singleton_id                     BIGINT PRIMARY KEY CHECK(singleton_id = 1),
  generation                       BIGINT NOT NULL,
  inventory_generation_id          TEXT NOT NULL UNIQUE,
  query_digest_sha256              TEXT NOT NULL,
  updated_at_ms                    BIGINT NOT NULL,
  FOREIGN KEY(generation, inventory_generation_id, query_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_generations(
      generation, inventory_generation_id, query_digest_sha256
    ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_inventory_pages (
  generation                       BIGINT NOT NULL,
  completion_epoch                 BIGINT NOT NULL,
  scan_pass                        BIGINT NOT NULL CHECK(scan_pass IN (1, 2)),
  page_index                       BIGINT NOT NULL CHECK(page_index BETWEEN 0 AND 4095),
  request_epoch                    BIGINT NOT NULL CHECK(request_epoch BETWEEN 1 AND 9007199254740991),
  fence                            BIGINT NOT NULL CHECK(fence BETWEEN 1 AND 9007199254740991),
  request_id                       TEXT NOT NULL UNIQUE
    CHECK(request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  predecessor_page_digest_sha256   TEXT,
  input_page_token_ciphertext      TEXT,
  input_page_token_hmac_sha256     TEXT,
  next_page_token_ciphertext       TEXT,
  next_page_token_hmac_sha256      TEXT,
  raw_ciphertexts_scrubbed         BOOLEAN NOT NULL DEFAULT FALSE,
  page_target_count                BIGINT NOT NULL CHECK(page_target_count BETWEEN 0 AND 100),
  page_targets_digest_sha256       TEXT NOT NULL,
  page_digest_sha256               TEXT NOT NULL,
  evidence_digest_sha256           TEXT NOT NULL,
  recorded_at_ms                   BIGINT NOT NULL,
  PRIMARY KEY(generation, completion_epoch, scan_pass, page_index),
  UNIQUE(generation, completion_epoch, scan_pass, page_index, page_digest_sha256),
  FOREIGN KEY(generation) REFERENCES jobs_workflow_legacy_inventory_generations(generation)
    ON DELETE RESTRICT,
  CHECK(
    (NOT raw_ciphertexts_scrubbed
      AND (input_page_token_ciphertext IS NULL) = (input_page_token_hmac_sha256 IS NULL)
      AND (next_page_token_ciphertext IS NULL) = (next_page_token_hmac_sha256 IS NULL))
    OR (raw_ciphertexts_scrubbed
      AND input_page_token_ciphertext IS NULL
      AND next_page_token_ciphertext IS NULL)
  ),
  CHECK(input_page_token_ciphertext IS NULL OR (
    length(input_page_token_ciphertext) BETWEEN 32 AND 16777216
      AND substring(input_page_token_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  CHECK(next_page_token_ciphertext IS NULL OR (
    length(next_page_token_ciphertext) BETWEEN 32 AND 16777216
      AND substring(next_page_token_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  CHECK(input_page_token_hmac_sha256 IS NULL OR input_page_token_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(next_page_token_hmac_sha256 IS NULL OR next_page_token_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK((page_index = 0) = (predecessor_page_digest_sha256 IS NULL)),
  CHECK(predecessor_page_digest_sha256 IS NULL
    OR predecessor_page_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(page_targets_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(page_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(evidence_digest_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_targets (
  generation                        BIGINT NOT NULL,
  target_identity_hmac_sha256       TEXT NOT NULL,
  workflow_id_ciphertext            TEXT,
  workflow_id_hmac_sha256           TEXT NOT NULL,
  run_id_ciphertext                 TEXT,
  run_id_hmac_sha256                TEXT NOT NULL,
  first_execution_run_id_ciphertext TEXT,
  first_execution_run_id_hmac_sha256 TEXT NOT NULL,
  target_digest_sha256              TEXT NOT NULL,
  raw_ids_scrubbed                  BOOLEAN NOT NULL DEFAULT FALSE,
  discovered_completion_epoch       BIGINT NOT NULL,
  discovered_scan_pass              BIGINT NOT NULL CHECK(discovered_scan_pass IN (1, 2)),
  discovered_page_index             BIGINT NOT NULL,
  discovered_page_digest_sha256     TEXT NOT NULL,
  observed_status                   TEXT NOT NULL CHECK(observed_status IN ('running', 'closed')),
  target_state                      TEXT NOT NULL CHECK(target_state IN (
    'running_wait', 'delete_pending', 'absence_pending',
    'absence_proved', 'identity_conflict'
  )),
  proof_epoch                       BIGINT NOT NULL DEFAULT 1
    CHECK(proof_epoch BETWEEN 1 AND 9007199254740991),
  positive_reset_required           BOOLEAN NOT NULL DEFAULT FALSE,
  observation_pass                  BIGINT NOT NULL DEFAULT 1 CHECK(observation_pass IN (1, 2)),
  first_absence_observed_at_ms      BIGINT,
  request_epoch                     BIGINT NOT NULL DEFAULT 0,
  fence                             BIGINT NOT NULL DEFAULT 0,
  request_id                        TEXT,
  first_request_started_at_ms       BIGINT,
  last_outcome_code                 TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'running', 'retry', 'absence_proved', 'identity_conflict',
    'transport_unknown', 'gateway_unavailable'
  )),
  lease_owner                       TEXT,
  lease_token_sha256                TEXT,
  lease_expires_at_ms               BIGINT,
  next_attempt_at_ms                BIGINT NOT NULL,
  absence_proved_at_ms              BIGINT,
  created_at_ms                     BIGINT NOT NULL,
  updated_at_ms                     BIGINT NOT NULL,
  PRIMARY KEY(generation, target_identity_hmac_sha256),
  UNIQUE(generation, workflow_id_hmac_sha256, run_id_hmac_sha256),
  UNIQUE(generation, target_identity_hmac_sha256, target_digest_sha256),
  FOREIGN KEY(generation, discovered_completion_epoch, discovered_scan_pass,
    discovered_page_index, discovered_page_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_pages(
      generation, completion_epoch, scan_pass, page_index, page_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(target_identity_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(workflow_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(run_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(first_execution_run_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(target_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(workflow_id_ciphertext IS NULL OR (
    length(workflow_id_ciphertext) BETWEEN 32 AND 16777216
      AND substring(workflow_id_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  CHECK(run_id_ciphertext IS NULL OR (
    length(run_id_ciphertext) BETWEEN 32 AND 16777216
      AND substring(run_id_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  CHECK(first_execution_run_id_ciphertext IS NULL OR (
    length(first_execution_run_id_ciphertext) BETWEEN 32 AND 16777216
      AND substring(first_execution_run_id_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  CHECK(
    (NOT raw_ids_scrubbed AND workflow_id_ciphertext IS NOT NULL
      AND run_id_ciphertext IS NOT NULL
      AND first_execution_run_id_ciphertext IS NOT NULL)
    OR (raw_ids_scrubbed AND workflow_id_ciphertext IS NULL
      AND run_id_ciphertext IS NULL
      AND first_execution_run_id_ciphertext IS NULL)
  ),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND request_id IS NULL)
    OR (lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 ~ '^[0-9a-f]{64}$'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND request_id ~ '^[A-Za-z0-9_-]{20,128}$')
  ),
  CHECK(target_state <> 'absence_proved' OR absence_proved_at_ms IS NOT NULL),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_target_observations (
  id                                TEXT PRIMARY KEY,
  generation                        BIGINT NOT NULL,
  target_identity_hmac_sha256       TEXT NOT NULL,
  target_digest_sha256              TEXT NOT NULL,
  proof_epoch                       BIGINT NOT NULL
    CHECK(proof_epoch BETWEEN 1 AND 9007199254740991),
  observation_pass                  BIGINT NOT NULL CHECK(observation_pass IN (1, 2)),
  request_epoch                     BIGINT NOT NULL,
  cleanup_fence                     BIGINT NOT NULL,
  cleanup_request_id                TEXT NOT NULL UNIQUE,
  outcome                           TEXT NOT NULL CHECK(outcome IN (
    'running', 'retry', 'absence_proved', 'identity_conflict'
  )),
  describe_state                    TEXT NOT NULL CHECK(describe_state IN (
    'found', 'not_found', 'not_checked', 'unavailable'
  )),
  history_state                     TEXT NOT NULL CHECK(history_state IN (
    'found', 'not_found', 'not_checked', 'unavailable'
  )),
  visibility_state                  TEXT NOT NULL CHECK(visibility_state IN (
    'found', 'not_found', 'not_checked', 'unavailable'
  )),
  evidence_digest_sha256            TEXT NOT NULL,
  recorded_at_ms                    BIGINT NOT NULL,
  UNIQUE(generation, target_identity_hmac_sha256, request_epoch, cleanup_fence),
  FOREIGN KEY(generation, target_identity_hmac_sha256, target_digest_sha256)
    REFERENCES jobs_workflow_legacy_targets(
      generation, target_identity_hmac_sha256, target_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(length(id) BETWEEN 20 AND 128),
  CHECK(target_identity_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(target_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(cleanup_request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(evidence_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(outcome <> 'absence_proved' OR (
    describe_state = 'not_found' AND history_state = 'not_found'
      AND visibility_state = 'not_found'
  ))
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_zero_observations (
  generation                       BIGINT NOT NULL,
  completion_epoch                 BIGINT NOT NULL,
  scan_pass                        BIGINT NOT NULL CHECK(scan_pass IN (1, 2)),
  final_page_index                 BIGINT NOT NULL CHECK(final_page_index = 0),
  final_page_digest_sha256         TEXT NOT NULL,
  zero_digest_sha256               TEXT NOT NULL,
  recorded_at_ms                   BIGINT NOT NULL,
  PRIMARY KEY(generation, completion_epoch, scan_pass),
  UNIQUE(generation, completion_epoch, zero_digest_sha256),
  FOREIGN KEY(generation, completion_epoch, scan_pass, final_page_index,
    final_page_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_pages(
      generation, completion_epoch, scan_pass, page_index, page_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(final_page_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(zero_digest_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_completion_tombstones (
  tombstone_id                     TEXT PRIMARY KEY,
  account_subject_hmac_sha256      TEXT NOT NULL,
  account_generation              BIGINT NOT NULL,
  workflow_cleanup_generation     BIGINT NOT NULL,
  cleanup_generation_id           TEXT NOT NULL,
  target_set_hmac_sha256           TEXT NOT NULL,
  legacy_generation               BIGINT NOT NULL,
  legacy_inventory_generation_id  TEXT NOT NULL,
  legacy_completion_epoch         BIGINT NOT NULL,
  legacy_completion_digest_sha256 TEXT NOT NULL,
  legacy_revalidate_after_ms       BIGINT NOT NULL,
  completion_digest_sha256        TEXT NOT NULL,
  completed_at_ms                 BIGINT NOT NULL,
  UNIQUE(tombstone_id, completion_digest_sha256),
  UNIQUE(account_subject_hmac_sha256, account_generation,
    cleanup_generation_id, legacy_completion_epoch,
    legacy_completion_digest_sha256),
  CHECK(length(tombstone_id) BETWEEN 20 AND 128),
  CHECK(length(cleanup_generation_id) BETWEEN 20 AND 128),
  CHECK(length(legacy_inventory_generation_id) BETWEEN 20 AND 128),
  CHECK(account_subject_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(target_set_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(legacy_completion_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(legacy_revalidate_after_ms > completed_at_ms),
  CHECK(completion_digest_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_account_bindings (
  account_id                       TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  account_generation              BIGINT NOT NULL,
  workflow_cleanup_generation     BIGINT NOT NULL,
  cleanup_generation_id           TEXT NOT NULL UNIQUE,
  target_set_hmac_sha256           TEXT NOT NULL,
  legacy_generation               BIGINT NOT NULL,
  legacy_inventory_generation_id  TEXT NOT NULL,
  legacy_query_digest_sha256       TEXT NOT NULL,
  state                            TEXT NOT NULL CHECK(state IN ('frozen', 'draining', 'complete')),
  completion_tombstone_id          TEXT,
  completion_digest_sha256         TEXT,
  object_sweep_started_at_ms       BIGINT,
  object_sweep_deleted_count       BIGINT NOT NULL DEFAULT 0
    CHECK(object_sweep_deleted_count BETWEEN 0 AND 9007199254740991),
  object_sweep_orphan_count        BIGINT NOT NULL DEFAULT 0
    CHECK(object_sweep_orphan_count BETWEEN 0 AND 9007199254740991),
  hard_delete_authorized_at_ms     BIGINT,
  hard_delete_sweep_attempt_id     TEXT,
  hard_delete_authorization_digest_sha256 TEXT,
  created_at_ms                    BIGINT NOT NULL,
  updated_at_ms                    BIGINT NOT NULL,
  UNIQUE(account_id, account_generation),
  UNIQUE(account_id, workflow_cleanup_generation, target_set_hmac_sha256),
  FOREIGN KEY(account_id, workflow_cleanup_generation, target_set_hmac_sha256)
    REFERENCES jobs_workflow_cleanup_generations(
      account_id, generation, target_set_hmac_sha256
    ) ON DELETE CASCADE,
  FOREIGN KEY(legacy_generation, legacy_inventory_generation_id,
    legacy_query_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_generations(
      generation, inventory_generation_id, query_digest_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(completion_tombstone_id, completion_digest_sha256)
    REFERENCES jobs_workflow_cleanup_completion_tombstones(
      tombstone_id, completion_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK((completion_tombstone_id IS NULL) = (completion_digest_sha256 IS NULL)),
  CHECK((hard_delete_authorized_at_ms IS NULL) =
    (hard_delete_authorization_digest_sha256 IS NULL)
    AND (hard_delete_authorized_at_ms IS NULL) =
    (hard_delete_sweep_attempt_id IS NULL)),
  CHECK(hard_delete_sweep_attempt_id IS NULL
    OR hard_delete_sweep_attempt_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(hard_delete_authorization_digest_sha256 IS NULL
    OR hard_delete_authorization_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK((object_sweep_deleted_count = 0 AND object_sweep_orphan_count = 0)
    OR object_sweep_started_at_ms IS NOT NULL),
  CHECK(state <> 'complete' OR completion_tombstone_id IS NOT NULL),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE INDEX IF NOT EXISTS idx_jobs_workflow_legacy_generation_due
  ON jobs_workflow_legacy_inventory_generations(state, next_attempt_at_ms, generation);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_legacy_target_due
  ON jobs_workflow_legacy_targets(target_state, next_attempt_at_ms, generation, updated_at_ms);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_cleanup_binding_legacy
  ON jobs_workflow_cleanup_account_bindings(legacy_generation, state, account_id);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_target_authorities (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     BIGINT NOT NULL,
  workflow_id                      TEXT NOT NULL,
  known_run_epoch                  BIGINT NOT NULL DEFAULT 1
    CHECK(known_run_epoch BETWEEN 1 AND 9007199254740991),
  positive_reset_required          BOOLEAN NOT NULL DEFAULT FALSE,
  known_run_set_digest_sha256      TEXT NOT NULL,
  target_digest_sha256             TEXT NOT NULL,
  observation_pass                 BIGINT NOT NULL DEFAULT 1 CHECK(observation_pass IN (1, 2)),
  first_absence_observed_at_ms     BIGINT,
  request_epoch                    BIGINT NOT NULL DEFAULT 0
    CHECK(request_epoch BETWEEN 0 AND 9007199254740991),
  cleanup_fence                    BIGINT NOT NULL DEFAULT 0
    CHECK(cleanup_fence BETWEEN 0 AND 9007199254740991),
  cleanup_request_id               TEXT,
  first_request_started_at_ms      BIGINT,
  last_outcome_code                TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'pending', 'absence_observed', 'transport_unknown', 'gateway_unavailable',
    'identity_conflict'
  )),
  lease_owner                      TEXT,
  lease_token_sha256               TEXT,
  lease_expires_at_ms              BIGINT,
  next_attempt_at_ms               BIGINT NOT NULL,
  created_at_ms                    BIGINT NOT NULL,
  updated_at_ms                    BIGINT NOT NULL,
  PRIMARY KEY(account_id, workflow_cleanup_generation, workflow_id),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_targets(account_id, generation, workflow_id)
    ON DELETE CASCADE,
  CHECK(known_run_set_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(target_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK((observation_pass = 2) = (first_absence_observed_at_ms IS NOT NULL)),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND cleanup_request_id IS NULL)
    OR (lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 ~ '^[0-9a-f]{64}$'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND cleanup_request_id ~ '^[A-Za-z0-9_-]{20,128}$')
  ),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_known_runs (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     BIGINT NOT NULL,
  workflow_id                      TEXT NOT NULL,
  run_id_hmac_sha256               TEXT NOT NULL,
  run_id_ciphertext                TEXT NOT NULL,
  discovered_request_epoch        BIGINT NOT NULL,
  discovered_cleanup_fence        BIGINT NOT NULL,
  run_identity_digest_sha256       TEXT NOT NULL,
  created_at_ms                    BIGINT NOT NULL,
  PRIMARY KEY(account_id, workflow_cleanup_generation, workflow_id, run_id_hmac_sha256),
  UNIQUE(account_id, workflow_cleanup_generation, workflow_id, run_identity_digest_sha256),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_v2_target_authorities(
      account_id, workflow_cleanup_generation, workflow_id
    ) ON DELETE CASCADE,
  CHECK(run_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(run_identity_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(length(run_id_ciphertext) BETWEEN 32 AND 16777216
    AND substring(run_id_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_run_observations (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     BIGINT NOT NULL,
  workflow_id                      TEXT NOT NULL,
  known_run_epoch                  BIGINT NOT NULL,
  observation_pass                BIGINT NOT NULL CHECK(observation_pass IN (1, 2)),
  subject_kind                     TEXT NOT NULL CHECK(subject_kind IN ('run', 'workflow')),
  run_id_hmac_sha256               TEXT NOT NULL,
  target_digest_sha256             TEXT NOT NULL,
  request_epoch                    BIGINT NOT NULL,
  cleanup_fence                    BIGINT NOT NULL,
  cleanup_request_id               TEXT NOT NULL,
  evidence_digest_sha256           TEXT NOT NULL,
  describe_state                   TEXT NOT NULL CHECK(describe_state = 'not_found'),
  history_state                    TEXT NOT NULL CHECK(history_state = 'not_found'),
  visibility_state                 TEXT NOT NULL CHECK(visibility_state = 'not_found'),
  recorded_at_ms                   BIGINT NOT NULL,
  PRIMARY KEY(account_id, workflow_cleanup_generation, workflow_id,
    known_run_epoch, observation_pass, subject_kind, run_id_hmac_sha256),
  UNIQUE(account_id, workflow_cleanup_generation, workflow_id,
    request_epoch, cleanup_fence, observation_pass, subject_kind, run_id_hmac_sha256),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_v2_target_authorities(
      account_id, workflow_cleanup_generation, workflow_id
    ) ON DELETE CASCADE,
  CHECK(run_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(target_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(cleanup_request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(evidence_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK((subject_kind = 'workflow') =
    (run_id_hmac_sha256 = '0000000000000000000000000000000000000000000000000000000000000000'))
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_receipts (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     BIGINT NOT NULL,
  workflow_id                      TEXT NOT NULL,
  cleanup_request_id               TEXT NOT NULL PRIMARY KEY,
  request_epoch                    BIGINT NOT NULL,
  cleanup_fence                    BIGINT NOT NULL,
  known_run_epoch                  BIGINT NOT NULL,
  target_digest_sha256             TEXT NOT NULL,
  outcome                          TEXT NOT NULL CHECK(outcome IN ('pending', 'absence_observed')),
  reason                           TEXT NOT NULL CHECK(reason IN (
    'termination_pending', 'history_delete_pending', 'visibility_pending',
    'temporal_unavailable', 'absence_observed'
  )),
  first_execution_run_id_ciphertext TEXT,
  first_execution_run_id_hmac_sha256 TEXT,
  response_run_set_digest_sha256   TEXT NOT NULL,
  evidence_digest_sha256           TEXT NOT NULL,
  recorded_at_ms                   BIGINT NOT NULL,
  UNIQUE(account_id, workflow_cleanup_generation, workflow_id,
    request_epoch, cleanup_fence),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_v2_target_authorities(
      account_id, workflow_cleanup_generation, workflow_id
    ) ON DELETE CASCADE,
  CHECK((first_execution_run_id_ciphertext IS NULL) =
    (first_execution_run_id_hmac_sha256 IS NULL)),
  CHECK(first_execution_run_id_ciphertext IS NULL OR (
    length(first_execution_run_id_ciphertext) BETWEEN 32 AND 16777216
      AND substring(first_execution_run_id_ciphertext FROM 1 FOR 14) = 'bluey-jobs:v1:')),
  CHECK(first_execution_run_id_hmac_sha256 IS NULL
    OR first_execution_run_id_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(target_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(response_run_set_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(evidence_digest_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_authorizations (
  account_id                       TEXT NOT NULL,
  account_generation              BIGINT NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  runner_purge_request_id          TEXT NOT NULL,
  runner_purge_generation          BIGINT NOT NULL CHECK(runner_purge_generation >= 1),
  runner_purge_tombstone_generation BIGINT NOT NULL
    CHECK(runner_purge_tombstone_generation >= 1),
  runner_legacy_inventory_generation BIGINT NOT NULL
    CHECK(runner_legacy_inventory_generation >= 1),
  runner_legacy_reconciliation_id  TEXT NOT NULL,
  runner_legacy_authority_id        TEXT NOT NULL,
  runner_legacy_authority_sha256    TEXT NOT NULL,
  runner_authority_digest_sha256    TEXT NOT NULL,
  completion_tombstone_id          TEXT NOT NULL,
  completion_digest_sha256         TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  scope_set_digest_sha256          TEXT NOT NULL,
  scope_count                      BIGINT NOT NULL CHECK(scope_count BETWEEN 1 AND 2),
  known_object_count               BIGINT NOT NULL
    CHECK(known_object_count BETWEEN 0 AND 9007199254740991),
  sealed                           BOOLEAN NOT NULL DEFAULT FALSE,
  authorized_at_ms                 BIGINT NOT NULL,
  PRIMARY KEY(account_id, account_generation, sweep_attempt_id),
  UNIQUE(account_id, account_generation, authorization_digest_sha256),
  UNIQUE(account_id, account_generation, sweep_attempt_id,
    authorization_digest_sha256),
  FOREIGN KEY(account_id, account_generation)
    REFERENCES jobs_workflow_cleanup_account_bindings(account_id, account_generation)
    ON DELETE CASCADE,
  FOREIGN KEY(completion_tombstone_id, completion_digest_sha256)
    REFERENCES jobs_workflow_cleanup_completion_tombstones(
      tombstone_id, completion_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(sweep_attempt_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(runner_purge_request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(runner_legacy_reconciliation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(runner_legacy_authority_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(runner_legacy_authority_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(runner_authority_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(completion_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(authorization_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(scope_set_digest_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_authorization_scopes (
  account_id                       TEXT NOT NULL,
  account_generation              BIGINT NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  manifest_digest_sha256           TEXT NOT NULL,
  object_count                     BIGINT NOT NULL
    CHECK(object_count BETWEEN 0 AND 9007199254740991),
  prefix_sweep                     BOOLEAN NOT NULL CHECK(prefix_sweep),
  created_at_ms                    BIGINT NOT NULL,
  PRIMARY KEY(account_id, account_generation, sweep_attempt_id, scope_id),
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorizations(
      account_id, account_generation, sweep_attempt_id
    ) ON DELETE CASCADE,
  CHECK(scope_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(manifest_digest_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_manifest_objects (
  account_id                       TEXT NOT NULL,
  account_generation              BIGINT NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  object_key_hmac_sha256           TEXT NOT NULL,
  created_at_ms                    BIGINT NOT NULL,
  PRIMARY KEY(account_id, account_generation, sweep_attempt_id, scope_id,
    object_key_hmac_sha256),
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id, scope_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorization_scopes(
      account_id, account_generation, sweep_attempt_id, scope_id
    ) ON DELETE CASCADE,
  CHECK(object_key_hmac_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_progress (
  account_id                       TEXT NOT NULL,
  account_generation              BIGINT NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  object_key_hmac_sha256           TEXT NOT NULL,
  deleted_at_ms                    BIGINT NOT NULL,
  PRIMARY KEY(account_id, account_generation, scope_id, object_key_hmac_sha256),
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorizations(
      account_id, account_generation, sweep_attempt_id
    ) ON DELETE CASCADE,
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id,
    authorization_digest_sha256)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorizations(
      account_id, account_generation, sweep_attempt_id,
      authorization_digest_sha256
    ) ON DELETE CASCADE,
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id, scope_id,
    object_key_hmac_sha256)
    REFERENCES jobs_workflow_cleanup_object_sweep_manifest_objects(
      account_id, account_generation, sweep_attempt_id, scope_id,
      object_key_hmac_sha256
    ) ON DELETE CASCADE,
  CHECK(authorization_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(scope_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(object_key_hmac_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_scopes (
  account_id                       TEXT NOT NULL,
  account_generation              BIGINT NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  deleted_count                    BIGINT NOT NULL CHECK(deleted_count >= 0),
  orphan_count                     BIGINT NOT NULL CHECK(orphan_count >= 0),
  result_digest_sha256             TEXT NOT NULL,
  recorded_at_ms                   BIGINT NOT NULL,
  PRIMARY KEY(account_id, account_generation, sweep_attempt_id, scope_id),
  UNIQUE(account_id, account_generation, sweep_attempt_id, result_digest_sha256),
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorizations(
      account_id, account_generation, sweep_attempt_id
    ) ON DELETE CASCADE,
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id,
    authorization_digest_sha256)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorizations(
      account_id, account_generation, sweep_attempt_id,
      authorization_digest_sha256
    ) ON DELETE CASCADE,
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id, scope_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorization_scopes(
      account_id, account_generation, sweep_attempt_id, scope_id
    ) ON DELETE CASCADE,
  CHECK(sweep_attempt_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(authorization_digest_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK(scope_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(result_digest_sha256 ~ '^[0-9a-f]{64}$')
);

-- Transaction-scoped-by-trigger cascade token. It is inserted only by the
-- exact account DELETE guard and removed by the caller after DELETE returns.
CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_hard_delete_cascade_tokens (
  account_id                       TEXT PRIMARY KEY,
  account_generation              BIGINT NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  created_at_ms                    BIGINT NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id)
    ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
  CHECK(sweep_attempt_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  CHECK(authorization_digest_sha256 ~ '^[0-9a-f]{64}$')
);

-- Drop dependent views first so idempotent replay never needs CASCADE.
DROP VIEW IF EXISTS jobs_workflow_cleanup_hard_delete_ready;
DROP VIEW IF EXISTS jobs_workflow_cleanup_ready_object_sweeps;
DROP VIEW IF EXISTS jobs_workflow_cleanup_v2_proved_targets;
CREATE VIEW jobs_workflow_cleanup_v2_proved_targets AS
SELECT target.account_id, target.generation, target.target_set_hmac_sha256,
       target.workflow_id
  FROM jobs_workflow_cleanup_targets target
  JOIN jobs_workflow_cleanup_v2_target_authorities authority
    ON authority.account_id = target.account_id
   AND authority.workflow_cleanup_generation = target.generation
   AND authority.workflow_id = target.workflow_id
  JOIN jobs_workflow_cleanup_account_bindings binding
    ON binding.account_id = target.account_id
   AND binding.workflow_cleanup_generation = target.generation
   AND binding.target_set_hmac_sha256 = target.target_set_hmac_sha256
  JOIN jobs_workflow_legacy_inventory_generations legacy
    ON legacy.generation = binding.legacy_generation
   AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
 WHERE target.target_state = 'absence_proved'
   AND authority.observation_pass = 2
   AND NOT authority.positive_reset_required
   AND authority.first_absence_observed_at_ms IS NOT NULL
   AND target.absence_proved_at_ms IS NOT NULL
   AND (
     (EXISTS (
        SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
         WHERE run.account_id = authority.account_id
           AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
           AND run.workflow_id = authority.workflow_id
      ) AND target.first_execution_run_id IS NOT NULL
      AND NOT EXISTS (
        SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
         WHERE run.account_id = authority.account_id
           AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
           AND run.workflow_id = authority.workflow_id
           AND NOT EXISTS (
             SELECT 1
               FROM jobs_workflow_cleanup_v2_run_observations first
               JOIN jobs_workflow_cleanup_v2_run_observations second
                 ON second.account_id = first.account_id
                AND second.workflow_cleanup_generation =
                    first.workflow_cleanup_generation
                AND second.workflow_id = first.workflow_id
                AND second.known_run_epoch = first.known_run_epoch
                AND second.observation_pass = 2
                AND second.subject_kind = first.subject_kind
                AND second.run_id_hmac_sha256 = first.run_id_hmac_sha256
                AND second.target_digest_sha256 = first.target_digest_sha256
                AND second.recorded_at_ms >=
                    first.recorded_at_ms + legacy.confirmation_age_ms
              WHERE first.account_id = authority.account_id
                AND first.workflow_cleanup_generation =
                    authority.workflow_cleanup_generation
                AND first.workflow_id = authority.workflow_id
                AND first.known_run_epoch = authority.known_run_epoch
                AND first.observation_pass = 1
                AND first.subject_kind = 'run'
                AND first.run_id_hmac_sha256 = run.run_id_hmac_sha256
                AND first.target_digest_sha256 = authority.target_digest_sha256
                AND target.absence_proved_at_ms >= second.recorded_at_ms
           )
      ))
     OR
     (NOT EXISTS (
        SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
         WHERE run.account_id = authority.account_id
           AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
           AND run.workflow_id = authority.workflow_id
      ) AND target.first_execution_run_id IS NULL
      AND (SELECT COUNT(*)::bigint
             FROM jobs_workflow_cleanup_v2_run_observations observation
            WHERE observation.account_id = authority.account_id
              AND observation.workflow_cleanup_generation =
                  authority.workflow_cleanup_generation
              AND observation.workflow_id = authority.workflow_id
              AND observation.known_run_epoch = authority.known_run_epoch
              AND observation.subject_kind = 'workflow'
              AND observation.run_id_hmac_sha256 = repeat('0', 64)
              AND observation.target_digest_sha256 = authority.target_digest_sha256) = 2
      AND EXISTS (
        SELECT 1
          FROM jobs_workflow_cleanup_v2_run_observations first
          JOIN jobs_workflow_cleanup_v2_run_observations second
            ON second.account_id = first.account_id
           AND second.workflow_cleanup_generation = first.workflow_cleanup_generation
           AND second.workflow_id = first.workflow_id
           AND second.known_run_epoch = first.known_run_epoch
           AND second.observation_pass = 2
           AND second.subject_kind = first.subject_kind
           AND second.run_id_hmac_sha256 = first.run_id_hmac_sha256
           AND second.target_digest_sha256 = first.target_digest_sha256
           AND second.recorded_at_ms >=
               first.recorded_at_ms + legacy.confirmation_age_ms
         WHERE first.account_id = authority.account_id
           AND first.workflow_cleanup_generation = authority.workflow_cleanup_generation
           AND first.workflow_id = authority.workflow_id
           AND first.known_run_epoch = authority.known_run_epoch
           AND first.observation_pass = 1
           AND first.subject_kind = 'workflow'
           AND first.run_id_hmac_sha256 = repeat('0', 64)
           AND first.target_digest_sha256 = authority.target_digest_sha256
           AND target.absence_proved_at_ms >= second.recorded_at_ms
      ))
   );

DROP VIEW IF EXISTS jobs_workflow_cleanup_ready_object_sweeps;
CREATE VIEW jobs_workflow_cleanup_ready_object_sweeps AS
SELECT sweep_auth.account_id, sweep_auth.account_generation,
       sweep_auth.sweep_attempt_id,
       sweep_auth.authorization_digest_sha256,
       sweep_auth.completion_tombstone_id,
       sweep_auth.completion_digest_sha256
  FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
  JOIN jobs_workflow_cleanup_account_bindings binding
    ON binding.account_id = sweep_auth.account_id
   AND binding.account_generation = sweep_auth.account_generation
   AND binding.completion_tombstone_id = sweep_auth.completion_tombstone_id
   AND binding.completion_digest_sha256 = sweep_auth.completion_digest_sha256
  JOIN jobs_runner_purge_requests request
    ON request.request_id = sweep_auth.runner_purge_request_id
   AND request.account_id = sweep_auth.account_id
   AND request.purge_generation = sweep_auth.runner_purge_generation
   AND request.legacy_inventory_generation =
       sweep_auth.runner_legacy_inventory_generation
   AND request.legacy_inventory_reconciliation_id =
       sweep_auth.runner_legacy_reconciliation_id
   AND request.legacy_inventory_authority_id = sweep_auth.runner_legacy_authority_id
   AND request.legacy_inventory_authority_sha256 =
       sweep_auth.runner_legacy_authority_sha256
   AND request.state = 'complete' AND request.legacy_unresolved_count = 0
   AND request.resolved_target_count = request.required_target_count
  JOIN jobs_runner_purge_tombstones runner_tombstone
    ON runner_tombstone.request_id = request.request_id
   AND runner_tombstone.purge_generation = request.purge_generation
   AND runner_tombstone.tombstone_generation =
       sweep_auth.runner_purge_tombstone_generation
   AND runner_tombstone.purge_subject = request.purge_subject
   AND runner_tombstone.target_set_sha256 = request.target_set_sha256
   AND runner_tombstone.required_target_count = request.required_target_count
   AND runner_tombstone.completed_at_ms = request.completed_at_ms
  JOIN jobs_runner_volume_fleet_state fleet
    ON fleet.singleton_id = 1 AND fleet.legacy_inventory_state = 'ready'
   AND fleet.legacy_inventory_generation = sweep_auth.runner_legacy_inventory_generation
   AND fleet.legacy_inventory_reconciliation_id =
       sweep_auth.runner_legacy_reconciliation_id
   AND fleet.legacy_inventory_authority_id = sweep_auth.runner_legacy_authority_id
   AND fleet.legacy_inventory_authority_sha256 =
       sweep_auth.runner_legacy_authority_sha256
 WHERE sweep_auth.sealed AND binding.state = 'complete'
   AND sweep_auth.scope_count = (
     SELECT COUNT(*)::bigint
       FROM jobs_workflow_cleanup_object_sweep_authorization_scopes scope
      WHERE scope.account_id = sweep_auth.account_id
        AND scope.account_generation = sweep_auth.account_generation
        AND scope.sweep_attempt_id = sweep_auth.sweep_attempt_id
        AND scope.prefix_sweep
   )
   AND sweep_auth.known_object_count = (
     SELECT COUNT(*)::bigint
       FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
      WHERE object.account_id = sweep_auth.account_id
        AND object.account_generation = sweep_auth.account_generation
        AND object.sweep_attempt_id = sweep_auth.sweep_attempt_id
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
      WHERE object.account_id = sweep_auth.account_id
        AND object.account_generation = sweep_auth.account_generation
        AND object.sweep_attempt_id = sweep_auth.sweep_attempt_id
        AND NOT EXISTS (
          SELECT 1 FROM jobs_workflow_cleanup_object_sweep_progress progress
           WHERE progress.account_id = object.account_id
             AND progress.account_generation = object.account_generation
             AND progress.scope_id = object.scope_id
             AND progress.object_key_hmac_sha256 = object.object_key_hmac_sha256
        )
   )
   AND sweep_auth.scope_count = (
     SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_object_sweep_scopes result
      WHERE result.account_id = sweep_auth.account_id
        AND result.account_generation = sweep_auth.account_generation
        AND result.sweep_attempt_id = sweep_auth.sweep_attempt_id
        AND result.authorization_digest_sha256 =
            sweep_auth.authorization_digest_sha256
   );

DROP VIEW IF EXISTS jobs_workflow_cleanup_hard_delete_ready;
CREATE VIEW jobs_workflow_cleanup_hard_delete_ready AS
SELECT binding.account_id, binding.account_generation,
       binding.workflow_cleanup_generation, binding.target_set_hmac_sha256,
       binding.hard_delete_sweep_attempt_id
  FROM jobs_workflow_cleanup_account_bindings binding
  JOIN jobs_workflow_cleanup_ready_object_sweeps sweep
    ON sweep.account_id = binding.account_id
   AND sweep.account_generation = binding.account_generation
   AND sweep.sweep_attempt_id = binding.hard_delete_sweep_attempt_id
   AND sweep.completion_tombstone_id = binding.completion_tombstone_id
   AND sweep.completion_digest_sha256 = binding.completion_digest_sha256
  JOIN jobs_workflow_cleanup_completion_tombstones tombstone
    ON tombstone.tombstone_id = binding.completion_tombstone_id
   AND tombstone.completion_digest_sha256 = binding.completion_digest_sha256
  JOIN jobs_workflow_legacy_inventory_head head ON head.singleton_id = 1
  JOIN jobs_workflow_legacy_inventory_generations legacy
    ON legacy.generation = head.generation
   AND legacy.inventory_generation_id = head.inventory_generation_id
   AND legacy.query_digest_sha256 = head.query_digest_sha256
   AND legacy.generation = binding.legacy_generation
   AND legacy.inventory_generation_id = binding.legacy_inventory_generation_id
   AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
   AND legacy.completion_epoch = tombstone.legacy_completion_epoch
   AND legacy.completion_digest_sha256 = tombstone.legacy_completion_digest_sha256
   AND legacy.revalidate_after_ms = tombstone.legacy_revalidate_after_ms
  JOIN jobs_workflow_cleanup_generations generation
    ON generation.account_id = binding.account_id
   AND generation.generation = binding.workflow_cleanup_generation
   AND generation.target_set_hmac_sha256 = binding.target_set_hmac_sha256
 WHERE binding.state = 'complete'
   AND binding.hard_delete_authorized_at_ms IS NOT NULL
   AND binding.hard_delete_authorization_digest_sha256 IS NOT NULL
   AND legacy.state = 'complete'
   AND legacy.page_token_ciphertext IS NULL
   AND legacy.page_token_hmac_sha256 IS NULL
   AND legacy.revalidate_after_ms >
       FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint
   AND generation.target_count = (
     SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets target
      WHERE target.account_id = generation.account_id
        AND target.generation = generation.generation
        AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256
   )
   AND generation.target_count = (
     SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_v2_target_authorities authority
      WHERE authority.account_id = generation.account_id
        AND authority.workflow_cleanup_generation = generation.generation
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
      WHERE authority.account_id = generation.account_id
        AND authority.workflow_cleanup_generation = generation.generation
        AND authority.positive_reset_required
   )
   AND generation.target_count = (
     SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_v2_proved_targets proved
      WHERE proved.account_id = generation.account_id
        AND proved.generation = generation.generation
        AND proved.target_set_hmac_sha256 = generation.target_set_hmac_sha256
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_targets target
      WHERE target.account_id = generation.account_id
        AND target.generation = generation.generation
        AND target.target_state <> 'absence_proved'
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_targets target
      WHERE target.generation = legacy.generation
        AND (target.target_state <> 'absence_proved'
          OR target.positive_reset_required)
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
      WHERE page.generation = legacy.generation
        AND NOT page.raw_ciphertexts_scrubbed
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_targets target
      WHERE target.generation = legacy.generation
        AND NOT target.raw_ids_scrubbed
   );

CREATE OR REPLACE FUNCTION reject_jobs_workflow_legacy_generation_identity_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow legacy inventory identity is immutable';
END;
$$;

CREATE OR REPLACE FUNCTION reject_jobs_workflow_cleanup_observation_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow cleanup observation is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_generation_identity_immutable
  ON jobs_workflow_legacy_inventory_generations;
CREATE TRIGGER trg_jobs_workflow_legacy_generation_identity_immutable
BEFORE UPDATE OF generation, inventory_generation_id, namespace_ciphertext,
  namespace_hmac_sha256, workflow_type, visibility_cutoff_ms, confirmation_age_ms,
  visibility_query_ciphertext, query_digest_sha256,
  query_hmac_sha256, created_at_ms
ON jobs_workflow_legacy_inventory_generations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_legacy_generation_identity_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_generation_epoch_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (NEW.completion_epoch = OLD.completion_epoch
      AND OLD.state = 'complete' AND NEW.state <> 'complete')
    OR (OLD.state = 'identity_conflict' AND NEW.state <> 'identity_conflict')
    OR (NEW.completion_epoch <> OLD.completion_epoch AND (
      OLD.completion_epoch >= 9007199254740991
      OR NEW.completion_epoch <> OLD.completion_epoch + 1
      OR NEW.state NOT IN ('scanning', 'draining')
      OR NEW.scan_pass <> 1 OR NEW.page_index <> 0
      OR NEW.predecessor_page_digest_sha256 = OLD.predecessor_page_digest_sha256
      OR NEW.page_token_ciphertext IS NOT NULL
      OR NEW.page_token_hmac_sha256 IS NOT NULL
      OR NEW.request_epoch <> 0 OR NEW.fence <> 0 OR NEW.request_id IS NOT NULL
      OR NEW.first_request_started_at_ms IS NOT NULL
      OR NEW.last_outcome_code IS NOT NULL
      OR NEW.lease_owner IS NOT NULL OR NEW.lease_token_sha256 IS NOT NULL
      OR NEW.lease_expires_at_ms IS NOT NULL
      OR NEW.first_zero_observed_at_ms IS NOT NULL
      OR NEW.completed_at_ms IS NOT NULL
      OR NEW.completion_digest_sha256 IS NOT NULL
      OR NEW.revalidate_after_ms IS NOT NULL
      OR NEW.updated_at_ms <= OLD.updated_at_ms
    )) THEN
    RAISE EXCEPTION
      'workflow legacy completion epoch must advance and reopen cleanly';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_generation_epoch_guard
  ON jobs_workflow_legacy_inventory_generations;
CREATE TRIGGER trg_jobs_workflow_legacy_generation_epoch_guard
BEFORE UPDATE ON jobs_workflow_legacy_inventory_generations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_generation_epoch_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_head_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.generation < OLD.generation OR (
    NEW.generation = OLD.generation
      AND (NEW.inventory_generation_id <> OLD.inventory_generation_id
        OR NEW.query_digest_sha256 <> OLD.query_digest_sha256)
  ) THEN
    RAISE EXCEPTION 'workflow legacy inventory head is not monotonic';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_head_monotonic
  ON jobs_workflow_legacy_inventory_head;
CREATE TRIGGER trg_jobs_workflow_legacy_head_monotonic
BEFORE UPDATE ON jobs_workflow_legacy_inventory_head
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_head_update();

CREATE OR REPLACE FUNCTION reject_jobs_workflow_legacy_authority_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow legacy inventory authority is permanent';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_head_delete_guard
  ON jobs_workflow_legacy_inventory_head;
CREATE TRIGGER trg_jobs_workflow_legacy_head_delete_guard
BEFORE DELETE ON jobs_workflow_legacy_inventory_head
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_legacy_authority_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_generation_delete_guard
  ON jobs_workflow_legacy_inventory_generations;
CREATE TRIGGER trg_jobs_workflow_legacy_generation_delete_guard
BEFORE DELETE ON jobs_workflow_legacy_inventory_generations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_legacy_authority_delete();

DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_page_immutable
  ON jobs_workflow_legacy_inventory_pages;
CREATE TRIGGER trg_jobs_workflow_legacy_page_immutable
BEFORE UPDATE OF generation, completion_epoch, scan_pass, page_index,
  request_epoch, fence, request_id, predecessor_page_digest_sha256,
  input_page_token_hmac_sha256, next_page_token_hmac_sha256,
  page_target_count, page_targets_digest_sha256, page_digest_sha256,
  evidence_digest_sha256, recorded_at_ms
ON jobs_workflow_legacy_inventory_pages
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_page_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_inventory_head head
    JOIN jobs_workflow_legacy_inventory_generations generation
      ON generation.generation = head.generation
     AND generation.inventory_generation_id = head.inventory_generation_id
     AND generation.query_digest_sha256 = head.query_digest_sha256
     WHERE head.singleton_id = 1 AND generation.generation = NEW.generation
       AND generation.state = 'scanning'
       AND generation.completion_epoch = NEW.completion_epoch
       AND generation.scan_pass = NEW.scan_pass
       AND generation.page_index = NEW.page_index
       AND generation.request_epoch = NEW.request_epoch
       AND generation.fence = NEW.fence
       AND generation.request_id = NEW.request_id
       AND ((NEW.page_index = 0 AND NEW.predecessor_page_digest_sha256 IS NULL)
         OR (NEW.page_index > 0
           AND generation.predecessor_page_digest_sha256 =
               NEW.predecessor_page_digest_sha256))
       AND generation.page_token_hmac_sha256 IS NOT DISTINCT FROM
           NEW.input_page_token_hmac_sha256
       AND generation.first_request_started_at_ms IS NOT NULL
       AND generation.lease_owner IS NOT NULL
       AND generation.lease_token_sha256 IS NOT NULL
       AND generation.lease_expires_at_ms >= NEW.recorded_at_ms
  ) THEN
    RAISE EXCEPTION 'workflow legacy page is not current request authority';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_page_insert_guard
  ON jobs_workflow_legacy_inventory_pages;
CREATE TRIGGER trg_jobs_workflow_legacy_page_insert_guard
BEFORE INSERT ON jobs_workflow_legacy_inventory_pages
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_page_insert();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_page_raw_scrub()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT (
    NOT OLD.raw_ciphertexts_scrubbed AND NEW.raw_ciphertexts_scrubbed
    AND NEW.input_page_token_ciphertext IS NULL
    AND NEW.next_page_token_ciphertext IS NULL
  ) THEN
    RAISE EXCEPTION 'workflow legacy page raw ciphertext can only be scrubbed';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_page_raw_scrub_guard
  ON jobs_workflow_legacy_inventory_pages;
CREATE TRIGGER trg_jobs_workflow_legacy_page_raw_scrub_guard
BEFORE UPDATE OF input_page_token_ciphertext, next_page_token_ciphertext,
  raw_ciphertexts_scrubbed ON jobs_workflow_legacy_inventory_pages
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_page_raw_scrub();

CREATE OR REPLACE FUNCTION reject_jobs_workflow_legacy_target_identity_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow legacy target identity is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_identity_immutable
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_identity_immutable
BEFORE UPDATE OF generation, target_identity_hmac_sha256,
  workflow_id_hmac_sha256, run_id_hmac_sha256,
  first_execution_run_id_hmac_sha256,
  target_digest_sha256, discovered_completion_epoch, discovered_scan_pass,
  discovered_page_index, discovered_page_digest_sha256, created_at_ms
ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_legacy_target_identity_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_target_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.raw_ids_scrubbed OR NOT EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_inventory_head head
    JOIN jobs_workflow_legacy_inventory_generations generation
      ON generation.generation = head.generation
     AND generation.inventory_generation_id = head.inventory_generation_id
     AND generation.query_digest_sha256 = head.query_digest_sha256
    JOIN jobs_workflow_legacy_inventory_pages page
      ON page.generation = generation.generation
     AND page.completion_epoch = NEW.discovered_completion_epoch
     AND page.scan_pass = NEW.discovered_scan_pass
     AND page.page_index = NEW.discovered_page_index
     AND page.page_digest_sha256 = NEW.discovered_page_digest_sha256
     WHERE head.singleton_id = 1 AND generation.generation = NEW.generation
       AND generation.state = 'scanning'
  ) THEN
    RAISE EXCEPTION 'workflow legacy target is not current page authority';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_insert_guard
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_insert_guard
BEFORE INSERT ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_target_insert();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_target_raw_identity_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT (
    (NOT OLD.raw_ids_scrubbed AND NEW.raw_ids_scrubbed
      AND OLD.target_state = 'absence_proved'
      AND NEW.workflow_id_ciphertext IS NULL AND NEW.run_id_ciphertext IS NULL
      AND NEW.first_execution_run_id_ciphertext IS NULL)
    OR (OLD.raw_ids_scrubbed AND NOT NEW.raw_ids_scrubbed
      AND NEW.workflow_id_ciphertext IS NOT NULL AND NEW.run_id_ciphertext IS NOT NULL
      AND NEW.first_execution_run_id_ciphertext IS NOT NULL
      AND NEW.proof_epoch = OLD.proof_epoch + 1
      AND NEW.observation_pass = 1
      AND NEW.first_absence_observed_at_ms IS NULL
      AND NEW.absence_proved_at_ms IS NULL
      AND NEW.request_id IS NULL AND NEW.first_request_started_at_ms IS NULL
      AND NEW.lease_owner IS NULL AND NEW.lease_token_sha256 IS NULL
      AND NEW.lease_expires_at_ms IS NULL
      AND EXISTS (
        SELECT 1 FROM jobs_workflow_legacy_inventory_head head
        JOIN jobs_workflow_legacy_inventory_generations generation
          ON generation.generation = head.generation
         AND generation.inventory_generation_id = head.inventory_generation_id
         AND generation.query_digest_sha256 = head.query_digest_sha256
         WHERE head.singleton_id = 1 AND generation.generation = NEW.generation
           AND generation.state <> 'complete'
      ))
  ) THEN
    RAISE EXCEPTION 'workflow legacy target raw identity transition is invalid';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_raw_identity_guard
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_raw_identity_guard
BEFORE UPDATE OF workflow_id_ciphertext, run_id_ciphertext,
  first_execution_run_id_ciphertext, raw_ids_scrubbed
ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_target_raw_identity_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_target_proof_epoch_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.proof_epoch <> OLD.proof_epoch + 1
     OR NEW.positive_reset_required
     OR NEW.raw_ids_scrubbed
     OR NEW.target_state IN ('absence_pending', 'absence_proved')
     OR NEW.observation_pass <> 1
     OR NEW.first_absence_observed_at_ms IS NOT NULL
     OR NEW.absence_proved_at_ms IS NOT NULL
     OR NEW.request_id IS NOT NULL OR NEW.first_request_started_at_ms IS NOT NULL
     OR NEW.lease_owner IS NOT NULL OR NEW.lease_token_sha256 IS NOT NULL
     OR NEW.lease_expires_at_ms IS NOT NULL
     OR NOT EXISTS (
       SELECT 1 FROM jobs_workflow_legacy_inventory_head head
       JOIN jobs_workflow_legacy_inventory_generations generation
         ON generation.generation = head.generation
        AND generation.inventory_generation_id = head.inventory_generation_id
        AND generation.query_digest_sha256 = head.query_digest_sha256
      WHERE head.singleton_id = 1
        AND generation.generation = NEW.generation
        AND generation.state IN ('scanning', 'draining')
     ) THEN
    RAISE EXCEPTION 'workflow legacy proof epoch must reopen cleanly';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_proof_epoch_guard
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_proof_epoch_guard
BEFORE UPDATE OF proof_epoch ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_target_proof_epoch_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_target_terminal_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (OLD.target_state = 'absence_proved'
      AND NEW.proof_epoch = OLD.proof_epoch
      AND (NEW.target_state <> OLD.target_state
        OR NEW.observation_pass <> OLD.observation_pass
        OR NEW.first_absence_observed_at_ms IS DISTINCT FROM
           OLD.first_absence_observed_at_ms
        OR NEW.absence_proved_at_ms IS DISTINCT FROM OLD.absence_proved_at_ms
        OR NEW.positive_reset_required IS DISTINCT FROM
           OLD.positive_reset_required))
    OR (OLD.target_state = 'identity_conflict'
      AND (NEW.target_state <> 'identity_conflict'
        OR NEW.proof_epoch <> OLD.proof_epoch
        OR NEW.observation_pass <> OLD.observation_pass
        OR NEW.first_absence_observed_at_ms IS DISTINCT FROM
           OLD.first_absence_observed_at_ms
        OR NEW.absence_proved_at_ms IS DISTINCT FROM OLD.absence_proved_at_ms
        OR NEW.positive_reset_required IS DISTINCT FROM
           OLD.positive_reset_required)) THEN
    RAISE EXCEPTION
      'workflow legacy target terminal state requires a new proof epoch';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_terminal_guard
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_terminal_guard
BEFORE UPDATE OF target_state, proof_epoch, observation_pass,
  first_absence_observed_at_ms, absence_proved_at_ms, positive_reset_required
ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_target_terminal_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_positive_reset_clear()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.positive_reset_required AND NOT NEW.positive_reset_required
     AND NEW.proof_epoch <> OLD.proof_epoch + 1 THEN
    RAISE EXCEPTION 'workflow legacy positive evidence requires a new proof epoch';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_positive_reset_clear_guard
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_positive_reset_clear_guard
BEFORE UPDATE OF positive_reset_required ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_positive_reset_clear();

DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_observation_immutable
  ON jobs_workflow_legacy_target_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_observation_immutable
BEFORE UPDATE ON jobs_workflow_legacy_target_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_observation_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_targets target
     WHERE target.generation = NEW.generation
       AND target.target_identity_hmac_sha256 = NEW.target_identity_hmac_sha256
       AND target.target_digest_sha256 = NEW.target_digest_sha256
       AND target.proof_epoch = NEW.proof_epoch
       AND target.observation_pass = NEW.observation_pass
       AND target.request_epoch = NEW.request_epoch
       AND target.fence = NEW.cleanup_fence
       AND target.request_id = NEW.cleanup_request_id
       AND target.first_request_started_at_ms IS NOT NULL
       AND target.lease_owner IS NOT NULL
       AND target.lease_expires_at_ms >= NEW.recorded_at_ms
  ) THEN
    RAISE EXCEPTION 'workflow legacy observation lease is not current';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_observation_current_lease
  ON jobs_workflow_legacy_target_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_observation_current_lease
BEFORE INSERT ON jobs_workflow_legacy_target_observations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_observation_insert();

CREATE OR REPLACE FUNCTION require_jobs_workflow_legacy_positive_reset()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.describe_state = 'found' OR NEW.history_state = 'found'
     OR NEW.visibility_state = 'found' THEN
    UPDATE jobs_workflow_legacy_targets
       SET positive_reset_required = TRUE,
           updated_at_ms = GREATEST(NEW.recorded_at_ms, updated_at_ms + 1)
     WHERE generation = NEW.generation
       AND target_identity_hmac_sha256 = NEW.target_identity_hmac_sha256
       AND proof_epoch = NEW.proof_epoch;
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_observation_positive_reset
  ON jobs_workflow_legacy_target_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_observation_positive_reset
AFTER INSERT ON jobs_workflow_legacy_target_observations
FOR EACH ROW EXECUTE FUNCTION require_jobs_workflow_legacy_positive_reset();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_target_absence()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.target_state = 'absence_proved' AND OLD.target_state <> 'absence_proved' AND (
    NEW.positive_reset_required
    OR NEW.observation_pass <> 2 OR NEW.first_absence_observed_at_ms IS NULL
    OR NEW.absence_proved_at_ms IS NULL
    OR (SELECT COUNT(*)::bigint FROM jobs_workflow_legacy_target_observations observation
         WHERE observation.generation = NEW.generation
           AND observation.target_identity_hmac_sha256 = NEW.target_identity_hmac_sha256
           AND observation.target_digest_sha256 = NEW.target_digest_sha256
           AND observation.proof_epoch = NEW.proof_epoch
           AND observation.outcome = 'absence_proved') <> 2
    OR NOT EXISTS (
      SELECT 1 FROM jobs_workflow_legacy_target_observations first
      JOIN jobs_workflow_legacy_target_observations second
        ON second.generation = first.generation
       AND second.target_identity_hmac_sha256 = first.target_identity_hmac_sha256
       AND second.target_digest_sha256 = first.target_digest_sha256
       AND second.proof_epoch = first.proof_epoch
       AND second.observation_pass = 2
      JOIN jobs_workflow_legacy_inventory_generations generation
        ON generation.generation = first.generation
       AND second.recorded_at_ms >= first.recorded_at_ms + generation.confirmation_age_ms
     WHERE first.generation = NEW.generation
       AND first.target_identity_hmac_sha256 = NEW.target_identity_hmac_sha256
       AND first.target_digest_sha256 = NEW.target_digest_sha256
       AND first.proof_epoch = NEW.proof_epoch
       AND first.observation_pass = 1
       AND first.recorded_at_ms = NEW.first_absence_observed_at_ms
       AND NEW.absence_proved_at_ms >= second.recorded_at_ms
       AND first.outcome = 'absence_proved' AND second.outcome = 'absence_proved'
    )
  ) THEN
    RAISE EXCEPTION 'workflow legacy target lacks two exact absence passes';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_absence_guard
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_absence_guard
BEFORE UPDATE OF target_state, observation_pass, first_absence_observed_at_ms,
  absence_proved_at_ms ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_target_absence();

DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_zero_immutable
  ON jobs_workflow_legacy_zero_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_zero_immutable
BEFORE UPDATE ON jobs_workflow_legacy_zero_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_zero_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
     WHERE page.generation = NEW.generation
       AND page.completion_epoch = NEW.completion_epoch
       AND page.scan_pass = NEW.scan_pass
       AND page.page_index = NEW.final_page_index
       AND page.page_digest_sha256 = NEW.final_page_digest_sha256
       AND page.page_index = 0
       AND page.input_page_token_ciphertext IS NULL
       AND page.input_page_token_hmac_sha256 IS NULL
       AND page.page_target_count = 0
       AND page.next_page_token_ciphertext IS NULL
       AND page.next_page_token_hmac_sha256 IS NULL
  ) THEN
    RAISE EXCEPTION 'workflow legacy zero is not bound to an exhausted empty scan';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_zero_exact_page
  ON jobs_workflow_legacy_zero_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_zero_exact_page
BEFORE INSERT ON jobs_workflow_legacy_zero_observations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_zero_insert();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_authority_immutable_identity
  ON jobs_workflow_cleanup_v2_target_authorities;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_authority_immutable_identity
BEFORE UPDATE OF account_id, workflow_cleanup_generation, workflow_id, created_at_ms
ON jobs_workflow_cleanup_v2_target_authorities
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_proof_epoch()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.known_run_epoch <> OLD.known_run_epoch + 1
    OR NEW.positive_reset_required
    OR NEW.observation_pass <> 1
    OR NEW.first_absence_observed_at_ms IS NOT NULL
    OR NEW.cleanup_request_id IS NOT NULL
    OR NEW.first_request_started_at_ms IS NOT NULL
    OR NEW.lease_owner IS NOT NULL OR NEW.lease_token_sha256 IS NOT NULL
    OR NEW.lease_expires_at_ms IS NOT NULL
    OR NOT EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_targets target
       WHERE target.account_id = NEW.account_id
         AND target.generation = NEW.workflow_cleanup_generation
         AND target.workflow_id = NEW.workflow_id
         AND target.target_state NOT IN ('absence_proved', 'identity_conflict')
    )
  THEN
    RAISE EXCEPTION 'workflow v2 proof epoch must reopen cleanly';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_proof_epoch_guard
  ON jobs_workflow_cleanup_v2_target_authorities;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_proof_epoch_guard
BEFORE UPDATE OF known_run_epoch ON jobs_workflow_cleanup_v2_target_authorities
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_proof_epoch();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_pass_transition()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.known_run_epoch = OLD.known_run_epoch
    AND (NEW.known_run_set_digest_sha256 <> OLD.known_run_set_digest_sha256
      OR NEW.target_digest_sha256 <> OLD.target_digest_sha256
      OR NEW.observation_pass <> OLD.observation_pass
      OR NEW.first_absence_observed_at_ms IS DISTINCT FROM
         OLD.first_absence_observed_at_ms)
    AND NOT (
      NEW.known_run_set_digest_sha256 = OLD.known_run_set_digest_sha256
      AND NEW.target_digest_sha256 = OLD.target_digest_sha256
      AND OLD.observation_pass = 1 AND OLD.first_absence_observed_at_ms IS NULL
      AND NEW.observation_pass = 2 AND NEW.first_absence_observed_at_ms IS NOT NULL
      AND EXISTS (
        SELECT 1 FROM jobs_workflow_cleanup_targets target
         WHERE target.account_id = NEW.account_id
           AND target.generation = NEW.workflow_cleanup_generation
           AND target.workflow_id = NEW.workflow_id
           AND target.target_state NOT IN ('absence_proved', 'identity_conflict')
      )
      AND (
        (EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
            WHERE run.account_id = NEW.account_id
              AND run.workflow_cleanup_generation = NEW.workflow_cleanup_generation
              AND run.workflow_id = NEW.workflow_id
         ) AND NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
            WHERE run.account_id = NEW.account_id
              AND run.workflow_cleanup_generation = NEW.workflow_cleanup_generation
              AND run.workflow_id = NEW.workflow_id
              AND NOT EXISTS (
                SELECT 1 FROM jobs_workflow_cleanup_v2_run_observations observation
                 WHERE observation.account_id = NEW.account_id
                   AND observation.workflow_cleanup_generation =
                       NEW.workflow_cleanup_generation
                   AND observation.workflow_id = NEW.workflow_id
                   AND observation.known_run_epoch = NEW.known_run_epoch
                   AND observation.observation_pass = 1
                   AND observation.subject_kind = 'run'
                   AND observation.run_id_hmac_sha256 = run.run_id_hmac_sha256
                   AND observation.target_digest_sha256 = NEW.target_digest_sha256
                   AND observation.recorded_at_ms = NEW.first_absence_observed_at_ms
              )
         ))
        OR (NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
            WHERE run.account_id = NEW.account_id
              AND run.workflow_cleanup_generation = NEW.workflow_cleanup_generation
              AND run.workflow_id = NEW.workflow_id
         ) AND EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_run_observations observation
            WHERE observation.account_id = NEW.account_id
              AND observation.workflow_cleanup_generation = NEW.workflow_cleanup_generation
              AND observation.workflow_id = NEW.workflow_id
              AND observation.known_run_epoch = NEW.known_run_epoch
              AND observation.observation_pass = 1
              AND observation.subject_kind = 'workflow'
              AND observation.run_id_hmac_sha256 =
                  '0000000000000000000000000000000000000000000000000000000000000000'
              AND observation.target_digest_sha256 = NEW.target_digest_sha256
              AND observation.recorded_at_ms = NEW.first_absence_observed_at_ms
         ))
      )
    ) THEN
    RAISE EXCEPTION
      'workflow v2 proof tuple must advance from immutable evidence';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_pass_transition_guard
  ON jobs_workflow_cleanup_v2_target_authorities;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_pass_transition_guard
BEFORE UPDATE OF known_run_set_digest_sha256, target_digest_sha256,
  observation_pass, first_absence_observed_at_ms
ON jobs_workflow_cleanup_v2_target_authorities
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_pass_transition();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_positive_reset_clear()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.positive_reset_required AND NOT NEW.positive_reset_required
     AND NEW.known_run_epoch <> OLD.known_run_epoch + 1 THEN
    RAISE EXCEPTION 'workflow v2 positive evidence requires a new proof epoch';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_positive_reset_clear_guard
  ON jobs_workflow_cleanup_v2_target_authorities;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_positive_reset_clear_guard
BEFORE UPDATE OF positive_reset_required ON jobs_workflow_cleanup_v2_target_authorities
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_positive_reset_clear();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_terminal_positive_reset()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.positive_reset_required IS DISTINCT FROM OLD.positive_reset_required
    AND EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_targets target
       WHERE target.account_id = NEW.account_id
         AND target.generation = NEW.workflow_cleanup_generation
         AND target.workflow_id = NEW.workflow_id
         AND target.target_state IN ('absence_proved', 'identity_conflict')
    ) THEN
    RAISE EXCEPTION 'workflow v2 terminal positive-reset authority is immutable';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_terminal_positive_reset_guard
  ON jobs_workflow_cleanup_v2_target_authorities;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_terminal_positive_reset_guard
BEFORE UPDATE OF positive_reset_required ON jobs_workflow_cleanup_v2_target_authorities
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_terminal_positive_reset();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_known_run_immutable
  ON jobs_workflow_cleanup_v2_known_runs;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_known_run_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_v2_known_runs
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_known_run_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_targets target
    JOIN jobs_workflow_cleanup_v2_target_authorities authority
      ON authority.account_id = target.account_id
     AND authority.workflow_cleanup_generation = target.generation
     AND authority.workflow_id = target.workflow_id
   WHERE target.account_id = NEW.account_id
     AND target.generation = NEW.workflow_cleanup_generation
     AND target.workflow_id = NEW.workflow_id
     AND target.target_state NOT IN ('absence_proved', 'identity_conflict')
  ) OR (SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_v2_known_runs run
       WHERE run.account_id = NEW.account_id
         AND run.workflow_cleanup_generation = NEW.workflow_cleanup_generation
         AND run.workflow_id = NEW.workflow_id) >= 32 THEN
    RAISE EXCEPTION 'workflow v2 known run set exceeds 32';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_known_run_bound
  ON jobs_workflow_cleanup_v2_known_runs;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_known_run_bound
BEFORE INSERT ON jobs_workflow_cleanup_v2_known_runs
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_known_run_insert();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_observation_immutable
  ON jobs_workflow_cleanup_v2_run_observations;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_observation_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_v2_run_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_receipt_immutable
  ON jobs_workflow_cleanup_v2_receipts;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_receipt_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_v2_receipts
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_receipt_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
     WHERE authority.account_id = NEW.account_id
       AND authority.workflow_cleanup_generation = NEW.workflow_cleanup_generation
       AND authority.workflow_id = NEW.workflow_id
       AND authority.known_run_epoch = NEW.known_run_epoch
       AND authority.target_digest_sha256 = NEW.target_digest_sha256
       AND authority.request_epoch = NEW.request_epoch
       AND authority.cleanup_fence = NEW.cleanup_fence
       AND authority.cleanup_request_id = NEW.cleanup_request_id
       AND authority.first_request_started_at_ms IS NOT NULL
       AND authority.lease_owner IS NOT NULL
       AND authority.lease_expires_at_ms >= NEW.recorded_at_ms
  ) THEN
    RAISE EXCEPTION 'workflow v2 receipt lease is not current';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_receipt_current_lease
  ON jobs_workflow_cleanup_v2_receipts;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_receipt_current_lease
BEFORE INSERT ON jobs_workflow_cleanup_v2_receipts
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_receipt_insert();

CREATE OR REPLACE FUNCTION require_jobs_workflow_cleanup_v2_positive_reset()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.outcome = 'pending' AND NEW.reason <> 'temporal_unavailable' THEN
    UPDATE jobs_workflow_cleanup_v2_target_authorities
       SET positive_reset_required = TRUE,
           updated_at_ms = GREATEST(NEW.recorded_at_ms, updated_at_ms + 1)
     WHERE account_id = NEW.account_id
       AND workflow_cleanup_generation = NEW.workflow_cleanup_generation
       AND workflow_id = NEW.workflow_id
       AND known_run_epoch = NEW.known_run_epoch;
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_receipt_positive_reset
  ON jobs_workflow_cleanup_v2_receipts;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_receipt_positive_reset
AFTER INSERT ON jobs_workflow_cleanup_v2_receipts
FOR EACH ROW EXECUTE FUNCTION require_jobs_workflow_cleanup_v2_positive_reset();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_observation_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
    JOIN jobs_workflow_cleanup_account_bindings binding
      ON binding.account_id = authority.account_id
     AND binding.workflow_cleanup_generation = authority.workflow_cleanup_generation
    JOIN jobs_workflow_legacy_inventory_generations legacy
      ON legacy.generation = binding.legacy_generation
     WHERE authority.account_id = NEW.account_id
       AND authority.workflow_cleanup_generation = NEW.workflow_cleanup_generation
       AND authority.workflow_id = NEW.workflow_id
       AND authority.known_run_epoch = NEW.known_run_epoch
       AND authority.observation_pass = NEW.observation_pass
       AND authority.target_digest_sha256 = NEW.target_digest_sha256
       AND authority.request_epoch = NEW.request_epoch
       AND authority.cleanup_fence = NEW.cleanup_fence
       AND authority.cleanup_request_id = NEW.cleanup_request_id
       AND authority.first_request_started_at_ms IS NOT NULL
       AND authority.lease_owner IS NOT NULL
       AND authority.lease_expires_at_ms >= NEW.recorded_at_ms
       AND (
         (NEW.subject_kind = 'run' AND EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
            WHERE run.account_id = authority.account_id
              AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
              AND run.workflow_id = authority.workflow_id
              AND run.run_id_hmac_sha256 = NEW.run_id_hmac_sha256
         ))
         OR (NEW.subject_kind = 'workflow' AND NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
            WHERE run.account_id = authority.account_id
              AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
              AND run.workflow_id = authority.workflow_id
         ))
       )
  ) THEN
    RAISE EXCEPTION 'workflow v2 observation lease or known-run epoch is not current';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_observation_current_lease
  ON jobs_workflow_cleanup_v2_run_observations;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_observation_current_lease
BEFORE INSERT ON jobs_workflow_cleanup_v2_run_observations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_observation_insert();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_proved_target_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.target_state = 'absence_proved' AND (
    NEW.target_state <> OLD.target_state
    OR NEW.absence_proved_at_ms IS DISTINCT FROM OLD.absence_proved_at_ms
    OR NEW.first_execution_run_id IS DISTINCT FROM OLD.first_execution_run_id
  ) THEN
    RAISE EXCEPTION 'workflow v2 proved target is immutable';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_target_proved_immutable
  ON jobs_workflow_cleanup_targets;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_target_proved_immutable
BEFORE UPDATE OF target_state, absence_proved_at_ms, first_execution_run_id
ON jobs_workflow_cleanup_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_proved_target_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_v2_target_absence()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.target_state = 'absence_proved' AND OLD.target_state <> 'absence_proved'
     AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
    JOIN jobs_workflow_cleanup_account_bindings binding
      ON binding.account_id = authority.account_id
     AND binding.workflow_cleanup_generation = authority.workflow_cleanup_generation
    JOIN jobs_workflow_legacy_inventory_generations legacy
      ON legacy.generation = binding.legacy_generation
     AND legacy.inventory_generation_id = binding.legacy_inventory_generation_id
     AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
    JOIN jobs_workflow_legacy_inventory_head head
      ON head.singleton_id = 1
     AND head.generation = legacy.generation
     AND head.inventory_generation_id = legacy.inventory_generation_id
     AND head.query_digest_sha256 = legacy.query_digest_sha256
     WHERE authority.account_id = NEW.account_id
       AND authority.workflow_cleanup_generation = NEW.generation
       AND authority.workflow_id = NEW.workflow_id
       AND authority.observation_pass = 2
       AND NOT authority.positive_reset_required
       AND authority.first_absence_observed_at_ms IS NOT NULL
       AND NEW.absence_proved_at_ms IS NOT NULL
       AND (
         (EXISTS (
            SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
             WHERE run.account_id = authority.account_id
               AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
               AND run.workflow_id = authority.workflow_id
          ) AND NEW.first_execution_run_id IS NOT NULL
          AND NOT EXISTS (
            SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
             WHERE run.account_id = authority.account_id
               AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
               AND run.workflow_id = authority.workflow_id
               AND NOT EXISTS (
                 SELECT 1
                   FROM jobs_workflow_cleanup_v2_run_observations first
                   JOIN jobs_workflow_cleanup_v2_run_observations second
                     ON second.account_id = first.account_id
                    AND second.workflow_cleanup_generation =
                        first.workflow_cleanup_generation
                    AND second.workflow_id = first.workflow_id
                    AND second.known_run_epoch = first.known_run_epoch
                    AND second.observation_pass = 2
                    AND second.subject_kind = first.subject_kind
                    AND second.run_id_hmac_sha256 = first.run_id_hmac_sha256
                    AND second.target_digest_sha256 = first.target_digest_sha256
                    AND second.recorded_at_ms >=
                        first.recorded_at_ms + legacy.confirmation_age_ms
                  WHERE first.account_id = authority.account_id
                    AND first.workflow_cleanup_generation =
                        authority.workflow_cleanup_generation
                    AND first.workflow_id = authority.workflow_id
                    AND first.known_run_epoch = authority.known_run_epoch
                    AND first.observation_pass = 1
                    AND first.subject_kind = 'run'
                    AND first.run_id_hmac_sha256 = run.run_id_hmac_sha256
                    AND first.target_digest_sha256 = authority.target_digest_sha256
                    AND NEW.absence_proved_at_ms >= second.recorded_at_ms
               )
          ))
         OR (NOT EXISTS (
            SELECT 1 FROM jobs_workflow_cleanup_v2_known_runs run
             WHERE run.account_id = authority.account_id
               AND run.workflow_cleanup_generation = authority.workflow_cleanup_generation
               AND run.workflow_id = authority.workflow_id
          ) AND NEW.first_execution_run_id IS NULL
          AND EXISTS (
            SELECT 1
              FROM jobs_workflow_cleanup_v2_run_observations first
              JOIN jobs_workflow_cleanup_v2_run_observations second
                ON second.account_id = first.account_id
               AND second.workflow_cleanup_generation = first.workflow_cleanup_generation
               AND second.workflow_id = first.workflow_id
               AND second.known_run_epoch = first.known_run_epoch
               AND second.observation_pass = 2
               AND second.subject_kind = first.subject_kind
               AND second.run_id_hmac_sha256 = first.run_id_hmac_sha256
               AND second.target_digest_sha256 = first.target_digest_sha256
               AND second.recorded_at_ms >=
                   first.recorded_at_ms + legacy.confirmation_age_ms
             WHERE first.account_id = authority.account_id
               AND first.workflow_cleanup_generation = authority.workflow_cleanup_generation
               AND first.workflow_id = authority.workflow_id
               AND first.known_run_epoch = authority.known_run_epoch
               AND first.observation_pass = 1
               AND first.subject_kind = 'workflow'
               AND first.run_id_hmac_sha256 = repeat('0', 64)
               AND first.target_digest_sha256 = authority.target_digest_sha256
               AND NEW.absence_proved_at_ms >= second.recorded_at_ms
          ))
       )
  ) THEN
    RAISE EXCEPTION 'workflow v2 target lacks per-run two-pass absence proof';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_target_absence_guard
  ON jobs_workflow_cleanup_targets;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_target_absence_guard
BEFORE UPDATE OF target_state, absence_proved_at_ms ON jobs_workflow_cleanup_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_v2_target_absence();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_sweep_authorization_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.sealed THEN
    RAISE EXCEPTION 'workflow cleanup sweep authorization must start unsealed';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_authorization_insert_guard
  ON jobs_workflow_cleanup_object_sweep_authorizations;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_authorization_insert_guard
BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_authorizations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_sweep_authorization_insert();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_sweep_authorization_seal()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.sealed OR NOT NEW.sealed
    OR (to_jsonb(NEW) - 'sealed') <> (to_jsonb(OLD) - 'sealed')
    OR NEW.scope_count <> (
      SELECT COUNT(*)::bigint
        FROM jobs_workflow_cleanup_object_sweep_authorization_scopes scope
       WHERE scope.account_id = NEW.account_id
         AND scope.account_generation = NEW.account_generation
         AND scope.sweep_attempt_id = NEW.sweep_attempt_id
    )
    OR NEW.known_object_count <> (
      SELECT COUNT(*)::bigint
        FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
       WHERE object.account_id = NEW.account_id
         AND object.account_generation = NEW.account_generation
         AND object.sweep_attempt_id = NEW.sweep_attempt_id
    )
    OR EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_object_sweep_authorization_scopes scope
       WHERE scope.account_id = NEW.account_id
         AND scope.account_generation = NEW.account_generation
         AND scope.sweep_attempt_id = NEW.sweep_attempt_id
         AND scope.object_count <> (
           SELECT COUNT(*)::bigint
             FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
            WHERE object.account_id = scope.account_id
              AND object.account_generation = scope.account_generation
              AND object.sweep_attempt_id = scope.sweep_attempt_id
              AND object.scope_id = scope.scope_id
         )
    )
  THEN
    RAISE EXCEPTION 'workflow cleanup sweep authorization is immutable or incomplete';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_authorization_seal_guard
  ON jobs_workflow_cleanup_object_sweep_authorizations;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_authorization_seal_guard
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_authorizations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_sweep_authorization_seal();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_sweep_manifest_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
     WHERE sweep_auth.account_id = NEW.account_id
       AND sweep_auth.account_generation = NEW.account_generation
       AND sweep_auth.sweep_attempt_id = NEW.sweep_attempt_id
       AND NOT sweep_auth.sealed
  ) THEN
    RAISE EXCEPTION 'workflow cleanup sweep authorization is sealed';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_scope_manifest_insert_guard
  ON jobs_workflow_cleanup_object_sweep_authorization_scopes;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_scope_manifest_insert_guard
BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_authorization_scopes
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_sweep_manifest_insert();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_manifest_object_insert_guard
  ON jobs_workflow_cleanup_object_sweep_manifest_objects;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_manifest_object_insert_guard
BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_manifest_objects
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_sweep_manifest_insert();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_scope_manifest_immutable
  ON jobs_workflow_cleanup_object_sweep_authorization_scopes;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_scope_manifest_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_authorization_scopes
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_manifest_object_immutable
  ON jobs_workflow_cleanup_object_sweep_manifest_objects;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_manifest_object_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_manifest_objects
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_scope_immutable
  ON jobs_workflow_cleanup_object_sweep_scopes;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_scope_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_scopes
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_tombstone_immutable
  ON jobs_workflow_cleanup_completion_tombstones;
CREATE TRIGGER trg_jobs_workflow_cleanup_tombstone_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_completion_tombstones
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  old_json JSONB := to_jsonb(OLD);
  old_account_id TEXT := old_json ->> 'account_id';
  old_account_generation BIGINT := (old_json ->> 'account_generation')::bigint;
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id = old_account_id
       AND (old_account_generation IS NULL OR token.account_generation = old_account_generation)
       AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = old_account_id)
  ) THEN
    RAISE EXCEPTION 'workflow cleanup evidence cannot be deleted before completion';
  END IF;
  RETURN OLD;
END;
$$;

CREATE OR REPLACE FUNCTION guard_jobs_workflow_command_cleanup_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_targets target
     WHERE target.start_command_id = OLD.id AND target.account_id = OLD.account_id
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id = OLD.account_id
       AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
  ) THEN
    RAISE EXCEPTION 'workflow cleanup target cannot be deleted before completion';
  END IF;
  RETURN OLD;
END;
$$;

DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_page_delete_immutable
  ON jobs_workflow_legacy_inventory_pages;
CREATE TRIGGER trg_jobs_workflow_legacy_page_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_inventory_pages
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_target_delete_immutable
  ON jobs_workflow_legacy_targets;
CREATE TRIGGER trg_jobs_workflow_legacy_target_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_targets
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_observation_delete_immutable
  ON jobs_workflow_legacy_target_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_observation_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_target_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_zero_delete_immutable
  ON jobs_workflow_legacy_zero_observations;
CREATE TRIGGER trg_jobs_workflow_legacy_zero_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_zero_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_tombstone_delete_immutable
  ON jobs_workflow_cleanup_completion_tombstones;
CREATE TRIGGER trg_jobs_workflow_cleanup_tombstone_delete_immutable
BEFORE DELETE ON jobs_workflow_cleanup_completion_tombstones
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();

DROP TRIGGER IF EXISTS trg_jobs_workflow_command_cleanup_delete_guard
  ON jobs_workflow_commands;
CREATE TRIGGER trg_jobs_workflow_command_cleanup_delete_guard
BEFORE DELETE ON jobs_workflow_commands
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_command_cleanup_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_target_delete_guard
  ON jobs_workflow_cleanup_targets;
CREATE TRIGGER trg_jobs_workflow_cleanup_target_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_targets
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_observation_delete_guard
  ON jobs_workflow_execution_cleanup_observations;
CREATE TRIGGER trg_jobs_workflow_cleanup_observation_delete_guard
BEFORE DELETE ON jobs_workflow_execution_cleanup_observations
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_authority_delete_guard
  ON jobs_workflow_cleanup_v2_target_authorities;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_authority_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_target_authorities
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_known_run_delete_guard
  ON jobs_workflow_cleanup_v2_known_runs;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_known_run_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_known_runs
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_run_observation_delete_guard
  ON jobs_workflow_cleanup_v2_run_observations;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_run_observation_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_run_observations
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_v2_receipt_delete_guard
  ON jobs_workflow_cleanup_v2_receipts;
CREATE TRIGGER trg_jobs_workflow_cleanup_v2_receipt_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_receipts
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_progress_immutable
  ON jobs_workflow_cleanup_object_sweep_progress;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_progress_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_progress
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_observation_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_progress_delete_guard
  ON jobs_workflow_cleanup_object_sweep_progress;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_progress_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_progress
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_scope_delete_guard
  ON jobs_workflow_cleanup_object_sweep_scopes;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_scope_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_scopes
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_authorization_delete_guard
  ON jobs_workflow_cleanup_object_sweep_authorizations;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_authorization_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_authorizations
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_manifest_scope_delete_guard
  ON jobs_workflow_cleanup_object_sweep_authorization_scopes;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_manifest_scope_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_authorization_scopes
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_sweep_manifest_object_delete_guard
  ON jobs_workflow_cleanup_object_sweep_manifest_objects;
CREATE TRIGGER trg_jobs_workflow_cleanup_sweep_manifest_object_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_manifest_objects
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_binding_delete_guard
  ON jobs_workflow_cleanup_account_bindings;
CREATE TRIGGER trg_jobs_workflow_cleanup_binding_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_account_bindings
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_account_evidence_delete();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_cleanup_generation_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
     WHERE binding.account_id = OLD.account_id
       AND binding.workflow_cleanup_generation = OLD.generation
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id = OLD.account_id
       AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
  ) THEN
    RAISE EXCEPTION 'workflow cleanup generation cannot be deleted';
  END IF;
  RETURN OLD;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_generation_delete_guard
  ON jobs_workflow_cleanup_generations;
CREATE TRIGGER trg_jobs_workflow_cleanup_generation_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_generations
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_cleanup_generation_delete();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_complete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.state = 'complete' AND OLD.state <> 'complete' AND (
    NEW.scan_pass <> 2 OR NEW.completed_at_ms IS NULL
    OR NEW.page_token_ciphertext IS NOT NULL OR NEW.page_token_hmac_sha256 IS NOT NULL
    OR NEW.revalidate_after_ms IS NULL OR NEW.revalidate_after_ms <= NEW.completed_at_ms
    OR NEW.completion_digest_sha256 IS NULL
    OR EXISTS (
      SELECT 1 FROM jobs_workflow_legacy_targets target
       WHERE target.generation = NEW.generation
         AND (target.target_state <> 'absence_proved'
           OR target.positive_reset_required)
    )
    OR EXISTS (
      SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
       WHERE page.generation = NEW.generation
         AND NOT page.raw_ciphertexts_scrubbed
    )
    OR EXISTS (
      SELECT 1 FROM jobs_workflow_legacy_targets target
       WHERE target.generation = NEW.generation
         AND NOT target.raw_ids_scrubbed
    )
    OR (SELECT COUNT(*)::bigint FROM jobs_workflow_legacy_zero_observations zero
        JOIN jobs_workflow_legacy_inventory_pages page
          ON page.generation = zero.generation
         AND page.completion_epoch = zero.completion_epoch
         AND page.scan_pass = zero.scan_pass
         AND page.page_index = zero.final_page_index
         AND page.page_digest_sha256 = zero.final_page_digest_sha256
         AND page.page_index = 0
         AND page.input_page_token_ciphertext IS NULL
         AND page.input_page_token_hmac_sha256 IS NULL
         AND page.page_target_count = 0
         AND page.next_page_token_ciphertext IS NULL
         AND page.next_page_token_hmac_sha256 IS NULL
         WHERE zero.generation = NEW.generation
           AND zero.completion_epoch = NEW.completion_epoch) <> 2
    OR NOT EXISTS (
      SELECT 1 FROM jobs_workflow_legacy_zero_observations first
      JOIN jobs_workflow_legacy_zero_observations second
        ON second.generation = first.generation
       AND second.completion_epoch = first.completion_epoch
       AND second.scan_pass = 2
       AND second.recorded_at_ms >= first.recorded_at_ms + NEW.confirmation_age_ms
     WHERE first.generation = NEW.generation
       AND first.completion_epoch = NEW.completion_epoch
       AND first.scan_pass = 1
    )
  ) THEN
    RAISE EXCEPTION 'workflow legacy inventory is not exactly zero';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_complete_guard
  ON jobs_workflow_legacy_inventory_generations;
CREATE TRIGGER trg_jobs_workflow_legacy_complete_guard
BEFORE UPDATE OF state, completed_at_ms, completion_digest_sha256, revalidate_after_ms
ON jobs_workflow_legacy_inventory_generations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_complete();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_legacy_complete_immutable()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state = 'complete' AND NEW.state = 'complete' AND (
    NEW.scan_pass <> OLD.scan_pass OR NEW.page_index <> OLD.page_index
    OR NEW.predecessor_page_digest_sha256 <> OLD.predecessor_page_digest_sha256
    OR NEW.page_token_ciphertext IS DISTINCT FROM OLD.page_token_ciphertext
    OR NEW.page_token_hmac_sha256 IS DISTINCT FROM OLD.page_token_hmac_sha256
    OR NEW.request_epoch <> OLD.request_epoch OR NEW.fence <> OLD.fence
    OR NEW.request_id IS DISTINCT FROM OLD.request_id
    OR NEW.first_request_started_at_ms IS DISTINCT FROM OLD.first_request_started_at_ms
    OR NEW.last_outcome_code IS DISTINCT FROM OLD.last_outcome_code
    OR NEW.lease_owner IS DISTINCT FROM OLD.lease_owner
    OR NEW.lease_token_sha256 IS DISTINCT FROM OLD.lease_token_sha256
    OR NEW.lease_expires_at_ms IS DISTINCT FROM OLD.lease_expires_at_ms
    OR NEW.next_attempt_at_ms <> OLD.next_attempt_at_ms
    OR NEW.first_zero_observed_at_ms IS DISTINCT FROM OLD.first_zero_observed_at_ms
    OR NEW.completed_at_ms IS DISTINCT FROM OLD.completed_at_ms
    OR NEW.completion_digest_sha256 IS DISTINCT FROM OLD.completion_digest_sha256
    OR NEW.revalidate_after_ms IS DISTINCT FROM OLD.revalidate_after_ms
    OR NEW.completion_epoch <> OLD.completion_epoch
    OR NEW.updated_at_ms <> OLD.updated_at_ms
  ) THEN
    RAISE EXCEPTION 'workflow legacy completion is immutable until revalidation';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_legacy_complete_immutable
  ON jobs_workflow_legacy_inventory_generations;
CREATE TRIGGER trg_jobs_workflow_legacy_complete_immutable
BEFORE UPDATE
ON jobs_workflow_legacy_inventory_generations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_legacy_complete_immutable();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_binding_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.account_id <> OLD.account_id
    OR NEW.account_generation <> OLD.account_generation
    OR NEW.workflow_cleanup_generation <> OLD.workflow_cleanup_generation
    OR NEW.cleanup_generation_id <> OLD.cleanup_generation_id
    OR NEW.target_set_hmac_sha256 <> OLD.target_set_hmac_sha256
    OR NEW.legacy_generation <> OLD.legacy_generation
    OR NEW.legacy_inventory_generation_id <> OLD.legacy_inventory_generation_id
    OR NEW.legacy_query_digest_sha256 <> OLD.legacy_query_digest_sha256
    OR NEW.created_at_ms <> OLD.created_at_ms
    OR NEW.updated_at_ms < OLD.updated_at_ms
    OR (OLD.state IN ('draining', 'complete') AND NEW.state = 'frozen')
    OR (OLD.state = 'complete' AND NEW.state = 'draining' AND (
      NEW.completion_tombstone_id IS NOT NULL
      OR NEW.completion_digest_sha256 IS NOT NULL
      OR NEW.hard_delete_authorized_at_ms IS NOT NULL
      OR NEW.hard_delete_sweep_attempt_id IS NOT NULL
      OR NEW.hard_delete_authorization_digest_sha256 IS NOT NULL
    ))
    OR (OLD.object_sweep_started_at_ms IS NOT NULL
      AND NEW.object_sweep_started_at_ms IS DISTINCT FROM OLD.object_sweep_started_at_ms)
    OR (OLD.object_sweep_started_at_ms IS NULL
      AND NEW.object_sweep_started_at_ms IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
         WHERE sweep_auth.account_id = NEW.account_id
           AND sweep_auth.account_generation = NEW.account_generation
           AND sweep_auth.sealed
           AND sweep_auth.authorized_at_ms = NEW.object_sweep_started_at_ms
      ))
    OR NEW.object_sweep_deleted_count < OLD.object_sweep_deleted_count
    OR NEW.object_sweep_orphan_count < OLD.object_sweep_orphan_count
    OR ((NEW.object_sweep_deleted_count > 0 OR NEW.object_sweep_orphan_count > 0)
      AND NEW.object_sweep_started_at_ms IS NULL)
    OR (OLD.hard_delete_authorized_at_ms IS NOT NULL
      AND NEW.hard_delete_authorized_at_ms IS NOT NULL AND (
        NEW.hard_delete_authorized_at_ms <> OLD.hard_delete_authorized_at_ms
        OR NEW.hard_delete_sweep_attempt_id <> OLD.hard_delete_sweep_attempt_id
        OR NEW.hard_delete_authorization_digest_sha256 <>
           OLD.hard_delete_authorization_digest_sha256
      ))
    OR (OLD.hard_delete_authorized_at_ms IS NULL
      AND NEW.hard_delete_authorized_at_ms IS NOT NULL AND (
        NEW.state <> 'complete' OR NOT EXISTS (
          SELECT 1 FROM jobs_workflow_cleanup_ready_object_sweeps sweep
           WHERE sweep.account_id = NEW.account_id
             AND sweep.account_generation = NEW.account_generation
             AND sweep.sweep_attempt_id = NEW.hard_delete_sweep_attempt_id
             AND sweep.completion_tombstone_id = NEW.completion_tombstone_id
             AND sweep.completion_digest_sha256 = NEW.completion_digest_sha256
        )
      ))
  THEN
    RAISE EXCEPTION 'workflow cleanup account binding is not monotonic';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_binding_monotonic
  ON jobs_workflow_cleanup_account_bindings;
CREATE TRIGGER trg_jobs_workflow_cleanup_binding_monotonic
BEFORE UPDATE ON jobs_workflow_cleanup_account_bindings
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_binding_monotonic();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_binding_complete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.state = 'complete' AND (
    OLD.state <> 'complete'
    OR NEW.completion_tombstone_id IS DISTINCT FROM OLD.completion_tombstone_id
    OR NEW.completion_digest_sha256 IS DISTINCT FROM OLD.completion_digest_sha256
  ) AND NOT (
    EXISTS (
      SELECT 1 FROM account_deletion_intents deletion
       WHERE deletion.account_id = NEW.account_id
         AND deletion.requested_at_ms = NEW.account_generation
    )
    AND EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_generations generation
       WHERE generation.account_id = NEW.account_id
         AND generation.generation = NEW.workflow_cleanup_generation
         AND generation.target_set_hmac_sha256 = NEW.target_set_hmac_sha256
         AND generation.target_count = (
           SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets target
            WHERE target.account_id = generation.account_id
              AND target.generation = generation.generation
              AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256
         )
         AND generation.target_count = (
           SELECT COUNT(*)::bigint
             FROM jobs_workflow_cleanup_v2_target_authorities authority
            WHERE authority.account_id = generation.account_id
              AND authority.workflow_cleanup_generation = generation.generation
         )
         AND NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
            WHERE authority.account_id = generation.account_id
              AND authority.workflow_cleanup_generation = generation.generation
              AND authority.positive_reset_required
         )
         AND generation.target_count = (
           SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_v2_proved_targets proved
            WHERE proved.account_id = generation.account_id
              AND proved.generation = generation.generation
              AND proved.target_set_hmac_sha256 = generation.target_set_hmac_sha256
         )
    )
    AND NOT EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_targets target
       WHERE target.account_id = NEW.account_id
         AND target.generation = NEW.workflow_cleanup_generation
         AND target.target_set_hmac_sha256 = NEW.target_set_hmac_sha256
         AND target.target_state <> 'absence_proved'
    )
    AND EXISTS (
      SELECT 1 FROM jobs_workflow_legacy_inventory_head head
      JOIN jobs_workflow_legacy_inventory_generations legacy
        ON legacy.generation = head.generation
       AND legacy.query_digest_sha256 = head.query_digest_sha256
     WHERE head.singleton_id = 1
       AND legacy.generation = NEW.legacy_generation
       AND legacy.query_digest_sha256 = NEW.legacy_query_digest_sha256
       AND legacy.state = 'complete'
       AND legacy.revalidate_after_ms >
           FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint
       AND NOT EXISTS (
         SELECT 1 FROM jobs_workflow_legacy_targets target
          WHERE target.generation = NEW.legacy_generation
            AND (target.target_state <> 'absence_proved'
              OR target.positive_reset_required)
       )
    )
    AND EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_completion_tombstones tombstone
      JOIN jobs_workflow_legacy_inventory_generations legacy
        ON legacy.generation = tombstone.legacy_generation
       AND legacy.completion_epoch = tombstone.legacy_completion_epoch
       AND legacy.completion_digest_sha256 = tombstone.legacy_completion_digest_sha256
       AND legacy.revalidate_after_ms = tombstone.legacy_revalidate_after_ms
     WHERE tombstone.tombstone_id = NEW.completion_tombstone_id
       AND tombstone.completion_digest_sha256 = NEW.completion_digest_sha256
       AND tombstone.account_generation = NEW.account_generation
       AND tombstone.workflow_cleanup_generation = NEW.workflow_cleanup_generation
       AND tombstone.cleanup_generation_id = NEW.cleanup_generation_id
       AND tombstone.target_set_hmac_sha256 = NEW.target_set_hmac_sha256
       AND tombstone.legacy_generation = NEW.legacy_generation
       AND tombstone.legacy_inventory_generation_id = NEW.legacy_inventory_generation_id
    )
  ) THEN
    RAISE EXCEPTION 'workflow cleanup account binding is not exactly complete';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_binding_complete_guard
  ON jobs_workflow_cleanup_account_bindings;
CREATE TRIGGER trg_jobs_workflow_cleanup_binding_complete_guard
BEFORE UPDATE OF state, completion_tombstone_id, completion_digest_sha256
ON jobs_workflow_cleanup_account_bindings
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_binding_complete();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_account_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (
    EXISTS (SELECT 1 FROM account_deletion_intents deletion WHERE deletion.account_id = OLD.id)
    OR EXISTS (SELECT 1 FROM jobs_workflow_commands command WHERE command.account_id = OLD.id)
    OR EXISTS (SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
                WHERE binding.account_id = OLD.id)
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_ready ready
     WHERE ready.account_id = OLD.id
       AND ready.account_generation = (
       SELECT requested_at_ms FROM account_deletion_intents WHERE account_id = OLD.id
     )
  ) THEN
    RAISE EXCEPTION 'exact workflow and runner sweep completion is required';
  END IF;
  INSERT INTO jobs_workflow_cleanup_hard_delete_cascade_tokens (
    account_id, account_generation, sweep_attempt_id,
    authorization_digest_sha256, created_at_ms
  )
  SELECT binding.account_id, binding.account_generation,
         binding.hard_delete_sweep_attempt_id,
         binding.hard_delete_authorization_digest_sha256,
         FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint
    FROM jobs_workflow_cleanup_account_bindings binding
    JOIN jobs_workflow_cleanup_hard_delete_ready ready
      ON ready.account_id = binding.account_id
     AND ready.account_generation = binding.account_generation
     AND ready.hard_delete_sweep_attempt_id = binding.hard_delete_sweep_attempt_id
   WHERE binding.account_id = OLD.id
  ON CONFLICT(account_id) DO UPDATE SET
    account_generation = EXCLUDED.account_generation,
    sweep_attempt_id = EXCLUDED.sweep_attempt_id,
    authorization_digest_sha256 = EXCLUDED.authorization_digest_sha256,
    created_at_ms = EXCLUDED.created_at_ms;
  RETURN OLD;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_guard ON accounts;
CREATE TRIGGER trg_jobs_workflow_account_delete_guard
BEFORE DELETE ON accounts
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_account_delete();

DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_cascade_token_cleanup ON accounts;
DROP FUNCTION IF EXISTS cleanup_jobs_workflow_account_delete_token();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_application_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM jobs_workflow_commands command
     WHERE command.account_id = OLD.account_id AND command.application_id = OLD.id
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id = OLD.account_id
       AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
  ) THEN
    RAISE EXCEPTION 'workflow cleanup application deletion requires account cascade';
  END IF;
  RETURN OLD;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_application_delete_guard ON jobs_applications;
CREATE TRIGGER trg_jobs_workflow_application_delete_guard
BEFORE DELETE ON jobs_applications
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_application_delete();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_intervention_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM jobs_workflow_commands command
     WHERE command.account_id = OLD.account_id AND command.intervention_id = OLD.id
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
     WHERE token.account_id = OLD.account_id
       AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
  ) THEN
    RAISE EXCEPTION 'workflow cleanup intervention deletion requires account cascade';
  END IF;
  RETURN OLD;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_intervention_delete_guard ON jobs_interventions;
CREATE TRIGGER trg_jobs_workflow_intervention_delete_guard
BEFORE DELETE ON jobs_interventions
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_intervention_delete();

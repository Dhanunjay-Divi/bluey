-- Target: SQLite
-- Phase 610: durable global Temporal-v1 inventory and account-bound workflow cleanup.
-- Legacy Temporal identifiers and continuation tokens, plus companion copies
-- of v2 known-run identifiers, are encrypted. Retained Phase609 v2 workflow
-- and first-run authority is bounded provider-opaque data and is deleted only
-- by the authorized account cascade.

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_inventory_generations (
  generation                       INTEGER PRIMARY KEY
    CHECK(generation BETWEEN 1 AND 9007199254740991),
  inventory_generation_id          TEXT NOT NULL UNIQUE
    CHECK(length(inventory_generation_id) BETWEEN 20 AND 128
      AND inventory_generation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  namespace_ciphertext             TEXT NOT NULL
    CHECK(length(namespace_ciphertext) BETWEEN 32 AND 16777216
      AND substr(namespace_ciphertext, 1, 14) = 'bluey-jobs:v1:'),
  namespace_hmac_sha256            TEXT NOT NULL
    CHECK(length(namespace_hmac_sha256) = 64
      AND lower(namespace_hmac_sha256) = namespace_hmac_sha256
      AND namespace_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  workflow_type                    TEXT NOT NULL CHECK(workflow_type = 'applicationWorkflow'),
  visibility_cutoff_ms             INTEGER NOT NULL
    CHECK(visibility_cutoff_ms BETWEEN 0 AND 253402300799999),
  confirmation_age_ms              INTEGER NOT NULL
    CHECK(confirmation_age_ms BETWEEN 1000 AND 600000),
  visibility_query_ciphertext      TEXT NOT NULL
    CHECK(length(visibility_query_ciphertext) BETWEEN 32 AND 16777216
      AND substr(visibility_query_ciphertext, 1, 14) = 'bluey-jobs:v1:'),
  query_digest_sha256              TEXT NOT NULL
    CHECK(length(query_digest_sha256) = 64
      AND lower(query_digest_sha256) = query_digest_sha256
      AND query_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  query_hmac_sha256                TEXT NOT NULL
    CHECK(length(query_hmac_sha256) = 64
      AND lower(query_hmac_sha256) = query_hmac_sha256
      AND query_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  state                            TEXT NOT NULL CHECK(state IN (
    'scanning', 'draining', 'awaiting_second_scan', 'complete', 'identity_conflict'
  )),
  scan_pass                        INTEGER NOT NULL CHECK(scan_pass IN (1, 2)),
  page_index                       INTEGER NOT NULL
    CHECK(page_index BETWEEN 0 AND 4095),
  predecessor_page_digest_sha256   TEXT NOT NULL
    CHECK(length(predecessor_page_digest_sha256) = 64
      AND lower(predecessor_page_digest_sha256) = predecessor_page_digest_sha256
      AND predecessor_page_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  page_token_ciphertext            TEXT
    CHECK(page_token_ciphertext IS NULL OR (
      length(page_token_ciphertext) BETWEEN 32 AND 16777216
      AND substr(page_token_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  page_token_hmac_sha256           TEXT
    CHECK(page_token_hmac_sha256 IS NULL OR (
      length(page_token_hmac_sha256) = 64
      AND lower(page_token_hmac_sha256) = page_token_hmac_sha256
      AND page_token_hmac_sha256 NOT GLOB '*[^0-9a-f]*')),
  request_epoch                    INTEGER NOT NULL DEFAULT 0
    CHECK(request_epoch BETWEEN 0 AND 9007199254740991),
  fence                            INTEGER NOT NULL DEFAULT 0
    CHECK(fence BETWEEN 0 AND 9007199254740991),
  request_id                       TEXT
    CHECK(request_id IS NULL OR (
      length(request_id) BETWEEN 20 AND 128
      AND request_id NOT GLOB '*[^A-Za-z0-9_-]*')),
  first_request_started_at_ms      INTEGER,
  last_outcome_code                TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'page_recorded', 'transport_unknown', 'gateway_unavailable', 'identity_conflict'
  )),
  lease_owner                      TEXT,
  lease_token_sha256               TEXT,
  lease_expires_at_ms              INTEGER,
  next_attempt_at_ms               INTEGER NOT NULL
    CHECK(next_attempt_at_ms BETWEEN 0 AND 9007199254740991),
  completion_epoch                 INTEGER NOT NULL DEFAULT 1
    CHECK(completion_epoch BETWEEN 1 AND 9007199254740991),
  first_zero_observed_at_ms        INTEGER,
  completed_at_ms                  INTEGER,
  completion_digest_sha256         TEXT,
  revalidate_after_ms              INTEGER,
  created_at_ms                    INTEGER NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                    INTEGER NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(generation, query_digest_sha256),
  UNIQUE(generation, inventory_generation_id, query_digest_sha256),
  UNIQUE(namespace_hmac_sha256, workflow_type, visibility_cutoff_ms, query_digest_sha256),
  CHECK((page_token_ciphertext IS NULL) = (page_token_hmac_sha256 IS NULL)),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND request_id IS NULL)
    OR (lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 IS NOT NULL AND length(lease_token_sha256) = 64
      AND lower(lease_token_sha256) = lease_token_sha256
      AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND request_id IS NOT NULL)
  ),
  CHECK((state = 'awaiting_second_scan') = (first_zero_observed_at_ms IS NOT NULL)),
  CHECK(state <> 'complete' OR (
    scan_pass = 2 AND completed_at_ms IS NOT NULL
      AND revalidate_after_ms IS NOT NULL AND revalidate_after_ms > completed_at_ms
      AND completion_digest_sha256 IS NOT NULL
      AND length(completion_digest_sha256) = 64
      AND lower(completion_digest_sha256) = completion_digest_sha256
      AND completion_digest_sha256 NOT GLOB '*[^0-9a-f]*'
  )),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_inventory_head (
  singleton_id                     INTEGER PRIMARY KEY CHECK(singleton_id = 1),
  generation                       INTEGER NOT NULL,
  inventory_generation_id          TEXT NOT NULL UNIQUE,
  query_digest_sha256              TEXT NOT NULL,
  updated_at_ms                    INTEGER NOT NULL,
  FOREIGN KEY(generation, inventory_generation_id, query_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_generations(
      generation, inventory_generation_id, query_digest_sha256
    ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_inventory_pages (
  generation                       INTEGER NOT NULL,
  completion_epoch                 INTEGER NOT NULL,
  scan_pass                        INTEGER NOT NULL CHECK(scan_pass IN (1, 2)),
  page_index                       INTEGER NOT NULL
    CHECK(page_index BETWEEN 0 AND 4095),
  request_epoch                    INTEGER NOT NULL
    CHECK(request_epoch BETWEEN 1 AND 9007199254740991),
  fence                            INTEGER NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  request_id                       TEXT NOT NULL UNIQUE
    CHECK(length(request_id) BETWEEN 20 AND 128
      AND request_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  predecessor_page_digest_sha256   TEXT,
  input_page_token_ciphertext      TEXT,
  input_page_token_hmac_sha256     TEXT,
  next_page_token_ciphertext       TEXT,
  next_page_token_hmac_sha256      TEXT,
  raw_ciphertexts_scrubbed         INTEGER NOT NULL DEFAULT 0
    CHECK(raw_ciphertexts_scrubbed IN (0, 1)),
  page_target_count                INTEGER NOT NULL
    CHECK(page_target_count BETWEEN 0 AND 100),
  page_targets_digest_sha256       TEXT NOT NULL,
  page_digest_sha256               TEXT NOT NULL,
  evidence_digest_sha256           TEXT NOT NULL,
  recorded_at_ms                   INTEGER NOT NULL,
  PRIMARY KEY(generation, completion_epoch, scan_pass, page_index),
  UNIQUE(generation, completion_epoch, scan_pass, page_index, page_digest_sha256),
  FOREIGN KEY(generation) REFERENCES jobs_workflow_legacy_inventory_generations(generation)
    ON DELETE RESTRICT,
  CHECK(
    (raw_ciphertexts_scrubbed = 0
      AND (input_page_token_ciphertext IS NULL) = (input_page_token_hmac_sha256 IS NULL)
      AND (next_page_token_ciphertext IS NULL) = (next_page_token_hmac_sha256 IS NULL))
    OR (raw_ciphertexts_scrubbed = 1
      AND input_page_token_ciphertext IS NULL
      AND next_page_token_ciphertext IS NULL)
  ),
  CHECK(input_page_token_ciphertext IS NULL OR (
    length(input_page_token_ciphertext) BETWEEN 32 AND 16777216
      AND substr(input_page_token_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  CHECK(next_page_token_ciphertext IS NULL OR (
    length(next_page_token_ciphertext) BETWEEN 32 AND 16777216
      AND substr(next_page_token_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  CHECK(input_page_token_hmac_sha256 IS NULL OR (
    length(input_page_token_hmac_sha256) = 64
      AND lower(input_page_token_hmac_sha256) = input_page_token_hmac_sha256
      AND input_page_token_hmac_sha256 NOT GLOB '*[^0-9a-f]*')),
  CHECK(next_page_token_hmac_sha256 IS NULL OR (
    length(next_page_token_hmac_sha256) = 64
      AND lower(next_page_token_hmac_sha256) = next_page_token_hmac_sha256
      AND next_page_token_hmac_sha256 NOT GLOB '*[^0-9a-f]*')),
  CHECK((page_index = 0) = (predecessor_page_digest_sha256 IS NULL)),
  CHECK(predecessor_page_digest_sha256 IS NULL OR (
    length(predecessor_page_digest_sha256) = 64
      AND lower(predecessor_page_digest_sha256) = predecessor_page_digest_sha256
      AND predecessor_page_digest_sha256 NOT GLOB '*[^0-9a-f]*')),
  CHECK(length(page_targets_digest_sha256) = 64
    AND lower(page_targets_digest_sha256) = page_targets_digest_sha256
    AND page_targets_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(page_digest_sha256) = 64
    AND lower(page_digest_sha256) = page_digest_sha256
    AND page_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(evidence_digest_sha256) = 64
    AND lower(evidence_digest_sha256) = evidence_digest_sha256
    AND evidence_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_targets (
  generation                       INTEGER NOT NULL,
  target_identity_hmac_sha256      TEXT NOT NULL,
  workflow_id_ciphertext           TEXT,
  workflow_id_hmac_sha256          TEXT NOT NULL,
  run_id_ciphertext                TEXT,
  run_id_hmac_sha256               TEXT NOT NULL,
  first_execution_run_id_ciphertext TEXT,
  first_execution_run_id_hmac_sha256 TEXT NOT NULL,
  target_digest_sha256             TEXT NOT NULL,
  raw_ids_scrubbed                 INTEGER NOT NULL DEFAULT 0 CHECK(raw_ids_scrubbed IN (0, 1)),
  discovered_completion_epoch      INTEGER NOT NULL,
  discovered_scan_pass             INTEGER NOT NULL CHECK(discovered_scan_pass IN (1, 2)),
  discovered_page_index            INTEGER NOT NULL,
  discovered_page_digest_sha256    TEXT NOT NULL,
  observed_status                  TEXT NOT NULL CHECK(observed_status IN ('running', 'closed')),
  target_state                     TEXT NOT NULL CHECK(target_state IN (
    'running_wait', 'delete_pending', 'absence_pending',
    'absence_proved', 'identity_conflict'
  )),
  proof_epoch                      INTEGER NOT NULL DEFAULT 1
    CHECK(proof_epoch BETWEEN 1 AND 9007199254740991),
  positive_reset_required          INTEGER NOT NULL DEFAULT 0
    CHECK(positive_reset_required IN (0, 1)),
  observation_pass                 INTEGER NOT NULL DEFAULT 1 CHECK(observation_pass IN (1, 2)),
  first_absence_observed_at_ms     INTEGER,
  request_epoch                    INTEGER NOT NULL DEFAULT 0,
  fence                            INTEGER NOT NULL DEFAULT 0,
  request_id                       TEXT,
  first_request_started_at_ms      INTEGER,
  last_outcome_code                TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'running', 'retry', 'absence_proved', 'identity_conflict',
    'transport_unknown', 'gateway_unavailable'
  )),
  lease_owner                      TEXT,
  lease_token_sha256               TEXT,
  lease_expires_at_ms              INTEGER,
  next_attempt_at_ms               INTEGER NOT NULL,
  absence_proved_at_ms             INTEGER,
  created_at_ms                    INTEGER NOT NULL,
  updated_at_ms                    INTEGER NOT NULL,
  PRIMARY KEY(generation, target_identity_hmac_sha256),
  UNIQUE(generation, workflow_id_hmac_sha256, run_id_hmac_sha256),
  UNIQUE(generation, target_identity_hmac_sha256, target_digest_sha256),
  FOREIGN KEY(generation, discovered_completion_epoch, discovered_scan_pass,
    discovered_page_index, discovered_page_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_pages(
      generation, completion_epoch, scan_pass, page_index, page_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(length(target_identity_hmac_sha256) = 64
    AND lower(target_identity_hmac_sha256) = target_identity_hmac_sha256
    AND target_identity_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(workflow_id_hmac_sha256) = 64
    AND lower(workflow_id_hmac_sha256) = workflow_id_hmac_sha256
    AND workflow_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(run_id_hmac_sha256) = 64
    AND lower(run_id_hmac_sha256) = run_id_hmac_sha256
    AND run_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(first_execution_run_id_hmac_sha256) = 64
    AND lower(first_execution_run_id_hmac_sha256) = first_execution_run_id_hmac_sha256
    AND first_execution_run_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(target_digest_sha256) = 64
    AND lower(target_digest_sha256) = target_digest_sha256
    AND target_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(workflow_id_ciphertext IS NULL OR (
    length(workflow_id_ciphertext) BETWEEN 32 AND 16777216
      AND substr(workflow_id_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  CHECK(run_id_ciphertext IS NULL OR (
    length(run_id_ciphertext) BETWEEN 32 AND 16777216
      AND substr(run_id_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  CHECK(first_execution_run_id_ciphertext IS NULL OR (
    length(first_execution_run_id_ciphertext) BETWEEN 32 AND 16777216
      AND substr(first_execution_run_id_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  CHECK(
    (raw_ids_scrubbed = 0 AND workflow_id_ciphertext IS NOT NULL
      AND run_id_ciphertext IS NOT NULL
      AND first_execution_run_id_ciphertext IS NOT NULL)
    OR (raw_ids_scrubbed = 1 AND workflow_id_ciphertext IS NULL
      AND run_id_ciphertext IS NULL
      AND first_execution_run_id_ciphertext IS NULL)
  ),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND request_id IS NULL)
    OR (lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 IS NOT NULL AND length(lease_token_sha256) = 64
      AND lower(lease_token_sha256) = lease_token_sha256
      AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND request_id IS NOT NULL AND length(request_id) BETWEEN 20 AND 128
      AND request_id NOT GLOB '*[^A-Za-z0-9_-]*')
  ),
  CHECK(target_state <> 'absence_proved' OR absence_proved_at_ms IS NOT NULL),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_target_observations (
  id                                TEXT PRIMARY KEY,
  generation                        INTEGER NOT NULL,
  target_identity_hmac_sha256       TEXT NOT NULL,
  target_digest_sha256              TEXT NOT NULL,
  proof_epoch                       INTEGER NOT NULL
    CHECK(proof_epoch BETWEEN 1 AND 9007199254740991),
  observation_pass                  INTEGER NOT NULL CHECK(observation_pass IN (1, 2)),
  request_epoch                     INTEGER NOT NULL,
  cleanup_fence                    INTEGER NOT NULL,
  cleanup_request_id               TEXT NOT NULL UNIQUE,
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
  recorded_at_ms                    INTEGER NOT NULL,
  UNIQUE(generation, target_identity_hmac_sha256, request_epoch, cleanup_fence),
  FOREIGN KEY(generation, target_identity_hmac_sha256, target_digest_sha256)
    REFERENCES jobs_workflow_legacy_targets(
      generation, target_identity_hmac_sha256, target_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(length(id) BETWEEN 20 AND 128),
  CHECK(length(target_identity_hmac_sha256) = 64
    AND lower(target_identity_hmac_sha256) = target_identity_hmac_sha256
    AND target_identity_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(target_digest_sha256) = 64
    AND lower(target_digest_sha256) = target_digest_sha256
    AND target_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(cleanup_request_id) BETWEEN 20 AND 128
    AND cleanup_request_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(evidence_digest_sha256) = 64
    AND lower(evidence_digest_sha256) = evidence_digest_sha256
    AND evidence_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(outcome <> 'absence_proved' OR (
    describe_state = 'not_found' AND history_state = 'not_found'
      AND visibility_state = 'not_found'
  ))
);

CREATE TABLE IF NOT EXISTS jobs_workflow_legacy_zero_observations (
  generation                       INTEGER NOT NULL,
  completion_epoch                 INTEGER NOT NULL,
  scan_pass                        INTEGER NOT NULL CHECK(scan_pass IN (1, 2)),
  final_page_index                 INTEGER NOT NULL CHECK(final_page_index = 0),
  final_page_digest_sha256         TEXT NOT NULL,
  zero_digest_sha256               TEXT NOT NULL,
  recorded_at_ms                   INTEGER NOT NULL,
  PRIMARY KEY(generation, completion_epoch, scan_pass),
  UNIQUE(generation, completion_epoch, zero_digest_sha256),
  FOREIGN KEY(generation, completion_epoch, scan_pass, final_page_index,
    final_page_digest_sha256)
    REFERENCES jobs_workflow_legacy_inventory_pages(
      generation, completion_epoch, scan_pass, page_index, page_digest_sha256
    ) ON DELETE RESTRICT,
  CHECK(length(final_page_digest_sha256) = 64
    AND lower(final_page_digest_sha256) = final_page_digest_sha256
    AND final_page_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(zero_digest_sha256) = 64
    AND lower(zero_digest_sha256) = zero_digest_sha256
    AND zero_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_completion_tombstones (
  tombstone_id                     TEXT PRIMARY KEY,
  account_subject_hmac_sha256      TEXT NOT NULL,
  account_generation              INTEGER NOT NULL,
  workflow_cleanup_generation     INTEGER NOT NULL,
  cleanup_generation_id           TEXT NOT NULL,
  target_set_hmac_sha256           TEXT NOT NULL,
  legacy_generation               INTEGER NOT NULL,
  legacy_inventory_generation_id  TEXT NOT NULL,
  legacy_completion_epoch         INTEGER NOT NULL,
  legacy_completion_digest_sha256 TEXT NOT NULL,
  legacy_revalidate_after_ms       INTEGER NOT NULL,
  completion_digest_sha256        TEXT NOT NULL,
  completed_at_ms                 INTEGER NOT NULL,
  UNIQUE(tombstone_id, completion_digest_sha256),
  UNIQUE(account_subject_hmac_sha256, account_generation,
    cleanup_generation_id, legacy_completion_epoch,
    legacy_completion_digest_sha256),
  CHECK(length(tombstone_id) BETWEEN 20 AND 128),
  CHECK(length(cleanup_generation_id) BETWEEN 20 AND 128),
  CHECK(length(legacy_inventory_generation_id) BETWEEN 20 AND 128),
  CHECK(length(account_subject_hmac_sha256) = 64
    AND lower(account_subject_hmac_sha256) = account_subject_hmac_sha256
    AND account_subject_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(target_set_hmac_sha256) = 64
    AND lower(target_set_hmac_sha256) = target_set_hmac_sha256
    AND target_set_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(legacy_completion_digest_sha256) = 64
    AND lower(legacy_completion_digest_sha256) = legacy_completion_digest_sha256
    AND legacy_completion_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(legacy_revalidate_after_ms > completed_at_ms),
  CHECK(length(completion_digest_sha256) = 64
    AND lower(completion_digest_sha256) = completion_digest_sha256
    AND completion_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_account_bindings (
  account_id                       TEXT PRIMARY KEY
    REFERENCES accounts(id) ON DELETE CASCADE,
  account_generation              INTEGER NOT NULL,
  workflow_cleanup_generation     INTEGER NOT NULL,
  cleanup_generation_id           TEXT NOT NULL UNIQUE,
  target_set_hmac_sha256           TEXT NOT NULL,
  legacy_generation               INTEGER NOT NULL,
  legacy_inventory_generation_id  TEXT NOT NULL,
  legacy_query_digest_sha256       TEXT NOT NULL,
  state                            TEXT NOT NULL CHECK(state IN ('frozen', 'draining', 'complete')),
  completion_tombstone_id          TEXT,
  completion_digest_sha256         TEXT,
  object_sweep_started_at_ms       INTEGER,
  object_sweep_deleted_count       INTEGER NOT NULL DEFAULT 0
    CHECK(object_sweep_deleted_count BETWEEN 0 AND 9007199254740991),
  object_sweep_orphan_count        INTEGER NOT NULL DEFAULT 0
    CHECK(object_sweep_orphan_count BETWEEN 0 AND 9007199254740991),
  hard_delete_authorized_at_ms     INTEGER,
  hard_delete_sweep_attempt_id     TEXT,
  hard_delete_authorization_digest_sha256 TEXT,
  created_at_ms                    INTEGER NOT NULL,
  updated_at_ms                    INTEGER NOT NULL,
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
  CHECK(hard_delete_sweep_attempt_id IS NULL OR (
    length(hard_delete_sweep_attempt_id) BETWEEN 20 AND 128
      AND hard_delete_sweep_attempt_id NOT GLOB '*[^A-Za-z0-9_-]*')),
  CHECK(hard_delete_authorization_digest_sha256 IS NULL OR (
    length(hard_delete_authorization_digest_sha256) = 64
      AND lower(hard_delete_authorization_digest_sha256) =
          hard_delete_authorization_digest_sha256
      AND hard_delete_authorization_digest_sha256 NOT GLOB '*[^0-9a-f]*')),
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
  workflow_cleanup_generation     INTEGER NOT NULL,
  workflow_id                      TEXT NOT NULL,
  known_run_epoch                  INTEGER NOT NULL DEFAULT 1
    CHECK(known_run_epoch BETWEEN 1 AND 9007199254740991),
  positive_reset_required          INTEGER NOT NULL DEFAULT 0
    CHECK(positive_reset_required IN (0, 1)),
  known_run_set_digest_sha256      TEXT NOT NULL,
  target_digest_sha256             TEXT NOT NULL,
  observation_pass                 INTEGER NOT NULL DEFAULT 1 CHECK(observation_pass IN (1, 2)),
  first_absence_observed_at_ms     INTEGER,
  request_epoch                    INTEGER NOT NULL DEFAULT 0
    CHECK(request_epoch BETWEEN 0 AND 9007199254740991),
  cleanup_fence                    INTEGER NOT NULL DEFAULT 0
    CHECK(cleanup_fence BETWEEN 0 AND 9007199254740991),
  cleanup_request_id               TEXT,
  first_request_started_at_ms      INTEGER,
  last_outcome_code                TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'pending', 'absence_observed', 'transport_unknown', 'gateway_unavailable',
    'identity_conflict'
  )),
  lease_owner                      TEXT,
  lease_token_sha256               TEXT,
  lease_expires_at_ms              INTEGER,
  next_attempt_at_ms               INTEGER NOT NULL,
  created_at_ms                    INTEGER NOT NULL,
  updated_at_ms                    INTEGER NOT NULL,
  PRIMARY KEY(account_id, workflow_cleanup_generation, workflow_id),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_targets(account_id, generation, workflow_id)
    ON DELETE CASCADE,
  CHECK(length(known_run_set_digest_sha256) = 64
    AND lower(known_run_set_digest_sha256) = known_run_set_digest_sha256
    AND known_run_set_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(target_digest_sha256) = 64
    AND lower(target_digest_sha256) = target_digest_sha256
    AND target_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK((observation_pass = 2) = (first_absence_observed_at_ms IS NOT NULL)),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND cleanup_request_id IS NULL)
    OR (lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 IS NOT NULL AND length(lease_token_sha256) = 64
      AND lower(lease_token_sha256) = lease_token_sha256
      AND lease_token_sha256 NOT GLOB '*[^0-9a-f]*'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND cleanup_request_id IS NOT NULL
      AND length(cleanup_request_id) BETWEEN 20 AND 128
      AND cleanup_request_id NOT GLOB '*[^A-Za-z0-9_-]*')
  ),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_known_runs (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     INTEGER NOT NULL,
  workflow_id                      TEXT NOT NULL,
  run_id_hmac_sha256               TEXT NOT NULL,
  run_id_ciphertext                TEXT NOT NULL,
  discovered_request_epoch        INTEGER NOT NULL,
  discovered_cleanup_fence        INTEGER NOT NULL,
  run_identity_digest_sha256       TEXT NOT NULL,
  created_at_ms                    INTEGER NOT NULL,
  PRIMARY KEY(account_id, workflow_cleanup_generation, workflow_id, run_id_hmac_sha256),
  UNIQUE(account_id, workflow_cleanup_generation, workflow_id, run_identity_digest_sha256),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_v2_target_authorities(
      account_id, workflow_cleanup_generation, workflow_id
    ) ON DELETE CASCADE,
  CHECK(length(run_id_hmac_sha256) = 64
    AND lower(run_id_hmac_sha256) = run_id_hmac_sha256
    AND run_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(run_identity_digest_sha256) = 64
    AND lower(run_identity_digest_sha256) = run_identity_digest_sha256
    AND run_identity_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(run_id_ciphertext) BETWEEN 32 AND 16777216
    AND substr(run_id_ciphertext, 1, 14) = 'bluey-jobs:v1:')
);

-- One immutable row per exact known run and observation pass. For the
-- no-execution case, one `workflow` row with the reserved all-zero HMAC is
-- required instead. Completion guards below never accept an aggregate proof.
CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_run_observations (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     INTEGER NOT NULL,
  workflow_id                      TEXT NOT NULL,
  known_run_epoch                  INTEGER NOT NULL,
  observation_pass                INTEGER NOT NULL CHECK(observation_pass IN (1, 2)),
  subject_kind                     TEXT NOT NULL CHECK(subject_kind IN ('run', 'workflow')),
  run_id_hmac_sha256               TEXT NOT NULL,
  target_digest_sha256             TEXT NOT NULL,
  request_epoch                    INTEGER NOT NULL,
  cleanup_fence                    INTEGER NOT NULL,
  cleanup_request_id               TEXT NOT NULL,
  evidence_digest_sha256           TEXT NOT NULL,
  describe_state                   TEXT NOT NULL CHECK(describe_state = 'not_found'),
  history_state                    TEXT NOT NULL CHECK(history_state = 'not_found'),
  visibility_state                 TEXT NOT NULL CHECK(visibility_state = 'not_found'),
  recorded_at_ms                   INTEGER NOT NULL,
  PRIMARY KEY(account_id, workflow_cleanup_generation, workflow_id,
    known_run_epoch, observation_pass, subject_kind, run_id_hmac_sha256),
  UNIQUE(account_id, workflow_cleanup_generation, workflow_id,
    request_epoch, cleanup_fence, observation_pass, subject_kind, run_id_hmac_sha256),
  FOREIGN KEY(account_id, workflow_cleanup_generation, workflow_id)
    REFERENCES jobs_workflow_cleanup_v2_target_authorities(
      account_id, workflow_cleanup_generation, workflow_id
    ) ON DELETE CASCADE,
  CHECK(length(run_id_hmac_sha256) = 64
    AND lower(run_id_hmac_sha256) = run_id_hmac_sha256
    AND run_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(target_digest_sha256) = 64
    AND lower(target_digest_sha256) = target_digest_sha256
    AND target_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(cleanup_request_id) BETWEEN 20 AND 128
    AND cleanup_request_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(evidence_digest_sha256) = 64
    AND lower(evidence_digest_sha256) = evidence_digest_sha256
    AND evidence_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK((subject_kind = 'workflow') =
    (run_id_hmac_sha256 = '0000000000000000000000000000000000000000000000000000000000000000'))
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_v2_receipts (
  account_id                       TEXT NOT NULL,
  workflow_cleanup_generation     INTEGER NOT NULL,
  workflow_id                      TEXT NOT NULL,
  cleanup_request_id               TEXT NOT NULL PRIMARY KEY,
  request_epoch                    INTEGER NOT NULL,
  cleanup_fence                    INTEGER NOT NULL,
  known_run_epoch                  INTEGER NOT NULL,
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
  recorded_at_ms                   INTEGER NOT NULL,
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
      AND substr(first_execution_run_id_ciphertext, 1, 14) = 'bluey-jobs:v1:')),
  CHECK(first_execution_run_id_hmac_sha256 IS NULL OR (
    length(first_execution_run_id_hmac_sha256) = 64
      AND lower(first_execution_run_id_hmac_sha256) = first_execution_run_id_hmac_sha256
      AND first_execution_run_id_hmac_sha256 NOT GLOB '*[^0-9a-f]*')),
  CHECK(length(target_digest_sha256) = 64
    AND lower(target_digest_sha256) = target_digest_sha256
    AND target_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(response_run_set_digest_sha256) = 64
    AND lower(response_run_set_digest_sha256) = response_run_set_digest_sha256
    AND response_run_set_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(evidence_digest_sha256) = 64
    AND lower(evidence_digest_sha256) = evidence_digest_sha256
    AND evidence_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_authorizations (
  account_id                       TEXT NOT NULL,
  account_generation              INTEGER NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  runner_purge_request_id          TEXT NOT NULL,
  runner_purge_generation          INTEGER NOT NULL CHECK(runner_purge_generation >= 1),
  runner_purge_tombstone_generation INTEGER NOT NULL
    CHECK(runner_purge_tombstone_generation >= 1),
  runner_legacy_inventory_generation INTEGER NOT NULL
    CHECK(runner_legacy_inventory_generation >= 1),
  runner_legacy_reconciliation_id  TEXT NOT NULL,
  runner_legacy_authority_id        TEXT NOT NULL,
  runner_legacy_authority_sha256    TEXT NOT NULL,
  runner_authority_digest_sha256    TEXT NOT NULL,
  completion_tombstone_id          TEXT NOT NULL,
  completion_digest_sha256         TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  scope_set_digest_sha256          TEXT NOT NULL,
  scope_count                      INTEGER NOT NULL CHECK(scope_count BETWEEN 1 AND 2),
  known_object_count               INTEGER NOT NULL
    CHECK(known_object_count BETWEEN 0 AND 9007199254740991),
  sealed                           INTEGER NOT NULL DEFAULT 0 CHECK(sealed IN (0, 1)),
  authorized_at_ms                 INTEGER NOT NULL,
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
  CHECK(length(sweep_attempt_id) BETWEEN 20 AND 128
    AND sweep_attempt_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(runner_purge_request_id) BETWEEN 20 AND 128
    AND runner_purge_request_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(runner_legacy_reconciliation_id) BETWEEN 20 AND 128
    AND runner_legacy_reconciliation_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(runner_legacy_authority_id) BETWEEN 20 AND 128
    AND runner_legacy_authority_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(runner_legacy_authority_sha256) = 64
    AND lower(runner_legacy_authority_sha256) = runner_legacy_authority_sha256
    AND runner_legacy_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(runner_authority_digest_sha256) = 64
    AND lower(runner_authority_digest_sha256) = runner_authority_digest_sha256
    AND runner_authority_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(completion_digest_sha256) = 64
    AND lower(completion_digest_sha256) = completion_digest_sha256
    AND completion_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(authorization_digest_sha256) = 64
    AND lower(authorization_digest_sha256) = authorization_digest_sha256
    AND authorization_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(scope_set_digest_sha256) = 64
    AND lower(scope_set_digest_sha256) = scope_set_digest_sha256
    AND scope_set_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_authorization_scopes (
  account_id                       TEXT NOT NULL,
  account_generation              INTEGER NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  manifest_digest_sha256           TEXT NOT NULL,
  object_count                     INTEGER NOT NULL
    CHECK(object_count BETWEEN 0 AND 9007199254740991),
  prefix_sweep                     INTEGER NOT NULL CHECK(prefix_sweep = 1),
  created_at_ms                    INTEGER NOT NULL,
  PRIMARY KEY(account_id, account_generation, sweep_attempt_id, scope_id),
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorizations(
      account_id, account_generation, sweep_attempt_id
    ) ON DELETE CASCADE,
  CHECK(length(scope_id) BETWEEN 20 AND 128
    AND scope_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(manifest_digest_sha256) = 64
    AND lower(manifest_digest_sha256) = manifest_digest_sha256
    AND manifest_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_manifest_objects (
  account_id                       TEXT NOT NULL,
  account_generation              INTEGER NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  object_key_hmac_sha256           TEXT NOT NULL,
  created_at_ms                    INTEGER NOT NULL,
  PRIMARY KEY(account_id, account_generation, sweep_attempt_id, scope_id,
    object_key_hmac_sha256),
  FOREIGN KEY(account_id, account_generation, sweep_attempt_id, scope_id)
    REFERENCES jobs_workflow_cleanup_object_sweep_authorization_scopes(
      account_id, account_generation, sweep_attempt_id, scope_id
    ) ON DELETE CASCADE,
  CHECK(length(object_key_hmac_sha256) = 64
    AND lower(object_key_hmac_sha256) = object_key_hmac_sha256
    AND object_key_hmac_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_progress (
  account_id                       TEXT NOT NULL,
  account_generation              INTEGER NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  object_key_hmac_sha256           TEXT NOT NULL,
  deleted_at_ms                    INTEGER NOT NULL,
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
  CHECK(length(authorization_digest_sha256) = 64
    AND lower(authorization_digest_sha256) = authorization_digest_sha256
    AND authorization_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(scope_id) BETWEEN 20 AND 128
    AND scope_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(object_key_hmac_sha256) = 64
    AND lower(object_key_hmac_sha256) = object_key_hmac_sha256
    AND object_key_hmac_sha256 NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_object_sweep_scopes (
  account_id                       TEXT NOT NULL,
  account_generation              INTEGER NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  scope_id                         TEXT NOT NULL,
  deleted_count                    INTEGER NOT NULL CHECK(deleted_count >= 0),
  orphan_count                     INTEGER NOT NULL CHECK(orphan_count >= 0),
  result_digest_sha256             TEXT NOT NULL,
  recorded_at_ms                   INTEGER NOT NULL,
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
  CHECK(length(sweep_attempt_id) BETWEEN 20 AND 128
    AND sweep_attempt_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(authorization_digest_sha256) = 64
    AND lower(authorization_digest_sha256) = authorization_digest_sha256
    AND authorization_digest_sha256 NOT GLOB '*[^0-9a-f]*'),
  CHECK(length(scope_id) BETWEEN 20 AND 128
    AND scope_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(result_digest_sha256) = 64
    AND lower(result_digest_sha256) = result_digest_sha256
    AND result_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

-- This row exists only inside the exact account DELETE transaction. The
-- account BEFORE trigger creates it after rechecking both cleanup authorities;
-- the caller removes it after DELETE returns and all cascades have completed.
CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_hard_delete_cascade_tokens (
  account_id                       TEXT PRIMARY KEY,
  account_generation              INTEGER NOT NULL,
  sweep_attempt_id                 TEXT NOT NULL,
  authorization_digest_sha256      TEXT NOT NULL,
  created_at_ms                    INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id)
    ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
  CHECK(length(sweep_attempt_id) BETWEEN 20 AND 128
    AND sweep_attempt_id NOT GLOB '*[^A-Za-z0-9_-]*'),
  CHECK(length(authorization_digest_sha256) = 64
    AND lower(authorization_digest_sha256) = authorization_digest_sha256
    AND authorization_digest_sha256 NOT GLOB '*[^0-9a-f]*')
);

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
   AND authority.positive_reset_required = 0
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
      AND (SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_run_observations observation
            WHERE observation.account_id = authority.account_id
              AND observation.workflow_cleanup_generation =
                  authority.workflow_cleanup_generation
              AND observation.workflow_id = authority.workflow_id
              AND observation.known_run_epoch = authority.known_run_epoch
              AND observation.subject_kind = 'workflow'
              AND observation.run_id_hmac_sha256 =
                  '0000000000000000000000000000000000000000000000000000000000000000'
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
           AND first.run_id_hmac_sha256 =
               '0000000000000000000000000000000000000000000000000000000000000000'
           AND first.target_digest_sha256 = authority.target_digest_sha256
           AND target.absence_proved_at_ms >= second.recorded_at_ms
      ))
   );

DROP VIEW IF EXISTS jobs_workflow_cleanup_ready_object_sweeps;
CREATE VIEW jobs_workflow_cleanup_ready_object_sweeps AS
SELECT authorization.account_id, authorization.account_generation,
       authorization.sweep_attempt_id,
       authorization.authorization_digest_sha256,
       authorization.completion_tombstone_id,
       authorization.completion_digest_sha256
  FROM jobs_workflow_cleanup_object_sweep_authorizations authorization
  JOIN jobs_workflow_cleanup_account_bindings binding
    ON binding.account_id = authorization.account_id
   AND binding.account_generation = authorization.account_generation
   AND binding.completion_tombstone_id = authorization.completion_tombstone_id
   AND binding.completion_digest_sha256 = authorization.completion_digest_sha256
  JOIN jobs_runner_purge_requests request
    ON request.request_id = authorization.runner_purge_request_id
   AND request.account_id = authorization.account_id
   AND request.purge_generation = authorization.runner_purge_generation
   AND request.legacy_inventory_generation =
       authorization.runner_legacy_inventory_generation
   AND request.legacy_inventory_reconciliation_id =
       authorization.runner_legacy_reconciliation_id
   AND request.legacy_inventory_authority_id = authorization.runner_legacy_authority_id
   AND request.legacy_inventory_authority_sha256 =
       authorization.runner_legacy_authority_sha256
   AND request.state = 'complete' AND request.legacy_unresolved_count = 0
   AND request.resolved_target_count = request.required_target_count
  JOIN jobs_runner_purge_tombstones runner_tombstone
    ON runner_tombstone.request_id = request.request_id
   AND runner_tombstone.purge_generation = request.purge_generation
   AND runner_tombstone.tombstone_generation =
       authorization.runner_purge_tombstone_generation
   AND runner_tombstone.purge_subject = request.purge_subject
   AND runner_tombstone.target_set_sha256 = request.target_set_sha256
   AND runner_tombstone.required_target_count = request.required_target_count
   AND runner_tombstone.completed_at_ms = request.completed_at_ms
  JOIN jobs_runner_volume_fleet_state fleet
    ON fleet.singleton_id = 1 AND fleet.legacy_inventory_state = 'ready'
   AND fleet.legacy_inventory_generation = authorization.runner_legacy_inventory_generation
   AND fleet.legacy_inventory_reconciliation_id =
       authorization.runner_legacy_reconciliation_id
   AND fleet.legacy_inventory_authority_id = authorization.runner_legacy_authority_id
   AND fleet.legacy_inventory_authority_sha256 =
       authorization.runner_legacy_authority_sha256
 WHERE authorization.sealed = 1 AND binding.state = 'complete'
   AND authorization.scope_count = (
     SELECT COUNT(*) FROM jobs_workflow_cleanup_object_sweep_authorization_scopes scope
      WHERE scope.account_id = authorization.account_id
        AND scope.account_generation = authorization.account_generation
        AND scope.sweep_attempt_id = authorization.sweep_attempt_id
        AND scope.prefix_sweep = 1
   )
   AND authorization.known_object_count = (
     SELECT COUNT(*) FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
      WHERE object.account_id = authorization.account_id
        AND object.account_generation = authorization.account_generation
        AND object.sweep_attempt_id = authorization.sweep_attempt_id
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
      WHERE object.account_id = authorization.account_id
        AND object.account_generation = authorization.account_generation
        AND object.sweep_attempt_id = authorization.sweep_attempt_id
        AND NOT EXISTS (
          SELECT 1 FROM jobs_workflow_cleanup_object_sweep_progress progress
           WHERE progress.account_id = object.account_id
             AND progress.account_generation = object.account_generation
             AND progress.scope_id = object.scope_id
             AND progress.object_key_hmac_sha256 = object.object_key_hmac_sha256
        )
   )
   AND authorization.scope_count = (
     SELECT COUNT(*) FROM jobs_workflow_cleanup_object_sweep_scopes result
      WHERE result.account_id = authorization.account_id
        AND result.account_generation = authorization.account_generation
        AND result.sweep_attempt_id = authorization.sweep_attempt_id
        AND result.authorization_digest_sha256 =
            authorization.authorization_digest_sha256
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
       CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
   AND generation.target_count = (
     SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
      WHERE target.account_id = generation.account_id
        AND target.generation = generation.generation
        AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256
   )
   AND generation.target_count = (
     SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities authority
      WHERE authority.account_id = generation.account_id
        AND authority.workflow_cleanup_generation = generation.generation
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
      WHERE authority.account_id = generation.account_id
        AND authority.workflow_cleanup_generation = generation.generation
        AND authority.positive_reset_required = 1
   )
   AND generation.target_count = (
     SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets proved
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
          OR target.positive_reset_required = 1)
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
      WHERE page.generation = legacy.generation
        AND page.raw_ciphertexts_scrubbed <> 1
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_targets target
      WHERE target.generation = legacy.generation
        AND target.raw_ids_scrubbed <> 1
   );

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_generation_identity_immutable
BEFORE UPDATE OF generation, inventory_generation_id, namespace_ciphertext,
  namespace_hmac_sha256, workflow_type, visibility_cutoff_ms, confirmation_age_ms,
  visibility_query_ciphertext, query_digest_sha256,
  query_hmac_sha256, created_at_ms
ON jobs_workflow_legacy_inventory_generations
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy inventory identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_generation_epoch_guard
BEFORE UPDATE ON jobs_workflow_legacy_inventory_generations
WHEN
  (NEW.completion_epoch = OLD.completion_epoch
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
  ))
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy completion epoch must advance and reopen cleanly');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_head_monotonic
BEFORE UPDATE ON jobs_workflow_legacy_inventory_head
WHEN NEW.generation < OLD.generation
  OR (NEW.generation = OLD.generation
    AND (NEW.inventory_generation_id <> OLD.inventory_generation_id
      OR NEW.query_digest_sha256 <> OLD.query_digest_sha256))
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy inventory head is not monotonic');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_head_delete_guard
BEFORE DELETE ON jobs_workflow_legacy_inventory_head
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy inventory head is permanent');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_generation_delete_guard
BEFORE DELETE ON jobs_workflow_legacy_inventory_generations
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy inventory generation is permanent');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_page_immutable
BEFORE UPDATE OF generation, completion_epoch, scan_pass, page_index,
  request_epoch, fence, request_id, predecessor_page_digest_sha256,
  input_page_token_hmac_sha256, next_page_token_hmac_sha256,
  page_target_count, page_targets_digest_sha256, page_digest_sha256,
  evidence_digest_sha256, recorded_at_ms
ON jobs_workflow_legacy_inventory_pages
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy inventory page is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_page_insert_guard
BEFORE INSERT ON jobs_workflow_legacy_inventory_pages
WHEN NOT EXISTS (
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
     AND generation.page_token_hmac_sha256 IS NEW.input_page_token_hmac_sha256
     AND generation.first_request_started_at_ms IS NOT NULL
     AND generation.lease_owner IS NOT NULL
     AND generation.lease_token_sha256 IS NOT NULL
     AND generation.lease_expires_at_ms >= NEW.recorded_at_ms
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy page is not current request authority');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_page_raw_scrub_guard
BEFORE UPDATE OF input_page_token_ciphertext, next_page_token_ciphertext,
  raw_ciphertexts_scrubbed
ON jobs_workflow_legacy_inventory_pages
WHEN NOT (
  OLD.raw_ciphertexts_scrubbed = 0 AND NEW.raw_ciphertexts_scrubbed = 1
  AND NEW.input_page_token_ciphertext IS NULL
  AND NEW.next_page_token_ciphertext IS NULL
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy page raw ciphertext can only be scrubbed');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_identity_immutable
BEFORE UPDATE OF generation, target_identity_hmac_sha256,
  workflow_id_hmac_sha256, run_id_hmac_sha256,
  first_execution_run_id_hmac_sha256,
  target_digest_sha256, discovered_completion_epoch, discovered_scan_pass,
  discovered_page_index, discovered_page_digest_sha256, created_at_ms
ON jobs_workflow_legacy_targets
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy target identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_insert_guard
BEFORE INSERT ON jobs_workflow_legacy_targets
WHEN NEW.raw_ids_scrubbed <> 0 OR NOT EXISTS (
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy target is not current page authority');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_raw_identity_guard
BEFORE UPDATE OF workflow_id_ciphertext, run_id_ciphertext,
  first_execution_run_id_ciphertext, raw_ids_scrubbed
ON jobs_workflow_legacy_targets
WHEN NOT (
  (OLD.raw_ids_scrubbed = 0 AND NEW.raw_ids_scrubbed = 1
    AND OLD.target_state = 'absence_proved'
    AND NEW.workflow_id_ciphertext IS NULL AND NEW.run_id_ciphertext IS NULL
    AND NEW.first_execution_run_id_ciphertext IS NULL)
  OR (OLD.raw_ids_scrubbed = 1 AND NEW.raw_ids_scrubbed = 0
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy target raw identity transition is invalid');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_proof_epoch_guard
BEFORE UPDATE OF proof_epoch ON jobs_workflow_legacy_targets
WHEN NEW.proof_epoch <> OLD.proof_epoch + 1
  OR NEW.positive_reset_required <> 0
  OR NEW.raw_ids_scrubbed <> 0
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
  )
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy proof epoch must reopen cleanly');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_terminal_guard
BEFORE UPDATE OF target_state, proof_epoch, observation_pass,
  first_absence_observed_at_ms, absence_proved_at_ms, positive_reset_required
ON jobs_workflow_legacy_targets
WHEN (OLD.target_state = 'absence_proved'
    AND NEW.proof_epoch = OLD.proof_epoch
    AND (NEW.target_state <> OLD.target_state
      OR NEW.observation_pass <> OLD.observation_pass
      OR NEW.first_absence_observed_at_ms IS NOT OLD.first_absence_observed_at_ms
      OR NEW.absence_proved_at_ms IS NOT OLD.absence_proved_at_ms
      OR NEW.positive_reset_required <> OLD.positive_reset_required))
  OR (OLD.target_state = 'identity_conflict'
    AND (NEW.target_state <> 'identity_conflict'
      OR NEW.proof_epoch <> OLD.proof_epoch
      OR NEW.observation_pass <> OLD.observation_pass
      OR NEW.first_absence_observed_at_ms IS NOT OLD.first_absence_observed_at_ms
      OR NEW.absence_proved_at_ms IS NOT OLD.absence_proved_at_ms
      OR NEW.positive_reset_required <> OLD.positive_reset_required))
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy target terminal state requires a new proof epoch');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_positive_reset_clear_guard
BEFORE UPDATE OF positive_reset_required ON jobs_workflow_legacy_targets
WHEN OLD.positive_reset_required = 1 AND NEW.positive_reset_required = 0
  AND NEW.proof_epoch <> OLD.proof_epoch + 1
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy positive evidence requires a new proof epoch');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_observation_immutable
BEFORE UPDATE ON jobs_workflow_legacy_target_observations
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy target observation is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_observation_current_lease
BEFORE INSERT ON jobs_workflow_legacy_target_observations
WHEN NOT EXISTS (
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy observation lease is not current');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_observation_positive_reset
AFTER INSERT ON jobs_workflow_legacy_target_observations
WHEN NEW.describe_state = 'found' OR NEW.history_state = 'found'
  OR NEW.visibility_state = 'found'
BEGIN
  UPDATE jobs_workflow_legacy_targets
     SET positive_reset_required = 1,
         updated_at_ms = MAX(NEW.recorded_at_ms, updated_at_ms + 1)
   WHERE generation = NEW.generation
     AND target_identity_hmac_sha256 = NEW.target_identity_hmac_sha256
     AND proof_epoch = NEW.proof_epoch;
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_absence_guard
BEFORE UPDATE OF target_state, observation_pass, first_absence_observed_at_ms,
  absence_proved_at_ms
ON jobs_workflow_legacy_targets
WHEN NEW.target_state = 'absence_proved' AND OLD.target_state <> 'absence_proved' AND (
  NEW.positive_reset_required <> 0
  OR NEW.observation_pass <> 2 OR NEW.first_absence_observed_at_ms IS NULL
  OR NEW.absence_proved_at_ms IS NULL
  OR (SELECT COUNT(*) FROM jobs_workflow_legacy_target_observations observation
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy target lacks two exact absence passes');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_zero_immutable
BEFORE UPDATE ON jobs_workflow_legacy_zero_observations
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy zero observation is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_zero_exact_page
BEFORE INSERT ON jobs_workflow_legacy_zero_observations
WHEN NOT EXISTS (
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy zero is not bound to an exhausted empty scan');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_authority_immutable_identity
BEFORE UPDATE OF account_id, workflow_cleanup_generation, workflow_id, created_at_ms
ON jobs_workflow_cleanup_v2_target_authorities
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 cleanup authority identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_proof_epoch_guard
BEFORE UPDATE OF known_run_epoch ON jobs_workflow_cleanup_v2_target_authorities
WHEN NEW.known_run_epoch <> OLD.known_run_epoch + 1
  OR NEW.positive_reset_required <> 0
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
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 proof epoch must reopen cleanly');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_pass_transition_guard
BEFORE UPDATE OF known_run_set_digest_sha256, target_digest_sha256,
  observation_pass, first_absence_observed_at_ms
ON jobs_workflow_cleanup_v2_target_authorities
WHEN NEW.known_run_epoch = OLD.known_run_epoch
  AND (NEW.known_run_set_digest_sha256 <> OLD.known_run_set_digest_sha256
    OR NEW.target_digest_sha256 <> OLD.target_digest_sha256
    OR NEW.observation_pass <> OLD.observation_pass
    OR NEW.first_absence_observed_at_ms IS NOT OLD.first_absence_observed_at_ms)
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
  )
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 proof tuple must advance from immutable evidence');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_positive_reset_clear_guard
BEFORE UPDATE OF positive_reset_required ON jobs_workflow_cleanup_v2_target_authorities
WHEN OLD.positive_reset_required = 1 AND NEW.positive_reset_required = 0
  AND NEW.known_run_epoch <> OLD.known_run_epoch + 1
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 positive evidence requires a new proof epoch');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_terminal_positive_reset_guard
BEFORE UPDATE OF positive_reset_required ON jobs_workflow_cleanup_v2_target_authorities
WHEN NEW.positive_reset_required <> OLD.positive_reset_required AND EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_targets target
   WHERE target.account_id = NEW.account_id
     AND target.generation = NEW.workflow_cleanup_generation
     AND target.workflow_id = NEW.workflow_id
     AND target.target_state IN ('absence_proved', 'identity_conflict')
)
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 terminal positive-reset authority is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_known_run_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_v2_known_runs
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 known run is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_known_run_bound
BEFORE INSERT ON jobs_workflow_cleanup_v2_known_runs
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_targets target
  JOIN jobs_workflow_cleanup_v2_target_authorities authority
    ON authority.account_id = target.account_id
   AND authority.workflow_cleanup_generation = target.generation
   AND authority.workflow_id = target.workflow_id
 WHERE target.account_id = NEW.account_id
   AND target.generation = NEW.workflow_cleanup_generation
   AND target.workflow_id = NEW.workflow_id
   AND target.target_state NOT IN ('absence_proved', 'identity_conflict')
) OR (SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_known_runs run
       WHERE run.account_id = NEW.account_id
         AND run.workflow_cleanup_generation = NEW.workflow_cleanup_generation
         AND run.workflow_id = NEW.workflow_id) >= 32
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 known run set exceeds 32');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_observation_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_v2_run_observations
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 run observation is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_receipt_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_v2_receipts
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 cleanup receipt is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_receipt_current_lease
BEFORE INSERT ON jobs_workflow_cleanup_v2_receipts
WHEN NOT EXISTS (
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 receipt lease is not current');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_receipt_positive_reset
AFTER INSERT ON jobs_workflow_cleanup_v2_receipts
WHEN NEW.outcome = 'pending' AND NEW.reason <> 'temporal_unavailable'
BEGIN
  UPDATE jobs_workflow_cleanup_v2_target_authorities
     SET positive_reset_required = 1,
         updated_at_ms = MAX(NEW.recorded_at_ms, updated_at_ms + 1)
   WHERE account_id = NEW.account_id
     AND workflow_cleanup_generation = NEW.workflow_cleanup_generation
     AND workflow_id = NEW.workflow_id
     AND known_run_epoch = NEW.known_run_epoch;
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_observation_current_lease
BEFORE INSERT ON jobs_workflow_cleanup_v2_run_observations
WHEN NOT EXISTS (
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 observation lease or known-run epoch is not current');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_target_proved_immutable
BEFORE UPDATE OF target_state, absence_proved_at_ms, first_execution_run_id
ON jobs_workflow_cleanup_targets
WHEN OLD.target_state = 'absence_proved' AND (
  NEW.target_state <> OLD.target_state
  OR NEW.absence_proved_at_ms IS NOT OLD.absence_proved_at_ms
  OR NEW.first_execution_run_id IS NOT OLD.first_execution_run_id
)
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 proved target is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_target_absence_guard
BEFORE UPDATE OF target_state, absence_proved_at_ms ON jobs_workflow_cleanup_targets
WHEN NEW.target_state = 'absence_proved' AND OLD.target_state <> 'absence_proved' AND NOT EXISTS (
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
     AND authority.positive_reset_required = 0
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
             AND first.run_id_hmac_sha256 =
                 '0000000000000000000000000000000000000000000000000000000000000000'
             AND first.target_digest_sha256 = authority.target_digest_sha256
             AND NEW.absence_proved_at_ms >= second.recorded_at_ms
        ))
     )
)
BEGIN
  SELECT RAISE(ABORT, 'workflow v2 target lacks per-run two-pass absence proof');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_authorization_insert_guard
BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_authorizations
WHEN NEW.sealed <> 0
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep authorization must start unsealed');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_authorization_seal_guard
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_authorizations
WHEN OLD.sealed <> 0 OR NEW.sealed <> 1
  OR NEW.account_id <> OLD.account_id
  OR NEW.account_generation <> OLD.account_generation
  OR NEW.sweep_attempt_id <> OLD.sweep_attempt_id
  OR NEW.runner_purge_request_id <> OLD.runner_purge_request_id
  OR NEW.runner_purge_generation <> OLD.runner_purge_generation
  OR NEW.runner_purge_tombstone_generation <> OLD.runner_purge_tombstone_generation
  OR NEW.runner_legacy_inventory_generation <> OLD.runner_legacy_inventory_generation
  OR NEW.runner_legacy_reconciliation_id <> OLD.runner_legacy_reconciliation_id
  OR NEW.runner_legacy_authority_id <> OLD.runner_legacy_authority_id
  OR NEW.runner_legacy_authority_sha256 <> OLD.runner_legacy_authority_sha256
  OR NEW.runner_authority_digest_sha256 <> OLD.runner_authority_digest_sha256
  OR NEW.completion_tombstone_id <> OLD.completion_tombstone_id
  OR NEW.completion_digest_sha256 <> OLD.completion_digest_sha256
  OR NEW.authorization_digest_sha256 <> OLD.authorization_digest_sha256
  OR NEW.scope_set_digest_sha256 <> OLD.scope_set_digest_sha256
  OR NEW.scope_count <> OLD.scope_count
  OR NEW.known_object_count <> OLD.known_object_count
  OR NEW.authorized_at_ms <> OLD.authorized_at_ms
  OR NEW.scope_count <> (
    SELECT COUNT(*)
      FROM jobs_workflow_cleanup_object_sweep_authorization_scopes scope
     WHERE scope.account_id = NEW.account_id
       AND scope.account_generation = NEW.account_generation
       AND scope.sweep_attempt_id = NEW.sweep_attempt_id
  )
  OR NEW.known_object_count <> (
    SELECT COUNT(*)
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
         SELECT COUNT(*)
           FROM jobs_workflow_cleanup_object_sweep_manifest_objects object
          WHERE object.account_id = scope.account_id
            AND object.account_generation = scope.account_generation
            AND object.sweep_attempt_id = scope.sweep_attempt_id
            AND object.scope_id = scope.scope_id
       )
  )
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep authorization is immutable or incomplete');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_scope_manifest_insert_guard
BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_authorization_scopes
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_object_sweep_authorizations authorization
   WHERE authorization.account_id = NEW.account_id
     AND authorization.account_generation = NEW.account_generation
     AND authorization.sweep_attempt_id = NEW.sweep_attempt_id
     AND authorization.sealed = 0
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep authorization is sealed');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_manifest_object_insert_guard
BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_manifest_objects
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_object_sweep_authorizations authorization
   WHERE authorization.account_id = NEW.account_id
     AND authorization.account_generation = NEW.account_generation
     AND authorization.sweep_attempt_id = NEW.sweep_attempt_id
     AND authorization.sealed = 0
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep authorization is sealed');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_scope_manifest_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_authorization_scopes
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep manifest is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_manifest_object_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_manifest_objects
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep manifest is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_scope_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_scopes
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep scope is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_tombstone_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_completion_tombstones
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup completion tombstone is immutable');
END;

-- Global inventory evidence is permanent. Account-scoped evidence is
-- append-only; only the exact hard-delete cascade token, after the parent
-- account is already absent, permits the final account cascade.
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_page_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_inventory_pages
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup evidence cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_target_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_targets
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup evidence cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_observation_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_target_observations
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup evidence cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_zero_delete_immutable
BEFORE DELETE ON jobs_workflow_legacy_zero_observations
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup evidence cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_tombstone_delete_immutable
BEFORE DELETE ON jobs_workflow_cleanup_completion_tombstones
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup completion tombstone cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_command_cleanup_delete_guard
BEFORE DELETE ON jobs_workflow_commands
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_targets target
   WHERE target.start_command_id = OLD.id AND target.account_id = OLD.account_id
) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup target cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_target_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_targets
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup target cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_observation_delete_guard
BEFORE DELETE ON jobs_workflow_execution_cleanup_observations
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup evidence cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_authority_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_target_authorities
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup authority cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_known_run_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_known_runs
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup known run cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_run_observation_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_run_observations
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup evidence cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_v2_receipt_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_v2_receipts
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup receipt cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_progress_immutable
BEFORE UPDATE ON jobs_workflow_cleanup_object_sweep_progress
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep progress is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_progress_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_progress
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND token.account_generation = OLD.account_generation
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep progress cannot be deleted before completion');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_scope_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_scopes
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND token.account_generation = OLD.account_generation
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep scope cannot be deleted before completion');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_authorization_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_authorizations
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND token.account_generation = OLD.account_generation
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep authorization cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_manifest_scope_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_authorization_scopes
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND token.account_generation = OLD.account_generation
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep manifest cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_sweep_manifest_object_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_object_sweep_manifest_objects
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND token.account_generation = OLD.account_generation
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup sweep manifest cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_binding_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_account_bindings
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND token.account_generation = OLD.account_generation
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup binding cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_generation_delete_guard
BEFORE DELETE ON jobs_workflow_cleanup_generations
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
   WHERE binding.account_id = OLD.account_id
     AND binding.workflow_cleanup_generation = OLD.generation
) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup generation cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_complete_guard
BEFORE UPDATE OF state, completed_at_ms, completion_digest_sha256, revalidate_after_ms
ON jobs_workflow_legacy_inventory_generations
WHEN NEW.state = 'complete' AND OLD.state <> 'complete' AND (
  NEW.scan_pass <> 2 OR NEW.completed_at_ms IS NULL
  OR NEW.page_token_ciphertext IS NOT NULL OR NEW.page_token_hmac_sha256 IS NOT NULL
  OR NEW.revalidate_after_ms IS NULL OR NEW.revalidate_after_ms <= NEW.completed_at_ms
  OR NEW.completion_digest_sha256 IS NULL
  OR EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_targets target
     WHERE target.generation = NEW.generation
       AND (target.target_state <> 'absence_proved'
         OR target.positive_reset_required = 1)
  )
  OR EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
     WHERE page.generation = NEW.generation
       AND page.raw_ciphertexts_scrubbed <> 1
  )
  OR EXISTS (
    SELECT 1 FROM jobs_workflow_legacy_targets target
     WHERE target.generation = NEW.generation
       AND target.raw_ids_scrubbed <> 1
  )
  OR (SELECT COUNT(*) FROM jobs_workflow_legacy_zero_observations zero
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy inventory is not exactly zero');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_legacy_complete_immutable
BEFORE UPDATE ON jobs_workflow_legacy_inventory_generations
WHEN OLD.state = 'complete' AND NEW.state = 'complete' AND (
  NEW.scan_pass <> OLD.scan_pass OR NEW.page_index <> OLD.page_index
  OR NEW.predecessor_page_digest_sha256 <> OLD.predecessor_page_digest_sha256
  OR NEW.page_token_ciphertext IS NOT OLD.page_token_ciphertext
  OR NEW.page_token_hmac_sha256 IS NOT OLD.page_token_hmac_sha256
  OR NEW.request_epoch <> OLD.request_epoch OR NEW.fence <> OLD.fence
  OR NEW.request_id IS NOT OLD.request_id
  OR NEW.first_request_started_at_ms IS NOT OLD.first_request_started_at_ms
  OR NEW.last_outcome_code IS NOT OLD.last_outcome_code
  OR NEW.lease_owner IS NOT OLD.lease_owner
  OR NEW.lease_token_sha256 IS NOT OLD.lease_token_sha256
  OR NEW.lease_expires_at_ms IS NOT OLD.lease_expires_at_ms
  OR NEW.next_attempt_at_ms <> OLD.next_attempt_at_ms
  OR NEW.first_zero_observed_at_ms IS NOT OLD.first_zero_observed_at_ms
  OR NEW.completed_at_ms IS NOT OLD.completed_at_ms
  OR NEW.completion_digest_sha256 IS NOT OLD.completion_digest_sha256
  OR NEW.revalidate_after_ms IS NOT OLD.revalidate_after_ms
  OR NEW.completion_epoch <> OLD.completion_epoch
  OR NEW.updated_at_ms <> OLD.updated_at_ms
)
BEGIN
  SELECT RAISE(ABORT, 'workflow legacy completion is immutable until revalidation');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_binding_monotonic
BEFORE UPDATE ON jobs_workflow_cleanup_account_bindings
WHEN NEW.account_id <> OLD.account_id
  OR NEW.account_generation <> OLD.account_generation
  OR NEW.workflow_cleanup_generation <> OLD.workflow_cleanup_generation
  OR NEW.cleanup_generation_id <> OLD.cleanup_generation_id
  OR NEW.target_set_hmac_sha256 <> OLD.target_set_hmac_sha256
  OR NEW.legacy_generation <> OLD.legacy_generation
  OR NEW.legacy_inventory_generation_id <> OLD.legacy_inventory_generation_id
  OR NEW.legacy_query_digest_sha256 <> OLD.legacy_query_digest_sha256
  OR NEW.created_at_ms <> OLD.created_at_ms
  OR NEW.updated_at_ms < OLD.updated_at_ms
  OR (OLD.state = 'draining' AND NEW.state = 'frozen')
  OR (OLD.state = 'complete' AND NEW.state = 'frozen')
  OR (OLD.state = 'complete' AND NEW.state = 'draining' AND (
    NEW.completion_tombstone_id IS NOT NULL
    OR NEW.completion_digest_sha256 IS NOT NULL
    OR NEW.hard_delete_authorized_at_ms IS NOT NULL
    OR NEW.hard_delete_sweep_attempt_id IS NOT NULL
    OR NEW.hard_delete_authorization_digest_sha256 IS NOT NULL
  ))
  OR (OLD.object_sweep_started_at_ms IS NOT NULL
    AND NEW.object_sweep_started_at_ms IS NOT OLD.object_sweep_started_at_ms)
  OR (OLD.object_sweep_started_at_ms IS NULL
    AND NEW.object_sweep_started_at_ms IS NOT NULL
    AND NOT EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_object_sweep_authorizations authorization
       WHERE authorization.account_id = NEW.account_id
         AND authorization.account_generation = NEW.account_generation
         AND authorization.sealed = 1
         AND authorization.authorized_at_ms = NEW.object_sweep_started_at_ms
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
      NEW.state <> 'complete'
      OR NOT EXISTS (
        SELECT 1 FROM jobs_workflow_cleanup_ready_object_sweeps sweep
         WHERE sweep.account_id = NEW.account_id
           AND sweep.account_generation = NEW.account_generation
           AND sweep.sweep_attempt_id = NEW.hard_delete_sweep_attempt_id
           AND sweep.completion_tombstone_id = NEW.completion_tombstone_id
           AND sweep.completion_digest_sha256 = NEW.completion_digest_sha256
      )
    ))
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup account binding is not monotonic');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_workflow_cleanup_binding_complete_guard
BEFORE UPDATE OF state, completion_tombstone_id, completion_digest_sha256
ON jobs_workflow_cleanup_account_bindings
WHEN NEW.state = 'complete' AND (
  OLD.state <> 'complete'
  OR NEW.completion_tombstone_id IS NOT OLD.completion_tombstone_id
  OR NEW.completion_digest_sha256 IS NOT OLD.completion_digest_sha256
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
         SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
          WHERE target.account_id = generation.account_id
            AND target.generation = generation.generation
            AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256
       )
       AND generation.target_count = (
         SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities authority
          WHERE authority.account_id = generation.account_id
            AND authority.workflow_cleanup_generation = generation.generation
       )
       AND NOT EXISTS (
         SELECT 1 FROM jobs_workflow_cleanup_v2_target_authorities authority
          WHERE authority.account_id = generation.account_id
            AND authority.workflow_cleanup_generation = generation.generation
            AND authority.positive_reset_required = 1
       )
       AND generation.target_count = (
         SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets proved
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
         CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
     AND NOT EXISTS (
       SELECT 1 FROM jobs_workflow_legacy_targets target
        WHERE target.generation = NEW.legacy_generation
          AND (target.target_state <> 'absence_proved'
            OR target.positive_reset_required = 1)
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
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup account binding is not exactly complete');
END;

DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_guard;
CREATE TRIGGER trg_jobs_workflow_account_delete_guard
BEFORE DELETE ON accounts
WHEN (
  EXISTS (SELECT 1 FROM account_deletion_intents deletion WHERE deletion.account_id = OLD.id)
  OR EXISTS (SELECT 1 FROM jobs_workflow_commands command WHERE command.account_id = OLD.id)
  OR EXISTS (SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
              WHERE binding.account_id = OLD.id)
)
 AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
  JOIN jobs_workflow_cleanup_completion_tombstones tombstone
    ON tombstone.tombstone_id = binding.completion_tombstone_id
   AND tombstone.completion_digest_sha256 = binding.completion_digest_sha256
  JOIN jobs_workflow_legacy_inventory_head head ON head.singleton_id = 1
  JOIN jobs_workflow_legacy_inventory_generations legacy
    ON legacy.generation = head.generation
   AND legacy.query_digest_sha256 = head.query_digest_sha256
   AND legacy.generation = binding.legacy_generation
   AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
   AND legacy.completion_epoch = tombstone.legacy_completion_epoch
   AND legacy.completion_digest_sha256 = tombstone.legacy_completion_digest_sha256
   AND legacy.revalidate_after_ms = tombstone.legacy_revalidate_after_ms
 WHERE binding.account_id = OLD.id AND binding.state = 'complete'
   AND binding.hard_delete_authorized_at_ms IS NOT NULL
   AND tombstone.cleanup_generation_id = binding.cleanup_generation_id
   AND tombstone.legacy_inventory_generation_id = binding.legacy_inventory_generation_id
   AND binding.account_generation = (
     SELECT requested_at_ms FROM account_deletion_intents WHERE account_id = OLD.id
   )
   AND legacy.state = 'complete'
   AND legacy.revalidate_after_ms >
       CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
   AND EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_generations generation
      WHERE generation.account_id = binding.account_id
        AND generation.generation = binding.workflow_cleanup_generation
        AND generation.target_set_hmac_sha256 = binding.target_set_hmac_sha256
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
           WHERE target.account_id = generation.account_id
             AND target.generation = generation.generation
             AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256)
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities authority
           WHERE authority.account_id = generation.account_id
             AND authority.workflow_cleanup_generation = generation.generation)
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets proved
           WHERE proved.account_id = generation.account_id
             AND proved.generation = generation.generation
             AND proved.target_set_hmac_sha256 = generation.target_set_hmac_sha256)
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_targets target
      WHERE target.account_id = binding.account_id
        AND target.generation = binding.workflow_cleanup_generation
        AND target.target_set_hmac_sha256 = binding.target_set_hmac_sha256
        AND target.target_state <> 'absence_proved'
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_targets target
      WHERE target.generation = binding.legacy_generation
        AND (target.target_state <> 'absence_proved'
          OR target.positive_reset_required = 1)
   )
 )
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup completion is required');
END;

DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_exact_sweep_guard;
CREATE TRIGGER trg_jobs_workflow_account_delete_exact_sweep_guard
BEFORE DELETE ON accounts
WHEN (
  EXISTS (SELECT 1 FROM account_deletion_intents deletion WHERE deletion.account_id = OLD.id)
  OR EXISTS (SELECT 1 FROM jobs_workflow_commands command WHERE command.account_id = OLD.id)
) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_ready ready
   WHERE ready.account_id = OLD.id
     AND ready.account_generation = (
       SELECT requested_at_ms FROM account_deletion_intents WHERE account_id = OLD.id
     )
)
BEGIN
  SELECT RAISE(ABORT, 'exact workflow and runner sweep completion is required');
END;

DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_cascade_token;
CREATE TRIGGER trg_jobs_workflow_account_delete_cascade_token
BEFORE DELETE ON accounts
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_ready ready
   WHERE ready.account_id = OLD.id
)
BEGIN
  INSERT OR REPLACE INTO jobs_workflow_cleanup_hard_delete_cascade_tokens (
    account_id, account_generation, sweep_attempt_id,
    authorization_digest_sha256, created_at_ms
  )
  SELECT binding.account_id, binding.account_generation,
         binding.hard_delete_sweep_attempt_id,
         binding.hard_delete_authorization_digest_sha256,
         CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
    FROM jobs_workflow_cleanup_account_bindings binding
   WHERE binding.account_id = OLD.id;
END;

-- A previous revision briefly installed an AFTER DELETE cleanup trigger. It
-- is deliberately absent: FK cascade trigger ordering is not a portable
-- lifetime boundary. The hard-delete transaction removes the token only
-- after DELETE FROM accounts returns and every child cascade has completed.
DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_cascade_token_cleanup;

DROP TRIGGER IF EXISTS trg_jobs_workflow_application_delete_guard;
CREATE TRIGGER trg_jobs_workflow_application_delete_guard
BEFORE DELETE ON jobs_applications
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_commands command
   WHERE command.account_id = OLD.account_id AND command.application_id = OLD.id
 ) AND NOT EXISTS (
   SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
    WHERE token.account_id = OLD.account_id
      AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
 ) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
  JOIN jobs_workflow_cleanup_completion_tombstones tombstone
    ON tombstone.tombstone_id = binding.completion_tombstone_id
   AND tombstone.completion_digest_sha256 = binding.completion_digest_sha256
  JOIN jobs_workflow_legacy_inventory_head head ON head.singleton_id = 1
  JOIN jobs_workflow_legacy_inventory_generations legacy
    ON legacy.generation = head.generation
   AND legacy.query_digest_sha256 = head.query_digest_sha256
   AND legacy.generation = binding.legacy_generation
   AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
   AND legacy.completion_epoch = tombstone.legacy_completion_epoch
   AND legacy.completion_digest_sha256 = tombstone.legacy_completion_digest_sha256
   AND legacy.revalidate_after_ms = tombstone.legacy_revalidate_after_ms
 WHERE binding.account_id = OLD.account_id AND binding.state = 'complete'
   AND binding.hard_delete_authorized_at_ms IS NOT NULL
   AND tombstone.cleanup_generation_id = binding.cleanup_generation_id
   AND tombstone.legacy_inventory_generation_id = binding.legacy_inventory_generation_id
   AND legacy.state = 'complete'
   AND legacy.revalidate_after_ms >
       CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
   AND EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_generations generation
      WHERE generation.account_id = binding.account_id
        AND generation.generation = binding.workflow_cleanup_generation
        AND generation.target_set_hmac_sha256 = binding.target_set_hmac_sha256
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
           WHERE target.account_id = generation.account_id
             AND target.generation = generation.generation
             AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256)
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities authority
           WHERE authority.account_id = generation.account_id
             AND authority.workflow_cleanup_generation = generation.generation)
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets proved
           WHERE proved.account_id = generation.account_id
             AND proved.generation = generation.generation
             AND proved.target_set_hmac_sha256 = generation.target_set_hmac_sha256)
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_targets target
      WHERE target.account_id = binding.account_id
        AND target.generation = binding.workflow_cleanup_generation
        AND target.target_set_hmac_sha256 = binding.target_set_hmac_sha256
        AND target.target_state <> 'absence_proved'
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_targets target
      WHERE target.generation = binding.legacy_generation
        AND (target.target_state <> 'absence_proved'
          OR target.positive_reset_required = 1)
   )
 )
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup completion is required');
END;

DROP TRIGGER IF EXISTS trg_jobs_workflow_application_delete_cascade_only;
CREATE TRIGGER trg_jobs_workflow_application_delete_cascade_only
BEFORE DELETE ON jobs_applications
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_commands command
   WHERE command.account_id = OLD.account_id AND command.application_id = OLD.id
) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup application deletion requires account cascade');
END;

DROP TRIGGER IF EXISTS trg_jobs_workflow_intervention_delete_guard;
CREATE TRIGGER trg_jobs_workflow_intervention_delete_guard
BEFORE DELETE ON jobs_interventions
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_commands command
   WHERE command.account_id = OLD.account_id AND command.intervention_id = OLD.id
 ) AND NOT EXISTS (
   SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
    WHERE token.account_id = OLD.account_id
      AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
 ) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_account_bindings binding
  JOIN jobs_workflow_cleanup_completion_tombstones tombstone
    ON tombstone.tombstone_id = binding.completion_tombstone_id
   AND tombstone.completion_digest_sha256 = binding.completion_digest_sha256
  JOIN jobs_workflow_legacy_inventory_head head ON head.singleton_id = 1
  JOIN jobs_workflow_legacy_inventory_generations legacy
    ON legacy.generation = head.generation
   AND legacy.query_digest_sha256 = head.query_digest_sha256
   AND legacy.generation = binding.legacy_generation
   AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
   AND legacy.completion_epoch = tombstone.legacy_completion_epoch
   AND legacy.completion_digest_sha256 = tombstone.legacy_completion_digest_sha256
   AND legacy.revalidate_after_ms = tombstone.legacy_revalidate_after_ms
 WHERE binding.account_id = OLD.account_id AND binding.state = 'complete'
   AND binding.hard_delete_authorized_at_ms IS NOT NULL
   AND tombstone.cleanup_generation_id = binding.cleanup_generation_id
   AND tombstone.legacy_inventory_generation_id = binding.legacy_inventory_generation_id
   AND legacy.state = 'complete'
   AND legacy.revalidate_after_ms >
       CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
   AND EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_generations generation
      WHERE generation.account_id = binding.account_id
        AND generation.generation = binding.workflow_cleanup_generation
        AND generation.target_set_hmac_sha256 = binding.target_set_hmac_sha256
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
           WHERE target.account_id = generation.account_id
             AND target.generation = generation.generation
             AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256)
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities authority
           WHERE authority.account_id = generation.account_id
             AND authority.workflow_cleanup_generation = generation.generation)
        AND generation.target_count = (
          SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets proved
           WHERE proved.account_id = generation.account_id
             AND proved.generation = generation.generation
             AND proved.target_set_hmac_sha256 = generation.target_set_hmac_sha256)
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_cleanup_targets target
      WHERE target.account_id = binding.account_id
        AND target.generation = binding.workflow_cleanup_generation
        AND target.target_set_hmac_sha256 = binding.target_set_hmac_sha256
        AND target.target_state <> 'absence_proved'
   )
   AND NOT EXISTS (
     SELECT 1 FROM jobs_workflow_legacy_targets target
      WHERE target.generation = binding.legacy_generation
        AND (target.target_state <> 'absence_proved'
          OR target.positive_reset_required = 1)
   )
 )
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup completion is required');
END;

DROP TRIGGER IF EXISTS trg_jobs_workflow_intervention_delete_cascade_only;
CREATE TRIGGER trg_jobs_workflow_intervention_delete_cascade_only
BEFORE DELETE ON jobs_interventions
WHEN EXISTS (
  SELECT 1 FROM jobs_workflow_commands command
   WHERE command.account_id = OLD.account_id AND command.intervention_id = OLD.id
) AND NOT EXISTS (
  SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
   WHERE token.account_id = OLD.account_id
     AND NOT EXISTS (SELECT 1 FROM accounts account WHERE account.id = OLD.account_id)
)
BEGIN
  SELECT RAISE(ABORT, 'workflow cleanup intervention deletion requires account cascade');
END;

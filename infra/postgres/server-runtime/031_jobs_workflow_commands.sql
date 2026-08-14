-- Target: PostgreSQL
-- Durable, ambiguity-safe Temporal workflow start and intervention-resume commands.

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_applications_workflow_command_authority
  ON jobs_applications(id, account_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_interventions_workflow_command_authority
  ON jobs_interventions(id, account_id, application_id);

CREATE TABLE IF NOT EXISTS jobs_workflow_commands (
  id                              TEXT PRIMARY KEY
    CHECK(length(id) BETWEEN 1 AND 128),
  account_id                      TEXT NOT NULL
    REFERENCES accounts(id) ON DELETE CASCADE,
  application_id                  TEXT NOT NULL,
  run_id                          TEXT NOT NULL
    CHECK(length(run_id) BETWEEN 20 AND 128 AND run_id ~ '^[A-Za-z0-9_-]+$'),
  workflow_id                     TEXT NOT NULL
    CHECK(length(workflow_id) BETWEEN 20 AND 192 AND workflow_id ~ '^[A-Za-z0-9_-]+$'),
  intervention_id                 TEXT,
  command_kind                    TEXT NOT NULL
    CHECK(command_kind IN ('start', 'resume')),
  protocol_version                BIGINT NOT NULL DEFAULT 2
    CHECK(protocol_version = 2),
  idempotency_key_hmac_sha256     TEXT NOT NULL
    CHECK(length(idempotency_key_hmac_sha256) = 64
      AND lower(idempotency_key_hmac_sha256) = idempotency_key_hmac_sha256
      AND idempotency_key_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  request_id                      TEXT NOT NULL UNIQUE
    CHECK(length(request_id) BETWEEN 20 AND 128 AND request_id ~ '^[A-Za-z0-9_-]+$'),
  request_hmac_sha256             TEXT NOT NULL
    CHECK(length(request_hmac_sha256) = 64
      AND lower(request_hmac_sha256) = request_hmac_sha256
      AND request_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  payload_hmac_sha256             TEXT NOT NULL
    CHECK(length(payload_hmac_sha256) = 64
      AND lower(payload_hmac_sha256) = payload_hmac_sha256
      AND payload_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  state                           TEXT NOT NULL CHECK(state IN (
    'pending', 'claimed', 'delivering', 'delivery_unknown', 'accepted',
    'identity_conflict', 'rejected', 'cancelled'
  )),
  command_json                    TEXT NOT NULL
    CHECK(length(command_json) BETWEEN 15 AND 16777216
      AND left(command_json, 14) = 'bluey-jobs:v1:'),
  attempt_count                   BIGINT NOT NULL DEFAULT 0
    CHECK(attempt_count BETWEEN 0 AND 9007199254740991),
  fence                           BIGINT NOT NULL DEFAULT 0
    CHECK(fence BETWEEN 0 AND 9007199254740991),
  lease_owner                     TEXT,
  lease_token_sha256              TEXT,
  lease_expires_at_ms             BIGINT,
  active_attempt_id               TEXT,
  first_request_started_at_ms     BIGINT,
  first_ambiguous_at_ms           BIGINT,
  next_attempt_at_ms              BIGINT,
  last_outcome_code               TEXT CHECK(last_outcome_code IS NULL OR last_outcome_code IN (
    'accepted', 'already_accepted', 'transport_timeout', 'connection_lost',
    'gateway_unavailable', 'gateway_5xx', 'malformed_response', 'lease_expired',
    'identity_conflict', 'invalid_request', 'unauthorized', 'workflow_not_found',
    'unsupported_protocol', 'gateway_rejected', 'operator_cancelled'
  )),
  temporal_run_id                 TEXT,
  accepted_at_ms                  BIGINT,
  created_at_ms                   BIGINT NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                   BIGINT NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, command_kind, idempotency_key_hmac_sha256),
  UNIQUE(id, account_id),
  UNIQUE(id, account_id, request_id, payload_hmac_sha256),
  FOREIGN KEY(application_id, account_id)
    REFERENCES jobs_applications(id, account_id) ON DELETE CASCADE,
  FOREIGN KEY(intervention_id, account_id, application_id)
    REFERENCES jobs_interventions(id, account_id, application_id) ON DELETE CASCADE,
  CHECK(
    (command_kind = 'start' AND intervention_id IS NULL)
    OR (command_kind = 'resume' AND intervention_id IS NOT NULL
      AND length(intervention_id) BETWEEN 20 AND 128
      AND intervention_id ~ '^[A-Za-z0-9_-]+$')
  ),
  CHECK(attempt_count = fence),
  CHECK(
    (attempt_count = 0 AND active_attempt_id IS NULL)
    OR (attempt_count > 0 AND active_attempt_id IS NOT NULL)
  ),
  CHECK(
    (state IN ('claimed', 'delivering')
      AND lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 IS NOT NULL AND length(lease_token_sha256) = 64
      AND lower(lease_token_sha256) = lease_token_sha256
      AND lease_token_sha256 ~ '^[0-9a-f]{64}$'
      AND lease_expires_at_ms IS NOT NULL
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991
      AND next_attempt_at_ms IS NULL)
    OR (state IN ('pending', 'delivery_unknown')
      AND lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL
      AND next_attempt_at_ms IS NOT NULL
      AND next_attempt_at_ms BETWEEN 0 AND 9007199254740991)
    OR (state IN ('accepted', 'identity_conflict', 'rejected', 'cancelled')
      AND lease_owner IS NULL AND lease_token_sha256 IS NULL
      AND lease_expires_at_ms IS NULL AND next_attempt_at_ms IS NULL)
  ),
  CHECK(state <> 'delivering' OR first_request_started_at_ms IS NOT NULL),
  CHECK(state <> 'delivery_unknown' OR (
    first_request_started_at_ms IS NOT NULL AND first_ambiguous_at_ms IS NOT NULL
  )),
  CHECK(state <> 'cancelled' OR first_request_started_at_ms IS NULL),
  CHECK(
    (state = 'accepted' AND temporal_run_id IS NOT NULL
      AND length(temporal_run_id) BETWEEN 20 AND 128
      AND temporal_run_id ~ '^[A-Za-z0-9_-]+$'
      AND accepted_at_ms IS NOT NULL)
    OR (state <> 'accepted' AND temporal_run_id IS NULL AND accepted_at_ms IS NULL)
  ),
  CHECK(updated_at_ms >= created_at_ms),
  CHECK(first_request_started_at_ms IS NULL OR
    first_request_started_at_ms BETWEEN created_at_ms AND updated_at_ms),
  CHECK(first_ambiguous_at_ms IS NULL OR
    first_ambiguous_at_ms BETWEEN created_at_ms AND updated_at_ms),
  CHECK(accepted_at_ms IS NULL OR accepted_at_ms BETWEEN created_at_ms AND updated_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_command_attempts (
  id                          TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 128),
  account_id                  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  command_id                  TEXT NOT NULL,
  attempt_no                  BIGINT NOT NULL
    CHECK(attempt_no BETWEEN 1 AND 9007199254740991),
  fence                       BIGINT NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  lease_owner                 TEXT NOT NULL CHECK(length(lease_owner) BETWEEN 1 AND 128),
  lease_token_sha256          TEXT NOT NULL
    CHECK(length(lease_token_sha256) = 64
      AND lower(lease_token_sha256) = lease_token_sha256
      AND lease_token_sha256 ~ '^[0-9a-f]{64}$'),
  request_id                  TEXT NOT NULL,
  payload_hmac_sha256         TEXT NOT NULL,
  claimed_at_ms               BIGINT NOT NULL
    CHECK(claimed_at_ms BETWEEN 0 AND 9007199254740991),
  lease_expires_at_ms         BIGINT NOT NULL
    CHECK(lease_expires_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(command_id, attempt_no),
  UNIQUE(command_id, fence),
  UNIQUE(id, account_id, command_id, fence),
  FOREIGN KEY(command_id, account_id, request_id, payload_hmac_sha256)
    REFERENCES jobs_workflow_commands(id, account_id, request_id, payload_hmac_sha256)
    ON DELETE CASCADE,
  CHECK(attempt_no = fence),
  CHECK(lease_expires_at_ms > claimed_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_command_attempt_events (
  id                          TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 128),
  account_id                  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  command_id                  TEXT NOT NULL,
  attempt_id                  TEXT NOT NULL,
  fence                       BIGINT NOT NULL
    CHECK(fence BETWEEN 1 AND 9007199254740991),
  event_phase                 TEXT NOT NULL CHECK(event_phase IN ('request_started', 'terminal')),
  event_kind                  TEXT NOT NULL CHECK(event_kind IN (
    'request_started', 'accepted', 'already_accepted', 'delivery_unknown',
    'identity_conflict', 'rejected', 'lease_expired_before_start'
  )),
  reason_code                 TEXT CHECK(reason_code IS NULL OR reason_code IN (
    'transport_timeout', 'connection_lost', 'gateway_unavailable', 'gateway_5xx',
    'malformed_response', 'lease_expired', 'identity_conflict', 'invalid_request',
    'unauthorized', 'workflow_not_found', 'unsupported_protocol', 'gateway_rejected'
  )),
  temporal_run_id             TEXT,
  recorded_at_ms              BIGINT NOT NULL
    CHECK(recorded_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(attempt_id, event_phase),
  FOREIGN KEY(attempt_id, account_id, command_id, fence)
    REFERENCES jobs_workflow_command_attempts(id, account_id, command_id, fence)
    ON DELETE CASCADE,
  CHECK(
    (event_phase = 'request_started' AND event_kind = 'request_started'
      AND reason_code IS NULL AND temporal_run_id IS NULL)
    OR (event_phase = 'terminal' AND event_kind <> 'request_started')
  ),
  CHECK(
    (event_kind IN ('accepted', 'already_accepted')
      AND reason_code IS NULL AND temporal_run_id IS NOT NULL
      AND length(temporal_run_id) BETWEEN 20 AND 128
      AND temporal_run_id ~ '^[A-Za-z0-9_-]+$')
    OR (event_kind = 'delivery_unknown'
      AND reason_code IN ('transport_timeout', 'connection_lost', 'gateway_unavailable',
        'gateway_5xx', 'malformed_response', 'lease_expired')
      AND temporal_run_id IS NULL)
    OR (event_kind = 'identity_conflict'
      AND reason_code = 'identity_conflict' AND temporal_run_id IS NULL)
    OR (event_kind = 'rejected'
      AND reason_code IN ('invalid_request', 'unauthorized', 'workflow_not_found',
        'unsupported_protocol', 'gateway_rejected') AND temporal_run_id IS NULL)
    OR (event_kind = 'lease_expired_before_start'
      AND reason_code = 'lease_expired' AND temporal_run_id IS NULL)
    OR event_kind = 'request_started'
  )
);

CREATE TABLE IF NOT EXISTS jobs_workflow_executions (
  account_id                       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  workflow_id                      TEXT NOT NULL,
  start_command_id                 TEXT NOT NULL,
  first_execution_run_id           TEXT NOT NULL
    CHECK(length(first_execution_run_id) BETWEEN 20 AND 128
      AND first_execution_run_id ~ '^[A-Za-z0-9_-]+$'),
  lifecycle_state                  TEXT NOT NULL CHECK(lifecycle_state IN (
    'accepted', 'running', 'terminal', 'termination_requested', 'terminated', 'absent'
  )),
  cleanup_state                    TEXT NOT NULL CHECK(cleanup_state IN (
    'required', 'termination_pending', 'history_delete_pending',
    'history_deleted', 'absence_proved'
  )),
  deletion_target_generation       BIGINT
    CHECK(deletion_target_generation IS NULL
      OR deletion_target_generation BETWEEN 1 AND 9007199254740991),
  deletion_target_hmac_sha256      TEXT,
  cleanup_fence                    BIGINT NOT NULL DEFAULT 0
    CHECK(cleanup_fence BETWEEN 0 AND 9007199254740991),
  cleanup_lease_owner              TEXT,
  cleanup_lease_token_sha256       TEXT,
  cleanup_lease_expires_at_ms      BIGINT,
  terminal_at_ms                   BIGINT,
  termination_requested_at_ms      BIGINT,
  terminated_at_ms                 BIGINT,
  history_deleted_at_ms            BIGINT,
  absence_proved_at_ms             BIGINT,
  created_at_ms                    BIGINT NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                    BIGINT NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(account_id, workflow_id),
  UNIQUE(start_command_id, account_id, workflow_id, first_execution_run_id),
  FOREIGN KEY(start_command_id, account_id)
    REFERENCES jobs_workflow_commands(id, account_id) ON DELETE CASCADE,
  CHECK(
    (deletion_target_generation IS NULL AND deletion_target_hmac_sha256 IS NULL)
    OR (deletion_target_generation IS NOT NULL
      AND deletion_target_hmac_sha256 IS NOT NULL
      AND length(deletion_target_hmac_sha256) = 64
      AND lower(deletion_target_hmac_sha256) = deletion_target_hmac_sha256
      AND deletion_target_hmac_sha256 ~ '^[0-9a-f]{64}$')
  ),
  CHECK(lifecycle_state <> 'terminal' OR terminal_at_ms IS NOT NULL),
  CHECK(lifecycle_state <> 'termination_requested'
    OR termination_requested_at_ms IS NOT NULL),
  CHECK(lifecycle_state <> 'terminated' OR terminated_at_ms IS NOT NULL),
  CHECK(cleanup_state <> 'history_deleted' OR history_deleted_at_ms IS NOT NULL),
  CHECK(cleanup_state <> 'absence_proved' OR absence_proved_at_ms IS NOT NULL),
  CHECK(cleanup_state NOT IN ('history_deleted', 'absence_proved')
    OR deletion_target_generation IS NOT NULL),
  CHECK(
    (cleanup_lease_owner IS NULL AND cleanup_lease_token_sha256 IS NULL
      AND cleanup_lease_expires_at_ms IS NULL)
    OR (cleanup_lease_owner IS NOT NULL AND length(cleanup_lease_owner) BETWEEN 1 AND 128
      AND cleanup_lease_token_sha256 IS NOT NULL
      AND length(cleanup_lease_token_sha256) = 64
      AND lower(cleanup_lease_token_sha256) = cleanup_lease_token_sha256
      AND cleanup_lease_token_sha256 ~ '^[0-9a-f]{64}$'
      AND cleanup_lease_expires_at_ms IS NOT NULL)
  ),
  CHECK(updated_at_ms >= created_at_ms)
);

-- Account-scoped workflow cleanup is an earlier, independent deletion fence.
CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_generations (
  account_id                       TEXT PRIMARY KEY
    REFERENCES accounts(id) ON DELETE CASCADE,
  generation                       BIGINT NOT NULL
    CHECK(generation BETWEEN 1 AND 9007199254740991),
  state                            TEXT NOT NULL CHECK(state IN ('frozen', 'cleaning', 'complete')),
  target_set_hmac_sha256           TEXT NOT NULL
    CHECK(length(target_set_hmac_sha256) = 64
      AND lower(target_set_hmac_sha256) = target_set_hmac_sha256
      AND target_set_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  target_count                     BIGINT NOT NULL
    CHECK(target_count BETWEEN 0 AND 9007199254740991),
  -- Fail closed until an authenticated, fully paginated legacy Temporal
  -- inventory attestation has its own durable authority table/API.
  legacy_reconciled                BOOLEAN NOT NULL DEFAULT FALSE CHECK(NOT legacy_reconciled),
  legacy_unresolved_count          BIGINT NOT NULL DEFAULT 1
    CHECK(legacy_unresolved_count BETWEEN 1 AND 9007199254740991),
  frozen_at_ms                     BIGINT NOT NULL
    CHECK(frozen_at_ms BETWEEN 0 AND 9007199254740991),
  completed_at_ms                  BIGINT
    CHECK(completed_at_ms IS NULL
      OR completed_at_ms BETWEEN 0 AND 9007199254740991),
  created_at_ms                    BIGINT NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                    BIGINT NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, generation),
  UNIQUE(account_id, generation, target_set_hmac_sha256),
  CHECK(state <> 'complete' OR (
    completed_at_ms IS NOT NULL AND legacy_reconciled AND legacy_unresolved_count = 0
  )),
  CHECK(updated_at_ms >= created_at_ms)
);

-- The frozen target is the exact start request identity, not merely an
-- acceptance receipt. This preserves delivery-unknown commands that have no
-- firstExecutionRunId yet and lets cleanup reconcile that identity safely.
CREATE TABLE IF NOT EXISTS jobs_workflow_cleanup_targets (
  account_id                       TEXT NOT NULL,
  generation                       BIGINT NOT NULL
    CHECK(generation BETWEEN 1 AND 9007199254740991),
  target_set_hmac_sha256           TEXT NOT NULL
    CHECK(length(target_set_hmac_sha256) = 64
      AND lower(target_set_hmac_sha256) = target_set_hmac_sha256
      AND target_set_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  workflow_id                      TEXT NOT NULL
    CHECK(length(workflow_id) BETWEEN 20 AND 192
      AND workflow_id ~ '^[A-Za-z0-9_-]+$'),
  start_command_id                 TEXT NOT NULL CHECK(length(start_command_id) BETWEEN 1 AND 128),
  start_request_id                 TEXT NOT NULL
    CHECK(length(start_request_id) BETWEEN 20 AND 128
      AND start_request_id ~ '^[A-Za-z0-9_-]+$'),
  start_payload_hmac_sha256        TEXT NOT NULL
    CHECK(length(start_payload_hmac_sha256) = 64
      AND lower(start_payload_hmac_sha256) = start_payload_hmac_sha256
      AND start_payload_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  first_execution_run_id           TEXT
    CHECK(first_execution_run_id IS NULL OR (
      length(first_execution_run_id) BETWEEN 20 AND 128
      AND first_execution_run_id ~ '^[A-Za-z0-9_-]+$')),
  target_state                     TEXT NOT NULL CHECK(target_state IN (
    'delivery_drain', 'identity_reconcile', 'cleanup_required', 'termination_pending',
    'history_delete_pending', 'absence_proved', 'identity_conflict'
  )),
  fence                            BIGINT NOT NULL DEFAULT 0
    CHECK(fence BETWEEN 0 AND 9007199254740991),
  cleanup_request_id               TEXT
    CHECK(cleanup_request_id IS NULL OR (
      length(cleanup_request_id) BETWEEN 20 AND 128
      AND cleanup_request_id ~ '^[A-Za-z0-9_-]+$')),
  lease_owner                      TEXT,
  lease_token_sha256               TEXT,
  lease_expires_at_ms              BIGINT,
  absence_proved_at_ms             BIGINT
    CHECK(absence_proved_at_ms IS NULL
      OR absence_proved_at_ms BETWEEN 0 AND 9007199254740991),
  created_at_ms                    BIGINT NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  updated_at_ms                    BIGINT NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(account_id, generation, workflow_id),
  UNIQUE(start_command_id),
  UNIQUE(account_id, generation, target_set_hmac_sha256, workflow_id),
  FOREIGN KEY(account_id, generation, target_set_hmac_sha256)
    REFERENCES jobs_workflow_cleanup_generations(
      account_id, generation, target_set_hmac_sha256
    ) ON DELETE CASCADE,
  FOREIGN KEY(start_command_id, account_id, start_request_id, start_payload_hmac_sha256)
    REFERENCES jobs_workflow_commands(id, account_id, request_id, payload_hmac_sha256)
    ON DELETE CASCADE,
  CHECK(
    (target_state IN ('delivery_drain', 'identity_reconcile')
      AND first_execution_run_id IS NULL)
    OR (target_state NOT IN ('delivery_drain', 'identity_reconcile'))
  ),
  CHECK(target_state <> 'absence_proved' OR absence_proved_at_ms IS NOT NULL),
  CHECK(
    (lease_owner IS NULL AND lease_token_sha256 IS NULL AND lease_expires_at_ms IS NULL)
    OR (target_state NOT IN ('absence_proved', 'identity_conflict')
      AND cleanup_request_id IS NOT NULL
      AND lease_owner IS NOT NULL AND length(lease_owner) BETWEEN 1 AND 128
      AND lease_token_sha256 IS NOT NULL
      AND length(lease_token_sha256) = 64
      AND lower(lease_token_sha256) = lease_token_sha256
      AND lease_token_sha256 ~ '^[0-9a-f]{64}$'
      AND lease_expires_at_ms BETWEEN 0 AND 9007199254740991)
  ),
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_execution_cleanup_observations (
  id                               TEXT PRIMARY KEY
    CHECK(length(id) BETWEEN 20 AND 128
      AND id ~ '^[A-Za-z0-9_-]+$'),
  account_id                       TEXT NOT NULL,
  workflow_id                      TEXT NOT NULL
    CHECK(length(workflow_id) BETWEEN 20 AND 192
      AND workflow_id ~ '^[A-Za-z0-9_-]+$'),
  generation                       BIGINT NOT NULL
    CHECK(generation BETWEEN 1 AND 9007199254740991),
  target_set_hmac_sha256           TEXT NOT NULL
    CHECK(length(target_set_hmac_sha256) = 64
      AND lower(target_set_hmac_sha256) = target_set_hmac_sha256
      AND target_set_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  observation_kind                 TEXT NOT NULL CHECK(observation_kind IN (
    'identity_confirmed', 'identity_conflict', 'termination_requested',
    'termination_confirmed', 'history_delete_requested', 'history_delete_confirmed',
    'absence_proved'
  )),
  observed_execution_run_id        TEXT
    CHECK(observed_execution_run_id IS NULL OR (
      length(observed_execution_run_id) BETWEEN 20 AND 128
      AND observed_execution_run_id ~ '^[A-Za-z0-9_-]+$')),
  cleanup_fence                    BIGINT NOT NULL
    CHECK(cleanup_fence BETWEEN 1 AND 9007199254740991),
  cleanup_request_id               TEXT NOT NULL
    CHECK(length(cleanup_request_id) BETWEEN 20 AND 128
      AND cleanup_request_id ~ '^[A-Za-z0-9_-]+$'),
  evidence_hmac_sha256             TEXT NOT NULL
    CHECK(length(evidence_hmac_sha256) = 64
      AND lower(evidence_hmac_sha256) = evidence_hmac_sha256
      AND evidence_hmac_sha256 ~ '^[0-9a-f]{64}$'),
  recorded_at_ms                   BIGINT NOT NULL
    CHECK(recorded_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, workflow_id, generation, cleanup_fence, observation_kind),
  FOREIGN KEY(account_id, generation, target_set_hmac_sha256, workflow_id)
    REFERENCES jobs_workflow_cleanup_targets(
      account_id, generation, target_set_hmac_sha256, workflow_id
    ) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS jobs_workflow_intervention_preparations (
  command_id                       TEXT PRIMARY KEY,
  account_id                       TEXT NOT NULL,
  request_id                       TEXT NOT NULL UNIQUE,
  payload_hmac_sha256              TEXT NOT NULL,
  intervention_id                 TEXT NOT NULL UNIQUE,
  receipt_hmac_sha256              TEXT NOT NULL,
  intervention_json               TEXT NOT NULL
    CHECK(length(intervention_json) BETWEEN 15 AND 16777216
      AND left(intervention_json, 14) = 'bluey-jobs:v1:'),
  published_at_ms                  BIGINT,
  created_at_ms                    BIGINT NOT NULL,
  updated_at_ms                    BIGINT NOT NULL,
  FOREIGN KEY(command_id, account_id, request_id, payload_hmac_sha256)
    REFERENCES jobs_workflow_commands(id, account_id, request_id, payload_hmac_sha256)
    ON DELETE CASCADE,
  CHECK(updated_at_ms >= created_at_ms)
);

CREATE TABLE IF NOT EXISTS jobs_workflow_execution_finalizations (
  command_id                       TEXT PRIMARY KEY,
  account_id                       TEXT NOT NULL,
  request_id                       TEXT NOT NULL UNIQUE,
  payload_hmac_sha256              TEXT NOT NULL,
  outcome_kind                     TEXT NOT NULL CHECK(outcome_kind IN ('failed', 'side_effect_unknown')),
  reason_code                      TEXT NOT NULL CHECK(reason_code IN (
    'runner_failed', 'runner_ambiguous', 'intervention_timeout', 'intervention_limit'
  )),
  open_intervention_id             TEXT,
  outcome_hmac_sha256              TEXT NOT NULL,
  finalized_at_ms                  BIGINT NOT NULL,
  FOREIGN KEY(command_id, account_id, request_id, payload_hmac_sha256)
    REFERENCES jobs_workflow_commands(id, account_id, request_id, payload_hmac_sha256)
    ON DELETE CASCADE,
  CHECK(
    (outcome_kind = 'side_effect_unknown' AND reason_code = 'runner_ambiguous')
    OR (outcome_kind = 'failed' AND reason_code IN (
      'runner_failed', 'intervention_timeout', 'intervention_limit'
    ))
  ),
  CHECK(
    (reason_code = 'intervention_timeout' AND open_intervention_id IS NOT NULL)
    OR (reason_code <> 'intervention_timeout' AND open_intervention_id IS NULL)
  )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_workflow_commands_start_run
  ON jobs_workflow_commands(account_id, application_id, run_id)
  WHERE command_kind = 'start';
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_workflow_commands_start_workflow
  ON jobs_workflow_commands(workflow_id)
  WHERE command_kind = 'start';
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_workflow_commands_resume_intervention
  ON jobs_workflow_commands(account_id, intervention_id)
  WHERE command_kind = 'resume';
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_commands_due
  ON jobs_workflow_commands(state, next_attempt_at_ms, created_at_ms, id);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_commands_lease_expiry
  ON jobs_workflow_commands(state, lease_expires_at_ms, id);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_commands_account_history
  ON jobs_workflow_commands(account_id, application_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_attempts_command
  ON jobs_workflow_command_attempts(account_id, command_id, attempt_no DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_attempt_events_command
  ON jobs_workflow_command_attempt_events(account_id, command_id, recorded_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_executions_cleanup
  ON jobs_workflow_executions(cleanup_state, lifecycle_state, updated_at_ms, workflow_id);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_cleanup_targets_due
  ON jobs_workflow_cleanup_targets(
    target_state, lease_expires_at_ms, updated_at_ms, account_id, workflow_id
  );
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_cleanup_observations_target
  ON jobs_workflow_execution_cleanup_observations(
    account_id, workflow_id, generation, observation_kind
  );

CREATE OR REPLACE FUNCTION validate_jobs_workflow_command_resume_authority()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.command_kind = 'resume' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_commands AS start_command
     WHERE start_command.account_id = NEW.account_id
       AND start_command.application_id = NEW.application_id
       AND start_command.run_id = NEW.run_id
       AND start_command.workflow_id = NEW.workflow_id
       AND start_command.command_kind = 'start'
  ) THEN
    RAISE EXCEPTION 'workflow resume has no start authority';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_command_resume_authority_insert
  ON jobs_workflow_commands;
CREATE TRIGGER trg_jobs_workflow_command_resume_authority_insert
BEFORE INSERT ON jobs_workflow_commands
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_command_resume_authority();

CREATE OR REPLACE FUNCTION reject_jobs_workflow_command_authority_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow command authority is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_command_authority_immutable
  ON jobs_workflow_commands;
CREATE TRIGGER trg_jobs_workflow_command_authority_immutable
BEFORE UPDATE OF
  account_id, application_id, run_id, workflow_id, intervention_id, command_kind,
  protocol_version, idempotency_key_hmac_sha256, request_id, request_hmac_sha256,
  payload_hmac_sha256, command_json, created_at_ms
ON jobs_workflow_commands
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_command_authority_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_command_active_attempt()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.active_attempt_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_command_attempts AS attempt
     WHERE attempt.id = NEW.active_attempt_id
       AND attempt.account_id = NEW.account_id
       AND attempt.command_id = NEW.id
       AND attempt.fence = NEW.fence
       AND attempt.request_id = NEW.request_id
       AND attempt.payload_hmac_sha256 = NEW.payload_hmac_sha256
  ) THEN
    RAISE EXCEPTION 'invalid workflow command attempt authority';
  END IF;
  IF NEW.state = 'delivering' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_command_attempt_events AS event
     WHERE event.attempt_id = NEW.active_attempt_id
       AND event.account_id = NEW.account_id AND event.command_id = NEW.id
       AND event.fence = NEW.fence AND event.event_kind = 'request_started'
  ) THEN
    RAISE EXCEPTION 'workflow request-start evidence is missing';
  END IF;
  IF NEW.state = 'delivery_unknown' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_command_attempt_events AS event
     WHERE event.attempt_id = NEW.active_attempt_id
       AND event.account_id = NEW.account_id AND event.command_id = NEW.id
       AND event.fence = NEW.fence AND event.event_kind = 'delivery_unknown'
  ) THEN
    RAISE EXCEPTION 'workflow ambiguity evidence is missing';
  END IF;
  IF NEW.state = 'accepted' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_command_attempt_events AS event
     WHERE event.attempt_id = NEW.active_attempt_id
       AND event.account_id = NEW.account_id AND event.command_id = NEW.id
       AND event.fence = NEW.fence
       AND event.event_kind IN ('accepted', 'already_accepted')
       AND event.temporal_run_id = NEW.temporal_run_id
  ) THEN
    RAISE EXCEPTION 'workflow acceptance evidence is missing';
  END IF;
  IF NEW.state = 'identity_conflict' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_command_attempt_events AS event
     WHERE event.attempt_id = NEW.active_attempt_id
       AND event.account_id = NEW.account_id AND event.command_id = NEW.id
       AND event.fence = NEW.fence AND event.event_kind = 'identity_conflict'
  ) THEN
    RAISE EXCEPTION 'workflow identity-conflict evidence is missing';
  END IF;
  IF NEW.state = 'rejected' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_command_attempt_events AS event
     WHERE event.attempt_id = NEW.active_attempt_id
       AND event.account_id = NEW.account_id AND event.command_id = NEW.id
       AND event.fence = NEW.fence AND event.event_kind = 'rejected'
  ) THEN
    RAISE EXCEPTION 'workflow rejection evidence is missing';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_command_active_attempt
  ON jobs_workflow_commands;
CREATE TRIGGER trg_jobs_workflow_command_active_attempt
BEFORE UPDATE OF state, active_attempt_id, fence ON jobs_workflow_commands
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_command_active_attempt();

CREATE OR REPLACE FUNCTION reject_jobs_workflow_attempt_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow command attempt ledger is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_attempts_no_update
  ON jobs_workflow_command_attempts;
CREATE TRIGGER trg_jobs_workflow_attempts_no_update
BEFORE UPDATE ON jobs_workflow_command_attempts
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_attempt_update();
DROP TRIGGER IF EXISTS trg_jobs_workflow_attempt_events_no_update
  ON jobs_workflow_command_attempt_events;
CREATE TRIGGER trg_jobs_workflow_attempt_events_no_update
BEFORE UPDATE ON jobs_workflow_command_attempt_events
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_attempt_update();

-- Existing account/application deletion code does not yet own Temporal cleanup.
-- Fail closed instead of cascading away a command that may have external history.
CREATE OR REPLACE FUNCTION reject_jobs_workflow_execution_binding_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow execution identity is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_execution_binding_immutable
  ON jobs_workflow_executions;
CREATE TRIGGER trg_jobs_workflow_execution_binding_immutable
BEFORE UPDATE OF account_id, workflow_id, start_command_id, first_execution_run_id, created_at_ms
ON jobs_workflow_executions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_execution_binding_update();

CREATE OR REPLACE FUNCTION reject_jobs_workflow_cleanup_generation_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow cleanup generation is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_generation_immutable
  ON jobs_workflow_cleanup_generations;
CREATE TRIGGER trg_jobs_workflow_cleanup_generation_immutable
BEFORE UPDATE OF generation, target_set_hmac_sha256, target_count, frozen_at_ms, created_at_ms
ON jobs_workflow_cleanup_generations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_generation_update();

DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_observation_no_update
  ON jobs_workflow_execution_cleanup_observations;
CREATE TRIGGER trg_jobs_workflow_cleanup_observation_no_update
BEFORE UPDATE ON jobs_workflow_execution_cleanup_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_attempt_update();

CREATE OR REPLACE FUNCTION reject_jobs_workflow_cleanup_target_identity_update()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'workflow cleanup target identity is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_target_identity_immutable
  ON jobs_workflow_cleanup_targets;
CREATE TRIGGER trg_jobs_workflow_cleanup_target_identity_immutable
BEFORE UPDATE OF account_id, generation, target_set_hmac_sha256, workflow_id,
  start_command_id, start_request_id, start_payload_hmac_sha256, created_at_ms
ON jobs_workflow_cleanup_targets
FOR EACH ROW EXECUTE FUNCTION reject_jobs_workflow_cleanup_target_identity_update();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_target_transition()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.target_state IN ('absence_proved', 'identity_conflict')
      AND NEW.target_state <> OLD.target_state THEN
    RAISE EXCEPTION 'workflow cleanup target is terminal';
  END IF;
  IF OLD.first_execution_run_id IS NOT NULL
      AND NEW.first_execution_run_id IS DISTINCT FROM OLD.first_execution_run_id THEN
    RAISE EXCEPTION 'workflow cleanup first run identity is immutable';
  END IF;
  IF NEW.target_state = 'identity_reconcile' AND OLD.target_state = 'delivery_drain'
      AND NOT EXISTS (
        SELECT 1 FROM jobs_workflow_commands command
         WHERE command.id = NEW.start_command_id
           AND command.account_id = NEW.account_id
           AND command.request_id = NEW.start_request_id
           AND command.payload_hmac_sha256 = NEW.start_payload_hmac_sha256
           AND command.state = 'delivery_unknown'
           AND command.lease_owner IS NULL
      ) THEN
    RAISE EXCEPTION 'workflow cleanup delivery is not drained';
  END IF;
  IF NEW.target_state = 'cleanup_required'
      AND OLD.target_state IN ('delivery_drain', 'identity_reconcile')
      AND NOT (NEW.first_execution_run_id IS NOT NULL AND (
        EXISTS (
          SELECT 1 FROM jobs_workflow_executions execution
           WHERE execution.account_id = NEW.account_id
             AND execution.workflow_id = NEW.workflow_id
             AND execution.start_command_id = NEW.start_command_id
             AND execution.first_execution_run_id = NEW.first_execution_run_id
        )
        OR EXISTS (
          SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
           WHERE observation.account_id = NEW.account_id
             AND observation.workflow_id = NEW.workflow_id
             AND observation.generation = NEW.generation
             AND observation.cleanup_fence = NEW.fence
             AND observation.cleanup_request_id = NEW.cleanup_request_id
             AND observation.observation_kind = 'identity_confirmed'
             AND observation.observed_execution_run_id = NEW.first_execution_run_id
        )
      )) THEN
    RAISE EXCEPTION 'workflow cleanup identity evidence is missing';
  END IF;
  IF NEW.target_state = 'identity_conflict' AND OLD.target_state <> 'identity_conflict'
      AND NOT EXISTS (
        SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
         WHERE observation.account_id = NEW.account_id
           AND observation.workflow_id = NEW.workflow_id
           AND observation.generation = NEW.generation
           AND observation.cleanup_fence = NEW.fence
           AND observation.cleanup_request_id = NEW.cleanup_request_id
           AND observation.observation_kind = 'identity_conflict'
      ) THEN
    RAISE EXCEPTION 'workflow cleanup identity-conflict evidence is missing';
  END IF;
  IF NEW.target_state = 'termination_pending' AND OLD.target_state <> 'termination_pending'
      AND NOT EXISTS (
        SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
         WHERE observation.account_id = NEW.account_id
           AND observation.workflow_id = NEW.workflow_id
           AND observation.generation = NEW.generation
           AND observation.cleanup_fence = NEW.fence
           AND observation.cleanup_request_id = NEW.cleanup_request_id
           AND observation.observation_kind = 'termination_requested'
           AND observation.observed_execution_run_id = NEW.first_execution_run_id
      ) THEN
    RAISE EXCEPTION 'workflow cleanup termination-request evidence is missing';
  END IF;
  IF NEW.target_state = 'history_delete_pending'
      AND OLD.target_state <> 'history_delete_pending' AND NOT EXISTS (
        SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
         WHERE observation.account_id = NEW.account_id
           AND observation.workflow_id = NEW.workflow_id
           AND observation.generation = NEW.generation
           AND observation.cleanup_fence = NEW.fence
           AND observation.cleanup_request_id = NEW.cleanup_request_id
           AND observation.observation_kind IN (
             'termination_confirmed', 'history_delete_requested', 'history_delete_confirmed'
           )
           AND observation.observed_execution_run_id = NEW.first_execution_run_id
      ) THEN
    RAISE EXCEPTION 'workflow cleanup history-delete evidence is missing';
  END IF;
  IF NEW.target_state = 'absence_proved' AND OLD.target_state <> 'absence_proved' AND NOT (
    NEW.absence_proved_at_ms IS NOT NULL
    AND EXISTS (SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
      WHERE observation.account_id = NEW.account_id
        AND observation.workflow_id = NEW.workflow_id
        AND observation.generation = NEW.generation
        AND observation.cleanup_fence = NEW.fence
        AND observation.cleanup_request_id = NEW.cleanup_request_id
        AND observation.observation_kind = 'absence_proved'
        AND observation.observed_execution_run_id IS NOT DISTINCT FROM NEW.first_execution_run_id)
  ) THEN
    RAISE EXCEPTION 'workflow cleanup absence proof is incomplete';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_target_transition_evidence
  ON jobs_workflow_cleanup_targets;
CREATE TRIGGER trg_jobs_workflow_cleanup_target_transition_evidence
BEFORE UPDATE OF first_execution_run_id, target_state, absence_proved_at_ms
ON jobs_workflow_cleanup_targets
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_target_transition();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_generation_complete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.state = 'complete' AND OLD.state <> 'complete' AND (
    NEW.completed_at_ms IS NULL OR NOT NEW.legacy_reconciled
    OR NEW.legacy_unresolved_count <> 0
    OR (SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets target
         WHERE target.account_id = NEW.account_id
           AND target.generation = NEW.generation
           AND target.target_set_hmac_sha256 = NEW.target_set_hmac_sha256)
         <> NEW.target_count
    OR EXISTS (
      SELECT 1 FROM jobs_workflow_cleanup_targets target
       WHERE target.account_id = NEW.account_id
         AND target.generation = NEW.generation
         AND target.target_set_hmac_sha256 = NEW.target_set_hmac_sha256
         AND target.target_state <> 'absence_proved'
    )
  ) THEN
    RAISE EXCEPTION 'workflow cleanup generation is not exactly complete';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_generation_complete_guard
  ON jobs_workflow_cleanup_generations;
CREATE TRIGGER trg_jobs_workflow_cleanup_generation_complete_guard
BEFORE UPDATE OF state, completed_at_ms ON jobs_workflow_cleanup_generations
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_generation_complete();

CREATE OR REPLACE FUNCTION validate_jobs_workflow_cleanup_transition()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.deletion_target_generation IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_generations generation
     WHERE generation.account_id = NEW.account_id
       AND generation.generation = NEW.deletion_target_generation
       AND generation.target_set_hmac_sha256 = NEW.deletion_target_hmac_sha256
  ) THEN
    RAISE EXCEPTION 'workflow cleanup target has no frozen generation';
  END IF;
  IF NEW.lifecycle_state = 'termination_requested'
      AND OLD.lifecycle_state <> 'termination_requested' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
     WHERE observation.account_id = NEW.account_id
       AND observation.workflow_id = NEW.workflow_id
       AND observation.generation = NEW.deletion_target_generation
       AND observation.observation_kind = 'termination_requested'
  ) THEN
    RAISE EXCEPTION 'workflow termination-request evidence is missing';
  END IF;
  IF NEW.lifecycle_state = 'terminated' AND OLD.lifecycle_state <> 'terminated' AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
     WHERE observation.account_id = NEW.account_id
       AND observation.workflow_id = NEW.workflow_id
       AND observation.generation = NEW.deletion_target_generation
       AND observation.observation_kind = 'termination_confirmed'
  ) THEN
    RAISE EXCEPTION 'workflow termination evidence is missing';
  END IF;
  IF NEW.cleanup_state = 'history_deleted' AND OLD.cleanup_state <> 'history_deleted'
      AND NOT EXISTS (
    SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
     WHERE observation.account_id = NEW.account_id
       AND observation.workflow_id = NEW.workflow_id
       AND observation.generation = NEW.deletion_target_generation
       AND observation.observation_kind = 'history_delete_confirmed'
  ) THEN
    RAISE EXCEPTION 'workflow history-delete evidence is missing';
  END IF;
  IF NEW.cleanup_state = 'absence_proved' AND OLD.cleanup_state <> 'absence_proved' AND NOT (
    EXISTS (SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
      JOIN jobs_workflow_cleanup_targets target
        ON target.account_id = observation.account_id
       AND target.workflow_id = observation.workflow_id
       AND target.generation = observation.generation
      WHERE observation.account_id = NEW.account_id
        AND observation.workflow_id = NEW.workflow_id
        AND observation.generation = NEW.deletion_target_generation
        AND observation.cleanup_fence = target.fence
        AND observation.cleanup_request_id = target.cleanup_request_id
        AND observation.observation_kind = 'absence_proved'
        AND observation.observed_execution_run_id = NEW.first_execution_run_id)
  ) THEN
    RAISE EXCEPTION 'workflow absence proof is incomplete';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_cleanup_transition_evidence
  ON jobs_workflow_executions;
CREATE TRIGGER trg_jobs_workflow_cleanup_transition_evidence
BEFORE UPDATE OF lifecycle_state, cleanup_state, deletion_target_generation,
  deletion_target_hmac_sha256
ON jobs_workflow_executions
FOR EACH ROW EXECUTE FUNCTION validate_jobs_workflow_cleanup_transition();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_account_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM jobs_workflow_commands command
     WHERE command.account_id = OLD.id
       AND (
         (command.first_request_started_at_ms IS NULL
           AND command.state NOT IN ('cancelled', 'rejected'))
         OR (command.first_request_started_at_ms IS NOT NULL AND NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_targets target
           JOIN jobs_workflow_cleanup_generations generation
             ON generation.account_id = target.account_id
            AND generation.generation = target.generation
            AND generation.target_set_hmac_sha256 = target.target_set_hmac_sha256
            WHERE target.account_id = command.account_id
              AND target.workflow_id = command.workflow_id
              AND target.target_state = 'absence_proved'
              AND generation.state = 'complete'
         ))
       )
  ) THEN
    RAISE EXCEPTION 'workflow external state is unresolved';
  END IF;
  RETURN OLD;
END;
$$;

DROP TRIGGER IF EXISTS trg_jobs_workflow_account_delete_guard ON accounts;
CREATE TRIGGER trg_jobs_workflow_account_delete_guard
BEFORE DELETE ON accounts
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_account_delete();

CREATE OR REPLACE FUNCTION guard_jobs_workflow_application_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM jobs_workflow_commands command
     WHERE command.account_id = OLD.account_id AND command.application_id = OLD.id
       AND (
         (command.first_request_started_at_ms IS NULL
           AND command.state NOT IN ('cancelled', 'rejected'))
         OR (command.first_request_started_at_ms IS NOT NULL AND NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_targets target
           JOIN jobs_workflow_cleanup_generations generation
             ON generation.account_id = target.account_id
            AND generation.generation = target.generation
            AND generation.target_set_hmac_sha256 = target.target_set_hmac_sha256
            WHERE target.account_id = command.account_id
              AND target.workflow_id = command.workflow_id
              AND target.target_state = 'absence_proved'
              AND generation.state = 'complete'
         ))
       )
  ) THEN
    RAISE EXCEPTION 'workflow external state is unresolved';
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
       AND (
         (command.first_request_started_at_ms IS NULL
           AND command.state NOT IN ('cancelled', 'rejected'))
         OR (command.first_request_started_at_ms IS NOT NULL AND NOT EXISTS (
           SELECT 1 FROM jobs_workflow_cleanup_targets target
           JOIN jobs_workflow_cleanup_generations generation
             ON generation.account_id = target.account_id
            AND generation.generation = target.generation
            AND generation.target_set_hmac_sha256 = target.target_set_hmac_sha256
            WHERE target.account_id = command.account_id
              AND target.workflow_id = command.workflow_id
              AND target.target_state = 'absence_proved'
              AND generation.state = 'complete'
         ))
       )
  ) THEN
    RAISE EXCEPTION 'workflow external state is unresolved';
  END IF;
  RETURN OLD;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_workflow_intervention_delete_guard ON jobs_interventions;
CREATE TRIGGER trg_jobs_workflow_intervention_delete_guard
BEFORE DELETE ON jobs_interventions
FOR EACH ROW EXECUTE FUNCTION guard_jobs_workflow_intervention_delete();

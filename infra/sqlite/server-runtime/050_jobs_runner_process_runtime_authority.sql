-- Process-scoped cloud-runner runtime authority.
--
-- A persistent runner volume proves storage identity, not which immutable
-- container/browser toolchain is executing. Deployment grants therefore
-- approve one exact runtime independently. The first signed instance claim
-- consumes a grant into an immutable worker+volume+epoch+process binding, and
-- each execution-lease fence records the exact server-loaded binding it used.

CREATE TABLE IF NOT EXISTS jobs_runner_process_runtime_grants (
  grant_id                         TEXT PRIMARY KEY
    CHECK(length(grant_id) BETWEEN 1 AND 128),
  token_sha256                     TEXT NOT NULL UNIQUE
    CHECK(length(token_sha256) = 64 AND lower(token_sha256) = token_sha256),
  expected_worker_id               TEXT NOT NULL
    CHECK(length(expected_worker_id) BETWEEN 1 AND 128),
  runtime_sha256                   TEXT NOT NULL
    CHECK(length(runtime_sha256) = 64 AND lower(runtime_sha256) = runtime_sha256),
  runner_image_sha256              TEXT NOT NULL
    CHECK(length(runner_image_sha256) = 64
      AND lower(runner_image_sha256) = runner_image_sha256),
  runner_build_id                  TEXT NOT NULL
    CHECK(length(runner_build_id) BETWEEN 1 AND 128),
  platform                         TEXT NOT NULL
    CHECK(platform IN ('linux', 'macos', 'windows')),
  architecture                     TEXT NOT NULL
    CHECK(architecture IN ('arm64', 'x86_64')),
  automation_bundle_sha256         TEXT NOT NULL
    CHECK(length(automation_bundle_sha256) = 64
      AND lower(automation_bundle_sha256) = automation_bundle_sha256),
  playwright_version               TEXT NOT NULL
    CHECK(length(playwright_version) BETWEEN 1 AND 80),
  chromium_revision                TEXT NOT NULL
    CHECK(length(chromium_revision) BETWEEN 1 AND 80),
  chromium_executable_sha256       TEXT NOT NULL
    CHECK(length(chromium_executable_sha256) = 64
      AND lower(chromium_executable_sha256) = chromium_executable_sha256),
  authorization_ref                TEXT NOT NULL
    CHECK(length(authorization_ref) BETWEEN 1 AND 1024),
  created_by                       TEXT NOT NULL
    CHECK(length(created_by) BETWEEN 1 AND 240),
  expires_at_ms                    INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  created_at_ms                    INTEGER NOT NULL CHECK(created_at_ms >= 0),
  CHECK(expires_at_ms > created_at_ms)
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_process_runtime_grants_expiry
  ON jobs_runner_process_runtime_grants(expires_at_ms, expected_worker_id);

-- Cancellation is append-only and applies only before consumption. A bound
-- process remains historical recovery evidence even if later authority is
-- withdrawn elsewhere.
CREATE TABLE IF NOT EXISTS jobs_runner_process_runtime_grant_revocations (
  grant_id                         TEXT PRIMARY KEY
    REFERENCES jobs_runner_process_runtime_grants(grant_id),
  reason                           TEXT NOT NULL CHECK(length(reason) BETWEEN 1 AND 240),
  authorization_ref                TEXT NOT NULL
    CHECK(length(authorization_ref) BETWEEN 1 AND 1024),
  revoked_by                       TEXT NOT NULL
    CHECK(length(revoked_by) BETWEEN 1 AND 240),
  revoked_at_ms                    INTEGER NOT NULL CHECK(revoked_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS jobs_runner_process_runtime_bindings (
  grant_id                         TEXT PRIMARY KEY
    REFERENCES jobs_runner_process_runtime_grants(grant_id),
  worker_id                        TEXT NOT NULL CHECK(length(worker_id) BETWEEN 1 AND 128),
  volume_id                        TEXT NOT NULL CHECK(length(volume_id) = 43),
  enrollment_epoch                 INTEGER NOT NULL CHECK(enrollment_epoch >= 1),
  process_instance_id              TEXT NOT NULL CHECK(length(process_instance_id) = 43),
  runtime_sha256                   TEXT NOT NULL
    CHECK(length(runtime_sha256) = 64 AND lower(runtime_sha256) = runtime_sha256),
  bound_at_ms                      INTEGER NOT NULL CHECK(bound_at_ms >= 0),
  FOREIGN KEY(volume_id, enrollment_epoch)
    REFERENCES jobs_runner_volume_keys(volume_id, enrollment_epoch),
  UNIQUE(worker_id, volume_id, enrollment_epoch, process_instance_id),
  UNIQUE(volume_id, enrollment_epoch, process_instance_id, runtime_sha256),
  UNIQUE(
    grant_id, worker_id, volume_id, enrollment_epoch, process_instance_id, runtime_sha256
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_runner_process_runtime_bindings_process
  ON jobs_runner_process_runtime_bindings(
    worker_id, volume_id, enrollment_epoch, process_instance_id
  );

CREATE TABLE IF NOT EXISTS jobs_execution_lease_process_runtime_bindings (
  -- Deliberately no lease/account FK: opaque runtime history survives normal
  -- customer-row deletion without preventing the execution-lease cascade.
  run_id                            TEXT NOT NULL CHECK(length(run_id) BETWEEN 1 AND 240),
  fence                             INTEGER NOT NULL CHECK(fence >= 1),
  runtime_grant_id                  TEXT NOT NULL
    REFERENCES jobs_runner_process_runtime_bindings(grant_id),
  worker_id                         TEXT NOT NULL CHECK(length(worker_id) BETWEEN 1 AND 128),
  volume_id                         TEXT NOT NULL CHECK(length(volume_id) = 43),
  enrollment_epoch                 INTEGER NOT NULL CHECK(enrollment_epoch >= 1),
  process_instance_id              TEXT NOT NULL CHECK(length(process_instance_id) = 43),
  runtime_sha256                   TEXT NOT NULL
    CHECK(length(runtime_sha256) = 64 AND lower(runtime_sha256) = runtime_sha256),
  bound_at_ms                      INTEGER NOT NULL CHECK(bound_at_ms >= 0),
  PRIMARY KEY(run_id, fence),
  FOREIGN KEY(
    runtime_grant_id, worker_id, volume_id, enrollment_epoch,
    process_instance_id, runtime_sha256
  )
    REFERENCES jobs_runner_process_runtime_bindings(
      grant_id, worker_id, volume_id, enrollment_epoch, process_instance_id, runtime_sha256
    )
);

CREATE INDEX IF NOT EXISTS idx_jobs_execution_lease_process_runtime_grant
  ON jobs_execution_lease_process_runtime_bindings(runtime_grant_id, run_id, fence);

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_process_runtime_grants_no_update
BEFORE UPDATE ON jobs_runner_process_runtime_grants BEGIN
  SELECT RAISE(ABORT, 'runner process runtime grant is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_process_runtime_grants_no_delete
BEFORE DELETE ON jobs_runner_process_runtime_grants BEGIN
  SELECT RAISE(ABORT, 'runner process runtime grant is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_process_runtime_grant_revocations_no_update
BEFORE UPDATE ON jobs_runner_process_runtime_grant_revocations BEGIN
  SELECT RAISE(ABORT, 'runner process runtime grant revocation is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_process_runtime_grant_revocations_no_delete
BEFORE DELETE ON jobs_runner_process_runtime_grant_revocations BEGIN
  SELECT RAISE(ABORT, 'runner process runtime grant revocation is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_process_runtime_bindings_no_update
BEFORE UPDATE ON jobs_runner_process_runtime_bindings BEGIN
  SELECT RAISE(ABORT, 'runner process runtime binding is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_runner_process_runtime_bindings_no_delete
BEFORE DELETE ON jobs_runner_process_runtime_bindings BEGIN
  SELECT RAISE(ABORT, 'runner process runtime binding is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_execution_lease_process_runtime_bindings_no_update
BEFORE UPDATE ON jobs_execution_lease_process_runtime_bindings BEGIN
  SELECT RAISE(ABORT, 'execution lease process runtime binding is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_execution_lease_process_runtime_bindings_no_delete
BEFORE DELETE ON jobs_execution_lease_process_runtime_bindings BEGIN
  SELECT RAISE(ABORT, 'execution lease process runtime binding is immutable');
END;

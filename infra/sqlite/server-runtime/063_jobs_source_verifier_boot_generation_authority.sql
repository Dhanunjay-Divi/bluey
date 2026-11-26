-- Target: SQLite
-- Original-source verifier boot-generation authority.
--
-- Phase 628 bound startup identity to (runtime_instance_id, instance_epoch) in
-- migration 062. A lost ready response therefore forced a restarted lifecycle to
-- collide with the stored startup row, and a late probe_failed heartbeat could
-- degrade an already-ready runtime.
--
-- This migration adds a server-assigned, strictly monotonic boot generation plus
-- naturally idempotent registration. Historical v1/v2 rows, audiences, protocol
-- ordinals, and serialized bytes are untouched: the v1 startup table is not
-- rebuilt or reinterpreted, and the new wire is a separate protocol id
-- ('source_verifier_boot' version 1, ordinal 11) rather than a version bump of
-- CHECK-pinned 'source_verifier_startup' version 1.
--
-- No boot, startup, or readiness row is seeded by this migration.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

-- Manifest-scoped registration of the new boot protocol. Immutable, mirroring
-- the startup protocol table added in 062.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_source_verifier_boot_protocols (
  manifest_sha256                 TEXT PRIMARY KEY,
  protocol_id                     TEXT NOT NULL DEFAULT 'source_verifier_boot'
    CHECK(protocol_id = 'source_verifier_boot'),
  protocol_version                INTEGER NOT NULL CHECK(protocol_version = 1),
  schema_sha256                   TEXT NOT NULL
    CHECK(length(schema_sha256) = 64 AND lower(schema_sha256) = schema_sha256
      AND schema_sha256 NOT GLOB '*[^0-9a-f]*'),
  ordinal                         INTEGER NOT NULL CHECK(ordinal = 11),
  UNIQUE(manifest_sha256, protocol_id),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_protocol_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_source_verifier_boot_protocols BEGIN
  SELECT RAISE(ABORT, 'managed source-verifier boot protocol is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_protocol_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_source_verifier_boot_protocols BEGIN
  SELECT RAISE(ABORT, 'managed source-verifier boot protocol is immutable');
END;

-- The single mutable row in this migration. It exists only so the server can
-- assign a strictly monotonic boot generation under one immediate transaction.
-- Clients never propose a generation.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_boot_generation_heads (
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  INTEGER NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  current_boot_generation         INTEGER NOT NULL
    CHECK(current_boot_generation BETWEEN 1 AND 9007199254740991),
  ready_boot_generation           INTEGER
    CHECK(ready_boot_generation IS NULL
      OR ready_boot_generation BETWEEN 1 AND 9007199254740991),
  updated_at_ms                   INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  PRIMARY KEY(runtime_instance_id, instance_epoch),
  CHECK(ready_boot_generation IS NULL
    OR ready_boot_generation <= current_boot_generation)
);

-- Generation heads may only advance. Readiness may only advance.
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_head_monotonic
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_generation_heads
WHEN NEW.runtime_instance_id <> OLD.runtime_instance_id
  OR NEW.instance_epoch <> OLD.instance_epoch
  OR NEW.current_boot_generation < OLD.current_boot_generation
  OR NEW.updated_at_ms < OLD.updated_at_ms
  OR (OLD.ready_boot_generation IS NOT NULL
      AND (NEW.ready_boot_generation IS NULL
           OR NEW.ready_boot_generation < OLD.ready_boot_generation))
BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot generation head is monotonic');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_head_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_generation_heads BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot generation head is permanent');
END;

-- Immutable, idempotent boot registrations. One row per assigned generation.
-- Exact replay is served from canonical_response_base64url; a reused
-- registration_request_id with a different payload violates the UNIQUE pair and
-- must be reported as a conflict by the server.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_boot_registrations (
  boot_id                         TEXT PRIMARY KEY CHECK(
    length(boot_id) BETWEEN 20 AND 128
    AND boot_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
  registration_request_id         TEXT NOT NULL CHECK(
    length(registration_request_id) BETWEEN 20 AND 128
    AND registration_request_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
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
  UNIQUE(runtime_instance_id, instance_epoch, boot_generation),
  UNIQUE(registration_request_id, registration_request_sha256),
  FOREIGN KEY(runtime_instance_id, instance_epoch)
    REFERENCES jobs_managed_cloud_source_verifier_boot_generation_heads(
      runtime_instance_id, instance_epoch
    ) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_managed_source_verifier_boot_registrations_instance
  ON jobs_managed_cloud_source_verifier_boot_registrations(
    runtime_instance_id, instance_epoch, boot_generation DESC
  );

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_registration_no_update
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_registrations BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot registration is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_registration_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_registrations BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot registration is immutable');
END;

-- A registration may never claim a generation above the assigned head.
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_registration_generation
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_registrations
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
   WHERE head.runtime_instance_id = NEW.runtime_instance_id
     AND head.instance_epoch = NEW.instance_epoch
     AND head.current_boot_generation = NEW.boot_generation
) BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot registration generation is not the assigned head');
END;

-- Immutable v3 startup authority bound to a boot generation. The v1 table from
-- migration 062 is intentionally left untouched so historical bytes and
-- semantics are preserved exactly.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_boot_startup_authorities (
  startup_request_id              TEXT PRIMARY KEY CHECK(
    length(startup_request_id) BETWEEN 20 AND 128
    AND startup_request_id NOT GLOB '*[^A-Za-z0-9_-]*'
  ),
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
  degraded_heartbeat_sequence     INTEGER NOT NULL
    CHECK(degraded_heartbeat_sequence BETWEEN 1 AND 9007199254740990),
  ready_heartbeat_sequence        INTEGER NOT NULL
    CHECK(ready_heartbeat_sequence = degraded_heartbeat_sequence + 1),
  probe_result                    TEXT NOT NULL CHECK(probe_result IN ('idle', 'leaseable')),
  startup_authority_sha256        TEXT NOT NULL UNIQUE
    CHECK(length(startup_authority_sha256) = 64
      AND lower(startup_authority_sha256) = startup_authority_sha256
      AND startup_authority_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_response_base64url    TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256                 TEXT NOT NULL UNIQUE
    CHECK(length(response_sha256) = 64 AND lower(response_sha256) = response_sha256
      AND response_sha256 NOT GLOB '*[^0-9a-f]*'),
  startup_at_ms                   INTEGER NOT NULL
    CHECK(startup_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(runtime_instance_id, instance_epoch, boot_generation),
  UNIQUE(startup_request_id, request_sha256),
  FOREIGN KEY(boot_id)
    REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id)
    ON DELETE RESTRICT
);

CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_startup_no_update
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_startup_authorities BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup authority is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_startup_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_startup_authorities BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup authority is immutable');
END;

-- Startup must bind the exact registered boot and its assigned generation.
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_startup_binding
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations registration
   WHERE registration.boot_id = NEW.boot_id
     AND registration.runtime_instance_id = NEW.runtime_instance_id
     AND registration.instance_epoch = NEW.instance_epoch
     AND registration.boot_generation = NEW.boot_generation
     AND registration.worker_id = NEW.worker_id
) BEGIN
  SELECT RAISE(ABORT, 'source-verifier boot startup binding is invalid');
END;

-- Startup may never regress the recorded ready generation.
CREATE TRIGGER IF NOT EXISTS trg_jobs_managed_source_verifier_boot_startup_not_stale
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

INSERT OR IGNORE INTO bluey_data_migrations(name)
VALUES ('jobs-source-verifier-boot-generation-authority-v1');
COMMIT;
PRAGMA foreign_keys = ON;

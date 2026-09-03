-- Target: PostgreSQL
-- Original-source verifier boot-generation authority.
--
-- Paired with SQLite migration
-- 063_jobs_source_verifier_boot_generation_authority.sql. Semantics must remain
-- equivalent across both engines.
--
-- Phase 628 bound startup identity to (runtime_instance_id, instance_epoch) in
-- PostgreSQL migration 040. A lost ready response therefore forced a restarted
-- lifecycle to collide with the stored startup row, and a late probe_failed
-- heartbeat could degrade an already-ready runtime.
--
-- This migration adds a server-assigned, strictly monotonic boot generation plus
-- naturally idempotent registration. Historical v1/v2 rows, audiences, protocol
-- ordinals, and serialized bytes are untouched: the v1 startup table is not
-- rebuilt or reinterpreted, and the new wire is a separate protocol id
-- ('source_verifier_boot' version 1, ordinal 11) rather than a version bump of
-- CHECK-pinned 'source_verifier_startup' version 1.
--
-- Lock order is preserved: callers continue to take operational hold H, managed
-- release registry M, discovery account D, then assignment. The boot generation
-- head is locked after M and before D.
--
-- No boot, startup, or readiness row is seeded by this migration.
BEGIN;

CREATE TABLE IF NOT EXISTS jobs_managed_cloud_manifest_source_verifier_boot_protocols (
  manifest_sha256                 TEXT PRIMARY KEY,
  protocol_id                     TEXT NOT NULL DEFAULT 'source_verifier_boot'
    CHECK(protocol_id = 'source_verifier_boot'),
  protocol_version                BIGINT NOT NULL CHECK(protocol_version = 1),
  schema_sha256                   TEXT NOT NULL
    CHECK(schema_sha256 ~ '^[0-9a-f]{64}$'),
  ordinal                         BIGINT NOT NULL CHECK(ordinal = 11),
  UNIQUE(manifest_sha256, protocol_id),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_managed_cloud_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_protocol_immutable()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'managed source-verifier boot protocol is immutable';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_protocol_no_update
  ON jobs_managed_cloud_manifest_source_verifier_boot_protocols;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_protocol_no_update
BEFORE UPDATE ON jobs_managed_cloud_manifest_source_verifier_boot_protocols
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_protocol_immutable();

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_protocol_no_delete
  ON jobs_managed_cloud_manifest_source_verifier_boot_protocols;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_protocol_no_delete
BEFORE DELETE ON jobs_managed_cloud_manifest_source_verifier_boot_protocols
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_protocol_immutable();

-- The single mutable row in this migration. It exists only so the server can
-- assign a strictly monotonic boot generation. Clients never propose one.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_boot_generation_heads (
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  BIGINT NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  current_boot_generation         BIGINT NOT NULL
    CHECK(current_boot_generation BETWEEN 1 AND 9007199254740991),
  ready_boot_generation           BIGINT
    CHECK(ready_boot_generation IS NULL
      OR ready_boot_generation BETWEEN 1 AND 9007199254740991),
  updated_at_ms                   BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  PRIMARY KEY(runtime_instance_id, instance_epoch),
  CHECK(ready_boot_generation IS NULL
    OR ready_boot_generation <= current_boot_generation)
);

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_head_monotonic()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.runtime_instance_id <> OLD.runtime_instance_id
     OR NEW.instance_epoch <> OLD.instance_epoch
     OR NEW.current_boot_generation < OLD.current_boot_generation
     OR NEW.updated_at_ms < OLD.updated_at_ms
     OR (OLD.ready_boot_generation IS NOT NULL
         AND (NEW.ready_boot_generation IS NULL
              OR NEW.ready_boot_generation < OLD.ready_boot_generation)) THEN
    RAISE EXCEPTION 'source-verifier boot generation head is monotonic';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_head_monotonic
  ON jobs_managed_cloud_source_verifier_boot_generation_heads;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_head_monotonic
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_generation_heads
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_head_monotonic();

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_head_permanent()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'source-verifier boot generation head is permanent';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_head_no_delete
  ON jobs_managed_cloud_source_verifier_boot_generation_heads;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_head_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_generation_heads
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_head_permanent();

-- Immutable, idempotent boot registrations. One row per assigned generation.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_boot_registrations (
  boot_id                         TEXT PRIMARY KEY
    CHECK(boot_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  registration_request_id         TEXT NOT NULL
    CHECK(registration_request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  BIGINT NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  boot_generation                 BIGINT NOT NULL
    CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  worker_id                       TEXT NOT NULL,
  runtime_identity_sha256         TEXT NOT NULL
    CHECK(runtime_identity_sha256 ~ '^[0-9a-f]{64}$'),
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  component_id                    TEXT NOT NULL CHECK(component_id = 'jobs-workflows'),
  role                            TEXT NOT NULL CHECK(role = 'original_source_verifier'),
  registration_request_sha256     TEXT NOT NULL
    CHECK(registration_request_sha256 ~ '^[0-9a-f]{64}$'),
  registration_authority_sha256   TEXT NOT NULL UNIQUE
    CHECK(registration_authority_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_response_base64url    TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256                 TEXT NOT NULL UNIQUE
    CHECK(response_sha256 ~ '^[0-9a-f]{64}$'),
  registered_at_ms                BIGINT NOT NULL
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

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_registration_immutable()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'source-verifier boot registration is immutable';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_registration_no_update
  ON jobs_managed_cloud_source_verifier_boot_registrations;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_registration_no_update
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_registrations
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_registration_immutable();

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_registration_no_delete
  ON jobs_managed_cloud_source_verifier_boot_registrations;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_registration_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_registrations
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_registration_immutable();

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_registration_generation()
RETURNS TRIGGER AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
     WHERE head.runtime_instance_id = NEW.runtime_instance_id
       AND head.instance_epoch = NEW.instance_epoch
       AND head.current_boot_generation = NEW.boot_generation
  ) THEN
    RAISE EXCEPTION 'source-verifier boot registration generation is not the assigned head';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_registration_generation
  ON jobs_managed_cloud_source_verifier_boot_registrations;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_registration_generation
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_registrations
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_registration_generation();

-- Immutable v3 startup authority bound to a boot generation. The v1 table from
-- PostgreSQL migration 040 is intentionally left untouched.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_source_verifier_boot_startup_authorities (
  startup_request_id              TEXT PRIMARY KEY
    CHECK(startup_request_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  boot_id                         TEXT NOT NULL UNIQUE,
  runtime_instance_id             TEXT NOT NULL,
  instance_epoch                  BIGINT NOT NULL
    CHECK(instance_epoch BETWEEN 1 AND 9007199254740991),
  boot_generation                 BIGINT NOT NULL
    CHECK(boot_generation BETWEEN 1 AND 9007199254740991),
  worker_id                       TEXT NOT NULL,
  request_sha256                  TEXT NOT NULL
    CHECK(request_sha256 ~ '^[0-9a-f]{64}$'),
  degraded_heartbeat_sequence     BIGINT NOT NULL
    CHECK(degraded_heartbeat_sequence BETWEEN 1 AND 9007199254740990),
  ready_heartbeat_sequence        BIGINT NOT NULL
    CHECK(ready_heartbeat_sequence = degraded_heartbeat_sequence + 1),
  probe_result                    TEXT NOT NULL CHECK(probe_result IN ('idle', 'leaseable')),
  startup_authority_sha256        TEXT NOT NULL UNIQUE
    CHECK(startup_authority_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_response_base64url    TEXT NOT NULL CHECK(length(canonical_response_base64url) > 0),
  response_sha256                 TEXT NOT NULL UNIQUE
    CHECK(response_sha256 ~ '^[0-9a-f]{64}$'),
  startup_at_ms                   BIGINT NOT NULL
    CHECK(startup_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(runtime_instance_id, instance_epoch, boot_generation),
  UNIQUE(startup_request_id, request_sha256),
  FOREIGN KEY(boot_id)
    REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id)
    ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_startup_immutable()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'source-verifier boot startup authority is immutable';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_startup_no_update
  ON jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_no_update
BEFORE UPDATE ON jobs_managed_cloud_source_verifier_boot_startup_authorities
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_startup_immutable();

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_startup_no_delete
  ON jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_no_delete
BEFORE DELETE ON jobs_managed_cloud_source_verifier_boot_startup_authorities
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_startup_immutable();

CREATE OR REPLACE FUNCTION jobs_managed_source_verifier_boot_startup_guard()
RETURNS TRIGGER AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations registration
     WHERE registration.boot_id = NEW.boot_id
       AND registration.runtime_instance_id = NEW.runtime_instance_id
       AND registration.instance_epoch = NEW.instance_epoch
       AND registration.boot_generation = NEW.boot_generation
       AND registration.worker_id = NEW.worker_id
  ) THEN
    RAISE EXCEPTION 'source-verifier boot startup binding is invalid';
  END IF;
  IF EXISTS (
    SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
     WHERE head.runtime_instance_id = NEW.runtime_instance_id
       AND head.instance_epoch = NEW.instance_epoch
       AND head.ready_boot_generation IS NOT NULL
       AND head.ready_boot_generation > NEW.boot_generation
  ) THEN
    RAISE EXCEPTION 'source-verifier boot startup generation is stale';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_source_verifier_boot_startup_guard
  ON jobs_managed_cloud_source_verifier_boot_startup_authorities;
CREATE TRIGGER trg_jobs_managed_source_verifier_boot_startup_guard
BEFORE INSERT ON jobs_managed_cloud_source_verifier_boot_startup_authorities
FOR EACH ROW EXECUTE FUNCTION jobs_managed_source_verifier_boot_startup_guard();

INSERT INTO bluey_data_migrations(name)
VALUES ('jobs-source-verifier-boot-generation-authority-v1')
ON CONFLICT DO NOTHING;
COMMIT;

-- Target: SQLite
-- Boot-generation fencing for original-source-verifier runtime heartbeats.
--
-- Migration 063 established a server-assigned monotonic boot generation and
-- recorded the committed ready generation on
-- jobs_managed_cloud_source_verifier_boot_generation_heads. This migration
-- projects that fence onto the heartbeat table so a generic liveness heartbeat,
-- or a stale or already-completed failure report, cannot degrade a runtime whose
-- readiness was committed under a generation that is still current.
--
-- Deliberately schema-free: the heartbeat table is not widened at all. An
-- earlier draft widened the heartbeat table with a new generation column, which
-- is NOT replay-safe in SQLite because that form of table alteration has no
-- if-not-exists variant, so re-running the migration chain failed with a
-- duplicate column error. Deriving the fence from the generation head instead
-- keeps this migration idempotent under replay.
--
-- How a legitimate new boot is distinguished from a stale one, without storing a
-- generation on the heartbeat row: registration advances
-- current_boot_generation while ready_boot_generation stays where it was. So
-- current > ready means a newer boot has registered and is entitled to publish
-- its own degraded startup heartbeat, whereas current == ready means no newer
-- boot exists and any non-ready heartbeat is stale or generic.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

DROP TRIGGER IF EXISTS trg_jobs_managed_osv_heartbeat_ready_fence;
CREATE TRIGGER trg_jobs_managed_osv_heartbeat_ready_fence
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
WHEN NEW.health_state <> 'ready'
  AND EXISTS (
    SELECT 1
      FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
     WHERE head.runtime_instance_id = NEW.runtime_instance_id
       AND head.instance_epoch = NEW.instance_epoch
       AND head.ready_boot_generation IS NOT NULL
       AND head.current_boot_generation <= head.ready_boot_generation
  )
BEGIN
  SELECT RAISE(ABORT, 'source-verifier committed readiness cannot be lowered');
END;

INSERT OR IGNORE INTO bluey_data_migrations(name)
VALUES ('jobs-source-verifier-heartbeat-boot-generation-fence-v1');
COMMIT;
PRAGMA foreign_keys = ON;

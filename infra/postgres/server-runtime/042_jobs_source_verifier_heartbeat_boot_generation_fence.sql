-- Target: PostgreSQL
-- Boot-generation fencing for original-source-verifier runtime heartbeats.
--
-- Paired with SQLite migration
-- 064_jobs_source_verifier_heartbeat_boot_generation_fence.sql. Semantics must
-- remain equivalent across both engines.
--
-- Migration 041 established a server-assigned monotonic boot generation and
-- recorded the committed ready generation on
-- jobs_managed_cloud_source_verifier_boot_generation_heads. This migration
-- projects that fence onto the heartbeat table so a generic liveness heartbeat,
-- or a stale or already-completed failure report, cannot degrade a runtime whose
-- readiness was committed under a generation that is still current.
--
-- Deliberately schema-free, matching the SQLite pair: the heartbeat table is not
-- widened at all, which keeps both migrations idempotent under replay.
--
-- Registration advances current_boot_generation while ready_boot_generation
-- stays where it was, so current > ready means a newer boot has registered and
-- may publish its own degraded startup heartbeat, whereas current == ready means
-- no newer boot exists and any non-ready heartbeat is stale or generic.
BEGIN;

CREATE OR REPLACE FUNCTION jobs_managed_osv_heartbeat_ready_fence()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.health_state <> 'ready'
     AND EXISTS (
       SELECT 1
         FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
        WHERE head.runtime_instance_id = NEW.runtime_instance_id
          AND head.instance_epoch = NEW.instance_epoch
          AND head.ready_boot_generation IS NOT NULL
          AND head.current_boot_generation <= head.ready_boot_generation
     ) THEN
    RAISE EXCEPTION 'source-verifier committed readiness cannot be lowered';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_managed_osv_heartbeat_ready_fence
  ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats;
CREATE TRIGGER trg_jobs_managed_osv_heartbeat_ready_fence
BEFORE UPDATE ON jobs_managed_cloud_original_source_verifier_runtime_heartbeats
FOR EACH ROW EXECUTE FUNCTION jobs_managed_osv_heartbeat_ready_fence();

INSERT INTO bluey_data_migrations(name)
VALUES ('jobs-source-verifier-heartbeat-boot-generation-fence-v1')
ON CONFLICT DO NOTHING;
COMMIT;

-- Target: Postgres
-- Keep the zero default after backfill so the pre-revision server can be
-- rolled back without failing inserts that omit updated_at_ms.

ALTER TABLE cloud_context_artifacts
  ADD COLUMN IF NOT EXISTS updated_at_ms BIGINT DEFAULT 0;

ALTER TABLE cloud_context_artifacts
  ALTER COLUMN updated_at_ms SET DEFAULT 0;

UPDATE cloud_context_artifacts
SET updated_at_ms = created_at_ms
WHERE updated_at_ms IS NULL OR updated_at_ms < created_at_ms;

ALTER TABLE cloud_context_artifacts
  ALTER COLUMN updated_at_ms SET NOT NULL;

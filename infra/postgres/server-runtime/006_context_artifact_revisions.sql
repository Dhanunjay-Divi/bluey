ALTER TABLE cloud_context_artifacts
  ADD COLUMN IF NOT EXISTS updated_at_ms BIGINT;

UPDATE cloud_context_artifacts
SET updated_at_ms = created_at_ms
WHERE updated_at_ms IS NULL OR updated_at_ms < created_at_ms;

ALTER TABLE cloud_context_artifacts
  ALTER COLUMN updated_at_ms SET NOT NULL;

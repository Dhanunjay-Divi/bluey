-- Target: PostgreSQL
-- Persist bounded semantic-row quarantine evidence for global discovery runs.

ALTER TABLE jobs_global_ingestion_runs
  ADD COLUMN IF NOT EXISTS rejected_rows BIGINT NOT NULL DEFAULT 0;

ALTER TABLE jobs_global_ingestion_runs
  ADD COLUMN IF NOT EXISTS rejection_summary_json TEXT NOT NULL DEFAULT '{}';

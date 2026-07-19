-- Target: PostgreSQL only.
-- Migration 009 is reserved for the Jobs discovery board-owner boundary.
-- Provider usage provenance is a distinct additive migration so databases
-- that already applied 008 receive the settlement trust field safely.

ALTER TABLE jobs_provider_cost_holds
  ADD COLUMN IF NOT EXISTS usage_provenance TEXT NOT NULL DEFAULT 'missing';

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
      FROM pg_constraint
     WHERE conname = 'jobs_provider_cost_holds_usage_provenance'
       AND conrelid = 'jobs_provider_cost_holds'::regclass
  ) THEN
    ALTER TABLE jobs_provider_cost_holds
      ADD CONSTRAINT jobs_provider_cost_holds_usage_provenance
      CHECK (usage_provenance IN ('exact', 'estimated', 'missing')) NOT VALID;
  END IF;
END $$;

ALTER TABLE jobs_provider_cost_holds
  VALIDATE CONSTRAINT jobs_provider_cost_holds_usage_provenance;

-- Target: Postgres. Paired additive successor to SQLite 066; no guessed legacy backfill.
BEGIN;
ALTER TABLE jobs_attempt_reservations
  ADD COLUMN IF NOT EXISTS canonical_employer_id TEXT,
  ADD COLUMN IF NOT EXISTS canonical_employer_domain TEXT,
  ADD COLUMN IF NOT EXISTS employer_attestation_sha256 TEXT;

CREATE OR REPLACE FUNCTION jobs_attempt_employer_binding_valid(
  employer_id TEXT, employer_domain TEXT, attestation_sha256 TEXT
) RETURNS BOOLEAN LANGUAGE SQL IMMUTABLE AS $fn$
  SELECT (employer_id IS NULL AND employer_domain IS NULL AND attestation_sha256 IS NULL)
    OR COALESCE(
      employer_id IS NOT NULL AND employer_domain IS NOT NULL AND attestation_sha256 IS NOT NULL
      AND employer_id COLLATE "C" ~ '^[A-Za-z0-9_./:-]{1,240}$'
      AND length(employer_domain) BETWEEN 1 AND 253
      AND employer_domain COLLATE "C" ~
        '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$'
      AND employer_domain <> 'localhost' AND employer_domain NOT LIKE '%.localhost'
      -- Reject all URL IPv4 forms, including numeric and hex-numeric terminal labels.
      AND employer_domain COLLATE "C" !~ '\.([0-9]+|0x[0-9a-f]*)$'
      AND attestation_sha256 COLLATE "C" ~ '^[0-9a-f]{64}$',
      FALSE
    );
$fn$;
ALTER TABLE jobs_attempt_reservations
  DROP CONSTRAINT IF EXISTS jobs_attempt_employer_binding_shape;
ALTER TABLE jobs_attempt_reservations
  ADD CONSTRAINT jobs_attempt_employer_binding_shape CHECK (
    jobs_attempt_employer_binding_valid(
      canonical_employer_id, canonical_employer_domain, employer_attestation_sha256
    )
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_active_employer_id
  ON jobs_attempt_reservations(account_id, canonical_employer_id)
  WHERE canonical_employer_id IS NOT NULL
    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted');
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_active_employer_domain
  ON jobs_attempt_reservations(account_id, canonical_employer_domain)
  WHERE canonical_employer_domain IS NOT NULL
    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted');

CREATE OR REPLACE FUNCTION jobs_attempt_employer_identity_guard()
RETURNS TRIGGER LANGUAGE plpgsql AS $fn$
BEGIN
  IF NOT jobs_attempt_employer_binding_valid(
    NEW.canonical_employer_id, NEW.canonical_employer_domain, NEW.employer_attestation_sha256
  ) THEN
    RAISE EXCEPTION 'invalid attempt employer identity binding';
  END IF;
  IF TG_OP = 'INSERT' THEN
    IF NEW.status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')
      AND NEW.canonical_employer_id IS NULL THEN
      RAISE EXCEPTION 'active attempt requires employer identity binding';
    END IF;
  ELSE
    IF OLD.status = 'submitted' AND NEW.status IS DISTINCT FROM 'submitted' THEN
      RAISE EXCEPTION 'submitted attempt status is irreversible';
    END IF;
    IF OLD.status = 'side_effect_unknown'
      AND NEW.status NOT IN ('side_effect_unknown', 'submitted', 'released') THEN
      RAISE EXCEPTION 'unknown attempt requires reconciliation before reentry';
    END IF;
    IF OLD.canonical_employer_id IS NOT NULL AND (
      NEW.canonical_employer_id IS DISTINCT FROM OLD.canonical_employer_id
      OR NEW.canonical_employer_domain IS DISTINCT FROM OLD.canonical_employer_domain
    ) THEN
      RAISE EXCEPTION 'attempt employer identity is immutable';
    END IF;
    IF OLD.employer_attestation_sha256 IS NOT NULL
      AND NEW.employer_attestation_sha256 IS DISTINCT FROM OLD.employer_attestation_sha256
      AND NOT (OLD.status = 'released' AND NEW.status = 'reserved'
        AND NEW.canonical_employer_id IS NOT DISTINCT FROM OLD.canonical_employer_id
        AND NEW.canonical_employer_domain IS NOT DISTINCT FROM OLD.canonical_employer_domain) THEN
      RAISE EXCEPTION 'attempt employer attestation is immutable';
    END IF;
    IF NEW.canonical_employer_id IS NULL
      AND NEW.status NOT IN ('released', 'side_effect_unknown', 'submitted') THEN
      RAISE EXCEPTION 'active attempt requires employer identity binding';
    END IF;
  END IF;
  RETURN NEW;
END;
$fn$;
DROP TRIGGER IF EXISTS trg_jobs_attempt_employer_identity ON jobs_attempt_reservations;
CREATE TRIGGER trg_jobs_attempt_employer_identity
  BEFORE INSERT OR UPDATE ON jobs_attempt_reservations
  FOR EACH ROW EXECUTE FUNCTION jobs_attempt_employer_identity_guard();
COMMIT;

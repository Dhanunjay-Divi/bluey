-- Target: SQLite. Additive employer binding; historical reservation rows stay unchanged.
-- The runtime runner installs nullable TEXT columns with ensure_column before this replayed SQL.
BEGIN IMMEDIATE;
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_active_employer_id
  ON jobs_attempt_reservations(account_id, canonical_employer_id)
  WHERE canonical_employer_id IS NOT NULL
    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted');
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_active_employer_domain
  ON jobs_attempt_reservations(account_id, canonical_employer_domain)
  WHERE canonical_employer_domain IS NOT NULL
    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted');

CREATE TRIGGER IF NOT EXISTS trg_jobs_attempt_employer_identity_insert
BEFORE INSERT ON jobs_attempt_reservations
BEGIN
  SELECT RAISE(ABORT, 'invalid attempt employer identity binding')
  WHERE (
    (NEW.canonical_employer_id IS NULL AND NEW.canonical_employer_domain IS NULL
      AND NEW.employer_attestation_sha256 IS NULL)
    OR (
      NEW.canonical_employer_id IS NOT NULL AND NEW.canonical_employer_domain IS NOT NULL
      AND NEW.employer_attestation_sha256 IS NOT NULL
      AND length(NEW.canonical_employer_id) BETWEEN 1 AND 240
      AND length(CAST(NEW.canonical_employer_id AS BLOB)) = length(NEW.canonical_employer_id)
      AND NEW.canonical_employer_id NOT GLOB '*[^A-Za-z0-9_./:-]*'
      AND length(NEW.canonical_employer_domain) BETWEEN 1 AND 253
      AND length(CAST(NEW.canonical_employer_domain AS BLOB)) = length(NEW.canonical_employer_domain)
      AND NEW.canonical_employer_domain NOT GLOB '*[^a-z0-9.-]*'
      AND instr(NEW.canonical_employer_domain, '.') > 0
      AND NEW.canonical_employer_domain <> 'localhost'
      AND NEW.canonical_employer_domain NOT LIKE '%.localhost'
      AND length(NEW.employer_attestation_sha256) = 64
      AND length(CAST(NEW.employer_attestation_sha256 AS BLOB)) = 64
      AND NEW.employer_attestation_sha256 NOT GLOB '*[^0-9a-f]*'
    )
  ) IS NOT TRUE;
  -- The final numeric/hex-numeric label is a WHATWG IPv4 candidate, not a URL domain.
  SELECT RAISE(ABORT, 'invalid attempt employer identity binding')
  WHERE NEW.canonical_employer_domain IS NOT NULL AND EXISTS (
    WITH RECURSIVE labels(label, remaining, ordinal) AS (
      SELECT '', NEW.canonical_employer_domain || '.', 0
      UNION ALL
      SELECT substr(remaining, 1, instr(remaining, '.') - 1),
             substr(remaining, instr(remaining, '.') + 1), ordinal + 1
        FROM labels WHERE remaining <> ''
    )
    SELECT 1 FROM labels WHERE ordinal > 0 AND (
      length(label) NOT BETWEEN 1 AND 63 OR substr(label, 1, 1) = '-'
      OR substr(label, -1) = '-'
      OR (remaining = '' AND (
        label NOT GLOB '*[^0-9]*'
        OR (substr(label, 1, 2) = '0x' AND substr(label, 3) NOT GLOB '*[^0-9a-f]*')
      ))
    )
  );
  SELECT RAISE(ABORT, 'active attempt requires employer identity binding')
  WHERE NEW.status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')
    AND NEW.canonical_employer_id IS NULL;
END;

-- Refresh only the guard definition on replay; never rewrite historical rows.
DROP TRIGGER IF EXISTS trg_jobs_attempt_employer_identity_update;
CREATE TRIGGER IF NOT EXISTS trg_jobs_attempt_employer_identity_update
BEFORE UPDATE ON jobs_attempt_reservations
BEGIN
  SELECT RAISE(ABORT, 'invalid attempt employer identity binding')
  WHERE (
    (NEW.canonical_employer_id IS NULL AND NEW.canonical_employer_domain IS NULL
      AND NEW.employer_attestation_sha256 IS NULL)
    OR (
      NEW.canonical_employer_id IS NOT NULL AND NEW.canonical_employer_domain IS NOT NULL
      AND NEW.employer_attestation_sha256 IS NOT NULL
      AND length(NEW.canonical_employer_id) BETWEEN 1 AND 240
      AND length(CAST(NEW.canonical_employer_id AS BLOB)) = length(NEW.canonical_employer_id)
      AND NEW.canonical_employer_id NOT GLOB '*[^A-Za-z0-9_./:-]*'
      AND length(NEW.canonical_employer_domain) BETWEEN 1 AND 253
      AND length(CAST(NEW.canonical_employer_domain AS BLOB)) = length(NEW.canonical_employer_domain)
      AND NEW.canonical_employer_domain NOT GLOB '*[^a-z0-9.-]*'
      AND instr(NEW.canonical_employer_domain, '.') > 0
      AND NEW.canonical_employer_domain <> 'localhost'
      AND NEW.canonical_employer_domain NOT LIKE '%.localhost'
      AND length(NEW.employer_attestation_sha256) = 64
      AND length(CAST(NEW.employer_attestation_sha256 AS BLOB)) = 64
      AND NEW.employer_attestation_sha256 NOT GLOB '*[^0-9a-f]*'
    )
  ) IS NOT TRUE;
  -- The final numeric/hex-numeric label is a WHATWG IPv4 candidate, not a URL domain.
  SELECT RAISE(ABORT, 'invalid attempt employer identity binding')
  WHERE NEW.canonical_employer_domain IS NOT NULL AND EXISTS (
    WITH RECURSIVE labels(label, remaining, ordinal) AS (
      SELECT '', NEW.canonical_employer_domain || '.', 0
      UNION ALL
      SELECT substr(remaining, 1, instr(remaining, '.') - 1),
             substr(remaining, instr(remaining, '.') + 1), ordinal + 1
        FROM labels WHERE remaining <> ''
    )
    SELECT 1 FROM labels WHERE ordinal > 0 AND (
      length(label) NOT BETWEEN 1 AND 63 OR substr(label, 1, 1) = '-'
      OR substr(label, -1) = '-'
      OR (remaining = '' AND (
        label NOT GLOB '*[^0-9]*'
        OR (substr(label, 1, 2) = '0x' AND substr(label, 3) NOT GLOB '*[^0-9a-f]*')
      ))
    )
  );
  SELECT RAISE(ABORT, 'attempt employer identity is immutable')
  WHERE OLD.canonical_employer_id IS NOT NULL AND (
    NEW.canonical_employer_id IS NOT OLD.canonical_employer_id
    OR NEW.canonical_employer_domain IS NOT OLD.canonical_employer_domain
  );
  SELECT RAISE(ABORT, 'attempt employer attestation is immutable')
  WHERE OLD.employer_attestation_sha256 IS NOT NULL
    AND NEW.employer_attestation_sha256 IS NOT OLD.employer_attestation_sha256
    AND NOT (OLD.status = 'released' AND NEW.status = 'reserved'
      AND NEW.canonical_employer_id IS OLD.canonical_employer_id
      AND NEW.canonical_employer_domain IS OLD.canonical_employer_domain);
  -- Preserve late terminal/reconciliation recording, never unbound execution reentry.
  SELECT RAISE(ABORT, 'submitted attempt status is irreversible')
  WHERE OLD.status = 'submitted' AND NEW.status IS NOT 'submitted';
  SELECT RAISE(ABORT, 'unknown attempt requires reconciliation before reentry')
  WHERE OLD.status = 'side_effect_unknown'
    AND NEW.status NOT IN ('side_effect_unknown', 'submitted', 'released');
  SELECT RAISE(ABORT, 'active attempt requires employer identity binding')
  WHERE NEW.canonical_employer_id IS NULL
    AND NEW.status NOT IN ('released', 'side_effect_unknown', 'submitted');
END;
COMMIT;

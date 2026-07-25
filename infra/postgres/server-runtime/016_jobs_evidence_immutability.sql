-- Target: Postgres
-- Frozen candidate evidence is append-only.
--
-- Account and Career Track deletion may still cascade for privacy. Existing
-- revisions and claim rows cannot be rewritten after they authorize an
-- employer-facing packet.

CREATE OR REPLACE FUNCTION bluey_jobs_reject_evidence_update()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'Bluey Jobs candidate evidence is immutable';
END;
$$;

DROP TRIGGER IF EXISTS jobs_profile_evidence_revisions_immutable
  ON jobs_profile_evidence_revisions;
CREATE TRIGGER jobs_profile_evidence_revisions_immutable
BEFORE UPDATE ON jobs_profile_evidence_revisions
FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_reject_evidence_update();

DROP TRIGGER IF EXISTS jobs_resume_claim_evidence_immutable
  ON jobs_resume_claim_evidence;
CREATE TRIGGER jobs_resume_claim_evidence_immutable
BEFORE UPDATE ON jobs_resume_claim_evidence
FOR EACH ROW
EXECUTE FUNCTION bluey_jobs_reject_evidence_update();

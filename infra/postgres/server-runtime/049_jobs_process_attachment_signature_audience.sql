-- Target: Postgres
-- FAIL-CLOSED OPERATOR MARKER ONLY.
--
-- This filename reserves canonical 049 in the operator migration inventory.
-- It intentionally contains no forward DDL: only the Rust guarded transaction
-- may acquire the signature lock, validate catalog/state, apply the compiled
-- forward body, revalidate, and record canonical history atomically.
-- Raw psql/script execution of this file is forbidden and must fail closed.
DO $phase657_operator_marker$
BEGIN
  RAISE EXCEPTION '049_jobs_process_attachment_signature_audience.sql requires the guarded Rust migration driver';
END;
$phase657_operator_marker$;

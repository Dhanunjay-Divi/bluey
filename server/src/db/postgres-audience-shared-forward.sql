-- Private Phase657 fragment, not a standalone operator migration.
-- The Rust owner supplies schema validation, transaction/lock and ledger boundaries.
DO $phase657_forward$
DECLARE
  state_count bigint;
  tag text;
  target_oid oid := pg_catalog.to_regclass('public.jobs_managed_cloud_signature_sets');
  owner_name text;
BEGIN
  SELECT count(*), min(state) INTO state_count, tag
    FROM pg_temp.phase657_audience_guard_result;
  IF state_count <> 1 OR tag IS NULL OR tag NOT IN ('legacy13', 'successor14') THEN
    RAISE EXCEPTION 'signature audience guard result invalid';
  END IF;
  IF target_oid IS NULL THEN
    RAISE EXCEPTION 'signature audience target unavailable';
  END IF;
  IF tag = 'successor14' THEN
    -- Guard already verified the exact 14/state pairing under the held lock.
    RETURN;
  END IF;
  IF pg_catalog.to_regclass('public.jobs_signature_audience_upgrade_state') IS NOT NULL THEN
    RAISE EXCEPTION 'signature audience state unexpectedly present';
  END IF;
  SELECT pg_catalog.pg_get_userbyid(c.relowner) INTO owner_name
    FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
   WHERE c.oid=target_oid AND n.nspname='public' AND c.relkind='r';
  IF owner_name IS NULL THEN
    RAISE EXCEPTION 'signature audience target owner unavailable';
  END IF;

  ALTER TABLE public.jobs_managed_cloud_signature_sets
    DROP CONSTRAINT jobs_managed_cloud_signature_sets_target_audience_check;
  ALTER TABLE public.jobs_managed_cloud_signature_sets
    ADD CONSTRAINT jobs_managed_cloud_signature_sets_target_audience_check
    CHECK (target_audience IN (
      'bluey-jobs-managed-cloud-activation-v1',
      'bluey-jobs-managed-cloud-activation-v2',
      'bluey-jobs-managed-cloud-activation-v3',
      'bluey-jobs-managed-cloud-cohort-v1',
      'bluey-jobs-managed-cloud-release-v1',
      'bluey-jobs-managed-cloud-release-v2',
      'bluey-jobs-managed-cloud-release-v3',
      'bluey-jobs-managed-cloud-revocation-v1',
      'bluey-jobs-managed-cloud-rollback-v1',
      'bluey-jobs-managed-cloud-trust-policy-v1',
      'bluey-jobs-managed-cloud-provisioning-v4',
      'bluey-jobs-managed-cloud-preproduction-context-v1',
      'bluey-jobs-managed-cloud-serving-latch-v1',
      'bluey-jobs-managed-cloud-process-attachment-v1'
    ));
  CREATE TABLE public.jobs_signature_audience_upgrade_state (
    singleton smallint PRIMARY KEY CHECK (singleton = 1),
    version smallint NOT NULL CHECK (version = 1)
  );
  INSERT INTO public.jobs_signature_audience_upgrade_state(singleton,version)
    VALUES (1,1);
  -- Match the guarded signature relation's owner. PostgreSQL permission errors
  -- abort the transaction; no fallback ownership or automatic repair is safe.
  EXECUTE pg_catalog.format(
    'ALTER TABLE public.jobs_signature_audience_upgrade_state OWNER TO %I', owner_name);
END
$phase657_forward$;

-- Integration still must revalidate the resulting exact catalog/state under the
-- same lock, insert the selected ledger without resetting an existing timestamp,
-- and commit atomically. Do not reuse or mutate a guard result across calls.
-- Unrun: PostgreSQL 17.10 behavior, schema ownership/privileges, rollback,
-- concurrent writers, custom ledger, restore, and existing data preservation
-- require owned physical tests. Old writers remain a release-quiescence hold.

-- Target: PostgreSQL
-- FAIL-CLOSED GUARDED MIGRATION MARKER.
--
-- Runtime startup must extract the closed body below and execute it only in
-- the Rust-owned transaction after exact shape, state, lock, and ledger
-- validation. Raw execution is forbidden and stops here before any mutation.
DO $phase660_operator_marker$
BEGIN
  RAISE EXCEPTION '052_jobs_managed_cloud_release_v4_audiences.sql requires the guarded Rust migration driver';
END;
$phase660_operator_marker$;

/* BEGIN GUARDED AUDIENCE V4 BODY
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
    'bluey-jobs-managed-cloud-process-attachment-v1',
    'bluey-jobs-managed-cloud-release-v4',
    'bluey-jobs-managed-cloud-activation-v4'
  ));

ALTER TABLE public.jobs_signature_audience_upgrade_state
  DROP CONSTRAINT jobs_signature_audience_upgrade_state_version_check;
UPDATE public.jobs_signature_audience_upgrade_state
   SET version = 2
 WHERE singleton = 1 AND version = 1;
ALTER TABLE public.jobs_signature_audience_upgrade_state
  ADD CONSTRAINT jobs_signature_audience_upgrade_state_version_check
  CHECK (version = 2);
END GUARDED AUDIENCE V4 BODY */

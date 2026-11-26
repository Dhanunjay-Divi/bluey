-- Private Phase657 fragment, not a standalone operator migration.
-- The Rust owner supplies schema validation, transaction/lock and ledger boundaries.
SET LOCAL search_path = pg_catalog, pg_temp;
SET LOCAL lock_timeout = '5s';
SET LOCAL statement_timeout = '30s';
LOCK TABLE public.jobs_managed_cloud_signature_sets IN ACCESS EXCLUSIVE MODE;

-- Exact ordered lists from registered 033, 036 and 040 respectively.
CREATE TEMP TABLE phase657_audience6 (
  target_audience text NOT NULL,
  CONSTRAINT audience_check CHECK (target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1',
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
    'bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1',
    'bluey-jobs-managed-cloud-trust-policy-v1'
  ))
) ON COMMIT DROP;
CREATE TEMP TABLE phase657_audience8 (
  target_audience text NOT NULL,
  CONSTRAINT audience_check CHECK (target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1',
    'bluey-jobs-managed-cloud-activation-v2',
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
    'bluey-jobs-managed-cloud-release-v2',
    'bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1',
    'bluey-jobs-managed-cloud-trust-policy-v1'
  ))
) ON COMMIT DROP;
CREATE TEMP TABLE phase657_audience10 (
  target_audience text NOT NULL,
  CONSTRAINT audience_check CHECK (target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1',
    'bluey-jobs-managed-cloud-activation-v2',
    'bluey-jobs-managed-cloud-activation-v3',
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
    'bluey-jobs-managed-cloud-release-v2',
    'bluey-jobs-managed-cloud-release-v3',
    'bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1',
    'bluey-jobs-managed-cloud-trust-policy-v1'
  ))
) ON COMMIT DROP;
CREATE TEMP TABLE phase657_audience13 (
  target_audience text NOT NULL,
  CONSTRAINT audience_check CHECK (target_audience IN (
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
    'bluey-jobs-managed-cloud-serving-latch-v1'
  ))
) ON COMMIT DROP;
CREATE TEMP TABLE phase657_audience14 (
  target_audience text NOT NULL,
  CONSTRAINT audience_check CHECK (target_audience IN (
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
  ))
) ON COMMIT DROP;
CREATE TEMP TABLE phase657_audience_state_reference (
  singleton smallint PRIMARY KEY CHECK (singleton = 1),
  version smallint NOT NULL CHECK (version = 1)
) ON COMMIT DROP;
CREATE TEMP TABLE phase660_audience16 (
  target_audience text NOT NULL,
  CONSTRAINT audience_check CHECK (target_audience IN (
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
  ))
) ON COMMIT DROP;
CREATE TEMP TABLE phase660_audience_state_reference (
  singleton smallint PRIMARY KEY CHECK (singleton = 1),
  version smallint NOT NULL CHECK (version = 2)
) ON COMMIT DROP;
CREATE TEMP TABLE phase657_audience_guard_result (
  state text PRIMARY KEY CHECK (state IN (
    'legacy6', 'legacy8', 'legacy10', 'legacy13', 'successor14', 'release16'
  ))
) ON COMMIT DROP;

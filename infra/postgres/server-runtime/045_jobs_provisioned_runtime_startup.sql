-- Target: PostgreSQL. Phase648 C1 storage only, not positive proof/admission.
-- Upgrade existing populated authority tables; commit schema and replay marker together.
BEGIN;

ALTER TABLE jobs_managed_cloud_signature_sets
  DROP CONSTRAINT IF EXISTS jobs_managed_cloud_signature_sets_target_audience_check;
ALTER TABLE jobs_managed_cloud_signature_sets
  ADD CONSTRAINT jobs_managed_cloud_signature_sets_target_audience_check CHECK(target_audience IN (
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
  ));

-- Allocation intent is not a claim that Temporal created or owns this namespace.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_queue_generations (
  queue_generation_id TEXT NOT NULL PRIMARY KEY CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  issuance_ref TEXT NOT NULL UNIQUE CHECK(issuance_ref ~ '^[A-Za-z0-9_-]{20,128}$'),
  environment TEXT NOT NULL CHECK(environment IN ('production','staging')),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel IN ('canary','general','shadow')),
  purpose TEXT NOT NULL CHECK(purpose IN ('production','nonproduction_synthetic')),
  namespace TEXT NOT NULL CHECK(octet_length(namespace) BETWEEN 1 AND 240),
  task_queue TEXT NOT NULL CHECK(octet_length(task_queue) BETWEEN 1 AND 240),
  task_queue_sha256 TEXT NOT NULL UNIQUE CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  created_at_ms BIGINT NOT NULL CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(namespace,task_queue),
  UNIQUE(queue_generation_id,environment,region,channel,purpose,task_queue_sha256),
  CHECK(purpose='production' OR (environment='staging' AND channel='canary'))
);

-- These are canonical context METADATA, not verified resource/egress evidence.
CREATE TABLE IF NOT EXISTS jobs_managed_cloud_preproduction_contexts (
  context_sha256 TEXT NOT NULL PRIMARY KEY CHECK(context_sha256 ~ '^[0-9a-f]{64}$'),
  context_id TEXT NOT NULL UNIQUE CHECK(context_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  trust_generation BIGINT NOT NULL REFERENCES jobs_managed_cloud_trust_policies(trust_generation)
    ON DELETE RESTRICT CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  environment TEXT NOT NULL CHECK(environment='staging'),
  region TEXT NOT NULL CHECK(region ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
  channel TEXT NOT NULL CHECK(channel='canary'),
  purpose TEXT NOT NULL DEFAULT 'nonproduction_synthetic' CHECK(purpose='nonproduction_synthetic'),
  source_commit TEXT NOT NULL CHECK(source_commit ~ '^[0-9a-f]{40}$'
    AND source_commit<>'0000000000000000000000000000000000000000'),
  component_set_sha256 TEXT NOT NULL CHECK(component_set_sha256 ~ '^[0-9a-f]{64}$'),
  db_instance_sha256 TEXT NOT NULL CHECK(db_instance_sha256 ~ '^[0-9a-f]{64}$'),
  private_resource_set_sha256 TEXT NOT NULL CHECK(private_resource_set_sha256 ~ '^[0-9a-f]{64}$'),
  namespace TEXT NOT NULL CHECK(octet_length(namespace) BETWEEN 1 AND 240),
  task_queue TEXT NOT NULL CHECK(octet_length(task_queue) BETWEEN 1 AND 240),
  queue_generation_id TEXT NOT NULL CHECK(queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$'),
  task_queue_sha256 TEXT NOT NULL CHECK(task_queue_sha256 ~ '^[0-9a-f]{64}$'),
  egress_policy_sha256 TEXT NOT NULL CHECK(egress_policy_sha256 ~ '^[0-9a-f]{64}$'),
  cohort_sha256 TEXT NOT NULL REFERENCES jobs_managed_cloud_cohorts(cohort_sha256) ON DELETE RESTRICT
    CHECK(cohort_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_context_base64url TEXT NOT NULL CHECK(
    length(canonical_context_base64url) BETWEEN 1 AND 174763
    AND canonical_context_base64url ~ '^[A-Za-z0-9_-]+$'),
  authorization_signature_set_sha256 TEXT NOT NULL
    REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256) ON DELETE RESTRICT
    CHECK(authorization_signature_set_sha256 ~ '^[0-9a-f]{64}$'),
  issued_at_ms BIGINT NOT NULL CHECK(issued_at_ms BETWEEN 0 AND 9007199254740991),
  not_before_ms BIGINT NOT NULL CHECK(not_before_ms BETWEEN issued_at_ms AND 9007199254740991),
  expires_at_ms BIGINT NOT NULL CHECK(expires_at_ms BETWEEN 1 AND 9007199254740991
    AND expires_at_ms>not_before_ms AND expires_at_ms-issued_at_ms<=86400000),
  recorded_by TEXT NOT NULL CHECK(octet_length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms BIGINT NOT NULL CHECK(recorded_at_ms BETWEEN issued_at_ms AND 9007199254740991),
  UNIQUE(context_sha256,environment,region,channel,queue_generation_id,cohort_sha256),
  FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256)
    REFERENCES jobs_managed_cloud_queue_generations(
      queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION jobs_managed_cloud_context_insert_guard() RETURNS TRIGGER AS $$
BEGIN
  IF NEW.expires_at_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_queue_generations queue
      WHERE queue.queue_generation_id=NEW.queue_generation_id
        AND queue.namespace=NEW.namespace AND queue.task_queue=NEW.task_queue)
    OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_cohorts cohort
      WHERE cohort.cohort_sha256=NEW.cohort_sha256 AND cohort.trust_generation=NEW.trust_generation
        AND cohort.environment=NEW.environment AND cohort.region=NEW.region AND cohort.channel=NEW.channel)
    OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_signature_sets signature
      WHERE signature.signature_set_sha256=NEW.authorization_signature_set_sha256
        AND signature.target_audience='bluey-jobs-managed-cloud-preproduction-context-v1'
        AND signature.target_sha256=NEW.context_sha256 AND signature.trust_generation=NEW.trust_generation)
  THEN RAISE EXCEPTION 'invalid managed cloud context metadata'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_context_insert ON jobs_managed_cloud_preproduction_contexts;
CREATE TRIGGER trg_jobs_managed_cloud_context_insert BEFORE INSERT
ON jobs_managed_cloud_preproduction_contexts FOR EACH ROW
EXECUTE FUNCTION jobs_managed_cloud_context_insert_guard();

ALTER TABLE jobs_managed_cloud_activations
  ADD COLUMN IF NOT EXISTS authority_kind TEXT NOT NULL DEFAULT 'legacy_activation'
    CHECK(authority_kind IN ('legacy_activation','provisioning_v4')),
  ADD COLUMN IF NOT EXISTS purpose TEXT NOT NULL DEFAULT 'production'
    CHECK(purpose IN ('production','nonproduction_synthetic')),
  ADD COLUMN IF NOT EXISTS context_sha256 TEXT CHECK(context_sha256 IS NULL OR context_sha256 ~ '^[0-9a-f]{64}$'),
  ADD COLUMN IF NOT EXISTS queue_generation_id TEXT CHECK(queue_generation_id IS NULL OR queue_generation_id ~ '^[A-Za-z0-9_-]{20,128}$');
ALTER TABLE jobs_managed_cloud_activations
  ALTER COLUMN runner_fleet_evidence_sha256 DROP NOT NULL,
  ALTER COLUMN cleanup_authority_sha256 DROP NOT NULL,
  ALTER COLUMN canary_evidence_sha256 DROP NOT NULL,
  ALTER COLUMN portal_readback_evidence_sha256 DROP NOT NULL,
  ALTER COLUMN portal_readback_at_ms DROP NOT NULL,
  ALTER COLUMN portal_readback_ttl_ms DROP NOT NULL;
ALTER TABLE jobs_managed_cloud_activations DROP CONSTRAINT IF EXISTS ck_jobs_managed_cloud_activation_kind;
-- Remove only the two old portal-derived expiry checks, not common expiry checks.
DO $$ DECLARE name TEXT; BEGIN
  FOR name IN SELECT conname FROM pg_constraint
    WHERE conrelid='jobs_managed_cloud_activations'::regclass AND contype='c'
      AND pg_get_constraintdef(oid) LIKE '%portal_readback_at_ms%'
      AND (pg_get_constraintdef(oid) LIKE '%issued_at_ms%'
        OR pg_get_constraintdef(oid) LIKE '%expires_at_ms%')
  LOOP EXECUTE format('ALTER TABLE jobs_managed_cloud_activations DROP CONSTRAINT %I',name); END LOOP;
END $$;
ALTER TABLE jobs_managed_cloud_activations ADD CONSTRAINT ck_jobs_managed_cloud_activation_kind CHECK((
  (authority_kind='legacy_activation' AND purpose='production'
    AND context_sha256 IS NULL AND queue_generation_id IS NULL
    AND runner_fleet_evidence_sha256 IS NOT NULL AND cleanup_authority_sha256 IS NOT NULL
    AND canary_evidence_sha256 IS NOT NULL AND portal_readback_evidence_sha256 IS NOT NULL
    AND portal_readback_at_ms IS NOT NULL AND portal_readback_ttl_ms IS NOT NULL
    AND portal_readback_at_ms<=issued_at_ms
    AND expires_at_ms<=portal_readback_at_ms+portal_readback_ttl_ms)
  OR (authority_kind='provisioning_v4' AND queue_generation_id IS NOT NULL AND requirement_count=9
    AND ((purpose='production' AND context_sha256 IS NULL)
      OR (purpose='nonproduction_synthetic' AND context_sha256 IS NOT NULL
        AND environment='staging' AND channel='canary'))
    AND runner_fleet_evidence_sha256 IS NULL AND cleanup_authority_sha256 IS NULL
    AND canary_evidence_sha256 IS NULL AND portal_readback_evidence_sha256 IS NULL
    AND portal_readback_at_ms IS NULL AND portal_readback_ttl_ms IS NULL
    AND NOT cloud_distribution_enabled AND NOT workflow_command_dispatch_enabled
    AND NOT workflow_cleanup_enabled AND NOT direct_discovery_enabled
    AND NOT global_discovery_enabled AND NOT source_verification_enabled)
) IS TRUE);
DO $$ BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_activations'::regclass
    AND conname='fk_jobs_managed_cloud_activation_queue') THEN
    ALTER TABLE jobs_managed_cloud_activations ADD CONSTRAINT fk_jobs_managed_cloud_activation_queue
      FOREIGN KEY(queue_generation_id,environment,region,channel,purpose,task_queue_sha256)
      REFERENCES jobs_managed_cloud_queue_generations(
        queue_generation_id,environment,region,channel,purpose,task_queue_sha256) ON DELETE RESTRICT;
  END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='jobs_managed_cloud_activations'::regclass
    AND conname='fk_jobs_managed_cloud_activation_context') THEN
    ALTER TABLE jobs_managed_cloud_activations ADD CONSTRAINT fk_jobs_managed_cloud_activation_context
      FOREIGN KEY(context_sha256,environment,region,channel,queue_generation_id,cohort_sha256)
      REFERENCES jobs_managed_cloud_preproduction_contexts(
        context_sha256,environment,region,channel,queue_generation_id,cohort_sha256) ON DELETE RESTRICT;
  END IF;
END $$;
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_managed_cloud_activation_queue_generation
  ON jobs_managed_cloud_activations(queue_generation_id) WHERE queue_generation_id IS NOT NULL;
CREATE OR REPLACE FUNCTION jobs_managed_cloud_provisioning_insert_guard() RETURNS TRIGGER AS $$
BEGIN
  IF NEW.authority_kind='provisioning_v4' AND (
    NEW.expires_at_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    OR NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_signature_sets signature
      WHERE signature.signature_set_sha256=NEW.authorization_signature_set_sha256
        AND signature.target_audience='bluey-jobs-managed-cloud-provisioning-v4'
        AND signature.target_sha256=NEW.activation_sha256 AND signature.trust_generation=NEW.trust_generation)
    OR (NEW.context_sha256 IS NOT NULL AND NOT EXISTS(
      SELECT 1 FROM jobs_managed_cloud_preproduction_contexts context
       WHERE context.context_sha256=NEW.context_sha256 AND context.not_before_ms<=NEW.not_before_ms
         AND context.expires_at_ms>=NEW.expires_at_ms))
  ) THEN RAISE EXCEPTION 'invalid managed cloud provisioning metadata'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_provisioning_insert ON jobs_managed_cloud_activations;
CREATE TRIGGER trg_jobs_managed_cloud_provisioning_insert BEFORE INSERT ON jobs_managed_cloud_activations
FOR EACH ROW EXECUTE FUNCTION jobs_managed_cloud_provisioning_insert_guard();

-- Preserve every predecessor/target FK and non-kind monotonicity constraint.
DO $$ DECLARE name TEXT; BEGIN
  FOR name IN SELECT conname FROM pg_constraint
    WHERE conrelid='jobs_managed_cloud_head_transitions'::regclass AND contype='c'
      AND pg_get_constraintdef(oid) LIKE '%transition_kind%'
  LOOP EXECUTE format('ALTER TABLE jobs_managed_cloud_head_transitions DROP CONSTRAINT %I',name); END LOOP;
END $$;
ALTER TABLE jobs_managed_cloud_head_transitions
  ADD CONSTRAINT ck_jobs_managed_cloud_transition_kind CHECK(transition_kind IN ('activation','rollback','provisioning')),
  ADD CONSTRAINT ck_jobs_managed_cloud_transition_first CHECK(
    (head_revision=1 AND previous_head_revision=0 AND previous_transition_sha256 IS NULL
      AND previous_activation_sha256 IS NULL AND previous_manifest_sha256 IS NULL
      AND previous_trust_generation IS NULL AND previous_channel_sequence IS NULL
      AND transition_kind IN ('activation','provisioning'))
    OR (head_revision>1 AND previous_transition_sha256 IS NOT NULL AND previous_activation_sha256 IS NOT NULL
      AND previous_manifest_sha256 IS NOT NULL AND previous_trust_generation IS NOT NULL AND previous_channel_sequence IS NOT NULL)),
  ADD CONSTRAINT ck_jobs_managed_cloud_transition_authority CHECK(
    (transition_kind IN ('activation','provisioning') AND authority_sha256=next_activation_sha256
      AND rollback_authority_sha256 IS NULL)
    OR (transition_kind='rollback' AND rollback_authority_sha256=authority_sha256));
CREATE OR REPLACE FUNCTION jobs_managed_cloud_transition_kind_guard() RETURNS TRIGGER AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations target
    WHERE target.activation_sha256=NEW.next_activation_sha256
      AND ((NEW.transition_kind='provisioning' AND target.authority_kind='provisioning_v4')
        OR (NEW.transition_kind IN ('activation','rollback') AND target.authority_kind='legacy_activation')))
    OR (NEW.transition_kind<>'provisioning' AND EXISTS(
      SELECT 1 FROM jobs_managed_cloud_activations previous
       WHERE previous.activation_sha256=NEW.previous_activation_sha256 AND previous.authority_kind='provisioning_v4'))
  THEN RAISE EXCEPTION 'managed cloud transition authority kind mismatch'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_transition_kind ON jobs_managed_cloud_head_transitions;
CREATE TRIGGER trg_jobs_managed_cloud_transition_kind BEFORE INSERT ON jobs_managed_cloud_head_transitions
FOR EACH ROW EXECUTE FUNCTION jobs_managed_cloud_transition_kind_guard();
CREATE OR REPLACE FUNCTION jobs_managed_cloud_head_kind_guard() RETURNS TRIGGER AS $$
BEGIN
  IF EXISTS(SELECT 1 FROM jobs_managed_cloud_activations previous
    WHERE previous.activation_sha256=OLD.current_activation_sha256 AND previous.authority_kind='provisioning_v4')
    AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_activations target
      WHERE target.activation_sha256=NEW.current_activation_sha256 AND target.authority_kind='provisioning_v4')
  THEN RAISE EXCEPTION 'managed cloud v4 authority cannot downgrade'; END IF;
  RETURN NEW;
END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_head_kind ON jobs_managed_cloud_heads;
CREATE TRIGGER trg_jobs_managed_cloud_head_kind BEFORE UPDATE ON jobs_managed_cloud_heads
FOR EACH ROW EXECUTE FUNCTION jobs_managed_cloud_head_kind_guard();
CREATE OR REPLACE FUNCTION jobs_managed_cloud_provisioning_metadata_immutable() RETURNS TRIGGER AS $$
BEGIN RAISE EXCEPTION 'managed cloud provisioning metadata is immutable'; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_queue_generations_immutable ON jobs_managed_cloud_queue_generations;
CREATE TRIGGER trg_jobs_managed_cloud_queue_generations_immutable BEFORE UPDATE OR DELETE
ON jobs_managed_cloud_queue_generations FOR EACH ROW EXECUTE FUNCTION jobs_managed_cloud_provisioning_metadata_immutable();
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_preproduction_contexts_immutable ON jobs_managed_cloud_preproduction_contexts;
CREATE TRIGGER trg_jobs_managed_cloud_preproduction_contexts_immutable BEFORE UPDATE OR DELETE
ON jobs_managed_cloud_preproduction_contexts FOR EACH ROW EXECUTE FUNCTION jobs_managed_cloud_provisioning_metadata_immutable();

DO $migration$
DECLARE
  ledger text := current_setting('bluey.migration_table', true);
BEGIN
  IF ledger IS NULL THEN
    ledger := 'bluey_schema_migrations';
  END IF;
  IF octet_length(ledger) > 63 OR ledger COLLATE "C" !~ '^[a-z_][a-z0-9_]*$' THEN
    RAISE EXCEPTION 'invalid migration ledger identifier';
  END IF;
  EXECUTE format('INSERT INTO %I(version) VALUES ($1) ON CONFLICT(version) DO NOTHING', ledger)
    USING '045_jobs_provisioned_runtime_startup.sql';
END;
$migration$;
COMMIT;

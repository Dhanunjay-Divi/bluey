-- Target: PostgreSQL
-- Account-independent, dual-role signed employer-identity and job-risk
-- authority. The migration seeds no trust, attestation, revocation, or head.

-- Migration 035 added the signed v2 release and activation contracts. Widen
-- the original immutable signature-set audience constraint before those
-- authorities are imported. Replace and validate it only when the replayed
-- migration observes the old definition, avoiding an unnecessary table lock
-- on every server startup.
DO $$
DECLARE
  audience_relation_exists BOOLEAN;
  audience_constraint_definition TEXT;
BEGIN
  SELECT EXISTS (
    SELECT 1
      FROM pg_class relation
      JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
     WHERE namespace.nspname = current_schema()
       AND relation.relname = 'jobs_managed_cloud_signature_sets'
       AND relation.relkind IN ('r', 'p')
  ) INTO audience_relation_exists;
  SELECT pg_get_constraintdef(constraint_record.oid)
    INTO audience_constraint_definition
    FROM pg_constraint constraint_record
    JOIN pg_class relation ON relation.oid = constraint_record.conrelid
    JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
   WHERE namespace.nspname = current_schema()
     AND relation.relname = 'jobs_managed_cloud_signature_sets'
     AND constraint_record.conname =
       'jobs_managed_cloud_signature_sets_target_audience_check';
  IF audience_relation_exists
     AND (
       audience_constraint_definition IS NULL
       OR position(
         'bluey-jobs-managed-cloud-activation-v2' IN audience_constraint_definition
       ) = 0
       OR position(
         'bluey-jobs-managed-cloud-release-v2' IN audience_constraint_definition
       ) = 0
     ) THEN
    ALTER TABLE jobs_managed_cloud_signature_sets
      DROP CONSTRAINT IF EXISTS
        jobs_managed_cloud_signature_sets_target_audience_check;
    ALTER TABLE jobs_managed_cloud_signature_sets
      ADD CONSTRAINT jobs_managed_cloud_signature_sets_target_audience_check CHECK (
        target_audience IN (
          'bluey-jobs-managed-cloud-activation-v1',
          'bluey-jobs-managed-cloud-activation-v2',
          'bluey-jobs-managed-cloud-cohort-v1',
          'bluey-jobs-managed-cloud-release-v1',
          'bluey-jobs-managed-cloud-release-v2',
          'bluey-jobs-managed-cloud-revocation-v1',
          'bluey-jobs-managed-cloud-rollback-v1',
          'bluey-jobs-managed-cloud-trust-policy-v1'
        )
      ) NOT VALID;
    ALTER TABLE jobs_managed_cloud_signature_sets
      VALIDATE CONSTRAINT
        jobs_managed_cloud_signature_sets_target_audience_check;
  END IF;
END;
$$;

CREATE TABLE IF NOT EXISTS jobs_job_integrity_trust_policies (
  policy_sha256                         TEXT PRIMARY KEY CHECK(policy_sha256 ~ '^[0-9a-f]{64}$'),
  policy_id                             TEXT NOT NULL UNIQUE CHECK(length(policy_id) BETWEEN 1 AND 120),
  trust_generation                     BIGINT NOT NULL UNIQUE
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256             TEXT UNIQUE
    CHECK(predecessor_policy_sha256 IS NULL OR predecessor_policy_sha256 ~ '^[0-9a-f]{64}$'),
  root_anchor_sha256                    TEXT NOT NULL CHECK(root_anchor_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_policy_base64url            TEXT NOT NULL
    CHECK(octet_length(canonical_policy_base64url) BETWEEN 4 AND 87384),
  root_authorization_id                  TEXT NOT NULL UNIQUE
    CHECK(length(root_authorization_id) BETWEEN 1 AND 120),
  root_authorization_sha256             TEXT NOT NULL UNIQUE
    CHECK(root_authorization_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_root_authorization_base64url TEXT NOT NULL
    CHECK(octet_length(canonical_root_authorization_base64url) BETWEEN 4 AND 87384),
  employer_identity_threshold           BIGINT NOT NULL
    CHECK(employer_identity_threshold BETWEEN 1 AND 16),
  job_risk_threshold                    BIGINT NOT NULL CHECK(job_risk_threshold BETWEEN 1 AND 16),
  revocation_threshold                  BIGINT NOT NULL CHECK(revocation_threshold BETWEEN 1 AND 16),
  maximum_positive_lifetime_ms          BIGINT NOT NULL
    CHECK(maximum_positive_lifetime_ms BETWEEN 60000 AND 31536000000),
  maximum_nonpositive_lifetime_ms       BIGINT NOT NULL
    CHECK(maximum_nonpositive_lifetime_ms BETWEEN 60000 AND 31536000000),
  maximum_clock_skew_ms                 BIGINT NOT NULL CHECK(maximum_clock_skew_ms BETWEEN 0 AND 300000),
  maximum_canonical_bytes               BIGINT NOT NULL CHECK(maximum_canonical_bytes BETWEEN 1024 AND 65536),
  maximum_identity_evidence_count       BIGINT NOT NULL
    CHECK(maximum_identity_evidence_count BETWEEN 1 AND 64),
  maximum_risk_evidence_count           BIGINT NOT NULL
    CHECK(maximum_risk_evidence_count BETWEEN 1 AND 64),
  allowed_providers_json                TEXT NOT NULL
    CHECK(jsonb_typeof(allowed_providers_json::jsonb) = 'array'),
  required_identity_methods_json        TEXT NOT NULL
    CHECK(jsonb_typeof(required_identity_methods_json::jsonb) = 'array'),
  required_identity_evidence_classes_json TEXT NOT NULL
    CHECK(jsonb_typeof(required_identity_evidence_classes_json::jsonb) = 'array'),
  required_risk_evidence_classes_json   TEXT NOT NULL
    CHECK(jsonb_typeof(required_risk_evidence_classes_json::jsonb) = 'array'),
  allowed_identity_evidence_classes_json TEXT NOT NULL
    CHECK(jsonb_typeof(allowed_identity_evidence_classes_json::jsonb) = 'array'),
  allowed_risk_evidence_classes_json    TEXT NOT NULL
    CHECK(jsonb_typeof(allowed_risk_evidence_classes_json::jsonb) = 'array'),
  allowed_risk_signal_codes_json        TEXT NOT NULL
    CHECK(jsonb_typeof(allowed_risk_signal_codes_json::jsonb) = 'array'),
  allowed_risk_policy_sha256s_json      TEXT NOT NULL
    CHECK(jsonb_typeof(allowed_risk_policy_sha256s_json::jsonb) = 'array'),
  issued_at_ms                          BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  valid_from_ms                         BIGINT NOT NULL CHECK(valid_from_ms >= issued_at_ms),
  expires_at_ms                         BIGINT NOT NULL CHECK(expires_at_ms > valid_from_ms),
  recorded_by                           TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms                        BIGINT NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(policy_sha256, trust_generation),
  FOREIGN KEY(predecessor_policy_sha256)
    REFERENCES jobs_job_integrity_trust_policies(policy_sha256) ON DELETE RESTRICT,
  CHECK((trust_generation = 1 AND predecessor_policy_sha256 IS NULL)
    OR (trust_generation > 1 AND predecessor_policy_sha256 IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_trust_policies_generation
  ON jobs_job_integrity_trust_policies(trust_generation DESC, expires_at_ms);

CREATE TABLE IF NOT EXISTS jobs_job_integrity_trust_keys (
  policy_sha256                       TEXT NOT NULL,
  trust_generation                   BIGINT NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role                               TEXT NOT NULL
    CHECK(role IN ('employer_identity','job_risk','revocation')),
  threshold                          BIGINT NOT NULL CHECK(threshold BETWEEN 1 AND 16),
  key_id                             TEXT NOT NULL CHECK(length(key_id) BETWEEN 1 AND 120),
  public_key_base64url               TEXT NOT NULL CHECK(length(public_key_base64url) = 43),
  key_sha256                         TEXT NOT NULL CHECK(key_sha256 ~ '^[0-9a-f]{64}$'),
  valid_from_ms                      BIGINT NOT NULL CHECK(valid_from_ms >= 0),
  expires_at_ms                      BIGINT NOT NULL CHECK(expires_at_ms > valid_from_ms),
  PRIMARY KEY(policy_sha256, key_id),
  UNIQUE(policy_sha256, role, key_id),
  UNIQUE(policy_sha256, public_key_base64url),
  UNIQUE(policy_sha256, key_sha256),
  FOREIGN KEY(policy_sha256, trust_generation)
    REFERENCES jobs_job_integrity_trust_policies(
      policy_sha256, trust_generation) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_trust_keys_role
  ON jobs_job_integrity_trust_keys(policy_sha256, role, key_id);

CREATE TABLE IF NOT EXISTS jobs_job_integrity_attestations (
  attestation_sha256                  TEXT PRIMARY KEY CHECK(attestation_sha256 ~ '^[0-9a-f]{64}$'),
  attestation_id                      TEXT NOT NULL UNIQUE CHECK(length(attestation_id) BETWEEN 1 AND 120),
  policy_sha256                       TEXT NOT NULL,
  subject_sha256                      TEXT NOT NULL CHECK(subject_sha256 ~ '^[0-9a-f]{64}$'),
  source_material_sha256              TEXT NOT NULL CHECK(source_material_sha256 ~ '^[0-9a-f]{64}$'),
  attestation_generation              BIGINT NOT NULL
    CHECK(attestation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_attestation_sha256      TEXT UNIQUE
    CHECK(predecessor_attestation_sha256 IS NULL
      OR predecessor_attestation_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_job_id                    TEXT NOT NULL CHECK(length(canonical_job_id) BETWEEN 1 AND 240),
  provider_family                     TEXT NOT NULL CHECK(length(provider_family) BETWEEN 1 AND 64),
  provider_record_id                  TEXT NOT NULL CHECK(length(provider_record_id) BETWEEN 1 AND 512),
  provider_host                       TEXT NOT NULL CHECK(length(provider_host) BETWEEN 1 AND 253),
  provider_tenant                     TEXT NOT NULL CHECK(length(provider_tenant) BETWEEN 1 AND 240),
  provider_job                        TEXT NOT NULL CHECK(length(provider_job) BETWEEN 1 AND 512),
  provider_variant                    TEXT NOT NULL CHECK(length(provider_variant) BETWEEN 1 AND 120),
  canonical_application_url           TEXT NOT NULL CHECK(length(canonical_application_url) BETWEEN 8 AND 4096),
  application_domain                  TEXT NOT NULL CHECK(length(application_domain) BETWEEN 1 AND 253),
  ats_tenant_binding_sha256           TEXT NOT NULL CHECK(ats_tenant_binding_sha256 ~ '^[0-9a-f]{64}$'),
  employer_status                     TEXT NOT NULL CHECK(employer_status IN ('verified','unverified','mismatch')),
  canonical_employer_id               TEXT NOT NULL CHECK(length(canonical_employer_id) BETWEEN 1 AND 240),
  canonical_employer_domain           TEXT NOT NULL CHECK(length(canonical_employer_domain) BETWEEN 1 AND 253),
  verification_methods_json           TEXT NOT NULL
    CHECK(jsonb_typeof(verification_methods_json::jsonb) = 'array'),
  identity_evidence_json              TEXT NOT NULL
    CHECK(jsonb_typeof(identity_evidence_json::jsonb) = 'array'),
  risk_status                         TEXT NOT NULL CHECK(risk_status IN ('clear','review_required','blocked')),
  risk_signal_codes_json              TEXT NOT NULL
    CHECK(jsonb_typeof(risk_signal_codes_json::jsonb) = 'array'),
  risk_policy_sha256                  TEXT NOT NULL CHECK(risk_policy_sha256 ~ '^[0-9a-f]{64}$'),
  risk_input_sha256                   TEXT NOT NULL CHECK(risk_input_sha256 ~ '^[0-9a-f]{64}$'),
  risk_engine_release_sha256          TEXT NOT NULL CHECK(risk_engine_release_sha256 ~ '^[0-9a-f]{64}$'),
  risk_evidence_json                  TEXT NOT NULL
    CHECK(jsonb_typeof(risk_evidence_json::jsonb) = 'array'),
  canonical_attestation_base64url     TEXT NOT NULL
    CHECK(octet_length(canonical_attestation_base64url) BETWEEN 4 AND 87384),
  employer_identity_authorization_id  TEXT NOT NULL UNIQUE
    CHECK(length(employer_identity_authorization_id) BETWEEN 1 AND 120),
  employer_authorization_sha256       TEXT NOT NULL UNIQUE
    CHECK(employer_authorization_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_employer_authorization_base64url TEXT NOT NULL
    CHECK(octet_length(canonical_employer_authorization_base64url) BETWEEN 4 AND 87384),
  job_risk_authorization_id           TEXT NOT NULL UNIQUE
    CHECK(length(job_risk_authorization_id) BETWEEN 1 AND 120),
  risk_authorization_sha256           TEXT NOT NULL UNIQUE CHECK(risk_authorization_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_risk_authorization_base64url TEXT NOT NULL
    CHECK(octet_length(canonical_risk_authorization_base64url) BETWEEN 4 AND 87384),
  assessed_at_ms                      BIGINT NOT NULL CHECK(assessed_at_ms >= 0),
  issued_at_ms                        BIGINT NOT NULL CHECK(issued_at_ms >= assessed_at_ms),
  not_before_ms                       BIGINT NOT NULL CHECK(not_before_ms >= 0),
  expires_at_ms                       BIGINT NOT NULL CHECK(expires_at_ms > not_before_ms),
  effective_expires_at_ms             BIGINT NOT NULL CHECK(effective_expires_at_ms > not_before_ms),
  recorded_by                         TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms                      BIGINT NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(subject_sha256, attestation_generation),
  FOREIGN KEY(policy_sha256)
    REFERENCES jobs_job_integrity_trust_policies(policy_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(predecessor_attestation_sha256)
    REFERENCES jobs_job_integrity_attestations(attestation_sha256) ON DELETE RESTRICT,
  CHECK((attestation_generation = 1 AND predecessor_attestation_sha256 IS NULL)
    OR (attestation_generation > 1 AND predecessor_attestation_sha256 IS NOT NULL)),
  CHECK(effective_expires_at_ms <= expires_at_ms)
);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_attestations_subject
  ON jobs_job_integrity_attestations(subject_sha256, attestation_generation DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_attestations_employer
  ON jobs_job_integrity_attestations(canonical_employer_id, canonical_employer_domain, expires_at_ms);

CREATE TABLE IF NOT EXISTS jobs_job_integrity_revocations (
  revocation_sha256                 TEXT PRIMARY KEY CHECK(revocation_sha256 ~ '^[0-9a-f]{64}$'),
  revocation_id                     TEXT NOT NULL UNIQUE CHECK(length(revocation_id) BETWEEN 1 AND 120),
  revocation_generation             BIGINT NOT NULL UNIQUE
    CHECK(revocation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_revocation_sha256     TEXT UNIQUE
    CHECK(predecessor_revocation_sha256 IS NULL
      OR predecessor_revocation_sha256 ~ '^[0-9a-f]{64}$'),
  policy_sha256                     TEXT NOT NULL,
  subject_kind                      TEXT NOT NULL CHECK(subject_kind IN (
    'trust_policy','trust_key','attestation','subject','canonical_employer',
    'identity_evidence','risk_evidence','risk_policy','risk_engine_release')),
  subject_id                        TEXT NOT NULL CHECK(length(subject_id) BETWEEN 1 AND 512),
  subject_sha256                    TEXT NOT NULL CHECK(subject_sha256 ~ '^[0-9a-f]{64}$'),
  reason_code                       TEXT NOT NULL CHECK(length(reason_code) BETWEEN 1 AND 64),
  reason_ref                        TEXT NOT NULL CHECK(length(reason_ref) BETWEEN 1 AND 240),
  effective_at_ms                   BIGINT NOT NULL CHECK(effective_at_ms >= 0),
  issued_at_ms                      BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  canonical_revocation_base64url    TEXT NOT NULL
    CHECK(octet_length(canonical_revocation_base64url) BETWEEN 4 AND 87384),
  authorization_id                  TEXT NOT NULL UNIQUE CHECK(length(authorization_id) BETWEEN 1 AND 120),
  authorization_sha256              TEXT NOT NULL UNIQUE CHECK(authorization_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(octet_length(canonical_authorization_base64url) BETWEEN 4 AND 87384),
  recorded_by                       TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms                    BIGINT NOT NULL CHECK(recorded_at_ms >= 0),
  FOREIGN KEY(policy_sha256)
    REFERENCES jobs_job_integrity_trust_policies(policy_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(predecessor_revocation_sha256)
    REFERENCES jobs_job_integrity_revocations(revocation_sha256) ON DELETE RESTRICT,
  CHECK((revocation_generation = 1 AND predecessor_revocation_sha256 IS NULL)
    OR (revocation_generation > 1 AND predecessor_revocation_sha256 IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_revocations_subject
  ON jobs_job_integrity_revocations(subject_kind, subject_id, subject_sha256, effective_at_ms);

CREATE TABLE IF NOT EXISTS jobs_job_integrity_head_transitions (
  transition_sha256                 TEXT PRIMARY KEY CHECK(transition_sha256 ~ '^[0-9a-f]{64}$'),
  subject_sha256                    TEXT NOT NULL CHECK(subject_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision                     BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision            BIGINT NOT NULL CHECK(previous_head_revision >= 0),
  predecessor_transition_sha256     TEXT UNIQUE
    CHECK(predecessor_transition_sha256 IS NULL
      OR predecessor_transition_sha256 ~ '^[0-9a-f]{64}$'),
  previous_attestation_sha256       TEXT,
  attestation_sha256                TEXT NOT NULL UNIQUE,
  attestation_generation            BIGINT NOT NULL
    CHECK(attestation_generation BETWEEN 1 AND 9007199254740991),
  policy_sha256                     TEXT NOT NULL,
  transition_actor                 TEXT NOT NULL CHECK(length(transition_actor) BETWEEN 1 AND 240),
  transitioned_at_ms               BIGINT NOT NULL CHECK(transitioned_at_ms >= 0),
  UNIQUE(subject_sha256, head_revision),
  FOREIGN KEY(predecessor_transition_sha256)
    REFERENCES jobs_job_integrity_head_transitions(transition_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(previous_attestation_sha256)
    REFERENCES jobs_job_integrity_attestations(attestation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(attestation_sha256)
    REFERENCES jobs_job_integrity_attestations(attestation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(policy_sha256)
    REFERENCES jobs_job_integrity_trust_policies(policy_sha256) ON DELETE RESTRICT,
  CHECK(head_revision = previous_head_revision + 1),
  CHECK((head_revision = 1 AND predecessor_transition_sha256 IS NULL
      AND previous_attestation_sha256 IS NULL)
    OR (head_revision > 1 AND predecessor_transition_sha256 IS NOT NULL
      AND previous_attestation_sha256 IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_head_transitions_history
  ON jobs_job_integrity_head_transitions(subject_sha256, head_revision DESC);

CREATE TABLE IF NOT EXISTS jobs_job_integrity_heads (
  subject_sha256                    TEXT PRIMARY KEY CHECK(subject_sha256 ~ '^[0-9a-f]{64}$'),
  head_revision                     BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256                 TEXT NOT NULL UNIQUE,
  attestation_sha256                TEXT NOT NULL UNIQUE,
  attestation_generation            BIGINT NOT NULL
    CHECK(attestation_generation BETWEEN 1 AND 9007199254740991),
  policy_sha256                     TEXT NOT NULL,
  updated_at_ms                     BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  FOREIGN KEY(transition_sha256)
    REFERENCES jobs_job_integrity_head_transitions(transition_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(attestation_sha256)
    REFERENCES jobs_job_integrity_attestations(attestation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(policy_sha256)
    REFERENCES jobs_job_integrity_trust_policies(policy_sha256) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_jobs_job_integrity_heads_attestation
  ON jobs_job_integrity_heads(attestation_sha256, policy_sha256);

CREATE TABLE IF NOT EXISTS jobs_job_integrity_control (
  singleton_id                      BIGINT PRIMARY KEY CHECK(singleton_id = 1),
  control_revision                  BIGINT NOT NULL CHECK(control_revision BETWEEN 0 AND 9007199254740991),
  current_policy_sha256             TEXT,
  current_trust_generation          BIGINT NOT NULL
    CHECK(current_trust_generation BETWEEN 0 AND 9007199254740991),
  current_revocation_sha256         TEXT,
  current_revocation_generation     BIGINT NOT NULL DEFAULT 0 CHECK(current_revocation_generation >= 0),
  updated_by                        TEXT NOT NULL CHECK(length(updated_by) BETWEEN 1 AND 240),
  updated_at_ms                     BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  FOREIGN KEY(current_policy_sha256)
    REFERENCES jobs_job_integrity_trust_policies(policy_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(current_revocation_sha256)
    REFERENCES jobs_job_integrity_revocations(revocation_sha256) ON DELETE RESTRICT,
  CHECK((current_trust_generation = 0 AND current_policy_sha256 IS NULL)
    OR (current_trust_generation > 0 AND current_policy_sha256 IS NOT NULL)),
  CHECK((current_revocation_generation = 0 AND current_revocation_sha256 IS NULL)
    OR (current_revocation_generation > 0 AND current_revocation_sha256 IS NOT NULL))
);
INSERT INTO jobs_job_integrity_control(
  singleton_id, control_revision, current_policy_sha256, current_trust_generation,
  current_revocation_sha256, current_revocation_generation, updated_by, updated_at_ms
) VALUES (1, 0, NULL, 0, NULL, 0, 'migration-scaffold', 0)
ON CONFLICT (singleton_id) DO NOTHING;

CREATE OR REPLACE FUNCTION reject_jobs_job_integrity_immutable_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'job-integrity signed authority is immutable';
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_job_integrity_head_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.head_revision <> 1 OR NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_head_transitions transition
     WHERE transition.transition_sha256 = NEW.transition_sha256
       AND transition.subject_sha256 = NEW.subject_sha256
       AND transition.head_revision = 1
       AND transition.previous_head_revision = 0
       AND transition.predecessor_transition_sha256 IS NULL
       AND transition.previous_attestation_sha256 IS NULL
       AND transition.attestation_sha256 = NEW.attestation_sha256
       AND transition.attestation_generation = NEW.attestation_generation
       AND transition.policy_sha256 = NEW.policy_sha256
  ) THEN
    RAISE EXCEPTION 'job-integrity initial head revision is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_job_integrity_head_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.subject_sha256 <> OLD.subject_sha256
     OR NEW.head_revision <> OLD.head_revision + 1
     OR NEW.attestation_generation <> OLD.attestation_generation + 1
     OR NEW.updated_at_ms < OLD.updated_at_ms
     OR NOT EXISTS (
       SELECT 1 FROM jobs_job_integrity_head_transitions transition
        WHERE transition.transition_sha256 = NEW.transition_sha256
          AND transition.subject_sha256 = OLD.subject_sha256
          AND transition.head_revision = OLD.head_revision + 1
          AND transition.previous_head_revision = OLD.head_revision
          AND transition.predecessor_transition_sha256 = OLD.transition_sha256
          AND transition.previous_attestation_sha256 = OLD.attestation_sha256
          AND transition.attestation_sha256 = NEW.attestation_sha256
          AND transition.attestation_generation = OLD.attestation_generation + 1
          AND transition.policy_sha256 = NEW.policy_sha256
     ) THEN
    RAISE EXCEPTION 'job-integrity head transition is not monotonic';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_job_integrity_transition_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_attestations attestation
     WHERE attestation.attestation_sha256 = NEW.attestation_sha256
       AND attestation.subject_sha256 = NEW.subject_sha256
       AND attestation.attestation_generation = NEW.attestation_generation
       AND attestation.policy_sha256 = NEW.policy_sha256
       AND attestation.predecessor_attestation_sha256
             IS NOT DISTINCT FROM NEW.previous_attestation_sha256
  ) OR (NEW.head_revision=1 AND (
      NEW.previous_head_revision<>0 OR NEW.predecessor_transition_sha256 IS NOT NULL
      OR NEW.previous_attestation_sha256 IS NOT NULL
  )) OR (NEW.head_revision>1 AND NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_head_transitions predecessor
     WHERE predecessor.transition_sha256=NEW.predecessor_transition_sha256
       AND predecessor.subject_sha256=NEW.subject_sha256
       AND predecessor.head_revision=NEW.previous_head_revision
       AND predecessor.attestation_sha256=NEW.previous_attestation_sha256
  )) THEN
    RAISE EXCEPTION 'job-integrity transition attestation binding is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_job_integrity_attestation_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.attestation_generation<>1 AND NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_attestations predecessor
     WHERE predecessor.attestation_sha256=NEW.predecessor_attestation_sha256
       AND predecessor.subject_sha256=NEW.subject_sha256
       AND predecessor.attestation_generation=NEW.attestation_generation-1
  ) THEN
    RAISE EXCEPTION 'job-integrity attestation predecessor binding is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_job_integrity_revocation_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.revocation_generation<>1 AND NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_revocations predecessor
     WHERE predecessor.revocation_sha256=NEW.predecessor_revocation_sha256
       AND predecessor.revocation_generation=NEW.revocation_generation-1
  ) THEN
    RAISE EXCEPTION 'job-integrity revocation predecessor binding is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_job_integrity_trust_policy_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.trust_generation > 1 AND NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_trust_policies predecessor
     WHERE predecessor.policy_sha256=NEW.predecessor_policy_sha256
       AND predecessor.trust_generation=NEW.trust_generation-1
       AND predecessor.root_anchor_sha256=NEW.root_anchor_sha256
  ) THEN
    RAISE EXCEPTION 'job-integrity trust policy root chain is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_job_integrity_trust_key_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_job_integrity_trust_policies policy
     WHERE policy.policy_sha256=NEW.policy_sha256
       AND policy.trust_generation=NEW.trust_generation
       AND NEW.threshold=CASE NEW.role
         WHEN 'employer_identity' THEN policy.employer_identity_threshold
         WHEN 'job_risk' THEN policy.job_risk_threshold
         WHEN 'revocation' THEN policy.revocation_threshold
       END
       AND NEW.valid_from_ms>=policy.valid_from_ms
       AND NEW.expires_at_ms<=policy.expires_at_ms
  ) OR EXISTS (
    SELECT 1 FROM jobs_job_integrity_trust_keys existing
     WHERE (existing.key_id=NEW.key_id
         OR existing.public_key_base64url=NEW.public_key_base64url
         OR existing.key_sha256=NEW.key_sha256)
       AND (existing.role<>NEW.role OR existing.key_id<>NEW.key_id
         OR existing.public_key_base64url<>NEW.public_key_base64url
         OR existing.key_sha256<>NEW.key_sha256)
  ) THEN
    RAISE EXCEPTION 'job-integrity trust key policy binding is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_job_integrity_control_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.singleton_id <> OLD.singleton_id
     OR NEW.control_revision <> OLD.control_revision + 1
     OR NEW.updated_at_ms < OLD.updated_at_ms
     OR ((NEW.current_policy_sha256, NEW.current_trust_generation)
           IS DISTINCT FROM (OLD.current_policy_sha256, OLD.current_trust_generation))
        = ((NEW.current_revocation_sha256, NEW.current_revocation_generation)
           IS DISTINCT FROM (OLD.current_revocation_sha256, OLD.current_revocation_generation))
     OR (((NEW.current_policy_sha256, NEW.current_trust_generation)
           IS DISTINCT FROM (OLD.current_policy_sha256, OLD.current_trust_generation))
       AND (NEW.current_trust_generation <> OLD.current_trust_generation + 1
         OR (NEW.current_revocation_sha256, NEW.current_revocation_generation)
            IS DISTINCT FROM (OLD.current_revocation_sha256, OLD.current_revocation_generation)
         OR NOT EXISTS (
           SELECT 1 FROM jobs_job_integrity_trust_policies policy
            WHERE policy.policy_sha256 = NEW.current_policy_sha256
              AND policy.trust_generation = NEW.current_trust_generation
              AND policy.predecessor_policy_sha256 IS NOT DISTINCT FROM OLD.current_policy_sha256
         )))
     OR (((NEW.current_revocation_sha256, NEW.current_revocation_generation)
           IS DISTINCT FROM (OLD.current_revocation_sha256, OLD.current_revocation_generation))
       AND (NEW.current_revocation_generation <> OLD.current_revocation_generation + 1
         OR (NEW.current_policy_sha256, NEW.current_trust_generation)
            IS DISTINCT FROM (OLD.current_policy_sha256, OLD.current_trust_generation)
         OR NOT EXISTS (
           SELECT 1 FROM jobs_job_integrity_revocations revocation
            WHERE revocation.revocation_sha256 = NEW.current_revocation_sha256
              AND revocation.revocation_generation = NEW.current_revocation_generation
              AND revocation.policy_sha256 = OLD.current_policy_sha256
              AND revocation.predecessor_revocation_sha256
                    IS NOT DISTINCT FROM OLD.current_revocation_sha256
         ))) THEN
    RAISE EXCEPTION 'job-integrity control transition is not monotonic';
  END IF;
  RETURN NEW;
END;
$$;

DO $$
DECLARE table_name TEXT;
BEGIN
  FOREACH table_name IN ARRAY ARRAY[
    'jobs_job_integrity_trust_policies', 'jobs_job_integrity_trust_keys',
    'jobs_job_integrity_attestations', 'jobs_job_integrity_revocations',
    'jobs_job_integrity_head_transitions'
  ] LOOP
    EXECUTE format('DROP TRIGGER IF EXISTS trg_%s_no_update ON %I', table_name, table_name);
    EXECUTE format(
      'CREATE TRIGGER trg_%s_no_update BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION reject_jobs_job_integrity_immutable_mutation()',
      table_name, table_name);
    EXECUTE format('DROP TRIGGER IF EXISTS trg_%s_no_delete ON %I', table_name, table_name);
    EXECUTE format(
      'CREATE TRIGGER trg_%s_no_delete BEFORE DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_jobs_job_integrity_immutable_mutation()',
      table_name, table_name);
  END LOOP;
END;
$$;

DROP TRIGGER IF EXISTS trg_jobs_job_integrity_heads_validate_insert ON jobs_job_integrity_heads;
CREATE TRIGGER trg_jobs_job_integrity_heads_validate_insert
BEFORE INSERT ON jobs_job_integrity_heads FOR EACH ROW
EXECUTE FUNCTION validate_jobs_job_integrity_head_insert();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_heads_monotonic ON jobs_job_integrity_heads;
CREATE TRIGGER trg_jobs_job_integrity_heads_monotonic
BEFORE UPDATE ON jobs_job_integrity_heads FOR EACH ROW
EXECUTE FUNCTION enforce_jobs_job_integrity_head_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_heads_no_delete ON jobs_job_integrity_heads;
CREATE TRIGGER trg_jobs_job_integrity_heads_no_delete
BEFORE DELETE ON jobs_job_integrity_heads FOR EACH ROW
EXECUTE FUNCTION reject_jobs_job_integrity_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_head_transitions_validate_insert
  ON jobs_job_integrity_head_transitions;
CREATE TRIGGER trg_jobs_job_integrity_head_transitions_validate_insert
BEFORE INSERT ON jobs_job_integrity_head_transitions FOR EACH ROW
EXECUTE FUNCTION validate_jobs_job_integrity_transition_insert();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_trust_keys_validate_insert
  ON jobs_job_integrity_trust_keys;
CREATE TRIGGER trg_jobs_job_integrity_trust_keys_validate_insert
BEFORE INSERT ON jobs_job_integrity_trust_keys FOR EACH ROW
EXECUTE FUNCTION validate_jobs_job_integrity_trust_key_insert();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_trust_policies_validate_insert
  ON jobs_job_integrity_trust_policies;
CREATE TRIGGER trg_jobs_job_integrity_trust_policies_validate_insert
BEFORE INSERT ON jobs_job_integrity_trust_policies FOR EACH ROW
EXECUTE FUNCTION validate_jobs_job_integrity_trust_policy_insert();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_attestations_validate_insert
  ON jobs_job_integrity_attestations;
CREATE TRIGGER trg_jobs_job_integrity_attestations_validate_insert
BEFORE INSERT ON jobs_job_integrity_attestations FOR EACH ROW
EXECUTE FUNCTION validate_jobs_job_integrity_attestation_insert();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_revocations_validate_insert
  ON jobs_job_integrity_revocations;
CREATE TRIGGER trg_jobs_job_integrity_revocations_validate_insert
BEFORE INSERT ON jobs_job_integrity_revocations FOR EACH ROW
EXECUTE FUNCTION validate_jobs_job_integrity_revocation_insert();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_control_monotonic ON jobs_job_integrity_control;
CREATE TRIGGER trg_jobs_job_integrity_control_monotonic
BEFORE UPDATE ON jobs_job_integrity_control FOR EACH ROW
EXECUTE FUNCTION enforce_jobs_job_integrity_control_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_job_integrity_control_no_delete ON jobs_job_integrity_control;
CREATE TRIGGER trg_jobs_job_integrity_control_no_delete
BEFORE DELETE ON jobs_job_integrity_control FOR EACH ROW
EXECUTE FUNCTION reject_jobs_job_integrity_immutable_mutation();

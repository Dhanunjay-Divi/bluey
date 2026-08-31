-- Target: SQLite
-- Account-independent, dual-role signed employer-identity and job-risk
-- authority. The migration seeds no trust, attestation, revocation, or head.

-- Migration 057 added the signed v2 release and activation contracts. SQLite
-- cannot alter an existing CHECK constraint, so rebuild only the immutable
-- signature-set table while foreign-key enforcement is disabled. The exact
-- copy, index, and immutable triggers make this safe when runtime migrations
-- replay on every startup.
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
DROP TABLE IF EXISTS jobs_managed_cloud_signature_sets_phase614b_replacement;
CREATE TABLE jobs_managed_cloud_signature_sets_phase614b_replacement (
  signature_set_sha256             TEXT PRIMARY KEY
    CHECK(length(signature_set_sha256) = 64
      AND lower(signature_set_sha256) = signature_set_sha256
      AND signature_set_sha256 NOT GLOB '*[^0-9a-f]*'),
  signature_set_id                 TEXT NOT NULL UNIQUE CHECK(length(signature_set_id) > 0),
  trust_generation                 INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role                             TEXT NOT NULL CHECK(role IN (
    'general_promotion', 'incident', 'promotion', 'release', 'root'
  )),
  target_audience                  TEXT NOT NULL CHECK(target_audience IN (
    'bluey-jobs-managed-cloud-activation-v1',
    'bluey-jobs-managed-cloud-activation-v2',
    'bluey-jobs-managed-cloud-cohort-v1',
    'bluey-jobs-managed-cloud-release-v1',
    'bluey-jobs-managed-cloud-release-v2',
    'bluey-jobs-managed-cloud-revocation-v1',
    'bluey-jobs-managed-cloud-rollback-v1',
    'bluey-jobs-managed-cloud-trust-policy-v1'
  )),
  target_sha256                    TEXT NOT NULL
    CHECK(length(target_sha256) = 64 AND lower(target_sha256) = target_sha256
      AND target_sha256 NOT GLOB '*[^0-9a-f]*'),
  signed_at_ms                     INTEGER NOT NULL CHECK(signed_at_ms >= 0),
  signature_count                  INTEGER NOT NULL CHECK(signature_count BETWEEN 1 AND 32),
  canonical_signature_set_base64url TEXT NOT NULL
    CHECK(length(canonical_signature_set_base64url) > 0),
  recorded_by                      TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                   INTEGER NOT NULL CHECK(recorded_at_ms >= signed_at_ms),
  UNIQUE(target_audience, target_sha256, trust_generation, role)
);
INSERT INTO jobs_managed_cloud_signature_sets_phase614b_replacement (
  signature_set_sha256, signature_set_id, trust_generation, role,
  target_audience, target_sha256, signed_at_ms, signature_count,
  canonical_signature_set_base64url, recorded_by, recorded_at_ms
)
SELECT
  signature_set_sha256, signature_set_id, trust_generation, role,
  target_audience, target_sha256, signed_at_ms, signature_count,
  canonical_signature_set_base64url, recorded_by, recorded_at_ms
FROM jobs_managed_cloud_signature_sets;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_signature_sets_no_update;
DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_signature_sets_no_delete;
DROP TABLE jobs_managed_cloud_signature_sets;
ALTER TABLE jobs_managed_cloud_signature_sets_phase614b_replacement
  RENAME TO jobs_managed_cloud_signature_sets;
CREATE INDEX idx_jobs_managed_cloud_signature_sets_target
  ON jobs_managed_cloud_signature_sets(
    target_audience, target_sha256, trust_generation, role
  );
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_update
BEFORE UPDATE ON jobs_managed_cloud_signature_sets BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature set is immutable');
END;
CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_delete
BEFORE DELETE ON jobs_managed_cloud_signature_sets BEGIN
  SELECT RAISE(ABORT, 'managed cloud signature set is immutable');
END;
COMMIT;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS jobs_job_integrity_trust_policies (
  policy_sha256                         TEXT PRIMARY KEY
    CHECK(length(policy_sha256) = 64 AND lower(policy_sha256) = policy_sha256
      AND policy_sha256 NOT GLOB '*[^0-9a-f]*'),
  policy_id                             TEXT NOT NULL UNIQUE
    CHECK(length(policy_id) BETWEEN 1 AND 120),
  trust_generation                     INTEGER NOT NULL UNIQUE
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256             TEXT UNIQUE
    CHECK(predecessor_policy_sha256 IS NULL OR
      (length(predecessor_policy_sha256) = 64
        AND lower(predecessor_policy_sha256) = predecessor_policy_sha256
        AND predecessor_policy_sha256 NOT GLOB '*[^0-9a-f]*')),
  root_anchor_sha256                    TEXT NOT NULL
    CHECK(length(root_anchor_sha256) = 64 AND lower(root_anchor_sha256) = root_anchor_sha256
      AND root_anchor_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_policy_base64url            TEXT NOT NULL
    CHECK(length(canonical_policy_base64url) BETWEEN 4 AND 87384),
  root_authorization_id                  TEXT NOT NULL UNIQUE
    CHECK(length(root_authorization_id) BETWEEN 1 AND 120),
  root_authorization_sha256             TEXT NOT NULL UNIQUE
    CHECK(length(root_authorization_sha256) = 64
      AND lower(root_authorization_sha256) = root_authorization_sha256
      AND root_authorization_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_root_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_root_authorization_base64url) BETWEEN 4 AND 87384),
  employer_identity_threshold           INTEGER NOT NULL
    CHECK(employer_identity_threshold BETWEEN 1 AND 16),
  job_risk_threshold                    INTEGER NOT NULL
    CHECK(job_risk_threshold BETWEEN 1 AND 16),
  revocation_threshold                  INTEGER NOT NULL
    CHECK(revocation_threshold BETWEEN 1 AND 16),
  maximum_positive_lifetime_ms          INTEGER NOT NULL
    CHECK(maximum_positive_lifetime_ms BETWEEN 60000 AND 31536000000),
  maximum_nonpositive_lifetime_ms       INTEGER NOT NULL
    CHECK(maximum_nonpositive_lifetime_ms BETWEEN 60000 AND 31536000000),
  maximum_clock_skew_ms                 INTEGER NOT NULL
    CHECK(maximum_clock_skew_ms BETWEEN 0 AND 300000),
  maximum_canonical_bytes               INTEGER NOT NULL
    CHECK(maximum_canonical_bytes BETWEEN 1024 AND 65536),
  maximum_identity_evidence_count       INTEGER NOT NULL
    CHECK(maximum_identity_evidence_count BETWEEN 1 AND 64),
  maximum_risk_evidence_count           INTEGER NOT NULL
    CHECK(maximum_risk_evidence_count BETWEEN 1 AND 64),
  allowed_providers_json                TEXT NOT NULL
    CHECK(json_valid(allowed_providers_json) AND json_type(allowed_providers_json) = 'array'),
  required_identity_methods_json        TEXT NOT NULL
    CHECK(json_valid(required_identity_methods_json)
      AND json_type(required_identity_methods_json) = 'array'),
  required_identity_evidence_classes_json TEXT NOT NULL
    CHECK(json_valid(required_identity_evidence_classes_json)
      AND json_type(required_identity_evidence_classes_json) = 'array'),
  required_risk_evidence_classes_json   TEXT NOT NULL
    CHECK(json_valid(required_risk_evidence_classes_json)
      AND json_type(required_risk_evidence_classes_json) = 'array'),
  allowed_identity_evidence_classes_json TEXT NOT NULL
    CHECK(json_valid(allowed_identity_evidence_classes_json)
      AND json_type(allowed_identity_evidence_classes_json) = 'array'),
  allowed_risk_evidence_classes_json    TEXT NOT NULL
    CHECK(json_valid(allowed_risk_evidence_classes_json)
      AND json_type(allowed_risk_evidence_classes_json) = 'array'),
  allowed_risk_signal_codes_json        TEXT NOT NULL
    CHECK(json_valid(allowed_risk_signal_codes_json)
      AND json_type(allowed_risk_signal_codes_json) = 'array'),
  allowed_risk_policy_sha256s_json      TEXT NOT NULL
    CHECK(json_valid(allowed_risk_policy_sha256s_json)
      AND json_type(allowed_risk_policy_sha256s_json) = 'array'),
  issued_at_ms                          INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  valid_from_ms                         INTEGER NOT NULL CHECK(valid_from_ms >= issued_at_ms),
  expires_at_ms                         INTEGER NOT NULL CHECK(expires_at_ms > valid_from_ms),
  recorded_by                           TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms                        INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
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
  trust_generation                   INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role                               TEXT NOT NULL
    CHECK(role IN ('employer_identity','job_risk','revocation')),
  threshold                          INTEGER NOT NULL CHECK(threshold BETWEEN 1 AND 16),
  key_id                             TEXT NOT NULL CHECK(length(key_id) BETWEEN 1 AND 120),
  public_key_base64url               TEXT NOT NULL CHECK(length(public_key_base64url) = 43),
  key_sha256                         TEXT NOT NULL
    CHECK(length(key_sha256) = 64 AND lower(key_sha256) = key_sha256
      AND key_sha256 NOT GLOB '*[^0-9a-f]*'),
  valid_from_ms                      INTEGER NOT NULL CHECK(valid_from_ms >= 0),
  expires_at_ms                      INTEGER NOT NULL CHECK(expires_at_ms > valid_from_ms),
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
  attestation_sha256                  TEXT PRIMARY KEY
    CHECK(length(attestation_sha256) = 64 AND lower(attestation_sha256) = attestation_sha256
      AND attestation_sha256 NOT GLOB '*[^0-9a-f]*'),
  attestation_id                      TEXT NOT NULL UNIQUE
    CHECK(length(attestation_id) BETWEEN 1 AND 120),
  policy_sha256                       TEXT NOT NULL,
  subject_sha256                      TEXT NOT NULL
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  source_material_sha256              TEXT NOT NULL
    CHECK(length(source_material_sha256) = 64 AND lower(source_material_sha256) = source_material_sha256
      AND source_material_sha256 NOT GLOB '*[^0-9a-f]*'),
  attestation_generation              INTEGER NOT NULL
    CHECK(attestation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_attestation_sha256      TEXT UNIQUE
    CHECK(predecessor_attestation_sha256 IS NULL OR
      (length(predecessor_attestation_sha256) = 64
        AND lower(predecessor_attestation_sha256) = predecessor_attestation_sha256
        AND predecessor_attestation_sha256 NOT GLOB '*[^0-9a-f]*')),
  canonical_job_id                    TEXT NOT NULL CHECK(length(canonical_job_id) BETWEEN 1 AND 240),
  provider_family                     TEXT NOT NULL CHECK(length(provider_family) BETWEEN 1 AND 64),
  provider_record_id                  TEXT NOT NULL CHECK(length(provider_record_id) BETWEEN 1 AND 512),
  provider_host                       TEXT NOT NULL CHECK(length(provider_host) BETWEEN 1 AND 253),
  provider_tenant                     TEXT NOT NULL CHECK(length(provider_tenant) BETWEEN 1 AND 240),
  provider_job                        TEXT NOT NULL CHECK(length(provider_job) BETWEEN 1 AND 512),
  provider_variant                    TEXT NOT NULL CHECK(length(provider_variant) BETWEEN 1 AND 120),
  canonical_application_url           TEXT NOT NULL CHECK(length(canonical_application_url) BETWEEN 8 AND 4096),
  application_domain                  TEXT NOT NULL CHECK(length(application_domain) BETWEEN 1 AND 253),
  ats_tenant_binding_sha256           TEXT NOT NULL
    CHECK(length(ats_tenant_binding_sha256) = 64
      AND lower(ats_tenant_binding_sha256) = ats_tenant_binding_sha256
      AND ats_tenant_binding_sha256 NOT GLOB '*[^0-9a-f]*'),
  employer_status                     TEXT NOT NULL CHECK(employer_status IN ('verified','unverified','mismatch')),
  canonical_employer_id               TEXT NOT NULL CHECK(length(canonical_employer_id) BETWEEN 1 AND 240),
  canonical_employer_domain           TEXT NOT NULL CHECK(length(canonical_employer_domain) BETWEEN 1 AND 253),
  verification_methods_json           TEXT NOT NULL
    CHECK(json_valid(verification_methods_json)
      AND json_type(verification_methods_json) = 'array'),
  identity_evidence_json              TEXT NOT NULL
    CHECK(json_valid(identity_evidence_json) AND json_type(identity_evidence_json) = 'array'),
  risk_status                         TEXT NOT NULL CHECK(risk_status IN ('clear','review_required','blocked')),
  risk_signal_codes_json              TEXT NOT NULL
    CHECK(json_valid(risk_signal_codes_json) AND json_type(risk_signal_codes_json) = 'array'),
  risk_policy_sha256                  TEXT NOT NULL
    CHECK(length(risk_policy_sha256) = 64 AND lower(risk_policy_sha256) = risk_policy_sha256
      AND risk_policy_sha256 NOT GLOB '*[^0-9a-f]*'),
  risk_input_sha256                   TEXT NOT NULL
    CHECK(length(risk_input_sha256) = 64 AND lower(risk_input_sha256) = risk_input_sha256
      AND risk_input_sha256 NOT GLOB '*[^0-9a-f]*'),
  risk_engine_release_sha256          TEXT NOT NULL
    CHECK(length(risk_engine_release_sha256) = 64
      AND lower(risk_engine_release_sha256) = risk_engine_release_sha256
      AND risk_engine_release_sha256 NOT GLOB '*[^0-9a-f]*'),
  risk_evidence_json                  TEXT NOT NULL
    CHECK(json_valid(risk_evidence_json) AND json_type(risk_evidence_json) = 'array'),
  canonical_attestation_base64url     TEXT NOT NULL
    CHECK(length(canonical_attestation_base64url) BETWEEN 4 AND 87384),
  employer_identity_authorization_id  TEXT NOT NULL UNIQUE
    CHECK(length(employer_identity_authorization_id) BETWEEN 1 AND 120),
  employer_authorization_sha256       TEXT NOT NULL UNIQUE
    CHECK(length(employer_authorization_sha256) = 64
      AND lower(employer_authorization_sha256) = employer_authorization_sha256
      AND employer_authorization_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_employer_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_employer_authorization_base64url) BETWEEN 4 AND 87384),
  job_risk_authorization_id           TEXT NOT NULL UNIQUE
    CHECK(length(job_risk_authorization_id) BETWEEN 1 AND 120),
  risk_authorization_sha256           TEXT NOT NULL UNIQUE
    CHECK(length(risk_authorization_sha256) = 64
      AND lower(risk_authorization_sha256) = risk_authorization_sha256
      AND risk_authorization_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_risk_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_risk_authorization_base64url) BETWEEN 4 AND 87384),
  assessed_at_ms                      INTEGER NOT NULL CHECK(assessed_at_ms >= 0),
  issued_at_ms                        INTEGER NOT NULL CHECK(issued_at_ms >= assessed_at_ms),
  not_before_ms                       INTEGER NOT NULL CHECK(not_before_ms >= 0),
  expires_at_ms                       INTEGER NOT NULL CHECK(expires_at_ms > not_before_ms),
  effective_expires_at_ms             INTEGER NOT NULL CHECK(effective_expires_at_ms > not_before_ms),
  recorded_by                         TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms                      INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
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
  revocation_sha256                 TEXT PRIMARY KEY
    CHECK(length(revocation_sha256) = 64 AND lower(revocation_sha256) = revocation_sha256
      AND revocation_sha256 NOT GLOB '*[^0-9a-f]*'),
  revocation_id                     TEXT NOT NULL UNIQUE CHECK(length(revocation_id) BETWEEN 1 AND 120),
  revocation_generation             INTEGER NOT NULL UNIQUE
    CHECK(revocation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_revocation_sha256     TEXT UNIQUE
    CHECK(predecessor_revocation_sha256 IS NULL OR
      (length(predecessor_revocation_sha256) = 64
        AND lower(predecessor_revocation_sha256) = predecessor_revocation_sha256
        AND predecessor_revocation_sha256 NOT GLOB '*[^0-9a-f]*')),
  policy_sha256                     TEXT NOT NULL,
  subject_kind                      TEXT NOT NULL CHECK(subject_kind IN (
    'trust_policy','trust_key','attestation','subject','canonical_employer',
    'identity_evidence','risk_evidence','risk_policy','risk_engine_release')),
  subject_id                        TEXT NOT NULL CHECK(length(subject_id) BETWEEN 1 AND 512),
  subject_sha256                    TEXT NOT NULL
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  reason_code                       TEXT NOT NULL CHECK(length(reason_code) BETWEEN 1 AND 64),
  reason_ref                        TEXT NOT NULL CHECK(length(reason_ref) BETWEEN 1 AND 240),
  effective_at_ms                   INTEGER NOT NULL CHECK(effective_at_ms >= 0),
  issued_at_ms                      INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  canonical_revocation_base64url    TEXT NOT NULL CHECK(length(canonical_revocation_base64url) BETWEEN 4 AND 87384),
  authorization_id                  TEXT NOT NULL UNIQUE CHECK(length(authorization_id) BETWEEN 1 AND 120),
  authorization_sha256              TEXT NOT NULL UNIQUE
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256
      AND authorization_sha256 NOT GLOB '*[^0-9a-f]*'),
  canonical_authorization_base64url TEXT NOT NULL CHECK(length(canonical_authorization_base64url) BETWEEN 4 AND 87384),
  recorded_by                       TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 240),
  recorded_at_ms                    INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
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
  transition_sha256                 TEXT PRIMARY KEY
    CHECK(length(transition_sha256) = 64 AND lower(transition_sha256) = transition_sha256
      AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
  subject_sha256                    TEXT NOT NULL
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision                     INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision            INTEGER NOT NULL CHECK(previous_head_revision >= 0),
  predecessor_transition_sha256     TEXT UNIQUE
    CHECK(predecessor_transition_sha256 IS NULL OR
      (length(predecessor_transition_sha256) = 64
        AND lower(predecessor_transition_sha256) = predecessor_transition_sha256
        AND predecessor_transition_sha256 NOT GLOB '*[^0-9a-f]*')),
  previous_attestation_sha256       TEXT,
  attestation_sha256                TEXT NOT NULL UNIQUE,
  attestation_generation            INTEGER NOT NULL
    CHECK(attestation_generation BETWEEN 1 AND 9007199254740991),
  policy_sha256                     TEXT NOT NULL,
  transition_actor                 TEXT NOT NULL CHECK(length(transition_actor) BETWEEN 1 AND 240),
  transitioned_at_ms               INTEGER NOT NULL CHECK(transitioned_at_ms >= 0),
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
  subject_sha256                    TEXT PRIMARY KEY
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256
      AND subject_sha256 NOT GLOB '*[^0-9a-f]*'),
  head_revision                     INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  transition_sha256                 TEXT NOT NULL UNIQUE,
  attestation_sha256                TEXT NOT NULL UNIQUE,
  attestation_generation            INTEGER NOT NULL
    CHECK(attestation_generation BETWEEN 1 AND 9007199254740991),
  policy_sha256                     TEXT NOT NULL,
  updated_at_ms                     INTEGER NOT NULL CHECK(updated_at_ms >= 0),
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
  singleton_id                      INTEGER PRIMARY KEY CHECK(singleton_id = 1),
  control_revision                  INTEGER NOT NULL CHECK(control_revision BETWEEN 0 AND 9007199254740991),
  current_policy_sha256             TEXT,
  current_trust_generation          INTEGER NOT NULL CHECK(current_trust_generation BETWEEN 0 AND 9007199254740991),
  current_revocation_sha256         TEXT,
  current_revocation_generation     INTEGER NOT NULL DEFAULT 0 CHECK(current_revocation_generation >= 0),
  updated_by                        TEXT NOT NULL CHECK(length(updated_by) BETWEEN 1 AND 240),
  updated_at_ms                     INTEGER NOT NULL CHECK(updated_at_ms >= 0),
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
) SELECT 1, 0, NULL, 0, NULL, 0, 'migration-scaffold', 0
   WHERE NOT EXISTS (SELECT 1 FROM jobs_job_integrity_control WHERE singleton_id=1);

-- Immutable signed rows and transition history.
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_policies_no_update
BEFORE UPDATE ON jobs_job_integrity_trust_policies BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust policy is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_policies_no_conflicting_insert
BEFORE INSERT ON jobs_job_integrity_trust_policies
WHEN EXISTS (
  SELECT 1 FROM jobs_job_integrity_trust_policies existing
   WHERE existing.policy_sha256=NEW.policy_sha256
      OR existing.policy_id=NEW.policy_id
      OR existing.trust_generation=NEW.trust_generation
      OR existing.root_authorization_id=NEW.root_authorization_id
      OR existing.root_authorization_sha256=NEW.root_authorization_sha256
      OR (NEW.predecessor_policy_sha256 IS NOT NULL
        AND existing.predecessor_policy_sha256=NEW.predecessor_policy_sha256)
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust policy identity already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_policies_validate_insert
BEFORE INSERT ON jobs_job_integrity_trust_policies
WHEN NEW.trust_generation > 1 AND NOT EXISTS (
  SELECT 1 FROM jobs_job_integrity_trust_policies predecessor
   WHERE predecessor.policy_sha256=NEW.predecessor_policy_sha256
     AND predecessor.trust_generation=NEW.trust_generation-1
     AND predecessor.root_anchor_sha256=NEW.root_anchor_sha256
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust policy root chain is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_policies_no_delete
BEFORE DELETE ON jobs_job_integrity_trust_policies BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust policy is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_keys_no_update
BEFORE UPDATE ON jobs_job_integrity_trust_keys BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust key is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_keys_no_conflicting_insert
BEFORE INSERT ON jobs_job_integrity_trust_keys
WHEN EXISTS (
  SELECT 1 FROM jobs_job_integrity_trust_keys existing
   WHERE (existing.policy_sha256=NEW.policy_sha256
      AND existing.role=NEW.role
      AND existing.key_id=NEW.key_id)
      OR ((existing.key_id=NEW.key_id
         OR existing.public_key_base64url=NEW.public_key_base64url
         OR existing.key_sha256=NEW.key_sha256)
       AND (existing.role<>NEW.role OR existing.key_id<>NEW.key_id
         OR existing.public_key_base64url<>NEW.public_key_base64url
         OR existing.key_sha256<>NEW.key_sha256))
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust key identity already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_keys_validate_insert
BEFORE INSERT ON jobs_job_integrity_trust_keys
WHEN NOT EXISTS (
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
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust key policy binding is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_trust_keys_no_delete
BEFORE DELETE ON jobs_job_integrity_trust_keys BEGIN
  SELECT RAISE(ABORT, 'job-integrity trust key is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_attestations_no_update
BEFORE UPDATE ON jobs_job_integrity_attestations BEGIN
  SELECT RAISE(ABORT, 'job-integrity attestation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_attestations_no_conflicting_insert
BEFORE INSERT ON jobs_job_integrity_attestations
WHEN EXISTS (
  SELECT 1 FROM jobs_job_integrity_attestations existing
   WHERE existing.attestation_sha256=NEW.attestation_sha256
      OR existing.attestation_id=NEW.attestation_id
      OR (existing.subject_sha256=NEW.subject_sha256
        AND existing.attestation_generation=NEW.attestation_generation)
      OR existing.employer_identity_authorization_id=NEW.employer_identity_authorization_id
      OR existing.employer_authorization_sha256=NEW.employer_authorization_sha256
      OR existing.job_risk_authorization_id=NEW.job_risk_authorization_id
      OR existing.risk_authorization_sha256=NEW.risk_authorization_sha256
      OR (NEW.predecessor_attestation_sha256 IS NOT NULL
        AND existing.predecessor_attestation_sha256=NEW.predecessor_attestation_sha256)
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity attestation identity already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_attestations_validate_insert
BEFORE INSERT ON jobs_job_integrity_attestations
WHEN NEW.attestation_generation<>1 AND NOT EXISTS (
  SELECT 1 FROM jobs_job_integrity_attestations predecessor
   WHERE predecessor.attestation_sha256=NEW.predecessor_attestation_sha256
     AND predecessor.subject_sha256=NEW.subject_sha256
     AND predecessor.attestation_generation=NEW.attestation_generation-1
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity attestation predecessor binding is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_attestations_no_delete
BEFORE DELETE ON jobs_job_integrity_attestations BEGIN
  SELECT RAISE(ABORT, 'job-integrity attestation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_revocations_no_update
BEFORE UPDATE ON jobs_job_integrity_revocations BEGIN
  SELECT RAISE(ABORT, 'job-integrity revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_revocations_no_conflicting_insert
BEFORE INSERT ON jobs_job_integrity_revocations
WHEN EXISTS (
  SELECT 1 FROM jobs_job_integrity_revocations existing
   WHERE existing.revocation_sha256=NEW.revocation_sha256
      OR existing.revocation_id=NEW.revocation_id
      OR existing.revocation_generation=NEW.revocation_generation
      OR existing.authorization_id=NEW.authorization_id
      OR existing.authorization_sha256=NEW.authorization_sha256
      OR (NEW.predecessor_revocation_sha256 IS NOT NULL
        AND existing.predecessor_revocation_sha256=NEW.predecessor_revocation_sha256)
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity revocation identity already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_revocations_validate_insert
BEFORE INSERT ON jobs_job_integrity_revocations
WHEN NEW.revocation_generation<>1 AND NOT EXISTS (
  SELECT 1 FROM jobs_job_integrity_revocations predecessor
   WHERE predecessor.revocation_sha256=NEW.predecessor_revocation_sha256
     AND predecessor.revocation_generation=NEW.revocation_generation-1
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity revocation predecessor binding is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_revocations_no_delete
BEFORE DELETE ON jobs_job_integrity_revocations BEGIN
  SELECT RAISE(ABORT, 'job-integrity revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_head_transitions_no_update
BEFORE UPDATE ON jobs_job_integrity_head_transitions BEGIN
  SELECT RAISE(ABORT, 'job-integrity head transition is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_head_transitions_no_conflicting_insert
BEFORE INSERT ON jobs_job_integrity_head_transitions
WHEN EXISTS (
  SELECT 1 FROM jobs_job_integrity_head_transitions existing
   WHERE existing.transition_sha256=NEW.transition_sha256
      OR existing.attestation_sha256=NEW.attestation_sha256
      OR (existing.subject_sha256=NEW.subject_sha256
        AND existing.head_revision=NEW.head_revision)
      OR (NEW.predecessor_transition_sha256 IS NOT NULL
        AND existing.predecessor_transition_sha256=NEW.predecessor_transition_sha256)
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity head transition identity already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_head_transitions_no_delete
BEFORE DELETE ON jobs_job_integrity_head_transitions BEGIN
  SELECT RAISE(ABORT, 'job-integrity head transition is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_head_transitions_validate_insert
BEFORE INSERT ON jobs_job_integrity_head_transitions
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_job_integrity_attestations attestation
   WHERE attestation.attestation_sha256 = NEW.attestation_sha256
     AND attestation.subject_sha256 = NEW.subject_sha256
     AND attestation.attestation_generation = NEW.attestation_generation
     AND attestation.policy_sha256 = NEW.policy_sha256
     AND attestation.predecessor_attestation_sha256 IS NEW.previous_attestation_sha256
)
OR (NEW.head_revision=1 AND (
  NEW.previous_head_revision<>0 OR NEW.predecessor_transition_sha256 IS NOT NULL
  OR NEW.previous_attestation_sha256 IS NOT NULL
))
OR (NEW.head_revision>1 AND NOT EXISTS (
  SELECT 1 FROM jobs_job_integrity_head_transitions predecessor
   WHERE predecessor.transition_sha256=NEW.predecessor_transition_sha256
     AND predecessor.subject_sha256=NEW.subject_sha256
     AND predecessor.head_revision=NEW.previous_head_revision
     AND predecessor.attestation_sha256=NEW.previous_attestation_sha256
)) BEGIN
  SELECT RAISE(ABORT, 'job-integrity transition attestation binding is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_heads_validate_insert
BEFORE INSERT ON jobs_job_integrity_heads
WHEN NEW.head_revision <> 1
  OR NOT EXISTS (
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
  ) BEGIN
  SELECT RAISE(ABORT, 'job-integrity initial head revision is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_heads_no_conflicting_insert
BEFORE INSERT ON jobs_job_integrity_heads
WHEN EXISTS (
  SELECT 1 FROM jobs_job_integrity_heads existing
   WHERE existing.subject_sha256=NEW.subject_sha256
      OR existing.transition_sha256=NEW.transition_sha256
      OR existing.attestation_sha256=NEW.attestation_sha256
) BEGIN
  SELECT RAISE(ABORT, 'job-integrity current head identity already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_heads_monotonic
BEFORE UPDATE ON jobs_job_integrity_heads
WHEN NEW.subject_sha256 <> OLD.subject_sha256
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
  )
BEGIN
  SELECT RAISE(ABORT, 'job-integrity head transition is not monotonic');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_heads_no_delete
BEFORE DELETE ON jobs_job_integrity_heads BEGIN
  SELECT RAISE(ABORT, 'job-integrity head cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_control_monotonic
BEFORE UPDATE ON jobs_job_integrity_control
WHEN NEW.singleton_id <> OLD.singleton_id
  OR NEW.control_revision <> OLD.control_revision + 1
  OR NEW.updated_at_ms < OLD.updated_at_ms
  OR (
    (NEW.current_policy_sha256 IS NOT OLD.current_policy_sha256
      OR NEW.current_trust_generation <> OLD.current_trust_generation)
    =
    (NEW.current_revocation_sha256 IS NOT OLD.current_revocation_sha256
      OR NEW.current_revocation_generation <> OLD.current_revocation_generation)
  )
  OR (
    (NEW.current_policy_sha256 IS NOT OLD.current_policy_sha256
      OR NEW.current_trust_generation <> OLD.current_trust_generation)
    AND (
      NEW.current_trust_generation <> OLD.current_trust_generation + 1
      OR NEW.current_revocation_sha256 IS NOT OLD.current_revocation_sha256
      OR NEW.current_revocation_generation <> OLD.current_revocation_generation
      OR NOT EXISTS (
        SELECT 1 FROM jobs_job_integrity_trust_policies policy
         WHERE policy.policy_sha256 = NEW.current_policy_sha256
           AND policy.trust_generation = NEW.current_trust_generation
           AND ((OLD.current_policy_sha256 IS NULL
                  AND policy.predecessor_policy_sha256 IS NULL)
             OR policy.predecessor_policy_sha256 = OLD.current_policy_sha256)
      )
    )
  )
  OR (
    (NEW.current_revocation_sha256 IS NOT OLD.current_revocation_sha256
      OR NEW.current_revocation_generation <> OLD.current_revocation_generation)
    AND (
      NEW.current_revocation_generation <> OLD.current_revocation_generation + 1
      OR NEW.current_policy_sha256 IS NOT OLD.current_policy_sha256
      OR NEW.current_trust_generation <> OLD.current_trust_generation
      OR NOT EXISTS (
        SELECT 1 FROM jobs_job_integrity_revocations revocation
         WHERE revocation.revocation_sha256 = NEW.current_revocation_sha256
           AND revocation.revocation_generation = NEW.current_revocation_generation
           AND revocation.policy_sha256 = OLD.current_policy_sha256
           AND ((OLD.current_revocation_sha256 IS NULL
                  AND revocation.predecessor_revocation_sha256 IS NULL)
             OR revocation.predecessor_revocation_sha256 = OLD.current_revocation_sha256)
      )
    )
  )
BEGIN
  SELECT RAISE(ABORT, 'job-integrity control transition is not monotonic');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_control_no_insert
BEFORE INSERT ON jobs_job_integrity_control
WHEN EXISTS (SELECT 1 FROM jobs_job_integrity_control) BEGIN
  SELECT RAISE(ABORT, 'job-integrity control singleton already exists');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_job_integrity_control_no_delete
BEFORE DELETE ON jobs_job_integrity_control BEGIN
  SELECT RAISE(ABORT, 'job-integrity control cannot be deleted');
END;

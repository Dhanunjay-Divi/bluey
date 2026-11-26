-- Target: PostgreSQL
-- Signed, immutable ATS certification evidence and authorities. Imported
-- activations remain inert until an explicit compare-and-swap head transition.

CREATE TABLE IF NOT EXISTS jobs_ats_certification_trust_policies (
  policy_sha256                   TEXT PRIMARY KEY
    CHECK(length(policy_sha256) = 64 AND lower(policy_sha256) = policy_sha256),
  policy_id                       TEXT NOT NULL UNIQUE CHECK(length(policy_id) > 0),
  trust_generation               BIGINT NOT NULL UNIQUE
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256      TEXT UNIQUE,
  maximum_clock_skew_ms          BIGINT NOT NULL CHECK(maximum_clock_skew_ms BETWEEN 0 AND 300000),
  maximum_manifest_size_bytes    BIGINT NOT NULL CHECK(maximum_manifest_size_bytes BETWEEN 1024 AND 65536),
  maximum_target_count           BIGINT NOT NULL CHECK(maximum_target_count BETWEEN 1 AND 32),
  maximum_observation_count      BIGINT NOT NULL CHECK(maximum_observation_count BETWEEN 1 AND 64),
  maximum_evidence_object_count  BIGINT NOT NULL CHECK(maximum_evidence_object_count BETWEEN 1 AND 64),
  canonical_policy_base64url     TEXT NOT NULL CHECK(length(canonical_policy_base64url) > 0),
  authorization_sha256           TEXT NOT NULL UNIQUE
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  root_trust_anchor_sha256       TEXT NOT NULL
    CHECK(length(root_trust_anchor_sha256) = 64
      AND lower(root_trust_anchor_sha256) = root_trust_anchor_sha256),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  valid_from_ms                   BIGINT NOT NULL CHECK(valid_from_ms >= issued_at_ms),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > valid_from_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= valid_from_ms),
  FOREIGN KEY(predecessor_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT,
  CHECK(
    (trust_generation = 1 AND predecessor_policy_sha256 IS NULL)
    OR (trust_generation > 1 AND predecessor_policy_sha256 IS NOT NULL
      AND length(predecessor_policy_sha256) = 64
      AND lower(predecessor_policy_sha256) = predecessor_policy_sha256)
  )
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_trust_keys (
  policy_sha256                   TEXT NOT NULL,
  trust_generation               BIGINT NOT NULL,
  role                            TEXT NOT NULL
    CHECK(role IN ('activation', 'evidence', 'layout_observation', 'manifest', 'revocation')),
  threshold                       BIGINT NOT NULL CHECK(threshold BETWEEN 1 AND 16),
  key_id                          TEXT NOT NULL CHECK(length(key_id) > 0),
  public_key_base64url            TEXT NOT NULL CHECK(length(public_key_base64url) > 0),
  key_state                       TEXT NOT NULL CHECK(key_state = 'active'),
  valid_from_ms                   BIGINT NOT NULL CHECK(valid_from_ms >= 0),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > valid_from_ms),
  PRIMARY KEY(policy_sha256, role, key_id),
  UNIQUE(policy_sha256, key_id),
  UNIQUE(policy_sha256, public_key_base64url),
  FOREIGN KEY(policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_trust_head (
  singleton_id                    BIGINT PRIMARY KEY CHECK(singleton_id = 1),
  current_policy_sha256           TEXT NOT NULL UNIQUE,
  current_trust_generation        BIGINT NOT NULL UNIQUE
    CHECK(current_trust_generation BETWEEN 1 AND 9007199254740991),
  head_revision                   BIGINT NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  root_trust_anchor_sha256        TEXT NOT NULL
    CHECK(length(root_trust_anchor_sha256) = 64
      AND lower(root_trust_anchor_sha256) = root_trust_anchor_sha256),
  updated_by                      TEXT NOT NULL CHECK(length(updated_by) > 0),
  updated_at_ms                   BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  FOREIGN KEY(current_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(current_trust_generation)
    REFERENCES jobs_ats_certification_trust_policies(trust_generation) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_evidence (
  evidence_sha256                 TEXT PRIMARY KEY
    CHECK(length(evidence_sha256) = 64 AND lower(evidence_sha256) = evidence_sha256),
  evidence_id                     TEXT NOT NULL UNIQUE CHECK(length(evidence_id) > 0),
  provider                        TEXT NOT NULL
    CHECK(provider IN ('ashby', 'greenhouse', 'lever', 'smartrecruiters', 'workday')),
  target_key                      TEXT NOT NULL CHECK(length(target_key) > 0),
  variant_key                     TEXT NOT NULL CHECK(length(variant_key) > 0),
  surface_sha256                  TEXT NOT NULL
    CHECK(length(surface_sha256) = 64 AND lower(surface_sha256) = surface_sha256),
  source_kind                     TEXT NOT NULL CHECK(source_kind IN (
    'authorized_canary', 'authorized_sandbox', 'fault_injection', 'synthetic'
  )),
  object_key                      TEXT NOT NULL CHECK(length(object_key) > 0),
  object_sha256                   TEXT NOT NULL
    CHECK(length(object_sha256) = 64 AND lower(object_sha256) = object_sha256),
  object_size_bytes               BIGINT NOT NULL CHECK(object_size_bytes > 0),
  media_type                      TEXT NOT NULL CHECK(length(media_type) > 0),
  provenance_sha256               TEXT NOT NULL
    CHECK(length(provenance_sha256) = 64 AND lower(provenance_sha256) = provenance_sha256),
  authorization_ref               TEXT NOT NULL CHECK(length(authorization_ref) > 0),
  sanitizer_version               TEXT NOT NULL CHECK(length(sanitizer_version) > 0),
  canonical_evidence_base64url    TEXT NOT NULL CHECK(length(canonical_evidence_base64url) > 0),
  authorization_sha256            TEXT NOT NULL
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  trust_policy_sha256             TEXT NOT NULL
    CHECK(length(trust_policy_sha256) = 64 AND lower(trust_policy_sha256) = trust_policy_sha256),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  captured_at_ms                  BIGINT NOT NULL CHECK(captured_at_ms >= 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= captured_at_ms),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > issued_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(provider, target_key, variant_key, surface_sha256, evidence_id),
  FOREIGN KEY(trust_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_evidence_scope
  ON jobs_ats_certification_evidence(
    provider, target_key, variant_key, surface_sha256, expires_at_ms
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_layout_observations (
  observation_sha256              TEXT PRIMARY KEY
    CHECK(length(observation_sha256) = 64 AND lower(observation_sha256) = observation_sha256),
  observation_id                  TEXT NOT NULL UNIQUE CHECK(length(observation_id) > 0),
  provider                        TEXT NOT NULL
    CHECK(provider IN ('ashby', 'greenhouse', 'lever', 'smartrecruiters', 'workday')),
  target_fingerprint_sha256       TEXT NOT NULL
    CHECK(length(target_fingerprint_sha256) = 64
      AND lower(target_fingerprint_sha256) = target_fingerprint_sha256),
  page_variant                    TEXT NOT NULL CHECK(length(page_variant) > 0),
  surface_sha256                  TEXT NOT NULL
    CHECK(length(surface_sha256) = 64 AND lower(surface_sha256) = surface_sha256),
  adapter_version                 TEXT NOT NULL CHECK(length(adapter_version) > 0),
  runner_target_sha256            TEXT NOT NULL
    CHECK(length(runner_target_sha256) = 64 AND lower(runner_target_sha256) = runner_target_sha256),
  evidence_class                  TEXT NOT NULL
    CHECK(evidence_class IN ('synthetic', 'authorized_sandbox', 'authorized_live')),
  predecessor_observation_sha256  TEXT,
  canonical_observation_base64url TEXT NOT NULL CHECK(length(canonical_observation_base64url) > 0),
  authorization_sha256            TEXT NOT NULL
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  trust_policy_sha256             TEXT NOT NULL,
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  observed_at_ms                  BIGINT NOT NULL CHECK(observed_at_ms >= 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= observed_at_ms),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > issued_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  FOREIGN KEY(predecessor_observation_sha256)
    REFERENCES jobs_ats_certification_layout_observations(observation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(trust_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_layout_observations_target
  ON jobs_ats_certification_layout_observations(
    provider, target_fingerprint_sha256, page_variant, adapter_version,
    runner_target_sha256, expires_at_ms
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_manifests (
  manifest_sha256                 TEXT PRIMARY KEY
    CHECK(length(manifest_sha256) = 64 AND lower(manifest_sha256) = manifest_sha256),
  certification_id                TEXT NOT NULL UNIQUE CHECK(length(certification_id) > 0),
  manifest_generation             BIGINT NOT NULL
    CHECK(manifest_generation BETWEEN 1 AND 9007199254740991),
  predecessor_manifest_sha256     TEXT,
  provider                        TEXT NOT NULL
    CHECK(provider IN ('ashby', 'greenhouse', 'lever', 'smartrecruiters', 'workday')),
  target_key                      TEXT NOT NULL CHECK(length(target_key) > 0),
  allowed_provider_hosts_json     TEXT NOT NULL
    CHECK(length(allowed_provider_hosts_json) BETWEEN 1 AND 2048),
  variant_key                     TEXT NOT NULL CHECK(length(variant_key) > 0),
  surface_sha256                  TEXT NOT NULL
    CHECK(length(surface_sha256) = 64 AND lower(surface_sha256) = surface_sha256),
  scope_sha256                    TEXT NOT NULL
    CHECK(length(scope_sha256) = 64 AND lower(scope_sha256) = scope_sha256),
  adapter_version                 TEXT NOT NULL CHECK(length(adapter_version) > 0),
  final_submit_control_id         TEXT NOT NULL CHECK(length(final_submit_control_id) > 0),
  adapter_bundle_sha256           TEXT NOT NULL
    CHECK(length(adapter_bundle_sha256) = 64
      AND lower(adapter_bundle_sha256) = adapter_bundle_sha256),
  source_commit                   TEXT NOT NULL
    CHECK(length(source_commit) = 40 AND lower(source_commit) = source_commit),
  layout_contract_version         BIGINT NOT NULL
    CHECK(layout_contract_version BETWEEN 1 AND 9007199254740991),
  layout_contract_sha256          TEXT NOT NULL
    CHECK(length(layout_contract_sha256) = 64
      AND lower(layout_contract_sha256) = layout_contract_sha256),
  maximum_capability              TEXT NOT NULL
    CHECK(maximum_capability IN ('observe_only', 'reviewed_submit', 'unattended_submit')),
  layout_set_sha256               TEXT NOT NULL
    CHECK(length(layout_set_sha256) = 64 AND lower(layout_set_sha256) = layout_set_sha256),
  suite_id                        TEXT NOT NULL CHECK(length(suite_id) > 0),
  suite_version                   TEXT NOT NULL CHECK(length(suite_version) > 0),
  suite_manifest_sha256           TEXT NOT NULL
    CHECK(length(suite_manifest_sha256) = 64
      AND lower(suite_manifest_sha256) = suite_manifest_sha256),
  layout_observation_count        BIGINT NOT NULL CHECK(layout_observation_count BETWEEN 1 AND 64),
  check_result_count              BIGINT NOT NULL CHECK(check_result_count BETWEEN 1 AND 1536),
  hard_filter_violations          BIGINT NOT NULL CHECK(hard_filter_violations = 0),
  unsupported_factual_claims      BIGINT NOT NULL CHECK(unsupported_factual_claims = 0),
  duplicate_submit_activations    BIGINT NOT NULL CHECK(duplicate_submit_activations = 0),
  false_submitted_states          BIGINT NOT NULL CHECK(false_submitted_states = 0),
  incomplete_receipts             BIGINT NOT NULL CHECK(incomplete_receipts = 0),
  pii_bearing_observations        BIGINT NOT NULL CHECK(pii_bearing_observations = 0),
  evidence_count                  BIGINT NOT NULL CHECK(evidence_count BETWEEN 1 AND 64),
  runtime_target_count            BIGINT NOT NULL CHECK(runtime_target_count BETWEEN 1 AND 32),
  canonical_manifest_base64url    TEXT NOT NULL CHECK(length(canonical_manifest_base64url) > 0),
  authorization_sha256            TEXT NOT NULL
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  trust_policy_sha256             TEXT NOT NULL
    CHECK(length(trust_policy_sha256) = 64 AND lower(trust_policy_sha256) = trust_policy_sha256),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  tested_at_ms                    BIGINT NOT NULL CHECK(tested_at_ms >= 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  not_before_ms                   BIGINT NOT NULL CHECK(not_before_ms >= issued_at_ms),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > not_before_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  CHECK(tested_at_ms <= issued_at_ms),
  UNIQUE(provider, target_key, variant_key, surface_sha256, manifest_generation),
  UNIQUE(manifest_sha256, provider, adapter_version),
  UNIQUE(manifest_sha256, scope_sha256, provider, target_key, variant_key, surface_sha256),
  CHECK(
    maximum_capability = 'observe_only'
    OR (provider = 'greenhouse' AND adapter_version = '2026.07.1-beta.1'
      AND target_key LIKE 'greenhouse:%:%'
      AND allowed_provider_hosts_json = '["boards.greenhouse.io","job-boards.greenhouse.io"]'
      AND final_submit_control_id = 'greenhouse_submit_application')
    OR (provider = 'lever' AND adapter_version = '2026.07.0-beta.1'
      AND final_submit_control_id = 'lever_application_submit'
      AND ((target_key LIKE 'lever:jobs.lever.co:%:%'
          AND allowed_provider_hosts_json = '["jobs.lever.co"]')
        OR (target_key LIKE 'lever:jobs.eu.lever.co:%:%'
          AND allowed_provider_hosts_json = '["jobs.eu.lever.co"]')))
  ),
  FOREIGN KEY(predecessor_manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(trust_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_manifests_scope
  ON jobs_ats_certification_manifests(
    scope_sha256, manifest_generation DESC, expires_at_ms
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_manifest_layouts (
  manifest_sha256                 TEXT NOT NULL,
  observation_sha256              TEXT NOT NULL,
  ordinal                         BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 63),
  PRIMARY KEY(manifest_sha256, observation_sha256),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(observation_sha256)
    REFERENCES jobs_ats_certification_layout_observations(observation_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_manifest_check_results (
  manifest_sha256                 TEXT NOT NULL,
  runner_target_sha256            TEXT NOT NULL,
  check_id                        TEXT NOT NULL CHECK(length(check_id) > 0),
  evidence_class                  TEXT NOT NULL
    CHECK(evidence_class IN ('synthetic', 'authorized_sandbox', 'authorized_live')),
  passed_count                    BIGINT NOT NULL CHECK(passed_count > 0),
  failed_count                    BIGINT NOT NULL CHECK(failed_count = 0),
  skipped_count                   BIGINT NOT NULL CHECK(skipped_count = 0),
  ordinal                         BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 1535),
  PRIMARY KEY(manifest_sha256, runner_target_sha256, check_id, evidence_class),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_manifest_evidence (
  manifest_sha256                 TEXT NOT NULL,
  evidence_sha256                 TEXT NOT NULL,
  ordinal                         BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 63),
  PRIMARY KEY(manifest_sha256, evidence_sha256),
  UNIQUE(manifest_sha256, ordinal),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(evidence_sha256)
    REFERENCES jobs_ats_certification_evidence(evidence_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_manifest_evidence_evidence
  ON jobs_ats_certification_manifest_evidence(evidence_sha256, manifest_sha256);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_runtime_targets (
  manifest_sha256                 TEXT NOT NULL,
  runtime_kind                    TEXT NOT NULL CHECK(runtime_kind IN ('cloud', 'local')),
  runtime_id                      TEXT NOT NULL CHECK(length(runtime_id) > 0),
  runtime_sha256                  TEXT NOT NULL
    CHECK(length(runtime_sha256) = 64 AND lower(runtime_sha256) = runtime_sha256),
  platform                        TEXT NOT NULL CHECK(platform IN ('linux', 'macos', 'windows')),
  architecture                    TEXT NOT NULL CHECK(architecture IN ('arm64', 'x86_64')),
  automation_bundle_sha256        TEXT NOT NULL
    CHECK(length(automation_bundle_sha256) = 64
      AND lower(automation_bundle_sha256) = automation_bundle_sha256),
  browser_release_manifest_sha256 TEXT,
  browser_artifact_sha256         TEXT,
  browser_build_descriptor_sha256 TEXT,
  runner_build_id                 TEXT,
  runner_image_sha256             TEXT,
  playwright_version              TEXT NOT NULL CHECK(length(playwright_version) > 0),
  chromium_revision               TEXT NOT NULL CHECK(length(chromium_revision) > 0),
  chromium_executable_sha256      TEXT NOT NULL
    CHECK(length(chromium_executable_sha256) = 64
      AND lower(chromium_executable_sha256) = chromium_executable_sha256),
  ordinal                         BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 31),
  PRIMARY KEY(manifest_sha256, runtime_kind, runtime_id),
  UNIQUE(manifest_sha256, ordinal),
  UNIQUE(manifest_sha256, runtime_id),
  CHECK(
    (runtime_kind = 'local'
      AND browser_release_manifest_sha256 IS NOT NULL
      AND length(browser_release_manifest_sha256) = 64
      AND lower(browser_release_manifest_sha256) = browser_release_manifest_sha256
      AND browser_artifact_sha256 IS NOT NULL AND length(browser_artifact_sha256) = 64
      AND lower(browser_artifact_sha256) = browser_artifact_sha256
      AND browser_build_descriptor_sha256 IS NOT NULL
      AND length(browser_build_descriptor_sha256) = 64
      AND lower(browser_build_descriptor_sha256) = browser_build_descriptor_sha256
      AND runner_build_id IS NULL AND runner_image_sha256 IS NULL)
    OR
    (runtime_kind = 'cloud'
      AND browser_release_manifest_sha256 IS NULL
      AND browser_artifact_sha256 IS NULL AND browser_build_descriptor_sha256 IS NULL
      AND runner_build_id IS NOT NULL AND length(runner_build_id) > 0
      AND runner_image_sha256 IS NOT NULL AND length(runner_image_sha256) = 64
      AND lower(runner_image_sha256) = runner_image_sha256)
  ),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_runtime_targets_runtime
  ON jobs_ats_certification_runtime_targets(runtime_kind, runtime_id, runtime_sha256);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_activations (
  activation_sha256               TEXT PRIMARY KEY
    CHECK(length(activation_sha256) = 64 AND lower(activation_sha256) = activation_sha256),
  activation_id                   TEXT NOT NULL UNIQUE CHECK(length(activation_id) > 0),
  activation_generation           BIGINT NOT NULL
    CHECK(activation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_activation_sha256   TEXT,
  manifest_sha256                 TEXT NOT NULL,
  provider                        TEXT NOT NULL,
  adapter_version                 TEXT NOT NULL CHECK(length(adapter_version) > 0),
  scope_sha256                    TEXT NOT NULL
    CHECK(length(scope_sha256) = 64 AND lower(scope_sha256) = scope_sha256),
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general', 'shadow')),
  channel_sequence                BIGINT NOT NULL
    CHECK(channel_sequence BETWEEN 1 AND 9007199254740991),
  capability                      TEXT NOT NULL
    CHECK(capability IN ('observe_only', 'reviewed_submit', 'unattended_submit')),
  account_allowlist_sha256        TEXT,
  canary_max_submissions          BIGINT NOT NULL
    CHECK(canary_max_submissions BETWEEN 0 AND 10000),
  canary_account_cap              BIGINT NOT NULL CHECK(canary_account_cap BETWEEN 0 AND 10000),
  canary_concurrency_cap          BIGINT NOT NULL CHECK(canary_concurrency_cap BETWEEN 0 AND 1000),
  canary_daily_side_effect_cap    BIGINT NOT NULL
    CHECK(canary_daily_side_effect_cap BETWEEN 0 AND 10000),
  canary_evidence_manifest_sha256 TEXT,
  approval_ref                    TEXT NOT NULL CHECK(length(approval_ref) > 0),
  canonical_activation_base64url  TEXT NOT NULL CHECK(length(canonical_activation_base64url) > 0),
  authorization_sha256            TEXT NOT NULL
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  trust_policy_sha256             TEXT NOT NULL
    CHECK(length(trust_policy_sha256) = 64 AND lower(trust_policy_sha256) = trust_policy_sha256),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  not_before_ms                   BIGINT NOT NULL CHECK(not_before_ms >= issued_at_ms),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > not_before_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(scope_sha256, channel, channel_sequence),
  UNIQUE(activation_sha256, manifest_sha256, scope_sha256, channel, channel_sequence),
  CHECK(
    (channel = 'shadow' AND capability = 'observe_only'
      AND account_allowlist_sha256 IS NULL AND canary_max_submissions = 0)
    OR
    (channel = 'canary' AND account_allowlist_sha256 IS NOT NULL
      AND length(account_allowlist_sha256) = 64
      AND lower(account_allowlist_sha256) = account_allowlist_sha256
      AND canary_max_submissions > 0
      AND canary_account_cap > 0 AND canary_concurrency_cap > 0
      AND canary_account_cap <= canary_max_submissions
      AND canary_concurrency_cap <= canary_max_submissions
      AND canary_daily_side_effect_cap > 0
      AND canary_daily_side_effect_cap <= canary_max_submissions
      AND canary_evidence_manifest_sha256 IS NOT NULL
      AND length(canary_evidence_manifest_sha256) = 64
      AND canary_evidence_manifest_sha256 = manifest_sha256)
    OR
    (channel = 'general' AND account_allowlist_sha256 IS NULL
      AND canary_max_submissions = 0)
  ),
  CHECK(
    channel = 'canary'
    OR (canary_account_cap = 0 AND canary_concurrency_cap = 0
      AND canary_daily_side_effect_cap = 0 AND canary_evidence_manifest_sha256 IS NULL)
  ),
  CHECK(
    channel = 'shadow'
    OR (provider = 'greenhouse' AND adapter_version = '2026.07.1-beta.1')
    OR (provider = 'lever' AND adapter_version = '2026.07.0-beta.1')
  ),
  FOREIGN KEY(manifest_sha256, provider, adapter_version)
    REFERENCES jobs_ats_certification_manifests(
      manifest_sha256, provider, adapter_version
    ) ON DELETE RESTRICT,
  FOREIGN KEY(predecessor_activation_sha256)
    REFERENCES jobs_ats_certification_activations(activation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(trust_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_activations_scope
  ON jobs_ats_certification_activations(
    scope_sha256, channel, channel_sequence DESC, expires_at_ms
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_revocations (
  revocation_sha256               TEXT PRIMARY KEY
    CHECK(length(revocation_sha256) = 64 AND lower(revocation_sha256) = revocation_sha256),
  revocation_id                   TEXT NOT NULL UNIQUE CHECK(length(revocation_id) > 0),
  revocation_generation           BIGINT NOT NULL
    CHECK(revocation_generation BETWEEN 1 AND 9007199254740991),
  predecessor_revocation_sha256  TEXT UNIQUE,
  subject_kind                    TEXT NOT NULL CHECK(subject_kind IN (
    'activation', 'adapter_bundle', 'browser_release_manifest', 'evidence',
    'layout_observation', 'manifest', 'policy', 'runner_build', 'runner_image',
    'runtime', 'scope', 'target', 'trust_key'
  )),
  subject_id                      TEXT NOT NULL CHECK(length(subject_id) > 0),
  subject_sha256                  TEXT NOT NULL
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  canonical_revocation_base64url  TEXT NOT NULL CHECK(length(canonical_revocation_base64url) > 0),
  authorization_sha256            TEXT NOT NULL
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  trust_policy_sha256             TEXT NOT NULL
    CHECK(length(trust_policy_sha256) = 64 AND lower(trust_policy_sha256) = trust_policy_sha256),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  effective_at_ms                 BIGINT NOT NULL CHECK(effective_at_ms >= issued_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(trust_policy_sha256, revocation_generation),
  UNIQUE(subject_kind, subject_id, subject_sha256),
  FOREIGN KEY(predecessor_revocation_sha256)
    REFERENCES jobs_ats_certification_revocations(revocation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(trust_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT,
  CHECK(
    (revocation_generation = 1 AND predecessor_revocation_sha256 IS NULL)
    OR (revocation_generation > 1 AND predecessor_revocation_sha256 IS NOT NULL
      AND length(predecessor_revocation_sha256) = 64
      AND lower(predecessor_revocation_sha256) = predecessor_revocation_sha256)
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_revocations_subject
  ON jobs_ats_certification_revocations(
    subject_kind, subject_id, subject_sha256, effective_at_ms
  );

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_revocations_generation
  ON jobs_ats_certification_revocations(
    trust_policy_sha256, revocation_generation DESC
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_head_transitions (
  transition_sha256               TEXT PRIMARY KEY
    CHECK(length(transition_sha256) = 64 AND lower(transition_sha256) = transition_sha256),
  scope_sha256                    TEXT NOT NULL
    CHECK(length(scope_sha256) = 64 AND lower(scope_sha256) = scope_sha256),
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general', 'shadow')),
  head_revision                   BIGINT NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision          BIGINT NOT NULL CHECK(previous_head_revision >= 0),
  previous_transition_sha256      TEXT,
  previous_activation_sha256      TEXT,
  next_activation_sha256          TEXT NOT NULL,
  next_channel_sequence           BIGINT NOT NULL
    CHECK(next_channel_sequence BETWEEN 1 AND 9007199254740991),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(scope_sha256, channel, head_revision),
  UNIQUE(
    transition_sha256, scope_sha256, channel, head_revision, next_activation_sha256
  ),
  CHECK(
    (previous_head_revision = 0 AND previous_transition_sha256 IS NULL
      AND previous_activation_sha256 IS NULL AND head_revision = 1)
    OR
    (previous_head_revision > 0 AND previous_transition_sha256 IS NOT NULL
      AND length(previous_transition_sha256) = 64
      AND previous_activation_sha256 IS NOT NULL
      AND length(previous_activation_sha256) = 64
      AND head_revision = previous_head_revision + 1)
  ),
  FOREIGN KEY(next_activation_sha256)
    REFERENCES jobs_ats_certification_activations(activation_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_heads (
  scope_sha256                    TEXT NOT NULL,
  channel                         TEXT NOT NULL CHECK(channel IN ('canary', 'general', 'shadow')),
  head_revision                   BIGINT NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  current_transition_sha256       TEXT NOT NULL,
  current_activation_sha256       TEXT NOT NULL,
  current_channel_sequence        BIGINT NOT NULL
    CHECK(current_channel_sequence BETWEEN 1 AND 9007199254740991),
  updated_by                      TEXT NOT NULL CHECK(length(updated_by) > 0),
  updated_at_ms                   BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  PRIMARY KEY(scope_sha256, channel),
  FOREIGN KEY(
    current_transition_sha256, scope_sha256, channel, head_revision,
    current_activation_sha256
  ) REFERENCES jobs_ats_certification_head_transitions(
    transition_sha256, scope_sha256, channel, head_revision, next_activation_sha256
  ) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_heads_activation
  ON jobs_ats_certification_heads(current_activation_sha256, channel);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_quarantine_commands (
  command_sha256                  TEXT PRIMARY KEY
    CHECK(length(command_sha256) = 64 AND lower(command_sha256) = command_sha256),
  command_id                      TEXT NOT NULL UNIQUE CHECK(length(command_id) > 0),
  command_generation              BIGINT NOT NULL
    CHECK(command_generation BETWEEN 1 AND 9007199254740991),
  scope_kind                      TEXT NOT NULL CHECK(scope_kind IN (
    'activation', 'adapter', 'provider', 'runtime', 'surface', 'target'
  )),
  scope_id                        TEXT NOT NULL CHECK(length(scope_id) > 0),
  scope_sha256                    TEXT NOT NULL
    CHECK(length(scope_sha256) = 64 AND lower(scope_sha256) = scope_sha256),
  command_sequence                BIGINT NOT NULL
    CHECK(command_sequence BETWEEN 1 AND 9007199254740991),
  predecessor_command_sha256      TEXT,
  action                          TEXT NOT NULL CHECK(action IN ('quarantine', 'release')),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  canonical_command_base64url     TEXT NOT NULL CHECK(length(canonical_command_base64url) > 0),
  authorization_sha256            TEXT NOT NULL
    CHECK(length(authorization_sha256) = 64 AND lower(authorization_sha256) = authorization_sha256),
  trust_policy_sha256             TEXT NOT NULL
    CHECK(length(trust_policy_sha256) = 64 AND lower(trust_policy_sha256) = trust_policy_sha256),
  canonical_authorization_base64url TEXT NOT NULL
    CHECK(length(canonical_authorization_base64url) > 0),
  issued_at_ms                    BIGINT NOT NULL CHECK(issued_at_ms >= 0),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(scope_kind, scope_id, scope_sha256, command_sequence),
  UNIQUE(command_sha256, scope_kind, scope_id, scope_sha256, command_sequence),
  FOREIGN KEY(predecessor_command_sha256)
    REFERENCES jobs_ats_certification_quarantine_commands(command_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(trust_policy_sha256)
    REFERENCES jobs_ats_certification_trust_policies(policy_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_quarantine_commands_scope
  ON jobs_ats_certification_quarantine_commands(
    scope_kind, scope_id, scope_sha256, command_sequence DESC
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_quarantine_heads (
  scope_kind                      TEXT NOT NULL,
  scope_id                        TEXT NOT NULL,
  scope_sha256                    TEXT NOT NULL,
  head_revision                   BIGINT NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  current_command_sha256          TEXT NOT NULL,
  current_command_sequence        BIGINT NOT NULL
    CHECK(current_command_sequence BETWEEN 1 AND 9007199254740991),
  state                           TEXT NOT NULL CHECK(state IN ('quarantined', 'released')),
  updated_by                      TEXT NOT NULL CHECK(length(updated_by) > 0),
  updated_at_ms                   BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  PRIMARY KEY(scope_kind, scope_id, scope_sha256),
  FOREIGN KEY(
    current_command_sha256, scope_kind, scope_id, scope_sha256,
    current_command_sequence
  ) REFERENCES jobs_ats_certification_quarantine_commands(
    command_sha256, scope_kind, scope_id, scope_sha256, command_sequence
  ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_circuit_events (
  event_id                        TEXT PRIMARY KEY CHECK(length(event_id) > 0),
  event_sha256                    TEXT NOT NULL UNIQUE
    CHECK(length(event_sha256) = 64 AND lower(event_sha256) = event_sha256),
  canonical_event_base64url       TEXT NOT NULL CHECK(length(canonical_event_base64url) > 0),
  scope_kind                      TEXT NOT NULL
    CHECK(scope_kind IN ('activation', 'adapter', 'provider', 'runtime', 'target')),
  subject_key                     TEXT NOT NULL CHECK(length(subject_key) > 0),
  transition                      TEXT NOT NULL CHECK(transition IN ('opened', 'held', 'closed')),
  trigger_kind                    TEXT NOT NULL CHECK(trigger_kind IN (
    'confirmation_ambiguity', 'error_threshold', 'evidence_failure', 'false_state_risk',
    'layout_drift', 'newer_activation', 'reviewed_close', 'side_effect_unknown'
  )),
  window_started_at_ms            BIGINT NOT NULL CHECK(window_started_at_ms >= 0),
  window_ended_at_ms              BIGINT NOT NULL CHECK(window_ended_at_ms >= window_started_at_ms),
  failure_count                   BIGINT NOT NULL CHECK(failure_count >= 0),
  sample_count                    BIGINT NOT NULL CHECK(sample_count >= failure_count),
  threshold_count                 BIGINT NOT NULL CHECK(threshold_count >= 0),
  authority_ref                   TEXT NOT NULL CHECK(length(authority_ref) > 0),
  event_at_ms                     BIGINT NOT NULL CHECK(event_at_ms >= window_ended_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= event_at_ms),
  UNIQUE(scope_kind, subject_key, event_at_ms, event_id)
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_circuit_heads (
  scope_kind                      TEXT NOT NULL,
  subject_key                     TEXT NOT NULL,
  head_revision                   BIGINT NOT NULL CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  current_event_id                TEXT NOT NULL UNIQUE,
  state                           TEXT NOT NULL CHECK(state IN ('closed', 'held', 'opened')),
  updated_at_ms                   BIGINT NOT NULL CHECK(updated_at_ms >= 0),
  PRIMARY KEY(scope_kind, subject_key),
  FOREIGN KEY(current_event_id)
    REFERENCES jobs_ats_certification_circuit_events(event_id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_runtime_layout_quarantine_evidence (
  evidence_sha256                 TEXT PRIMARY KEY
    CHECK(length(evidence_sha256) = 64 AND lower(evidence_sha256) = evidence_sha256),
  evidence_kind                   TEXT NOT NULL
    CHECK(evidence_kind IN ('layout_drift', 'layout_drift_overflow')),
  activation_sha256               TEXT NOT NULL
    CHECK(length(activation_sha256) = 64 AND lower(activation_sha256) = activation_sha256),
  manifest_sha256                 TEXT NOT NULL
    CHECK(length(manifest_sha256) = 64 AND lower(manifest_sha256) = manifest_sha256),
  scope_sha256                    TEXT NOT NULL
    CHECK(length(scope_sha256) = 64 AND lower(scope_sha256) = scope_sha256),
  adapter_bundle_sha256           TEXT NOT NULL
    CHECK(length(adapter_bundle_sha256) = 64 AND lower(adapter_bundle_sha256) = adapter_bundle_sha256),
  runtime_sha256                  TEXT NOT NULL
    CHECK(length(runtime_sha256) = 64 AND lower(runtime_sha256) = runtime_sha256),
  layout_set_sha256               TEXT NOT NULL
    CHECK(length(layout_set_sha256) = 64 AND lower(layout_set_sha256) = layout_set_sha256),
  observed_variant_key            TEXT NOT NULL
    CHECK(length(observed_variant_key) BETWEEN 1 AND 120),
  observed_layout_contract_version BIGINT NOT NULL
    CHECK(observed_layout_contract_version BETWEEN 1 AND 9007199254740991),
  observed_surface_sha256         TEXT NOT NULL
    CHECK(length(observed_surface_sha256) = 64
      AND lower(observed_surface_sha256) = observed_surface_sha256),
  canonical_evidence_base64url    TEXT NOT NULL UNIQUE
    CHECK(length(canonical_evidence_base64url) > 0),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  BIGINT NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(
    activation_sha256, runtime_sha256, evidence_kind, observed_variant_key,
    observed_layout_contract_version, observed_surface_sha256
  ),
  FOREIGN KEY(activation_sha256)
    REFERENCES jobs_ats_certification_activations(activation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_runtime_layout_quarantine_scope
  ON jobs_ats_certification_runtime_layout_quarantine_evidence(
    activation_sha256, runtime_sha256, evidence_kind, recorded_at_ms DESC
  );

CREATE TABLE IF NOT EXISTS jobs_ats_certification_canary_allowlists (
  allowlist_sha256                TEXT PRIMARY KEY
    CHECK(length(allowlist_sha256) = 64 AND lower(allowlist_sha256) = allowlist_sha256),
  allowlist_id                    TEXT NOT NULL UNIQUE CHECK(length(allowlist_id) > 0),
  canonical_allowlist_base64url  TEXT NOT NULL UNIQUE CHECK(length(canonical_allowlist_base64url) > 0),
  member_count                   BIGINT NOT NULL CHECK(member_count BETWEEN 1 AND 10000),
  approval_ref                   TEXT NOT NULL CHECK(length(approval_ref) > 0),
  not_before_ms                  BIGINT NOT NULL CHECK(not_before_ms >= 0),
  expires_at_ms                  BIGINT NOT NULL CHECK(expires_at_ms > not_before_ms),
  approved_by                    TEXT NOT NULL CHECK(length(approved_by) > 0),
  approved_at_ms                 BIGINT NOT NULL CHECK(approved_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_canary_allowlist_members (
  allowlist_sha256                TEXT NOT NULL,
  ordinal                        BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 9999),
  account_id                     TEXT NOT NULL CHECK(length(account_id) > 0),
  PRIMARY KEY(allowlist_sha256, ordinal),
  UNIQUE(allowlist_sha256, account_id),
  FOREIGN KEY(allowlist_sha256)
    REFERENCES jobs_ats_certification_canary_allowlists(allowlist_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_canary_allowlist_members_account
  ON jobs_ats_certification_canary_allowlist_members(account_id, allowlist_sha256);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_canary_allowlist_revocations (
  allowlist_sha256                TEXT PRIMARY KEY,
  revocation_ref                 TEXT NOT NULL CHECK(length(revocation_ref) > 0),
  revoked_by                     TEXT NOT NULL CHECK(length(revoked_by) > 0),
  revoked_at_ms                  BIGINT NOT NULL CHECK(revoked_at_ms >= 0),
  FOREIGN KEY(allowlist_sha256)
    REFERENCES jobs_ats_certification_canary_allowlists(allowlist_sha256) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_application_ats_certification_bindings (
  binding_id                      TEXT PRIMARY KEY CHECK(length(binding_id) > 0),
  binding_sha256                  TEXT NOT NULL UNIQUE
    CHECK(length(binding_sha256) = 64 AND lower(binding_sha256) = binding_sha256),
  account_id                      TEXT NOT NULL CHECK(length(account_id) > 0),
  application_id                  TEXT NOT NULL CHECK(length(application_id) > 0),
  run_id                          TEXT NOT NULL CHECK(length(run_id) > 0),
  attempt_id                      TEXT NOT NULL CHECK(length(attempt_id) > 0),
  browser_session_id              TEXT NOT NULL CHECK(length(browser_session_id) > 0),
  browser_profile_id              TEXT NOT NULL CHECK(length(browser_profile_id) > 0),
  packet_checksum_sha256          TEXT NOT NULL CHECK(length(packet_checksum_sha256) = 64),
  auto_authorization_id           TEXT NOT NULL CHECK(length(auto_authorization_id) > 0),
  auto_authorization_revision     BIGINT NOT NULL CHECK(auto_authorization_revision > 0),
  auto_authorization_fingerprint_sha256 TEXT NOT NULL
    CHECK(length(auto_authorization_fingerprint_sha256) = 64),
  provider                        TEXT NOT NULL,
  target_key                      TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  activation_sha256               TEXT NOT NULL,
  layout_set_sha256               TEXT NOT NULL,
  adapter_bundle_sha256           TEXT NOT NULL,
  runner_target_sha256            TEXT NOT NULL,
  platform                        TEXT NOT NULL CHECK(platform IN ('linux', 'macos', 'windows')),
  architecture                    TEXT NOT NULL CHECK(architecture IN ('arm64', 'x86_64')),
  automation_bundle_sha256        TEXT NOT NULL CHECK(length(automation_bundle_sha256) = 64),
  browser_release_manifest_sha256 TEXT,
  browser_artifact_sha256         TEXT,
  browser_build_descriptor_sha256 TEXT,
  runner_build_id                 TEXT,
  runner_image_sha256             TEXT,
  browser_runtime_sha256          TEXT NOT NULL,
  chromium_executable_sha256      TEXT NOT NULL CHECK(length(chromium_executable_sha256) = 64),
  nonce_sha256                    TEXT NOT NULL UNIQUE CHECK(length(nonce_sha256) = 64),
  frozen_certification_base64url  TEXT NOT NULL CHECK(length(frozen_certification_base64url) > 0),
  layout_observation_sha256       TEXT
    CHECK(layout_observation_sha256 IS NULL OR
      (length(layout_observation_sha256) = 64
        AND lower(layout_observation_sha256) = layout_observation_sha256)),
  observed_surface_sha256         TEXT
    CHECK(observed_surface_sha256 IS NULL OR
      (length(observed_surface_sha256) = 64
        AND lower(observed_surface_sha256) = observed_surface_sha256)),
  phase_b_request_id              TEXT,
  phase_b_request_sha256          TEXT
    CHECK(phase_b_request_sha256 IS NULL OR length(phase_b_request_sha256) = 64),
  canary_reservation_sha256       TEXT
    CHECK(canary_reservation_sha256 IS NULL OR length(canary_reservation_sha256) = 64),
  metering_reservation_sha256     TEXT
    CHECK(metering_reservation_sha256 IS NULL OR length(metering_reservation_sha256) = 64),
  expires_at_ms                   BIGINT NOT NULL CHECK(expires_at_ms > 0),
  phase                           TEXT NOT NULL
    CHECK(phase IN ('preflight', 'consumed', 'invalidated', 'side_effect_unknown')),
  fence                           BIGINT NOT NULL CHECK(fence BETWEEN 0 AND 9007199254740991),
  created_at_ms                   BIGINT NOT NULL CHECK(created_at_ms >= 0),
  consumed_at_ms                  BIGINT,
  invalidation_kind               TEXT,
  invalidated_at_ms               BIGINT,
  UNIQUE(account_id, application_id, attempt_id),
  UNIQUE(account_id, application_id, run_id),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256)
    REFERENCES jobs_ats_certification_activations(activation_sha256) ON DELETE RESTRICT,
  CHECK(
    (phase = 'preflight' AND fence = 0 AND consumed_at_ms IS NULL
      AND invalidation_kind IS NULL AND invalidated_at_ms IS NULL)
    OR (phase = 'invalidated' AND fence = 1 AND consumed_at_ms IS NULL
      AND invalidation_kind IN (
        'auto_authorization_changed', 'claim_lost', 'expired', 'packet_changed', 'run_cancelled'
      ) AND invalidated_at_ms IS NOT NULL AND invalidated_at_ms >= created_at_ms
      AND layout_observation_sha256 IS NULL AND observed_surface_sha256 IS NULL
      AND phase_b_request_id IS NULL AND phase_b_request_sha256 IS NULL
      AND canary_reservation_sha256 IS NULL AND metering_reservation_sha256 IS NULL)
    OR (phase IN ('consumed', 'side_effect_unknown') AND fence = 1
      AND consumed_at_ms IS NOT NULL AND consumed_at_ms >= created_at_ms
      AND invalidation_kind IS NULL AND invalidated_at_ms IS NULL
      AND layout_observation_sha256 IS NOT NULL AND phase_b_request_id IS NOT NULL
      AND observed_surface_sha256 IS NOT NULL
      AND phase_b_request_sha256 IS NOT NULL
      AND canary_reservation_sha256 IS NOT NULL
      AND metering_reservation_sha256 IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS jobs_ats_certification_canary_reservations (
  reservation_id                  TEXT PRIMARY KEY CHECK(length(reservation_id) > 0),
  reservation_sha256              TEXT NOT NULL UNIQUE
    CHECK(length(reservation_sha256) = 64 AND lower(reservation_sha256) = reservation_sha256),
  binding_id                      TEXT NOT NULL UNIQUE,
  activation_sha256               TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  target_key                      TEXT NOT NULL,
  runner_target_sha256            TEXT NOT NULL,
  account_id                      TEXT NOT NULL,
  application_id                  TEXT NOT NULL,
  run_id                          TEXT NOT NULL,
  attempt_id                      TEXT NOT NULL,
  period_key                      TEXT NOT NULL CHECK(length(period_key) > 0),
  status                          TEXT NOT NULL CHECK(status = 'reserved'),
  fence                           BIGINT NOT NULL CHECK(fence = 1),
  reserved_at_ms                  BIGINT NOT NULL CHECK(reserved_at_ms >= 0),
  consumed_at_ms                  BIGINT,
  released_at_ms                  BIGINT,
  metering_reservation_sha256     TEXT NOT NULL
    CHECK(length(metering_reservation_sha256) = 64
      AND lower(metering_reservation_sha256) = metering_reservation_sha256),
  UNIQUE(activation_sha256, account_id, period_key, application_id, attempt_id),
  FOREIGN KEY(binding_id)
    REFERENCES jobs_application_ats_certification_bindings(binding_id) ON DELETE RESTRICT,
  FOREIGN KEY(activation_sha256)
    REFERENCES jobs_ats_certification_activations(activation_sha256) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_ats_certification_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_ats_certification_canary_capacity
  ON jobs_ats_certification_canary_reservations(
    activation_sha256, period_key, status, reserved_at_ms
  );

-- Every imported authority and transition is immutable. Only explicit heads
-- may advance, exactly one revision and one sequence at a time.
CREATE OR REPLACE FUNCTION reject_jobs_ats_certification_immutable_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'ATS certification authority is immutable';
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_ats_certification_head_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.head_revision <> OLD.head_revision + 1
     OR NEW.current_channel_sequence <= OLD.current_channel_sequence
     OR NEW.current_transition_sha256 = OLD.current_transition_sha256 THEN
    RAISE EXCEPTION 'ATS certification head must advance monotonically';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_ats_certification_trust_head_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.head_revision <> OLD.head_revision + 1
     OR NEW.current_trust_generation <> OLD.current_trust_generation + 1
     OR NEW.current_policy_sha256 = OLD.current_policy_sha256
     OR NEW.root_trust_anchor_sha256 <> OLD.root_trust_anchor_sha256 THEN
    RAISE EXCEPTION 'ATS certification trust head must advance monotonically';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_ats_certification_revocation_chain()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (NEW.revocation_generation = 1 AND (
        NEW.predecessor_revocation_sha256 IS NOT NULL
        OR EXISTS (
          SELECT 1 FROM jobs_ats_certification_revocations existing
           WHERE existing.trust_policy_sha256 = NEW.trust_policy_sha256
        )
      ))
     OR (NEW.revocation_generation > 1 AND NOT EXISTS (
       SELECT 1 FROM jobs_ats_certification_revocations predecessor
        WHERE predecessor.revocation_sha256 = NEW.predecessor_revocation_sha256
          AND predecessor.trust_policy_sha256 = NEW.trust_policy_sha256
          AND predecessor.revocation_generation = NEW.revocation_generation - 1
     )) THEN
    RAISE EXCEPTION 'ATS certification revocation chain is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_ats_quarantine_head_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.head_revision <> OLD.head_revision + 1
     OR NEW.current_command_sequence <= OLD.current_command_sequence
     OR NEW.current_command_sha256 = OLD.current_command_sha256 THEN
    RAISE EXCEPTION 'ATS certification quarantine head must advance monotonically';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_ats_circuit_head_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.head_revision <> OLD.head_revision + 1
     OR NEW.current_event_id = OLD.current_event_id THEN
    RAISE EXCEPTION 'ATS certification circuit head must advance monotonically';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_application_ats_binding_transition()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.phase <> 'preflight'
     OR NEW.binding_id <> OLD.binding_id
     OR NEW.account_id <> OLD.account_id
     OR NEW.application_id <> OLD.application_id
     OR NEW.run_id <> OLD.run_id
     OR NEW.attempt_id <> OLD.attempt_id
     OR NEW.binding_sha256 <> OLD.binding_sha256
     OR NEW.browser_session_id <> OLD.browser_session_id
     OR NEW.browser_profile_id <> OLD.browser_profile_id
     OR NEW.packet_checksum_sha256 <> OLD.packet_checksum_sha256
     OR NEW.auto_authorization_id <> OLD.auto_authorization_id
     OR NEW.auto_authorization_revision <> OLD.auto_authorization_revision
     OR NEW.auto_authorization_fingerprint_sha256 <> OLD.auto_authorization_fingerprint_sha256
     OR NEW.provider <> OLD.provider
     OR NEW.target_key <> OLD.target_key
     OR NEW.manifest_sha256 <> OLD.manifest_sha256
     OR NEW.activation_sha256 <> OLD.activation_sha256
     OR NEW.layout_set_sha256 <> OLD.layout_set_sha256
     OR NEW.adapter_bundle_sha256 <> OLD.adapter_bundle_sha256
     OR NEW.runner_target_sha256 <> OLD.runner_target_sha256
     OR NEW.platform <> OLD.platform
     OR NEW.architecture <> OLD.architecture
     OR NEW.automation_bundle_sha256 <> OLD.automation_bundle_sha256
     OR NEW.browser_release_manifest_sha256 IS DISTINCT FROM OLD.browser_release_manifest_sha256
     OR NEW.browser_artifact_sha256 IS DISTINCT FROM OLD.browser_artifact_sha256
     OR NEW.browser_build_descriptor_sha256 IS DISTINCT FROM OLD.browser_build_descriptor_sha256
     OR NEW.runner_build_id IS DISTINCT FROM OLD.runner_build_id
     OR NEW.runner_image_sha256 IS DISTINCT FROM OLD.runner_image_sha256
     OR NEW.browser_runtime_sha256 <> OLD.browser_runtime_sha256
     OR NEW.chromium_executable_sha256 <> OLD.chromium_executable_sha256
     OR NEW.nonce_sha256 <> OLD.nonce_sha256
     OR NEW.frozen_certification_base64url <> OLD.frozen_certification_base64url
     OR NEW.expires_at_ms <> OLD.expires_at_ms
     OR NEW.created_at_ms <> OLD.created_at_ms
     OR NEW.phase NOT IN ('consumed', 'invalidated', 'side_effect_unknown')
     OR NEW.fence <> 1 THEN
    RAISE EXCEPTION 'ATS application certification binding transition is invalid';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION reject_jobs_ats_certification_head_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'ATS certification head cannot be deleted';
END;
$$;

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_trust_policies_no_update
  ON jobs_ats_certification_trust_policies;
CREATE TRIGGER trg_jobs_ats_certification_trust_policies_no_update
BEFORE UPDATE ON jobs_ats_certification_trust_policies
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_trust_policies_no_delete
  ON jobs_ats_certification_trust_policies;
CREATE TRIGGER trg_jobs_ats_certification_trust_policies_no_delete
BEFORE DELETE ON jobs_ats_certification_trust_policies
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_trust_keys_no_update
  ON jobs_ats_certification_trust_keys;
CREATE TRIGGER trg_jobs_ats_certification_trust_keys_no_update
BEFORE UPDATE ON jobs_ats_certification_trust_keys
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_trust_keys_no_delete
  ON jobs_ats_certification_trust_keys;
CREATE TRIGGER trg_jobs_ats_certification_trust_keys_no_delete
BEFORE DELETE ON jobs_ats_certification_trust_keys
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_trust_head_monotonic
  ON jobs_ats_certification_trust_head;
CREATE TRIGGER trg_jobs_ats_certification_trust_head_monotonic
BEFORE UPDATE ON jobs_ats_certification_trust_head
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_certification_trust_head_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_trust_head_no_delete
  ON jobs_ats_certification_trust_head;
CREATE TRIGGER trg_jobs_ats_certification_trust_head_no_delete
BEFORE DELETE ON jobs_ats_certification_trust_head
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_head_delete();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_evidence_no_update
  ON jobs_ats_certification_evidence;
CREATE TRIGGER trg_jobs_ats_certification_evidence_no_update
BEFORE UPDATE ON jobs_ats_certification_evidence
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_evidence_no_delete
  ON jobs_ats_certification_evidence;
CREATE TRIGGER trg_jobs_ats_certification_evidence_no_delete
BEFORE DELETE ON jobs_ats_certification_evidence
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_layout_observations_no_update
  ON jobs_ats_certification_layout_observations;
CREATE TRIGGER trg_jobs_ats_certification_layout_observations_no_update
BEFORE UPDATE ON jobs_ats_certification_layout_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_layout_observations_no_delete
  ON jobs_ats_certification_layout_observations;
CREATE TRIGGER trg_jobs_ats_certification_layout_observations_no_delete
BEFORE DELETE ON jobs_ats_certification_layout_observations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifests_no_update
  ON jobs_ats_certification_manifests;
CREATE TRIGGER trg_jobs_ats_certification_manifests_no_update
BEFORE UPDATE ON jobs_ats_certification_manifests
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifests_no_delete
  ON jobs_ats_certification_manifests;
CREATE TRIGGER trg_jobs_ats_certification_manifests_no_delete
BEFORE DELETE ON jobs_ats_certification_manifests
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifest_evidence_no_update
  ON jobs_ats_certification_manifest_evidence;
CREATE TRIGGER trg_jobs_ats_certification_manifest_evidence_no_update
BEFORE UPDATE ON jobs_ats_certification_manifest_evidence
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifest_evidence_no_delete
  ON jobs_ats_certification_manifest_evidence;
CREATE TRIGGER trg_jobs_ats_certification_manifest_evidence_no_delete
BEFORE DELETE ON jobs_ats_certification_manifest_evidence
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifest_layouts_no_update
  ON jobs_ats_certification_manifest_layouts;
CREATE TRIGGER trg_jobs_ats_certification_manifest_layouts_no_update
BEFORE UPDATE ON jobs_ats_certification_manifest_layouts
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifest_layouts_no_delete
  ON jobs_ats_certification_manifest_layouts;
CREATE TRIGGER trg_jobs_ats_certification_manifest_layouts_no_delete
BEFORE DELETE ON jobs_ats_certification_manifest_layouts
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifest_check_results_no_update
  ON jobs_ats_certification_manifest_check_results;
CREATE TRIGGER trg_jobs_ats_certification_manifest_check_results_no_update
BEFORE UPDATE ON jobs_ats_certification_manifest_check_results
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_manifest_check_results_no_delete
  ON jobs_ats_certification_manifest_check_results;
CREATE TRIGGER trg_jobs_ats_certification_manifest_check_results_no_delete
BEFORE DELETE ON jobs_ats_certification_manifest_check_results
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_runtime_targets_no_update
  ON jobs_ats_certification_runtime_targets;
CREATE TRIGGER trg_jobs_ats_certification_runtime_targets_no_update
BEFORE UPDATE ON jobs_ats_certification_runtime_targets
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_runtime_targets_no_delete
  ON jobs_ats_certification_runtime_targets;
CREATE TRIGGER trg_jobs_ats_certification_runtime_targets_no_delete
BEFORE DELETE ON jobs_ats_certification_runtime_targets
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_activations_no_update
  ON jobs_ats_certification_activations;
CREATE TRIGGER trg_jobs_ats_certification_activations_no_update
BEFORE UPDATE ON jobs_ats_certification_activations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_activations_no_delete
  ON jobs_ats_certification_activations;
CREATE TRIGGER trg_jobs_ats_certification_activations_no_delete
BEFORE DELETE ON jobs_ats_certification_activations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_revocations_no_update
  ON jobs_ats_certification_revocations;
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_revocations_chain
  ON jobs_ats_certification_revocations;
CREATE TRIGGER trg_jobs_ats_certification_revocations_chain
BEFORE INSERT ON jobs_ats_certification_revocations
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_certification_revocation_chain();
CREATE TRIGGER trg_jobs_ats_certification_revocations_no_update
BEFORE UPDATE ON jobs_ats_certification_revocations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_revocations_no_delete
  ON jobs_ats_certification_revocations;
CREATE TRIGGER trg_jobs_ats_certification_revocations_no_delete
BEFORE DELETE ON jobs_ats_certification_revocations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_head_transitions_no_update
  ON jobs_ats_certification_head_transitions;
CREATE TRIGGER trg_jobs_ats_certification_head_transitions_no_update
BEFORE UPDATE ON jobs_ats_certification_head_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_head_transitions_no_delete
  ON jobs_ats_certification_head_transitions;
CREATE TRIGGER trg_jobs_ats_certification_head_transitions_no_delete
BEFORE DELETE ON jobs_ats_certification_head_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_quarantine_commands_no_update
  ON jobs_ats_certification_quarantine_commands;
CREATE TRIGGER trg_jobs_ats_certification_quarantine_commands_no_update
BEFORE UPDATE ON jobs_ats_certification_quarantine_commands
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_quarantine_commands_no_delete
  ON jobs_ats_certification_quarantine_commands;
CREATE TRIGGER trg_jobs_ats_certification_quarantine_commands_no_delete
BEFORE DELETE ON jobs_ats_certification_quarantine_commands
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_heads_monotonic
  ON jobs_ats_certification_heads;
CREATE TRIGGER trg_jobs_ats_certification_heads_monotonic
BEFORE UPDATE ON jobs_ats_certification_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_certification_head_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_heads_no_delete
  ON jobs_ats_certification_heads;
CREATE TRIGGER trg_jobs_ats_certification_heads_no_delete
BEFORE DELETE ON jobs_ats_certification_heads
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_head_delete();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_quarantine_heads_monotonic
  ON jobs_ats_certification_quarantine_heads;
CREATE TRIGGER trg_jobs_ats_certification_quarantine_heads_monotonic
BEFORE UPDATE ON jobs_ats_certification_quarantine_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_quarantine_head_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_quarantine_heads_no_delete
  ON jobs_ats_certification_quarantine_heads;
CREATE TRIGGER trg_jobs_ats_certification_quarantine_heads_no_delete
BEFORE DELETE ON jobs_ats_certification_quarantine_heads
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_head_delete();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_circuit_events_no_update
  ON jobs_ats_certification_circuit_events;
CREATE TRIGGER trg_jobs_ats_certification_circuit_events_no_update
BEFORE UPDATE ON jobs_ats_certification_circuit_events
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_circuit_events_no_delete
  ON jobs_ats_certification_circuit_events;
CREATE TRIGGER trg_jobs_ats_certification_circuit_events_no_delete
BEFORE DELETE ON jobs_ats_certification_circuit_events
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_runtime_layout_quarantine_no_update
  ON jobs_ats_certification_runtime_layout_quarantine_evidence;
CREATE TRIGGER trg_jobs_ats_runtime_layout_quarantine_no_update
BEFORE UPDATE ON jobs_ats_certification_runtime_layout_quarantine_evidence
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_runtime_layout_quarantine_no_delete
  ON jobs_ats_certification_runtime_layout_quarantine_evidence;
CREATE TRIGGER trg_jobs_ats_runtime_layout_quarantine_no_delete
BEFORE DELETE ON jobs_ats_certification_runtime_layout_quarantine_evidence
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_circuit_heads_monotonic
  ON jobs_ats_certification_circuit_heads;
CREATE TRIGGER trg_jobs_ats_certification_circuit_heads_monotonic
BEFORE UPDATE ON jobs_ats_certification_circuit_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_circuit_head_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_circuit_heads_no_delete
  ON jobs_ats_certification_circuit_heads;
CREATE TRIGGER trg_jobs_ats_certification_circuit_heads_no_delete
BEFORE DELETE ON jobs_ats_certification_circuit_heads
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_head_delete();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_allowlists_no_update
  ON jobs_ats_certification_canary_allowlists;
CREATE TRIGGER trg_jobs_ats_certification_canary_allowlists_no_update
BEFORE UPDATE ON jobs_ats_certification_canary_allowlists
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_allowlists_no_delete
  ON jobs_ats_certification_canary_allowlists;
CREATE TRIGGER trg_jobs_ats_certification_canary_allowlists_no_delete
BEFORE DELETE ON jobs_ats_certification_canary_allowlists
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_allowlist_members_no_update
  ON jobs_ats_certification_canary_allowlist_members;
CREATE TRIGGER trg_jobs_ats_certification_canary_allowlist_members_no_update
BEFORE UPDATE ON jobs_ats_certification_canary_allowlist_members
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_allowlist_members_no_delete
  ON jobs_ats_certification_canary_allowlist_members;
CREATE TRIGGER trg_jobs_ats_certification_canary_allowlist_members_no_delete
BEFORE DELETE ON jobs_ats_certification_canary_allowlist_members
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_allowlist_revocations_no_update
  ON jobs_ats_certification_canary_allowlist_revocations;
CREATE TRIGGER trg_jobs_ats_certification_canary_allowlist_revocations_no_update
BEFORE UPDATE ON jobs_ats_certification_canary_allowlist_revocations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_allowlist_revocations_no_delete
  ON jobs_ats_certification_canary_allowlist_revocations;
CREATE TRIGGER trg_jobs_ats_certification_canary_allowlist_revocations_no_delete
BEFORE DELETE ON jobs_ats_certification_canary_allowlist_revocations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_application_ats_certification_bindings_transition
  ON jobs_application_ats_certification_bindings;
CREATE TRIGGER trg_jobs_application_ats_certification_bindings_transition
BEFORE UPDATE ON jobs_application_ats_certification_bindings
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_application_ats_binding_transition();
DROP TRIGGER IF EXISTS trg_jobs_application_ats_certification_bindings_no_delete
  ON jobs_application_ats_certification_bindings;
CREATE TRIGGER trg_jobs_application_ats_certification_bindings_no_delete
BEFORE DELETE ON jobs_application_ats_certification_bindings
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_head_delete();

DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_reservations_no_update
  ON jobs_ats_certification_canary_reservations;
CREATE TRIGGER trg_jobs_ats_certification_canary_reservations_no_update
BEFORE UPDATE ON jobs_ats_certification_canary_reservations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_ats_certification_canary_reservations_no_delete
  ON jobs_ats_certification_canary_reservations;
CREATE TRIGGER trg_jobs_ats_certification_canary_reservations_no_delete
BEFORE DELETE ON jobs_ats_certification_canary_reservations
FOR EACH ROW EXECUTE FUNCTION reject_jobs_ats_certification_immutable_mutation();

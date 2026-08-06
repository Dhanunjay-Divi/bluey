-- Target: SQLite
-- Threshold-authorized Bluey Browser releases, explicit channel-head CAS,
-- immutable per-run bindings, and byte-exact local-claim replay.

-- Signature sets are imported before their targets so trust-policy bootstrap and
-- rotation never create an insertion cycle. The application verifies the exact
-- target audience, digest, role, threshold, key history, and canonical bytes.
CREATE TABLE IF NOT EXISTS jobs_browser_release_signature_sets (
  signature_set_sha256          TEXT PRIMARY KEY
    CHECK(length(signature_set_sha256) = 64
      AND lower(signature_set_sha256) = signature_set_sha256),
  signature_set_id              TEXT NOT NULL UNIQUE CHECK(length(signature_set_id) > 0),
  trust_generation              INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  role                          TEXT NOT NULL
    CHECK(role IN ('incident', 'promotion', 'release', 'root')),
  target_audience               TEXT NOT NULL CHECK(target_audience IN (
    'bluey-jobs-browser-release-manifest-v1',
    'bluey-jobs-browser-release-activation-v1',
    'bluey-jobs-browser-release-rollback-v1',
    'bluey-jobs-browser-release-revocation-v1',
    'bluey-jobs-browser-release-trust-policy-v1'
  )),
  target_sha256                 TEXT NOT NULL
    CHECK(length(target_sha256) = 64 AND lower(target_sha256) = target_sha256),
  signed_at_ms                  INTEGER NOT NULL CHECK(signed_at_ms >= 0),
  signature_count               INTEGER NOT NULL CHECK(signature_count BETWEEN 1 AND 32),
  canonical_signature_set_base64url TEXT NOT NULL
    CHECK(length(canonical_signature_set_base64url) > 0),
  recorded_by                   TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                INTEGER NOT NULL CHECK(recorded_at_ms >= signed_at_ms),
  UNIQUE(
    signature_set_sha256, trust_generation, role, target_audience, target_sha256
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_signature_sets_target
  ON jobs_browser_release_signature_sets(
    target_audience, target_sha256, trust_generation, role
  );

CREATE TABLE IF NOT EXISTS jobs_browser_release_signatures (
  signature_set_sha256          TEXT NOT NULL,
  key_id                        TEXT NOT NULL CHECK(length(key_id) > 0),
  signature_base64url           TEXT NOT NULL CHECK(length(signature_base64url) = 86),
  PRIMARY KEY(signature_set_sha256, key_id),
  FOREIGN KEY(signature_set_sha256)
    REFERENCES jobs_browser_release_signature_sets(signature_set_sha256)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_signatures_key
  ON jobs_browser_release_signatures(key_id, signature_set_sha256);

CREATE TABLE IF NOT EXISTS jobs_browser_release_trust_policies (
  policy_sha256                 TEXT PRIMARY KEY
    CHECK(length(policy_sha256) = 64 AND lower(policy_sha256) = policy_sha256),
  policy_id                     TEXT NOT NULL UNIQUE CHECK(length(policy_id) > 0),
  trust_generation              INTEGER NOT NULL UNIQUE
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256     TEXT,
  predecessor_trust_generation  INTEGER NOT NULL CHECK(predecessor_trust_generation >= 0),
  root_threshold                INTEGER NOT NULL CHECK(root_threshold BETWEEN 1 AND 32),
  release_threshold             INTEGER NOT NULL CHECK(release_threshold BETWEEN 1 AND 32),
  promotion_threshold           INTEGER NOT NULL CHECK(promotion_threshold BETWEEN 1 AND 32),
  incident_threshold            INTEGER NOT NULL CHECK(incident_threshold BETWEEN 1 AND 32),
  key_count                     INTEGER NOT NULL CHECK(key_count BETWEEN 4 AND 64),
  canonical_policy_base64url    TEXT NOT NULL CHECK(length(canonical_policy_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                  INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  valid_from_ms                 INTEGER NOT NULL CHECK(valid_from_ms >= 0),
  expires_at_ms                 INTEGER NOT NULL CHECK(expires_at_ms > issued_at_ms),
  recorded_by                   TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(policy_sha256, trust_generation),
  CHECK(valid_from_ms <= issued_at_ms),
  CHECK(
    (trust_generation = 1 AND predecessor_trust_generation = 0
      AND predecessor_policy_sha256 IS NULL)
    OR
    (trust_generation > 1
      AND predecessor_trust_generation = trust_generation - 1
      AND predecessor_policy_sha256 IS NOT NULL
      AND length(predecessor_policy_sha256) = 64)
  ),
  FOREIGN KEY(predecessor_policy_sha256, predecessor_trust_generation)
    REFERENCES jobs_browser_release_trust_policies(policy_sha256, trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_browser_release_signature_sets(signature_set_sha256)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_trust_policies_generation
  ON jobs_browser_release_trust_policies(trust_generation DESC, expires_at_ms);

CREATE TABLE IF NOT EXISTS jobs_browser_release_trust_keys (
  policy_sha256                 TEXT NOT NULL,
  trust_generation              INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  key_id                        TEXT NOT NULL CHECK(length(key_id) > 0),
  role                          TEXT NOT NULL
    CHECK(role IN ('incident', 'promotion', 'release', 'root')),
  public_key_base64url          TEXT NOT NULL CHECK(length(public_key_base64url) = 43),
  state                         TEXT NOT NULL CHECK(state IN ('active', 'retired', 'revoked')),
  valid_from_ms                 INTEGER NOT NULL CHECK(valid_from_ms >= 0),
  valid_until_ms                INTEGER NOT NULL CHECK(valid_until_ms >= valid_from_ms),
  minimum_trust_generation      INTEGER NOT NULL
    CHECK(minimum_trust_generation BETWEEN 1 AND 9007199254740991),
  maximum_trust_generation      INTEGER NOT NULL
    CHECK(maximum_trust_generation BETWEEN 1 AND 9007199254740991),
  PRIMARY KEY(policy_sha256, key_id),
  CHECK(maximum_trust_generation >= minimum_trust_generation),
  FOREIGN KEY(policy_sha256, trust_generation)
    REFERENCES jobs_browser_release_trust_policies(policy_sha256, trust_generation)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_trust_keys_history
  ON jobs_browser_release_trust_keys(key_id, trust_generation DESC, state);

CREATE TABLE IF NOT EXISTS jobs_browser_release_manifests (
  manifest_sha256              TEXT PRIMARY KEY
    CHECK(length(manifest_sha256) = 64 AND lower(manifest_sha256) = manifest_sha256),
  manifest_id                  TEXT NOT NULL UNIQUE CHECK(length(manifest_id) > 0),
  manifest_generation          INTEGER NOT NULL UNIQUE
    CHECK(manifest_generation BETWEEN 1 AND 9007199254740991),
  release_id                   TEXT NOT NULL UNIQUE CHECK(length(release_id) > 0),
  release_sequence             INTEGER NOT NULL UNIQUE
    CHECK(release_sequence BETWEEN 1 AND 9007199254740991),
  build_id                     TEXT NOT NULL UNIQUE CHECK(length(build_id) > 0),
  app_version                  TEXT NOT NULL CHECK(length(app_version) > 0),
  protocol_version             INTEGER NOT NULL CHECK(protocol_version >= 1),
  source_commit                TEXT NOT NULL
    CHECK(length(source_commit) = 40 AND lower(source_commit) = source_commit),
  electron_version             TEXT NOT NULL CHECK(length(electron_version) > 0),
  playwright_version           TEXT NOT NULL CHECK(length(playwright_version) > 0),
  chromium_revision            TEXT NOT NULL CHECK(length(chromium_revision) > 0),
  release_notes_url            TEXT NOT NULL CHECK(substr(release_notes_url, 1, 8) = 'https://'),
  artifact_count               INTEGER NOT NULL CHECK(artifact_count = 5),
  canonical_manifest_base64url TEXT NOT NULL CHECK(length(canonical_manifest_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  published_at_ms              INTEGER NOT NULL CHECK(published_at_ms >= 0),
  recorded_by                  TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms               INTEGER NOT NULL CHECK(recorded_at_ms >= published_at_ms),
  UNIQUE(manifest_sha256, authorization_signature_set_sha256),
  UNIQUE(manifest_sha256, release_id, build_id, app_version, protocol_version),
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_browser_release_signature_sets(signature_set_sha256)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_manifests_release
  ON jobs_browser_release_manifests(release_sequence DESC, recorded_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_browser_release_artifacts (
  artifact_id                     TEXT PRIMARY KEY CHECK(length(artifact_id) > 0),
  manifest_sha256                 TEXT NOT NULL,
  platform                        TEXT NOT NULL CHECK(platform IN ('darwin', 'windows')),
  architecture                    TEXT NOT NULL CHECK(architecture IN ('arm64', 'x64')),
  package_kind                    TEXT NOT NULL
    CHECK(package_kind IN ('darwin-dmg', 'darwin-zip', 'windows-nsis')),
  build_descriptor_sha256         TEXT NOT NULL
    CHECK(length(build_descriptor_sha256) = 64
      AND lower(build_descriptor_sha256) = build_descriptor_sha256),
  build_descriptor_base64url      TEXT NOT NULL CHECK(length(build_descriptor_base64url) > 0),
  build_descriptor_signature_base64url TEXT NOT NULL
    CHECK(length(build_descriptor_signature_base64url) = 86),
  build_descriptor_signing_key_id TEXT NOT NULL CHECK(length(build_descriptor_signing_key_id) > 0),
  artifact_url                    TEXT NOT NULL CHECK(substr(artifact_url, 1, 8) = 'https://'),
  artifact_filename               TEXT NOT NULL CHECK(length(artifact_filename) > 0),
  artifact_size_bytes             INTEGER NOT NULL CHECK(artifact_size_bytes > 0),
  artifact_sha256                 TEXT NOT NULL
    CHECK(length(artifact_sha256) = 64 AND lower(artifact_sha256) = artifact_sha256),
  app_content_sha256              TEXT NOT NULL
    CHECK(length(app_content_sha256) = 64 AND lower(app_content_sha256) = app_content_sha256),
  verification_evidence_sha256    TEXT NOT NULL
    CHECK(length(verification_evidence_sha256) = 64
      AND lower(verification_evidence_sha256) = verification_evidence_sha256),
  native_signature_kind           TEXT NOT NULL
    CHECK(native_signature_kind IN ('apple-developer-id', 'microsoft-authenticode')),
  native_signer_identity          TEXT NOT NULL CHECK(length(native_signer_identity) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(manifest_sha256, platform, architecture, package_kind),
  UNIQUE(manifest_sha256, artifact_sha256),
  UNIQUE(
    manifest_sha256, artifact_id, build_descriptor_sha256, artifact_sha256,
    platform, architecture, package_kind
  ),
  FOREIGN KEY(manifest_sha256)
    REFERENCES jobs_browser_release_manifests(manifest_sha256) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_artifacts_descriptor_target
  ON jobs_browser_release_artifacts(
    build_descriptor_sha256, platform, architecture, package_kind
  );

CREATE TABLE IF NOT EXISTS jobs_browser_release_activations (
  activation_sha256               TEXT PRIMARY KEY
    CHECK(length(activation_sha256) = 64 AND lower(activation_sha256) = activation_sha256),
  activation_id                   TEXT NOT NULL UNIQUE CHECK(length(activation_id) > 0),
  activation_generation           INTEGER NOT NULL UNIQUE
    CHECK(activation_generation BETWEEN 1 AND 9007199254740991),
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  channel                         TEXT NOT NULL CHECK(channel IN ('internal', 'beta', 'stable')),
  channel_sequence                INTEGER NOT NULL
    CHECK(channel_sequence BETWEEN 1 AND 9007199254740991),
  manifest_sha256                 TEXT NOT NULL,
  manifest_signature_set_sha256   TEXT NOT NULL,
  authorization_signature_set_sha256 TEXT NOT NULL,
  accepted_server_release_ids_json TEXT NOT NULL
    CHECK(length(accepted_server_release_ids_json) > 0),
  canary_evidence_sha256          TEXT NOT NULL
    CHECK(length(canary_evidence_sha256) = 64
      AND lower(canary_evidence_sha256) = canary_evidence_sha256),
  canonical_activation_base64url  TEXT NOT NULL CHECK(length(canonical_activation_base64url) > 0),
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  expires_at_ms                   INTEGER NOT NULL CHECK(expires_at_ms > issued_at_ms),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(channel, trust_generation, channel_sequence),
  UNIQUE(activation_sha256, manifest_sha256, channel),
  UNIQUE(
    activation_sha256, manifest_sha256, channel, activation_generation,
    trust_generation, channel_sequence, manifest_signature_set_sha256,
    authorization_signature_set_sha256
  ),
  UNIQUE(
    activation_sha256, manifest_sha256, channel, trust_generation, channel_sequence
  ),
  FOREIGN KEY(manifest_sha256, manifest_signature_set_sha256)
    REFERENCES jobs_browser_release_manifests(
      manifest_sha256, authorization_signature_set_sha256
    ) ON DELETE RESTRICT,
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_browser_release_trust_policies(trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_browser_release_signature_sets(signature_set_sha256)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_activations_channel_history
  ON jobs_browser_release_activations(
    channel, trust_generation DESC, channel_sequence DESC, expires_at_ms
  );

CREATE TABLE IF NOT EXISTS jobs_browser_release_rollbacks (
  rollback_sha256                 TEXT PRIMARY KEY
    CHECK(length(rollback_sha256) = 64 AND lower(rollback_sha256) = rollback_sha256),
  rollback_id                     TEXT NOT NULL UNIQUE CHECK(length(rollback_id) > 0),
  rollback_generation             INTEGER NOT NULL
    CHECK(rollback_generation BETWEEN 1 AND 9007199254740991),
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  channel                         TEXT NOT NULL CHECK(channel IN ('internal', 'beta', 'stable')),
  from_activation_sha256          TEXT NOT NULL,
  from_manifest_sha256            TEXT NOT NULL,
  to_activation_sha256            TEXT NOT NULL,
  to_manifest_sha256              TEXT NOT NULL,
  canary_evidence_sha256          TEXT NOT NULL
    CHECK(length(canary_evidence_sha256) = 64
      AND lower(canary_evidence_sha256) = canary_evidence_sha256),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  canonical_rollback_base64url    TEXT NOT NULL CHECK(length(canonical_rollback_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(trust_generation, rollback_generation),
  UNIQUE(
    rollback_sha256, channel, from_activation_sha256, from_manifest_sha256,
    to_activation_sha256, to_manifest_sha256
  ),
  CHECK(from_activation_sha256 <> to_activation_sha256),
  CHECK(from_manifest_sha256 <> to_manifest_sha256),
  FOREIGN KEY(from_activation_sha256, from_manifest_sha256, channel)
    REFERENCES jobs_browser_release_activations(
      activation_sha256, manifest_sha256, channel
    ) ON DELETE RESTRICT,
  FOREIGN KEY(to_activation_sha256, to_manifest_sha256, channel)
    REFERENCES jobs_browser_release_activations(
      activation_sha256, manifest_sha256, channel
    ) ON DELETE RESTRICT,
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_browser_release_trust_policies(trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_browser_release_signature_sets(signature_set_sha256)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_rollbacks_channel
  ON jobs_browser_release_rollbacks(
    channel, trust_generation DESC, rollback_generation DESC, recorded_at_ms DESC
  );

CREATE TABLE IF NOT EXISTS jobs_browser_release_revocations (
  revocation_sha256               TEXT PRIMARY KEY
    CHECK(length(revocation_sha256) = 64 AND lower(revocation_sha256) = revocation_sha256),
  revocation_id                   TEXT NOT NULL UNIQUE CHECK(length(revocation_id) > 0),
  revocation_generation           INTEGER NOT NULL
    CHECK(revocation_generation BETWEEN 1 AND 9007199254740991),
  trust_generation                INTEGER NOT NULL
    CHECK(trust_generation BETWEEN 1 AND 9007199254740991),
  subject_kind                    TEXT NOT NULL CHECK(subject_kind IN (
    'signing-key', 'build-descriptor', 'manifest', 'release', 'artifact'
  )),
  subject_id                      TEXT NOT NULL CHECK(length(subject_id) > 0),
  subject_sha256                  TEXT NOT NULL
    CHECK(length(subject_sha256) = 64 AND lower(subject_sha256) = subject_sha256),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  canonical_revocation_base64url  TEXT NOT NULL CHECK(length(canonical_revocation_base64url) > 0),
  authorization_signature_set_sha256 TEXT NOT NULL,
  issued_at_ms                    INTEGER NOT NULL CHECK(issued_at_ms >= 0),
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= issued_at_ms),
  UNIQUE(trust_generation, revocation_generation),
  UNIQUE(subject_kind, subject_id, subject_sha256),
  FOREIGN KEY(trust_generation)
    REFERENCES jobs_browser_release_trust_policies(trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(authorization_signature_set_sha256)
    REFERENCES jobs_browser_release_signature_sets(signature_set_sha256)
    ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_revocations_subject
  ON jobs_browser_release_revocations(
    subject_kind, subject_id, subject_sha256,
    trust_generation DESC, revocation_generation DESC
  );

-- Imported activations and rollback targets remain inert until an explicit
-- compare-and-swap transition is appended and installed as the channel head.
CREATE TABLE IF NOT EXISTS jobs_browser_release_channel_transitions (
  transition_sha256               TEXT PRIMARY KEY
    CHECK(length(transition_sha256) = 64 AND lower(transition_sha256) = transition_sha256),
  channel                         TEXT NOT NULL CHECK(channel IN ('internal', 'beta', 'stable')),
  head_revision                   INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  previous_head_revision          INTEGER NOT NULL CHECK(previous_head_revision >= 0),
  previous_transition_sha256      TEXT,
  previous_activation_sha256      TEXT,
  previous_manifest_sha256        TEXT,
  previous_trust_generation       INTEGER,
  previous_channel_sequence       INTEGER,
  next_activation_sha256          TEXT NOT NULL,
  next_manifest_sha256            TEXT NOT NULL,
  next_trust_generation           INTEGER NOT NULL
    CHECK(next_trust_generation BETWEEN 1 AND 9007199254740991),
  next_channel_sequence           INTEGER NOT NULL
    CHECK(next_channel_sequence BETWEEN 1 AND 9007199254740991),
  transition_kind                 TEXT NOT NULL CHECK(transition_kind IN ('activation', 'rollback')),
  authority_sha256                TEXT NOT NULL
    CHECK(length(authority_sha256) = 64 AND lower(authority_sha256) = authority_sha256),
  rollback_authority_sha256       TEXT,
  recorded_by                     TEXT NOT NULL CHECK(length(recorded_by) > 0),
  recorded_at_ms                  INTEGER NOT NULL CHECK(recorded_at_ms >= 0),
  UNIQUE(channel, head_revision),
  UNIQUE(channel, previous_head_revision),
  UNIQUE(
    transition_sha256, channel, head_revision, next_activation_sha256,
    next_manifest_sha256, next_trust_generation, next_channel_sequence
  ),
  CHECK(head_revision = previous_head_revision + 1),
  CHECK(
    (head_revision = 1 AND previous_head_revision = 0
      AND previous_transition_sha256 IS NULL
      AND previous_activation_sha256 IS NULL
      AND previous_manifest_sha256 IS NULL
      AND previous_trust_generation IS NULL
      AND previous_channel_sequence IS NULL
      AND transition_kind = 'activation')
    OR
    (head_revision > 1 AND previous_head_revision >= 1
      AND previous_transition_sha256 IS NOT NULL
      AND previous_activation_sha256 IS NOT NULL
      AND previous_manifest_sha256 IS NOT NULL
      AND previous_trust_generation IS NOT NULL
      AND previous_channel_sequence IS NOT NULL)
  ),
  CHECK(
    (transition_kind = 'activation'
      AND authority_sha256 = next_activation_sha256
      AND rollback_authority_sha256 IS NULL)
    OR
    (transition_kind = 'rollback'
      AND rollback_authority_sha256 = authority_sha256)
  ),
  CHECK(
    transition_kind <> 'activation'
    OR head_revision = 1
    OR next_trust_generation > previous_trust_generation
    OR (next_trust_generation = previous_trust_generation
      AND next_channel_sequence > previous_channel_sequence)
  ),
  FOREIGN KEY(
    previous_transition_sha256, channel, previous_head_revision,
    previous_activation_sha256, previous_manifest_sha256,
    previous_trust_generation, previous_channel_sequence
  ) REFERENCES jobs_browser_release_channel_transitions(
    transition_sha256, channel, head_revision, next_activation_sha256,
    next_manifest_sha256, next_trust_generation, next_channel_sequence
  ) ON DELETE RESTRICT,
  FOREIGN KEY(
    next_activation_sha256, next_manifest_sha256, channel,
    next_trust_generation, next_channel_sequence
  ) REFERENCES jobs_browser_release_activations(
    activation_sha256, manifest_sha256, channel, trust_generation, channel_sequence
  ) ON DELETE RESTRICT,
  FOREIGN KEY(
    rollback_authority_sha256, channel, previous_activation_sha256,
    previous_manifest_sha256, next_activation_sha256, next_manifest_sha256
  ) REFERENCES jobs_browser_release_rollbacks(
    rollback_sha256, channel, from_activation_sha256, from_manifest_sha256,
    to_activation_sha256, to_manifest_sha256
  ) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_channel_transitions_history
  ON jobs_browser_release_channel_transitions(channel, head_revision DESC, recorded_at_ms DESC);

-- This is the only mutable global release-authority table. Writers append the
-- transition first, then CAS this row on the exact prior revision and digest.
CREATE TABLE IF NOT EXISTS jobs_browser_release_channel_heads (
  channel                         TEXT PRIMARY KEY
    CHECK(channel IN ('internal', 'beta', 'stable')),
  head_revision                   INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  current_transition_sha256       TEXT NOT NULL,
  current_activation_sha256       TEXT NOT NULL,
  current_manifest_sha256         TEXT NOT NULL,
  current_trust_generation        INTEGER NOT NULL
    CHECK(current_trust_generation BETWEEN 1 AND 9007199254740991),
  current_channel_sequence        INTEGER NOT NULL
    CHECK(current_channel_sequence BETWEEN 1 AND 9007199254740991),
  updated_at_ms                   INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  FOREIGN KEY(
    current_transition_sha256, channel, head_revision,
    current_activation_sha256, current_manifest_sha256,
    current_trust_generation, current_channel_sequence
  ) REFERENCES jobs_browser_release_channel_transitions(
    transition_sha256, channel, head_revision, next_activation_sha256,
    next_manifest_sha256, next_trust_generation, next_channel_sequence
  ) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS jobs_browser_account_channel_assignments (
  assignment_sha256               TEXT PRIMARY KEY
    CHECK(length(assignment_sha256) = 64 AND lower(assignment_sha256) = assignment_sha256),
  account_id                      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  assignment_generation           INTEGER NOT NULL
    CHECK(assignment_generation BETWEEN 1 AND 9007199254740991),
  predecessor_assignment_sha256   TEXT,
  predecessor_generation          INTEGER NOT NULL CHECK(predecessor_generation >= 0),
  channel                         TEXT NOT NULL CHECK(channel IN ('internal', 'beta', 'stable')),
  reason_ref                      TEXT NOT NULL CHECK(length(reason_ref) > 0),
  assigned_by                     TEXT NOT NULL CHECK(length(assigned_by) > 0),
  assigned_at_ms                  INTEGER NOT NULL CHECK(assigned_at_ms >= 0),
  UNIQUE(account_id, assignment_generation),
  UNIQUE(assignment_sha256, account_id),
  UNIQUE(assignment_sha256, account_id, assignment_generation),
  UNIQUE(assignment_sha256, account_id, channel, assignment_generation),
  CHECK(
    (assignment_generation = 1 AND predecessor_generation = 0
      AND predecessor_assignment_sha256 IS NULL)
    OR
    (assignment_generation > 1
      AND predecessor_generation = assignment_generation - 1
      AND predecessor_assignment_sha256 IS NOT NULL
      AND length(predecessor_assignment_sha256) = 64)
  ),
  FOREIGN KEY(predecessor_assignment_sha256, account_id, predecessor_generation)
    REFERENCES jobs_browser_account_channel_assignments(
      assignment_sha256, account_id, assignment_generation
    ) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_jobs_browser_account_channel_assignments_current
  ON jobs_browser_account_channel_assignments(account_id, assignment_generation DESC);

CREATE TABLE IF NOT EXISTS jobs_local_run_release_bindings (
  run_id                           TEXT PRIMARY KEY
    REFERENCES jobs_local_run_tickets(id) ON DELETE CASCADE,
  account_id                      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  application_id                  TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
  binding_sha256                  TEXT NOT NULL UNIQUE
    CHECK(length(binding_sha256) = 64 AND lower(binding_sha256) = binding_sha256),
  account_channel_assignment_sha256 TEXT NOT NULL,
  account_channel_assignment_generation INTEGER NOT NULL
    CHECK(account_channel_assignment_generation >= 1),
  channel                         TEXT NOT NULL CHECK(channel IN ('internal', 'beta', 'stable')),
  channel_head_revision           INTEGER NOT NULL CHECK(channel_head_revision >= 1),
  channel_transition_sha256       TEXT NOT NULL,
  activation_sha256               TEXT NOT NULL,
  activation_generation           INTEGER NOT NULL CHECK(activation_generation >= 1),
  trust_generation                INTEGER NOT NULL CHECK(trust_generation >= 1),
  trust_policy_sha256              TEXT NOT NULL,
  channel_sequence                INTEGER NOT NULL CHECK(channel_sequence >= 1),
  manifest_signature_set_sha256   TEXT NOT NULL,
  activation_authorization_signature_set_sha256 TEXT NOT NULL,
  manifest_sha256                 TEXT NOT NULL,
  artifact_id                     TEXT NOT NULL,
  release_id                      TEXT NOT NULL CHECK(length(release_id) > 0),
  build_id                        TEXT NOT NULL CHECK(length(build_id) > 0),
  app_version                     TEXT NOT NULL CHECK(length(app_version) > 0),
  protocol_version                INTEGER NOT NULL CHECK(protocol_version >= 1),
  platform                        TEXT NOT NULL CHECK(platform IN ('darwin', 'windows')),
  architecture                    TEXT NOT NULL CHECK(architecture IN ('arm64', 'x64')),
  package_kind                    TEXT NOT NULL
    CHECK(package_kind IN ('darwin-dmg', 'darwin-zip', 'windows-nsis')),
  build_descriptor_sha256         TEXT NOT NULL
    CHECK(length(build_descriptor_sha256) = 64
      AND lower(build_descriptor_sha256) = build_descriptor_sha256),
  artifact_sha256                 TEXT NOT NULL
    CHECK(length(artifact_sha256) = 64 AND lower(artifact_sha256) = artifact_sha256),
  bound_at_ms                     INTEGER NOT NULL CHECK(bound_at_ms >= 0),
  UNIQUE(run_id, account_id),
  FOREIGN KEY(
    account_channel_assignment_sha256, account_id, channel,
    account_channel_assignment_generation
  ) REFERENCES jobs_browser_account_channel_assignments(
    assignment_sha256, account_id, channel, assignment_generation
  ) ON DELETE CASCADE,
  FOREIGN KEY(
    channel_transition_sha256, channel, channel_head_revision,
    activation_sha256, manifest_sha256, trust_generation, channel_sequence
  ) REFERENCES jobs_browser_release_channel_transitions(
    transition_sha256, channel, head_revision, next_activation_sha256,
    next_manifest_sha256, next_trust_generation, next_channel_sequence
  ) ON DELETE RESTRICT,
  FOREIGN KEY(trust_policy_sha256, trust_generation)
    REFERENCES jobs_browser_release_trust_policies(policy_sha256, trust_generation)
    ON DELETE RESTRICT,
  FOREIGN KEY(
    activation_sha256, manifest_sha256, channel, activation_generation,
    trust_generation, channel_sequence, manifest_signature_set_sha256,
    activation_authorization_signature_set_sha256
  ) REFERENCES jobs_browser_release_activations(
    activation_sha256, manifest_sha256, channel, activation_generation,
    trust_generation, channel_sequence, manifest_signature_set_sha256,
    authorization_signature_set_sha256
  ) ON DELETE RESTRICT,
  FOREIGN KEY(manifest_sha256, release_id, build_id, app_version, protocol_version)
    REFERENCES jobs_browser_release_manifests(
      manifest_sha256, release_id, build_id, app_version, protocol_version
    ) ON DELETE RESTRICT,
  FOREIGN KEY(
    manifest_sha256, artifact_id, build_descriptor_sha256, artifact_sha256,
    platform, architecture, package_kind
  ) REFERENCES jobs_browser_release_artifacts(
    manifest_sha256, artifact_id, build_descriptor_sha256, artifact_sha256,
    platform, architecture, package_kind
  ) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_local_run_release_bindings_account
  ON jobs_local_run_release_bindings(account_id, application_id, bound_at_ms DESC);

CREATE TABLE IF NOT EXISTS jobs_local_run_claim_replays (
  run_id                           TEXT PRIMARY KEY,
  account_id                      TEXT NOT NULL,
  claim_nonce_sha256              TEXT NOT NULL UNIQUE
    CHECK(length(claim_nonce_sha256) = 64 AND lower(claim_nonce_sha256) = claim_nonce_sha256),
  claim_request_sha256            TEXT NOT NULL
    CHECK(length(claim_request_sha256) = 64 AND lower(claim_request_sha256) = claim_request_sha256),
  claim_response_sha256           TEXT NOT NULL
    CHECK(length(claim_response_sha256) = 64
      AND lower(claim_response_sha256) = claim_response_sha256),
  claim_response_secret           TEXT NOT NULL CHECK(length(claim_response_secret) > 0),
  created_at_ms                   INTEGER NOT NULL CHECK(created_at_ms >= 0),
  UNIQUE(run_id, claim_nonce_sha256, claim_request_sha256, claim_response_sha256),
  FOREIGN KEY(run_id, account_id)
    REFERENCES jobs_local_run_release_bindings(run_id, account_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_jobs_local_run_claim_replays_created
  ON jobs_local_run_claim_replays(account_id, created_at_ms DESC);

CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_signature_sets_no_update
BEFORE UPDATE ON jobs_browser_release_signature_sets
BEGIN
  SELECT RAISE(ABORT, 'Browser release signature set is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_signature_sets_no_delete
BEFORE DELETE ON jobs_browser_release_signature_sets
BEGIN
  SELECT RAISE(ABORT, 'Browser release signature set is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_signatures_no_update
BEFORE UPDATE ON jobs_browser_release_signatures
BEGIN
  SELECT RAISE(ABORT, 'Browser release detached signature is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_signatures_no_delete
BEFORE DELETE ON jobs_browser_release_signatures
BEGIN
  SELECT RAISE(ABORT, 'Browser release detached signature is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_trust_policies_no_update
BEFORE UPDATE ON jobs_browser_release_trust_policies
BEGIN
  SELECT RAISE(ABORT, 'Browser release trust policy is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_trust_policies_no_delete
BEFORE DELETE ON jobs_browser_release_trust_policies
BEGIN
  SELECT RAISE(ABORT, 'Browser release trust policy is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_trust_keys_no_update
BEFORE UPDATE ON jobs_browser_release_trust_keys
BEGIN
  SELECT RAISE(ABORT, 'Browser release trust key is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_trust_keys_no_delete
BEFORE DELETE ON jobs_browser_release_trust_keys
BEGIN
  SELECT RAISE(ABORT, 'Browser release trust key is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_manifests_no_update
BEFORE UPDATE ON jobs_browser_release_manifests
BEGIN
  SELECT RAISE(ABORT, 'Browser release manifest is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_manifests_no_delete
BEFORE DELETE ON jobs_browser_release_manifests
BEGIN
  SELECT RAISE(ABORT, 'Browser release manifest is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_artifacts_no_update
BEFORE UPDATE ON jobs_browser_release_artifacts
BEGIN
  SELECT RAISE(ABORT, 'Browser release artifact is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_artifacts_no_delete
BEFORE DELETE ON jobs_browser_release_artifacts
BEGIN
  SELECT RAISE(ABORT, 'Browser release artifact is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_activations_no_update
BEFORE UPDATE ON jobs_browser_release_activations
BEGIN
  SELECT RAISE(ABORT, 'Browser release activation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_activations_no_delete
BEFORE DELETE ON jobs_browser_release_activations
BEGIN
  SELECT RAISE(ABORT, 'Browser release activation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_rollbacks_no_update
BEFORE UPDATE ON jobs_browser_release_rollbacks
BEGIN
  SELECT RAISE(ABORT, 'Browser release rollback is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_rollbacks_no_delete
BEFORE DELETE ON jobs_browser_release_rollbacks
BEGIN
  SELECT RAISE(ABORT, 'Browser release rollback is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_revocations_no_update
BEFORE UPDATE ON jobs_browser_release_revocations
BEGIN
  SELECT RAISE(ABORT, 'Browser release revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_revocations_no_delete
BEFORE DELETE ON jobs_browser_release_revocations
BEGIN
  SELECT RAISE(ABORT, 'Browser release revocation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_channel_transitions_no_update
BEFORE UPDATE ON jobs_browser_release_channel_transitions
BEGIN
  SELECT RAISE(ABORT, 'Browser release channel transition is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_channel_transitions_no_delete
BEFORE DELETE ON jobs_browser_release_channel_transitions
BEGIN
  SELECT RAISE(ABORT, 'Browser release channel transition is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_release_channel_heads_no_delete
BEFORE DELETE ON jobs_browser_release_channel_heads
BEGIN
  SELECT RAISE(ABORT, 'Browser release channel head cannot be deleted');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_account_channel_assignments_no_update
BEFORE UPDATE ON jobs_browser_account_channel_assignments
BEGIN
  SELECT RAISE(ABORT, 'Browser account channel assignment is append-only');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_local_run_release_bindings_no_update
BEFORE UPDATE ON jobs_local_run_release_bindings
BEGIN
  SELECT RAISE(ABORT, 'local Browser release binding is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_local_run_claim_replays_no_update
BEFORE UPDATE ON jobs_local_run_claim_replays
BEGIN
  SELECT RAISE(ABORT, 'local Browser claim replay is immutable');
END;

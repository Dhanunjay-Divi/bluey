-- Target: PostgreSQL
-- Immutable activation, semantic-input, Career Track policy, review, and
-- compare-and-swap authority. Runtime startup activates the compiled taxonomy
-- and canonicalizer tuple; this migration seeds no review/execution authority.

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_tracks_account_id_unique
  ON jobs_tracks(account_id, id);

CREATE TABLE IF NOT EXISTS jobs_track_policy_taxonomy_activation_events (
  activation_epoch                       BIGINT PRIMARY KEY
    CHECK(activation_epoch BETWEEN 1 AND 9007199254740991),
  previous_activation_epoch              BIGINT NOT NULL
    CHECK(previous_activation_epoch BETWEEN 0 AND 9007199254740991),
  taxonomy_version                       TEXT NOT NULL
    CHECK(length(taxonomy_version) BETWEEN 1 AND 64),
  taxonomy_digest_sha256                 TEXT NOT NULL
    CHECK(taxonomy_digest_sha256 ~ '^[0-9a-f]{64}$'),
  canonicalizer_schema_version           BIGINT NOT NULL
    CHECK(canonicalizer_schema_version BETWEEN 1 AND 9007199254740991),
  canonicalizer_digest_sha256            TEXT NOT NULL
    CHECK(canonicalizer_digest_sha256 ~ '^[0-9a-f]{64}$'),
  activation_transition_sha256           TEXT NOT NULL UNIQUE
    CHECK(activation_transition_sha256 ~ '^[0-9a-f]{64}$'),
  predecessor_activation_transition_sha256 TEXT,
  activated_at_ms                        BIGINT NOT NULL
    CHECK(activated_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(activation_epoch, activation_transition_sha256),
  FOREIGN KEY(previous_activation_epoch,
              predecessor_activation_transition_sha256)
    REFERENCES jobs_track_policy_taxonomy_activation_events(
      activation_epoch, activation_transition_sha256),
  CHECK(
    (activation_epoch = 1 AND previous_activation_epoch = 0
      AND predecessor_activation_transition_sha256 IS NULL)
    OR (activation_epoch > 1
      AND previous_activation_epoch = activation_epoch - 1
      AND predecessor_activation_transition_sha256 IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_taxonomy_activation_head (
  singleton_id                           BIGINT PRIMARY KEY CHECK(singleton_id = 1),
  activation_epoch                       BIGINT NOT NULL,
  previous_activation_epoch              BIGINT NOT NULL,
  taxonomy_version                       TEXT NOT NULL,
  taxonomy_digest_sha256                 TEXT NOT NULL,
  canonicalizer_schema_version           BIGINT NOT NULL,
  canonicalizer_digest_sha256            TEXT NOT NULL,
  activation_transition_sha256           TEXT NOT NULL,
  predecessor_activation_transition_sha256 TEXT,
  activated_at_ms                        BIGINT NOT NULL,
  UNIQUE(activation_epoch, activation_transition_sha256),
  FOREIGN KEY(activation_epoch, activation_transition_sha256)
    REFERENCES jobs_track_policy_taxonomy_activation_events(
      activation_epoch, activation_transition_sha256)
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_account_input_transitions (
  input_transition_id                    TEXT PRIMARY KEY,
  account_id                             TEXT NOT NULL,
  input_generation                       BIGINT NOT NULL
    CHECK(input_generation BETWEEN 1 AND 9007199254740991),
  previous_input_generation              BIGINT NOT NULL
    CHECK(previous_input_generation BETWEEN 0 AND 9007199254740991),
  input_kind                             TEXT NOT NULL CHECK(input_kind IN (
    'baseline', 'profile', 'preferences', 'fact', 'identity', 'resume'
  )),
  input_subject_sha256                   TEXT NOT NULL
    CHECK(input_subject_sha256 ~ '^[0-9a-f]{64}$'),
  account_semantic_sha256                TEXT NOT NULL
    CHECK(account_semantic_sha256 ~ '^[0-9a-f]{64}$'),
  input_transition_sha256                TEXT NOT NULL UNIQUE
    CHECK(input_transition_sha256 ~ '^[0-9a-f]{64}$'),
  predecessor_input_transition_sha256    TEXT,
  changed_at_ms                          BIGINT NOT NULL
    CHECK(changed_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, input_generation),
  UNIQUE(account_id, input_generation, input_transition_sha256),
  UNIQUE(account_id, input_generation, input_transition_id,
         input_transition_sha256),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id, previous_input_generation,
              predecessor_input_transition_sha256)
    REFERENCES jobs_track_policy_account_input_transitions(
      account_id, input_generation, input_transition_sha256)
      ON DELETE CASCADE,
  CHECK(
    (input_generation = 1 AND previous_input_generation = 0
      AND predecessor_input_transition_sha256 IS NULL)
    OR (input_generation > 1
      AND previous_input_generation = input_generation - 1
      AND predecessor_input_transition_sha256 IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_account_input_heads (
  account_id                             TEXT PRIMARY KEY,
  input_generation                       BIGINT NOT NULL,
  previous_input_generation              BIGINT NOT NULL,
  input_transition_id                    TEXT NOT NULL,
  input_kind                             TEXT NOT NULL,
  input_subject_sha256                   TEXT NOT NULL,
  account_semantic_sha256                TEXT NOT NULL,
  input_transition_sha256                TEXT NOT NULL,
  predecessor_input_transition_sha256    TEXT,
  updated_at_ms                          BIGINT NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id, input_generation, input_transition_id,
              input_transition_sha256)
    REFERENCES jobs_track_policy_account_input_transitions(
      account_id, input_generation, input_transition_id,
      input_transition_sha256) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_track_input_transitions (
  input_transition_id                    TEXT PRIMARY KEY,
  account_id                             TEXT NOT NULL,
  career_track_id                        TEXT NOT NULL,
  input_generation                       BIGINT NOT NULL
    CHECK(input_generation BETWEEN 1 AND 9007199254740991),
  previous_input_generation              BIGINT NOT NULL
    CHECK(previous_input_generation BETWEEN 0 AND 9007199254740991),
  track_semantic_sha256                  TEXT NOT NULL
    CHECK(track_semantic_sha256 ~ '^[0-9a-f]{64}$'),
  input_transition_sha256                TEXT NOT NULL UNIQUE
    CHECK(input_transition_sha256 ~ '^[0-9a-f]{64}$'),
  predecessor_input_transition_sha256    TEXT,
  changed_at_ms                          BIGINT NOT NULL
    CHECK(changed_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, career_track_id, input_generation),
  UNIQUE(account_id, career_track_id, input_generation,
         input_transition_sha256),
  UNIQUE(account_id, career_track_id, input_generation, input_transition_id,
         input_transition_sha256),
  FOREIGN KEY(account_id, career_track_id)
    REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE,
  FOREIGN KEY(account_id, career_track_id, previous_input_generation,
              predecessor_input_transition_sha256)
    REFERENCES jobs_track_policy_track_input_transitions(
      account_id, career_track_id, input_generation,
      input_transition_sha256) ON DELETE CASCADE,
  CHECK(
    (input_generation = 1 AND previous_input_generation = 0
      AND predecessor_input_transition_sha256 IS NULL)
    OR (input_generation > 1
      AND previous_input_generation = input_generation - 1
      AND predecessor_input_transition_sha256 IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_track_input_heads (
  account_id                             TEXT NOT NULL,
  career_track_id                        TEXT NOT NULL,
  input_generation                       BIGINT NOT NULL,
  previous_input_generation              BIGINT NOT NULL,
  input_transition_id                    TEXT NOT NULL,
  track_semantic_sha256                  TEXT NOT NULL,
  input_transition_sha256                TEXT NOT NULL,
  predecessor_input_transition_sha256    TEXT,
  updated_at_ms                          BIGINT NOT NULL,
  PRIMARY KEY(account_id, career_track_id),
  FOREIGN KEY(account_id, career_track_id)
    REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE,
  FOREIGN KEY(account_id, career_track_id, input_generation,
              input_transition_id, input_transition_sha256)
    REFERENCES jobs_track_policy_track_input_transitions(
      account_id, career_track_id, input_generation, input_transition_id,
      input_transition_sha256) ON DELETE CASCADE
);

-- The composite Track parent key proves account ownership. Identity and resume
-- references are validated at insertion/head advance but are stored as exact
-- immutable values, so history never follows a mutable current-row replacement.

CREATE TABLE IF NOT EXISTS jobs_track_policy_revisions (
  revision_id                            TEXT PRIMARY KEY
    CHECK(length(revision_id) BETWEEN 20 AND 128
      AND revision_id ~ '^[A-Za-z0-9][A-Za-z0-9_-]{19,127}$'),
  account_id                             TEXT NOT NULL,
  career_track_id                        TEXT NOT NULL
    CHECK(length(career_track_id) BETWEEN 1 AND 128),
  revision_no                            BIGINT NOT NULL
    CHECK(revision_no BETWEEN 1 AND 9007199254740991),
  taxonomy_version                       TEXT NOT NULL
    CHECK(length(taxonomy_version) BETWEEN 1 AND 64
      AND taxonomy_version ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$'),
  taxonomy_digest_sha256                 TEXT NOT NULL
    CHECK(taxonomy_digest_sha256 ~ '^[0-9a-f]{64}$'),
  taxonomy_activation_epoch              BIGINT NOT NULL
    CHECK(taxonomy_activation_epoch BETWEEN 1 AND 9007199254740991),
  canonicalizer_schema_version           BIGINT NOT NULL
    CHECK(canonicalizer_schema_version BETWEEN 1 AND 9007199254740991),
  canonicalizer_digest_sha256            TEXT NOT NULL
    CHECK(canonicalizer_digest_sha256 ~ '^[0-9a-f]{64}$'),
  account_input_generation               BIGINT NOT NULL
    CHECK(account_input_generation BETWEEN 1 AND 9007199254740991),
  account_input_transition_sha256        TEXT NOT NULL
    CHECK(account_input_transition_sha256 ~ '^[0-9a-f]{64}$'),
  account_semantic_sha256                TEXT NOT NULL
    CHECK(account_semantic_sha256 ~ '^[0-9a-f]{64}$'),
  track_input_generation                 BIGINT NOT NULL
    CHECK(track_input_generation BETWEEN 1 AND 9007199254740991),
  track_input_transition_sha256          TEXT NOT NULL
    CHECK(track_input_transition_sha256 ~ '^[0-9a-f]{64}$'),
  track_semantic_sha256                  TEXT NOT NULL
    CHECK(track_semantic_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_policy_sha256                TEXT NOT NULL
    CHECK(canonical_policy_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_policy_ciphertext            TEXT NOT NULL
    CHECK(octet_length(canonical_policy_ciphertext) BETWEEN 54 AND 174814
      AND canonical_policy_ciphertext ~
        '^bluey-jobs:v1:[A-Za-z0-9_-]+$'),
  canonical_role_id                      TEXT NOT NULL
    CHECK(length(canonical_role_id) BETWEEN 1 AND 128
      AND canonical_role_id ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
  canonical_role_family                  TEXT NOT NULL
    CHECK(length(canonical_role_family) BETWEEN 1 AND 128
      AND canonical_role_family ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
  verified_application_identity_id       TEXT NOT NULL
    CHECK(length(verified_application_identity_id) BETWEEN 1 AND 128),
  verified_application_identity_sha256   TEXT NOT NULL
    CHECK(verified_application_identity_sha256 ~ '^[0-9a-f]{64}$'),
  source_resume_asset_id                 TEXT NOT NULL
    CHECK(length(source_resume_asset_id) BETWEEN 1 AND 128),
  source_resume_sha256                   TEXT NOT NULL
    CHECK(source_resume_sha256 ~ '^[0-9a-f]{64}$'),
  job_preferences_sha256                 TEXT NOT NULL
    CHECK(job_preferences_sha256 ~ '^[0-9a-f]{64}$'),
  predecessor_revision_id                TEXT,
  predecessor_revision_no                BIGINT
    CHECK(predecessor_revision_no IS NULL
      OR predecessor_revision_no BETWEEN 1 AND 9007199254740991),
  predecessor_policy_sha256               TEXT
    CHECK(predecessor_policy_sha256 IS NULL
      OR predecessor_policy_sha256 ~ '^[0-9a-f]{64}$'),
  compatibility_classification           TEXT NOT NULL CHECK(
    compatibility_classification IN (
      'initial', 'equivalent', 'compatible', 'review_required', 'incompatible'
    )
  ),
  review_state                           TEXT NOT NULL
    CHECK(review_state IN ('pending_review', 'approved', 'rejected')),
  created_by                             TEXT NOT NULL
    CHECK(length(created_by) BETWEEN 1 AND 128),
  created_at_ms                          BIGINT NOT NULL
    CHECK(created_at_ms BETWEEN 0 AND 9007199254740991),
  -- The same canonical SHA may recur in a later monotonic revision to represent
  -- an intentional reviewed reversion; revision number remains the authority.
  UNIQUE(account_id, career_track_id, revision_no),
  UNIQUE(
    account_id, career_track_id, revision_no, revision_id,
    canonical_policy_sha256
  ),
  FOREIGN KEY(account_id, career_track_id)
    REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE,
  FOREIGN KEY(
    account_id, career_track_id, predecessor_revision_no,
    predecessor_revision_id, predecessor_policy_sha256
  ) REFERENCES jobs_track_policy_revisions(
    account_id, career_track_id, revision_no, revision_id,
    canonical_policy_sha256
  ) ON DELETE CASCADE,
  CHECK(
    (revision_no = 1
      AND predecessor_revision_id IS NULL
      AND predecessor_revision_no IS NULL
      AND predecessor_policy_sha256 IS NULL
      AND compatibility_classification = 'initial')
    OR (revision_no > 1
      AND predecessor_revision_id IS NOT NULL
      AND predecessor_revision_no = revision_no - 1
      AND predecessor_policy_sha256 IS NOT NULL
      AND compatibility_classification <> 'initial')
  )
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_review_receipts (
  review_receipt_id                       TEXT PRIMARY KEY
    CHECK(length(review_receipt_id) BETWEEN 20 AND 128
      AND review_receipt_id ~ '^[A-Za-z0-9][A-Za-z0-9_-]{19,127}$'),
  review_receipt_sha256                   TEXT NOT NULL UNIQUE
    CHECK(review_receipt_sha256 ~ '^[0-9a-f]{64}$'),
  canonical_review_receipt_ciphertext     TEXT NOT NULL
    CHECK(octet_length(canonical_review_receipt_ciphertext) BETWEEN 54 AND 43742
      AND canonical_review_receipt_ciphertext ~
        '^bluey-jobs:v1:[A-Za-z0-9_-]+$'),
  account_id                              TEXT NOT NULL,
  career_track_id                         TEXT NOT NULL,
  policy_revision_id                      TEXT NOT NULL,
  policy_revision_no                      BIGINT NOT NULL
    CHECK(policy_revision_no BETWEEN 1 AND 9007199254740991),
  canonical_policy_sha256                 TEXT NOT NULL
    CHECK(canonical_policy_sha256 ~ '^[0-9a-f]{64}$'),
  taxonomy_digest_sha256                  TEXT NOT NULL
    CHECK(taxonomy_digest_sha256 ~ '^[0-9a-f]{64}$'),
  taxonomy_activation_epoch               BIGINT NOT NULL,
  canonicalizer_schema_version            BIGINT NOT NULL,
  canonicalizer_digest_sha256             TEXT NOT NULL,
  account_input_generation                BIGINT NOT NULL,
  account_input_transition_sha256         TEXT NOT NULL,
  account_semantic_sha256                 TEXT NOT NULL,
  track_input_generation                  BIGINT NOT NULL,
  track_input_transition_sha256           TEXT NOT NULL,
  track_semantic_sha256                   TEXT NOT NULL,
  verified_application_identity_sha256    TEXT NOT NULL
    CHECK(verified_application_identity_sha256 ~ '^[0-9a-f]{64}$'),
  source_resume_sha256                    TEXT NOT NULL
    CHECK(source_resume_sha256 ~ '^[0-9a-f]{64}$'),
  job_preferences_sha256                  TEXT NOT NULL
    CHECK(job_preferences_sha256 ~ '^[0-9a-f]{64}$'),
  reviewer_id                             TEXT NOT NULL
    CHECK(length(reviewer_id) BETWEEN 1 AND 128),
  decision                                TEXT NOT NULL
    CHECK(decision IN ('approved', 'rejected')),
  decided_at_ms                           BIGINT NOT NULL
    CHECK(decided_at_ms BETWEEN 0 AND 9007199254740991),
  UNIQUE(account_id, career_track_id, policy_revision_id, reviewer_id),
  UNIQUE(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256, review_receipt_id, review_receipt_sha256
  ),
  FOREIGN KEY(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256
  ) REFERENCES jobs_track_policy_revisions(
    account_id, career_track_id, revision_no, revision_id,
    canonical_policy_sha256
  ) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_head_transitions (
  account_id                              TEXT NOT NULL,
  career_track_id                         TEXT NOT NULL,
  head_generation                        BIGINT NOT NULL,
  previous_head_generation               BIGINT NOT NULL,
  policy_revision_id                     TEXT NOT NULL,
  policy_revision_no                     BIGINT NOT NULL,
  canonical_policy_sha256                TEXT NOT NULL,
  taxonomy_digest_sha256                 TEXT NOT NULL,
  taxonomy_activation_epoch              BIGINT NOT NULL,
  canonicalizer_schema_version           BIGINT NOT NULL,
  canonicalizer_digest_sha256            TEXT NOT NULL,
  account_input_generation               BIGINT NOT NULL,
  account_input_transition_sha256        TEXT NOT NULL,
  account_semantic_sha256                TEXT NOT NULL,
  track_input_generation                 BIGINT NOT NULL,
  track_input_transition_sha256          TEXT NOT NULL,
  track_semantic_sha256                  TEXT NOT NULL,
  verified_application_identity_sha256   TEXT NOT NULL,
  source_resume_sha256                   TEXT NOT NULL,
  job_preferences_sha256                 TEXT NOT NULL,
  review_receipt_id                      TEXT NOT NULL,
  review_receipt_sha256                  TEXT NOT NULL,
  head_transition_sha256                 TEXT NOT NULL UNIQUE,
  predecessor_head_transition_sha256     TEXT,
  updated_by                             TEXT NOT NULL,
  updated_at_ms                          BIGINT NOT NULL,
  PRIMARY KEY(account_id, career_track_id, head_generation),
  UNIQUE(account_id, career_track_id, head_generation,
         head_transition_sha256),
  FOREIGN KEY(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256
  ) REFERENCES jobs_track_policy_revisions(
    account_id, career_track_id, revision_no, revision_id,
    canonical_policy_sha256
  ) ON DELETE CASCADE,
  FOREIGN KEY(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256, review_receipt_id, review_receipt_sha256
  ) REFERENCES jobs_track_policy_review_receipts(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256, review_receipt_id, review_receipt_sha256
  ) ON DELETE CASCADE,
  FOREIGN KEY(account_id, career_track_id, previous_head_generation,
              predecessor_head_transition_sha256)
    REFERENCES jobs_track_policy_head_transitions(
      account_id, career_track_id, head_generation, head_transition_sha256)
      ON DELETE CASCADE,
  CHECK(
    (head_generation = 1 AND previous_head_generation = 0
      AND predecessor_head_transition_sha256 IS NULL)
    OR (head_generation > 1
      AND previous_head_generation = head_generation - 1
      AND predecessor_head_transition_sha256 IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS jobs_track_policy_heads (
  account_id                              TEXT NOT NULL,
  career_track_id                         TEXT NOT NULL,
  head_generation                        BIGINT NOT NULL
    CHECK(head_generation BETWEEN 1 AND 9007199254740991),
  previous_head_generation               BIGINT NOT NULL
    CHECK(previous_head_generation BETWEEN 0 AND 9007199254740991),
  policy_revision_id                     TEXT NOT NULL,
  policy_revision_no                     BIGINT NOT NULL
    CHECK(policy_revision_no BETWEEN 1 AND 9007199254740991),
  canonical_policy_sha256                TEXT NOT NULL
    CHECK(canonical_policy_sha256 ~ '^[0-9a-f]{64}$'),
  taxonomy_digest_sha256                 TEXT NOT NULL
    CHECK(taxonomy_digest_sha256 ~ '^[0-9a-f]{64}$'),
  taxonomy_activation_epoch              BIGINT NOT NULL,
  canonicalizer_schema_version           BIGINT NOT NULL,
  canonicalizer_digest_sha256            TEXT NOT NULL,
  account_input_generation               BIGINT NOT NULL,
  account_input_transition_sha256        TEXT NOT NULL,
  account_semantic_sha256                TEXT NOT NULL,
  track_input_generation                 BIGINT NOT NULL,
  track_input_transition_sha256          TEXT NOT NULL,
  track_semantic_sha256                  TEXT NOT NULL,
  verified_application_identity_sha256   TEXT NOT NULL
    CHECK(verified_application_identity_sha256 ~ '^[0-9a-f]{64}$'),
  source_resume_sha256                   TEXT NOT NULL
    CHECK(source_resume_sha256 ~ '^[0-9a-f]{64}$'),
  job_preferences_sha256                 TEXT NOT NULL
    CHECK(job_preferences_sha256 ~ '^[0-9a-f]{64}$'),
  review_receipt_id                      TEXT NOT NULL,
  review_receipt_sha256                  TEXT NOT NULL
    CHECK(review_receipt_sha256 ~ '^[0-9a-f]{64}$'),
  head_transition_sha256                 TEXT NOT NULL UNIQUE
    CHECK(head_transition_sha256 ~ '^[0-9a-f]{64}$'),
  predecessor_head_transition_sha256     TEXT
    CHECK(predecessor_head_transition_sha256 IS NULL
      OR predecessor_head_transition_sha256 ~ '^[0-9a-f]{64}$'),
  updated_by                             TEXT NOT NULL
    CHECK(length(updated_by) BETWEEN 1 AND 128),
  updated_at_ms                          BIGINT NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  PRIMARY KEY(account_id, career_track_id),
  UNIQUE(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256
  ),
  FOREIGN KEY(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256
  ) REFERENCES jobs_track_policy_revisions(
    account_id, career_track_id, revision_no, revision_id,
    canonical_policy_sha256
  ) ON DELETE CASCADE,
  FOREIGN KEY(account_id, career_track_id, head_generation,
              head_transition_sha256)
    REFERENCES jobs_track_policy_head_transitions(
      account_id, career_track_id, head_generation, head_transition_sha256)
      ON DELETE CASCADE,
  FOREIGN KEY(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256, review_receipt_id, review_receipt_sha256
  ) REFERENCES jobs_track_policy_review_receipts(
    account_id, career_track_id, policy_revision_no, policy_revision_id,
    canonical_policy_sha256, review_receipt_id, review_receipt_sha256
  ) ON DELETE CASCADE,
  CHECK(head_generation = policy_revision_no),
  CHECK(
    (head_generation = 1
      AND previous_head_generation = 0
      AND predecessor_head_transition_sha256 IS NULL)
    OR (head_generation > 1
      AND previous_head_generation = head_generation - 1
      AND predecessor_head_transition_sha256 IS NOT NULL)
  )
);

CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_revisions_history
  ON jobs_track_policy_revisions(
    account_id, career_track_id, revision_no DESC
  );
CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_revisions_authority
  ON jobs_track_policy_revisions(
    account_id, verified_application_identity_id, source_resume_asset_id,
    review_state
  );
CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_review_receipts_revision
  ON jobs_track_policy_review_receipts(
    account_id, career_track_id, policy_revision_no, decision, decided_at_ms DESC
  );
CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_heads_revision
  ON jobs_track_policy_heads(
    account_id, policy_revision_id, policy_revision_no, head_generation
  );
CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_account_input_transitions_history
  ON jobs_track_policy_account_input_transitions(account_id, input_generation DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_track_input_transitions_history
  ON jobs_track_policy_track_input_transitions(
    account_id, career_track_id, input_generation DESC
  );
CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_head_transitions_history
  ON jobs_track_policy_head_transitions(
    account_id, career_track_id, head_generation DESC
  );

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_activation_event_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (NEW.activation_epoch = 1 AND EXISTS (
        SELECT 1 FROM jobs_track_policy_taxonomy_activation_events
      )) OR (NEW.activation_epoch > 1 AND NOT EXISTS (
        SELECT 1 FROM jobs_track_policy_taxonomy_activation_events predecessor
         WHERE predecessor.activation_epoch = NEW.previous_activation_epoch
           AND predecessor.activation_transition_sha256 =
               NEW.predecessor_activation_transition_sha256
           AND predecessor.activated_at_ms <= NEW.activated_at_ms
           AND NOT (
             predecessor.taxonomy_version = NEW.taxonomy_version
             AND predecessor.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
             AND predecessor.canonicalizer_schema_version =
                 NEW.canonicalizer_schema_version
             AND predecessor.canonicalizer_digest_sha256 =
                 NEW.canonicalizer_digest_sha256
           )
      )) THEN
    RAISE EXCEPTION 'invalid taxonomy activation transition';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_track_policy_activation_head()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (TG_OP = 'INSERT' AND (
        NEW.activation_epoch <> 1 OR NEW.previous_activation_epoch <> 0
        OR NEW.predecessor_activation_transition_sha256 IS NOT NULL
      )) OR (TG_OP = 'UPDATE' AND (
        NEW.singleton_id <> OLD.singleton_id
        OR NEW.activation_epoch <> OLD.activation_epoch + 1
        OR NEW.previous_activation_epoch <> OLD.activation_epoch
        OR NEW.predecessor_activation_transition_sha256 <>
            OLD.activation_transition_sha256
        OR NEW.activated_at_ms < OLD.activated_at_ms
      )) OR NOT EXISTS (
        SELECT 1 FROM jobs_track_policy_taxonomy_activation_events event
         WHERE event.activation_epoch = NEW.activation_epoch
           AND event.previous_activation_epoch = NEW.previous_activation_epoch
           AND event.taxonomy_version = NEW.taxonomy_version
           AND event.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
           AND event.canonicalizer_schema_version =
               NEW.canonicalizer_schema_version
           AND event.canonicalizer_digest_sha256 =
               NEW.canonicalizer_digest_sha256
           AND event.activation_transition_sha256 =
               NEW.activation_transition_sha256
           AND event.predecessor_activation_transition_sha256
                 IS NOT DISTINCT FROM NEW.predecessor_activation_transition_sha256
           AND event.activated_at_ms = NEW.activated_at_ms
      ) THEN
    RAISE EXCEPTION 'taxonomy activation head must advance by exact CAS';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_account_input_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (NEW.input_generation = 1 AND EXISTS (
        SELECT 1 FROM jobs_track_policy_account_input_transitions
         WHERE account_id = NEW.account_id
      )) OR (NEW.input_generation > 1 AND NOT EXISTS (
        SELECT 1 FROM jobs_track_policy_account_input_transitions predecessor
         WHERE predecessor.account_id = NEW.account_id
           AND predecessor.input_generation = NEW.previous_input_generation
           AND predecessor.input_transition_sha256 =
               NEW.predecessor_input_transition_sha256
           AND predecessor.changed_at_ms <= NEW.changed_at_ms
      )) THEN
    RAISE EXCEPTION 'invalid account semantic-input transition';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_track_policy_account_input_head()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (TG_OP = 'INSERT' AND (
        NEW.input_generation <> 1 OR NEW.previous_input_generation <> 0
        OR NEW.predecessor_input_transition_sha256 IS NOT NULL
      )) OR (TG_OP = 'UPDATE' AND (
        NEW.account_id <> OLD.account_id
        OR NEW.input_generation <> OLD.input_generation + 1
        OR NEW.previous_input_generation <> OLD.input_generation
        OR NEW.predecessor_input_transition_sha256 <>
            OLD.input_transition_sha256
        OR NEW.updated_at_ms < OLD.updated_at_ms
      )) OR NOT EXISTS (
        SELECT 1 FROM jobs_track_policy_account_input_transitions event
         WHERE event.account_id = NEW.account_id
           AND event.input_generation = NEW.input_generation
           AND event.previous_input_generation = NEW.previous_input_generation
           AND event.input_transition_id = NEW.input_transition_id
           AND event.input_kind = NEW.input_kind
           AND event.input_subject_sha256 = NEW.input_subject_sha256
           AND event.account_semantic_sha256 = NEW.account_semantic_sha256
           AND event.input_transition_sha256 = NEW.input_transition_sha256
           AND event.predecessor_input_transition_sha256
                 IS NOT DISTINCT FROM NEW.predecessor_input_transition_sha256
           AND event.changed_at_ms = NEW.updated_at_ms
      ) THEN
    RAISE EXCEPTION 'account semantic-input head must advance by exact CAS';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_track_input_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (NEW.input_generation = 1 AND EXISTS (
        SELECT 1 FROM jobs_track_policy_track_input_transitions
         WHERE account_id = NEW.account_id
           AND career_track_id = NEW.career_track_id
      )) OR (NEW.input_generation > 1 AND NOT EXISTS (
        SELECT 1 FROM jobs_track_policy_track_input_transitions predecessor
         WHERE predecessor.account_id = NEW.account_id
           AND predecessor.career_track_id = NEW.career_track_id
           AND predecessor.input_generation = NEW.previous_input_generation
           AND predecessor.input_transition_sha256 =
               NEW.predecessor_input_transition_sha256
           AND predecessor.changed_at_ms <= NEW.changed_at_ms
      )) THEN
    RAISE EXCEPTION 'invalid Track semantic-input transition';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_track_policy_track_input_head()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF (TG_OP = 'INSERT' AND (
        NEW.input_generation <> 1 OR NEW.previous_input_generation <> 0
        OR NEW.predecessor_input_transition_sha256 IS NOT NULL
      )) OR (TG_OP = 'UPDATE' AND (
        NEW.account_id <> OLD.account_id
        OR NEW.career_track_id <> OLD.career_track_id
        OR NEW.input_generation <> OLD.input_generation + 1
        OR NEW.previous_input_generation <> OLD.input_generation
        OR NEW.predecessor_input_transition_sha256 <>
            OLD.input_transition_sha256
        OR NEW.updated_at_ms < OLD.updated_at_ms
      )) OR NOT EXISTS (
        SELECT 1 FROM jobs_track_policy_track_input_transitions event
         WHERE event.account_id = NEW.account_id
           AND event.career_track_id = NEW.career_track_id
           AND event.input_generation = NEW.input_generation
           AND event.previous_input_generation = NEW.previous_input_generation
           AND event.input_transition_id = NEW.input_transition_id
           AND event.track_semantic_sha256 = NEW.track_semantic_sha256
           AND event.input_transition_sha256 = NEW.input_transition_sha256
           AND event.predecessor_input_transition_sha256
                 IS NOT DISTINCT FROM NEW.predecessor_input_transition_sha256
           AND event.changed_at_ms = NEW.updated_at_ms
      ) THEN
    RAISE EXCEPTION 'Track semantic-input head must advance by exact CAS';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION reject_jobs_track_policy_global_immutable_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'global taxonomy activation evidence is immutable';
END;
$$;

CREATE OR REPLACE FUNCTION reject_jobs_track_policy_account_input_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE'
     AND NOT EXISTS (SELECT 1 FROM accounts WHERE id = OLD.account_id) THEN
    RETURN OLD;
  END IF;
  RAISE EXCEPTION 'account semantic-input evidence is immutable';
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_revision_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_track_policy_taxonomy_activation_head activation
     WHERE activation.singleton_id = 1
       AND activation.activation_epoch = NEW.taxonomy_activation_epoch
       AND activation.taxonomy_version = NEW.taxonomy_version
       AND activation.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
       AND activation.canonicalizer_schema_version =
           NEW.canonicalizer_schema_version
       AND activation.canonicalizer_digest_sha256 =
           NEW.canonicalizer_digest_sha256
  ) OR NOT EXISTS (
    SELECT 1 FROM jobs_track_policy_account_input_heads input
     WHERE input.account_id = NEW.account_id
       AND input.input_generation = NEW.account_input_generation
       AND input.input_transition_sha256 = NEW.account_input_transition_sha256
       AND input.account_semantic_sha256 = NEW.account_semantic_sha256
  ) OR NOT EXISTS (
    SELECT 1 FROM jobs_track_policy_track_input_heads input
     WHERE input.account_id = NEW.account_id
       AND input.career_track_id = NEW.career_track_id
       AND input.input_generation = NEW.track_input_generation
       AND input.input_transition_sha256 = NEW.track_input_transition_sha256
       AND input.track_semantic_sha256 = NEW.track_semantic_sha256
  ) OR NOT EXISTS (
    SELECT 1
      FROM jobs_application_identities AS identity
     WHERE identity.account_id = NEW.account_id
       AND identity.id = NEW.verified_application_identity_id
       AND identity.verification_status = 'verified'
  ) OR NOT EXISTS (
    SELECT 1
      FROM jobs_resume_source_assets AS asset
     WHERE asset.account_id = NEW.account_id
       AND asset.id = NEW.source_resume_asset_id
       AND asset.sha256 = NEW.source_resume_sha256
  ) OR (NEW.revision_no > 1 AND NOT EXISTS (
    SELECT 1
      FROM jobs_track_policy_revisions AS predecessor
     WHERE predecessor.account_id = NEW.account_id
       AND predecessor.career_track_id = NEW.career_track_id
       AND predecessor.revision_no = NEW.predecessor_revision_no
       AND predecessor.revision_id = NEW.predecessor_revision_id
       AND predecessor.canonical_policy_sha256 = NEW.predecessor_policy_sha256
       AND predecessor.created_at_ms <= NEW.created_at_ms
  )) THEN
    RAISE EXCEPTION 'invalid Career Track policy revision authority';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION reject_jobs_track_policy_immutable_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' AND (
    NOT EXISTS (SELECT 1 FROM accounts WHERE id = OLD.account_id)
    OR NOT EXISTS (
      SELECT 1
        FROM jobs_tracks
       WHERE account_id = OLD.account_id AND id = OLD.career_track_id
    )
  ) THEN
    RETURN OLD;
  END IF;
  RAISE EXCEPTION 'Career Track policy evidence is immutable';
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_review_receipt_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
      FROM jobs_track_policy_revisions AS revision
     WHERE revision.account_id = NEW.account_id
       AND revision.career_track_id = NEW.career_track_id
       AND revision.revision_id = NEW.policy_revision_id
       AND revision.revision_no = NEW.policy_revision_no
       AND revision.canonical_policy_sha256 = NEW.canonical_policy_sha256
       AND revision.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
       AND revision.taxonomy_activation_epoch = NEW.taxonomy_activation_epoch
       AND revision.canonicalizer_schema_version = NEW.canonicalizer_schema_version
       AND revision.canonicalizer_digest_sha256 = NEW.canonicalizer_digest_sha256
       AND revision.account_input_generation = NEW.account_input_generation
       AND revision.account_input_transition_sha256 =
           NEW.account_input_transition_sha256
       AND revision.account_semantic_sha256 = NEW.account_semantic_sha256
       AND revision.track_input_generation = NEW.track_input_generation
       AND revision.track_input_transition_sha256 =
           NEW.track_input_transition_sha256
       AND revision.track_semantic_sha256 = NEW.track_semantic_sha256
       AND revision.verified_application_identity_sha256 =
         NEW.verified_application_identity_sha256
       AND revision.source_resume_sha256 = NEW.source_resume_sha256
       AND revision.job_preferences_sha256 = NEW.job_preferences_sha256
       AND revision.review_state = NEW.decision
       AND revision.created_at_ms <= NEW.decided_at_ms
  ) THEN
    RAISE EXCEPTION 'review receipt does not bind the exact policy revision';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_head_transition_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_track_policy_revisions revision
     WHERE revision.account_id = NEW.account_id
       AND revision.career_track_id = NEW.career_track_id
       AND revision.revision_id = NEW.policy_revision_id
       AND revision.revision_no = NEW.policy_revision_no
       AND revision.canonical_policy_sha256 = NEW.canonical_policy_sha256
       AND revision.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
       AND revision.taxonomy_activation_epoch = NEW.taxonomy_activation_epoch
       AND revision.canonicalizer_schema_version = NEW.canonicalizer_schema_version
       AND revision.canonicalizer_digest_sha256 = NEW.canonicalizer_digest_sha256
       AND revision.account_input_generation = NEW.account_input_generation
       AND revision.account_input_transition_sha256 =
           NEW.account_input_transition_sha256
       AND revision.account_semantic_sha256 = NEW.account_semantic_sha256
       AND revision.track_input_generation = NEW.track_input_generation
       AND revision.track_input_transition_sha256 = NEW.track_input_transition_sha256
       AND revision.track_semantic_sha256 = NEW.track_semantic_sha256
       AND revision.verified_application_identity_sha256 =
           NEW.verified_application_identity_sha256
       AND revision.source_resume_sha256 = NEW.source_resume_sha256
       AND revision.job_preferences_sha256 = NEW.job_preferences_sha256
       AND revision.review_state = 'approved'
  ) OR NOT EXISTS (
    SELECT 1 FROM jobs_track_policy_review_receipts receipt
     WHERE receipt.account_id = NEW.account_id
       AND receipt.career_track_id = NEW.career_track_id
       AND receipt.policy_revision_id = NEW.policy_revision_id
       AND receipt.policy_revision_no = NEW.policy_revision_no
       AND receipt.canonical_policy_sha256 = NEW.canonical_policy_sha256
       AND receipt.review_receipt_id = NEW.review_receipt_id
       AND receipt.review_receipt_sha256 = NEW.review_receipt_sha256
       AND receipt.decision = 'approved'
  ) OR (NEW.head_generation > 1 AND NOT EXISTS (
    SELECT 1 FROM jobs_track_policy_head_transitions predecessor
     WHERE predecessor.account_id = NEW.account_id
       AND predecessor.career_track_id = NEW.career_track_id
       AND predecessor.head_generation = NEW.previous_head_generation
       AND predecessor.head_transition_sha256 =
           NEW.predecessor_head_transition_sha256
       AND predecessor.updated_at_ms <= NEW.updated_at_ms
  )) THEN
    RAISE EXCEPTION 'invalid Career Track policy head transition';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_jobs_track_policy_head_insert()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.head_generation <> 1 OR NEW.previous_head_generation <> 0
     OR NEW.predecessor_head_transition_sha256 IS NOT NULL
     OR NOT EXISTS (
       SELECT 1 FROM jobs_track_policy_head_transitions event
        WHERE event.account_id = NEW.account_id
          AND event.career_track_id = NEW.career_track_id
          AND event.head_generation = NEW.head_generation
          AND event.previous_head_generation = NEW.previous_head_generation
          AND event.policy_revision_id = NEW.policy_revision_id
          AND event.policy_revision_no = NEW.policy_revision_no
          AND event.canonical_policy_sha256 = NEW.canonical_policy_sha256
          AND event.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
          AND event.taxonomy_activation_epoch = NEW.taxonomy_activation_epoch
          AND event.canonicalizer_schema_version = NEW.canonicalizer_schema_version
          AND event.canonicalizer_digest_sha256 = NEW.canonicalizer_digest_sha256
          AND event.account_input_generation = NEW.account_input_generation
          AND event.account_input_transition_sha256 =
              NEW.account_input_transition_sha256
          AND event.account_semantic_sha256 = NEW.account_semantic_sha256
          AND event.track_input_generation = NEW.track_input_generation
          AND event.track_input_transition_sha256 = NEW.track_input_transition_sha256
          AND event.track_semantic_sha256 = NEW.track_semantic_sha256
          AND event.verified_application_identity_sha256 =
              NEW.verified_application_identity_sha256
          AND event.source_resume_sha256 = NEW.source_resume_sha256
          AND event.job_preferences_sha256 = NEW.job_preferences_sha256
          AND event.review_receipt_id = NEW.review_receipt_id
          AND event.review_receipt_sha256 = NEW.review_receipt_sha256
          AND event.head_transition_sha256 = NEW.head_transition_sha256
          AND event.predecessor_head_transition_sha256 IS NOT DISTINCT FROM
              NEW.predecessor_head_transition_sha256
          AND event.updated_by = NEW.updated_by
          AND event.updated_at_ms = NEW.updated_at_ms
     )
     OR NOT EXISTS (
       SELECT 1
         FROM jobs_track_policy_revisions AS revision
        WHERE revision.account_id = NEW.account_id
          AND revision.career_track_id = NEW.career_track_id
          AND revision.revision_id = NEW.policy_revision_id
          AND revision.revision_no = NEW.policy_revision_no
          AND revision.canonical_policy_sha256 = NEW.canonical_policy_sha256
          AND revision.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
          AND revision.verified_application_identity_sha256 =
            NEW.verified_application_identity_sha256
          AND revision.source_resume_sha256 = NEW.source_resume_sha256
          AND revision.job_preferences_sha256 = NEW.job_preferences_sha256
          AND EXISTS (
            SELECT 1
              FROM jobs_application_identities AS identity
             WHERE identity.account_id = revision.account_id
               AND identity.id = revision.verified_application_identity_id
               AND identity.verification_status = 'verified'
          )
          AND EXISTS (
            SELECT 1
              FROM jobs_resume_source_assets AS asset
             WHERE asset.account_id = revision.account_id
               AND asset.id = revision.source_resume_asset_id
               AND asset.sha256 = revision.source_resume_sha256
          )
          AND revision.review_state = 'approved'
          AND revision.created_at_ms <= NEW.updated_at_ms
     ) OR NOT EXISTS (
       SELECT 1
         FROM jobs_track_policy_review_receipts AS receipt
        WHERE receipt.account_id = NEW.account_id
          AND receipt.career_track_id = NEW.career_track_id
          AND receipt.policy_revision_id = NEW.policy_revision_id
          AND receipt.policy_revision_no = NEW.policy_revision_no
          AND receipt.canonical_policy_sha256 = NEW.canonical_policy_sha256
          AND receipt.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
          AND receipt.verified_application_identity_sha256 =
            NEW.verified_application_identity_sha256
          AND receipt.source_resume_sha256 = NEW.source_resume_sha256
          AND receipt.job_preferences_sha256 = NEW.job_preferences_sha256
          AND receipt.review_receipt_id = NEW.review_receipt_id
          AND receipt.review_receipt_sha256 = NEW.review_receipt_sha256
          AND receipt.decision = 'approved'
          AND receipt.decided_at_ms <= NEW.updated_at_ms
     ) THEN
    RAISE EXCEPTION 'invalid initial Career Track policy head';
  END IF;
  RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_track_policy_head_monotonic()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.account_id <> OLD.account_id
     OR NEW.career_track_id <> OLD.career_track_id
     OR NEW.head_generation <> OLD.head_generation + 1
     OR NEW.previous_head_generation <> OLD.head_generation
     OR NEW.predecessor_head_transition_sha256 <> OLD.head_transition_sha256
     OR NEW.policy_revision_no <> OLD.policy_revision_no + 1
     OR NEW.policy_revision_id = OLD.policy_revision_id
     OR NEW.head_transition_sha256 = OLD.head_transition_sha256
     OR NEW.updated_at_ms < OLD.updated_at_ms
     OR NOT EXISTS (
       SELECT 1 FROM jobs_track_policy_head_transitions event
        WHERE event.account_id = NEW.account_id
          AND event.career_track_id = NEW.career_track_id
          AND event.head_generation = NEW.head_generation
          AND event.previous_head_generation = NEW.previous_head_generation
          AND event.policy_revision_id = NEW.policy_revision_id
          AND event.policy_revision_no = NEW.policy_revision_no
          AND event.canonical_policy_sha256 = NEW.canonical_policy_sha256
          AND event.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
          AND event.taxonomy_activation_epoch = NEW.taxonomy_activation_epoch
          AND event.canonicalizer_schema_version = NEW.canonicalizer_schema_version
          AND event.canonicalizer_digest_sha256 = NEW.canonicalizer_digest_sha256
          AND event.account_input_generation = NEW.account_input_generation
          AND event.account_input_transition_sha256 =
              NEW.account_input_transition_sha256
          AND event.account_semantic_sha256 = NEW.account_semantic_sha256
          AND event.track_input_generation = NEW.track_input_generation
          AND event.track_input_transition_sha256 = NEW.track_input_transition_sha256
          AND event.track_semantic_sha256 = NEW.track_semantic_sha256
          AND event.verified_application_identity_sha256 =
              NEW.verified_application_identity_sha256
          AND event.source_resume_sha256 = NEW.source_resume_sha256
          AND event.job_preferences_sha256 = NEW.job_preferences_sha256
          AND event.review_receipt_id = NEW.review_receipt_id
          AND event.review_receipt_sha256 = NEW.review_receipt_sha256
          AND event.head_transition_sha256 = NEW.head_transition_sha256
          AND event.predecessor_head_transition_sha256 =
              NEW.predecessor_head_transition_sha256
          AND event.updated_by = NEW.updated_by
          AND event.updated_at_ms = NEW.updated_at_ms
     )
     OR NOT EXISTS (
       SELECT 1
         FROM jobs_track_policy_revisions AS revision
        WHERE revision.account_id = NEW.account_id
          AND revision.career_track_id = NEW.career_track_id
          AND revision.revision_id = NEW.policy_revision_id
          AND revision.revision_no = NEW.policy_revision_no
          AND revision.canonical_policy_sha256 = NEW.canonical_policy_sha256
          AND revision.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
          AND revision.verified_application_identity_sha256 =
            NEW.verified_application_identity_sha256
          AND revision.source_resume_sha256 = NEW.source_resume_sha256
          AND revision.job_preferences_sha256 = NEW.job_preferences_sha256
          AND EXISTS (
            SELECT 1
              FROM jobs_application_identities AS identity
             WHERE identity.account_id = revision.account_id
               AND identity.id = revision.verified_application_identity_id
               AND identity.verification_status = 'verified'
          )
          AND EXISTS (
            SELECT 1
              FROM jobs_resume_source_assets AS asset
             WHERE asset.account_id = revision.account_id
               AND asset.id = revision.source_resume_asset_id
               AND asset.sha256 = revision.source_resume_sha256
          )
          AND revision.predecessor_revision_id = OLD.policy_revision_id
          AND revision.predecessor_revision_no = OLD.policy_revision_no
          AND revision.predecessor_policy_sha256 = OLD.canonical_policy_sha256
          AND revision.review_state = 'approved'
          AND revision.created_at_ms <= NEW.updated_at_ms
     ) OR NOT EXISTS (
       SELECT 1
         FROM jobs_track_policy_review_receipts AS receipt
        WHERE receipt.account_id = NEW.account_id
          AND receipt.career_track_id = NEW.career_track_id
          AND receipt.policy_revision_id = NEW.policy_revision_id
          AND receipt.policy_revision_no = NEW.policy_revision_no
          AND receipt.canonical_policy_sha256 = NEW.canonical_policy_sha256
          AND receipt.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256
          AND receipt.verified_application_identity_sha256 =
            NEW.verified_application_identity_sha256
          AND receipt.source_resume_sha256 = NEW.source_resume_sha256
          AND receipt.job_preferences_sha256 = NEW.job_preferences_sha256
          AND receipt.review_receipt_id = NEW.review_receipt_id
          AND receipt.review_receipt_sha256 = NEW.review_receipt_sha256
          AND receipt.decision = 'approved'
          AND receipt.decided_at_ms <= NEW.updated_at_ms
     ) THEN
    RAISE EXCEPTION 'Career Track policy head must advance by exact CAS';
  END IF;
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_jobs_track_policy_taxonomy_activation_events_validate_insert
  ON jobs_track_policy_taxonomy_activation_events;
CREATE TRIGGER trg_jobs_track_policy_taxonomy_activation_events_validate_insert
BEFORE INSERT ON jobs_track_policy_taxonomy_activation_events
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_activation_event_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_taxonomy_activation_events_no_update
  ON jobs_track_policy_taxonomy_activation_events;
CREATE TRIGGER trg_jobs_track_policy_taxonomy_activation_events_no_update
BEFORE UPDATE ON jobs_track_policy_taxonomy_activation_events
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_global_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_taxonomy_activation_events_no_delete
  ON jobs_track_policy_taxonomy_activation_events;
CREATE TRIGGER trg_jobs_track_policy_taxonomy_activation_events_no_delete
BEFORE DELETE ON jobs_track_policy_taxonomy_activation_events
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_global_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_taxonomy_activation_head_validate_insert
  ON jobs_track_policy_taxonomy_activation_head;
CREATE TRIGGER trg_jobs_track_policy_taxonomy_activation_head_validate_insert
BEFORE INSERT ON jobs_track_policy_taxonomy_activation_head
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_activation_head();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_taxonomy_activation_head_monotonic
  ON jobs_track_policy_taxonomy_activation_head;
CREATE TRIGGER trg_jobs_track_policy_taxonomy_activation_head_monotonic
BEFORE UPDATE ON jobs_track_policy_taxonomy_activation_head
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_activation_head();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_taxonomy_activation_head_no_delete
  ON jobs_track_policy_taxonomy_activation_head;
CREATE TRIGGER trg_jobs_track_policy_taxonomy_activation_head_no_delete
BEFORE DELETE ON jobs_track_policy_taxonomy_activation_head
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_global_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_track_policy_account_input_transitions_validate_insert
  ON jobs_track_policy_account_input_transitions;
CREATE TRIGGER trg_jobs_track_policy_account_input_transitions_validate_insert
BEFORE INSERT ON jobs_track_policy_account_input_transitions
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_account_input_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_account_input_transitions_no_update
  ON jobs_track_policy_account_input_transitions;
CREATE TRIGGER trg_jobs_track_policy_account_input_transitions_no_update
BEFORE UPDATE ON jobs_track_policy_account_input_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_account_input_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_account_input_transitions_no_delete
  ON jobs_track_policy_account_input_transitions;
CREATE TRIGGER trg_jobs_track_policy_account_input_transitions_no_delete
BEFORE DELETE ON jobs_track_policy_account_input_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_account_input_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_account_input_heads_validate_insert
  ON jobs_track_policy_account_input_heads;
CREATE TRIGGER trg_jobs_track_policy_account_input_heads_validate_insert
BEFORE INSERT ON jobs_track_policy_account_input_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_account_input_head();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_account_input_heads_monotonic
  ON jobs_track_policy_account_input_heads;
CREATE TRIGGER trg_jobs_track_policy_account_input_heads_monotonic
BEFORE UPDATE ON jobs_track_policy_account_input_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_account_input_head();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_account_input_heads_no_delete
  ON jobs_track_policy_account_input_heads;
CREATE TRIGGER trg_jobs_track_policy_account_input_heads_no_delete
BEFORE DELETE ON jobs_track_policy_account_input_heads
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_account_input_mutation();

DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_transitions_validate_insert
  ON jobs_track_policy_track_input_transitions;
CREATE TRIGGER trg_jobs_track_policy_track_input_transitions_validate_insert
BEFORE INSERT ON jobs_track_policy_track_input_transitions
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_track_input_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_transitions_no_update
  ON jobs_track_policy_track_input_transitions;
CREATE TRIGGER trg_jobs_track_policy_track_input_transitions_no_update
BEFORE UPDATE ON jobs_track_policy_track_input_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_transitions_no_delete
  ON jobs_track_policy_track_input_transitions;
CREATE TRIGGER trg_jobs_track_policy_track_input_transitions_no_delete
BEFORE DELETE ON jobs_track_policy_track_input_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_heads_validate_insert
  ON jobs_track_policy_track_input_heads;
CREATE TRIGGER trg_jobs_track_policy_track_input_heads_validate_insert
BEFORE INSERT ON jobs_track_policy_track_input_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_track_input_head();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_heads_monotonic
  ON jobs_track_policy_track_input_heads;
CREATE TRIGGER trg_jobs_track_policy_track_input_heads_monotonic
BEFORE UPDATE ON jobs_track_policy_track_input_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_track_input_head();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_heads_no_delete
  ON jobs_track_policy_track_input_heads;
CREATE TRIGGER trg_jobs_track_policy_track_input_heads_no_delete
BEFORE DELETE ON jobs_track_policy_track_input_heads
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_track_policy_revisions_validate_insert
  ON jobs_track_policy_revisions;
CREATE TRIGGER trg_jobs_track_policy_revisions_validate_insert
BEFORE INSERT ON jobs_track_policy_revisions
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_revision_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_revisions_no_update
  ON jobs_track_policy_revisions;
CREATE TRIGGER trg_jobs_track_policy_revisions_no_update
BEFORE UPDATE ON jobs_track_policy_revisions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_revisions_no_delete
  ON jobs_track_policy_revisions;
CREATE TRIGGER trg_jobs_track_policy_revisions_no_delete
BEFORE DELETE ON jobs_track_policy_revisions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_track_policy_review_receipts_validate_insert
  ON jobs_track_policy_review_receipts;
CREATE TRIGGER trg_jobs_track_policy_review_receipts_validate_insert
BEFORE INSERT ON jobs_track_policy_review_receipts
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_review_receipt_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_review_receipts_no_update
  ON jobs_track_policy_review_receipts;
CREATE TRIGGER trg_jobs_track_policy_review_receipts_no_update
BEFORE UPDATE ON jobs_track_policy_review_receipts
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_review_receipts_no_delete
  ON jobs_track_policy_review_receipts;
CREATE TRIGGER trg_jobs_track_policy_review_receipts_no_delete
BEFORE DELETE ON jobs_track_policy_review_receipts
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();

DROP TRIGGER IF EXISTS trg_jobs_track_policy_heads_validate_insert
  ON jobs_track_policy_heads;
DROP TRIGGER IF EXISTS trg_jobs_track_policy_head_transitions_validate_insert
  ON jobs_track_policy_head_transitions;
CREATE TRIGGER trg_jobs_track_policy_head_transitions_validate_insert
BEFORE INSERT ON jobs_track_policy_head_transitions
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_head_transition_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_head_transitions_no_update
  ON jobs_track_policy_head_transitions;
CREATE TRIGGER trg_jobs_track_policy_head_transitions_no_update
BEFORE UPDATE ON jobs_track_policy_head_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_head_transitions_no_delete
  ON jobs_track_policy_head_transitions;
CREATE TRIGGER trg_jobs_track_policy_head_transitions_no_delete
BEFORE DELETE ON jobs_track_policy_head_transitions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();

CREATE TRIGGER trg_jobs_track_policy_heads_validate_insert
BEFORE INSERT ON jobs_track_policy_heads
FOR EACH ROW EXECUTE FUNCTION validate_jobs_track_policy_head_insert();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_heads_monotonic
  ON jobs_track_policy_heads;
CREATE TRIGGER trg_jobs_track_policy_heads_monotonic
BEFORE UPDATE ON jobs_track_policy_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_track_policy_head_monotonic();
DROP TRIGGER IF EXISTS trg_jobs_track_policy_heads_no_delete
  ON jobs_track_policy_heads;
CREATE TRIGGER trg_jobs_track_policy_heads_no_delete
BEFORE DELETE ON jobs_track_policy_heads
FOR EACH ROW EXECUTE FUNCTION reject_jobs_track_policy_immutable_mutation();

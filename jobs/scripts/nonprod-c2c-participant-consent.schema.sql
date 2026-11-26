-- Detached SQLite candidate. Do not register this fixture as a migration.
-- A committed append is simultaneously its immutable revision and transition.
-- The head view derives only the last committed revision of the exact subject.
CREATE TABLE jobs_c2c_fixture_participants (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    participant_id TEXT NOT NULL CHECK(length(participant_id) BETWEEN 1 AND 100
        AND participant_id NOT GLOB '*[^a-z0-9_-]*'
        AND substr(participant_id,1,1) GLOB '[a-z0-9]'),
    PRIMARY KEY(account_id, participant_id)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX fixture_c2c_source_revision_canonical
    ON jobs_c2c_source_rights_revisions(account_id,source_subject_id,revision_id,canonical_sha256);

CREATE TRIGGER fixture_participant_no_update
BEFORE UPDATE ON jobs_c2c_fixture_participants
BEGIN SELECT RAISE(ABORT,'consent_immutable_subject'); END;

CREATE TRIGGER fixture_participant_cleanup_only
BEFORE DELETE ON jobs_c2c_fixture_participants
WHEN EXISTS (SELECT 1 FROM accounts WHERE id=OLD.account_id)
  OR NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens t
     WHERE t.account_id=OLD.account_id
  )
BEGIN SELECT RAISE(ABORT,'consent_immutable_subject'); END;

CREATE TRIGGER fixture_participant_no_replace
BEFORE INSERT ON jobs_c2c_fixture_participants
WHEN EXISTS (SELECT 1 FROM jobs_c2c_fixture_participants p
              WHERE p.account_id=NEW.account_id AND p.participant_id=NEW.participant_id)
  OR EXISTS (SELECT 1 FROM account_deletion_intents i WHERE i.account_id=NEW.account_id)
BEGIN SELECT RAISE(ABORT,'consent_immutable_subject'); END;

CREATE TABLE jobs_c2c_fixture_participant_consent (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    participant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK(purpose = 'c2c_plan_only'),
    revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9007199254740991),
    predecessor_revision INTEGER,
    predecessor_sha256 TEXT CHECK(predecessor_sha256 IS NULL OR (
        length(predecessor_sha256)=64 AND predecessor_sha256 NOT GLOB '*[^0-9a-f]*')),
    transition_sha256 TEXT NOT NULL CHECK(
        length(transition_sha256) = 64 AND transition_sha256 NOT GLOB '*[^0-9a-f]*'),
    source_revision_id TEXT NOT NULL CHECK(
        length(source_revision_id)=64 AND source_revision_id NOT GLOB '*[^0-9a-f]*'),
    source_canonical_sha256 TEXT NOT NULL CHECK(
        length(source_canonical_sha256) = 64 AND source_canonical_sha256 NOT GLOB '*[^0-9a-f]*'),
    source_transition_sha256 TEXT NOT NULL CHECK(
        length(source_transition_sha256) = 64 AND source_transition_sha256 NOT GLOB '*[^0-9a-f]*'),
    track_transition_id TEXT NOT NULL CHECK(length(track_transition_id) BETWEEN 1 AND 100
        AND track_transition_id NOT GLOB '*[^a-z0-9_-]*'
        AND substr(track_transition_id,1,1) GLOB '[a-z0-9]'),
    track_input_sha256 TEXT NOT NULL CHECK(
        length(track_input_sha256) = 64 AND track_input_sha256 NOT GLOB '*[^0-9a-f]*'),
    evidence_sha256 TEXT NOT NULL CHECK(
        length(evidence_sha256) = 64 AND evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
    request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 100
        AND request_id NOT GLOB '*[^a-z0-9_-]*' AND substr(request_id,1,1) GLOB '[a-z0-9]'),
    request_sha256 TEXT NOT NULL CHECK(
        length(request_sha256) = 64 AND request_sha256 NOT GLOB '*[^0-9a-f]*'),
    state TEXT NOT NULL CHECK(state IN ('active','revoked')),
    issuer TEXT NOT NULL CHECK(issuer = 'synthetic-fixture'),
    recorded_ms INTEGER NOT NULL CHECK(recorded_ms >= 0),
    expires_ms INTEGER NOT NULL CHECK(
        (state = 'active' AND expires_ms > recorded_ms) OR
        (state = 'revoked' AND expires_ms = recorded_ms)),
    PRIMARY KEY(account_id,track_id,participant_id,source_id,purpose,revision),
    UNIQUE(account_id,track_id,participant_id,source_id,purpose,revision,transition_sha256),
    UNIQUE(account_id,request_id),
    UNIQUE(account_id,transition_sha256),
    FOREIGN KEY(account_id,participant_id)
        REFERENCES jobs_c2c_fixture_participants(account_id,participant_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY(account_id,source_id,source_revision_id,source_canonical_sha256)
        REFERENCES jobs_c2c_source_rights_revisions(
            account_id,source_subject_id,revision_id,canonical_sha256)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY(account_id,track_id,participant_id,source_id,purpose,
                predecessor_revision,predecessor_sha256)
        REFERENCES jobs_c2c_fixture_participant_consent(
            account_id,track_id,participant_id,source_id,purpose,revision,transition_sha256)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK(
        (revision = 1 AND predecessor_revision IS NULL AND predecessor_sha256 IS NULL
                      AND state = 'active') OR
        (revision > 1 AND predecessor_revision IS NOT NULL AND predecessor_sha256 IS NOT NULL
                      AND predecessor_revision = revision - 1))
) STRICT, WITHOUT ROWID;

CREATE VIEW jobs_c2c_fixture_participant_consent_heads AS
SELECT g.* FROM jobs_c2c_fixture_participant_consent g
 WHERE NOT EXISTS (
    SELECT 1 FROM jobs_c2c_fixture_participant_consent next
     WHERE next.account_id=g.account_id AND next.track_id=g.track_id
       AND next.participant_id=g.participant_id AND next.source_id=g.source_id
       AND next.purpose=g.purpose AND next.revision>g.revision
 );

-- Check all unique collision shapes before SQLite's REPLACE handling, even
-- when recursive triggers are off. Replays are reads, never SQL replacements.
CREATE TRIGGER fixture_participant_consent_validate_append
BEFORE INSERT ON jobs_c2c_fixture_participant_consent
BEGIN
    SELECT CASE WHEN EXISTS (
        SELECT 1 FROM account_deletion_intents i WHERE i.account_id=NEW.account_id
    ) THEN RAISE(ABORT,'consent_account_unavailable') END;
    SELECT CASE WHEN EXISTS (
        SELECT 1 FROM jobs_c2c_fixture_participant_consent r WHERE r.account_id=NEW.account_id
          AND (r.request_id=NEW.request_id OR r.transition_sha256=NEW.transition_sha256)
    ) THEN RAISE(ABORT,'consent_immutable_revision') END;
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM jobs_c2c_fixture_participants p
         WHERE p.account_id=NEW.account_id AND p.participant_id=NEW.participant_id
    ) THEN RAISE(ABORT,'consent_participant_unavailable') END;
    SELECT CASE WHEN NEW.state='active' AND EXISTS (
        SELECT 1 FROM jobs_c2c_fixture_participant_consent r
         WHERE r.account_id=NEW.account_id AND r.track_id=NEW.track_id
           AND r.participant_id=NEW.participant_id AND r.source_id=NEW.source_id
           AND r.purpose=NEW.purpose AND r.state='active' AND r.evidence_sha256=NEW.evidence_sha256
    ) THEN RAISE(ABORT,'consent_evidence_already_used') END;
    SELECT CASE WHEN NOT (
        (NEW.revision=1 AND NOT EXISTS (
          SELECT 1 FROM jobs_c2c_fixture_participant_consent_heads h
           WHERE h.account_id=NEW.account_id AND h.track_id=NEW.track_id
             AND h.participant_id=NEW.participant_id AND h.source_id=NEW.source_id
             AND h.purpose=NEW.purpose
        )) OR (NEW.revision>1 AND EXISTS (
          SELECT 1 FROM jobs_c2c_fixture_participant_consent_heads h
           WHERE h.account_id=NEW.account_id AND h.track_id=NEW.track_id
             AND h.participant_id=NEW.participant_id AND h.source_id=NEW.source_id
             AND h.purpose=NEW.purpose AND h.revision=NEW.predecessor_revision
             AND h.transition_sha256=NEW.predecessor_sha256 AND h.state='active'
             AND h.recorded_ms<=NEW.recorded_ms
             AND (NEW.state='active' OR (
                h.source_revision_id=NEW.source_revision_id
                AND h.source_canonical_sha256=NEW.source_canonical_sha256
                AND h.source_transition_sha256=NEW.source_transition_sha256
                AND h.track_transition_id=NEW.track_transition_id
                AND h.track_input_sha256=NEW.track_input_sha256
             ))
        ))
    ) THEN RAISE(ABORT,'consent_stale_predecessor') END;
    SELECT CASE WHEN NEW.state='active' AND NOT EXISTS (
        SELECT 1 FROM jobs_c2c_source_rights_heads h
         WHERE h.account_id=NEW.account_id AND h.source_subject_id=NEW.source_id
           AND h.revision_id=NEW.source_revision_id
           AND h.canonical_sha256=NEW.source_canonical_sha256
           AND h.transition_sha256=NEW.source_transition_sha256
    ) THEN RAISE(ABORT,'consent_source_unavailable') END;
END;

CREATE TRIGGER fixture_participant_consent_no_update
BEFORE UPDATE ON jobs_c2c_fixture_participant_consent
BEGIN SELECT RAISE(ABORT,'consent_immutable_revision'); END;

CREATE TRIGGER fixture_participant_consent_cleanup_only
BEFORE DELETE ON jobs_c2c_fixture_participant_consent
WHEN EXISTS (SELECT 1 FROM accounts a WHERE a.id=OLD.account_id)
  OR NOT EXISTS (
    SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens t
     WHERE t.account_id=OLD.account_id
  )
BEGIN SELECT RAISE(ABORT,'consent_immutable_revision'); END;

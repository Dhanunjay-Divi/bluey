-- Target: SQLite
-- Durable, revisioned Jobs operational holds. History is append-only and
-- heads advance only through an exact compare-and-swap transition.

CREATE TABLE IF NOT EXISTS jobs_operational_hold_events (
  event_id                       TEXT PRIMARY KEY
    CHECK(length(event_id) BETWEEN 1 AND 128),
  event_ref                      TEXT NOT NULL UNIQUE
    CHECK(length(event_ref) = 70 AND substr(event_ref, 1, 6) = 'event-'
      AND lower(event_ref) = event_ref),
  event_sha256                   TEXT NOT NULL UNIQUE
    CHECK(length(event_sha256) = 64 AND lower(event_sha256) = event_sha256),
  canonical_event_base64url      TEXT NOT NULL
    CHECK(length(canonical_event_base64url) BETWEEN 1 AND 16384),
  capability                     TEXT NOT NULL CHECK(capability IN (
    'all', 'discovery', 'generation', 'application_queue', 'runner_claim',
    'final_submit', 'mailbox_sync', 'communication_dispatch'
  )),
  scope_kind                     TEXT NOT NULL CHECK(scope_kind IN (
    'global', 'discovery_source', 'ats_provider', 'ats_adapter', 'employer_domain',
    'account', 'career_track', 'region', 'runner_kind', 'mailbox_provider',
    'model_provider', 'model'
  )),
  scope_id                       TEXT NOT NULL CHECK(length(scope_id) BETWEEN 1 AND 256),
  revision_no                    INTEGER NOT NULL
    CHECK(revision_no BETWEEN 1 AND 9007199254740991),
  previous_revision_no           INTEGER
    CHECK(previous_revision_no IS NULL
      OR previous_revision_no BETWEEN 1 AND 9007199254740991),
  predecessor_event_id           TEXT
    CHECK(predecessor_event_id IS NULL
      OR length(predecessor_event_id) BETWEEN 1 AND 128),
  transition                     TEXT NOT NULL CHECK(transition IN ('held', 'released')),
  reason_code                    TEXT NOT NULL CHECK(reason_code IN (
    'incident', 'security_review', 'privacy_review', 'compliance_review',
    'quality_regression', 'provider_outage', 'capacity_guard', 'maintenance',
    'account_request', 'certification_guard', 'rollout_guard', 'manual_release'
  )),
  reason_ref                     TEXT
    CHECK(reason_ref IS NULL OR length(reason_ref) BETWEEN 1 AND 192),
  recorded_by                    TEXT NOT NULL CHECK(length(recorded_by) BETWEEN 1 AND 128),
  recorded_at_ms                 INTEGER NOT NULL
    CHECK(recorded_at_ms BETWEEN 0 AND 9007199254740991),
  CHECK((scope_kind = 'global' AND scope_id = '*')
    OR (scope_kind <> 'global' AND scope_id <> '*')),
  UNIQUE(capability, scope_kind, scope_id, revision_no),
  UNIQUE(capability, scope_kind, scope_id, revision_no, event_id),
  UNIQUE(capability, scope_kind, scope_id, revision_no, event_id, event_ref),
  FOREIGN KEY(
    capability, scope_kind, scope_id, previous_revision_no, predecessor_event_id
  ) REFERENCES jobs_operational_hold_events(
    capability, scope_kind, scope_id, revision_no, event_id
  ) ON DELETE RESTRICT,
  CHECK(
    (revision_no = 1 AND previous_revision_no IS NULL
      AND predecessor_event_id IS NULL AND transition = 'held')
    OR (revision_no > 1 AND previous_revision_no = revision_no - 1
      AND predecessor_event_id IS NOT NULL)
  )
);

CREATE TABLE IF NOT EXISTS jobs_operational_hold_heads (
  capability                     TEXT NOT NULL CHECK(capability IN (
    'all', 'discovery', 'generation', 'application_queue', 'runner_claim',
    'final_submit', 'mailbox_sync', 'communication_dispatch'
  )),
  scope_kind                     TEXT NOT NULL CHECK(scope_kind IN (
    'global', 'discovery_source', 'ats_provider', 'ats_adapter', 'employer_domain',
    'account', 'career_track', 'region', 'runner_kind', 'mailbox_provider',
    'model_provider', 'model'
  )),
  scope_id                       TEXT NOT NULL CHECK(length(scope_id) BETWEEN 1 AND 256),
  scope_ref                      TEXT NOT NULL
    CHECK(length(scope_ref) = 70 AND substr(scope_ref, 1, 6) = 'scope-'
      AND lower(scope_ref) = scope_ref),
  head_revision                  INTEGER NOT NULL
    CHECK(head_revision BETWEEN 1 AND 9007199254740991),
  current_event_id               TEXT NOT NULL UNIQUE
    CHECK(length(current_event_id) BETWEEN 1 AND 128),
  current_event_ref              TEXT NOT NULL UNIQUE
    CHECK(length(current_event_ref) = 70 AND substr(current_event_ref, 1, 6) = 'event-'
      AND lower(current_event_ref) = current_event_ref),
  state                          TEXT NOT NULL CHECK(state IN ('held', 'released')),
  updated_by                     TEXT NOT NULL CHECK(length(updated_by) BETWEEN 1 AND 128),
  updated_at_ms                  INTEGER NOT NULL
    CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
  CHECK((scope_kind = 'global' AND scope_id = '*')
    OR (scope_kind <> 'global' AND scope_id <> '*')),
  PRIMARY KEY(capability, scope_kind, scope_id),
  FOREIGN KEY(
    capability, scope_kind, scope_id, head_revision, current_event_id, current_event_ref
  )
    REFERENCES jobs_operational_hold_events(
      capability, scope_kind, scope_id, revision_no, event_id, event_ref
    ) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_jobs_operational_hold_events_history
  ON jobs_operational_hold_events(
    capability, scope_kind, scope_id, revision_no DESC
  );
CREATE INDEX IF NOT EXISTS idx_jobs_operational_hold_heads_lookup
  ON jobs_operational_hold_heads(state, capability, scope_kind, scope_id);
CREATE INDEX IF NOT EXISTS idx_jobs_operational_hold_heads_refs
  ON jobs_operational_hold_heads(capability, scope_kind, scope_ref, head_revision);

CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_events_validate_insert
BEFORE INSERT ON jobs_operational_hold_events
WHEN NEW.event_ref GLOB 'event-*[^0-9a-f]*'
  OR NEW.event_sha256 GLOB '*[^0-9a-f]*'
  OR NEW.canonical_event_base64url GLOB '*[^A-Za-z0-9_-]*'
  OR (NEW.reason_ref IS NOT NULL AND (
    substr(NEW.reason_ref, 1, 1) NOT GLOB '[A-Za-z0-9]'
    OR NEW.reason_ref GLOB '*[^A-Za-z0-9._:/@+-]*'
  ))
  OR (NEW.revision_no > 1 AND NOT EXISTS (
    SELECT 1
      FROM jobs_operational_hold_events AS predecessor
     WHERE predecessor.capability = NEW.capability
       AND predecessor.scope_kind = NEW.scope_kind
       AND predecessor.scope_id = NEW.scope_id
       AND predecessor.revision_no = NEW.previous_revision_no
       AND predecessor.event_id = NEW.predecessor_event_id
       AND predecessor.recorded_at_ms <= NEW.recorded_at_ms
       AND NOT (NEW.transition = 'released' AND predecessor.transition = 'released')
  ))
BEGIN
  SELECT RAISE(ABORT, 'invalid operational hold event');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_events_no_update
BEFORE UPDATE ON jobs_operational_hold_events BEGIN
  SELECT RAISE(ABORT, 'operational hold event is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_events_no_delete
BEFORE DELETE ON jobs_operational_hold_events BEGIN
  SELECT RAISE(ABORT, 'operational hold event is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_heads_validate_insert
BEFORE INSERT ON jobs_operational_hold_heads
WHEN NEW.scope_ref GLOB 'scope-*[^0-9a-f]*'
  OR NEW.current_event_ref GLOB 'event-*[^0-9a-f]*'
  OR NEW.head_revision <> 1 OR NOT EXISTS (
  SELECT 1
    FROM jobs_operational_hold_events AS event
   WHERE event.capability = NEW.capability
     AND event.scope_kind = NEW.scope_kind
     AND event.scope_id = NEW.scope_id
     AND event.revision_no = NEW.head_revision
     AND event.event_id = NEW.current_event_id
     AND event.event_ref = NEW.current_event_ref
     AND event.transition = NEW.state
     AND event.recorded_by = NEW.updated_by
     AND event.recorded_at_ms = NEW.updated_at_ms
)
BEGIN
  SELECT RAISE(ABORT, 'invalid operational hold head');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_heads_monotonic
BEFORE UPDATE ON jobs_operational_hold_heads
WHEN NEW.capability <> OLD.capability
  OR NEW.scope_kind <> OLD.scope_kind
  OR NEW.scope_id <> OLD.scope_id
  OR NEW.scope_ref <> OLD.scope_ref
  OR NEW.head_revision <> OLD.head_revision + 1
  OR NEW.current_event_id = OLD.current_event_id
  OR NEW.current_event_ref = OLD.current_event_ref
  OR NEW.updated_at_ms < OLD.updated_at_ms
  OR NOT EXISTS (
    SELECT 1
      FROM jobs_operational_hold_events AS event
     WHERE event.capability = NEW.capability
       AND event.scope_kind = NEW.scope_kind
       AND event.scope_id = NEW.scope_id
       AND event.revision_no = NEW.head_revision
       AND event.previous_revision_no = OLD.head_revision
       AND event.predecessor_event_id = OLD.current_event_id
       AND event.event_id = NEW.current_event_id
       AND event.event_ref = NEW.current_event_ref
       AND event.transition = NEW.state
       AND event.recorded_by = NEW.updated_by
       AND event.recorded_at_ms = NEW.updated_at_ms
  )
BEGIN
  SELECT RAISE(ABORT, 'operational hold head must advance exactly one revision');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_heads_no_delete
BEFORE DELETE ON jobs_operational_hold_heads BEGIN
  SELECT RAISE(ABORT, 'operational hold head cannot be deleted');
END;

-- Target: PostgreSQL
-- Exact reviewed communication authority and append-only provider evidence.
ALTER TABLE jobs_communication_actions
  ADD COLUMN IF NOT EXISTS authority_sha256 TEXT NOT NULL DEFAULT '',
  ADD COLUMN IF NOT EXISTS lease_kind TEXT,
  ADD COLUMN IF NOT EXISTS active_attempt_id TEXT,
  ADD COLUMN IF NOT EXISTS reconciliation_count BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS action_revision BIGINT NOT NULL DEFAULT 1,
  ADD COLUMN IF NOT EXISTS approval_revision BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS approved_authority_sha256 TEXT NOT NULL DEFAULT '',
  ADD COLUMN IF NOT EXISTS approved_grant_revision BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS approved_grant_sha256 TEXT NOT NULL DEFAULT '';

ALTER TABLE jobs_communication_actions
  DROP CONSTRAINT IF EXISTS jobs_communication_actions_action_revision_check;
ALTER TABLE jobs_communication_actions
  ADD CONSTRAINT jobs_communication_actions_action_revision_check
  CHECK (action_revision > 0 AND action_revision <= 9007199254740991);

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_communication_action_authority
  ON jobs_communication_actions(id, account_id, connection_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_communication_action_attempt_authority
  ON jobs_communication_actions(id, account_id, connection_id, provider);

CREATE TABLE IF NOT EXISTS jobs_communication_write_fences (
  account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  connection_id   TEXT NOT NULL,
  reason           TEXT NOT NULL CHECK (reason IN ('account_deletion', 'mailbox_disconnect')),
  created_at_ms    BIGINT NOT NULL CHECK (created_at_ms >= 0),
  PRIMARY KEY(account_id, connection_id),
  CHECK (
    (reason = 'account_deletion' AND connection_id = '')
    OR (reason = 'mailbox_disconnect' AND connection_id <> '')
  )
);

CREATE TABLE IF NOT EXISTS jobs_communication_action_attempts (
  id                       TEXT PRIMARY KEY,
  account_id               TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  action_id                TEXT NOT NULL REFERENCES jobs_communication_actions(id) ON DELETE CASCADE,
  connection_id            TEXT NOT NULL REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
  provider                 TEXT NOT NULL CHECK (
    provider IN ('gmail', 'outlook_email', 'google_calendar', 'outlook_calendar')
  ),
  dispatch_no              BIGINT NOT NULL CHECK (dispatch_no > 0),
  fence                    BIGINT NOT NULL CHECK (fence > 0),
  approval_revision        BIGINT NOT NULL CHECK (approval_revision > 0),
  authority_sha256         TEXT NOT NULL CHECK (length(authority_sha256) = 64),
  grant_revision           BIGINT NOT NULL CHECK (grant_revision > 0),
  grant_sha256             TEXT NOT NULL CHECK (length(grant_sha256) = 64),
  provider_operation_key   TEXT NOT NULL CHECK (length(provider_operation_key) BETWEEN 16 AND 160),
  created_at_ms            BIGINT NOT NULL CHECK (created_at_ms >= 0),
  FOREIGN KEY(action_id, account_id, connection_id, provider)
    REFERENCES jobs_communication_actions(id, account_id, connection_id, provider)
    ON DELETE CASCADE,
  UNIQUE(action_id, dispatch_no),
  UNIQUE(action_id, fence),
  UNIQUE(id, account_id, action_id),
  UNIQUE(connection_id, provider, provider_operation_key)
);

CREATE TABLE IF NOT EXISTS jobs_communication_action_attempt_evidence (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  action_id           TEXT NOT NULL REFERENCES jobs_communication_actions(id) ON DELETE CASCADE,
  attempt_id          TEXT NOT NULL REFERENCES jobs_communication_action_attempts(id) ON DELETE CASCADE,
  event_kind          TEXT NOT NULL CHECK (
    event_kind IN (
      'request_started', 'sent', 'calendar_created', 'failed',
      'needs_input', 'side_effect_unknown'
    )
  ),
  provider_object_id  TEXT,
  evidence_sha256     TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
  evidence_json       TEXT NOT NULL,
  recorded_at_ms      BIGINT NOT NULL CHECK (recorded_at_ms >= 0),
  FOREIGN KEY(attempt_id, account_id, action_id)
    REFERENCES jobs_communication_action_attempts(id, account_id, action_id) ON DELETE CASCADE,
  UNIQUE(attempt_id, event_kind)
);

CREATE TABLE IF NOT EXISTS jobs_communication_action_reconciliations (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  action_id           TEXT NOT NULL REFERENCES jobs_communication_actions(id) ON DELETE CASCADE,
  attempt_id          TEXT NOT NULL REFERENCES jobs_communication_action_attempts(id) ON DELETE CASCADE,
  fence               BIGINT NOT NULL CHECK (fence > 0),
  resolution          TEXT NOT NULL CHECK (
    resolution IN ('confirmed_sent', 'confirmed_calendar', 'confirmed_absent', 'inconclusive')
  ),
  provider_object_id  TEXT,
  evidence_sha256     TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
  evidence_json       TEXT NOT NULL,
  recorded_at_ms      BIGINT NOT NULL CHECK (recorded_at_ms >= 0),
  FOREIGN KEY(attempt_id, account_id, action_id)
    REFERENCES jobs_communication_action_attempts(id, account_id, action_id) ON DELETE CASCADE,
  UNIQUE(action_id, fence)
);

CREATE INDEX IF NOT EXISTS idx_jobs_communication_attempts_action
  ON jobs_communication_action_attempts(account_id, action_id, dispatch_no DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_communication_attempt_evidence_action
  ON jobs_communication_action_attempt_evidence(account_id, action_id, recorded_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_communication_reconciliations_action
  ON jobs_communication_action_reconciliations(account_id, action_id, recorded_at_ms DESC);
DROP INDEX IF EXISTS idx_jobs_communication_provider_object;
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_communication_provider_object
  ON jobs_communication_actions(account_id, connection_id, provider, provider_object_id)
  WHERE provider_object_id IS NOT NULL AND provider_object_id <> '';

CREATE OR REPLACE FUNCTION validate_jobs_communication_action_authority()
RETURNS trigger AS $$
BEGIN
  IF NOT (
    (NEW.kind = 'reply' AND NEW.provider IN ('gmail', 'outlook_email'))
    OR (NEW.kind = 'calendar'
        AND NEW.provider IN ('google_calendar', 'outlook_calendar'))
  ) OR NOT EXISTS (
    SELECT 1 FROM jobs_applications
     WHERE id = NEW.application_id AND account_id = NEW.account_id
  ) OR NOT EXISTS (
    SELECT 1 FROM jobs_mailbox_connections
     WHERE id = NEW.connection_id AND account_id = NEW.account_id
       AND provider = CASE
         WHEN NEW.provider IN ('gmail', 'google_calendar') THEN 'gmail'
         WHEN NEW.provider IN ('outlook_email', 'outlook_calendar') THEN 'outlook'
         ELSE ''
       END
  ) OR (
    NEW.kind = 'reply' AND (
      NEW.source_message_id IS NULL OR NOT EXISTS (
        SELECT 1 FROM jobs_provider_messages
         WHERE id = NEW.source_message_id AND account_id = NEW.account_id
           AND connection_id = NEW.connection_id
           AND application_id = NEW.application_id
           AND provider = CASE
             WHEN NEW.provider = 'gmail' THEN 'gmail'
             WHEN NEW.provider = 'outlook_email' THEN 'outlook'
             ELSE ''
           END
      )
    )
  ) OR (NEW.kind = 'calendar' AND NEW.source_message_id IS NOT NULL) THEN
    RAISE EXCEPTION 'invalid communication action authority';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_communication_action_authority ON jobs_communication_actions;
CREATE TRIGGER trg_jobs_communication_action_authority
BEFORE INSERT OR UPDATE OF account_id, application_id, connection_id, source_message_id, kind,
  provider ON jobs_communication_actions
FOR EACH ROW EXECUTE FUNCTION validate_jobs_communication_action_authority();

CREATE OR REPLACE FUNCTION validate_jobs_communication_attempt_authority()
RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM jobs_communication_actions
     WHERE id = NEW.action_id AND account_id = NEW.account_id
       AND connection_id = NEW.connection_id AND provider = NEW.provider
  ) THEN
    RAISE EXCEPTION 'invalid communication attempt authority';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_communication_attempt_authority
  ON jobs_communication_action_attempts;
CREATE TRIGGER trg_jobs_communication_attempt_authority
BEFORE INSERT OR UPDATE OF account_id, action_id, connection_id, provider
ON jobs_communication_action_attempts
FOR EACH ROW EXECUTE FUNCTION validate_jobs_communication_attempt_authority();

CREATE OR REPLACE FUNCTION delete_jobs_communication_actions_for_parent()
RETURNS trigger AS $$
BEGIN
  IF TG_TABLE_NAME = 'jobs_provider_messages' THEN
    IF EXISTS (
      SELECT 1 FROM jobs_communication_actions AS action
       WHERE action.source_message_id = OLD.id
         AND (
           action.status NOT IN ('sent', 'calendar_created', 'cancelled')
           OR NOT EXISTS (
             SELECT 1 FROM jobs_communication_write_fences AS fence
              WHERE fence.account_id = action.account_id
                AND fence.connection_id IN ('', action.connection_id)
           )
         )
    ) THEN
      RAISE EXCEPTION 'communication parent purge is not authorized';
    END IF;
    DELETE FROM jobs_communication_actions
     WHERE source_message_id = OLD.id
       AND status NOT IN ('dispatching', 'side_effect_unknown');
  ELSIF TG_TABLE_NAME = 'jobs_applications' THEN
    IF EXISTS (
      SELECT 1 FROM jobs_communication_actions AS action
       WHERE action.account_id = OLD.account_id AND action.application_id = OLD.id
         AND (
           action.status NOT IN ('sent', 'calendar_created', 'cancelled')
           OR NOT EXISTS (
             SELECT 1 FROM jobs_communication_write_fences AS fence
              WHERE fence.account_id = action.account_id
                AND fence.connection_id IN ('', action.connection_id)
           )
         )
    ) THEN
      RAISE EXCEPTION 'communication parent purge is not authorized';
    END IF;
    DELETE FROM jobs_communication_actions
     WHERE account_id = OLD.account_id AND application_id = OLD.id
       AND status NOT IN ('dispatching', 'side_effect_unknown');
  ELSIF TG_TABLE_NAME = 'jobs_mailbox_connections' THEN
    IF EXISTS (
      SELECT 1 FROM jobs_communication_actions AS action
       WHERE action.account_id = OLD.account_id AND action.connection_id = OLD.id
         AND (
           action.status NOT IN ('sent', 'calendar_created', 'cancelled')
           OR NOT EXISTS (
             SELECT 1 FROM jobs_communication_write_fences AS fence
              WHERE fence.account_id = action.account_id
                AND fence.connection_id IN ('', action.connection_id)
           )
         )
    ) THEN
      RAISE EXCEPTION 'communication parent purge is not authorized';
    END IF;
    DELETE FROM jobs_communication_actions
     WHERE account_id = OLD.account_id AND connection_id = OLD.id
       AND status NOT IN ('dispatching', 'side_effect_unknown');
  ELSIF TG_TABLE_NAME = 'accounts' THEN
    IF EXISTS (
      SELECT 1 FROM jobs_communication_actions WHERE account_id = OLD.id
    ) AND NOT EXISTS (
      SELECT 1 FROM account_deletion_intents WHERE account_id = OLD.id
    ) THEN
      RAISE EXCEPTION 'account communication deletion intent is required';
    END IF;
    IF EXISTS (
      SELECT 1 FROM jobs_communication_actions WHERE account_id = OLD.id
    ) AND NOT EXISTS (
      SELECT 1 FROM jobs_communication_write_fences
       WHERE account_id = OLD.id AND connection_id = '' AND reason = 'account_deletion'
    ) THEN
      RAISE EXCEPTION 'account communication deletion fence is required';
    END IF;
    IF EXISTS (
      SELECT 1 FROM jobs_communication_actions
       WHERE account_id = OLD.id
         AND status IN ('dispatching', 'side_effect_unknown')
    ) THEN
      RAISE EXCEPTION 'communication outcome is unresolved';
    END IF;
    DELETE FROM jobs_communication_actions WHERE account_id = OLD.id;
  END IF;
  RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_communication_source_delete ON jobs_provider_messages;
CREATE TRIGGER trg_jobs_communication_source_delete
BEFORE DELETE ON jobs_provider_messages
FOR EACH ROW EXECUTE FUNCTION delete_jobs_communication_actions_for_parent();
DROP TRIGGER IF EXISTS trg_jobs_communication_application_delete ON jobs_applications;
CREATE TRIGGER trg_jobs_communication_application_delete
BEFORE DELETE ON jobs_applications
FOR EACH ROW EXECUTE FUNCTION delete_jobs_communication_actions_for_parent();
DROP TRIGGER IF EXISTS trg_jobs_communication_mailbox_delete ON jobs_mailbox_connections;
CREATE TRIGGER trg_jobs_communication_mailbox_delete
BEFORE DELETE ON jobs_mailbox_connections
FOR EACH ROW EXECUTE FUNCTION delete_jobs_communication_actions_for_parent();
DROP TRIGGER IF EXISTS trg_jobs_communication_account_delete ON accounts;
CREATE TRIGGER trg_jobs_communication_account_delete
BEFORE DELETE ON accounts
FOR EACH ROW EXECUTE FUNCTION delete_jobs_communication_actions_for_parent();

DO $$
BEGIN
  IF EXISTS (
    SELECT 1
      FROM jobs_communication_actions AS action
      LEFT JOIN jobs_applications AS application
        ON application.id = action.application_id
       AND application.account_id = action.account_id
      LEFT JOIN jobs_mailbox_connections AS connection
        ON connection.id = action.connection_id
       AND connection.account_id = action.account_id
       AND connection.provider = CASE
         WHEN action.provider IN ('gmail', 'google_calendar') THEN 'gmail'
         WHEN action.provider IN ('outlook_email', 'outlook_calendar') THEN 'outlook'
         ELSE ''
       END
      LEFT JOIN jobs_provider_messages AS source
        ON source.id = action.source_message_id
       AND source.account_id = action.account_id
       AND source.connection_id = action.connection_id
       AND source.application_id = action.application_id
       AND source.provider = CASE
         WHEN action.provider = 'gmail' THEN 'gmail'
         WHEN action.provider = 'outlook_email' THEN 'outlook'
         ELSE ''
       END
     WHERE application.id IS NULL OR connection.id IS NULL
        OR NOT (
          (action.kind = 'reply' AND action.provider IN ('gmail', 'outlook_email'))
          OR (action.kind = 'calendar'
              AND action.provider IN ('google_calendar', 'outlook_calendar'))
        )
        OR (action.kind = 'reply' AND source.id IS NULL)
        OR (action.kind = 'calendar' AND action.source_message_id IS NOT NULL)
  ) THEN
    RAISE EXCEPTION 'existing communication action authority is invalid';
  END IF;
END $$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
     WHERE conname = 'chk_jobs_communication_provider_object'
       AND conrelid = 'jobs_communication_actions'::regclass
  ) THEN
    ALTER TABLE jobs_communication_actions
      ADD CONSTRAINT chk_jobs_communication_provider_object CHECK (
        (
          status IN ('sent', 'calendar_created')
          AND provider_object_id IS NOT NULL
          AND provider_object_id <> ''
          AND length(provider_object_id) <= 2048
          AND provider_object_id !~ '[[:cntrl:]]'
        ) OR (
          status NOT IN ('sent', 'calendar_created')
          AND (provider_object_id IS NULL OR provider_object_id = '')
        )
      );
  END IF;
END $$;

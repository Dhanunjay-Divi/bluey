-- Exact reviewed communication authority and append-only provider evidence.
-- Additive columns are installed with `ensure_column` after the replayable
-- SQLite migration batch; SQLite has no `ADD COLUMN IF NOT EXISTS` syntax.

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_communication_action_authority
  ON jobs_communication_actions(id, account_id, connection_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_communication_action_attempt_authority
  ON jobs_communication_actions(id, account_id, connection_id, provider);

CREATE TABLE IF NOT EXISTS jobs_communication_write_fences (
  account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  connection_id   TEXT NOT NULL,
  reason           TEXT NOT NULL CHECK (reason IN ('account_deletion', 'mailbox_disconnect')),
  created_at_ms    INTEGER NOT NULL CHECK (created_at_ms >= 0),
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
  dispatch_no              INTEGER NOT NULL CHECK (dispatch_no > 0),
  fence                    INTEGER NOT NULL CHECK (fence > 0),
  approval_revision        INTEGER NOT NULL CHECK (approval_revision > 0),
  authority_sha256         TEXT NOT NULL CHECK (length(authority_sha256) = 64),
  grant_revision           INTEGER NOT NULL CHECK (grant_revision > 0),
  grant_sha256             TEXT NOT NULL CHECK (length(grant_sha256) = 64),
  provider_operation_key   TEXT NOT NULL CHECK (length(provider_operation_key) BETWEEN 16 AND 160),
  created_at_ms            INTEGER NOT NULL CHECK (created_at_ms >= 0),
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
  recorded_at_ms      INTEGER NOT NULL CHECK (recorded_at_ms >= 0),
  FOREIGN KEY(attempt_id, account_id, action_id)
    REFERENCES jobs_communication_action_attempts(id, account_id, action_id) ON DELETE CASCADE,
  UNIQUE(attempt_id, event_kind)
);

CREATE TABLE IF NOT EXISTS jobs_communication_action_reconciliations (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  action_id           TEXT NOT NULL REFERENCES jobs_communication_actions(id) ON DELETE CASCADE,
  attempt_id          TEXT NOT NULL REFERENCES jobs_communication_action_attempts(id) ON DELETE CASCADE,
  fence               INTEGER NOT NULL CHECK (fence > 0),
  resolution          TEXT NOT NULL CHECK (
    resolution IN ('confirmed_sent', 'confirmed_calendar', 'confirmed_absent', 'inconclusive')
  ),
  provider_object_id  TEXT,
  evidence_sha256     TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
  evidence_json       TEXT NOT NULL,
  recorded_at_ms      INTEGER NOT NULL CHECK (recorded_at_ms >= 0),
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

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_action_authority_insert
BEFORE INSERT ON jobs_communication_actions
WHEN NOT (
  (NEW.kind = 'reply' AND NEW.provider IN ('gmail', 'outlook_email'))
  OR (NEW.kind = 'calendar' AND NEW.provider IN ('google_calendar', 'outlook_calendar'))
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
) OR (NEW.kind = 'calendar' AND NEW.source_message_id IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'invalid communication action authority');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_action_authority_update
BEFORE UPDATE OF account_id, application_id, connection_id, source_message_id, kind, provider
ON jobs_communication_actions
WHEN NOT (
  (NEW.kind = 'reply' AND NEW.provider IN ('gmail', 'outlook_email'))
  OR (NEW.kind = 'calendar' AND NEW.provider IN ('google_calendar', 'outlook_calendar'))
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
) OR (NEW.kind = 'calendar' AND NEW.source_message_id IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'invalid communication action authority');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_attempt_provider_insert
BEFORE INSERT ON jobs_communication_action_attempts
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_communication_actions
   WHERE id = NEW.action_id AND account_id = NEW.account_id
     AND connection_id = NEW.connection_id AND provider = NEW.provider
)
BEGIN
  SELECT RAISE(ABORT, 'invalid communication attempt authority');
END;

-- Parent cleanup is explicit so the legacy source-message SET NULL foreign key
-- cannot leave an unauthoritative reply row or block account/mailbox erasure.
CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_source_delete
BEFORE DELETE ON jobs_provider_messages
BEGIN
  SELECT CASE WHEN EXISTS (
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
  ) THEN RAISE(ABORT, 'communication parent purge is not authorized') END;
  DELETE FROM jobs_communication_actions
   WHERE source_message_id = OLD.id
     AND status NOT IN ('dispatching', 'side_effect_unknown');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_application_delete
BEFORE DELETE ON jobs_applications
BEGIN
  SELECT CASE WHEN EXISTS (
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
  ) THEN RAISE(ABORT, 'communication parent purge is not authorized') END;
  DELETE FROM jobs_communication_actions
   WHERE account_id = OLD.account_id AND application_id = OLD.id
     AND status NOT IN ('dispatching', 'side_effect_unknown');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_mailbox_delete
BEFORE DELETE ON jobs_mailbox_connections
BEGIN
  SELECT CASE WHEN EXISTS (
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
  ) THEN RAISE(ABORT, 'communication parent purge is not authorized') END;
  DELETE FROM jobs_communication_actions
   WHERE account_id = OLD.account_id AND connection_id = OLD.id
     AND status NOT IN ('dispatching', 'side_effect_unknown');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_account_delete
BEFORE DELETE ON accounts
BEGIN
  SELECT CASE WHEN EXISTS (
    SELECT 1 FROM jobs_communication_actions WHERE account_id = OLD.id
  ) AND NOT EXISTS (
    SELECT 1 FROM account_deletion_intents WHERE account_id = OLD.id
  ) THEN RAISE(ABORT, 'account communication deletion intent is required') END;
  SELECT CASE WHEN EXISTS (
    SELECT 1 FROM jobs_communication_actions WHERE account_id = OLD.id
  ) AND NOT EXISTS (
    SELECT 1 FROM jobs_communication_write_fences
     WHERE account_id = OLD.id AND connection_id = '' AND reason = 'account_deletion'
  ) THEN RAISE(ABORT, 'account communication deletion fence is required') END;
  SELECT CASE WHEN EXISTS (
    SELECT 1 FROM jobs_communication_actions
     WHERE account_id = OLD.id
       AND status IN ('dispatching', 'side_effect_unknown')
  ) THEN RAISE(ABORT, 'communication outcome is unresolved') END;
  DELETE FROM jobs_communication_actions WHERE account_id = OLD.id;
END;

CREATE TEMP TABLE jobs_communication_authority_preflight (
  valid INTEGER NOT NULL CHECK (valid = 1)
);
INSERT INTO jobs_communication_authority_preflight(valid)
SELECT 0
 WHERE EXISTS (
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
 );
DROP TABLE jobs_communication_authority_preflight;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_attempt_provider_update
BEFORE UPDATE OF account_id, action_id, connection_id, provider
ON jobs_communication_action_attempts
WHEN NOT EXISTS (
  SELECT 1 FROM jobs_communication_actions
   WHERE id = NEW.action_id AND account_id = NEW.account_id
     AND connection_id = NEW.connection_id AND provider = NEW.provider
)
BEGIN
  SELECT RAISE(ABORT, 'invalid communication attempt authority');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_provider_object_insert
BEFORE INSERT ON jobs_communication_actions
WHEN (
  NEW.status IN ('sent', 'calendar_created') AND (
    NEW.provider_object_id IS NULL OR NEW.provider_object_id = ''
    OR length(NEW.provider_object_id) > 2048
    OR instr(NEW.provider_object_id, char(0)) > 0
    OR instr(NEW.provider_object_id, char(10)) > 0
    OR instr(NEW.provider_object_id, char(13)) > 0
    OR instr(NEW.provider_object_id, char(1)) > 0
    OR instr(NEW.provider_object_id, char(2)) > 0
    OR instr(NEW.provider_object_id, char(3)) > 0
    OR instr(NEW.provider_object_id, char(4)) > 0
    OR instr(NEW.provider_object_id, char(5)) > 0
    OR instr(NEW.provider_object_id, char(6)) > 0
    OR instr(NEW.provider_object_id, char(7)) > 0
    OR instr(NEW.provider_object_id, char(8)) > 0
    OR instr(NEW.provider_object_id, char(9)) > 0
    OR instr(NEW.provider_object_id, char(11)) > 0
    OR instr(NEW.provider_object_id, char(12)) > 0
    OR instr(NEW.provider_object_id, char(14)) > 0
    OR instr(NEW.provider_object_id, char(15)) > 0
    OR instr(NEW.provider_object_id, char(16)) > 0
    OR instr(NEW.provider_object_id, char(17)) > 0
    OR instr(NEW.provider_object_id, char(18)) > 0
    OR instr(NEW.provider_object_id, char(19)) > 0
    OR instr(NEW.provider_object_id, char(20)) > 0
    OR instr(NEW.provider_object_id, char(21)) > 0
    OR instr(NEW.provider_object_id, char(22)) > 0
    OR instr(NEW.provider_object_id, char(23)) > 0
    OR instr(NEW.provider_object_id, char(24)) > 0
    OR instr(NEW.provider_object_id, char(25)) > 0
    OR instr(NEW.provider_object_id, char(26)) > 0
    OR instr(NEW.provider_object_id, char(27)) > 0
    OR instr(NEW.provider_object_id, char(28)) > 0
    OR instr(NEW.provider_object_id, char(29)) > 0
    OR instr(NEW.provider_object_id, char(30)) > 0
    OR instr(NEW.provider_object_id, char(31)) > 0
    OR instr(NEW.provider_object_id, char(127)) > 0
    OR instr(NEW.provider_object_id, char(128)) > 0
    OR instr(NEW.provider_object_id, char(129)) > 0
    OR instr(NEW.provider_object_id, char(130)) > 0
    OR instr(NEW.provider_object_id, char(131)) > 0
    OR instr(NEW.provider_object_id, char(132)) > 0
    OR instr(NEW.provider_object_id, char(133)) > 0
    OR instr(NEW.provider_object_id, char(134)) > 0
    OR instr(NEW.provider_object_id, char(135)) > 0
    OR instr(NEW.provider_object_id, char(136)) > 0
    OR instr(NEW.provider_object_id, char(137)) > 0
    OR instr(NEW.provider_object_id, char(138)) > 0
    OR instr(NEW.provider_object_id, char(139)) > 0
    OR instr(NEW.provider_object_id, char(140)) > 0
    OR instr(NEW.provider_object_id, char(141)) > 0
    OR instr(NEW.provider_object_id, char(142)) > 0
    OR instr(NEW.provider_object_id, char(143)) > 0
    OR instr(NEW.provider_object_id, char(144)) > 0
    OR instr(NEW.provider_object_id, char(145)) > 0
    OR instr(NEW.provider_object_id, char(146)) > 0
    OR instr(NEW.provider_object_id, char(147)) > 0
    OR instr(NEW.provider_object_id, char(148)) > 0
    OR instr(NEW.provider_object_id, char(149)) > 0
    OR instr(NEW.provider_object_id, char(150)) > 0
    OR instr(NEW.provider_object_id, char(151)) > 0
    OR instr(NEW.provider_object_id, char(152)) > 0
    OR instr(NEW.provider_object_id, char(153)) > 0
    OR instr(NEW.provider_object_id, char(154)) > 0
    OR instr(NEW.provider_object_id, char(155)) > 0
    OR instr(NEW.provider_object_id, char(156)) > 0
    OR instr(NEW.provider_object_id, char(157)) > 0
    OR instr(NEW.provider_object_id, char(158)) > 0
    OR instr(NEW.provider_object_id, char(159)) > 0
  )
) OR (
  NEW.status NOT IN ('sent', 'calendar_created')
  AND NEW.provider_object_id IS NOT NULL AND NEW.provider_object_id <> ''
)
BEGIN
  SELECT RAISE(ABORT, 'invalid communication provider object identity');
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_communication_provider_object_update
BEFORE UPDATE OF status, provider_object_id ON jobs_communication_actions
WHEN (
  NEW.status IN ('sent', 'calendar_created') AND (
    NEW.provider_object_id IS NULL OR NEW.provider_object_id = ''
    OR length(NEW.provider_object_id) > 2048
    OR instr(NEW.provider_object_id, char(0)) > 0
    OR instr(NEW.provider_object_id, char(10)) > 0
    OR instr(NEW.provider_object_id, char(13)) > 0
    OR instr(NEW.provider_object_id, char(1)) > 0
    OR instr(NEW.provider_object_id, char(2)) > 0
    OR instr(NEW.provider_object_id, char(3)) > 0
    OR instr(NEW.provider_object_id, char(4)) > 0
    OR instr(NEW.provider_object_id, char(5)) > 0
    OR instr(NEW.provider_object_id, char(6)) > 0
    OR instr(NEW.provider_object_id, char(7)) > 0
    OR instr(NEW.provider_object_id, char(8)) > 0
    OR instr(NEW.provider_object_id, char(9)) > 0
    OR instr(NEW.provider_object_id, char(11)) > 0
    OR instr(NEW.provider_object_id, char(12)) > 0
    OR instr(NEW.provider_object_id, char(14)) > 0
    OR instr(NEW.provider_object_id, char(15)) > 0
    OR instr(NEW.provider_object_id, char(16)) > 0
    OR instr(NEW.provider_object_id, char(17)) > 0
    OR instr(NEW.provider_object_id, char(18)) > 0
    OR instr(NEW.provider_object_id, char(19)) > 0
    OR instr(NEW.provider_object_id, char(20)) > 0
    OR instr(NEW.provider_object_id, char(21)) > 0
    OR instr(NEW.provider_object_id, char(22)) > 0
    OR instr(NEW.provider_object_id, char(23)) > 0
    OR instr(NEW.provider_object_id, char(24)) > 0
    OR instr(NEW.provider_object_id, char(25)) > 0
    OR instr(NEW.provider_object_id, char(26)) > 0
    OR instr(NEW.provider_object_id, char(27)) > 0
    OR instr(NEW.provider_object_id, char(28)) > 0
    OR instr(NEW.provider_object_id, char(29)) > 0
    OR instr(NEW.provider_object_id, char(30)) > 0
    OR instr(NEW.provider_object_id, char(31)) > 0
    OR instr(NEW.provider_object_id, char(127)) > 0
    OR instr(NEW.provider_object_id, char(128)) > 0
    OR instr(NEW.provider_object_id, char(129)) > 0
    OR instr(NEW.provider_object_id, char(130)) > 0
    OR instr(NEW.provider_object_id, char(131)) > 0
    OR instr(NEW.provider_object_id, char(132)) > 0
    OR instr(NEW.provider_object_id, char(133)) > 0
    OR instr(NEW.provider_object_id, char(134)) > 0
    OR instr(NEW.provider_object_id, char(135)) > 0
    OR instr(NEW.provider_object_id, char(136)) > 0
    OR instr(NEW.provider_object_id, char(137)) > 0
    OR instr(NEW.provider_object_id, char(138)) > 0
    OR instr(NEW.provider_object_id, char(139)) > 0
    OR instr(NEW.provider_object_id, char(140)) > 0
    OR instr(NEW.provider_object_id, char(141)) > 0
    OR instr(NEW.provider_object_id, char(142)) > 0
    OR instr(NEW.provider_object_id, char(143)) > 0
    OR instr(NEW.provider_object_id, char(144)) > 0
    OR instr(NEW.provider_object_id, char(145)) > 0
    OR instr(NEW.provider_object_id, char(146)) > 0
    OR instr(NEW.provider_object_id, char(147)) > 0
    OR instr(NEW.provider_object_id, char(148)) > 0
    OR instr(NEW.provider_object_id, char(149)) > 0
    OR instr(NEW.provider_object_id, char(150)) > 0
    OR instr(NEW.provider_object_id, char(151)) > 0
    OR instr(NEW.provider_object_id, char(152)) > 0
    OR instr(NEW.provider_object_id, char(153)) > 0
    OR instr(NEW.provider_object_id, char(154)) > 0
    OR instr(NEW.provider_object_id, char(155)) > 0
    OR instr(NEW.provider_object_id, char(156)) > 0
    OR instr(NEW.provider_object_id, char(157)) > 0
    OR instr(NEW.provider_object_id, char(158)) > 0
    OR instr(NEW.provider_object_id, char(159)) > 0
  )
) OR (
  NEW.status NOT IN ('sent', 'calendar_created')
  AND NEW.provider_object_id IS NOT NULL AND NEW.provider_object_id <> ''
)
BEGIN
  SELECT RAISE(ABORT, 'invalid communication provider object identity');
END;

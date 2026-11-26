-- Account-owned encrypted answer history. Parent-account deletion may cascade;
-- ordinary updates, deletions and REPLACE remain forbidden.
BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS jobs_application_answer_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    application_id TEXT NOT NULL,
    intervention_id TEXT NOT NULL,
    revision_json TEXT NOT NULL,
    revised_at_ms INTEGER NOT NULL CHECK(revised_at_ms >= 0),
    UNIQUE(account_id, intervention_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_application_answer_revisions_account_application
    ON jobs_application_answer_revisions(account_id, application_id);
CREATE TRIGGER IF NOT EXISTS trg_jobs_answer_revision_insert_guard
BEFORE INSERT ON jobs_application_answer_revisions
WHEN NOT EXISTS (SELECT 1 FROM jobs_applications a
  WHERE a.id = NEW.application_id AND a.account_id = NEW.account_id)
 OR EXISTS (SELECT 1 FROM jobs_application_answer_revisions r
  WHERE r.id = NEW.id OR (r.account_id = NEW.account_id AND r.intervention_id = NEW.intervention_id))
BEGIN SELECT RAISE(ABORT, 'answer revision insert refused'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_answer_revision_no_update
BEFORE UPDATE ON jobs_application_answer_revisions
BEGIN SELECT RAISE(ABORT, 'answer revision is immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_answer_revision_no_delete
BEFORE DELETE ON jobs_application_answer_revisions
WHEN EXISTS (SELECT 1 FROM accounts a WHERE a.id = OLD.account_id)
BEGIN SELECT RAISE(ABORT, 'answer revision is immutable'); END;
COMMIT;

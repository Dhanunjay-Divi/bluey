-- Target: PostgreSQL
CREATE TABLE IF NOT EXISTS jobs_application_answer_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    application_id TEXT NOT NULL,
    intervention_id TEXT NOT NULL,
    revision_json TEXT NOT NULL,
    revised_at_ms BIGINT NOT NULL CHECK(revised_at_ms >= 0),
    UNIQUE(account_id, intervention_id)
);
CREATE INDEX IF NOT EXISTS idx_jobs_application_answer_revisions_account_application
    ON jobs_application_answer_revisions(account_id, application_id);
CREATE OR REPLACE FUNCTION guard_jobs_answer_revision_insert() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM jobs_applications a
    WHERE a.id = NEW.application_id AND a.account_id = NEW.account_id) THEN
    RAISE EXCEPTION 'answer revision insert refused';
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_answer_revision_insert_guard ON jobs_application_answer_revisions;
CREATE TRIGGER trg_jobs_answer_revision_insert_guard BEFORE INSERT ON jobs_application_answer_revisions
FOR EACH ROW EXECUTE FUNCTION guard_jobs_answer_revision_insert();
CREATE OR REPLACE FUNCTION reject_jobs_answer_revision_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' AND NOT EXISTS (SELECT 1 FROM accounts a WHERE a.id = OLD.account_id) THEN
    RETURN OLD;
  END IF;
  RAISE EXCEPTION 'answer revision is immutable';
END;
$$;
DROP TRIGGER IF EXISTS trg_jobs_answer_revision_no_update ON jobs_application_answer_revisions;
CREATE TRIGGER trg_jobs_answer_revision_no_update BEFORE UPDATE ON jobs_application_answer_revisions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_answer_revision_mutation();
DROP TRIGGER IF EXISTS trg_jobs_answer_revision_no_delete ON jobs_application_answer_revisions;
CREATE TRIGGER trg_jobs_answer_revision_no_delete BEFORE DELETE ON jobs_application_answer_revisions
FOR EACH ROW EXECUTE FUNCTION reject_jobs_answer_revision_mutation();

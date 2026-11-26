-- Target: Postgres. Paired with SQLite067; caller owns the migration transaction.
-- Keep validation and trigger installation in the migration transaction. The
-- locks prevent a concurrent head write from racing the preflight snapshot.
SET LOCAL lock_timeout = '5s';
SET LOCAL statement_timeout = '30s';
LOCK TABLE jobs_ats_certification_trust_head, jobs_ats_certification_heads, jobs_ats_certification_quarantine_heads,
  jobs_ats_certification_circuit_heads IN SHARE ROW EXCLUSIVE MODE;
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM jobs_ats_certification_trust_head h
    WHERE NOT EXISTS (SELECT 1 FROM jobs_ats_certification_trust_policies p
      WHERE p.policy_sha256 = h.current_policy_sha256 AND p.trust_generation = h.current_trust_generation
        AND p.root_trust_anchor_sha256 = h.root_trust_anchor_sha256))
    OR EXISTS (SELECT 1 FROM jobs_ats_certification_heads h
    WHERE NOT EXISTS (SELECT 1 FROM jobs_ats_certification_head_transitions t
      JOIN jobs_ats_certification_activations a ON a.activation_sha256 = t.next_activation_sha256
      WHERE t.transition_sha256 = h.current_transition_sha256
        AND t.scope_sha256 = h.scope_sha256 AND t.channel = h.channel
        AND t.head_revision = h.head_revision AND t.next_activation_sha256 = h.current_activation_sha256
        AND t.next_channel_sequence = h.current_channel_sequence
        AND a.scope_sha256 = h.scope_sha256 AND a.channel = h.channel
        AND a.channel_sequence = h.current_channel_sequence))
    OR EXISTS (SELECT 1 FROM jobs_ats_certification_quarantine_heads h
      WHERE NOT EXISTS (SELECT 1 FROM jobs_ats_certification_quarantine_commands c
        WHERE c.command_sha256 = h.current_command_sha256 AND c.scope_kind = h.scope_kind
          AND c.scope_id = h.scope_id AND c.scope_sha256 = h.scope_sha256
          AND c.command_sequence = h.current_command_sequence
          AND h.state = CASE c.action WHEN 'quarantine' THEN 'quarantined' WHEN 'release' THEN 'released' END))
    OR EXISTS (SELECT 1 FROM jobs_ats_certification_circuit_heads h
      WHERE NOT EXISTS (SELECT 1 FROM jobs_ats_certification_circuit_events e
        WHERE e.event_id = h.current_event_id AND e.scope_kind = h.scope_kind
          AND e.subject_key = h.subject_key AND e.transition = h.state)) THEN
    RAISE EXCEPTION 'inconsistent ATS authority head';
  END IF;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_jobs_ats_head_scope_immutable()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.scope_sha256 IS DISTINCT FROM OLD.scope_sha256 OR NEW.channel IS DISTINCT FROM OLD.channel THEN
    RAISE EXCEPTION 'ATS certification head scope is immutable';
  END IF;
  RETURN NEW;
END;
$$;
CREATE OR REPLACE FUNCTION enforce_jobs_ats_quarantine_scope_immutable()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.scope_kind IS DISTINCT FROM OLD.scope_kind OR NEW.scope_id IS DISTINCT FROM OLD.scope_id
    OR NEW.scope_sha256 IS DISTINCT FROM OLD.scope_sha256 THEN
    RAISE EXCEPTION 'ATS quarantine head scope is immutable';
  END IF;
  RETURN NEW;
END;
$$;
CREATE OR REPLACE FUNCTION enforce_jobs_ats_circuit_scope_and_event()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' AND (NEW.scope_kind IS DISTINCT FROM OLD.scope_kind
    OR NEW.subject_key IS DISTINCT FROM OLD.subject_key) THEN
    RAISE EXCEPTION 'ATS circuit head scope is immutable';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM jobs_ats_certification_circuit_events e
    WHERE e.event_id = NEW.current_event_id AND e.scope_kind = NEW.scope_kind
      AND e.subject_key = NEW.subject_key AND e.transition = NEW.state) THEN
    RAISE EXCEPTION 'ATS circuit head event scope mismatch';
  END IF;
  RETURN NEW;
END;
$$;
CREATE OR REPLACE FUNCTION enforce_jobs_ats_trust_projection()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM jobs_ats_certification_trust_policies p
    WHERE p.policy_sha256 = NEW.current_policy_sha256 AND p.trust_generation = NEW.current_trust_generation
      AND p.root_trust_anchor_sha256 = NEW.root_trust_anchor_sha256) THEN
    RAISE EXCEPTION 'ATS trust head projection mismatch';
  END IF;
  RETURN NEW;
END;
$$;
CREATE OR REPLACE FUNCTION enforce_jobs_ats_head_projection()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM jobs_ats_certification_head_transitions t
    JOIN jobs_ats_certification_activations a ON a.activation_sha256 = t.next_activation_sha256
    WHERE t.transition_sha256 = NEW.current_transition_sha256 AND t.scope_sha256 = NEW.scope_sha256
      AND t.channel = NEW.channel AND t.head_revision = NEW.head_revision
      AND t.next_activation_sha256 = NEW.current_activation_sha256 AND t.next_channel_sequence = NEW.current_channel_sequence
      AND a.scope_sha256 = NEW.scope_sha256 AND a.channel = NEW.channel AND a.channel_sequence = NEW.current_channel_sequence) THEN
    RAISE EXCEPTION 'ATS certification head projection mismatch';
  END IF;
  RETURN NEW;
END;
$$;
CREATE OR REPLACE FUNCTION enforce_jobs_ats_quarantine_projection()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM jobs_ats_certification_quarantine_commands c
    WHERE c.command_sha256 = NEW.current_command_sha256 AND c.scope_kind = NEW.scope_kind
      AND c.scope_id = NEW.scope_id AND c.scope_sha256 = NEW.scope_sha256
      AND c.command_sequence = NEW.current_command_sequence
      AND NEW.state = CASE c.action WHEN 'quarantine' THEN 'quarantined' WHEN 'release' THEN 'released' END) THEN
    RAISE EXCEPTION 'ATS quarantine head projection mismatch';
  END IF;
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_jobs_ats_trust_projection ON jobs_ats_certification_trust_head;
CREATE TRIGGER trg_jobs_ats_trust_projection BEFORE INSERT OR UPDATE ON jobs_ats_certification_trust_head
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_trust_projection();
DROP TRIGGER IF EXISTS trg_jobs_ats_head_projection ON jobs_ats_certification_heads;
CREATE TRIGGER trg_jobs_ats_head_projection BEFORE INSERT OR UPDATE ON jobs_ats_certification_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_head_projection();
DROP TRIGGER IF EXISTS trg_jobs_ats_quarantine_projection ON jobs_ats_certification_quarantine_heads;
CREATE TRIGGER trg_jobs_ats_quarantine_projection BEFORE INSERT OR UPDATE ON jobs_ats_certification_quarantine_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_quarantine_projection();
DROP TRIGGER IF EXISTS trg_jobs_ats_head_scope_immutable ON jobs_ats_certification_heads;
CREATE TRIGGER trg_jobs_ats_head_scope_immutable BEFORE UPDATE ON jobs_ats_certification_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_head_scope_immutable();
DROP TRIGGER IF EXISTS trg_jobs_ats_quarantine_scope_immutable ON jobs_ats_certification_quarantine_heads;
CREATE TRIGGER trg_jobs_ats_quarantine_scope_immutable BEFORE UPDATE ON jobs_ats_certification_quarantine_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_quarantine_scope_immutable();
DROP TRIGGER IF EXISTS trg_jobs_ats_circuit_scope_and_event ON jobs_ats_certification_circuit_heads;
CREATE TRIGGER trg_jobs_ats_circuit_scope_and_event BEFORE INSERT OR UPDATE ON jobs_ats_certification_circuit_heads
FOR EACH ROW EXECUTE FUNCTION enforce_jobs_ats_circuit_scope_and_event();

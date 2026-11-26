-- The caller must roll back this transaction on any error. Do not repair an
-- inconsistent authority head: it requires explicit operator investigation.
BEGIN IMMEDIATE;
CREATE TEMP TABLE jobs_ats_storage_fence_preflight (ok INTEGER NOT NULL CHECK(ok = 1));
INSERT INTO jobs_ats_storage_fence_preflight(ok)
SELECT CASE WHEN
  EXISTS (SELECT 1 FROM pragma_foreign_key_check)
  OR EXISTS (SELECT 1 FROM jobs_ats_certification_trust_head h
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
        AND e.subject_key = h.subject_key AND e.transition = h.state))
THEN 0 ELSE 1 END;
DROP TABLE jobs_ats_storage_fence_preflight;

-- REPLACE suppresses DELETE triggers when recursive_triggers is OFF. Inspect
-- both unique keys before conflict resolution. Identical replays are no-ops;
-- any different row colliding with either key is rejected.
CREATE TRIGGER IF NOT EXISTS trg_jobs_browser_runtime_component_insert_fence
BEFORE INSERT ON jobs_browser_release_artifact_runtime_components
WHEN EXISTS (SELECT 1 FROM jobs_browser_release_artifact_runtime_components c
  WHERE (c.manifest_sha256 = NEW.manifest_sha256 AND c.artifact_id = NEW.artifact_id)
     OR (c.manifest_sha256 = NEW.manifest_sha256 AND c.platform = NEW.platform
         AND c.architecture = NEW.architecture AND c.package_kind = NEW.package_kind))
BEGIN
  SELECT CASE WHEN EXISTS (SELECT 1 FROM jobs_browser_release_artifact_runtime_components c
    WHERE c.manifest_sha256 = NEW.manifest_sha256 AND c.artifact_id = NEW.artifact_id
      AND c.build_descriptor_sha256 = NEW.build_descriptor_sha256 AND c.artifact_sha256 = NEW.artifact_sha256
      AND c.platform = NEW.platform AND c.architecture = NEW.architecture AND c.package_kind = NEW.package_kind
      AND c.automation_bundle_sha256 = NEW.automation_bundle_sha256
      AND c.chromium_executable_sha256 = NEW.chromium_executable_sha256 AND c.recorded_at_ms = NEW.recorded_at_ms)
    THEN RAISE(IGNORE) ELSE RAISE(ABORT, 'Browser runtime component conflict') END;
END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_trust_head_insert_fence
BEFORE INSERT ON jobs_ats_certification_trust_head
WHEN EXISTS (SELECT 1 FROM jobs_ats_certification_trust_head h
  WHERE h.singleton_id = NEW.singleton_id OR h.current_policy_sha256 = NEW.current_policy_sha256
    OR h.current_trust_generation = NEW.current_trust_generation)
BEGIN SELECT RAISE(ABORT, 'ATS trust head conflict'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_head_insert_fence
BEFORE INSERT ON jobs_ats_certification_heads
WHEN EXISTS (SELECT 1 FROM jobs_ats_certification_heads h
  WHERE h.scope_sha256 = NEW.scope_sha256 AND h.channel = NEW.channel)
BEGIN SELECT RAISE(ABORT, 'ATS certification head conflict'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_quarantine_head_insert_fence
BEFORE INSERT ON jobs_ats_certification_quarantine_heads
WHEN EXISTS (SELECT 1 FROM jobs_ats_certification_quarantine_heads h
  WHERE h.scope_kind = NEW.scope_kind AND h.scope_id = NEW.scope_id AND h.scope_sha256 = NEW.scope_sha256)
BEGIN SELECT RAISE(ABORT, 'ATS quarantine head conflict'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_circuit_head_insert_fence
BEFORE INSERT ON jobs_ats_certification_circuit_heads
WHEN EXISTS (SELECT 1 FROM jobs_ats_certification_circuit_heads h
  WHERE (h.scope_kind = NEW.scope_kind AND h.subject_key = NEW.subject_key)
     OR h.current_event_id = NEW.current_event_id)
BEGIN SELECT RAISE(ABORT, 'ATS circuit head conflict'); END;

CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_head_scope_immutable
BEFORE UPDATE ON jobs_ats_certification_heads
WHEN NEW.scope_sha256 IS NOT OLD.scope_sha256 OR NEW.channel IS NOT OLD.channel
BEGIN SELECT RAISE(ABORT, 'ATS certification head scope is immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_quarantine_scope_immutable
BEFORE UPDATE ON jobs_ats_certification_quarantine_heads
WHEN NEW.scope_kind IS NOT OLD.scope_kind OR NEW.scope_id IS NOT OLD.scope_id OR NEW.scope_sha256 IS NOT OLD.scope_sha256
BEGIN SELECT RAISE(ABORT, 'ATS quarantine head scope is immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_circuit_scope_immutable
BEFORE UPDATE ON jobs_ats_certification_circuit_heads
WHEN NEW.scope_kind IS NOT OLD.scope_kind OR NEW.subject_key IS NOT OLD.subject_key
BEGIN SELECT RAISE(ABORT, 'ATS circuit head scope is immutable'); END;

-- The same projection relationships checked by preflight must hold for every
-- subsequent write, otherwise a clean migration could fail on its next replay.
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_trust_projection_insert
BEFORE INSERT ON jobs_ats_certification_trust_head
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_trust_policies p
  WHERE p.policy_sha256 = NEW.current_policy_sha256 AND p.trust_generation = NEW.current_trust_generation
    AND p.root_trust_anchor_sha256 = NEW.root_trust_anchor_sha256)
BEGIN SELECT RAISE(ABORT, 'ATS trust head projection mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_trust_projection_update
BEFORE UPDATE ON jobs_ats_certification_trust_head
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_trust_policies p
  WHERE p.policy_sha256 = NEW.current_policy_sha256 AND p.trust_generation = NEW.current_trust_generation
    AND p.root_trust_anchor_sha256 = NEW.root_trust_anchor_sha256)
BEGIN SELECT RAISE(ABORT, 'ATS trust head projection mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_head_projection_insert
BEFORE INSERT ON jobs_ats_certification_heads
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_head_transitions t
  JOIN jobs_ats_certification_activations a ON a.activation_sha256 = t.next_activation_sha256
  WHERE t.transition_sha256 = NEW.current_transition_sha256 AND t.scope_sha256 = NEW.scope_sha256
    AND t.channel = NEW.channel AND t.head_revision = NEW.head_revision
    AND t.next_activation_sha256 = NEW.current_activation_sha256 AND t.next_channel_sequence = NEW.current_channel_sequence
    AND a.scope_sha256 = NEW.scope_sha256 AND a.channel = NEW.channel AND a.channel_sequence = NEW.current_channel_sequence)
BEGIN SELECT RAISE(ABORT, 'ATS certification head projection mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_head_projection_update
BEFORE UPDATE ON jobs_ats_certification_heads
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_head_transitions t
  JOIN jobs_ats_certification_activations a ON a.activation_sha256 = t.next_activation_sha256
  WHERE t.transition_sha256 = NEW.current_transition_sha256 AND t.scope_sha256 = NEW.scope_sha256
    AND t.channel = NEW.channel AND t.head_revision = NEW.head_revision
    AND t.next_activation_sha256 = NEW.current_activation_sha256 AND t.next_channel_sequence = NEW.current_channel_sequence
    AND a.scope_sha256 = NEW.scope_sha256 AND a.channel = NEW.channel AND a.channel_sequence = NEW.current_channel_sequence)
BEGIN SELECT RAISE(ABORT, 'ATS certification head projection mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_quarantine_projection_insert
BEFORE INSERT ON jobs_ats_certification_quarantine_heads
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_quarantine_commands c
  WHERE c.command_sha256 = NEW.current_command_sha256 AND c.scope_kind = NEW.scope_kind
    AND c.scope_id = NEW.scope_id AND c.scope_sha256 = NEW.scope_sha256
    AND c.command_sequence = NEW.current_command_sequence
    AND NEW.state = CASE c.action WHEN 'quarantine' THEN 'quarantined' WHEN 'release' THEN 'released' END)
BEGIN SELECT RAISE(ABORT, 'ATS quarantine head projection mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_quarantine_projection_update
BEFORE UPDATE ON jobs_ats_certification_quarantine_heads
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_quarantine_commands c
  WHERE c.command_sha256 = NEW.current_command_sha256 AND c.scope_kind = NEW.scope_kind
    AND c.scope_id = NEW.scope_id AND c.scope_sha256 = NEW.scope_sha256
    AND c.command_sequence = NEW.current_command_sequence
    AND NEW.state = CASE c.action WHEN 'quarantine' THEN 'quarantined' WHEN 'release' THEN 'released' END)
BEGIN SELECT RAISE(ABORT, 'ATS quarantine head projection mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_circuit_event_scope_insert
BEFORE INSERT ON jobs_ats_certification_circuit_heads
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_circuit_events e
  WHERE e.event_id = NEW.current_event_id AND e.scope_kind = NEW.scope_kind
    AND e.subject_key = NEW.subject_key AND e.transition = NEW.state)
BEGIN SELECT RAISE(ABORT, 'ATS circuit head event scope mismatch'); END;
CREATE TRIGGER IF NOT EXISTS trg_jobs_ats_circuit_event_scope_update
BEFORE UPDATE ON jobs_ats_certification_circuit_heads
WHEN NOT EXISTS (SELECT 1 FROM jobs_ats_certification_circuit_events e
  WHERE e.event_id = NEW.current_event_id AND e.scope_kind = NEW.scope_kind
    AND e.subject_key = NEW.subject_key AND e.transition = NEW.state)
BEGIN SELECT RAISE(ABORT, 'ATS circuit head event scope mismatch'); END;
COMMIT;

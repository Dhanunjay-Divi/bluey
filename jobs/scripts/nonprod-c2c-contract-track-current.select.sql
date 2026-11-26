-- Nonproduction read predicate. The bound tuple comes from an immutable fact,
-- not a client request. A hit says only that its Track input is still current;
-- it does not establish review, source rights, terms, or execution authority.
SELECT 1 AS track_input_current
  FROM jobs_tracks AS t
  JOIN jobs_track_policy_track_input_heads AS h
    ON h.account_id = t.account_id AND h.career_track_id = t.id
  JOIN jobs_track_policy_track_input_transitions AS e
    ON e.account_id = h.account_id AND e.career_track_id = h.career_track_id
   AND e.input_generation = h.input_generation
   AND e.input_transition_id = h.input_transition_id
   AND e.input_transition_sha256 = h.input_transition_sha256
   AND e.track_semantic_sha256 = h.track_semantic_sha256
   AND e.previous_input_generation = h.previous_input_generation
   AND (e.predecessor_input_transition_sha256 = h.predecessor_input_transition_sha256
     OR (e.predecessor_input_transition_sha256 IS NULL
         AND h.predecessor_input_transition_sha256 IS NULL))
   AND e.changed_at_ms = h.updated_at_ms
 WHERE t.account_id = $1 AND t.id = $2 AND t.active = 1
   AND h.input_transition_id = $3 AND h.input_transition_sha256 = $4
   AND NOT EXISTS (
       SELECT 1 FROM account_deletion_intents AS i WHERE i.account_id = t.account_id
   )
   AND NOT EXISTS (
       SELECT 1 FROM jobs_track_policy_track_input_transitions AS newer
        WHERE newer.account_id = h.account_id AND newer.career_track_id = h.career_track_id
          AND newer.input_generation > h.input_generation
   );

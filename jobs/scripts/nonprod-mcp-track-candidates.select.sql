-- Detached SQL fixture: internal values come from a verified grant and cursor.
-- The selection is intentionally not an eligibility or customer result DTO.
WITH admitted AS MATERIALIZED (
  SELECT s.account_id, s.track_id, s.membership_revision
  FROM fixture_mcp_match_heads s
  JOIN accounts a ON a.id = s.account_id
  JOIN jobs_tracks t ON t.account_id = s.account_id AND t.id = s.track_id
  JOIN jobs_track_policy_track_input_heads h ON h.account_id = s.account_id AND h.career_track_id = s.track_id
  JOIN jobs_public_beta_enrollments en ON en.account_id = s.account_id AND en.cohort_id = 'public-v1'
  JOIN jobs_public_beta_cohorts c ON c.id = en.cohort_id
  WHERE s.account_id = $1 AND s.track_id = $2
    AND h.input_generation = $3 AND h.input_transition_sha256 = $4
    AND s.track_generation = h.input_generation AND s.track_transition_sha256 = h.input_transition_sha256
    AND s.membership_revision = $5 AND t.active = 1
    AND a.email_verified_at IS NOT NULL AND a.is_temporary = 0 AND c.state IN ('open','closed_to_new')
    AND NOT EXISTS (SELECT 1 FROM account_deletion_intents d WHERE d.account_id = s.account_id)
    AND NOT EXISTS (SELECT 1 FROM jobs_public_beta_overrides o
      WHERE o.account_id = s.account_id AND o.cohort_id = c.id AND o.denied <> 0)
    AND $6 BETWEEN 1 AND 10 AND CAST($6 AS INTEGER) = $6
    AND (($7 IS NULL AND $8 IS NULL AND $9 IS NULL)
      OR ($7 BETWEEN -9007199254740991 AND 9007199254740991 AND CAST($7 AS INTEGER) = $7
        AND $8 BETWEEN 0 AND 9007199254740991 AND CAST($8 AS INTEGER) = $8
        AND typeof($9) = 'text' AND instr($9, char(0)) = 0
        AND length($9) BETWEEN 1 AND 256 AND $9 NOT GLOB '*[^A-Za-z0-9_.:-]*'))
)
SELECT m.posting_id, m.match_score, m.updated_at_ms
FROM fixture_mcp_match_membership m
WHERE m.account_id = (SELECT account_id FROM admitted)
  AND m.track_id = (SELECT track_id FROM admitted)
  AND m.membership_revision = (SELECT membership_revision FROM admitted)
  -- Ascending physical keys preserve score DESC, time DESC, id BINARY ASC.
  -- The first-page sentinel is below every allowed persisted score key.
  AND (m.score_key, m.time_key, m.posting_id) >
    (COALESCE(-$7, -9007199254740992), COALESCE(-$8, -9007199254740992), COALESCE($9, ''))
ORDER BY m.score_key ASC, m.time_key ASC, m.posting_id ASC
LIMIT CASE WHEN $6 BETWEEN 1 AND 10 AND CAST($6 AS INTEGER) = $6 THEN $6 + 1 ELSE 0 END;

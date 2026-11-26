-- Read one synthetic account, Track, requirement, and trusted time in one snapshot.
-- Choose latest revisions before applying state/time predicates: no stale fallback.
SELECT r.requirement_id, r.revision AS requirement_revision,
       s.source_id, s.revision AS source_revision, g.revision AS grant_revision, r.vendor_id,
       g.account_id, g.track_id, r.terms_sha256, r.observed_at_ms, r.verified_at_ms,
       r.expires_at_ms AS requirement_expires_at_ms,
       s.evidence_sha256 AS source_evidence_sha256, s.effective_at_ms AS source_effective_at_ms,
       s.expires_at_ms AS source_expires_at_ms,
       g.evidence_sha256 AS grant_evidence_sha256, g.effective_at_ms AS grant_effective_at_ms,
       g.expires_at_ms AS grant_expires_at_ms,
       'review_only' AS state, 0 AS can_contact, 0 AS can_attach, 0 AS can_submit
  FROM jobs_c2c_fixture_grants g
  JOIN jobs_tracks t
    ON t.account_id = g.account_id AND t.id = g.track_id
  JOIN jobs_c2c_fixture_requirements r
    ON r.requirement_id = g.requirement_id AND r.revision = g.requirement_revision AND r.vendor_id = g.vendor_id
  JOIN jobs_c2c_fixture_source_rights s
    ON s.source_id = r.source_id AND s.revision = r.source_revision
   AND s.vendor_id = r.vendor_id AND s.terms_sha256 = r.terms_sha256
 WHERE g.account_id = $1 AND g.track_id = $2 AND g.requirement_id = $3
   AND t.active = 1
   AND NOT EXISTS (
       SELECT 1 FROM account_deletion_intents i WHERE i.account_id = t.account_id
   )
   AND NOT EXISTS (
       SELECT 1 FROM jobs_c2c_fixture_grants newer
        WHERE newer.account_id = g.account_id AND newer.track_id = g.track_id
          AND newer.requirement_id = g.requirement_id AND newer.revision > g.revision
   )
   AND NOT EXISTS (
       SELECT 1 FROM jobs_c2c_fixture_requirements newer
        WHERE newer.requirement_id = r.requirement_id AND newer.revision > r.revision
   )
   AND NOT EXISTS (
       SELECT 1 FROM jobs_c2c_fixture_source_rights newer
        WHERE newer.source_id = s.source_id AND newer.revision > s.revision
   )
   AND g.state = 'active' AND $4 >= g.effective_at_ms AND $4 < g.expires_at_ms
   AND r.engagement = 'c2c' AND $4 >= r.verified_at_ms AND $4 < r.expires_at_ms
   AND s.state = 'approved' AND s.permits_matching = 1
   AND r.observed_at_ms >= s.effective_at_ms AND $4 < s.expires_at_ms;

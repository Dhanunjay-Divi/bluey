-- Detached test fixture only. The account is supplied by a trusted resolver,
-- not request JSON. Time is explicit only to make fixture races deterministic.
WITH eligible AS MATERIALIZED (
  SELECT l.account_id, l.connection_id, l.provider, l.thread_key
  FROM fixture_thread_links l
  JOIN accounts a ON a.id = l.account_id
  JOIN fixture_c2c_context c ON c.account_id = l.account_id
    AND c.track_id = l.track_id AND c.track_revision = l.track_revision
    AND c.requirement_id = l.requirement_id AND c.requirement_revision = l.requirement_revision
    AND c.source_id = l.source_id AND c.source_revision = l.source_revision
    AND c.source_seal = l.source_seal AND c.vendor_id = l.vendor_id
  JOIN fixture_connections m ON m.account_id = l.account_id AND m.id = l.connection_id
    AND m.provider = l.provider AND m.subject_key = l.subject_key
    AND m.revision = l.connection_revision AND m.read_grant_revision = l.read_grant_revision
  WHERE l.account_id = $1 AND l.id = $2 AND l.purpose = 'c2c_thread_read'
    AND l.provider IN ('gmail','outlook_email')
    AND l.revoked_at_ms IS NULL AND l.effective_at_ms <= $3 AND $3 < l.expires_at_ms
    AND c.active = 1 AND $3 < c.expires_at_ms
    AND m.connected = 1 AND m.read_allowed = 1 AND $3 < m.read_expires_at_ms
    AND NOT EXISTS (SELECT 1 FROM account_deletion_intents d WHERE d.account_id = l.account_id)
), candidates AS MATERIALIZED (
  SELECT e.provider, e.direction, e.occurred_at_ms, e.private_order_key, e.retain_until_ms
  FROM fixture_thread_evidence e
  WHERE e.account_id = (SELECT account_id FROM eligible)
    AND e.connection_id = (SELECT connection_id FROM eligible)
    AND e.provider = (SELECT provider FROM eligible)
    AND e.thread_key = (SELECT thread_key FROM eligible)
  -- Reject the whole projection on overflow or expired evidence. An
  -- independent retention process, not this reader, removes expired rows.
  ORDER BY e.occurred_at_ms, e.private_order_key
  LIMIT 251
)
SELECT provider, direction, occurred_at_ms
FROM candidates
WHERE (SELECT COUNT(*) FROM candidates) BETWEEN 1 AND 250
  AND NOT EXISTS (
    SELECT 1 FROM candidates
    WHERE direction IS NULL OR direction <> 'inbound' OR occurred_at_ms IS NULL
      OR occurred_at_ms < 0 OR occurred_at_ms > $3 OR retain_until_ms <= $3
  )
ORDER BY occurred_at_ms, private_order_key;

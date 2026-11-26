-- Phase 626 hosted PostgreSQL plan gate. Run only against an isolated,
-- migrated test database; this fixture starts a transaction and rolls it back.
--
-- Acceptance: both plans use idx_jobs_postings_account_workspace_page and do
-- not contain a sequential scan or a Sort node. The deep cursor has 5,500
-- tied score/time rows, so an OFFSET plan cannot satisfy this gate.

BEGIN;

INSERT INTO accounts (id, email, password_hash)
VALUES (
  'phase626-workspace-explain',
  'phase626-workspace-explain@invalid.test',
  'not-a-login'
);

INSERT INTO jobs_postings (
  id, account_id, canonical_key, posting_json, source, canonical_url,
  company, title, location, match_score, status, created_at_ms, updated_at_ms
)
SELECT
  'phase626-workspace-' || lpad(series::text, 4, '0'),
  'phase626-workspace-explain',
  'phase626-canonical-' || lpad(series::text, 4, '0'),
  '{}',
  'fixture',
  'https://invalid.test/' || series::text,
  'Fixture',
  'Fixture role',
  'Remote',
  100,
  'matched',
  1735689600000,
  1735689600000
FROM generate_series(0, 5499) AS series;

ANALYZE jobs_postings;

-- First page: the expression order is exactly equivalent to
-- match_score DESC, updated_at_ms DESC, id ASC.
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT id, posting_json, match_score, updated_at_ms
FROM jobs_postings
WHERE account_id = 'phase626-workspace-explain'
ORDER BY (-match_score) ASC, (-updated_at_ms) ASC, id ASC
LIMIT 52;

-- Deep continuation: 5,000 tied rows precede the cursor. This must remain an
-- index range seek, not an account-history scan followed by a Sort or OFFSET.
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT id, posting_json, match_score, updated_at_ms
FROM jobs_postings
WHERE account_id = 'phase626-workspace-explain'
  AND ((-match_score), (-updated_at_ms), id)
      > (-100::bigint, -1735689600000::bigint, 'phase626-workspace-4999'::text)
ORDER BY (-match_score) ASC, (-updated_at_ms) ASC, id ASC
LIMIT 52;

ROLLBACK;

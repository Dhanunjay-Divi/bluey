-- Test-only synthetic rows for the registered PostgreSQL startup schema.
-- The caller owns the transaction and cleanup. This file grants no role or
-- product authority and must never be registered as a migration.
--
-- Reader tokens (the grants store only their SHA-256 digests):
--   A: bjm1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
--   B: bjm1_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
--   C: bjm1_cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
-- Expected shortlist rows:
--   A -> mcp-scope-track-a / mcp-scope-job-a
--   B -> mcp-scope-track-b / mcp-scope-job-b
--   C -> mcp-scope-track-c / NULL job_id (authorized-empty marker)

INSERT INTO public.accounts (
  id, email, password_hash, email_verified_at, is_temporary
) VALUES
  ('mcp-scope-account-a', 'mcp-scope-a@example.invalid',
   'synthetic-not-a-login-a', pg_catalog.statement_timestamp(), 0),
  ('mcp-scope-account-b', 'mcp-scope-b@example.invalid',
   'synthetic-not-a-login-b', pg_catalog.statement_timestamp(), 0),
  ('mcp-scope-account-c', 'mcp-scope-c@example.invalid',
   'synthetic-not-a-login-c', pg_catalog.statement_timestamp(), 0);

INSERT INTO public.jobs_tracks (
  id, account_id, track_json, active, created_at_ms, updated_at_ms
) VALUES
  ('mcp-scope-track-a', 'mcp-scope-account-a', '{}', 1,
   1700000000000, 1700000000100),
  ('mcp-scope-track-b', 'mcp-scope-account-b', '{}', 1,
   1700000000000, 1700000000100),
  ('mcp-scope-track-c', 'mcp-scope-account-c', '{}', 1,
   1700000000000, 1700000000100);

INSERT INTO public.jobs_track_policy_track_input_transitions (
  input_transition_id, account_id, career_track_id, input_generation,
  previous_input_generation, track_semantic_sha256,
  input_transition_sha256, predecessor_input_transition_sha256, changed_at_ms
) VALUES
  ('mcp-scope-track-transition-a', 'mcp-scope-account-a',
   'mcp-scope-track-a', 1, 0, pg_catalog.repeat('4', 64),
   pg_catalog.repeat('1', 64), NULL, 1700000000100),
  ('mcp-scope-track-transition-b', 'mcp-scope-account-b',
   'mcp-scope-track-b', 1, 0, pg_catalog.repeat('5', 64),
   pg_catalog.repeat('2', 64), NULL, 1700000000100),
  ('mcp-scope-track-transition-c', 'mcp-scope-account-c',
   'mcp-scope-track-c', 1, 0, pg_catalog.repeat('6', 64),
   pg_catalog.repeat('3', 64), NULL, 1700000000100);

INSERT INTO public.jobs_track_policy_track_input_heads (
  account_id, career_track_id, input_generation, previous_input_generation,
  input_transition_id, track_semantic_sha256, input_transition_sha256,
  predecessor_input_transition_sha256, updated_at_ms
) VALUES
  ('mcp-scope-account-a', 'mcp-scope-track-a', 1, 0,
   'mcp-scope-track-transition-a', pg_catalog.repeat('4', 64),
   pg_catalog.repeat('1', 64), NULL, 1700000000100),
  ('mcp-scope-account-b', 'mcp-scope-track-b', 1, 0,
   'mcp-scope-track-transition-b', pg_catalog.repeat('5', 64),
   pg_catalog.repeat('2', 64), NULL, 1700000000100),
  ('mcp-scope-account-c', 'mcp-scope-track-c', 1, 0,
   'mcp-scope-track-transition-c', pg_catalog.repeat('6', 64),
   pg_catalog.repeat('3', 64), NULL, 1700000000100);

UPDATE public.jobs_public_beta_cohorts
   SET state = 'open', opens_at_ms = NULL, closes_at_ms = NULL,
       hard_cap = 3, assigned_count = 3, revision = 2,
       updated_at_ms =
         (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT
 WHERE id = 'public-v1';

INSERT INTO public.jobs_public_beta_enrollments (
  cohort_id, account_id, source, admitted_at_ms
) VALUES
  ('public-v1', 'mcp-scope-account-a', 'admin', 1700000000100),
  ('public-v1', 'mcp-scope-account-b', 'admin', 1700000000100),
  ('public-v1', 'mcp-scope-account-c', 'admin', 1700000000100);

INSERT INTO public.jobs_global_discovery_sources (
  id, provider, source_key, source_json, status, health, next_run_at_ms,
  last_success_at_ms, created_at_ms, updated_at_ms
) VALUES (
  'mcp-scope-source', 'synthetic_fixture', 'mcp-scope-source', '{}',
  'active', 'healthy', 1700000001000, 1700000000200,
  1700000000000, 1700000000200
);

INSERT INTO public.jobs_global_ingestion_runs (
  id, source_id, replay_key, status, expected_rows, received_rows,
  received_batches, rejected_rows, expired_count, started_at_ms,
  completed_at_ms
) VALUES (
  'mcp-scope-run', 'mcp-scope-source', 'mcp-scope-replay', 'completed',
  2, 2, 1, 0, 0, 1700000000200, 1700000000300
);

INSERT INTO public.jobs_global_candidates (
  id, canonical_key, candidate_json, company, title, location, workplace,
  canonical_url, availability_status, first_seen_at_ms, last_seen_at_ms,
  updated_at_ms
) VALUES
  ('mcp-scope-candidate-a', 'mcp-scope-candidate-a', '{}',
   'Synthetic Employer A', 'Synthetic Data Engineer A', 'Remote', 'remote',
   'https://jobs.example.invalid/a', 'active',
   1700000000200, 1700000000300, 1700000000300),
  ('mcp-scope-candidate-b', 'mcp-scope-candidate-b', '{}',
   'Synthetic Employer B', 'Synthetic Platform Engineer B', 'Remote', 'remote',
   'https://jobs.example.invalid/b', 'active',
   1700000000200, 1700000000300, 1700000000300);

INSERT INTO public.jobs_global_candidate_memberships (
  source_id, external_id, candidate_id, content_hash, first_seen_at_ms,
  last_seen_at_ms, last_seen_run_id, availability_status
) VALUES
  ('mcp-scope-source', 'mcp-scope-external-a', 'mcp-scope-candidate-a',
   pg_catalog.repeat('7', 64), 1700000000200, 1700000000300,
   'mcp-scope-run', 'active'),
  ('mcp-scope-source', 'mcp-scope-external-b', 'mcp-scope-candidate-b',
   pg_catalog.repeat('8', 64), 1700000000200, 1700000000300,
   'mcp-scope-run', 'active');

INSERT INTO public.jobs_postings (
  id, account_id, canonical_key, posting_json, source, canonical_url,
  company, title, location, match_score, status, created_at_ms, updated_at_ms
) VALUES
  ('mcp-scope-job-a', 'mcp-scope-account-a', 'mcp-scope-job-a', '{}',
   'synthetic_fixture', 'https://jobs.example.invalid/a',
   'Synthetic Employer A', 'Synthetic Data Engineer A', 'Remote',
   91, 'matched', 1700000000300, 1700000000300),
  ('mcp-scope-job-b', 'mcp-scope-account-b', 'mcp-scope-job-b', '{}',
   'synthetic_fixture', 'https://jobs.example.invalid/b',
   'Synthetic Employer B', 'Synthetic Platform Engineer B', 'Remote',
   83, 'matched', 1700000000300, 1700000000300);

INSERT INTO public.jobs_global_candidate_materializations (
  account_id, candidate_id, job_id, track_id, source_updated_at_ms,
  materialized_at_ms
) VALUES
  ('mcp-scope-account-a', 'mcp-scope-candidate-a', 'mcp-scope-job-a',
   'mcp-scope-track-a', 1700000000300, 1700000000400),
  ('mcp-scope-account-b', 'mcp-scope-candidate-b', 'mcp-scope-job-b',
   'mcp-scope-track-b', 1700000000300, 1700000000400);

INSERT INTO public.jobs_mcp_read_grants (
  token_sha256, account_id, track_id, audience, scope,
  track_input_generation, track_input_transition_sha256,
  issued_at_ms, expires_at_ms, revoked_at_ms
) VALUES
  (pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(
     'bjm1_' || pg_catalog.repeat('a', 64), 'UTF8')), 'hex'),
   'mcp-scope-account-a', 'mcp-scope-track-a',
   'bluey-jobs-mcp-stdio-v1', 'jobs:matches:read', 1,
   pg_catalog.repeat('1', 64),
   (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT - 1000,
   (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT + 600000,
   NULL),
  (pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(
     'bjm1_' || pg_catalog.repeat('b', 64), 'UTF8')), 'hex'),
   'mcp-scope-account-b', 'mcp-scope-track-b',
   'bluey-jobs-mcp-stdio-v1', 'jobs:matches:read', 1,
   pg_catalog.repeat('2', 64),
   (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT - 1000,
   (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT + 600000,
   NULL),
  (pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(
     'bjm1_' || pg_catalog.repeat('c', 64), 'UTF8')), 'hex'),
   'mcp-scope-account-c', 'mcp-scope-track-c',
   'bluey-jobs-mcp-stdio-v1', 'jobs:matches:read', 1,
   pg_catalog.repeat('3', 64),
   (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT - 1000,
   (EXTRACT(epoch FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT + 600000,
   NULL);

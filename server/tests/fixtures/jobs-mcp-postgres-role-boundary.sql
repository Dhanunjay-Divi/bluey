-- Local synthetic qualification template. Never register this as a migration.
-- The verifier inserts only the pinned canonical shortlist query and DB clock.
BEGIN;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
REVOKE ALL ON DATABASE postgres FROM PUBLIC;
SET LOCAL search_path = pg_catalog, pg_temp;

CREATE ROLE jobs_mcp_fixture_owner NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
  NOINHERIT NOREPLICATION NOBYPASSRLS;
CREATE ROLE jobs_mcp_fixture_reader LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
  NOINHERIT NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 1;
CREATE ROLE jobs_mcp_fixture_observer LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
  NOINHERIT NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 1;
GRANT CONNECT ON DATABASE postgres TO jobs_mcp_fixture_reader, jobs_mcp_fixture_observer;

GRANT USAGE ON SCHEMA public TO jobs_mcp_fixture_owner;
CREATE SCHEMA jobs_mcp_fixture;
REVOKE ALL ON SCHEMA jobs_mcp_fixture FROM PUBLIC;
GRANT USAGE ON SCHEMA jobs_mcp_fixture
  TO jobs_mcp_fixture_reader, jobs_mcp_fixture_observer, jobs_mcp_fixture_owner;

GRANT SELECT (id, email_verified_at, is_temporary)
  ON public.accounts TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id, id, active, updated_at_ms)
  ON public.jobs_tracks TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id, career_track_id, input_generation, input_transition_sha256, updated_at_ms)
  ON public.jobs_track_policy_track_input_heads TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id, updated_at_ms) ON public.jobs_profiles TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id, updated_at_ms) ON public.jobs_preferences TO jobs_mcp_fixture_owner;
GRANT SELECT (id, state) ON public.jobs_public_beta_cohorts TO jobs_mcp_fixture_owner;
GRANT SELECT (cohort_id, account_id) ON public.jobs_public_beta_enrollments TO jobs_mcp_fixture_owner;
GRANT SELECT (cohort_id, account_id, denied)
  ON public.jobs_public_beta_overrides TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id) ON public.account_deletion_intents TO jobs_mcp_fixture_owner;
GRANT SELECT (token_sha256, account_id, track_id, audience, scope, track_input_generation,
              track_input_transition_sha256, issued_at_ms, expires_at_ms, revoked_at_ms)
  ON public.jobs_mcp_read_grants TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id, candidate_id, job_id, track_id, source_updated_at_ms, materialized_at_ms)
  ON public.jobs_global_candidate_materializations TO jobs_mcp_fixture_owner;
GRANT SELECT (id, title, company, location, availability_status, updated_at_ms)
  ON public.jobs_global_candidates TO jobs_mcp_fixture_owner;
GRANT SELECT (id, account_id, status, match_score) ON public.jobs_postings TO jobs_mcp_fixture_owner;
GRANT SELECT (account_id, job_id) ON public.jobs_workday_reconciliation_holds TO jobs_mcp_fixture_owner;
GRANT SELECT (source_id, candidate_id, last_seen_run_id, availability_status)
  ON public.jobs_global_candidate_memberships TO jobs_mcp_fixture_owner;
GRANT SELECT (id, status, health) ON public.jobs_global_discovery_sources TO jobs_mcp_fixture_owner;
GRANT SELECT (id, source_id, status, completed_at_ms)
  ON public.jobs_global_ingestion_runs TO jobs_mcp_fixture_owner;

-- Parsed at definition time; the verifier schema-qualifies all 17 relations.
-- Neither the install nor execution path searches caller-supplied schemas.
CREATE FUNCTION jobs_mcp_fixture.read_shortlist(text)
RETURNS TABLE (account_id text, track_id text, input_generation bigint,
               input_transition_sha256 text, issued_at_ms bigint, expires_at_ms bigint,
               inputs_updated_at_ms bigint, job_id text, title text, company text,
               location text, match_score bigint, candidate_updated_at_ms bigint)
LANGUAGE sql STABLE STRICT SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
SET row_security = on
BEGIN ATOMIC
-- @PINNED_SHORTLIST_QUERY@
END;

ALTER FUNCTION jobs_mcp_fixture.read_shortlist(text) OWNER TO jobs_mcp_fixture_owner;
REVOKE ALL ON FUNCTION jobs_mcp_fixture.read_shortlist(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION jobs_mcp_fixture.read_shortlist(text) TO jobs_mcp_fixture_reader;
COMMIT;

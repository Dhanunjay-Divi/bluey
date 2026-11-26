-- A transaction fragment for the dedicated MCP operator, not a migration.
-- A verified operator must lock and check the target, roles, and ACLs first.
-- This fragment must never be used as a standalone psql script.
CREATE SCHEMA jobs_mcp;
REVOKE ALL ON SCHEMA jobs_mcp FROM PUBLIC;
GRANT USAGE ON SCHEMA jobs_mcp TO bluey_jobs_mcp_owner, bluey_jobs_mcp_reader;

CREATE FUNCTION jobs_mcp.read_shortlist(text)
RETURNS TABLE (account_id text, track_id text, input_generation bigint,
               input_transition_sha256 text, issued_at_ms bigint, expires_at_ms bigint,
               inputs_updated_at_ms bigint, job_id text, title text, company text,
               location text, match_score bigint, candidate_updated_at_ms bigint)
LANGUAGE sql STABLE STRICT SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
SET row_security = on
BEGIN ATOMIC
WITH authorized AS (
  SELECT capability.account_id,
         capability.track_id,
         capability.track_input_generation AS input_generation,
         capability.track_input_transition_sha256 AS input_transition_sha256,
         capability.issued_at_ms,
         capability.expires_at_ms,
         GREATEST(track.updated_at_ms, head.updated_at_ms,
                  COALESCE(profile.updated_at_ms, 0),
                  COALESCE(preferences.updated_at_ms, 0)) AS inputs_updated_at_ms
    FROM public.jobs_mcp_read_grants AS capability
    JOIN public.accounts AS account
      ON account.id = capability.account_id
    JOIN public.jobs_tracks AS track
      ON track.account_id = capability.account_id
     AND track.id = capability.track_id
     AND track.active = 1
    JOIN public.jobs_track_policy_track_input_heads AS head
      ON head.account_id = capability.account_id
     AND head.career_track_id = capability.track_id
     AND head.input_generation = capability.track_input_generation
     AND head.input_transition_sha256 = capability.track_input_transition_sha256
    LEFT JOIN public.jobs_profiles AS profile
      ON profile.account_id = capability.account_id
    LEFT JOIN public.jobs_preferences AS preferences
      ON preferences.account_id = capability.account_id
    JOIN public.jobs_public_beta_cohorts AS cohort
      ON cohort.id = 'public-v1'
     AND cohort.state IN ('open', 'closed_to_new')
    JOIN public.jobs_public_beta_enrollments AS enrollment
      ON enrollment.cohort_id = cohort.id
     AND enrollment.account_id = capability.account_id
   WHERE capability.token_sha256 = (CASE WHEN $1 COLLATE pg_catalog."C" ~ '^[0-9a-f]{64}$' AND pg_catalog.octet_length($1) = 64 THEN $1 END)
     AND capability.audience = 'bluey-jobs-mcp-stdio-v1'
     AND capability.scope = 'jobs:matches:read'
     AND capability.issued_at_ms <= (EXTRACT(EPOCH FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT
     AND capability.expires_at_ms > (EXTRACT(EPOCH FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT
     AND capability.issued_at_ms <=
         (EXTRACT(EPOCH FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT
     AND capability.expires_at_ms >
         (EXTRACT(EPOCH FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT
     AND capability.expires_at_ms - capability.issued_at_ms BETWEEN 1 AND 900000
     AND capability.revoked_at_ms IS NULL
     AND account.email_verified_at IS NOT NULL
     AND account.is_temporary = 0
     AND NOT EXISTS (
       SELECT 1
         FROM public.jobs_public_beta_overrides AS override
        WHERE override.cohort_id = cohort.id
          AND override.account_id = capability.account_id
          AND override.denied = 1
     )
     AND NOT EXISTS (
       SELECT 1
         FROM public.account_deletion_intents AS deletion
        WHERE deletion.account_id = capability.account_id
     )
), current_matches AS (
  SELECT posting.id AS job_id,
         candidate.title,
         candidate.company,
         candidate.location,
         posting.match_score,
         candidate.updated_at_ms AS candidate_updated_at_ms
    FROM authorized
    JOIN public.jobs_global_candidate_materializations AS materialization
      ON materialization.account_id = authorized.account_id
     AND materialization.track_id = authorized.track_id
     AND materialization.materialized_at_ms >= authorized.inputs_updated_at_ms
    JOIN public.jobs_global_candidates AS candidate
      ON candidate.id = materialization.candidate_id
     AND materialization.source_updated_at_ms = candidate.updated_at_ms
    JOIN public.jobs_postings AS posting
      ON posting.id = materialization.job_id
     AND posting.account_id = authorized.account_id
     AND posting.status = 'matched'
   WHERE candidate.availability_status IN ('active', 'unknown')
     AND NOT EXISTS (
       SELECT 1
         FROM public.jobs_workday_reconciliation_holds AS hold
        WHERE hold.account_id = authorized.account_id
          AND hold.job_id = materialization.job_id
     )
     AND EXISTS (
       SELECT 1
         FROM public.jobs_global_candidate_memberships AS membership
         JOIN public.jobs_global_discovery_sources AS source
           ON source.id = membership.source_id
         JOIN public.jobs_global_ingestion_runs AS run
           ON run.id = membership.last_seen_run_id
          AND run.source_id = membership.source_id
        WHERE membership.candidate_id = candidate.id
          AND membership.availability_status IN ('active', 'unknown')
          AND source.status = 'active'
          AND source.health IN ('waiting', 'healthy')
          AND run.status = 'completed'
          AND run.completed_at_ms IS NOT NULL
     )
   ORDER BY posting.match_score DESC,
            candidate.updated_at_ms DESC,
            posting.id COLLATE pg_catalog."C" ASC
   LIMIT 241
)
SELECT authorized.account_id,
       authorized.track_id,
       authorized.input_generation,
       authorized.input_transition_sha256,
       authorized.issued_at_ms,
       authorized.expires_at_ms,
       authorized.inputs_updated_at_ms,
       current_matches.job_id,
       current_matches.title,
       current_matches.company,
       current_matches.location,
       current_matches.match_score,
       current_matches.candidate_updated_at_ms
  FROM authorized
  LEFT JOIN current_matches ON TRUE
 ORDER BY CASE WHEN current_matches.job_id IS NULL THEN 1 ELSE 0 END,
          current_matches.match_score DESC,
          current_matches.candidate_updated_at_ms DESC,
          current_matches.job_id COLLATE pg_catalog."C" ASC;
END;

REVOKE ALL ON FUNCTION jobs_mcp.read_shortlist(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION jobs_mcp.read_shortlist(text) TO bluey_jobs_mcp_reader;
-- PostgreSQL ownership transfer by a non-superuser needs CREATE for the
-- target owner. No other connection may see this transient grant.
GRANT CREATE ON SCHEMA jobs_mcp TO bluey_jobs_mcp_owner;
ALTER FUNCTION jobs_mcp.read_shortlist(text) OWNER TO bluey_jobs_mcp_owner;
REVOKE CREATE ON SCHEMA jobs_mcp FROM bluey_jobs_mcp_owner;

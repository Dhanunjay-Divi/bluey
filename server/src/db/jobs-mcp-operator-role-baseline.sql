-- Prospective read-only role and ACL check v2; no authority to apply DDL.
WITH expected(relation, cols) AS (
  VALUES
    ('accounts', ARRAY['id','email_verified_at','is_temporary']),
    ('account_deletion_intents', ARRAY['account_id']),
    ('jobs_global_candidate_materializations',
     ARRAY['account_id','candidate_id','job_id','track_id','source_updated_at_ms','materialized_at_ms']),
    ('jobs_global_candidate_memberships',
     ARRAY['source_id','candidate_id','last_seen_run_id','availability_status']),
    ('jobs_global_candidates', ARRAY['id','title','company','location','availability_status','updated_at_ms']),
    ('jobs_global_discovery_sources', ARRAY['id','status','health']),
    ('jobs_global_ingestion_runs', ARRAY['id','source_id','status','completed_at_ms']),
    ('jobs_mcp_read_grants',
     ARRAY['token_sha256','account_id','track_id','audience','scope','track_input_generation',
           'track_input_transition_sha256','issued_at_ms','expires_at_ms','revoked_at_ms']),
    ('jobs_postings', ARRAY['id','account_id','status','match_score']),
    ('jobs_preferences', ARRAY['account_id','updated_at_ms']),
    ('jobs_profiles', ARRAY['account_id','updated_at_ms']),
    ('jobs_public_beta_cohorts', ARRAY['id','state']),
    ('jobs_public_beta_enrollments', ARRAY['cohort_id','account_id']),
    ('jobs_public_beta_overrides', ARRAY['cohort_id','account_id','denied']),
    ('jobs_track_policy_track_input_heads',
     ARRAY['account_id','career_track_id','input_generation','input_transition_sha256','updated_at_ms']),
    ('jobs_tracks', ARRAY['account_id','id','active','updated_at_ms']),
    ('jobs_workday_reconciliation_holds', ARRAY['account_id','job_id'])
), application_namespaces AS (
  SELECT oid, nspname FROM pg_catalog.pg_namespace
  WHERE nspname NOT IN ('pg_catalog','information_schema') AND nspname !~ '^pg_'
    AND (NOT $4::boolean OR nspname<>'jobs_mcp')
), application_relations AS (
  SELECT c.oid, c.relname, c.relkind, c.relowner, c.relpersistence, c.relispartition, c.relrowsecurity,
         c.relforcerowsecurity, n.nspname
    FROM pg_catalog.pg_class c JOIN application_namespaces n ON n.oid=c.relnamespace
   WHERE c.relkind IN ('r','p','v','m','f','S')
), source_relations AS (
  SELECT c.oid, e.cols FROM expected e
    JOIN application_relations c ON c.nspname='public' AND c.relname=e.relation
     AND c.relkind='r' AND c.relpersistence='p' AND NOT c.relispartition
     AND NOT c.relrowsecurity AND NOT c.relforcerowsecurity
     AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_inherits h
                        WHERE h.inhrelid=c.oid OR h.inhparent=c.oid)
), source_columns AS (
  SELECT c.oid, a.attnum, a.attname, a.atttypid, a.atttypmod, a.attcollation FROM source_relations c
    JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid
     AND a.attnum>0 AND NOT a.attisdropped AND a.attname=ANY(c.cols)
), installed_column_dependencies AS (
  SELECT d.refobjid AS oid, d.refobjsubid AS attnum, d.objsubid, d.deptype
    FROM pg_catalog.pg_depend d
   WHERE $4::boolean AND d.classid='pg_catalog.pg_proc'::pg_catalog.regclass
     AND d.objid=pg_catalog.to_regprocedure('jobs_mcp.read_shortlist(text)')
     AND d.refclassid='pg_catalog.pg_class'::pg_catalog.regclass
)
SELECT
  i.rolcanlogin AND NOT o.rolcanlogin AND r.rolcanlogin
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_roles role
     WHERE role.oid IN (i.oid,o.oid,r.oid)
       AND (role.rolsuper OR role.rolinherit OR role.rolcreaterole
         OR role.rolcreatedb OR role.rolreplication OR role.rolbypassrls)
  )
  AND EXISTS (
    SELECT 1 FROM pg_catalog.pg_auth_members m
     WHERE m.roleid=o.oid AND m.member=i.oid AND m.set_option
       AND NOT m.inherit_option AND NOT m.admin_option
  )
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_auth_members m
     WHERE (m.roleid IN (i.oid,o.oid,r.oid) OR m.member IN (i.oid,o.oid,r.oid))
       AND NOT (m.roleid=o.oid AND m.member=i.oid AND m.set_option
         AND NOT m.inherit_option AND NOT m.admin_option)
  )
  AND (pg_catalog.to_regnamespace('jobs_mcp') IS NOT NULL)=$4::boolean
  AND pg_catalog.has_schema_privilege(o.oid,'public','USAGE')
  AND d.datdba NOT IN (i.oid,o.oid,r.oid)
  AND pg_catalog.has_database_privilege(i.oid,d.oid,'CONNECT')
  AND pg_catalog.has_database_privilege(i.oid,d.oid,'CREATE')
  AND NOT pg_catalog.has_database_privilege(i.oid,d.oid,'TEMPORARY')
  AND pg_catalog.has_database_privilege(r.oid,d.oid,'CONNECT')
  AND NOT pg_catalog.has_database_privilege(o.oid,d.oid,'TEMPORARY, CREATE')
  AND NOT pg_catalog.has_database_privilege(r.oid,d.oid,'TEMPORARY, CREATE')
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_database other
     WHERE other.datallowconn AND other.oid<>d.oid
       AND (pg_catalog.has_database_privilege(r.oid,other.oid,'CONNECT')
         OR pg_catalog.has_database_privilege(o.oid,other.oid,'CONNECT')
         OR pg_catalog.has_database_privilege(i.oid,other.oid,'CONNECT'))
  )
  AND NOT EXISTS (
    SELECT 1 FROM application_namespaces n
     WHERE pg_catalog.has_schema_privilege(r.oid,n.oid,'CREATE')
        OR pg_catalog.has_schema_privilege(o.oid,n.oid,'CREATE')
        OR pg_catalog.has_schema_privilege(i.oid,n.oid,'CREATE')
  )
  AND (SELECT count(*) FROM source_relations)=(SELECT count(*) FROM expected)
  AND (SELECT count(*) FROM source_columns)=(SELECT sum(cardinality(cols)) FROM expected)
  -- The caller separately validates the installed namespace and routine. For
  -- that path the parsed SQL must reference exactly the reviewed typed columns.
  AND (NOT $4::boolean OR (
    (SELECT count(*) FROM installed_column_dependencies)=(SELECT count(*) FROM source_columns)
    AND NOT EXISTS (
      SELECT 1 FROM source_columns c
       WHERE c.atttypmod<>-1 OR c.atttypid<>CASE
              WHEN c.attname IN ('is_temporary','active','denied') THEN 'pg_catalog.int4'::regtype
              WHEN c.attname='email_verified_at' THEN 'pg_catalog.timestamptz'::regtype
              WHEN c.attname IN ('input_generation','track_input_generation','match_score')
                   OR pg_catalog.right(c.attname::text,3)='_ms' THEN 'pg_catalog.int8'::regtype
              ELSE 'pg_catalog.text'::regtype
            END
          OR c.attcollation<>CASE WHEN c.atttypid='pg_catalog.text'::regtype
                 THEN 'pg_catalog."default"'::regcollation::oid ELSE 0::oid END
          OR NOT EXISTS (SELECT 1 FROM installed_column_dependencies x
                          WHERE x.oid=c.oid AND x.attnum=c.attnum
                            AND x.objsubid=0 AND x.deptype='n')
    )
  ))
  AND NOT EXISTS (
    SELECT 1 FROM source_columns c
     WHERE NOT pg_catalog.has_column_privilege(o.oid,c.oid,c.attnum,'SELECT')
        OR pg_catalog.has_column_privilege(o.oid,c.oid,c.attnum,'SELECT WITH GRANT OPTION')
  )
  AND NOT EXISTS (
    SELECT 1 FROM application_relations c WHERE c.relowner IN (i.oid,o.oid,r.oid)
  )
  AND NOT EXISTS (
    SELECT 1 FROM application_relations c
     WHERE CASE WHEN c.relkind='S' THEN
             pg_catalog.has_sequence_privilege(r.oid,c.oid,'USAGE, SELECT, UPDATE')
               OR pg_catalog.has_sequence_privilege(o.oid,c.oid,'USAGE, SELECT, UPDATE')
               OR pg_catalog.has_sequence_privilege(i.oid,c.oid,'USAGE, SELECT, UPDATE')
           ELSE
             pg_catalog.has_table_privilege(r.oid,c.oid,'SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER, MAINTAIN')
               OR pg_catalog.has_any_column_privilege(r.oid,c.oid,'SELECT, INSERT, UPDATE, REFERENCES')
               OR pg_catalog.has_table_privilege(o.oid,c.oid,'SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER, MAINTAIN')
               OR pg_catalog.has_any_column_privilege(o.oid,c.oid,'INSERT, UPDATE, REFERENCES')
               OR pg_catalog.has_table_privilege(i.oid,c.oid,'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER, MAINTAIN')
               OR pg_catalog.has_any_column_privilege(i.oid,c.oid,'INSERT, UPDATE, REFERENCES')
               OR (NOT (c.nspname='public' AND c.relname='bluey_schema_migrations')
                 AND (pg_catalog.has_table_privilege(i.oid,c.oid,'SELECT')
                   OR pg_catalog.has_any_column_privilege(i.oid,c.oid,'SELECT')))
               OR pg_catalog.has_table_privilege(i.oid,c.oid,'SELECT WITH GRANT OPTION')
           END
  )
  AND NOT EXISTS (
    SELECT 1 FROM application_relations c
      JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid
        AND a.attnum>0 AND NOT a.attisdropped
     WHERE c.relkind<>'S'
       AND NOT EXISTS (SELECT 1 FROM source_columns s WHERE s.oid=c.oid AND s.attnum=a.attnum)
       AND pg_catalog.has_column_privilege(o.oid,c.oid,a.attnum,'SELECT')
  )
  AND NOT EXISTS (
    SELECT 1 FROM application_relations c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid
     WHERE c.relkind<>'S' AND a.attnum>0 AND NOT a.attisdropped
       AND pg_catalog.has_column_privilege(i.oid,c.oid,a.attnum,'SELECT WITH GRANT OPTION')
  )
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_proc p JOIN application_namespaces n ON n.oid=p.pronamespace
     WHERE p.prosecdef AND (
       (pg_catalog.has_schema_privilege(r.oid,n.oid,'USAGE') AND pg_catalog.has_function_privilege(r.oid,p.oid,'EXECUTE'))
       OR (pg_catalog.has_schema_privilege(o.oid,n.oid,'USAGE') AND pg_catalog.has_function_privilege(o.oid,p.oid,'EXECUTE'))
       OR (pg_catalog.has_schema_privilege(i.oid,n.oid,'USAGE') AND pg_catalog.has_function_privilege(i.oid,p.oid,'EXECUTE')))
  )
  FROM pg_catalog.pg_roles i, pg_catalog.pg_roles o, pg_catalog.pg_roles r, pg_catalog.pg_database d
 WHERE i.rolname::text=$1 AND o.rolname::text=$2 AND r.rolname::text=$3 AND d.datname=current_database()

-- Read-only PostgreSQL 17.10 catalog projection for the one installed MCP
-- function. The caller hashes normalized_definition only after accepted=true.
-- Parameters are the exact installer, function-owner, and reader role names.
WITH role_ids AS (
  SELECT
    (SELECT oid FROM pg_catalog.pg_roles
      WHERE rolname::pg_catalog.text=$1::pg_catalog.text) AS installer,
    (SELECT oid FROM pg_catalog.pg_roles
      WHERE rolname::pg_catalog.text=$2::pg_catalog.text) AS function_owner,
    (SELECT oid FROM pg_catalog.pg_roles
      WHERE rolname::pg_catalog.text=$3::pg_catalog.text) AS reader
), target_namespace AS (
  SELECT n.oid, n.nspowner, n.nspacl
    FROM pg_catalog.pg_namespace n
   WHERE n.nspname='jobs_mcp'
), target_procs AS (
  SELECT p.*
    FROM pg_catalog.pg_proc p
    JOIN target_namespace n ON n.oid=p.pronamespace
), sole_proc AS (
  SELECT p.* FROM target_procs p
   WHERE (SELECT pg_catalog.count(*) FROM target_procs)=1
), proc_shape AS (
  SELECT
    pg_catalog.count(*)=1
    AND COALESCE(pg_catalog.bool_and(
      p.proname='read_shortlist'
      AND p.prokind='f'
      AND p.proowner=r.function_owner
      AND p.prolang=(SELECT oid FROM pg_catalog.pg_language WHERE lanname='sql')
      AND p.procost=100
      AND p.prorows=1000
      AND p.provariadic=0
      AND p.prosupport=0
      AND p.prosecdef
      AND NOT p.proleakproof
      AND p.proisstrict
      AND p.proretset
      AND p.provolatile='s'
      AND p.proparallel='u'
      AND p.pronargs=1
      AND p.pronargdefaults=0
      AND p.proargtypes[0]='pg_catalog.text'::pg_catalog.regtype
      AND p.prorettype='pg_catalog.record'::pg_catalog.regtype
      AND p.proallargtypes=ARRAY[
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.int8'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.int8'::pg_catalog.regtype,
        'pg_catalog.int8'::pg_catalog.regtype,
        'pg_catalog.int8'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.text'::pg_catalog.regtype,
        'pg_catalog.int8'::pg_catalog.regtype,
        'pg_catalog.int8'::pg_catalog.regtype
      ]::pg_catalog.oid[]
      AND p.proargmodes=ARRAY[
        'i'::pg_catalog."char", 't'::pg_catalog."char", 't'::pg_catalog."char",
        't'::pg_catalog."char", 't'::pg_catalog."char", 't'::pg_catalog."char",
        't'::pg_catalog."char", 't'::pg_catalog."char", 't'::pg_catalog."char",
        't'::pg_catalog."char", 't'::pg_catalog."char", 't'::pg_catalog."char",
        't'::pg_catalog."char", 't'::pg_catalog."char"
      ]
      AND p.proargnames=ARRAY[
        '', 'account_id', 'track_id', 'input_generation',
        'input_transition_sha256', 'issued_at_ms', 'expires_at_ms',
        'inputs_updated_at_ms', 'job_id', 'title', 'company', 'location',
        'match_score', 'candidate_updated_at_ms'
      ]::pg_catalog.text[]
      AND p.proargdefaults IS NULL
      AND p.prosqlbody IS NOT NULL
      AND p.probin IS NULL
      AND p.proconfig IS NOT NULL
      AND pg_catalog.cardinality(p.proconfig)=2
      AND p.proconfig @> ARRAY[
        'search_path=pg_catalog, pg_temp', 'row_security=on'
      ]::pg_catalog.text[]
      AND p.proconfig <@ ARRAY[
        'search_path=pg_catalog, pg_temp', 'row_security=on'
      ]::pg_catalog.text[]
      AND p.proacl IS NOT NULL
    ), false) AS accepted
    FROM sole_proc p
    CROSS JOIN role_ids r
), definition AS MATERIALIZED (
  SELECT CASE WHEN shape.accepted
                   AND pg_catalog.pg_column_size(p.prosqlbody)<=1048576
              THEN pg_catalog.pg_get_functiondef(p.oid) END AS body
    FROM sole_proc p CROSS JOIN proc_shape shape
), schema_acl_actual AS (
  SELECT acl.grantor, acl.grantee, acl.privilege_type, acl.is_grantable
    FROM target_namespace n
    CROSS JOIN LATERAL pg_catalog.aclexplode(
      COALESCE(n.nspacl, pg_catalog.acldefault('n',n.nspowner))
    ) acl
), schema_acl_expected AS (
  SELECT r.installer AS grantor, r.installer AS grantee,
         privilege_type, false AS is_grantable
    FROM role_ids r
    CROSS JOIN (VALUES ('CREATE'::pg_catalog.text), ('USAGE'::pg_catalog.text))
      expected(privilege_type)
  UNION ALL
  SELECT r.installer, r.function_owner, 'USAGE'::pg_catalog.text, false
    FROM role_ids r
  UNION ALL
  SELECT r.installer, r.reader, 'USAGE'::pg_catalog.text, false
    FROM role_ids r
), function_acl_actual AS (
  SELECT acl.grantor, acl.grantee, acl.privilege_type, acl.is_grantable
    FROM target_procs p
    CROSS JOIN LATERAL pg_catalog.aclexplode(
      COALESCE(p.proacl, pg_catalog.acldefault('f',p.proowner))
    ) acl
), function_acl_expected AS (
  SELECT r.function_owner AS grantor, r.function_owner AS grantee,
         'EXECUTE'::pg_catalog.text AS privilege_type, false AS is_grantable
    FROM role_ids r
  UNION ALL
  SELECT r.function_owner, r.reader, 'EXECUTE'::pg_catalog.text, false
    FROM role_ids r
), schema_acl_exact AS (
  SELECT
    (SELECT pg_catalog.count(*) FROM schema_acl_actual)=4
    AND NOT EXISTS (
      SELECT 1 FROM schema_acl_actual a
       WHERE NOT EXISTS (
         SELECT 1 FROM schema_acl_expected e
          WHERE e.grantor=a.grantor AND e.grantee=a.grantee
            AND e.privilege_type=a.privilege_type
            AND e.is_grantable=a.is_grantable
       )
    )
    AND NOT EXISTS (
      SELECT 1 FROM schema_acl_expected e
       WHERE NOT EXISTS (
         SELECT 1 FROM schema_acl_actual a
          WHERE a.grantor=e.grantor AND a.grantee=e.grantee
            AND a.privilege_type=e.privilege_type
            AND a.is_grantable=e.is_grantable
       )
    ) AS accepted
), function_acl_exact AS (
  SELECT
    (SELECT pg_catalog.count(*) FROM function_acl_actual)=2
    AND NOT EXISTS (
      SELECT 1 FROM function_acl_actual a
       WHERE NOT EXISTS (
         SELECT 1 FROM function_acl_expected e
          WHERE e.grantor=a.grantor AND e.grantee=a.grantee
            AND e.privilege_type=a.privilege_type
            AND e.is_grantable=a.is_grantable
       )
    )
    AND NOT EXISTS (
      SELECT 1 FROM function_acl_expected e
       WHERE NOT EXISTS (
         SELECT 1 FROM function_acl_actual a
          WHERE a.grantor=e.grantor AND a.grantee=e.grantee
            AND a.privilege_type=e.privilege_type
            AND a.is_grantable=e.is_grantable
       )
    ) AS accepted
), namespace_depend_exact AS (
  SELECT pg_catalog.count(*)=1
         AND COALESCE(pg_catalog.bool_and(
           d.classid='pg_catalog.pg_proc'::pg_catalog.regclass
           AND d.objid=p.oid AND d.objsubid=0
           AND d.refclassid='pg_catalog.pg_namespace'::pg_catalog.regclass
           AND d.refobjsubid=0 AND d.deptype='n'
         ), false) AS accepted
    FROM target_namespace n
    LEFT JOIN pg_catalog.pg_depend d
      ON d.refclassid='pg_catalog.pg_namespace'::pg_catalog.regclass
     AND d.refobjid=n.oid
    LEFT JOIN target_procs p ON p.oid=d.objid
)
SELECT COALESCE(
  $1::pg_catalog.text='bluey_jobs_mcp_installer'
  AND $2::pg_catalog.text='bluey_jobs_mcp_owner'
  AND $3::pg_catalog.text='bluey_jobs_mcp_reader'
  AND $1::pg_catalog.text<>$2::pg_catalog.text
  AND $1::pg_catalog.text<>$3::pg_catalog.text
  AND $2::pg_catalog.text<>$3::pg_catalog.text
  AND pg_catalog.current_setting('server_version_num')::pg_catalog.int4=170010
  AND r.installer IS NOT NULL AND r.function_owner IS NOT NULL AND r.reader IS NOT NULL
  AND (SELECT pg_catalog.count(*) FROM target_namespace)=1
  AND n.nspowner=r.installer AND n.nspacl IS NOT NULL
  AND shape.accepted
  AND schema_acl.accepted
  AND function_acl.accepted
  AND namespace_deps.accepted
  AND pg_catalog.has_schema_privilege(r.installer,n.oid,'USAGE')
  AND pg_catalog.has_schema_privilege(r.installer,n.oid,'CREATE')
  AND pg_catalog.has_schema_privilege(r.function_owner,n.oid,'USAGE')
  AND NOT pg_catalog.has_schema_privilege(r.function_owner,n.oid,'CREATE')
  AND pg_catalog.has_schema_privilege(r.reader,n.oid,'USAGE')
  AND NOT pg_catalog.has_schema_privilege(r.reader,n.oid,'CREATE')
  AND pg_catalog.has_function_privilege(r.function_owner,p.oid,'EXECUTE')
  AND pg_catalog.has_function_privilege(
        r.function_owner,p.oid,'EXECUTE WITH GRANT OPTION')
  AND pg_catalog.has_function_privilege(r.reader,p.oid,'EXECUTE')
  AND NOT pg_catalog.has_function_privilege(
        r.reader,p.oid,'EXECUTE WITH GRANT OPTION')
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_class c WHERE c.relnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_type t WHERE t.typnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_collation c WHERE c.collnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_conversion c WHERE c.connamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_operator o WHERE o.oprnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_opclass o WHERE o.opcnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_opfamily o WHERE o.opfnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c WHERE c.connamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_statistic_ext s WHERE s.stxnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_ts_config t WHERE t.cfgnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_ts_dict t WHERE t.dictnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_ts_parser t WHERE t.prsnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_ts_template t WHERE t.tmplnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_extension e WHERE e.extnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_default_acl d WHERE d.defaclnamespace=n.oid)
  AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_publication_namespace x WHERE x.pnnspid=n.oid)
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_description d
     WHERE (d.classoid='pg_catalog.pg_namespace'::pg_catalog.regclass
            AND d.objoid=n.oid)
        OR (d.classoid='pg_catalog.pg_proc'::pg_catalog.regclass
            AND d.objoid=p.oid)
  )
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_seclabel s
     WHERE (s.classoid='pg_catalog.pg_namespace'::pg_catalog.regclass
            AND s.objoid=n.oid)
        OR (s.classoid='pg_catalog.pg_proc'::pg_catalog.regclass
            AND s.objoid=p.oid)
  ), false) AS accepted,
  CASE WHEN pg_catalog.octet_length(def.body)<=16384 THEN def.body END AS normalized_definition
FROM role_ids r
CROSS JOIN proc_shape shape
CROSS JOIN schema_acl_exact schema_acl
CROSS JOIN function_acl_exact function_acl
CROSS JOIN namespace_depend_exact namespace_deps
LEFT JOIN target_namespace n ON true
LEFT JOIN sole_proc p ON true
LEFT JOIN definition def ON true;

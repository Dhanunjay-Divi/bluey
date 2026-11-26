-- Read-only PostgreSQL 17 catalog comparator for the fixed canonical ledger.
-- It returns one Boolean and neither locks nor creates, repairs, or mutates state.
WITH target AS (
  SELECT c.oid, c.relowner
    FROM pg_catalog.pg_class AS c
    JOIN pg_catalog.pg_namespace AS n ON n.oid = c.relnamespace
   WHERE n.nspname = 'public'
     AND c.relname = 'bluey_schema_migrations'
     AND c.relkind = 'r'
     AND c.relpersistence = 'p'
     AND NOT c.relispartition
     AND NOT c.relrowsecurity
     AND NOT c.relforcerowsecurity
), columns AS (
  SELECT a.attnum, a.attname, a.atttypid, a.atttypmod, a.attnotnull,
         a.attcollation, a.attgenerated, a.attidentity, a.attislocal,
         a.attinhcount, d.oid AS default_oid,
         pg_catalog.pg_get_expr(d.adbin, d.adrelid, false) AS default_expr
    FROM target AS t
    JOIN pg_catalog.pg_attribute AS a ON a.attrelid = t.oid
    LEFT JOIN pg_catalog.pg_attrdef AS d
      ON d.adrelid = a.attrelid AND d.adnum = a.attnum
   WHERE a.attnum > 0 AND NOT a.attisdropped
), version_column AS (
  SELECT * FROM columns
   WHERE attnum = 1
     AND attname = 'version'
     AND atttypid = 'pg_catalog.text'::pg_catalog.regtype
     AND atttypmod = -1
     AND attnotnull
     AND attcollation = 'pg_catalog."default"'::pg_catalog.regcollation
     AND attgenerated = ''
     AND attidentity = ''
     AND attislocal
     AND attinhcount = 0
     AND default_oid IS NULL
), applied_at_column AS (
  SELECT * FROM columns
   WHERE attnum = 2
     AND attname = 'applied_at'
     AND atttypid = 'pg_catalog.timestamptz'::pg_catalog.regtype
     AND atttypmod = -1
     AND attnotnull
     AND attcollation = 0
     AND attgenerated = ''
     AND attidentity = ''
     AND attislocal
     AND attinhcount = 0
     AND default_oid IS NOT NULL
     AND default_expr = 'now()'
)
SELECT COALESCE((
  SELECT
    (SELECT pg_catalog.count(*) FROM columns) = 2
    AND (SELECT pg_catalog.count(*) FROM version_column) = 1
    AND (SELECT pg_catalog.count(*) FROM applied_at_column) = 1
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_attribute AS a
       WHERE a.attrelid = t.oid AND a.attnum > 0 AND a.attisdropped
    )
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_inherits AS i
       WHERE i.inhrelid = t.oid OR i.inhparent = t.oid
    )
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_trigger AS tr
       WHERE tr.tgrelid = t.oid
    )
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_rewrite AS r WHERE r.ev_class = t.oid
    )
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_policy AS p WHERE p.polrelid = t.oid
    )
    AND (
      SELECT pg_catalog.count(*) FROM pg_catalog.pg_constraint AS c
       WHERE c.conrelid = t.oid
    ) = 1
    AND (
      SELECT pg_catalog.count(*) FROM pg_catalog.pg_index AS i
       WHERE i.indrelid = t.oid
    ) = 1
    AND EXISTS (
      SELECT 1
        FROM pg_catalog.pg_constraint AS c
        JOIN pg_catalog.pg_index AS i ON i.indexrelid = c.conindid
        JOIN pg_catalog.pg_class AS ci ON ci.oid = i.indexrelid
        JOIN pg_catalog.pg_am AS am ON am.oid = ci.relam
        JOIN version_column AS v ON true
        JOIN pg_catalog.pg_opclass AS opc ON opc.oid = i.indclass[0]
        JOIN pg_catalog.pg_namespace AS opn ON opn.oid = opc.opcnamespace
       WHERE c.conrelid = t.oid
         AND c.contype = 'p'
         AND c.convalidated
         AND c.conislocal
         AND c.coninhcount = 0
         AND c.conparentid = 0
         AND NOT c.condeferrable
         AND NOT c.condeferred
         AND c.conkey = ARRAY[v.attnum]::pg_catalog.int2[]
         AND ci.relkind = 'i'
         AND ci.relpersistence = 'p'
         AND NOT ci.relispartition
         AND ci.relowner = t.relowner
         AND am.amname = 'btree'
         AND i.indisvalid
         AND i.indisready
         AND i.indislive
         AND i.indisunique
         AND i.indisprimary
         AND NOT i.indisexclusion
         AND i.indimmediate
         AND NOT i.indnullsnotdistinct
         AND i.indpred IS NULL
         AND i.indexprs IS NULL
         AND i.indnkeyatts = 1
         AND i.indnatts = 1
         AND i.indkey[0] = v.attnum
         AND i.indcollation[0] = v.attcollation
         AND i.indoption[0] = 0
         AND opn.nspname = 'pg_catalog'
         AND opc.opcname = 'text_ops'
         AND opc.opcmethod = am.oid
         AND opc.opcintype = 'pg_catalog.text'::pg_catalog.regtype
    )
    AND t.relowner <> (
      SELECT r.oid FROM pg_catalog.pg_roles AS r
       WHERE r.rolname = current_user
    )
    AND pg_catalog.has_column_privilege(t.oid, 1::pg_catalog.int2, 'SELECT')
    AND NOT pg_catalog.has_table_privilege(
      t.oid,
      'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER, MAINTAIN'
    )
    AND NOT pg_catalog.has_any_column_privilege(
      t.oid,
      'INSERT, UPDATE, REFERENCES'
    )
    FROM target AS t
), false) AS accepted;

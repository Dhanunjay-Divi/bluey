-- Private Phase657 fragment, not a standalone operator migration.
-- The Rust owner supplies schema validation, transaction/lock and ledger boundaries.
TRUNCATE TABLE ONLY pg_temp.phase657_audience_guard_result;

DO $phase657_guard$
DECLARE
  target_oid oid := pg_catalog.to_regclass('public.jobs_managed_cloud_signature_sets');
  state_oid oid := pg_catalog.to_regclass('public.jobs_signature_audience_upgrade_state');
  reference_oid oid := 'pg_temp.phase657_audience_state_reference'::regclass;
  target_attnum smallint;
  actual_expr text;
  expr6 text;
  expr8 text;
  expr10 text;
  expr13 text;
  expr14 text;
  expr16 text;
  target_owner oid;
  expected_state_version smallint := 1;
  state_rows bigint;
  state_valid_rows bigint;
BEGIN
  IF target_oid IS NULL OR NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
     WHERE c.oid=target_oid AND n.nspname='public' AND c.relkind='r'
       AND c.relpersistence='p' AND NOT c.relispartition
  ) THEN
    RAISE EXCEPTION 'signature audience target unavailable';
  END IF;
  SELECT relowner INTO target_owner FROM pg_catalog.pg_class WHERE oid=target_oid;
  SELECT attnum INTO target_attnum FROM pg_catalog.pg_attribute
   WHERE attrelid=target_oid AND attname='target_audience' AND NOT attisdropped
     AND atttypid='pg_catalog.text'::regtype AND atttypmod=-1 AND attnotnull
     AND attgenerated='' AND attidentity=''
     AND attcollation=(SELECT attcollation FROM pg_catalog.pg_attribute
       WHERE attrelid='pg_temp.phase657_audience13'::regclass
         AND attname='target_audience' AND NOT attisdropped);
  IF target_attnum IS NULL OR (SELECT count(*) FROM pg_catalog.pg_constraint
       WHERE conrelid=target_oid AND contype='c' AND target_attnum=ANY(conkey)) <> 1 THEN
    RAISE EXCEPTION 'signature audience target shape mismatch';
  END IF;
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO actual_expr
    FROM pg_catalog.pg_constraint
   WHERE conrelid=target_oid AND conname='jobs_managed_cloud_signature_sets_target_audience_check'
     AND contype='c' AND convalidated AND conislocal AND coninhcount=0
     AND NOT connoinherit AND conkey=ARRAY[target_attnum]::smallint[];
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO expr6 FROM pg_catalog.pg_constraint
   WHERE conrelid='pg_temp.phase657_audience6'::regclass AND conname='audience_check';
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO expr8 FROM pg_catalog.pg_constraint
   WHERE conrelid='pg_temp.phase657_audience8'::regclass AND conname='audience_check';
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO expr10 FROM pg_catalog.pg_constraint
   WHERE conrelid='pg_temp.phase657_audience10'::regclass AND conname='audience_check';
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO expr13 FROM pg_catalog.pg_constraint
   WHERE conrelid='pg_temp.phase657_audience13'::regclass AND conname='audience_check';
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO expr14 FROM pg_catalog.pg_constraint
   WHERE conrelid='pg_temp.phase657_audience14'::regclass AND conname='audience_check';
  SELECT pg_catalog.pg_get_expr(conbin,conrelid) INTO expr16 FROM pg_catalog.pg_constraint
   WHERE conrelid='pg_temp.phase660_audience16'::regclass AND conname='audience_check';
  IF actual_expr IS NULL OR expr6 IS NULL OR expr8 IS NULL OR expr10 IS NULL
     OR expr13 IS NULL OR expr14 IS NULL OR expr16 IS NULL THEN
    RAISE EXCEPTION 'signature audience target constraint mismatch';
  END IF;

  IF state_oid IS NULL THEN
    IF actual_expr = expr6 THEN
      INSERT INTO pg_temp.phase657_audience_guard_result VALUES ('legacy6');
    ELSIF actual_expr = expr8 THEN
      INSERT INTO pg_temp.phase657_audience_guard_result VALUES ('legacy8');
    ELSIF actual_expr = expr10 THEN
      INSERT INTO pg_temp.phase657_audience_guard_result VALUES ('legacy10');
    ELSIF actual_expr = expr13 THEN
      INSERT INTO pg_temp.phase657_audience_guard_result VALUES ('legacy13');
    ELSE
      RAISE EXCEPTION 'signature audience state mismatch';
    END IF;
    RETURN;
  END IF;

  IF actual_expr = expr16 THEN
    reference_oid := 'pg_temp.phase660_audience_state_reference'::regclass;
    expected_state_version := 2;
  ELSIF actual_expr <> expr14 THEN
    RAISE EXCEPTION 'signature audience state mismatch';
  END IF;

  -- Supported writers hold the signature-table lock before touching this
  -- table. The owner is trusted; hostile DDL is outside the protocol.
  IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_class c
       JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
      WHERE c.oid=state_oid AND n.nspname='public' AND c.relkind='r'
        AND c.relpersistence='p' AND NOT c.relispartition AND c.relowner=target_owner)
     OR EXISTS (SELECT 1 FROM pg_catalog.pg_inherits
                 WHERE inhrelid=state_oid OR inhparent=state_oid)
     OR EXISTS (SELECT 1 FROM pg_catalog.pg_trigger WHERE tgrelid=state_oid AND NOT tgisinternal)
     OR EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite WHERE ev_class=state_oid)
     OR EXISTS (SELECT 1 FROM pg_catalog.pg_policy WHERE polrelid=state_oid)
     OR EXISTS (SELECT 1 FROM pg_catalog.pg_class WHERE oid=state_oid AND (relrowsecurity OR relforcerowsecurity)) THEN
    RAISE EXCEPTION 'signature audience state shape mismatch';
  END IF;
  -- Reject extra, dropped, defaulted, generated, identity, or differently
  -- typed columns. Compare the same-engine reference including NOT NULL.
  IF EXISTS (
    (SELECT attnum,attname,atttypid,atttypmod,attnotnull,attcollation,attgenerated,attidentity
       FROM pg_catalog.pg_attribute WHERE attrelid=state_oid AND attnum>0 AND NOT attisdropped
     EXCEPT
     SELECT attnum,attname,atttypid,atttypmod,attnotnull,attcollation,attgenerated,attidentity
       FROM pg_catalog.pg_attribute WHERE attrelid=reference_oid AND attnum>0 AND NOT attisdropped)
    UNION ALL
    (SELECT attnum,attname,atttypid,atttypmod,attnotnull,attcollation,attgenerated,attidentity
       FROM pg_catalog.pg_attribute WHERE attrelid=reference_oid AND attnum>0 AND NOT attisdropped
     EXCEPT
     SELECT attnum,attname,atttypid,atttypmod,attnotnull,attcollation,attgenerated,attidentity
       FROM pg_catalog.pg_attribute WHERE attrelid=state_oid AND attnum>0 AND NOT attisdropped)
  ) OR EXISTS (SELECT 1 FROM pg_catalog.pg_attribute
                WHERE attrelid=state_oid AND attnum>0 AND (attisdropped OR atthasdef)) THEN
    RAISE EXCEPTION 'signature audience state columns mismatch';
  END IF;
  -- Compare all constraint semantics, not generated constraint names.
  IF EXISTS (
    (SELECT contype,conkey,convalidated,conislocal,coninhcount,connoinherit,
            condeferrable,condeferred,pg_catalog.pg_get_expr(conbin,conrelid) AS expr
       FROM pg_catalog.pg_constraint WHERE conrelid=state_oid
     EXCEPT
     SELECT contype,conkey,convalidated,conislocal,coninhcount,connoinherit,
            condeferrable,condeferred,pg_catalog.pg_get_expr(conbin,conrelid)
       FROM pg_catalog.pg_constraint WHERE conrelid=reference_oid)
    UNION ALL
    (SELECT contype,conkey,convalidated,conislocal,coninhcount,connoinherit,
            condeferrable,condeferred,pg_catalog.pg_get_expr(conbin,conrelid)
       FROM pg_catalog.pg_constraint WHERE conrelid=reference_oid
     EXCEPT
     SELECT contype,conkey,convalidated,conislocal,coninhcount,connoinherit,
            condeferrable,condeferred,pg_catalog.pg_get_expr(conbin,conrelid)
       FROM pg_catalog.pg_constraint WHERE conrelid=state_oid)
  ) OR (SELECT count(*) FROM pg_catalog.pg_constraint WHERE conrelid=state_oid) <> 3
    OR (SELECT count(*) FROM pg_catalog.pg_index WHERE indrelid=state_oid) <> 1
    OR NOT EXISTS (SELECT 1 FROM pg_catalog.pg_index i
      JOIN pg_catalog.pg_constraint c ON c.conindid=i.indexrelid
      JOIN pg_catalog.pg_class actual_index ON actual_index.oid=i.indexrelid
      JOIN pg_catalog.pg_index r ON r.indrelid=reference_oid AND r.indisprimary
      JOIN pg_catalog.pg_class reference_index ON reference_index.oid=r.indexrelid
      WHERE i.indrelid=state_oid AND c.conrelid=state_oid AND c.contype='p'
        AND i.indisvalid AND i.indisready AND i.indislive AND i.indisunique
        AND i.indisprimary AND i.indpred IS NULL AND i.indexprs IS NULL
        AND i.indkey=r.indkey AND i.indnkeyatts=r.indnkeyatts AND i.indnatts=r.indnatts
        AND i.indclass=r.indclass AND i.indcollation=r.indcollation
        AND i.indoption=r.indoption AND actual_index.relam=reference_index.relam
        AND i.indnullsnotdistinct=r.indnullsnotdistinct
        AND r.indisvalid AND r.indisready AND r.indislive AND r.indisunique
        AND r.indpred IS NULL AND r.indexprs IS NULL) THEN
    RAISE EXCEPTION 'signature audience state constraints mismatch';
  END IF;
  EXECUTE 'SELECT count(*), count(*) FILTER (WHERE singleton=1 AND version=$1)
             FROM public.jobs_signature_audience_upgrade_state'
    INTO state_rows,state_valid_rows USING expected_state_version;
  IF state_rows<>1 OR state_valid_rows<>1 THEN
    RAISE EXCEPTION 'signature audience state mismatch';
  END IF;
  INSERT INTO pg_temp.phase657_audience_guard_result
    VALUES (CASE expected_state_version WHEN 2 THEN 'release16' ELSE 'successor14' END);
END
$phase657_guard$;

-- Read exactly one row from pg_temp.phase657_audience_guard_result before
-- the caller commits. Its transaction must keep the signature lock until all
-- subsequent guarded DDL and the appropriate selected ledger write complete.

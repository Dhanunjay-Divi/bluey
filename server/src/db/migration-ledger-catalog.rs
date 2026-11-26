// UNREGISTERED, UNRUN PostgreSQL 17.10-only source draft. The caller must
// already hold the signature-table lock, and must keep this transaction alive
// until its selected ledger write. This result is not a transferable capability.
// Public schema only; a missing ledger is refused, not created or repaired.
// Caller handles optional canonical absence and fresh creation separately.
fn setup_migration_ledger_reference(tx: &mut postgres::Transaction<'_>) -> Result<()> {
    // Once per transaction: a conflicting temp object is a refusal, not an
    // instruction to drop/replace it. Same-engine references avoid dependence
    // on hardcoded pg_get_expr pretty-printing.
    tx.batch_execute(
        "SET LOCAL search_path = pg_catalog, pg_temp;
         CREATE TEMP TABLE phase657_ledger_reference (
           version TEXT PRIMARY KEY,
           applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
         ) ON COMMIT DROP;",
    )
    .map_err(|_| anyhow::anyhow!("migration ledger validation unavailable"))?;
    Ok(())
}

fn validate_existing_public_migration_ledger(
    tx: &mut postgres::Transaction<'_>,
    ledger: &MigrationLedgerName,
) -> Result<()> {
    // The identifier renderer accepts only the closed ASCII grammar; it adds
    // both identifier quotes and the fixed public qualifier. No caller string
    // is interpolated without that type. Lock before resolving catalog identity.
    tx.batch_execute(&format!(
        "LOCK TABLE {} IN ROW EXCLUSIVE MODE",
        ledger.qualified_public_identifier()
    ))
    .map_err(|_| anyhow::anyhow!("migration ledger unavailable"))?;

    // Reference must already exist in this transaction; its absence makes the
    // explicit pg_temp regclass lookup below fail closed. Validation is repeatable.
    let valid: bool = tx
        .query_one(
            "WITH target AS (
               SELECT c.oid FROM pg_catalog.pg_class c
               JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
               WHERE n.nspname='public' AND c.relname=$1 AND c.relkind='r'
                 AND c.relpersistence='p' AND NOT c.relispartition
                 AND NOT c.relrowsecurity AND NOT c.relforcerowsecurity
             ), ref AS (
               SELECT 'pg_temp.phase657_ledger_reference'::regclass::oid AS oid
             ), target_columns AS (
               SELECT a.attnum,a.attname,a.atttypid,a.atttypmod,a.attnotnull,
                      a.attcollation,a.attgenerated,a.attidentity,
                      pg_catalog.pg_get_expr(d.adbin,d.adrelid) AS default_expr
               FROM pg_catalog.pg_attribute a
               LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
               WHERE a.attrelid=(SELECT oid FROM target) AND a.attnum>0 AND NOT a.attisdropped
             ), ref_columns AS (
               SELECT a.attnum,a.attname,a.atttypid,a.atttypmod,a.attnotnull,
                      a.attcollation,a.attgenerated,a.attidentity,
                      pg_catalog.pg_get_expr(d.adbin,d.adrelid) AS default_expr
               FROM pg_catalog.pg_attribute a
               LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
               WHERE a.attrelid=(SELECT oid FROM ref) AND a.attnum>0 AND NOT a.attisdropped
             )
             SELECT (SELECT count(*) FROM target)=1
               AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_inherits
                 WHERE inhrelid=(SELECT oid FROM target) OR inhparent=(SELECT oid FROM target))
               AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_attribute
                 WHERE attrelid=(SELECT oid FROM target) AND attnum>0 AND attisdropped)
               AND NOT EXISTS ((SELECT * FROM target_columns EXCEPT SELECT * FROM ref_columns)
                 UNION ALL (SELECT * FROM ref_columns EXCEPT SELECT * FROM target_columns))
               AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_trigger
                 WHERE tgrelid=(SELECT oid FROM target) AND NOT tgisinternal)
               AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite
                 WHERE ev_class=(SELECT oid FROM target))
               AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_policy
                 WHERE polrelid=(SELECT oid FROM target))
               AND (SELECT count(*) FROM pg_catalog.pg_constraint
                 WHERE conrelid=(SELECT oid FROM target))=1
               AND (SELECT count(*) FROM pg_catalog.pg_index
                 WHERE indrelid=(SELECT oid FROM target))=1
               AND EXISTS (
                 SELECT 1 FROM pg_catalog.pg_constraint c
                 JOIN pg_catalog.pg_index i ON i.indexrelid=c.conindid
                 JOIN pg_catalog.pg_class ci ON ci.oid=i.indexrelid
                 JOIN pg_catalog.pg_constraint rc ON rc.conrelid=(SELECT oid FROM ref) AND rc.contype='p'
                 JOIN pg_catalog.pg_index ri ON ri.indexrelid=rc.conindid
                 JOIN pg_catalog.pg_class rci ON rci.oid=ri.indexrelid
                 WHERE c.conrelid=(SELECT oid FROM target) AND c.contype='p'
                   AND c.convalidated AND c.conislocal AND c.coninhcount=0
                   AND NOT c.condeferrable AND NOT c.condeferred
                   AND c.conkey=rc.conkey AND i.indisvalid AND i.indisready AND i.indislive
                   AND i.indisunique AND i.indisprimary AND i.indpred IS NULL AND i.indexprs IS NULL
                   AND i.indkey=ri.indkey AND i.indnkeyatts=ri.indnkeyatts AND i.indnatts=ri.indnatts
                   AND i.indclass=ri.indclass AND i.indcollation=ri.indcollation
                   AND i.indoption=ri.indoption AND i.indnullsnotdistinct=ri.indnullsnotdistinct
                   AND ci.relam=rci.relam
               )",
            &[&ledger.as_str()],
        )
        .map_err(|_| anyhow::anyhow!("migration ledger validation unavailable"))?
        .try_get(0)
        .map_err(|_| anyhow::anyhow!("migration ledger validation unavailable"))?;
    anyhow::ensure!(valid, "migration ledger schema mismatch");
    Ok(())
}

// No owner/grant or trusted-public-schema policy is established here. The
// caller must validate that separately. Setup is exactly once per transaction;
// no arbitrary schema selection is supported and temp collisions fail closed.
// SQL and lock behavior require owned-PG tests.

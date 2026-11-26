// Phase657 integrated source candidate; full integrated verification is pending.
// Schema/transaction logic here is not signature, serving, or effect authority.
const SQLITE_SIGNATURE_AUDIENCE_V2: &str = include_str!("sqlite-signature-audience-v2.sql");
const SQLITE_SIGNATURE_AUDIENCE_MARKER: &str = "jobs-managed-cloud-process-attachment-audience-v1";
const SQLITE_SIGNATURE_RELEASE_V4_MARKER: &str = "073_jobs_managed_cloud_release_v4_audiences.sql";

fn signature_audience_fragments(sql: &str) -> anyhow::Result<(&str, &str)> {
    const BEGIN: &str = "BEGIN IMMEDIATE;\n";
    const SWAP: &str = "DROP TRIGGER trg_jobs_managed_cloud_signature_sets_no_update;";
    const COMMIT: &str = "\nCOMMIT;";
    anyhow::ensure!(
        sql.matches(BEGIN).count() == 1
            && sql.matches(SWAP).count() == 1
            && sql.matches(COMMIT).count() == 1,
        "signature audience migration shape changed"
    );
    let body = sql
        .split_once(BEGIN)
        .unwrap()
        .1
        .split_once(COMMIT)
        .unwrap()
        .0;
    body.split_once(SWAP)
        .map(|(a, _)| (a, &body[a.len()..]))
        .ok_or_else(|| anyhow::anyhow!("signature audience migration swap missing"))
}

// Build canonical sqlite_master rows using the actual registered 067 SQL on a
// separate in-memory DB. This preserves SQLite's own rename canonicalization,
// rather than guessing it. Compare all objects owned by the table, including
// NULL-sql autoindexes; extra user indexes/triggers therefore fail closed.
type SignatureAudienceSchemaRow = (String, String, String, Option<String>);
type SignatureAudienceSchema = Vec<SignatureAudienceSchemaRow>;

fn signature_audience_expected_schema(upgraded: bool) -> anyhow::Result<SignatureAudienceSchema> {
    signature_audience_expected_schema_for(upgraded, false)
}

fn signature_audience_expected_schema_for(
    upgraded: bool,
    release_v4: bool,
) -> anyhow::Result<SignatureAudienceSchema> {
    let sql = SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP;
    let begin = "CREATE TABLE jobs_managed_cloud_signature_sets_phase648 (";
    let end = "INSERT INTO jobs_managed_cloud_signature_sets_phase648 (";
    let after = "CREATE TRIGGER trg_jobs_managed_cloud_signature_sets_no_update BEFORE UPDATE ON jobs_managed_cloud_signature_sets";
    let tail = "-- These are immutable allocation/context METADATA";
    for marker in [begin, end, after, tail] {
        anyhow::ensure!(
            sql.matches(marker).count() == 1,
            "registered 067 signature schema changed"
        );
    }
    let create = &sql[sql.find(begin).unwrap()..sql.find(end).unwrap()];
    let objects = &sql[sql.find(after).unwrap()..sql.find(tail).unwrap()];
    let reference = rusqlite::Connection::open_in_memory()?;
    reference.execute_batch(create)?;
    reference.execute_batch("ALTER TABLE jobs_managed_cloud_signature_sets_phase648 RENAME TO jobs_managed_cloud_signature_sets;")?;
    reference.execute_batch(objects)?;
    if upgraded {
        let (copy, swap) = signature_audience_fragments(SQLITE_SIGNATURE_AUDIENCE_V2)?;
        reference.execute_batch(copy)?;
        reference.execute_batch(swap)?;
    }
    if release_v4 {
        let sql = guarded_release_v4_audience_body(SQLITE_JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES)?;
        let (copy, swap) = signature_audience_fragments(sql)?;
        reference.execute_batch(copy)?;
        reference.execute_batch(swap)?;
    }
    signature_audience_schema(&reference)
}

fn signature_audience_schema(
    conn: &rusqlite::Connection,
) -> anyhow::Result<SignatureAudienceSchema> {
    let mut stmt = conn.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_master WHERE tbl_name='jobs_managed_cloud_signature_sets' ORDER BY type,name")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn signature_audience_no_reserved_objects(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    for schema in ["main", "temp"] {
        let sql = format!("SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE name IN ('jobs_managed_cloud_signature_sets_phase657','phase657_signature_fk_guard','jobs_managed_cloud_signature_sets_phase660','phase660_signature_fk_guard'))");
        let found: bool = conn.query_row(&sql, [], |r| r.get(0))?;
        anyhow::ensure!(!found, "signature audience reserved object exists");
    }
    let shadow: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM temp.sqlite_master WHERE name='jobs_managed_cloud_signature_sets' OR tbl_name='jobs_managed_cloud_signature_sets')", [], |r| r.get(0))?;
    anyhow::ensure!(
        !shadow,
        "signature audience temporary shadow or trigger exists"
    );
    // Conservative supported shape: do not rewrite/drop custom direct views.
    // This textual guard can over-refuse and is not a general dependency parser.
    // Known registered cross-table triggers remain intact and are NOT rejected.
    for schema in ["main", "temp"] {
        let sql = format!("SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type='view' AND instr(lower(coalesce(sql,'')),'jobs_managed_cloud_signature_sets')>0)");
        let view: bool = conn.query_row(&sql, [], |r| r.get(0))?;
        anyhow::ensure!(!view, "signature audience dependent view unsupported");
    }
    Ok(())
}

// Only these three registered cross-table triggers may be temporarily removed.
// Their DDL is sliced from the embedded 067/068 source, never from live DB SQL.
// SQLite stores CREATE TRIGGER without its trailing semicolon in sqlite_master.
fn signature_audience_registered_triggers(
) -> anyhow::Result<Vec<(&'static str, &'static str, &'static str)>> {
    let specifications = [
        (
            "trg_jobs_managed_cloud_context_insert",
            "jobs_managed_cloud_preproduction_contexts",
            SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP,
            "\nEND;",
        ),
        (
            "trg_jobs_managed_cloud_provisioning_insert",
            "jobs_managed_cloud_activations",
            SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP,
            "\nEND;",
        ),
        (
            "trg_jobs_mc_serving_latch_insert_0_guard",
            "jobs_managed_cloud_serving_latches",
            SQLITE_JOBS_PROVISIONED_RUNTIME_OBSERVATIONS,
            " END;",
        ),
    ];
    let mut result = Vec::with_capacity(specifications.len());
    for (name, owner, source, terminator) in specifications {
        let prefix = format!("CREATE TRIGGER {name} ");
        anyhow::ensure!(
            source.matches(&prefix).count() == 1,
            "registered signature dependency boundary changed"
        );
        let start = source
            .find(&prefix)
            .ok_or_else(|| anyhow::anyhow!("registered signature dependency missing"))?;
        let remaining = &source[start..];
        let end = remaining
            .find(terminator)
            .ok_or_else(|| anyhow::anyhow!("registered signature dependency end missing"))?
            + terminator.len()
            - 1;
        result.push((name, owner, &remaining[..end]));
    }
    Ok(result)
}

fn signature_audience_validate_registered_triggers(
    conn: &rusqlite::Connection,
) -> anyhow::Result<()> {
    for (name, owner, expected) in signature_audience_registered_triggers()? {
        let actual: Option<(String, String)> = conn
            .query_row(
                "SELECT tbl_name,sql FROM main.sqlite_master WHERE type='trigger' AND name=?1",
                [name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        anyhow::ensure!(
            actual
                .as_ref()
                .is_some_and(|(actual_owner, sql)| actual_owner == owner && sql == expected),
            "registered signature dependency mismatch"
        );
    }
    let unknown: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM main.sqlite_master WHERE type='trigger' AND tbl_name<>'jobs_managed_cloud_signature_sets' AND instr(lower(coalesce(sql,'')),'jobs_managed_cloud_signature_sets')>0 AND name NOT IN ('trg_jobs_managed_cloud_context_insert','trg_jobs_managed_cloud_provisioning_insert','trg_jobs_mc_serving_latch_insert_0_guard')) OR EXISTS(SELECT 1 FROM temp.sqlite_master WHERE type='trigger' AND (instr(lower(coalesce(sql,'')),'jobs_managed_cloud_signature_sets')>0 OR name IN ('trg_jobs_managed_cloud_context_insert','trg_jobs_managed_cloud_provisioning_insert','trg_jobs_mc_serving_latch_insert_0_guard')))", [], |row| row.get(0))?;
    anyhow::ensure!(!unknown, "unrecognized signature dependent trigger");
    Ok(())
}

fn apply_sqlite_signature_audience_migration(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    apply_sqlite_signature_audience_migration_with_hook(conn, || Ok(()))
}

fn apply_sqlite_release_v4_signature_audience_migration(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    apply_sqlite_signature_audience_stage(conn, true, || Ok(()))
}

// Private deterministic fault seam; production entry always supplies no-op.
// It cannot create authority or relax validation, and the outer transaction
// must roll back if the test callback returns an error.
fn apply_sqlite_signature_audience_migration_with_hook<F: FnOnce() -> anyhow::Result<()>>(
    conn: &mut rusqlite::Connection,
    after_known_trigger_drop: F,
) -> anyhow::Result<()> {
    apply_sqlite_signature_audience_stage(conn, false, after_known_trigger_drop)
}

fn apply_sqlite_signature_audience_stage<F: FnOnce() -> anyhow::Result<()>>(
    conn: &mut rusqlite::Connection,
    release_v4: bool,
    after_known_trigger_drop: F,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        conn.is_autocommit(),
        "signature audience migration requires autocommit"
    );
    let baseline = signature_audience_expected_schema(false)?;
    let attachment = signature_audience_expected_schema(true)?;
    let latest = signature_audience_expected_schema_for(true, true)?;
    let original_fk: bool = conn.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    let result = (|| -> anyhow::Result<()> {
        conn.pragma_update(None, "foreign_keys", false)?;
        anyhow::ensure!(
            !conn.pragma_query_value::<bool, _>(None, "foreign_keys", |r| r.get(0))?,
            "foreign_keys could not be disabled"
        );
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        signature_audience_no_reserved_objects(&tx)?;
        signature_audience_validate_registered_triggers(&tx)?;
        let applied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM bluey_data_migrations WHERE name=?1)",
            [SQLITE_SIGNATURE_AUDIENCE_MARKER],
            |r| r.get(0),
        )?;
        let v4_applied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM bluey_data_migrations WHERE name=?1)",
            [SQLITE_SIGNATURE_RELEASE_V4_MARKER],
            |r| r.get(0),
        )?;
        anyhow::ensure!(
            !(release_v4 || v4_applied) || applied,
            "release-v4 signature audience requires process attachment migration"
        );
        let current = signature_audience_schema(&tx)?;
        let expected_current = if v4_applied {
            &latest
        } else if applied {
            &attachment
        } else {
            &baseline
        };
        anyhow::ensure!(
            &current == expected_current,
            "signature audience schema mismatch"
        );
        let needs_change = if release_v4 { !v4_applied } else { !applied };
        if needs_change {
            let (sql, next_table, marker, expected) = if release_v4 {
                (
                    guarded_release_v4_audience_body(
                        SQLITE_JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES,
                    )?,
                    "jobs_managed_cloud_signature_sets_phase660",
                    SQLITE_SIGNATURE_RELEASE_V4_MARKER,
                    &latest,
                )
            } else {
                (
                    SQLITE_SIGNATURE_AUDIENCE_V2,
                    "jobs_managed_cloud_signature_sets_phase657",
                    SQLITE_SIGNATURE_AUDIENCE_MARKER,
                    &attachment,
                )
            };
            let (copy, swap) = signature_audience_fragments(sql)?;
            tx.execute_batch(copy)?;
            // Compare SQL values under SQLite equality semantics; this is not
            // a raw-byte, storage-class, collation or implicit-rowid proof.
            let compare = format!(
                "SELECT EXISTS(SELECT * FROM jobs_managed_cloud_signature_sets EXCEPT SELECT * FROM {next_table})
                 OR EXISTS(SELECT * FROM {next_table} EXCEPT SELECT * FROM jobs_managed_cloud_signature_sets)"
            );
            let differs: bool = tx.query_row(&compare, [], |r| r.get(0))?;
            anyhow::ensure!(!differs, "signature audience copy mismatch");
            // The 070 baseline has three known external triggers that SQLite
            // reparses on ALTER TABLE. Only their pinned definitions are removed;
            // rollback restores them if any swap/recreation/validation fails.
            for (name, _, _) in signature_audience_registered_triggers()? {
                tx.execute_batch(&format!("DROP TRIGGER {name};"))?;
            }
            after_known_trigger_drop()?;
            tx.execute_batch(swap)?;
            anyhow::ensure!(
                signature_audience_schema(&tx)? == *expected,
                "signature audience replacement mismatch"
            );
            for (_, _, ddl) in signature_audience_registered_triggers()? {
                tx.execute_batch(&format!("{ddl};"))?;
            }
            signature_audience_validate_registered_triggers(&tx)?;
            anyhow::ensure!(
                tx.execute(
                    "INSERT INTO bluey_data_migrations(name) VALUES (?1)",
                    [marker]
                )? == 1,
                "signature audience marker insert failed"
            );
        }
        let violation = tx
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some();
        anyhow::ensure!(!violation, "signature audience foreign key violation");
        tx.commit()?;
        Ok(())
    })();
    // Transaction drop rolls back on all error paths before this restoration.
    let rollback = if conn.is_autocommit() {
        Ok(())
    } else {
        conn.execute_batch("ROLLBACK")
    };
    let restore = conn
        .pragma_update(None, "foreign_keys", original_fk)
        .and_then(|()| conn.pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0)))
        .and_then(|actual| {
            if actual == original_fk {
                Ok(())
            } else {
                Err(rusqlite::Error::InvalidQuery)
            }
        });
    if let Err(error) = result {
        return Err(error.context(format!(
            "signature audience rollback_ok={} fk_restore_ok={}",
            rollback.is_ok(),
            restore.is_ok()
        )));
    }
    rollback?;
    restore?;
    anyhow::ensure!(
        conn.is_autocommit(),
        "signature audience transaction remained open"
    );
    Ok(())
}

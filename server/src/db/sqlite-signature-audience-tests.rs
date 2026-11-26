// Phase657 integrated source candidate; full integrated verification is pending.
// Schema/transaction logic here is not signature, serving, or effect authority.
#[cfg(test)]
mod phase657_signature_audience_tests {
    use super::*;

    const OLD: &[&str] = &[
        "bluey-jobs-managed-cloud-activation-v1",
        "bluey-jobs-managed-cloud-activation-v2",
        "bluey-jobs-managed-cloud-activation-v3",
        "bluey-jobs-managed-cloud-cohort-v1",
        "bluey-jobs-managed-cloud-release-v1",
        "bluey-jobs-managed-cloud-release-v2",
        "bluey-jobs-managed-cloud-release-v3",
        "bluey-jobs-managed-cloud-revocation-v1",
        "bluey-jobs-managed-cloud-rollback-v1",
        "bluey-jobs-managed-cloud-trust-policy-v1",
        "bluey-jobs-managed-cloud-provisioning-v4",
        "bluey-jobs-managed-cloud-preproduction-context-v1",
        "bluey-jobs-managed-cloud-serving-latch-v1",
    ];
    const NEW: &str = "bluey-jobs-managed-cloud-process-attachment-v1";
    type SignatureAudienceRow = (
        String,
        String,
        i64,
        String,
        String,
        String,
        i64,
        i64,
        String,
        String,
        i64,
    );
    type TriggerSnapshot = Vec<(String, String, String)>;
    type ForeignKeySnapshot = Vec<(String, String, String, String)>;
    type RegisteredDependencySnapshot = (TriggerSnapshot, ForeignKeySnapshot);

    const KNOWN_TRIGGER_NAMES: &[(&str, &str)] = &[
        (
            "trg_jobs_managed_cloud_context_insert",
            "jobs_managed_cloud_preproduction_contexts",
        ),
        (
            "trg_jobs_managed_cloud_provisioning_insert",
            "jobs_managed_cloud_activations",
        ),
        (
            "trg_jobs_mc_serving_latch_insert_0_guard",
            "jobs_managed_cloud_serving_latches",
        ),
    ];

    fn chain_contains(error: &anyhow::Error, expected: &str) -> bool {
        error
            .chain()
            .any(|cause| cause.to_string().contains(expected))
    }

    fn chain_has_sqlite_constraint_check(error: &anyhow::Error) -> bool {
        error.chain().any(|cause| {
            matches!(
                cause.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::SqliteFailure(code, _))
                    if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_CHECK
            )
        })
    }

    fn pool() -> DbPool {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory().with_init(|conn| {
            conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA recursive_triggers=ON;")
        });
        let pool = DbPool::Sqlite(
            r2d2::Pool::builder()
                .max_size(1)
                .build(SafeSqliteConnectionManager::new(manager))
                .unwrap(),
        );
        // Construct the genuine 070 baseline without invoking the new hook.
        run_sqlite_migration_list(&pool, MIGRATIONS).unwrap();
        pool
    }

    fn insert(conn: &rusqlite::Connection, id: usize, audience: &str) -> rusqlite::Result<usize> {
        conn.execute("INSERT INTO jobs_managed_cloud_signature_sets (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,recorded_by,recorded_at_ms) VALUES (?1,?2,1,'root',?3,?4,1,1,'schema-only','fixture',1)",
          rusqlite::params![format!("{id:064x}"), format!("schema-{id}"), audience, format!("{:064x}",id+1000)])
    }

    fn rows(conn: &rusqlite::Connection) -> Vec<SignatureAudienceRow> {
        let mut stmt = conn.prepare("SELECT signature_set_sha256,signature_set_id,trust_generation,role,target_audience,target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,recorded_by,recorded_at_ms FROM jobs_managed_cloud_signature_sets ORDER BY signature_set_sha256").unwrap();
        stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    }

    fn foreign_key_witness(conn: &rusqlite::Connection) -> Vec<(String, i64, String, i64)> {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    }

    fn known_trigger_rows(conn: &rusqlite::Connection) -> Vec<(String, String, String)> {
        let mut triggers = Vec::new();
        for &(name, table) in KNOWN_TRIGGER_NAMES {
            let (owner, sql): (String, String) = conn
                .query_row(
                    "SELECT tbl_name,sql FROM sqlite_master WHERE type='trigger' AND name=?1",
                    [name],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(owner, table, "known trigger owner changed: {name}");
            triggers.push((name.to_owned(), owner, sql));
        }
        triggers
    }

    fn registered_dependency_snapshot(conn: &rusqlite::Connection) -> RegisteredDependencySnapshot {
        let triggers = known_trigger_rows(conn);
        let expected_triggers = signature_audience_registered_triggers()
            .unwrap()
            .into_iter()
            .map(|(name, owner, sql)| (name.to_owned(), owner.to_owned(), sql.to_owned()))
            .collect::<Vec<_>>();
        assert_eq!(
            triggers, expected_triggers,
            "registered trigger DDL changed"
        );
        let mut foreign_keys = Vec::new();
        for table in [
            "jobs_managed_cloud_signatures",
            "jobs_managed_cloud_trust_policies",
            "jobs_managed_cloud_manifests",
            "jobs_managed_cloud_cohorts",
            "jobs_managed_cloud_activations",
            "jobs_managed_cloud_rollbacks",
            "jobs_managed_cloud_revocations",
            "jobs_managed_cloud_preproduction_contexts",
            "jobs_managed_cloud_serving_latches",
        ] {
            let mut statement = conn
                .prepare(&format!("PRAGMA foreign_key_list({table})"))
                .unwrap();
            let matches = statement
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(6)?,
                    ))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
                .into_iter()
                .filter(|row| row.0 == "jobs_managed_cloud_signature_sets")
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "registered signature foreign key: {table}"
            );
            assert_eq!(matches[0].2, "signature_set_sha256");
            assert_eq!(matches[0].3, "RESTRICT");
            foreign_keys.push((
                table.to_owned(),
                matches[0].1.clone(),
                matches[0].2.clone(),
                matches[0].3.clone(),
            ));
        }
        (triggers, foreign_keys)
    }

    fn assert_registered_trigger_definitions_and_behavior(
        conn: &rusqlite::Connection,
        expected: &[(String, String, String)],
    ) {
        let actual = registered_dependency_snapshot(conn).0;
        assert_eq!(
            actual, expected,
            "known external trigger definition changed"
        );
        assert_registered_guards_reject_invalid_metadata(conn);
    }

    fn assert_registered_guards_reject_invalid_metadata(conn: &rusqlite::Connection) {
        for (sql, expected) in [
            ("INSERT INTO jobs_managed_cloud_preproduction_contexts DEFAULT VALUES", "invalid managed cloud context metadata"),
            ("INSERT INTO jobs_managed_cloud_activations(authority_kind) VALUES('provisioning_v4')", "invalid managed cloud provisioning metadata"),
            ("INSERT INTO jobs_managed_cloud_serving_latches DEFAULT VALUES", "managed cloud serving latch metadata or bootstrap snapshot is invalid"),
        ] {
            let error = conn.execute(sql, []).unwrap_err();
            assert!(error.to_string().contains(expected), "registered guard did not fire: {error}");
        }
    }

    #[test]
    fn release_v4_audience_upgrade_rolls_back_then_survives_registered_restart() {
        const RELEASE: &str = "bluey-jobs-managed-cloud-release-v4";
        const ACTIVATION: &str = "bluey-jobs-managed-cloud-activation-v4";
        let pool = pool();
        let mut db = pool.get().unwrap();
        apply_sqlite_signature_audience_migration(&mut db).unwrap();
        insert(&db, 1, OLD[0]).unwrap();
        insert(&db, 2, NEW).unwrap();
        let before = rows(&db);
        let dependencies = registered_dependency_snapshot(&db);
        assert!(insert(&db, 3, RELEASE).is_err());
        let raw = db.execute_batch(SQLITE_JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES);
        assert!(raw.unwrap_err().to_string().contains("no such function"));
        let failed = apply_sqlite_signature_audience_stage(&mut db, true, || {
            anyhow::bail!("injected release-v4 move failure");
        });
        assert!(chain_contains(
            &failed.unwrap_err(),
            "injected release-v4 move failure"
        ));
        assert!(insert(&db, 3, RELEASE).is_err());
        assert_eq!(rows(&db), before);
        assert_eq!(registered_dependency_snapshot(&db), dependencies);
        assert!(db.is_autocommit());
        assert!(db
            .pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
            .unwrap());
        let marker = |conn: &rusqlite::Connection| -> Option<String> {
            conn.query_row(
                "SELECT applied_at FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_RELEASE_V4_MARKER],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
        };
        assert!(marker(&db).is_none());
        apply_sqlite_release_v4_signature_audience_migration(&mut db).unwrap();
        assert_eq!(rows(&db), before);
        assert!(marker(&db).is_some());
        insert(&db, 3, RELEASE).unwrap();
        insert(&db, 4, ACTIVATION).unwrap();
        assert!(insert(&db, 5, "bluey-jobs-managed-cloud-release-v5").is_err());
        let after = rows(&db);
        let applied_at = marker(&db);
        drop(db);
        run_migrations(&pool).unwrap();
        let db = pool.get().unwrap();
        assert_eq!(rows(&db), after);
        assert_eq!(marker(&db), applied_at);
        assert_eq!(registered_dependency_snapshot(&db), dependencies);
    }

    #[test]
    fn populated_fresh_chain_preserves_all_rows_enforces_audiences_and_replays() {
        for original_fk in [false, true] {
            let pool = pool();
            let mut db = pool.get().unwrap();
            db.pragma_update(None, "foreign_keys", original_fk).unwrap();
            for (i, audience) in OLD.iter().enumerate() {
                insert(&db, i + 1, audience).unwrap();
            }
            db.execute_batch("CREATE TABLE phase657_signature_child(signature TEXT NOT NULL REFERENCES jobs_managed_cloud_signature_sets(signature_set_sha256));").unwrap();
            db.execute(
                "INSERT INTO phase657_signature_child VALUES (?1)",
                [format!("{:064x}", 1)],
            )
            .unwrap();
            let dependencies = registered_dependency_snapshot(&db);
            let trigger_definitions = dependencies.0.clone();
            assert_registered_trigger_definitions_and_behavior(&db, &trigger_definitions);
            let before = rows(&db);
            apply_sqlite_signature_audience_migration(&mut db).unwrap();
            assert_eq!(rows(&db), before);
            assert_eq!(registered_dependency_snapshot(&db), dependencies);
            assert_registered_trigger_definitions_and_behavior(&db, &trigger_definitions);
            assert_eq!(
                db.query_row("SELECT count(*) FROM phase657_signature_child", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                1
            );
            assert_eq!(
                db.pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
                    .unwrap(),
                original_fk
            );
            assert!(db.is_autocommit());
            insert(&db, 100, NEW).unwrap();
            assert!(insert(&db, 101, "bluey-jobs-unknown-audience-v1").is_err());
            assert!(db
                .execute(
                    "UPDATE jobs_managed_cloud_signature_sets SET recorded_by='changed'",
                    []
                )
                .is_err());
            assert!(db
                .execute("DELETE FROM jobs_managed_cloud_signature_sets", [])
                .is_err());
            assert!(insert(&db, 100, NEW).is_err());
            assert!(db.execute("INSERT INTO jobs_managed_cloud_signature_sets (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,recorded_by,recorded_at_ms) SELECT ?1,'another-id',trust_generation,role,target_audience,target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,recorded_by,recorded_at_ms FROM jobs_managed_cloud_signature_sets WHERE signature_set_sha256=?2", rusqlite::params![format!("{:064x}",999),format!("{:064x}",100)]).is_err());
            let replay = rows(&db);
            apply_sqlite_signature_audience_migration(&mut db).unwrap();
            assert_eq!(rows(&db), replay);
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                    [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            drop(db);
            // Actual production entry, not merely direct wrapper replay.
            run_migrations(&pool).unwrap();
            let db = pool.get().unwrap();
            assert_eq!(rows(&db), replay);
            assert_eq!(registered_dependency_snapshot(&db), dependencies);
            assert_registered_trigger_definitions_and_behavior(&db, &trigger_definitions);
        }
    }

    #[test]
    fn direct_custom_dependent_view_is_refused_without_changes() {
        let pool = pool();
        let mut db = pool.get().unwrap();
        insert(&db, 1, OLD[0]).unwrap();
        db.execute_batch("CREATE VIEW phase657_signature_view AS SELECT signature_set_sha256 FROM jobs_managed_cloud_signature_sets").unwrap();
        let before = rows(&db);
        let schema = signature_audience_schema(&db).unwrap();
        let dependencies = registered_dependency_snapshot(&db);
        let error = apply_sqlite_signature_audience_migration(&mut db).unwrap_err();
        assert!(chain_contains(
            &error,
            "signature audience dependent view unsupported"
        ));
        assert_eq!(rows(&db), before);
        assert_eq!(signature_audience_schema(&db).unwrap(), schema);
        assert_eq!(registered_dependency_snapshot(&db), dependencies);
        assert_eq!(
            db.query_row("SELECT count(*) FROM phase657_signature_view", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(db.is_autocommit());
    }

    #[test]
    fn unexpected_object_refuses_without_ddl_or_marker() {
        let pool = pool();
        let mut db = pool.get().unwrap();
        insert(&db, 1, OLD[0]).unwrap();
        db.execute_batch("CREATE INDEX phase657_unexpected_index ON jobs_managed_cloud_signature_sets(recorded_by)").unwrap();
        let baseline = signature_audience_schema(&db).unwrap();
        let original = rows(&db);
        let error = apply_sqlite_signature_audience_migration(&mut db).unwrap_err();
        assert!(chain_contains(&error, "signature audience schema mismatch"));
        assert_eq!(signature_audience_schema(&db).unwrap(), baseline);
        assert_eq!(rows(&db), original);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(db.is_autocommit());
    }

    #[test]
    fn changed_known_external_trigger_refuses_without_ddl_or_marker() {
        let pool = pool();
        let mut db = pool.get().unwrap();
        insert(&db, 1, OLD[0]).unwrap();
        db.execute_batch(
            "DROP TRIGGER trg_jobs_managed_cloud_context_insert;
             CREATE TRIGGER trg_jobs_managed_cloud_context_insert
             BEFORE INSERT ON jobs_managed_cloud_preproduction_contexts
             BEGIN SELECT RAISE(ABORT, 'phase657 altered registered trigger'); END;",
        )
        .unwrap();
        let changed_trigger_rows = known_trigger_rows(&db);
        let before_schema = signature_audience_schema(&db).unwrap();
        let before_rows = rows(&db);
        let error = apply_sqlite_signature_audience_migration(&mut db).unwrap_err();
        assert!(chain_contains(
            &error,
            "registered signature dependency mismatch"
        ));
        assert_eq!(known_trigger_rows(&db), changed_trigger_rows);
        assert_eq!(signature_audience_schema(&db).unwrap(), before_schema);
        assert_eq!(rows(&db), before_rows);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(db.is_autocommit());
    }

    #[test]
    fn fk_failure_after_swap_rolls_back_schema_rows_marker_and_restores_fk() {
        let pool = pool();
        let mut db = pool.get().unwrap();
        insert(&db, 1, OLD[0]).unwrap();
        db.pragma_update(None, "foreign_keys", false).unwrap();
        db.execute_batch("CREATE TABLE phase657_bad_child(parent TEXT REFERENCES accounts(id)); INSERT INTO phase657_bad_child VALUES('missing-parent');").unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        let baseline = signature_audience_schema(&db).unwrap();
        let original = rows(&db);
        let dependencies = registered_dependency_snapshot(&db);
        let trigger_definitions = dependencies.0.clone();
        let witness = foreign_key_witness(&db);
        assert_eq!(
            witness,
            vec![("phase657_bad_child".to_owned(), 1, "accounts".to_owned(), 0,)],
        );
        assert!(SQLITE_SIGNATURE_AUDIENCE_V2.contains(
            "INSERT INTO phase657_signature_fk_guard SELECT 1 FROM pragma_foreign_key_check LIMIT 1;"
        ));
        let error = apply_sqlite_signature_audience_migration(&mut db).unwrap_err();
        assert!(chain_has_sqlite_constraint_check(&error));
        assert!(chain_contains(&error, "CHECK constraint failed: ok = 0"));
        assert!(!chain_contains(
            &error,
            "no such table: main.jobs_managed_cloud_signature_sets"
        ));
        assert_eq!(signature_audience_schema(&db).unwrap(), baseline);
        assert_eq!(rows(&db), original);
        assert_eq!(foreign_key_witness(&db), witness);
        assert_eq!(registered_dependency_snapshot(&db), dependencies);
        assert_registered_trigger_definitions_and_behavior(&db, &trigger_definitions);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(db
            .pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
            .unwrap());
        assert!(db.is_autocommit());
    }

    #[test]
    fn injected_failure_after_known_trigger_drop_restores_all_registered_state() {
        let pool = pool();
        let mut db = pool.get().unwrap();
        insert(&db, 1, OLD[0]).unwrap();
        let dependencies = registered_dependency_snapshot(&db);
        let trigger_definitions = dependencies.0.clone();
        let before_schema = signature_audience_schema(&db).unwrap();
        let before_rows = rows(&db);
        let error = apply_sqlite_signature_audience_migration_with_hook(&mut db, || {
            Err(anyhow::anyhow!("phase657 injected after trigger drop"))
        })
        .unwrap_err();
        assert!(chain_contains(
            &error,
            "phase657 injected after trigger drop"
        ));
        assert_eq!(signature_audience_schema(&db).unwrap(), before_schema);
        assert_eq!(rows(&db), before_rows);
        assert_eq!(registered_dependency_snapshot(&db), dependencies);
        assert_registered_trigger_definitions_and_behavior(&db, &trigger_definitions);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(db
            .pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
            .unwrap());
        assert!(db.is_autocommit());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM jobs_managed_cloud_signature_sets",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn production_entry_installs_then_verifies_on_restart_and_refuses_missing_marker() {
        let pool = pool();
        {
            let db = pool.get().unwrap();
            insert(&db, 1, OLD[0]).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                    [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        }
        // The proposed registration patch makes this invoke the wrapper.
        run_migrations(&pool).unwrap();
        {
            let db = pool.get().unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                    [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            insert(&db, 100, NEW).unwrap();
        }
        run_migrations(&pool).unwrap();
        {
            let db = pool.get().unwrap();
            assert_eq!(db.query_row("SELECT count(*) FROM jobs_managed_cloud_signature_sets WHERE target_audience=?1", [NEW], |r| r.get::<_,i64>(0)).unwrap(), 1);
            // Deliberately malformed ledger in disposable test database only.
            db.execute(
                "DELETE FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
            )
            .unwrap();
        }
        assert!(run_migrations(&pool).is_err());
        let db = pool.get().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM jobs_managed_cloud_signature_sets WHERE target_audience=?1",
                [NEW],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn production_entry_rejects_unknown_schema_object_on_restart() {
        let pool = pool();
        run_migrations(&pool).unwrap();
        {
            let db = pool.get().unwrap();
            db.execute_batch("CREATE INDEX phase657_restart_unknown ON jobs_managed_cloud_signature_sets(recorded_by)").unwrap();
        }
        assert!(run_migrations(&pool).is_err());
        let db = pool.get().unwrap();
        assert!(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='phase657_restart_unknown')", [], |r| r.get::<_,bool>(0)).unwrap());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn production_entry_rejects_extra_direct_trigger_without_durable_mutation() {
        let pool = pool();
        run_migrations(&pool).unwrap();
        {
            let db = pool.get().unwrap();
            db.execute_batch(
                "CREATE TRIGGER phase657_restart_direct_trigger
                 AFTER INSERT ON jobs_managed_cloud_signature_sets
                 BEGIN SELECT RAISE(ABORT, 'phase657 direct trigger'); END;",
            )
            .unwrap();
        }
        let (schema_before, ledger_before, foreign_keys_before, autocommit_before) = {
            let db = pool.get().unwrap();
            let mut statement = db
                .prepare(
                    "SELECT name,applied_at FROM bluey_data_migrations ORDER BY name,applied_at",
                )
                .unwrap();
            let ledger = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (
                signature_audience_schema(&db).unwrap(),
                ledger,
                db.pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
                    .unwrap(),
                db.is_autocommit(),
            )
        };
        let error = run_migrations(&pool).expect_err("direct trigger must fail closed on replay");
        assert!(chain_contains(&error, "signature audience schema mismatch"));
        let db = pool.get().unwrap();
        let mut statement = db
            .prepare("SELECT name,applied_at FROM bluey_data_migrations ORDER BY name,applied_at")
            .unwrap();
        let ledger_after = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(signature_audience_schema(&db).unwrap(), schema_before);
        assert_eq!(ledger_after, ledger_before);
        assert_eq!(
            db.pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
                .unwrap(),
            foreign_keys_before,
        );
        assert_eq!(db.is_autocommit(), autocommit_before);
        assert!(db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master
                  WHERE type='trigger' AND name='phase657_restart_direct_trigger')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .unwrap());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM bluey_data_migrations WHERE name=?1",
                [SQLITE_SIGNATURE_AUDIENCE_MARKER],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
    }
}

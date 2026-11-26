// UNRUN owned-PostgreSQL tests for the selected-ledger V2 draft and the
// initialized-schema entry check. This is a future child module of db/mod.rs.
// It is storage coverage only, not CLI all-skipped or pool-discard proof.

#[cfg(test)]
mod phase657_postgres_selected_ledger_tests_v1 {
    use super::*;

    const CASE_SELECTED: &str = "db::phase657_postgres_selected_ledger_tests_v1::postgres_owned_selected_ledger_replays_fixed_migrations_when_configured";
    const CASE_CANONICAL_ABSENT: &str = "db::phase657_postgres_selected_ledger_tests_v1::postgres_owned_selected_ledger_allows_absent_distinct_canonical_when_configured";
    const CASE_REFUSAL: &str = "db::phase657_postgres_selected_ledger_tests_v1::postgres_owned_selected_ledger_and_entry_refuse_malformed_state_when_configured";
    const V036: &str = "036_jobs_signed_job_integrity_authority.sql";
    const V040: &str = "040_jobs_original_source_verifier_startup_authority.sql";
    const V045: &str = "045_jobs_provisioned_runtime_startup.sql";

    fn owned_url(case: &str) -> Option<String> {
        if std::env::args().skip(1).eq([
            case.to_owned(),
            "--exact".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]) {
            assert_eq!(
                std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref(),
                Ok("1"),
                "owned selected-ledger wrapper required"
            );
        }
        crate::db::owned_postgres_test::owned_url(case)
    }

    fn fresh_before(url: &str, version: &str) -> DbPool {
        let pool = crate::db::open_postgres_pool(url).expect("owned pool");
        let rows = super::POSTGRES_POST_JOBS_MIGRATIONS.to_vec();
        let at = rows
            .iter()
            .position(|(id, _)| *id == version)
            .expect("registered version");
        super::run_postgres_migration_list(&pool, &rows[..at]).expect("actual registered prefix");
        pool
    }

    fn custom_ledger(client: &mut postgres::Client, name: &MigrationLedgerName) {
        client.batch_execute(&format!("CREATE TABLE {} (version TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())", name.qualified_public_identifier())).expect("owned selected ledger");
    }

    fn rows11<C: postgres::GenericClient>(client: &mut C) -> Vec<Vec<String>> {
        client.query("SELECT signature_set_sha256::text,signature_set_id::text,trust_generation::text,role::text,target_audience::text,target_sha256::text,signed_at_ms::text,signature_count::text,canonical_signature_set_base64url::text,recorded_by::text,recorded_at_ms::text FROM public.jobs_managed_cloud_signature_sets ORDER BY signature_set_sha256", &[]).expect("eleven-field rows").into_iter().map(|row| (0..11).map(|index| row.get(index)).collect()).collect()
    }

    fn history<C: postgres::GenericClient>(
        client: &mut C,
        ledger: &MigrationLedgerName,
    ) -> Vec<(String, String)> {
        client
            .query(
                &format!(
                    "SELECT version,applied_at::text FROM {} ORDER BY version",
                    ledger.qualified_public_identifier()
                ),
                &[],
            )
            .expect("ledger history")
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect()
    }

    fn add_legacy(client: &mut postgres::Client) {
        client.execute("INSERT INTO public.jobs_managed_cloud_signature_sets (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,recorded_by,recorded_at_ms) VALUES ($1,'phase657-selected',1,'root','bluey-jobs-managed-cloud-release-v1',$2,1,1,'fixture','fixture',1)", &[&"a".repeat(64), &"b".repeat(64)]).expect("legacy row");
    }

    #[test]
    fn history_shape_finite_lower_bounds() {
        use SignatureAudienceShape::{Eight, Fourteen, Six, Sixteen, Ten, Thirteen};
        let shapes = [Six, Eight, Ten, Thirteen, Fourteen, Sixteen];
        for (version, expected) in [
            (V036, [false, true, true, true, true, true]),
            (V040, [false, false, true, true, true, true]),
            (V045, [false, false, false, true, true, true]),
            (PHASE657_049, [false, false, false, false, true, true]),
            // 052 uses the stricter canonical-history equality, not this
            // legacy lower-bound rule.
            (
                JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES_MIGRATION_ID,
                [false; 6],
            ),
            ("unknown", [false; 6]),
        ] {
            for (shape, allowed) in shapes.into_iter().zip(expected) {
                assert_eq!(
                    history_shape_allowed(version, shape),
                    allowed,
                    "{version} {shape:?}"
                );
            }
        }
        use SelectedSignatureAudienceMigration::{
            Forward049, Forward052, Replay036, Replay040, Replay045,
        };
        for kind in [Replay036, Replay040, Replay045, Forward049, Forward052] {
            assert_eq!(replay_shape_expected(kind, Sixteen).unwrap(), Sixteen);
        }
        for older in [Six, Eight, Ten, Thirteen] {
            assert!(replay_shape_expected(Forward052, older).is_err());
        }
        assert_eq!(
            replay_shape_expected(Forward052, Fourteen).unwrap(),
            Sixteen
        );
        println!("phase660_postgres_audience_shape_matrix_complete");
    }

    #[test]
    fn postgres_owned_selected_ledger_replays_fixed_migrations_when_configured() {
        let Some(url) = owned_url(CASE_SELECTED) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = fresh_before(&url, V036);
        let mut client = pool.get_pg().expect("owned client");
        let selected = MigrationLedgerName::parse("SELECT").expect("reserved selected identifier");
        custom_ledger(&mut client, &selected);
        add_legacy(&mut client);
        for command in [
            SelectedSignatureAudienceMigration::Replay036,
            SelectedSignatureAudienceMigration::Replay040,
            SelectedSignatureAudienceMigration::Replay045,
            SelectedSignatureAudienceMigration::Forward049,
        ] {
            execute_selected_signature_audience_migration(&mut client, command, &selected)
                .expect("fixed selected-ledger body");
        }
        let rows_before_retry = rows11(&mut **client);
        let selected_history = history(&mut **client, &selected);
        let canonical = MigrationLedgerName::parse("bluey_schema_migrations").unwrap();
        assert!(
            selected_history.iter().any(|(version, _)| version == V045),
            "045 records selected ledger"
        );
        assert!(
            !history(&mut **client, &canonical)
                .iter()
                .any(|(version, _)| version == V045),
            "045 must not write canonical ledger"
        );
        for command in [
            SelectedSignatureAudienceMigration::Replay036,
            SelectedSignatureAudienceMigration::Replay040,
            SelectedSignatureAudienceMigration::Replay045,
            SelectedSignatureAudienceMigration::Forward049,
        ] {
            execute_selected_signature_audience_migration(&mut client, command, &selected)
                .expect("exact replay");
        }
        assert_eq!(
            rows11(&mut **client),
            rows_before_retry,
            "all eleven signature fields preserved"
        );
        assert_eq!(
            history(&mut **client, &selected),
            selected_history,
            "selected history timestamps preserved"
        );
        println!("phase657_owned_postgres_selected_ledger_fixed_replay_complete");
    }

    #[test]
    fn postgres_owned_selected_ledger_allows_absent_distinct_canonical_when_configured() {
        let Some(url) = owned_url(CASE_CANONICAL_ABSENT) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = fresh_before(&url, V036);
        let mut client = pool.get_pg().expect("owned client");
        let selected = MigrationLedgerName::parse("phase657_selected_ledger").unwrap();
        custom_ledger(&mut client, &selected);
        let canonical_oid: u32 = client
            .query_one(
                "SELECT 'public.bluey_schema_migrations'::regclass::oid",
                &[],
            )
            .expect("canonical oid")
            .get(0);
        let canonical_rows = history(
            &mut **client,
            &MigrationLedgerName::parse("bluey_schema_migrations").unwrap(),
        );
        client.batch_execute("ALTER TABLE public.bluey_schema_migrations RENAME TO phase657_canonical_history_saved").expect("rename canonical only in owned DB");
        execute_selected_signature_audience_migration(
            &mut client,
            SelectedSignatureAudienceMigration::Replay036,
            &selected,
        )
        .expect("selected works without distinct canonical");
        assert_eq!(
            client
                .query_one(
                    "SELECT 'public.phase657_canonical_history_saved'::regclass::oid",
                    &[]
                )
                .expect("renamed oid")
                .get::<_, u32>(0),
            canonical_oid
        );
        assert_eq!(
            history(
                &mut **client,
                &MigrationLedgerName::parse("phase657_canonical_history_saved").unwrap()
            ),
            canonical_rows
        );
        println!("phase657_owned_postgres_selected_ledger_canonical_absent_complete");
    }

    #[test]
    fn postgres_owned_selected_ledger_and_entry_refuse_malformed_state_when_configured() {
        let Some(url) = owned_url(CASE_REFUSAL) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = fresh_before(&url, V036);
        let mut client = pool.get_pg().expect("owned client");
        let selected = MigrationLedgerName::parse("phase657_bad_ledger").unwrap();
        client.batch_execute("CREATE TABLE public.phase657_bad_ledger (version TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())").expect("malformed selected fixture");
        let rows_before = rows11(&mut **client);
        let canonical = MigrationLedgerName::parse("bluey_schema_migrations").unwrap();
        let history_before = history(&mut **client, &canonical);
        let error = execute_selected_signature_audience_migration(
            &mut client,
            SelectedSignatureAudienceMigration::Replay036,
            &selected,
        )
        .expect_err("catalog-mismatched selected ledger must refuse");
        assert!(format!("{error:#}").contains("migration ledger schema mismatch"));
        client.mark_broken();
        drop(client);
        let mut client = pool
            .get_pg()
            .expect("fresh checkout after selected-driver error");
        assert_eq!(rows11(&mut **client), rows_before);
        assert_eq!(history(&mut **client, &canonical), history_before);

        // Actual legacy-8 with a claimed 045 must refuse both entry and body,
        // regardless of which independently validated ledger contains the claim.
        execute_selected_signature_audience_migration(
            &mut client,
            SelectedSignatureAudienceMigration::Replay036,
            &canonical,
        )
        .expect("establish actual legacy-8");
        let cross = MigrationLedgerName::parse("phase657_cross_history").unwrap();
        custom_ledger(&mut client, &cross);
        add_legacy(&mut client);
        for bad_ledger in [&cross, &canonical] {
            client
                .execute(
                    &format!(
                        "INSERT INTO {}(version) VALUES ($1)",
                        bad_ledger.qualified_public_identifier()
                    ),
                    &[&V045],
                )
                .expect("inconsistent history fixture");
            let signature_rows = rows11(&mut **client);
            let canonical_history = history(&mut **client, &canonical);
            let cross_history = history(&mut **client, &cross);
            let schema: String = client.query_one(
                "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='public.jobs_managed_cloud_signature_sets'::regclass AND conname='jobs_managed_cloud_signature_sets_target_audience_check'",
                &[],
            ).expect("audience constraint").get(0);
            for entry in [true, false] {
                let error = if entry {
                    verify_initialized_signature_audience_operator_entry(&mut client, &cross)
                } else {
                    execute_selected_signature_audience_migration(
                        &mut client,
                        SelectedSignatureAudienceMigration::Replay040,
                        &cross,
                    )
                }
                .expect_err("claimed 045 on legacy-8 must refuse before body");
                assert!(format!("{error:#}")
                    .contains("PostgreSQL audience legacy history/state mismatch"));
                client.mark_broken();
                drop(client);
                client = pool.get_pg().expect("fresh checkout after refusal");
                assert_eq!(rows11(&mut **client), signature_rows);
                assert_eq!(history(&mut **client, &canonical), canonical_history);
                assert_eq!(history(&mut **client, &cross), cross_history);
                assert_eq!(client.query_one(
                    "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='public.jobs_managed_cloud_signature_sets'::regclass AND conname='jobs_managed_cloud_signature_sets_target_audience_check'",
                    &[],
                ).expect("unchanged constraint").get::<_, String>(0), schema);
                assert!(client.query_one(
                    "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NULL", &[],
                ).expect("state absence").get::<_, bool>(0));
            }
            client
                .execute(
                    &format!(
                        "DELETE FROM {} WHERE version=$1",
                        bad_ledger.qualified_public_identifier()
                    ),
                    &[&V045],
                )
                .expect("remove only injected history");
        }
        verify_initialized_signature_audience_operator_entry(&mut client, &cross)
            .expect("lagging empty selected ledger is permitted");

        let before_entry_rows = rows11(&mut **client);
        let before_entry_history = history(&mut **client, &canonical);
        let valid_selected = MigrationLedgerName::parse("phase657_valid_entry").unwrap();
        custom_ledger(&mut client, &valid_selected);
        for command in [
            SelectedSignatureAudienceMigration::Replay036,
            SelectedSignatureAudienceMigration::Replay040,
            SelectedSignatureAudienceMigration::Replay045,
            SelectedSignatureAudienceMigration::Forward049,
        ] {
            execute_selected_signature_audience_migration(&mut client, command, &valid_selected)
                .expect("record all fixed versions before entry");
        }
        let selected_history_before_entry = history(&mut **client, &valid_selected);
        assert_eq!(
            selected_history_before_entry.len(),
            4,
            "all fixed versions recorded in selected ledger"
        );
        let state_before: Vec<(i16, i16)> = client
            .query(
                "SELECT singleton,version
                   FROM public.jobs_signature_audience_upgrade_state
                  ORDER BY singleton",
                &[],
            )
            .expect("singleton state before malformed fixture")
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        assert_eq!(state_before, vec![(1_i16, 1_i16)]);
        assert_eq!(
            client
                .execute(
                    "DELETE FROM public.jobs_signature_audience_upgrade_state WHERE singleton=1",
                    &[],
                )
                .expect("malformed shared state fixture"),
            1,
            "fixture deletes exactly the singleton"
        );
        let malformed_rows = rows11(&mut **client);
        let malformed_history = history(&mut **client, &canonical);
        let entry_error =
            verify_initialized_signature_audience_operator_entry(&mut client, &valid_selected)
                .expect_err("malformed shared state must refuse entry");
        assert!(format!("{entry_error:#}").contains("signature audience state mismatch"));
        client.mark_broken();
        drop(client);
        let mut client = pool.get_pg().expect("fresh checkout after entry error");
        assert_eq!(
            rows11(&mut **client),
            malformed_rows,
            "entry makes no durable writes"
        );
        assert_eq!(history(&mut **client, &canonical), malformed_history);
        assert_eq!(
            before_entry_rows, malformed_rows,
            "fixed migrations do not rewrite signature rows"
        );
        assert_eq!(
            before_entry_history, malformed_history,
            "fixture leaves history unchanged"
        );
        assert_eq!(
            history(&mut **client, &valid_selected),
            selected_history_before_entry
        );
        assert!(
            client
                .query_one(
                    "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NOT NULL",
                    &[],
                )
                .expect("state relation remains after refused entry")
                .get::<_, bool>(0),
            "entry refusal must not repair or drop durable state relation"
        );
        let state_after: Vec<(i16, i16)> = client
            .query(
                "SELECT singleton,version
                   FROM public.jobs_signature_audience_upgrade_state
                  ORDER BY singleton",
                &[],
            )
            .expect("state rows after refused entry")
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        assert!(
            state_after.is_empty(),
            "entry refusal must not repair state rows"
        );
        println!("phase657_owned_postgres_selected_ledger_refusal_complete");
    }
}

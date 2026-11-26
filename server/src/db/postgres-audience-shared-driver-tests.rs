// Phase657 integrated source candidate; full integrated verification is pending.
// Schema/transaction logic here is not signature, serving, or effect authority.
#[cfg(test)]
mod phase657_postgres_audience_shared_driver_tests_v3 {
    use super::*;

    const CASE_BAD: &str = "db::phase657_postgres_audience_shared_driver_tests_v3::postgres_owned_audience_shared_driver_rejects_recorded_legacy_and_retries_when_configured";
    const CASE_CUSTOM: &str = "db::phase657_postgres_audience_shared_driver_tests_v3::postgres_owned_audience_shared_driver_preserves_custom_ledger_when_configured";
    const CANONICAL: &str = "049_jobs_process_attachment_signature_audience.sql";
    const SUCCESSOR: &str = "bluey-jobs-managed-cloud-process-attachment-v1";
    const SETUP: &str = include_str!("postgres-audience-shared-setup.sql");
    const VALIDATE: &str = include_str!("postgres-audience-shared-validate.sql");
    const FORWARD: &str = include_str!("postgres-audience-shared-forward.sql");

    #[derive(Debug, PartialEq, Eq)]
    struct SignatureRow(
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

    fn owned(case: &str) -> Option<String> {
        if std::env::args().skip(1).eq([
            case.to_owned(),
            "--exact".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]) {
            assert_eq!(
                std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref(),
                Ok("1")
            );
        }
        crate::db::owned_postgres_test::owned_url(case)
    }

    fn pre049_pool(url: &str) -> DbPool {
        let pool = crate::db::open_postgres_pool(url).expect("owned pool");
        let mut client = pool.get_pg().expect("owned client");
        assert!(client
            .query_one(
                "SELECT to_regclass('public.bluey_schema_migrations') IS NULL",
                &[],
            )
            .expect("fresh query")
            .get::<_, bool>(0));
        drop(client);
        let list = super::POSTGRES_POST_JOBS_MIGRATIONS;
        let cutoff = list
            .iter()
            .position(|(version, _)| *version == CANONICAL)
            .expect("049 remains registered");
        assert!(cutoff > 0, "actual pre-049 list must not be empty");
        super::run_postgres_migration_list(&pool, &list[..cutoff]).expect("actual pre-049 list");
        let mut client = pool.get_pg().expect("pre-049 client");
        assert!(marker(&mut **client, "public.bluey_schema_migrations", CANONICAL).is_none());
        drop(client);
        pool
    }

    fn marker<C: postgres::GenericClient>(
        client: &mut C,
        table: &str,
        version: &str,
    ) -> Option<(String, String)> {
        let sql = match table {
            "public.bluey_schema_migrations" => "SELECT version,applied_at::text FROM public.bluey_schema_migrations WHERE version=$1",
            "public.phase657_custom_migrations" => "SELECT version,applied_at::text FROM public.phase657_custom_migrations WHERE version=$1",
            _ => panic!("closed fixture ledger name"),
        };
        client
            .query_opt(sql, &[&version])
            .expect("ledger marker")
            .map(|row| (row.get(0), row.get(1)))
    }

    fn custom_rows<C: postgres::GenericClient>(client: &mut C) -> Vec<(String, String)> {
        client.query(
            "SELECT version,applied_at::text FROM public.phase657_custom_migrations ORDER BY version", &[],
        ).expect("all custom ledger rows").into_iter().map(|row| (row.get(0), row.get(1))).collect()
    }

    fn signature_rows<C: postgres::GenericClient>(client: &mut C) -> Vec<SignatureRow> {
        client.query(
            "SELECT signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
                    target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
                    recorded_by,recorded_at_ms FROM public.jobs_managed_cloud_signature_sets
              WHERE signature_set_id='phase657-shared-driver-legacy'", &[],
        ).expect("eleven signature fields").into_iter().map(|r| SignatureRow(
            r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5), r.get(6),
            r.get(7), r.get(8), r.get(9), r.get(10),
        )).collect()
    }

    fn constraint<C: postgres::GenericClient>(client: &mut C) -> String {
        client
            .query_one(
                "SELECT pg_catalog.pg_get_constraintdef(oid) FROM pg_catalog.pg_constraint
              WHERE conrelid='public.jobs_managed_cloud_signature_sets'::regclass
                AND conname='jobs_managed_cloud_signature_sets_target_audience_check'",
                &[],
            )
            .expect("constraint")
            .get(0)
    }

    fn state<C: postgres::GenericClient>(client: &mut C) -> Option<(i16, i16)> {
        client
            .query_opt(
                "SELECT singleton,version FROM public.jobs_signature_audience_upgrade_state",
                &[],
            )
            .expect("state")
            .map(|r| (r.get(0), r.get(1)))
    }

    fn insert_legacy<C: postgres::GenericClient>(client: &mut C) {
        client
            .execute(
                "INSERT INTO public.jobs_managed_cloud_signature_sets
             (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
              target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
              recorded_by,recorded_at_ms)
             VALUES ($1,'phase657-shared-driver-legacy',1,'root',
                     'bluey-jobs-managed-cloud-release-v1',$2,1,1,'schema-only','fixture',1)",
                &[&"a".repeat(64), &"b".repeat(64)],
            )
            .expect("legacy row");
    }

    fn validate(tx: &mut postgres::Transaction<'_>) -> Result<String, postgres::Error> {
        tx.batch_execute(VALIDATE)?;
        Ok(tx
            .query_one(
                "SELECT state FROM pg_temp.phase657_audience_guard_result",
                &[],
            )?
            .get(0))
    }

    fn custom_forward(client: &mut postgres::Client) {
        let mut tx = client
            .transaction()
            .expect("custom-ledger forward transaction");
        tx.batch_execute(
            "CREATE TABLE public.phase657_custom_migrations (
             version text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())",
        )
        .expect("fixed synthetic custom ledger");
        tx.batch_execute(SETUP).expect("guard setup");
        assert_eq!(validate(&mut tx).expect("legacy validate"), "legacy13");
        tx.batch_execute(FORWARD).expect("actual forward");
        assert_eq!(
            validate(&mut tx).expect("successor validate"),
            "successor14"
        );
        tx.execute(
            "INSERT INTO public.phase657_custom_migrations(version) VALUES ($1)",
            &[&CANONICAL],
        )
        .expect("custom ledger canonical marker");
        tx.commit().expect("commit custom-ledger forward");
    }

    fn reusable(client: &mut postgres::Client) {
        assert_eq!(
            client
                .query_one("SELECT 1", &[])
                .expect("reusable client")
                .get::<_, i32>(0),
            1
        );
    }

    #[test]
    fn postgres_owned_audience_shared_driver_rejects_recorded_legacy_and_retries_when_configured() {
        let Some(url) = owned(CASE_BAD) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = pre049_pool(&url);
        let mut client = pool.get_pg().expect("client");
        let old_constraint = constraint(&mut **client);
        insert_legacy(&mut **client);
        let old_rows = signature_rows(&mut **client);
        client
            .execute(
                "INSERT INTO public.bluey_schema_migrations(version) VALUES ($1)",
                &[&CANONICAL],
            )
            .expect("synthetic bad marker");
        let bad_marker = marker(&mut **client, "public.bluey_schema_migrations", CANONICAL);
        let error = super::execute_postgres_signature_audience_049(&mut client)
            .expect_err("legacy marker mismatch");
        assert!(format!("{error:#}").contains("history/state mismatch"));
        assert_eq!(constraint(&mut **client), old_constraint);
        assert_eq!(signature_rows(&mut **client), old_rows);
        assert!(
            client
                .query_one(
                    "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NULL",
                    &[],
                )
                .expect("durable audience state relation lookup")
                .get::<_, bool>(0),
            "recorded legacy mismatch must leave the durable state relation absent"
        );
        assert_eq!(
            marker(&mut **client, "public.bluey_schema_migrations", CANONICAL),
            bad_marker
        );
        reusable(&mut client);
        client
            .execute(
                "DELETE FROM public.bluey_schema_migrations WHERE version=$1",
                &[&CANONICAL],
            )
            .expect("remove synthetic bad marker");
        super::execute_postgres_signature_audience_049(&mut client).expect("first actual install");
        let installed_rows = signature_rows(&mut **client);
        let installed_state = state(&mut **client);
        let canonical = marker(&mut **client, "public.bluey_schema_migrations", CANONICAL)
            .expect("canonical history");
        assert_eq!(installed_rows, old_rows);
        assert_eq!(installed_state, Some((1, 1)));
        super::execute_postgres_signature_audience_049(&mut client).expect("exact retry");
        assert_eq!(signature_rows(&mut **client), installed_rows);
        assert_eq!(state(&mut **client), installed_state);
        assert_eq!(
            marker(&mut **client, "public.bluey_schema_migrations", CANONICAL),
            Some(canonical)
        );
        println!("phase657_owned_postgres_audience_shared_driver_bad_history_retry_complete");
    }

    #[test]
    fn postgres_owned_audience_shared_driver_preserves_custom_ledger_when_configured() {
        let Some(url) = owned(CASE_CUSTOM) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = pre049_pool(&url);
        let mut client = pool.get_pg().expect("client");
        insert_legacy(&mut **client);
        let old_rows = signature_rows(&mut **client);
        custom_forward(&mut client);
        let custom_oid: i64 = client
            .query_one(
                "SELECT 'public.phase657_custom_migrations'::regclass::oid::bigint",
                &[],
            )
            .expect("custom ledger oid")
            .get(0);
        let custom = marker(
            &mut **client,
            "public.phase657_custom_migrations",
            CANONICAL,
        )
        .expect("custom canonical marker");
        let custom_before = custom_rows(&mut **client);
        let successor_constraint = constraint(&mut **client);
        let successor_state = state(&mut **client);
        let successor_rows = signature_rows(&mut **client);
        assert!(successor_constraint.contains(SUCCESSOR));
        assert_eq!(successor_rows, old_rows);
        assert!(marker(&mut **client, "public.bluey_schema_migrations", CANONICAL).is_none());

        super::execute_postgres_signature_audience_049(&mut client)
            .expect("canonical history only");
        let canonical = marker(&mut **client, "public.bluey_schema_migrations", CANONICAL)
            .expect("server canonical history");
        assert_eq!(constraint(&mut **client), successor_constraint);
        assert_eq!(state(&mut **client), successor_state);
        assert_eq!(signature_rows(&mut **client), successor_rows);
        assert_eq!(
            client
                .query_one(
                    "SELECT 'public.phase657_custom_migrations'::regclass::oid::bigint",
                    &[]
                )
                .expect("custom oid")
                .get::<_, i64>(0),
            custom_oid
        );
        assert_eq!(
            marker(
                &mut **client,
                "public.phase657_custom_migrations",
                CANONICAL
            ),
            Some(custom.clone())
        );
        assert_eq!(custom_rows(&mut **client), custom_before);
        super::execute_postgres_signature_audience_049(&mut client).expect("exact retry");
        assert_eq!(constraint(&mut **client), successor_constraint);
        assert_eq!(state(&mut **client), successor_state);
        assert_eq!(signature_rows(&mut **client), successor_rows);
        assert_eq!(
            marker(
                &mut **client,
                "public.phase657_custom_migrations",
                CANONICAL
            ),
            Some(custom)
        );
        assert_eq!(custom_rows(&mut **client), custom_before);
        assert_eq!(
            client
                .query_one(
                    "SELECT 'public.phase657_custom_migrations'::regclass::oid::bigint",
                    &[]
                )
                .expect("custom retry oid")
                .get::<_, i64>(0),
            custom_oid
        );
        assert_eq!(
            marker(&mut **client, "public.bluey_schema_migrations", CANONICAL),
            Some(canonical)
        );
        reusable(&mut client);
        println!("phase657_owned_postgres_audience_shared_driver_custom_ledger_complete");
    }
}

// Phase657 integrated source candidate; full integrated verification is pending.
// Schema/transaction logic here is not signature, serving, or effect authority.
#[cfg(test)]
mod phase657_postgres_audience_replay_tests_v1 {
    use super::*;

    const CASE_REPLAY: &str = "db::phase657_postgres_audience_replay_tests_v1::postgres_owned_signature_audience_historical_replay_preserves_successor_when_configured";
    const CASE_ROLLBACK: &str = "db::phase657_postgres_audience_replay_tests_v1::postgres_owned_signature_audience_replay_refuses_malformed_state_prebody_when_configured";
    const CASE_LATE_LEDGER_ROLLBACK: &str = "db::phase657_postgres_audience_replay_tests_v1::postgres_owned_signature_audience_049_late_ledger_failure_rolls_back_when_configured";
    const V036: &str = "036_jobs_signed_job_integrity_authority.sql";
    const V040: &str = "040_jobs_original_source_verifier_startup_authority.sql";
    const V045: &str = "045_jobs_provisioned_runtime_startup.sql";
    const V049: &str = "049_jobs_process_attachment_signature_audience.sql";

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

    fn owned_url(case: &str) -> Option<String> {
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

    fn before(version: &str) -> Vec<(&'static str, &'static str)> {
        let rows: Vec<_> = super::POSTGRES_POST_JOBS_MIGRATIONS.to_vec();
        let position = rows
            .iter()
            .position(|(id, _)| *id == version)
            .expect("closed replay version remains registered");
        rows[..position].to_vec()
    }

    fn fresh_before(url: &str, version: &str) -> DbPool {
        let pool = crate::db::open_postgres_pool(url).expect("owned pool");
        let mut client = pool.get_pg().expect("owned client");
        assert!(client
            .query_one(
                "SELECT to_regclass('public.bluey_schema_migrations') IS NULL",
                &[]
            )
            .expect("fresh check")
            .get::<_, bool>(0));
        drop(client);
        let list = before(version);
        assert!(
            !list.is_empty(),
            "actual registered prefix must not be empty"
        );
        super::run_postgres_migration_list(&pool, &list).expect("actual registered prefix");
        pool
    }

    fn audience_count<C: postgres::GenericClient>(client: &mut C) -> i32 {
        client
            .query_one(
                "SELECT cardinality(regexp_split_to_array(
                 pg_catalog.pg_get_constraintdef(oid), '''bluey-jobs-managed-cloud-[^'']+''')) - 1
               FROM pg_catalog.pg_constraint
              WHERE conrelid='public.jobs_managed_cloud_signature_sets'::regclass
                AND conname='jobs_managed_cloud_signature_sets_target_audience_check'",
                &[],
            )
            .expect("audience count")
            .get(0)
    }

    fn apply_registered_between(pool: &DbPool, after: &str, before: &str) {
        let rows = super::POSTGRES_POST_JOBS_MIGRATIONS.to_vec();
        let after_index = rows
            .iter()
            .position(|(id, _)| *id == after)
            .expect("after version remains registered");
        let before_index = rows
            .iter()
            .position(|(id, _)| *id == before)
            .expect("before version remains registered");
        assert!(after_index < before_index, "registered transition ordering");
        super::run_postgres_migration_list(pool, &rows[after_index + 1..before_index])
            .expect("actual intervening registered migrations");
    }

    fn signature_rows<C: postgres::GenericClient>(client: &mut C) -> Vec<SignatureRow> {
        client.query(
            "SELECT signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
                    target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
                    recorded_by,recorded_at_ms FROM public.jobs_managed_cloud_signature_sets
              WHERE signature_set_id LIKE 'phase657-replay-%'
              ORDER BY signature_set_sha256", &[],
        ).expect("eleven-field snapshot").into_iter().map(|r| SignatureRow(
            r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5), r.get(6),
            r.get(7), r.get(8), r.get(9), r.get(10),
        )).collect()
    }

    fn all_signature_rows<C: postgres::GenericClient>(client: &mut C) -> Vec<SignatureRow> {
        client.query(
            "SELECT signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
                    target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
                    recorded_by,recorded_at_ms FROM public.jobs_managed_cloud_signature_sets
              ORDER BY signature_set_sha256", &[],
        ).expect("complete eleven-field signature snapshot").into_iter().map(|r| SignatureRow(
            r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5), r.get(6),
            r.get(7), r.get(8), r.get(9), r.get(10),
        )).collect()
    }

    fn history<C: postgres::GenericClient>(
        client: &mut C,
        version: &str,
    ) -> Option<(String, String)> {
        client.query_opt(
            "SELECT version,applied_at::text FROM public.bluey_schema_migrations WHERE version=$1", &[&version],
        ).expect("history row").map(|row| (row.get(0), row.get(1)))
    }

    fn all_history<C: postgres::GenericClient>(client: &mut C) -> Vec<(String, String)> {
        client
            .query(
                "SELECT version,applied_at::text FROM public.bluey_schema_migrations ORDER BY version",
                &[],
            )
            .expect("complete history snapshot")
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect()
    }

    fn singleton_state<C: postgres::GenericClient>(client: &mut C) -> Vec<(i16, i16)> {
        client
            .query(
                "SELECT singleton,version
                   FROM public.jobs_signature_audience_upgrade_state
                  ORDER BY singleton",
                &[],
            )
            .expect("exact signature audience singleton state")
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect()
    }

    fn audience_constraint<C: postgres::GenericClient>(client: &mut C) -> String {
        client
            .query_one(
                "SELECT pg_catalog.pg_get_constraintdef(oid)
                   FROM pg_catalog.pg_constraint
                  WHERE conrelid='public.jobs_managed_cloud_signature_sets'::regclass
                    AND conname='jobs_managed_cloud_signature_sets_target_audience_check'",
                &[],
            )
            .expect("signature audience constraint")
            .get(0)
    }

    fn state_relation_absent<C: postgres::GenericClient>(client: &mut C) {
        assert!(
            client
                .query_one(
                    "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NULL",
                    &[],
                )
                .expect("state relation check")
                .get::<_, bool>(0),
            "late ledger failure must roll back durable upgrade state"
        );
    }

    fn insert_legacy(client: &mut postgres::Client) {
        client
            .execute(
                "INSERT INTO public.jobs_managed_cloud_signature_sets
             (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
              target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
              recorded_by,recorded_at_ms)
             VALUES ($1,'phase657-replay-legacy',1,'root',
                     'bluey-jobs-managed-cloud-release-v1',$2,1,1,'schema-only','fixture',1)",
                &[&"a".repeat(64), &"b".repeat(64)],
            )
            .expect("real registered-table legacy row");
    }

    fn insert_successor_attachment(client: &mut postgres::Client) {
        client
            .execute(
                "INSERT INTO public.jobs_managed_cloud_signature_sets
             (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
              target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
              recorded_by,recorded_at_ms)
             VALUES ($1,'phase657-replay-successor',1,'root',
                     'bluey-jobs-managed-cloud-process-attachment-v1',$2,2,1,
                     'schema-only-successor','fixture',2)",
                &[&"c".repeat(64), &"d".repeat(64)],
            )
            .expect("real successor attachment row");
    }

    fn reusable(client: &mut postgres::Client) {
        assert_eq!(
            client
                .query_one("SELECT 1", &[])
                .expect("reusable connection")
                .get::<_, i32>(0),
            1
        );
    }

    fn prove_v4_late_canonical_write_rolls_back(pool: &DbPool) {
        let mut client = pool.get_pg().expect("pre-v4 failure fixture client");
        let ownership_and_acl = |client: &mut postgres::Client| {
            client
                .query(
                    "SELECT c.relname::text, pg_catalog.pg_get_userbyid(c.relowner)::text,
                            ARRAY(SELECT format('%s/%s/%s/%s',
                                   a.grantor, a.grantee, a.privilege_type, a.is_grantable)
                                    FROM pg_catalog.aclexplode(
                                         coalesce(c.relacl, pg_catalog.acldefault('r', c.relowner))
                                    ) AS a
                                ORDER BY a.grantor, a.grantee, a.privilege_type, a.is_grantable)
                       FROM pg_catalog.pg_class c
                      WHERE c.oid IN ('public.jobs_managed_cloud_signature_sets'::regclass,
                                      'public.jobs_signature_audience_upgrade_state'::regclass,
                                      'public.bluey_schema_migrations'::regclass)
                      ORDER BY c.relname",
                    &[],
                )
                .expect("snapshot owned relation owners and effective ACLs")
                .into_iter()
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, Vec<String>>(2),
                    )
                })
                .collect::<Vec<_>>()
        };
        let privileges_before = ownership_and_acl(&mut client);
        assert_eq!(privileges_before.len(), 3);
        let constraint_before = audience_constraint(&mut **client);
        let state_before = singleton_state(&mut **client);
        let rows_before = all_signature_rows(&mut **client);
        let history_before = all_history(&mut **client);
        let v4 = JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES_MIGRATION_ID;
        assert_eq!(audience_count(&mut **client), 14);
        assert_eq!(state_before, vec![(1, 1)]);
        assert!(history(&mut **client, v4).is_none());

        // The disposable actor owns both relations affected by the migration.
        // SELECT/UPDATE allows the ledger guard to lock and inspect the ledger,
        // but the actor cannot record final history.
        let actor = "phase660_replay_052_writer";
        let quoted: (String, String, String) = {
            let row = client
                .query_one(
                    "SELECT pg_catalog.quote_ident($1), pg_catalog.quote_ident(current_user),
                            pg_catalog.quote_ident(current_database())",
                    &[&actor],
                )
                .expect("quote owned fixture role, owner, and database");
            (row.get(0), row.get(1), row.get(2))
        };
        let (actor_id, owner_id, database_id) = quoted;
        client
            .batch_execute(&format!(
                "CREATE ROLE {actor_id} NOLOGIN NOSUPERUSER NOINHERIT;
                 GRANT USAGE ON SCHEMA public TO {actor_id};
                 GRANT TEMPORARY ON DATABASE {database_id} TO {actor_id};
                 GRANT {actor_id} TO {owner_id};
                 ALTER TABLE public.jobs_managed_cloud_signature_sets OWNER TO {actor_id};
                 ALTER TABLE public.jobs_signature_audience_upgrade_state OWNER TO {actor_id};
                 GRANT SELECT, UPDATE ON TABLE public.bluey_schema_migrations TO {actor_id};"
            ))
            .expect("create disposable v4 late-ledger actor");
        let privileges = client
            .query_one(
                "SELECT NOT rolcanlogin AND NOT rolsuper AND NOT rolinherit
                    AND has_table_privilege($1, 'public.bluey_schema_migrations', 'SELECT')
                    AND has_table_privilege($1, 'public.bluey_schema_migrations', 'UPDATE')
                    AND NOT has_table_privilege($1, 'public.bluey_schema_migrations', 'INSERT')
                    AND pg_catalog.pg_get_userbyid(sig.relowner)=$1
                    AND pg_catalog.pg_get_userbyid(state.relowner)=$1
               FROM pg_catalog.pg_roles, pg_catalog.pg_class sig, pg_catalog.pg_class state
              WHERE rolname=$1
                AND sig.oid='public.jobs_managed_cloud_signature_sets'::regclass
                AND state.oid='public.jobs_signature_audience_upgrade_state'::regclass",
                &[&actor],
            )
            .expect("verify disposable actor can apply DDL but not insert history");
        assert!(privileges.get::<_, bool>(0));
        drop(client);

        let mut actor_client = pool.get_pg().expect("owned v4 actor client");
        actor_client
            .batch_execute(&format!("SET ROLE {actor_id}"))
            .expect("switch to disposable v4 actor");
        let active_role: String = actor_client
            .query_one("SELECT current_user::text", &[])
            .expect("verify active v4 actor")
            .get(0);
        assert_eq!(active_role, actor);
        let error = execute_selected_signature_audience_migration(
            &mut actor_client,
            SelectedSignatureAudienceMigration::Forward052,
            &canonical_migration_ledger(),
        )
        .expect_err("late v4 canonical history write must fail");
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string() == "selected migration ledger write unavailable"),
            "v4 body and catalog postcondition must finish before the denied ledger insert"
        );
        actor_client.mark_broken();
        drop(actor_client);

        let mut client = pool
            .get_pg()
            .expect("fresh privileged v4 post-failure client");
        let active_owner: String = client
            .query_one("SELECT pg_catalog.quote_ident(current_user)", &[])
            .expect("verify post-failure checkout identity")
            .get(0);
        assert_eq!(active_owner, owner_id);
        assert_eq!(audience_count(&mut **client), 14);
        assert_eq!(audience_constraint(&mut **client), constraint_before);
        assert_eq!(singleton_state(&mut **client), state_before);
        assert_eq!(all_signature_rows(&mut **client), rows_before);
        assert_eq!(all_history(&mut **client), history_before);
        assert!(history(&mut **client, v4).is_none());
        client
            .batch_execute(&format!(
                "ALTER TABLE public.jobs_managed_cloud_signature_sets OWNER TO {owner_id};
                 ALTER TABLE public.jobs_signature_audience_upgrade_state OWNER TO {owner_id};
                 REVOKE SELECT, UPDATE ON TABLE public.bluey_schema_migrations FROM {actor_id};
                 REVOKE TEMPORARY ON DATABASE {database_id} FROM {actor_id};
                 REVOKE USAGE ON SCHEMA public FROM {actor_id};
                 REVOKE {actor_id} FROM {owner_id};
                 DROP ROLE {actor_id};"
            ))
            .expect("close the exact disposable v4 actor");
        assert_eq!(ownership_and_acl(&mut client), privileges_before);
        assert!(client
            .query_one("SELECT pg_catalog.to_regrole($1) IS NULL", &[&actor])
            .expect("verify disposable role absence")
            .get::<_, bool>(0));
        println!("phase660_owned_postgres_052_late_ledger_rollback_complete");
    }

    #[test]
    fn postgres_owned_signature_audience_historical_replay_preserves_successor_when_configured() {
        let Some(url) = owned_url(CASE_REPLAY) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = fresh_before(&url, V036);
        let mut client = pool.get_pg().expect("owned client");
        insert_legacy(&mut client);
        let legacy = signature_rows(&mut **client);
        assert_eq!(legacy.len(), 1);
        assert_eq!(
            audience_count(&mut **client),
            6,
            "registered pre-036 catalog"
        );

        super::execute_postgres_audience_replay(&mut client, V036).expect("actual 036 replay");
        assert_eq!(audience_count(&mut **client), 8);
        let h036 = history(&mut **client, V036).expect("036 history");
        drop(client);
        apply_registered_between(&pool, V036, V040);
        let mut client = pool.get_pg().expect("040 client");
        super::execute_postgres_audience_replay(&mut client, V040).expect("actual 040 replay");
        assert_eq!(audience_count(&mut **client), 10);
        let h040 = history(&mut **client, V040).expect("040 history");
        drop(client);
        apply_registered_between(&pool, V040, V045);
        let mut client = pool.get_pg().expect("045 client");
        super::execute_postgres_audience_replay(&mut client, V045).expect("actual 045 replay");
        assert_eq!(audience_count(&mut **client), 13);
        let h045 = history(&mut **client, V045).expect("045 history");
        drop(client);
        apply_registered_between(&pool, V045, V049);
        let mut client = pool.get_pg().expect("049 client");
        assert_eq!(
            audience_count(&mut **client),
            13,
            "fixture stops before 049"
        );
        assert!(
            history(&mut **client, V049).is_none(),
            "049 is not pre-applied"
        );
        state_relation_absent(&mut **client);
        super::execute_postgres_signature_audience_049(&mut client).expect("actual 049 helper");
        assert_eq!(audience_count(&mut **client), 14);
        let h049 = history(&mut **client, V049).expect("049 history");
        insert_successor_attachment(&mut client);
        let successor_rows = signature_rows(&mut **client);
        assert_eq!(successor_rows.len(), 2);
        assert!(successor_rows.contains(&legacy[0]));
        assert!(successor_rows
            .iter()
            .any(|row| row.4 == "bluey-jobs-managed-cloud-process-attachment-v1"));

        for version in [V036, V040, V045] {
            super::execute_postgres_audience_replay(&mut client, version)
                .expect("actual successor replay");
            assert_eq!(audience_count(&mut **client), 14);
            assert_eq!(signature_rows(&mut **client), successor_rows);
        }
        assert_eq!(history(&mut **client, V036), Some(h036));
        assert_eq!(history(&mut **client, V040), Some(h040));
        assert_eq!(history(&mut **client, V045), Some(h045));
        assert_eq!(history(&mut **client, V049), Some(h049));
        reusable(&mut client);
        drop(client);
        let v4 = JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES_MIGRATION_ID;
        apply_registered_between(&pool, V049, v4);
        let mut client = pool.get_pg().expect("pre-v4 client");
        let before_entry_history = all_history(&mut **client);
        assert!(history(&mut **client, v4).is_none());
        assert_eq!(singleton_state(&mut **client), vec![(1, 1)]);
        let raw = client
            .batch_execute(POSTGRES_JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES)
            .expect_err("the raw 052 file must not run");
        assert_eq!(
            raw.code(),
            Some(&postgres::error::SqlState::RAISE_EXCEPTION)
        );
        assert_eq!(audience_count(&mut **client), 14);
        assert_eq!(all_history(&mut **client), before_entry_history);
        drop(client);
        prove_v4_late_canonical_write_rolls_back(&pool);

        // Exercise the real registered entry, not just the finite helper bodies.
        super::run_migrations(&pool).expect("registered v4 upgrade with stored successor row");
        let mut client = pool.get_pg().expect("post-entry client");
        assert_eq!(audience_count(&mut **client), 16);
        assert_eq!(singleton_state(&mut **client), vec![(1, 2)]);
        assert_eq!(signature_rows(&mut **client), successor_rows);
        let after_entry_history = all_history(&mut **client);
        assert!(history(&mut **client, v4).is_some());
        assert_eq!(
            after_entry_history
                .iter()
                .filter(|(version, _)| version != v4)
                .cloned()
                .collect::<Vec<_>>(),
            before_entry_history
        );
        for (digit, kind) in [('e', "release-v4"), ('f', "activation-v4")] {
            let audience = format!("bluey-jobs-managed-cloud-{kind}");
            client
                .execute(
                    "INSERT INTO public.jobs_managed_cloud_signature_sets
                   (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,
                    target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,
                    recorded_by,recorded_at_ms)
                 VALUES ($1,$2,1,'root',$3,$1,3,1,'schema-only','fixture',3)",
                    &[
                        &digit.to_string().repeat(64),
                        &format!("phase657-replay-{kind}"),
                        &audience,
                    ],
                )
                .expect("v4 audience storage");
        }
        let after_v4_rows = signature_rows(&mut **client);
        assert_eq!(after_v4_rows.len(), successor_rows.len() + 2);
        drop(client);
        super::run_migrations(&pool).expect("historical replay with two v4 rows");
        let mut client = pool.get_pg().expect("post-v4-replay client");
        assert_eq!(audience_count(&mut **client), 16);
        assert_eq!(signature_rows(&mut **client), after_v4_rows);
        assert_eq!(all_history(&mut **client), after_entry_history);
        reusable(&mut client);
        println!("phase657_owned_postgres_signature_audience_historical_replay_complete");
    }

    #[test]
    fn postgres_owned_signature_audience_replay_refuses_malformed_state_prebody_when_configured() {
        let Some(url) = owned_url(CASE_ROLLBACK) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = fresh_before(&url, V036);
        let mut client = pool.get_pg().expect("owned client");
        insert_legacy(&mut client);
        super::execute_postgres_audience_replay(&mut client, V036).expect("036");
        drop(client);
        apply_registered_between(&pool, V036, V040);
        let mut client = pool.get_pg().expect("040 client");
        super::execute_postgres_audience_replay(&mut client, V040).expect("040");
        drop(client);
        apply_registered_between(&pool, V040, V045);
        let mut client = pool.get_pg().expect("045 client");
        super::execute_postgres_audience_replay(&mut client, V045).expect("045");
        drop(client);
        apply_registered_between(&pool, V045, V049);
        let mut client = pool.get_pg().expect("049 client");
        assert_eq!(
            audience_count(&mut **client),
            13,
            "fixture stops before 049"
        );
        assert!(
            history(&mut **client, V049).is_none(),
            "049 is not pre-applied"
        );
        state_relation_absent(&mut **client);
        super::execute_postgres_signature_audience_049(&mut client).expect("049");
        let rows_before = signature_rows(&mut **client);
        let history_before = history(&mut **client, V040);

        // This proves a guard-specific pre-body refusal only. The closed driver
        // has no fault hook after executing a replay body, so it cannot prove a
        // late-DDL rollback without a separately reviewed driver test seam.
        client
            .execute(
                "DELETE FROM public.jobs_signature_audience_upgrade_state WHERE singleton=1",
                &[],
            )
            .expect("malformed registered state fixture");
        let error = super::execute_postgres_audience_replay(&mut client, V040)
            .expect_err("malformed successor state must refuse");
        assert!(format!("{error:#}").contains("signature audience state mismatch"));
        assert!(
            client
                .query_one(
                    "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NOT NULL",
                    &[],
                )
                .expect("state relation remains")
                .get::<_, bool>(0),
            "driver rollback must not remove caller's malformed relation"
        );
        assert_eq!(audience_count(&mut **client), 14);
        assert_eq!(signature_rows(&mut **client), rows_before);
        assert_eq!(history(&mut **client, V040), history_before);
        reusable(&mut client);
        println!("phase657_owned_postgres_signature_audience_replay_rollback_complete");
    }

    #[test]
    fn postgres_owned_signature_audience_049_late_ledger_failure_rolls_back_when_configured() {
        let Some(url) = owned_url(CASE_LATE_LEDGER_ROLLBACK) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let pool = fresh_before(&url, V036);
        let mut client = pool.get_pg().expect("owned client");
        insert_legacy(&mut client);
        super::execute_postgres_audience_replay(&mut client, V036).expect("036");
        drop(client);
        apply_registered_between(&pool, V036, V040);
        let mut client = pool.get_pg().expect("040 client");
        super::execute_postgres_audience_replay(&mut client, V040).expect("040");
        drop(client);
        apply_registered_between(&pool, V040, V045);
        let mut client = pool.get_pg().expect("045 client");
        super::execute_postgres_audience_replay(&mut client, V045).expect("045");
        drop(client);
        apply_registered_between(&pool, V045, V049);
        let mut client = pool.get_pg().expect("049 client");

        let rows_before = all_signature_rows(&mut **client);
        let history_before = all_history(&mut **client);
        let constraint_before = audience_constraint(&mut **client);
        assert_eq!(audience_count(&mut **client), 13);
        assert_eq!(
            rows_before,
            vec![SignatureRow(
                "a".repeat(64),
                "phase657-replay-legacy".to_owned(),
                1,
                "root".to_owned(),
                "bluey-jobs-managed-cloud-release-v1".to_owned(),
                "b".repeat(64),
                1,
                1,
                "schema-only".to_owned(),
                "fixture".to_owned(),
                1,
            )],
            "legacy-13 fixture pins every signature field before the failed transition"
        );
        state_relation_absent(&mut **client);

        // This owned-private-PG role fixture is not a production hook. The
        // actor may perform the guarded forward and validate/lock the ledger,
        // but it cannot insert a canonical history row.
        let actor = "phase657_replay_049_writer";
        let actor_identifier: String = client
            .query_one("SELECT pg_catalog.quote_ident($1)", &[&actor])
            .expect("quote fixed actor")
            .get(0);
        let owner_identifier: String = client
            .query_one("SELECT pg_catalog.quote_ident(current_user)", &[])
            .expect("quote fixture owner")
            .get(0);
        let database_identifier: String = client
            .query_one("SELECT pg_catalog.quote_ident(current_database())", &[])
            .expect("quote current owned database")
            .get(0);
        client
            .batch_execute(&format!(
                "CREATE ROLE {actor_identifier} NOLOGIN NOSUPERUSER NOINHERIT;
                     ALTER TABLE public.jobs_managed_cloud_signature_sets
                       OWNER TO {actor_identifier};
                     GRANT USAGE, CREATE ON SCHEMA public TO {actor_identifier};
                     GRANT TEMPORARY ON DATABASE {database_identifier} TO {actor_identifier};
                     GRANT {actor_identifier} TO {owner_identifier};
                     GRANT SELECT, UPDATE ON TABLE public.bluey_schema_migrations
                       TO {actor_identifier};
                     REVOKE INSERT ON TABLE public.bluey_schema_migrations
                       FROM {actor_identifier};
                     REVOKE INSERT ON TABLE public.bluey_schema_migrations FROM PUBLIC;"
            ))
            .expect("create owned late-ledger role fixture");
        let privileges = client
            .query_one(
                "SELECT NOT rolcanlogin, NOT rolsuper, NOT rolinherit,
                        has_schema_privilege($1, 'public', 'USAGE'),
                        has_schema_privilege($1, 'public', 'CREATE'),
                        has_database_privilege($1, current_database(), 'TEMP'),
                        has_table_privilege($1, 'public.bluey_schema_migrations', 'SELECT'),
                        has_table_privilege($1, 'public.bluey_schema_migrations', 'UPDATE'),
                        NOT has_table_privilege(
                            $1, 'public.bluey_schema_migrations', 'INSERT'
                        ),
                        NOT has_table_privilege(
                            'public', 'public.bluey_schema_migrations', 'INSERT'
                        ),
                        (SELECT pg_catalog.pg_get_userbyid(relowner)=$1
                           FROM pg_catalog.pg_class
                          WHERE oid='public.jobs_managed_cloud_signature_sets'::regclass)
                   FROM pg_catalog.pg_roles WHERE rolname=$1",
                &[&actor],
            )
            .expect("read owned fixture role privileges");
        for column in 0..11 {
            assert!(
                privileges.get::<_, bool>(column),
                "owned late-ledger role privilege assertion {column}"
            );
        }
        drop(client);

        let mut actor_client = pool.get_pg().expect("owned actor client");
        actor_client
            .batch_execute(&format!("SET ROLE {actor_identifier}"))
            .expect("switch to owned actor");
        assert_eq!(
            actor_client
                .query_one("SELECT current_user::text", &[])
                .expect("read failed actor current user")
                .get::<_, String>(0),
            actor
        );
        let error = super::execute_postgres_signature_audience_049(&mut actor_client)
            .expect_err("late canonical history write must fail");
        assert!(
            error
                .chain()
                .map(ToString::to_string)
                .any(|context| context == "selected migration ledger write unavailable"),
            "driver must retain its static ledger-write context"
        );
        actor_client
            .batch_execute("RESET ROLE")
            .expect("reset failed actor checkout");
        actor_client.mark_broken();
        drop(actor_client);

        let mut client = pool.get_pg().expect("post-failure privileged client");
        assert_eq!(audience_count(&mut **client), 13);
        assert_eq!(audience_constraint(&mut **client), constraint_before);
        assert_eq!(all_signature_rows(&mut **client), rows_before);
        assert_eq!(all_history(&mut **client), history_before);
        state_relation_absent(&mut **client);
        reusable(&mut client);

        client
            .batch_execute(&format!(
                "GRANT INSERT ON TABLE public.bluey_schema_migrations
                       TO {actor_identifier}"
            ))
            .expect("grant owned actor canonical ledger insert");
        drop(client);

        let mut actor_client = pool.get_pg().expect("owned actor retry client");
        actor_client
            .batch_execute(&format!("SET ROLE {actor_identifier}"))
            .expect("switch to owned actor for retry");
        assert_eq!(
            actor_client
                .query_one("SELECT current_user::text", &[])
                .expect("read retry actor current user")
                .get::<_, String>(0),
            actor
        );
        super::execute_postgres_signature_audience_049(&mut actor_client)
            .expect("retry after owned insert grant");
        actor_client
            .batch_execute("RESET ROLE")
            .expect("reset successful actor checkout");
        assert_eq!(audience_count(&mut **actor_client), 14);
        let first_049 = history(&mut **actor_client, V049).expect("canonical 049 history");
        let first_signatures = all_signature_rows(&mut **actor_client);
        let first_singleton = singleton_state(&mut **actor_client);
        let first_history = all_history(&mut **actor_client);
        assert_eq!(
            first_signatures, rows_before,
            "049 changes the audience constraint, not any of the eleven signature fields"
        );
        assert_eq!(
            first_singleton,
            vec![(1_i16, 1_i16)],
            "049 commits the exact singleton/version state"
        );
        let mut expected_history = history_before.clone();
        expected_history.push(first_049.clone());
        expected_history.sort();
        assert_eq!(
            first_history, expected_history,
            "first successful 049 adds exactly its canonical ledger row"
        );
        actor_client
            .batch_execute(&format!("SET ROLE {actor_identifier}"))
            .expect("switch to owned actor for exact retry");
        assert_eq!(
            actor_client
                .query_one("SELECT current_user::text", &[])
                .expect("read exact-retry actor current user")
                .get::<_, String>(0),
            actor
        );
        super::execute_postgres_signature_audience_049(&mut actor_client).expect("exact retry");
        actor_client
            .batch_execute("RESET ROLE")
            .expect("reset exact-retry actor checkout");
        assert_eq!(
            history(&mut **actor_client, V049),
            Some(first_049.clone()),
            "exact retry preserves canonical timestamp"
        );
        assert_eq!(
            all_signature_rows(&mut **actor_client),
            first_signatures,
            "exact replay preserves every signature row and all eleven fields"
        );
        assert_eq!(
            singleton_state(&mut **actor_client),
            first_singleton,
            "exact replay preserves the singleton state"
        );
        assert_eq!(
            all_history(&mut **actor_client),
            first_history,
            "exact replay preserves complete canonical history"
        );
        actor_client.mark_broken();
        drop(actor_client);

        let mut client = pool.get_pg().expect("privileged cleanup client");
        assert_eq!(audience_count(&mut **client), 14);
        assert_eq!(history(&mut **client, V049), Some(first_049));
        assert_eq!(all_signature_rows(&mut **client), first_signatures);
        assert_eq!(singleton_state(&mut **client), first_singleton);
        assert_eq!(all_history(&mut **client), first_history);
        reusable(&mut client);
        client
            .batch_execute(&format!(
                "REASSIGN OWNED BY {actor_identifier} TO {owner_identifier};
                 DROP OWNED BY {actor_identifier};
                 REVOKE {actor_identifier} FROM {owner_identifier};
                 DROP ROLE {actor_identifier};"
            ))
            .expect("remove owned late-ledger role fixture");
        println!("phase657_owned_postgres_signature_audience_049_late_ledger_rollback_complete");
    }
}

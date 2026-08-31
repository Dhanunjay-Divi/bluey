use super::production_positive_authority_fixture::{
    install_production_positive_job_authorities, save_production_positive_verified_import,
};
use super::*;
use crate::db;
use serde_json::json;
use std::sync::{Arc, Barrier};

struct LocalAuthorityFixture {
    account_id: String,
    application: JobApplication,
    posting: JobPosting,
    run_id: String,
    ticket_hash: String,
    identity_id: String,
}

fn local_submission_capacity(
    fixture: &LocalAuthorityFixture,
) -> crate::db::object_uploads::NewSubmissionEvidenceCapacity {
    let now = now_ms();
    crate::db::object_uploads::NewSubmissionEvidenceCapacity {
        account_id: fixture.account_id.clone(),
        application_id: fixture.application.id.clone(),
        run_id: fixture.run_id.clone(),
        runner: "local".to_string(),
        reserved_bytes: 48 * 1024 * 1024,
        reserved_objects: 13,
        expires_at_ms: now.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS),
        now_ms: now,
        limits: crate::object_storage::UploadLimits {
            max_object_bytes: 8 * 1024 * 1024,
            max_account_bytes: 128 * 1024 * 1024,
            max_daily_bytes: 128 * 1024 * 1024,
            max_account_objects: 100,
        },
    }
}

fn postgres_pool() -> Option<DbPool> {
    let database_url = std::env::var("BLUEY_TEST_POSTGRES_URL").ok()?;
    let pool = db::open_postgres_pool(&database_url).expect("open PostgreSQL test pool");
    db::run_migrations(&pool).expect("apply PostgreSQL migrations");
    db::run_migrations(&pool).expect("replay PostgreSQL migrations");

    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL migration assertion connection");
    let vector_ready: bool = conn
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'vector')",
            &[],
        )
        .expect("query pgvector extension")
        .get(0);
    assert!(vector_ready, "PostgreSQL authority test requires pgvector");
    let migrations = conn
        .query(
            "SELECT version FROM bluey_schema_migrations
              WHERE version IN ('002_jobs.sql', '008_jobs_generation_allowance.sql',
                                '009_jobs_discovery_board_owner.sql',
                                '010_provider_usage_provenance.sql',
                                '011_jobs_candidate_evidence.sql',
                                '030_jobs_operational_holds.sql')",
            &[],
        )
        .expect("query Jobs migration ledger")
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<std::collections::HashSet<_>>();
    for required in [
        "002_jobs.sql",
        "008_jobs_generation_allowance.sql",
        "009_jobs_discovery_board_owner.sql",
        "010_provider_usage_provenance.sql",
        "011_jobs_candidate_evidence.sql",
        "030_jobs_operational_holds.sql",
    ] {
        assert!(
            migrations.contains(required),
            "missing replayed PostgreSQL migration {required}"
        );
    }
    let operational_hold_ledger_count = conn
        .query_one(
            "SELECT COUNT(*) FROM bluey_schema_migrations WHERE version = $1",
            &[&db::JOBS_OPERATIONAL_HOLDS_MIGRATION_ID],
        )
        .expect("count replayed PostgreSQL operational-hold migration ledger rows")
        .get::<_, i64>(0);
    assert_eq!(operational_hold_ledger_count, 1);
    for migration_id in [
        db::JOBS_ORIGINAL_SOURCE_VERIFICATION_AUTHORITY_MIGRATION_ID,
        db::JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY_MIGRATION_ID,
    ] {
        let ledger_count = conn
            .query_one(
                "SELECT COUNT(*) FROM bluey_schema_migrations WHERE version = $1",
                &[&migration_id],
            )
            .expect("count replayed PostgreSQL source/integrity migration ledger rows")
            .get::<_, i64>(0);
        assert_eq!(
            ledger_count, 1,
            "PostgreSQL migration {migration_id} must be recorded exactly once"
        );
    }

    let mut phase614b_tables = conn
        .query(
            "SELECT relation.relname
               FROM pg_class AS relation
               JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
              WHERE namespace.nspname = current_schema()
                AND relation.relkind = 'r'
                AND starts_with(relation.relname, 'jobs_job_integrity_')",
            &[],
        )
        .expect("query replayed PostgreSQL Phase 614B tables")
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();
    phase614b_tables.sort();
    let mut expected_phase614b_tables = [
        "jobs_job_integrity_attestations",
        "jobs_job_integrity_control",
        "jobs_job_integrity_head_transitions",
        "jobs_job_integrity_heads",
        "jobs_job_integrity_revocations",
        "jobs_job_integrity_trust_keys",
        "jobs_job_integrity_trust_policies",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    expected_phase614b_tables.sort();
    assert_eq!(phase614b_tables, expected_phase614b_tables);

    let mut phase614b_triggers = conn
        .query(
            "SELECT trigger.tgname
               FROM pg_trigger AS trigger
               JOIN pg_class AS relation ON relation.oid = trigger.tgrelid
               JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
              WHERE namespace.nspname = current_schema()
                AND starts_with(relation.relname, 'jobs_job_integrity_')
                AND starts_with(trigger.tgname, 'trg_jobs_job_integrity_')
                AND NOT trigger.tgisinternal
                AND trigger.tgenabled = 'O'",
            &[],
        )
        .expect("query replayed PostgreSQL Phase 614B triggers")
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();
    phase614b_triggers.sort();
    let mut expected_phase614b_triggers = [
        "trg_jobs_job_integrity_attestations_no_delete",
        "trg_jobs_job_integrity_attestations_no_update",
        "trg_jobs_job_integrity_attestations_validate_insert",
        "trg_jobs_job_integrity_control_monotonic",
        "trg_jobs_job_integrity_control_no_delete",
        "trg_jobs_job_integrity_head_transitions_no_delete",
        "trg_jobs_job_integrity_head_transitions_no_update",
        "trg_jobs_job_integrity_head_transitions_validate_insert",
        "trg_jobs_job_integrity_heads_monotonic",
        "trg_jobs_job_integrity_heads_no_delete",
        "trg_jobs_job_integrity_heads_validate_insert",
        "trg_jobs_job_integrity_revocations_no_delete",
        "trg_jobs_job_integrity_revocations_no_update",
        "trg_jobs_job_integrity_revocations_validate_insert",
        "trg_jobs_job_integrity_trust_keys_no_delete",
        "trg_jobs_job_integrity_trust_keys_no_update",
        "trg_jobs_job_integrity_trust_keys_validate_insert",
        "trg_jobs_job_integrity_trust_policies_no_delete",
        "trg_jobs_job_integrity_trust_policies_no_update",
        "trg_jobs_job_integrity_trust_policies_validate_insert",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    expected_phase614b_triggers.sort();
    assert_eq!(phase614b_triggers, expected_phase614b_triggers);
    drop(conn);
    Some(pool)
}

fn postgres_phase613_check_constraint_name(
    tx: &mut postgres::Transaction<'_>,
    schema: &str,
    table: &str,
    column: &str,
) -> String {
    let definition_pattern = format!("%{column}%9007199254740991%");
    let rows = tx
        .query(
            "SELECT constraint_record.conname
               FROM pg_constraint AS constraint_record
               JOIN pg_class AS relation
                 ON relation.oid = constraint_record.conrelid
               JOIN pg_namespace AS namespace
                 ON namespace.oid = relation.relnamespace
              WHERE namespace.nspname = $1
                AND relation.relname = $2
                AND constraint_record.contype = 'c'
                AND pg_get_constraintdef(constraint_record.oid) LIKE $3",
            &[&schema, &table, &definition_pattern],
        )
        .expect("query isolated PostgreSQL Phase 613 CHECK constraint");
    assert_eq!(
        rows.len(),
        1,
        "expected one PostgreSQL CHECK for {table}.{column}"
    );
    rows[0].get(0)
}

fn assert_postgres_phase613_check_violation(
    tx: &mut postgres::Transaction<'_>,
    statement: &str,
    expected_constraint: &str,
    invariant: &str,
) {
    tx.batch_execute("SAVEPOINT phase613_check_violation")
        .expect("create PostgreSQL Phase 613 CHECK savepoint");
    let error = tx
        .execute(statement, &[])
        .expect_err("out-of-range PostgreSQL Phase 613 authority row must fail");
    let db_error = error
        .as_db_error()
        .expect("PostgreSQL Phase 613 CHECK failure must expose database evidence");
    assert_eq!(
        db_error.code().code(),
        "23514",
        "{invariant} must fail with SQLSTATE check_violation"
    );
    assert_eq!(
        db_error.constraint(),
        Some(expected_constraint),
        "{invariant} must fail its exact migration 034 CHECK"
    );
    tx.batch_execute(
        "ROLLBACK TO SAVEPOINT phase613_check_violation;
         RELEASE SAVEPOINT phase613_check_violation;",
    )
    .expect("recover PostgreSQL transaction after expected CHECK violation");
}

#[test]
#[serial_test::serial]
fn postgres_operational_hold_migration_replay_is_exact_and_seedless() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let schema = format!("bluey_hold_migration_{}", uuid::Uuid::new_v4().simple());
    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL operational-hold migration replay connection");
    let mut tx = conn
        .transaction()
        .expect("begin PostgreSQL operational-hold migration replay transaction");
    tx.batch_execute(&format!(
        "CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema};
         CREATE TABLE bluey_schema_migrations (
             version TEXT PRIMARY KEY,
             applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
         );"
    ))
    .expect("create isolated PostgreSQL operational-hold migration schema");

    for _ in 0..2 {
        tx.batch_execute(db::POSTGRES_JOBS_OPERATIONAL_HOLDS)
            .expect("apply and replay isolated PostgreSQL operational-hold migration");
        tx.execute(
            "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
             ON CONFLICT (version) DO NOTHING",
            &[&db::JOBS_OPERATIONAL_HOLDS_MIGRATION_ID],
        )
        .expect("record isolated PostgreSQL operational-hold migration");
    }

    let ledger_count = tx
        .query_one(
            "SELECT COUNT(*) FROM bluey_schema_migrations WHERE version = $1",
            &[&db::JOBS_OPERATIONAL_HOLDS_MIGRATION_ID],
        )
        .expect("count isolated PostgreSQL operational-hold migration ledger rows")
        .get::<_, i64>(0);
    assert_eq!(ledger_count, 1);
    for table in [
        "jobs_operational_hold_events",
        "jobs_operational_hold_heads",
    ] {
        let seed_count = tx
            .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
            .expect("count isolated PostgreSQL operational-hold seed rows")
            .get::<_, i64>(0);
        assert_eq!(seed_count, 0, "migration must not seed {table}");
    }
    for function_name in [
        "validate_jobs_operational_hold_event",
        "reject_jobs_operational_hold_event_mutation",
        "validate_jobs_operational_hold_head_insert",
        "enforce_jobs_operational_hold_head_monotonic",
        "reject_jobs_operational_hold_head_delete",
    ] {
        let function_count = tx
            .query_one(
                "SELECT COUNT(*)
                   FROM pg_proc AS routine
                   JOIN pg_namespace AS namespace
                     ON namespace.oid = routine.pronamespace
                  WHERE namespace.nspname = $1 AND routine.proname = $2",
                &[&schema, &function_name],
            )
            .expect("count isolated PostgreSQL operational-hold trigger function")
            .get::<_, i64>(0);
        assert_eq!(
            function_count, 1,
            "migration replay must leave one {function_name} function"
        );
    }
    for trigger_name in [
        "trg_jobs_operational_hold_events_validate_insert",
        "trg_jobs_operational_hold_events_no_update",
        "trg_jobs_operational_hold_events_no_delete",
        "trg_jobs_operational_hold_heads_validate_insert",
        "trg_jobs_operational_hold_heads_monotonic",
        "trg_jobs_operational_hold_heads_no_delete",
    ] {
        let trigger_count = tx
            .query_one(
                "SELECT COUNT(*)
                   FROM pg_trigger AS trigger
                   JOIN pg_class AS relation ON relation.oid = trigger.tgrelid
                   JOIN pg_namespace AS namespace
                     ON namespace.oid = relation.relnamespace
                  WHERE namespace.nspname = $1
                    AND trigger.tgname = $2
                    AND NOT trigger.tgisinternal",
                &[&schema, &trigger_name],
            )
            .expect("count isolated PostgreSQL operational-hold trigger")
            .get::<_, i64>(0);
        assert_eq!(
            trigger_count, 1,
            "migration replay must leave one {trigger_name} trigger"
        );
    }

    tx.rollback()
        .expect("rollback isolated PostgreSQL operational-hold migration schema");
}

#[test]
#[serial_test::serial]
fn postgres_canonical_taxonomy_migration_rejects_out_of_range_authority_values() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let schema = format!("bluey_phase613_checks_{}", uuid::Uuid::new_v4().simple());
    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL Phase 613 CHECK assertion connection");
    let mut tx = conn
        .transaction()
        .expect("begin PostgreSQL Phase 613 CHECK assertion transaction");
    tx.batch_execute(&format!(
        "CREATE SCHEMA {schema};
         SET LOCAL search_path TO {schema};
         CREATE TABLE accounts (id TEXT PRIMARY KEY);
         CREATE TABLE jobs_tracks (
             id TEXT PRIMARY KEY,
             account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE
         );
         CREATE TABLE jobs_application_identities (
             id TEXT PRIMARY KEY,
             account_id TEXT NOT NULL,
             verification_status TEXT NOT NULL
         );
         CREATE TABLE jobs_resume_source_assets (
             id TEXT PRIMARY KEY,
             account_id TEXT NOT NULL,
             sha256 TEXT NOT NULL
         );"
    ))
    .expect("create isolated PostgreSQL Phase 613 CHECK schema");
    tx.batch_execute(db::POSTGRES_JOBS_CANONICAL_TAXONOMY_AUTHORITY)
        .expect("apply isolated PostgreSQL canonical-taxonomy migration");

    let negative_timestamp_constraint = postgres_phase613_check_constraint_name(
        &mut tx,
        &schema,
        "jobs_track_policy_taxonomy_activation_events",
        "activated_at_ms",
    );
    assert_postgres_phase613_check_violation(
        &mut tx,
        "INSERT INTO jobs_track_policy_taxonomy_activation_events (
             activation_epoch, previous_activation_epoch, taxonomy_version,
             taxonomy_digest_sha256, canonicalizer_schema_version,
             canonicalizer_digest_sha256, activation_transition_sha256,
             predecessor_activation_transition_sha256, activated_at_ms
         ) VALUES (
             1, 0, 'v1', repeat('a', 64), 1, repeat('b', 64), repeat('c', 64),
             NULL, -1
         )",
        &negative_timestamp_constraint,
        "negative activation timestamp",
    );

    let above_safe_integer_constraint = postgres_phase613_check_constraint_name(
        &mut tx,
        &schema,
        "jobs_track_policy_taxonomy_activation_events",
        "canonicalizer_schema_version",
    );
    assert_postgres_phase613_check_violation(
        &mut tx,
        "INSERT INTO jobs_track_policy_taxonomy_activation_events (
             activation_epoch, previous_activation_epoch, taxonomy_version,
             taxonomy_digest_sha256, canonicalizer_schema_version,
             canonicalizer_digest_sha256, activation_transition_sha256,
             predecessor_activation_transition_sha256, activated_at_ms
         ) VALUES (
             1, 0, 'v1', repeat('a', 64), 9007199254740992, repeat('b', 64),
             repeat('c', 64), NULL, 0
         )",
        &above_safe_integer_constraint,
        "canonicalizer version above the safe-integer maximum",
    );

    tx.batch_execute(
        "INSERT INTO accounts (id) VALUES ('phase613-check-account');
         INSERT INTO jobs_tracks (id, account_id)
         VALUES ('phase613-check-track', 'phase613-check-account');
         ALTER TABLE jobs_track_policy_revisions DISABLE TRIGGER USER;",
    )
    .expect("prepare isolated PostgreSQL policy-revision CHECK parent rows");
    let zero_generation_constraint = postgres_phase613_check_constraint_name(
        &mut tx,
        &schema,
        "jobs_track_policy_revisions",
        "account_input_generation",
    );
    assert_postgres_phase613_check_violation(
        &mut tx,
        "INSERT INTO jobs_track_policy_revisions (
             revision_id, account_id, career_track_id, revision_no,
             taxonomy_version, taxonomy_digest_sha256, taxonomy_activation_epoch,
             canonicalizer_schema_version, canonicalizer_digest_sha256,
             account_input_generation, account_input_transition_sha256,
             account_semantic_sha256, track_input_generation,
             track_input_transition_sha256, track_semantic_sha256,
             canonical_policy_sha256, canonical_policy_ciphertext,
             canonical_role_id, canonical_role_family,
             verified_application_identity_id, verified_application_identity_sha256,
             source_resume_asset_id, source_resume_sha256, job_preferences_sha256,
             predecessor_revision_id, predecessor_revision_no,
             predecessor_policy_sha256, compatibility_classification, review_state,
             created_by, created_at_ms
         ) VALUES (
             'phase613-check-revision-0001', 'phase613-check-account',
             'phase613-check-track', 1, 'v1', repeat('d', 64), 1, 1,
             repeat('e', 64), 0, repeat('f', 64), repeat('0', 64), 1,
             repeat('1', 64), repeat('2', 64), repeat('3', 64),
             'bluey-jobs:v1:' || repeat('A', 40), 'software-engineer',
             'software-engineering', 'phase613-check-identity', repeat('4', 64),
             'phase613-check-resume', repeat('5', 64), repeat('6', 64),
             NULL, NULL, NULL, 'initial', 'pending_review', 'phase613-test', 0
         )",
        &zero_generation_constraint,
        "zero account-input generation",
    );

    tx.rollback()
        .expect("rollback isolated PostgreSQL Phase 613 CHECK schema");
}

fn operational_hold_request(
    event_id: &str,
    capability: OperationalCapability,
    scope_kind: OperationalHoldScopeKind,
    scope_id: &str,
    transition: OperationalHoldTransition,
    expected_head_revision: i64,
    expected_current_event_id: Option<&str>,
) -> AppendOperationalHoldEventRequest {
    AppendOperationalHoldEventRequest {
        event_id: event_id.to_string(),
        capability,
        scope_kind,
        scope_id: scope_id.to_string(),
        transition,
        reason_code: if transition == OperationalHoldTransition::Released {
            OperationalHoldReasonCode::ManualRelease
        } else {
            OperationalHoldReasonCode::Incident
        },
        reason_ref: Some("PG-606".to_string()),
        expected_head_revision,
        expected_current_event_id: expected_current_event_id.map(str::to_string),
    }
}

struct PostgresOperationalHoldCaseCleanup {
    pool: DbPool,
    capability: OperationalCapability,
    scope_kind: OperationalHoldScopeKind,
    scope_id: String,
    account_id: String,
}

impl Drop for PostgresOperationalHoldCaseCleanup {
    fn drop(&mut self) {
        let Ok(mut conn) = self.pool.get_pg() else {
            return;
        };
        let Ok(mut tx) = conn.transaction() else {
            return;
        };
        let cleanup = tx
            .batch_execute(
                "ALTER TABLE jobs_operational_hold_heads
                    DISABLE TRIGGER trg_jobs_operational_hold_heads_no_delete;
                 ALTER TABLE jobs_operational_hold_events
                    DISABLE TRIGGER trg_jobs_operational_hold_events_no_delete;",
            )
            .and_then(|_| {
                tx.execute(
                    "DELETE FROM jobs_operational_hold_heads
                      WHERE capability = $1 AND scope_kind = $2 AND scope_id = $3",
                    &[
                        &self.capability.as_str(),
                        &self.scope_kind.as_str(),
                        &self.scope_id,
                    ],
                )
                .map(|_| ())
            })
            .and_then(|_| {
                tx.execute(
                    "DELETE FROM jobs_operational_hold_events
                      WHERE capability = $1 AND scope_kind = $2 AND scope_id = $3",
                    &[
                        &self.capability.as_str(),
                        &self.scope_kind.as_str(),
                        &self.scope_id,
                    ],
                )
                .map(|_| ())
            })
            .and_then(|_| {
                tx.batch_execute(
                    "ALTER TABLE jobs_operational_hold_heads
                        ENABLE TRIGGER trg_jobs_operational_hold_heads_no_delete;
                     ALTER TABLE jobs_operational_hold_events
                        ENABLE TRIGGER trg_jobs_operational_hold_events_no_delete;",
                )
            });
        if cleanup.is_ok() && tx.commit().is_ok() {
            let _ = conn.execute("DELETE FROM accounts WHERE id = $1", &[&self.account_id]);
        }
    }
}

fn insert_postgres_operational_hold_account(pool: &DbPool, account_id: &str, suffix: &str) {
    pool.get_pg()
        .expect("get PostgreSQL operational-hold account setup connection")
        .execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ($1, $2, 'hash', 0)",
            &[
                &account_id,
                &format!("operational-hold-{suffix}@example.test"),
            ],
        )
        .expect("insert PostgreSQL operational-hold account");
}

fn corrupt_postgres_operational_hold_canonical_fields(
    pool: &DbPool,
    event_id: &str,
    event_sha256: &str,
    canonical_event_base64url: &str,
) {
    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL canonical-corruption connection");
    let mut tx = conn
        .transaction()
        .expect("begin PostgreSQL canonical-corruption transaction");
    tx.batch_execute(
        "ALTER TABLE jobs_operational_hold_events
            DISABLE TRIGGER trg_jobs_operational_hold_events_no_update;",
    )
    .expect("temporarily disable PostgreSQL operational-hold update guard");
    tx.execute(
        "UPDATE jobs_operational_hold_events
            SET event_sha256 = $2, canonical_event_base64url = $3
          WHERE event_id = $1",
        &[&event_id, &event_sha256, &canonical_event_base64url],
    )
    .expect("inject PostgreSQL operational-hold canonical corruption");
    tx.batch_execute(
        "ALTER TABLE jobs_operational_hold_events
            ENABLE TRIGGER trg_jobs_operational_hold_events_no_update;",
    )
    .expect("restore PostgreSQL operational-hold update guard");
    tx.commit()
        .expect("commit PostgreSQL operational-hold canonical corruption");
}

fn assert_postgres_canonical_corruption_blocks_every_release_path(
    pool: &DbPool,
    held: &AppendOperationalHoldEventRequest,
    public_state: &OperationalHoldPublicState,
    raw_release_event_id: &str,
    by_ref_release_event_id: &str,
) {
    let raw_release = operational_hold_request(
        raw_release_event_id,
        held.capability,
        held.scope_kind,
        &held.scope_id,
        OperationalHoldTransition::Released,
        1,
        Some(&held.event_id),
    );
    assert!(matches!(
        append_operational_hold_event(pool, &raw_release, "pg-corruption-reviewer"),
        Err(OperationalHoldError::Storage(_))
    ));

    let by_ref_release = AppendOperationalHoldEventByRefRequest {
        event_id: by_ref_release_event_id.to_string(),
        capability: held.capability,
        scope_kind: held.scope_kind,
        scope_ref: public_state.scope_ref.clone(),
        transition: OperationalHoldTransition::Released,
        reason_code: OperationalHoldReasonCode::ManualRelease,
        reason_ref: Some("PG-606".to_string()),
        expected_head_revision: 1,
        expected_current_event_ref: public_state.current_event_ref.clone(),
    };
    assert!(matches!(
        append_operational_hold_event_by_ref(pool, &by_ref_release, "pg-corruption-reviewer"),
        Err(OperationalHoldError::Storage(_))
    ));
    assert!(matches!(
        list_operational_hold_states(pool, false, 500, None),
        Err(OperationalHoldError::Storage(_))
    ));

    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL canonical-corruption assertion connection");
    let head = conn
        .query_one(
            "SELECT head_revision, current_event_id, state
               FROM jobs_operational_hold_heads
              WHERE capability = $1 AND scope_kind = $2 AND scope_id = $3",
            &[
                &held.capability.as_str(),
                &held.scope_kind.as_str(),
                &held.scope_id,
            ],
        )
        .expect("query PostgreSQL corrupted operational-hold head");
    assert_eq!(head.get::<_, i64>(0), 1);
    assert_eq!(head.get::<_, String>(1), held.event_id);
    assert_eq!(head.get::<_, String>(2), "held");
    let release_count = conn
        .query_one(
            "SELECT COUNT(*) FROM jobs_operational_hold_events
              WHERE event_id IN ($1, $2)",
            &[&raw_release_event_id, &by_ref_release_event_id],
        )
        .expect("count rejected PostgreSQL corruption release events")
        .get::<_, i64>(0);
    assert_eq!(release_count, 0);
}

fn greenhouse_posting(url: &str) -> JobPosting {
    let mut posting = JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse".to_string(),
        external_id: url.to_string(),
        company: "Acme".to_string(),
        title: "Software Engineer".to_string(),
        location: "New York, NY".to_string(),
        workplace: "hybrid".to_string(),
        canonical_url: url.to_string(),
        description: "Build reliable products with Rust and TypeScript.".to_string(),
        compensation: "$170k-$200k".to_string(),
        employment_type: "full_time".to_string(),
        track_id: String::new(),
        match_score: 90,
        matched_reasons: vec!["Skills fit".to_string()],
        missing_requirements: Vec::new(),
        posted_at_ms: Some(now_ms()),
        last_verified_at_ms: Some(now_ms()),
        availability_status: "active".to_string(),
        status: "matched".to_string(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    };
    posting.canonical_key = canonical_job_key(&posting);
    posting.discovery_evidence = JobDiscoveryEvidence::verified_original_source(
        posting.canonical_key.clone(),
        "greenhouse:acme".to_string(),
        Some("boards.greenhouse.io".to_string()),
        now_ms(),
        "a".repeat(64),
    );
    posting
}

fn seed_postgres_browser_release_binding(pool: &DbPool, fixture: &LocalAuthorityFixture) {
    let release_fixture: Value = serde_json::from_str(include_str!(
        "../../../../jobs/browser/fixtures/release-authority-v1.json"
    ))
    .expect("parse shared Browser release authority fixture");
    let canonical_policy_base64url = release_fixture
        .pointer("/trustPolicy/canonical")
        .and_then(Value::as_str)
        .expect("shared Browser release policy canonical bytes")
        .to_string();
    let policy_sha256 = release_fixture
        .pointer("/trustPolicy/sha256")
        .and_then(Value::as_str)
        .expect("shared Browser release policy digest")
        .to_string();
    let manifest_sha256 = "1".repeat(64);
    let artifact_sha256 = "2".repeat(64);
    let descriptor_sha256 = "3".repeat(64);
    let activation_sha256 = "4".repeat(64);
    let manifest_signature_set_sha256 = "5".repeat(64);
    let assignment_sha256 = hex::encode(Sha256::digest(
        format!("postgres-browser-assignment:{}", fixture.account_id).as_bytes(),
    ));
    let policy_signature_set_sha256 = "a".repeat(64);
    let activation_signature_set_sha256 = "b".repeat(64);
    let transition_sha256 = "c".repeat(64);
    let signature = "A".repeat(86);
    let public_key = "A".repeat(43);
    let app_content_sha256 = "8".repeat(64);
    let verification_evidence_sha256 = "9".repeat(64);
    let automation_bundle_sha256 = "4".repeat(64);
    let chromium_executable_sha256 = "7".repeat(64);

    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL Browser release fixture connection");
    for (
        signature_set_id,
        signature_set_sha256,
        role,
        target_audience,
        target_sha256,
        signer_key_id,
    ) in [
        (
            "test-policy-signatures",
            policy_signature_set_sha256.as_str(),
            "root",
            "bluey-jobs-browser-release-trust-policy-v1",
            policy_sha256.as_str(),
            "test-root-key",
        ),
        (
            "test-manifest-signatures",
            manifest_signature_set_sha256.as_str(),
            "release",
            "bluey-jobs-browser-release-manifest-v1",
            manifest_sha256.as_str(),
            "test-build-key",
        ),
        (
            "test-activation-signatures",
            activation_signature_set_sha256.as_str(),
            "promotion",
            "bluey-jobs-browser-release-activation-v1",
            activation_sha256.as_str(),
            "test-promotion-key",
        ),
    ] {
        conn.execute(
            "INSERT INTO jobs_browser_release_signature_sets (
                signature_set_sha256, signature_set_id, trust_generation,
                role, target_audience, target_sha256, signed_at_ms,
                signature_count, canonical_signature_set_base64url,
                recorded_by, recorded_at_ms
             ) VALUES ($1, $2, 1, $3, $4, $5, 1, 1, 'dGVzdA', 'test-suite', 1)
             ON CONFLICT DO NOTHING",
            &[
                &signature_set_sha256,
                &signature_set_id,
                &role,
                &target_audience,
                &target_sha256,
            ],
        )
        .expect("insert PostgreSQL Browser signature set");
        conn.execute(
            "INSERT INTO jobs_browser_release_signatures (
                signature_set_sha256, key_id, signature_base64url
             ) VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
            &[&signature_set_sha256, &signer_key_id, &signature],
        )
        .expect("insert PostgreSQL Browser signature");
    }
    conn.execute(
        "INSERT INTO jobs_browser_release_trust_policies (
            policy_sha256, policy_id, trust_generation,
            predecessor_policy_sha256, predecessor_trust_generation,
            root_threshold, release_threshold, promotion_threshold,
            incident_threshold, key_count, canonical_policy_base64url,
            authorization_signature_set_sha256, issued_at_ms,
            valid_from_ms, expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
            $1, 'test-policy', 1, NULL, 0, 1, 1, 1, 1, 4,
            $2, $3, 1, 0, 9007199254740991, 'test-suite', 1
         ) ON CONFLICT DO NOTHING",
        &[
            &policy_sha256,
            &canonical_policy_base64url,
            &policy_signature_set_sha256,
        ],
    )
    .expect("insert PostgreSQL Browser trust policy");
    for (key_id, role) in [
        ("test-root-key", "root"),
        ("test-build-key", "release"),
        ("test-promotion-key", "promotion"),
        ("test-incident-key", "incident"),
    ] {
        conn.execute(
            "INSERT INTO jobs_browser_release_trust_keys (
                policy_sha256, trust_generation, key_id, role,
                public_key_base64url, state, valid_from_ms, valid_until_ms,
                minimum_trust_generation, maximum_trust_generation
             ) VALUES (
                $1, 1, $2, $3, $4, 'active', 0, 9007199254740991,
                1, 9007199254740991
             ) ON CONFLICT DO NOTHING",
            &[&policy_sha256, &key_id, &role, &public_key],
        )
        .expect("insert PostgreSQL Browser trust key");
    }
    conn.execute(
        "INSERT INTO jobs_browser_release_manifests (
            manifest_sha256, manifest_id, manifest_generation, release_id,
            release_sequence, build_id, app_version, protocol_version,
            source_commit, electron_version, playwright_version,
            chromium_revision, release_notes_url, artifact_count,
            canonical_manifest_base64url,
            authorization_signature_set_sha256, published_at_ms,
            recorded_by, recorded_at_ms
         ) VALUES (
            $1, 'test-manifest', 1, 'test-release', 1, 'browser-1.0',
            '1.0.0', 1, $2, '43.1.0', '1.61.1', '1228',
            'https://bluey.sh/jobs/browser/releases/test-release/RELEASE.md',
            5, 'dGVzdA', $3, 1, 'test-suite', 1
         ) ON CONFLICT DO NOTHING",
        &[
            &manifest_sha256,
            &"a".repeat(40),
            &manifest_signature_set_sha256,
        ],
    )
    .expect("insert PostgreSQL Browser manifest");
    conn.execute(
        "INSERT INTO jobs_browser_release_artifacts (
            artifact_id, manifest_sha256, platform, architecture,
            package_kind, build_descriptor_sha256,
            build_descriptor_base64url,
            build_descriptor_signature_base64url,
            build_descriptor_signing_key_id, artifact_url,
            artifact_filename, artifact_size_bytes, artifact_sha256,
            app_content_sha256, verification_evidence_sha256,
            native_signature_kind, native_signer_identity, recorded_at_ms
         ) VALUES (
            'test-artifact', $1, 'darwin', 'arm64', 'darwin-dmg', $2,
            'dGVzdA', $3, 'test-build-key',
            'https://bluey.sh/jobs/browser/releases/test-release/Bluey-Browser.dmg',
            'Bluey-Browser.dmg', 1, $4, $5, $6,
            'apple-developer-id', 'TESTTEAM', 1
         ) ON CONFLICT DO NOTHING",
        &[
            &manifest_sha256,
            &descriptor_sha256,
            &signature,
            &artifact_sha256,
            &app_content_sha256,
            &verification_evidence_sha256,
        ],
    )
    .expect("insert PostgreSQL Browser artifact");
    conn.execute(
        "INSERT INTO jobs_browser_release_artifact_runtime_components (
            manifest_sha256, artifact_id, build_descriptor_sha256,
            artifact_sha256, platform, architecture, package_kind,
            automation_bundle_sha256, chromium_executable_sha256, recorded_at_ms
         ) VALUES (
            $1, 'test-artifact', $2, $3, 'darwin', 'arm64', 'darwin-dmg',
            $4, $5, 1
         ) ON CONFLICT DO NOTHING",
        &[
            &manifest_sha256,
            &descriptor_sha256,
            &artifact_sha256,
            &automation_bundle_sha256,
            &chromium_executable_sha256,
        ],
    )
    .expect("insert PostgreSQL Browser runtime components");
    conn.execute(
        "INSERT INTO jobs_browser_release_activations (
            activation_sha256, activation_id, activation_generation,
            trust_generation, channel, channel_sequence, manifest_sha256,
            manifest_signature_set_sha256,
            authorization_signature_set_sha256,
            accepted_server_release_ids_json, canary_evidence_sha256,
            canonical_activation_base64url, issued_at_ms, expires_at_ms,
            recorded_by, recorded_at_ms
         ) VALUES (
            $1, 'test-activation', 1, 1, 'beta', 1, $2, $3, $4,
            '[\"alternate-test-server\",\"test-server\"]', $5, 'dGVzdA', 1,
            9007199254740991, 'test-suite', 1
         ) ON CONFLICT DO NOTHING",
        &[
            &activation_sha256,
            &manifest_sha256,
            &manifest_signature_set_sha256,
            &activation_signature_set_sha256,
            &"d".repeat(64),
        ],
    )
    .expect("insert PostgreSQL Browser activation");
    conn.execute(
        "INSERT INTO jobs_browser_release_channel_transitions (
            transition_sha256, channel, head_revision,
            previous_head_revision, previous_transition_sha256,
            previous_activation_sha256, previous_manifest_sha256,
            previous_trust_generation, previous_channel_sequence,
            next_activation_sha256, next_manifest_sha256,
            next_trust_generation, next_channel_sequence,
            transition_kind, authority_sha256,
            rollback_authority_sha256, recorded_by, recorded_at_ms
         ) VALUES (
            $1, 'beta', 1, 0, NULL, NULL, NULL, NULL, NULL,
            $2, $3, 1, 1, 'activation', $2, NULL, 'test-suite', 1
         ) ON CONFLICT DO NOTHING",
        &[&transition_sha256, &activation_sha256, &manifest_sha256],
    )
    .expect("insert PostgreSQL Browser channel transition");
    conn.execute(
        "INSERT INTO jobs_browser_release_channel_heads (
            channel, head_revision, current_transition_sha256,
            current_activation_sha256, current_manifest_sha256,
            current_trust_generation, current_channel_sequence, updated_at_ms
         ) VALUES ('beta', 1, $1, $2, $3, 1, 1, 1)
         ON CONFLICT DO NOTHING",
        &[&transition_sha256, &activation_sha256, &manifest_sha256],
    )
    .expect("insert PostgreSQL Browser channel head");
    conn.execute(
        "INSERT INTO jobs_browser_account_channel_assignments (
            assignment_sha256, account_id, assignment_generation,
            predecessor_assignment_sha256, predecessor_generation, channel,
            reason_ref, assigned_by, assigned_at_ms
         ) VALUES ($1, $2, 1, NULL, 0, 'beta', 'test-fixture', 'test-suite', 1)",
        &[&assignment_sha256, &fixture.account_id],
    )
    .expect("insert PostgreSQL Browser account assignment");

    let binding = BrowserReleaseClaimBinding {
        assignment_sha256: assignment_sha256.clone(),
        assignment_generation: 1,
        channel: "beta".to_string(),
        channel_head_revision: 1,
        channel_transition_sha256: transition_sha256.clone(),
        activation_sha256: activation_sha256.clone(),
        activation_generation: 1,
        trust_generation: 1,
        channel_sequence: 1,
        trust_policy_sha256: policy_sha256.clone(),
        manifest_signature_set_sha256: manifest_signature_set_sha256.clone(),
        activation_authorization_signature_set_sha256: activation_signature_set_sha256.clone(),
        manifest_sha256: manifest_sha256.clone(),
        release_sequence: 1,
        artifact_id: "test-artifact".to_string(),
        artifact_sha256: artifact_sha256.clone(),
        artifact_url: "https://bluey.sh/jobs/browser/releases/test-release/Bluey-Browser.dmg"
            .to_string(),
        artifact_filename: "Bluey-Browser.dmg".to_string(),
        artifact_size_bytes: 1,
        package_kind: "darwin-dmg".to_string(),
        release_id: "test-release".to_string(),
        build_id: "browser-1.0".to_string(),
        app_version: "1.0.0".to_string(),
        protocol_version: 1,
        platform: "darwin".to_string(),
        architecture: "arm64".to_string(),
        build_descriptor_sha256: descriptor_sha256.clone(),
        automation_bundle_sha256,
        chromium_executable_sha256,
        published_at_ms: 1,
    };
    let binding_sha256 = browser_release_binding_sha256(&binding);
    conn.execute(
        "INSERT INTO jobs_local_run_release_bindings (
            run_id, account_id, application_id, binding_sha256,
            account_channel_assignment_sha256,
            account_channel_assignment_generation, channel,
            channel_head_revision, channel_transition_sha256,
            activation_sha256, activation_generation, trust_generation,
            trust_policy_sha256, channel_sequence,
            manifest_signature_set_sha256,
            activation_authorization_signature_set_sha256,
            manifest_sha256, artifact_id, release_id, build_id,
            app_version, protocol_version, platform, architecture,
            package_kind, build_descriptor_sha256, artifact_sha256,
            bound_at_ms
         ) VALUES (
            $1, $2, $3, $4, $5, 1, 'beta', 1, $6, $7, 1, 1,
            $8, 1, $9, $10, $11, 'test-artifact', 'test-release',
            'browser-1.0', '1.0.0', 1, 'darwin', 'arm64', 'darwin-dmg',
            $12, $13, 1
         )",
        &[
            &fixture.run_id,
            &fixture.account_id,
            &fixture.application.id,
            &binding_sha256,
            &assignment_sha256,
            &transition_sha256,
            &activation_sha256,
            &policy_sha256,
            &manifest_signature_set_sha256,
            &activation_signature_set_sha256,
            &manifest_sha256,
            &descriptor_sha256,
            &artifact_sha256,
        ],
    )
    .expect("insert PostgreSQL Browser release binding");
}

fn local_authority_fixture(pool: &DbPool, label: &str) -> LocalAuthorityFixture {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let account_id = format!("acct_local_authority_{label}_{suffix}");
    let email = format!("local-authority-{label}-{suffix}@example.test");
    {
        let mut conn = pool.get_pg().expect("get PostgreSQL fixture connection");
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ($1, $2, 'hash', 0)",
            &[&account_id, &email],
        )
        .expect("insert PostgreSQL authority account");
    }

    let identity = ensure_primary_application_identity(pool, &account_id, &email)
        .expect("create PostgreSQL fixture application identity");
    let now = now_ms();
    let source = ResumeSourceAsset {
        id: format!("resume-source-local-authority-{suffix}"),
        file_name: "local-authority-source-resume.pdf".to_string(),
        media_type: "application/pdf".to_string(),
        file_type: "pdf".to_string(),
        storage_key: format!("accounts/{account_id}/jobs/local-authority-source-resume.pdf"),
        sha256: "e".repeat(64),
        size_bytes: 1_024,
        page_count: Some(1),
        template_status: "converted_layout".to_string(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    let mut profile = default_profile(&email);
    profile.onboarding_complete = true;
    profile.source_resume_name = source.file_name.clone();
    profile.source_resume_asset_id = source.id.clone();
    profile.source_resume_sha256 = source.sha256.clone();
    profile.source_resume_media_type = source.media_type.clone();
    profile.source_resume_template_status = source.template_status.clone();
    let (_, profile) = save_resume_source_asset(pool, &account_id, &source, &profile)
        .expect("save PostgreSQL fixture source resume");
    let preferences = save_preferences(
        pool,
        &account_id,
        &JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        },
    )
    .expect("save PostgreSQL fixture preferences");
    let track = upsert_track(
        pool,
        &account_id,
        &CareerTrack {
            id: format!("track-local-authority-{suffix}"),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: Some(identity.id),
            policy: CareerTrackPolicy {
                role_family: "software_engineering".to_string(),
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .expect("create approved PostgreSQL fixture Career Track");
    assert_eq!(track.policy.authority.review_state, "approved");
    set_entitlement_plan(pool, &account_id, "pro")
        .expect("enable PostgreSQL local-browser entitlement");
    let canonical_url = format!("https://boards.greenhouse.io/acme/jobs/{label}-{suffix}");
    let posting = JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse_import".to_string(),
        external_id: format!("{label}-{suffix}"),
        company: "Acme".to_string(),
        title: "Software Engineer".to_string(),
        location: "New York, NY".to_string(),
        workplace: "hybrid".to_string(),
        canonical_url,
        description: "Build reliable products with Rust and TypeScript.".to_string(),
        compensation: "$170k-$200k".to_string(),
        employment_type: "full_time".to_string(),
        track_id: track.id,
        match_score: 0,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: Some(now - 60_000),
        last_verified_at_ms: Some(now),
        availability_status: "active".to_string(),
        status: "matched".to_string(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    };
    let (posting, managed) = save_production_positive_verified_import(
        pool,
        &account_id,
        &posting,
        &profile,
        &preferences,
    );
    let authorities = install_production_positive_job_authorities(
        pool,
        &account_id,
        &posting,
        &managed,
        "acme.com",
        &format!("{label}-{suffix}"),
    );
    let posting = authorities.posting;
    let (application, _) =
        prepare_application(pool, &account_id, &posting.id, "factual", "review_first")
            .expect("prepare PostgreSQL application");
    let authority =
        current_application_approval_authority(pool, &account_id, &email, &application.id)
            .expect("resolve PostgreSQL current application approval authority")
            .expect("PostgreSQL current application approval authority exists");
    let resume = get_resume_version(
        pool,
        &account_id,
        authority
            .application
            .resume_version_id
            .as_deref()
            .expect("fixture application has a resume"),
    )
    .expect("load PostgreSQL resume")
    .expect("PostgreSQL resume exists");
    let identity_id = authority
        .application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .expect("frozen application identity")
        .to_string();
    let identity_email = authority
        .application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .expect("frozen application email");
    let browser_profile_id = execution_browser_profile_id(&account_id, &identity_id);
    let approved_packet = json!({
        "applicationId": authority.application.id,
        "jobId": authority.posting.id,
        "resumeVersionId": resume.id,
        "resumeContent": resume.content,
        "coverLetterContent": authority.application.cover_letter,
        "answers": {},
        "verifiedClaimIds": [],
        "applicationIdentityId": identity_id,
        "applicationEmail": identity_email,
        "browserProfileId": browser_profile_id,
    });
    let approved_job = json!({
        "externalId": authority.posting.external_id,
        "canonicalUrl": authority.posting.canonical_url,
        "company": authority.posting.company,
        "title": authority.posting.title,
        "location": authority.posting.location,
        "workplace": authority.posting.workplace,
        "description": authority.posting.description,
        "source": authority.posting.source,
        "compensation": authority.posting.compensation,
    });
    let admission = json!({ "kind": "review_approval" });
    let approved_execution = json!({
        "schema_version": 2,
        "approved_at_ms": authority.evaluated_at_ms,
        "checksum": approved_submission_checksum(
            2,
            &approved_packet,
            &approved_job,
            Some(&admission),
        )
        .expect("checksum PostgreSQL approved execution"),
        "admission": admission,
        "packet": approved_packet,
        "job": approved_job,
    });
    let application = persist_current_application_approval(
        pool,
        &account_id,
        &email,
        &authority,
        &approved_execution,
    )
    .expect("persist PostgreSQL current application approval")
    .expect("PostgreSQL approved application exists");
    reserve_application_attempt(pool, &account_id, &application.id, "local")
        .expect("reserve PostgreSQL local application attempt");
    let application = update_application(pool, &account_id, &application.id, "queued", None)
        .expect("queue PostgreSQL application")
        .expect("queued PostgreSQL application");
    let run_id = format!("local-run-{label}-{suffix}");
    upsert_browser_session(
        pool,
        &account_id,
        &BrowserSession {
            id: run_id.clone(),
            runner: "local".to_string(),
            status: "queued".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Waiting for Bluey Browser".to_string(),
            application_id: Some(application.id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .expect("save PostgreSQL browser session");
    let application = assign_application_run(pool, &account_id, &application.id, &run_id)
        .expect("bind PostgreSQL application run")
        .expect("bound PostgreSQL application run");
    let ticket_hash = format!("ticket-hash-{label}-{suffix}");
    save_local_run_ticket(
        pool,
        &account_id,
        &application.id,
        &run_id,
        &ticket_hash,
        &format!("ticket-secret-{label}-{suffix}"),
        json!({
            "accountId": account_id,
            "applicationId": application.id,
            "jobId": posting.id,
            "applicationIdentityId": identity_id,
            "browserProfileId": browser_profile_id,
            "runner": "local",
            "url": posting.canonical_url,
            "runId": run_id,
        }),
        now_ms() + 60_000,
    )
    .expect("save PostgreSQL local-run ticket");

    let fixture = LocalAuthorityFixture {
        account_id,
        application,
        posting,
        run_id,
        ticket_hash,
        identity_id,
    };
    seed_postgres_browser_release_binding(pool, &fixture);
    fixture
}

#[test]
#[serial_test::serial]
fn postgres_canonical_track_policy_ledger_encrypts_and_rejects_projection_drift() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let account_id = format!("acct_track_ledger_pg_{suffix}");
    let email = format!("track-ledger-pg-{suffix}@example.test");
    pool.get_pg()
        .expect("get PostgreSQL ledger fixture connection")
        .execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ($1, $2, 'hash', 0)",
            &[&account_id, &email],
        )
        .expect("insert PostgreSQL ledger account");
    let identity = ensure_primary_application_identity(&pool, &account_id, &email)
        .expect("create PostgreSQL verified application identity");
    let now = now_ms();
    let asset = ResumeSourceAsset {
        id: format!("resume-source-ledger-{suffix}"),
        file_name: "private-postgres-ledger-resume.pdf".to_string(),
        media_type: "application/pdf".to_string(),
        file_type: "pdf".to_string(),
        storage_key: format!("jobs/{account_id}/private-postgres-ledger-resume.pdf"),
        sha256: "a".repeat(64),
        size_bytes: 2_048,
        page_count: Some(2),
        template_status: "converted_layout".to_string(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    let mut profile = default_profile(&email);
    profile.onboarding_complete = true;
    profile.source_resume_name = asset.file_name.clone();
    profile.source_resume_asset_id = asset.id.clone();
    profile.source_resume_sha256 = asset.sha256.clone();
    profile.source_resume_media_type = asset.media_type.clone();
    profile.source_resume_template_status = asset.template_status.clone();
    let (_, profile) = save_resume_source_asset(&pool, &account_id, &asset, &profile)
        .expect("save PostgreSQL ledger source resume");
    let track = upsert_track(
        &pool,
        &account_id,
        &CareerTrack {
            id: format!("track-ledger-{suffix}"),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: Some(identity.id),
            policy: CareerTrackPolicy {
                employment_types: vec!["full_time".to_string()],
                work_authorizations: vec!["us_citizen".to_string()],
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .expect("create approved PostgreSQL ledger Track");
    assert_eq!(track.policy.authority.review_state, "approved");
    let preferences = get_preferences(&pool, &account_id).expect("load PostgreSQL preferences");
    let mut posting = greenhouse_posting(&format!(
        "https://boards.greenhouse.io/acme/jobs/track-ledger-{suffix}"
    ));
    posting.track_id = track.id.clone();
    let posting = upsert_posting(&pool, &account_id, &posting, &profile, &preferences)
        .expect("save PostgreSQL ledger posting");

    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL ledger assertion connection");
    let evidence = conn
        .query_one(
            "SELECT revision.canonical_policy_ciphertext,
                    receipt.canonical_review_receipt_ciphertext
               FROM jobs_track_policy_revisions AS revision
               JOIN jobs_track_policy_review_receipts AS receipt
                 ON receipt.account_id = revision.account_id
                AND receipt.career_track_id = revision.career_track_id
                AND receipt.policy_revision_id = revision.revision_id
              WHERE revision.account_id = $1 AND revision.career_track_id = $2",
            &[&account_id, &track.id],
        )
        .expect("read PostgreSQL encrypted policy evidence");
    for ciphertext in [evidence.get::<_, String>(0), evidence.get::<_, String>(1)] {
        assert!(ciphertext.starts_with(ENCRYPTED_PAYLOAD_PREFIX));
        assert!(!ciphertext.contains("New York"));
        assert!(!ciphertext.contains("us_citizen"));
    }
    let exported = export_canonical_track_policy_ledger(&pool, &account_id)
        .expect("export PostgreSQL canonical policy ledger");
    assert_eq!(exported.revisions.len(), 1);
    assert_eq!(exported.review_receipts.len(), 1);
    assert_eq!(exported.heads.len(), 1);

    let track_raw: String = conn
        .query_one(
            "SELECT track_json FROM jobs_tracks
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &track.id],
        )
        .expect("read PostgreSQL Track projection")
        .get(0);
    let mut drifted: CareerTrack =
        parse_json(track_raw, "PostgreSQL Track projection").expect("decrypt Track projection");
    drifted.locations = vec!["San Francisco, CA".to_string()];
    let drifted_raw = to_json(&drifted, "drifted PostgreSQL Track projection")
        .expect("encrypt drifted PostgreSQL Track projection");
    conn.execute(
        "UPDATE jobs_tracks SET track_json = $3
          WHERE account_id = $1 AND id = $2",
        &[&account_id, &track.id, &drifted_raw],
    )
    .expect("tamper PostgreSQL mutable Track projection");
    drop(conn);

    let projected = list_tracks(&pool, &account_id)
        .expect("list drifted PostgreSQL Track")
        .remove(0);
    assert_eq!(projected.policy.authority.review_state, "needs_review");
    assert_eq!(
        projected.policy.authority.review_reason_codes,
        vec!["policy_ledger_review_required"]
    );
    let application = JobApplication {
        id: format!("application-ledger-{suffix}"),
        job_id: posting.id,
        resume_version_id: None,
        state: "ready".to_string(),
        submission_mode: "review_first".to_string(),
        match_score: 90,
        answers: Vec::new(),
        cover_letter: String::new(),
        receipt: Value::Null,
        run_id: None,
        created_at_ms: now,
        updated_at_ms: now,
        submitted_at_ms: None,
    };
    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL execution assertion connection");
    let mut tx = conn
        .transaction()
        .expect("start PostgreSQL execution assertion transaction");
    lock_operational_hold_shared_postgres_tx(&mut tx)
        .expect("lock PostgreSQL operational-hold authority");
    lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
        .expect("lock PostgreSQL managed-release authority");
    lock_postgres_ats_certification(&mut tx).expect("lock PostgreSQL ATS authority");
    lock_discovery_account_shared_postgres(&mut tx, &account_id)
        .expect("lock PostgreSQL discovery-account authority");
    assert!(!current_execution_authorized_postgres_after_prelock(
        &mut tx,
        &account_id,
        &application,
        ExecutionAuthorityRunner::Local,
    )
    .expect("check PostgreSQL drifted execution authority"));
    tx.rollback()
        .expect("rollback PostgreSQL execution assertion transaction");
    pool.get_pg()
        .expect("get PostgreSQL ledger cleanup connection")
        .execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
        .expect("delete PostgreSQL ledger fixture");
}

fn resume_upload_input_for_postgres(
    account_id: &str,
    asset_id: &str,
    requested_profile_sha256: &str,
    now: i64,
) -> crate::db::object_uploads::NewObjectUpload {
    crate::db::object_uploads::NewObjectUpload {
        account_id: account_id.to_string(),
        object_kind: crate::db::object_uploads::ObjectKind::Artifact,
        logical_id: format!("jobs-resume-source:{asset_id}"),
        session_id: None,
        storage_scope: crate::db::object_uploads::StorageScope::Artifact,
        object_key: format!("accounts/{account_id}/jobs/resume-sources/{asset_id}.pdf"),
        size_bytes: 1_024,
        sha256: "d".repeat(64),
        content_type: "application/pdf".to_string(),
        expires_at_ms: i64::MAX,
        metadata_json: json!({
            "artifact_class": "jobs_resume_source",
            "jobs_resume_source_asset_id": asset_id,
            "request_id": asset_id,
            "profile_mode": "replace",
            "base_profile_sha256": null,
            "requested_profile_sha256": requested_profile_sha256,
            "replaces_source_asset_id": null,
            "file_name": "resume-lock-order.pdf",
            "file_type": "pdf",
            "media_type": "application/pdf",
            "page_count": 1,
            "retention_policy": "account_lifetime_until_deletion",
        }),
        now_ms: now,
        limits: crate::object_storage::UploadLimits {
            max_object_bytes: 1024 * 1024,
            max_account_bytes: 16 * 1024 * 1024,
            max_daily_bytes: 16 * 1024 * 1024,
            max_account_objects: 100,
        },
    }
}

#[test]
#[serial_test::serial]
fn postgres_resume_publication_locks_account_before_policy_children() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let now = now_ms();

    // Prove the preceding account-object reservation uses the same logical-object -> account
    // order. Deletion owns the parent row, commits its durable fence, and the blocked reservation
    // must then fail closed without creating an upload.
    let reservation_suffix = uuid::Uuid::new_v4().simple().to_string();
    let reservation_account_id = format!("acct_resume_reservation_fence_{reservation_suffix}");
    let reservation_email = format!("resume-reservation-fence-{reservation_suffix}@example.test");
    pool.get_pg()
        .expect("get PostgreSQL resume-reservation fixture connection")
        .execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ($1, $2, 'hash', 0)",
            &[&reservation_account_id, &reservation_email],
        )
        .expect("insert PostgreSQL resume-reservation account");
    let reservation_asset_id = format!("resume-reservation-fence-{reservation_suffix}");
    let reservation_input = resume_upload_input_for_postgres(
        &reservation_account_id,
        &reservation_asset_id,
        &"a".repeat(64),
        now,
    );
    let reservation_logical_id = reservation_input.logical_id.clone();

    let mut deletion_conn = pool
        .get_pg()
        .expect("get PostgreSQL reservation-race deletion connection");
    let mut deletion_tx = deletion_conn
        .transaction()
        .expect("begin PostgreSQL reservation-race deletion transaction");
    let deletion_pid = deletion_tx
        .query_one("SELECT pg_backend_pid()", &[])
        .expect("query PostgreSQL reservation-race deletion pid")
        .get::<_, i32>(0);
    assert!(deletion_tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&reservation_account_id],
        )
        .expect("lock PostgreSQL resume-reservation parent account")
        .is_some());

    let (reservation_finished_tx, reservation_finished_rx) = std::sync::mpsc::channel();
    let reservation_pool = pool.clone();
    let reservation_worker = std::thread::spawn(move || {
        reservation_finished_tx
            .send(crate::db::object_uploads::reserve_account_object_upload(
                &reservation_pool,
                &reservation_input,
            ))
            .expect("send PostgreSQL resume-reservation result");
    });
    let mut reservation_waits_on_account = false;
    for _ in 0..200 {
        reservation_waits_on_account = deletion_tx
            .query_one(
                "SELECT EXISTS (
                    SELECT 1 FROM pg_stat_activity AS activity
                     WHERE activity.pid <> $1
                       AND $1 = ANY(pg_blocking_pids(activity.pid))
                 )",
                &[&deletion_pid],
            )
            .expect("observe PostgreSQL resume reservation waiting on deletion")
            .get::<_, bool>(0);
        if reservation_waits_on_account {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        reservation_waits_on_account,
        "resume reservation did not reach the held parent-account fence"
    );
    deletion_tx
        .execute(
            "INSERT INTO account_deletion_intents (
                account_id, requested_at_ms, last_checked_at_ms,
                fresh_upload_cutoff_ms, fresh_in_flight_puts
             ) VALUES ($1, $2, $2, $2, 0)",
            &[&reservation_account_id, &now],
        )
        .expect("insert PostgreSQL resume-reservation deletion intent");
    deletion_tx
        .commit()
        .expect("commit PostgreSQL resume-reservation deletion fence");
    let reservation_error = reservation_finished_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("resume reservation completes after deletion commits")
        .expect_err("a committed deletion fence must reject resume reservation");
    reservation_worker
        .join()
        .expect("join PostgreSQL resume-reservation worker");
    assert!(matches!(
        reservation_error.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
        Some(crate::db::object_uploads::UploadControlError::AccountDeleting)
    ));
    let mut reservation_cleanup_conn = pool
        .get_pg()
        .expect("get PostgreSQL resume-reservation cleanup connection");
    let reservation_upload_count = reservation_cleanup_conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM object_uploads
              WHERE account_id = $1 AND logical_id = $2",
            &[&reservation_account_id, &reservation_logical_id],
        )
        .expect("count raced PostgreSQL resume reservations")
        .get::<_, i64>(0);
    assert_eq!(reservation_upload_count, 0);
    reservation_cleanup_conn
        .execute(
            "DELETE FROM account_deletion_intents WHERE account_id = $1",
            &[&reservation_account_id],
        )
        .expect("delete PostgreSQL resume-reservation deletion intent");
    reservation_cleanup_conn
        .execute(
            "DELETE FROM accounts WHERE id = $1",
            &[&reservation_account_id],
        )
        .expect("delete PostgreSQL resume-reservation account");

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let account_id = format!("acct_resume_publication_fence_{suffix}");
    let email = format!("resume-publication-fence-{suffix}@example.test");
    pool.get_pg()
        .expect("get PostgreSQL resume-publication fixture connection")
        .execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ($1, $2, 'hash', 0)",
            &[&account_id, &email],
        )
        .expect("insert PostgreSQL resume-publication account");

    let mut requested_profile = default_profile(&email);
    requested_profile.headline = "Resume publication lock-order test".to_string();
    requested_profile.onboarding_complete = true;
    let requested_profile_sha256 = resume_requested_profile_sha256(&requested_profile)
        .expect("hash requested PostgreSQL resume profile");
    let asset_id = format!("resume-publication-fence-{suffix}");
    let upload_input =
        resume_upload_input_for_postgres(&account_id, &asset_id, &requested_profile_sha256, now);
    let logical_id = upload_input.logical_id.clone();
    let reservation =
        crate::db::object_uploads::reserve_account_object_upload(&pool, &upload_input)
            .expect("reserve PostgreSQL resume-publication object");
    assert!(reservation.needs_put);
    let asset = ResumeSourceAsset {
        id: asset_id,
        file_name: "resume-lock-order.pdf".to_string(),
        media_type: reservation.upload.content_type.clone(),
        file_type: "pdf".to_string(),
        storage_key: reservation.upload.object_key.clone(),
        sha256: reservation.upload.sha256.clone(),
        size_bytes: reservation.upload.size_bytes,
        page_count: Some(1),
        template_status: "converted_layout".to_string(),
        created_at_ms: reservation.upload.created_at_ms,
        updated_at_ms: reservation.upload.created_at_ms,
    };

    let mut blocker_conn = pool
        .get_pg()
        .expect("get PostgreSQL resume logical-object blocker connection");
    let mut blocker_tx = blocker_conn
        .transaction()
        .expect("begin PostgreSQL resume logical-object blocker transaction");
    let blocker_pid = blocker_tx
        .query_one("SELECT pg_backend_pid()", &[])
        .expect("query PostgreSQL resume logical-object blocker pid")
        .get::<_, i32>(0);
    let logical_lock_key =
        crate::db::object_uploads::context_artifact_advisory_lock_key(&logical_id);
    blocker_tx
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))",
            &[&logical_lock_key],
        )
        .expect("hold PostgreSQL resume logical-object publication lock");

    let (publication_started_tx, publication_started_rx) = std::sync::mpsc::channel();
    let (publication_finished_tx, publication_finished_rx) = std::sync::mpsc::channel();
    let publication_pool = pool.clone();
    let publication_account_id = account_id.clone();
    let publication_asset = asset.clone();
    let publication_profile = requested_profile.clone();
    let publication_upload_id = reservation.upload.id.clone();
    let publication_worker = std::thread::spawn(move || {
        publication_started_tx
            .send(())
            .expect("signal PostgreSQL resume publication start");
        publication_finished_tx
            .send(publish_resume_source_asset(
                &publication_pool,
                &publication_account_id,
                &publication_asset,
                &publication_profile,
                &publication_upload_id,
            ))
            .expect("send PostgreSQL resume publication result");
    });
    publication_started_rx
        .recv()
        .expect("wait for PostgreSQL resume publication start");

    let mut publication_waits_on_logical_object = false;
    for _ in 0..200 {
        publication_waits_on_logical_object = blocker_tx
            .query_one(
                "SELECT EXISTS (
                    SELECT 1
                      FROM pg_locks AS holder
                      JOIN pg_locks AS waiter
                        ON waiter.locktype = holder.locktype
                       AND waiter.database = holder.database
                       AND waiter.classid = holder.classid
                       AND waiter.objid = holder.objid
                       AND waiter.objsubid = holder.objsubid
                     WHERE holder.pid = $1
                       AND holder.locktype = 'advisory'
                       AND holder.granted
                       AND NOT waiter.granted
                       AND waiter.pid <> holder.pid
                 )",
                &[&blocker_pid],
            )
            .expect("observe PostgreSQL resume publication advisory waiter")
            .get::<_, bool>(0);
        if publication_waits_on_logical_object {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        publication_waits_on_logical_object,
        "resume publication did not reach the held logical-object fence"
    );

    let (deletion_finished_tx, deletion_finished_rx) = std::sync::mpsc::channel();
    let deletion_pool = pool.clone();
    let deletion_account_id = account_id.clone();
    let deletion_worker = std::thread::spawn(move || {
        deletion_finished_tx
            .send(crate::db::account_data::begin_account_deletion(
                &deletion_pool,
                &deletion_account_id,
                now.saturating_add(1),
            ))
            .expect("send PostgreSQL account-deletion result");
    });
    let deletion_before_publication_release =
        deletion_finished_rx.recv_timeout(std::time::Duration::from_secs(5));

    blocker_tx
        .commit()
        .expect("release PostgreSQL resume logical-object publication lock");
    let publication_result = publication_finished_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("resume publication completes after logical-object fence release");
    publication_worker
        .join()
        .expect("join PostgreSQL resume publication worker");
    let deletion_completed_while_publication_waited = deletion_before_publication_release.is_ok();
    let deletion_result = match deletion_before_publication_release {
        Ok(result) => result,
        Err(_) => deletion_finished_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("account deletion completes after publication lock release"),
    };
    deletion_worker
        .join()
        .expect("join PostgreSQL account-deletion worker");

    let mut cleanup_conn = pool
        .get_pg()
        .expect("get PostgreSQL resume-publication cleanup connection");
    cleanup_conn
        .execute(
            "DELETE FROM account_deletion_intents WHERE account_id = $1",
            &[&account_id],
        )
        .expect("delete PostgreSQL resume-publication deletion intent");
    cleanup_conn
        .execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
        .expect("delete PostgreSQL resume-publication account");

    assert!(
        deletion_completed_while_publication_waited,
        "account deletion must acquire the parent account while publication waits before it"
    );
    let deletion = deletion_result
        .expect("begin PostgreSQL account deletion")
        .expect("PostgreSQL resume-publication account exists");
    assert!(matches!(
        deletion,
        crate::db::account_data::BeginAccountDeletionResult::WaitingForUploads(ref intent)
            if intent.fresh_in_flight_puts == 1
    ));
    let publication_error =
        publication_result.expect_err("a committed deletion fence must reject resume publication");
    assert!(matches!(
        publication_error.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
        Some(crate::db::object_uploads::UploadControlError::AccountDeleting)
    ));
}

fn local_final_submit_proof(fixture: &LocalAuthorityFixture) -> FinalSubmitProof {
    let provider_job_key =
        final_submit_provider_job_key("greenhouse", &fixture.posting.canonical_url)
            .expect("test final-submit URL has a provider job key");
    FinalSubmitProof {
        schema_version: 3,
        adapter: "greenhouse".to_string(),
        adapter_version: "2026.07.1-beta.1".to_string(),
        control: "greenhouse_submit_application".to_string(),
        job: FinalSubmitJobProof {
            approved_canonical_url: fixture.posting.canonical_url.clone(),
            page_url: fixture.posting.canonical_url.clone(),
        },
        target: FinalSubmitTargetProof {
            action_url: fixture.posting.canonical_url.clone(),
            method: "post".to_string(),
            enctype: "multipart/form-data".to_string(),
            form_target: "_self".to_string(),
            provider_job_key,
            form_identity: r#"[0,"application-form","","","","",""]"#.to_string(),
        },
        files: vec![FinalSubmitFileProof {
            field_name: "resume".to_string(),
            name: format!("resume-{}.pdf", "b".repeat(64)),
            byte_length: 1_024,
            sha256: "b".repeat(64),
        }],
        fields: vec![FinalSubmitFieldProof {
            field_name: "candidate_name".to_string(),
            value_byte_length: 0,
            value_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
        }],
        part_order: vec![
            FinalSubmitPartOrderProof {
                kind: "field".to_string(),
                index: 0,
            },
            FinalSubmitPartOrderProof {
                kind: "file".to_string(),
                index: 0,
            },
        ],
        documents: vec![FinalSubmitDocumentProof {
            kind: "resume".to_string(),
            version_id: fixture.application.resume_version_id.clone(),
            sha256: "b".repeat(64),
        }],
        certification: None,
        observed_surface: None,
    }
}

fn set_identity_status(pool: &DbPool, fixture: &LocalAuthorityFixture, status: &str) {
    pool.get_pg()
        .expect("get PostgreSQL identity connection")
        .execute(
            "UPDATE jobs_application_identities SET verification_status = $3
              WHERE account_id = $1 AND id = $2",
            &[&fixture.account_id, &fixture.identity_id, &status],
        )
        .expect("change PostgreSQL identity verification status");
}

fn running_session(fixture: &LocalAuthorityFixture) -> BrowserSession {
    BrowserSession {
        id: fixture.run_id.clone(),
        runner: "local".to_string(),
        status: "running".to_string(),
        current_company: "Acme".to_string(),
        current_step: "Ready to submit".to_string(),
        application_id: Some(fixture.application.id.clone()),
        takeover_url: None,
        created_at_ms: 0,
        updated_at_ms: 0,
    }
}

fn enter_running_state(pool: &DbPool, fixture: &LocalAuthorityFixture) {
    update_application(
        pool,
        &fixture.account_id,
        &fixture.application.id,
        "running",
        None,
    )
    .expect("start PostgreSQL application")
    .expect("running PostgreSQL application");
    assert!(update_attempt_reservation_status(
        pool,
        &fixture.account_id,
        &fixture.application.id,
        "running",
    )
    .expect("start PostgreSQL attempt reservation"));
    upsert_browser_session(pool, &fixture.account_id, &running_session(fixture))
        .expect("start PostgreSQL browser session");
}

fn cleanup(pool: &DbPool, fixture: &LocalAuthorityFixture) {
    let mut conn = pool.get_pg().expect("get PostgreSQL cleanup connection");
    conn.execute(
        "DELETE FROM account_deletion_intents WHERE account_id = $1",
        &[&fixture.account_id],
    )
    .expect("remove PostgreSQL fixture deletion fence");
    conn.execute("DELETE FROM accounts WHERE id = $1", &[&fixture.account_id])
        .expect("delete PostgreSQL authority fixture");
}

#[test]
#[serial_test::serial]
fn postgres_execution_lease_claim_observes_account_write_fence_first() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let fixture = local_authority_fixture(&pool, "execution-claim-account-fence");
    let now = now_ms();
    pool.get_pg()
        .expect("get PostgreSQL account-fence setup connection")
        .execute(
            "INSERT INTO account_deletion_intents (
                account_id, requested_at_ms, last_checked_at_ms,
                fresh_upload_cutoff_ms, fresh_in_flight_puts
             ) VALUES ($1, $2, $2, $2, 0)",
            &[&fixture.account_id, &now],
        )
        .expect("fence PostgreSQL account before execution claim");

    let error = claim_execution_lease(
        &pool,
        &fixture.account_id,
        &fixture.application.id,
        &fixture.run_id,
        "profile-that-must-not-be-resolved",
        "fenced-postgres-worker",
    )
    .unwrap_err();
    match error {
        ExecutionLeaseError::Storage(error) => assert!(matches!(
            error.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
            Some(crate::db::object_uploads::UploadControlError::AccountDeleting)
        )),
        other => panic!("expected PostgreSQL account-deletion fence, got {other:?}"),
    }
    let lease_count: i64 = pool
        .get_pg()
        .expect("get PostgreSQL claim-fence assertion connection")
        .query_one(
            "SELECT COUNT(*) FROM jobs_execution_leases WHERE account_id = $1",
            &[&fixture.account_id],
        )
        .expect("count PostgreSQL leases after fenced claim")
        .get(0);
    assert_eq!(lease_count, 0);

    cleanup(&pool, &fixture);
}

#[test]
#[serial_test::serial]
fn postgres_local_run_claim_and_submit_recheck_current_authority() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let claim_gate = local_authority_fixture(&pool, "claim-gate");
    set_entitlement_plan(&pool, &claim_gate.account_id, "free")
        .expect("downgrade PostgreSQL authority account");
    assert!(
        claim_authorized_local_run_ticket(&pool, &claim_gate.run_id, &claim_gate.ticket_hash)
            .expect("check downgraded PostgreSQL claim")
            .is_none(),
        "a downgraded account must not claim a local run"
    );
    set_entitlement_plan(&pool, &claim_gate.account_id, "pro")
        .expect("restore PostgreSQL authority account");
    set_identity_status(&pool, &claim_gate, "pending");
    assert!(
        claim_authorized_local_run_ticket(&pool, &claim_gate.run_id, &claim_gate.ticket_hash)
            .expect("check identity-revoked PostgreSQL claim")
            .is_none(),
        "an unverified identity must not claim a local run"
    );
    set_identity_status(&pool, &claim_gate, "verified");
    let mut wrong_binding = BrowserSession {
        id: claim_gate.run_id.clone(),
        runner: "local".to_string(),
        status: "queued".to_string(),
        current_company: "Acme".to_string(),
        current_step: "Waiting for Bluey Browser".to_string(),
        application_id: Some("another-application".to_string()),
        takeover_url: None,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    upsert_browser_session(&pool, &claim_gate.account_id, &wrong_binding)
        .expect("save mismatched PostgreSQL browser binding");
    assert!(
        claim_authorized_local_run_ticket(&pool, &claim_gate.run_id, &claim_gate.ticket_hash)
            .expect("check mismatched PostgreSQL claim")
            .is_none(),
        "a mismatched browser session must not claim a local run"
    );
    wrong_binding.application_id = Some(claim_gate.application.id.clone());
    upsert_browser_session(&pool, &claim_gate.account_id, &wrong_binding)
        .expect("restore PostgreSQL browser binding");
    assert!(
        claim_authorized_local_run_ticket(&pool, &claim_gate.run_id, &claim_gate.ticket_hash)
            .expect("claim restored PostgreSQL authority")
            .is_some(),
        "restoring every live binding should allow one claim"
    );

    let deleted_identity = local_authority_fixture(&pool, "identity-deleted");
    pool.get_pg()
        .expect("get PostgreSQL identity deletion connection")
        .execute(
            "DELETE FROM jobs_application_identities WHERE account_id = $1 AND id = $2",
            &[&deleted_identity.account_id, &deleted_identity.identity_id],
        )
        .expect("delete PostgreSQL application identity");
    assert!(
        claim_authorized_local_run_ticket(
            &pool,
            &deleted_identity.run_id,
            &deleted_identity.ticket_hash,
        )
        .expect("check identity-deleted PostgreSQL claim")
        .is_none(),
        "a deleted identity must fail closed"
    );

    let live = local_authority_fixture(&pool, "claim-race");
    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let pool = pool.clone();
            let run_id = live.run_id.clone();
            let ticket_hash = live.ticket_hash.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                    .expect("race PostgreSQL local-run claim")
                    .is_some()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        handles
            .into_iter()
            .map(|handle| handle.join().expect("join PostgreSQL claim thread"))
            .filter(|won| *won)
            .count(),
        1,
        "PostgreSQL must commit exactly one local-run claim winner"
    );
    enter_running_state(&pool, &live);
    let live_capacity = local_submission_capacity(&live);
    let live_proof = local_final_submit_proof(&live);
    assert!(live.posting.canonical_url.contains("boards.greenhouse.io"));
    assert!(
        !local_submission_approval_consumed(
            &pool,
            &live.account_id,
            &live.application.id,
            &live.run_id,
        )
        .expect("check unapproved PostgreSQL provider submission"),
        "Greenhouse submission requires a consumed review approval"
    );

    set_entitlement_plan(&pool, &live.account_id, "free")
        .expect("downgrade live PostgreSQL authority account");
    assert!(!local_run_submit_authorized(
        &pool,
        &live.run_id,
        &live.ticket_hash,
        &live_proof,
        &live_capacity,
    )
    .expect("recheck downgraded PostgreSQL submit authority"));
    set_entitlement_plan(&pool, &live.account_id, "pro")
        .expect("restore live PostgreSQL authority account");
    set_identity_status(&pool, &live, "pending");
    assert!(!local_run_submit_authorized(
        &pool,
        &live.run_id,
        &live.ticket_hash,
        &live_proof,
        &live_capacity,
    )
    .expect("recheck revoked-identity PostgreSQL submit authority"));
    set_identity_status(&pool, &live, "verified");
    let mut mismatched_session = running_session(&live);
    mismatched_session.application_id = Some("another-application".to_string());
    upsert_browser_session(&pool, &live.account_id, &mismatched_session)
        .expect("save mismatched running PostgreSQL session");
    assert!(!local_run_submit_authorized(
        &pool,
        &live.run_id,
        &live.ticket_hash,
        &live_proof,
        &live_capacity,
    )
    .expect("recheck mismatched PostgreSQL submit binding"));
    upsert_browser_session(&pool, &live.account_id, &running_session(&live))
        .expect("restore running PostgreSQL session");

    update_application(
        &pool,
        &live.account_id,
        &live.application.id,
        "needs_input",
        None,
    )
    .expect("pause PostgreSQL application for review")
    .expect("paused PostgreSQL application");
    let mut needs_input_session = running_session(&live);
    needs_input_session.status = "needs_input".to_string();
    needs_input_session.current_step = "Review before submit".to_string();
    upsert_browser_session(&pool, &live.account_id, &needs_input_session)
        .expect("pause PostgreSQL browser session for review");
    assert!(
        update_local_run_ticket_status(&pool, &live.run_id, &live.ticket_hash, "needs_input")
            .expect("pause PostgreSQL local-run ticket")
    );
    let intervention = save_intervention(
        &pool,
        &live.account_id,
        &Intervention {
            id: String::new(),
            application_id: Some(live.application.id.clone()),
            kind: "browser_takeover".to_string(),
            status: "approved".to_string(),
            title: "Review the Greenhouse application".to_string(),
            detail: "Review every employer-facing field before final submission.".to_string(),
            choices: Vec::new(),
            resolution_kind: "browser_takeover".to_string(),
            resume_after_resolution: true,
            provider: String::new(),
            provider_message_id: String::new(),
            expires_at_ms: None,
            metadata: json!({}),
            created_at_ms: 0,
            resolved_at_ms: None,
        },
    )
    .expect("save approved PostgreSQL intervention");
    approve_local_run_resume_action(
        &pool,
        &live.account_id,
        &live.application.id,
        &live.run_id,
        &intervention.id,
    )
    .expect("approve PostgreSQL submission resume")
    .expect("approved PostgreSQL submission resume");
    consume_local_run_resume_action(&pool, &live.run_id, &live.ticket_hash)
        .expect("consume PostgreSQL submission approval")
        .expect("consumed PostgreSQL submission approval");
    update_application(
        &pool,
        &live.account_id,
        &live.application.id,
        "running",
        None,
    )
    .expect("resume PostgreSQL application")
    .expect("resumed PostgreSQL application");
    upsert_browser_session(&pool, &live.account_id, &running_session(&live))
        .expect("resume PostgreSQL browser session");
    assert!(local_run_submit_authorized(
        &pool,
        &live.run_id,
        &live.ticket_hash,
        &live_proof,
        &live_capacity,
    )
    .expect("check final PostgreSQL submit authority"));
    assert!(
        local_submission_approval_consumed(
            &pool,
            &live.account_id,
            &live.application.id,
            &live.run_id,
        )
        .expect("check consumed PostgreSQL provider approval"),
        "the composite Greenhouse authorize-submit gate should now pass"
    );

    pool.get_pg()
        .expect("get PostgreSQL final identity deletion connection")
        .execute(
            "DELETE FROM jobs_application_identities WHERE account_id = $1 AND id = $2",
            &[&live.account_id, &live.identity_id],
        )
        .expect("delete live PostgreSQL application identity");
    assert!(!local_run_submit_authorized(
        &pool,
        &live.run_id,
        &live.ticket_hash,
        &live_proof,
        &live_capacity,
    )
    .expect("recheck identity-deleted PostgreSQL submit authority"));

    cleanup(&pool, &claim_gate);
    cleanup(&pool, &deleted_identity);
    cleanup(&pool, &live);
}

#[test]
#[serial_test::serial]
fn postgres_local_unknown_before_authorize_reserves_and_replays_capacity() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let fixture = local_authority_fixture(&pool, "unknown-before-authorize");
    assert!(
        claim_authorized_local_run_ticket(&pool, &fixture.run_id, &fixture.ticket_hash)
            .expect("claim PostgreSQL local run before uncertain result")
            .is_some()
    );
    reserve_application_attempt(&pool, &fixture.account_id, &fixture.application.id, "local")
        .expect("reserve PostgreSQL local attempt");
    enter_running_state(&pool, &fixture);
    let session = running_session(&fixture);
    let capacity = local_submission_capacity(&fixture);
    let receipt = json!({
        "status": "side_effect_unknown",
        "issues": [{
            "field": "submission",
            "message": "The submit response was lost."
        }]
    });
    let initial_capacity_count: i64 = pool
        .get_pg()
        .expect("get PostgreSQL capacity assertion connection")
        .query_one(
            "SELECT COUNT(*) FROM jobs_submission_evidence_capacity
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3",
            &[
                &fixture.account_id,
                &fixture.application.id,
                &fixture.run_id,
            ],
        )
        .expect("query missing PostgreSQL local capacity")
        .get(0);
    assert_eq!(initial_capacity_count, 0);

    let finalized = finalize_local_side_effect_unknown(
        &pool,
        &fixture.account_id,
        &fixture.application.id,
        &fixture.run_id,
        &fixture.ticket_hash,
        &capacity,
        receipt.clone(),
        &session,
    )
    .expect("finalize PostgreSQL pre-authorize uncertain result");
    assert_eq!(finalized.state, "side_effect_unknown");
    let row = pool
        .get_pg()
        .expect("get PostgreSQL uncertain-result assertion connection")
        .query_one(
            "SELECT ticket.status, application.state, attempt.status, session.status,
                    capacity.runner, capacity.reserved_bytes, capacity.reserved_objects,
                    capacity.state, capacity.expires_at_ms, ticket.expires_at_ms
               FROM jobs_local_run_tickets ticket
               JOIN jobs_applications application
                 ON application.account_id = ticket.account_id
                AND application.id = ticket.application_id
               JOIN jobs_attempt_reservations attempt
                 ON attempt.account_id = ticket.account_id
                AND attempt.application_id = ticket.application_id
               JOIN jobs_browser_sessions session
                 ON session.account_id = ticket.account_id AND session.id = ticket.id
               JOIN jobs_submission_evidence_capacity capacity
                 ON capacity.account_id = ticket.account_id
                AND capacity.application_id = ticket.application_id
                AND capacity.run_id = ticket.id
              WHERE ticket.id = $1",
            &[&fixture.run_id],
        )
        .expect("query PostgreSQL uncertain-result lifecycle");
    assert_eq!(row.get::<_, String>(0), "side_effect_unknown");
    assert_eq!(row.get::<_, String>(1), "side_effect_unknown");
    assert_eq!(row.get::<_, String>(2), "side_effect_unknown");
    assert_eq!(row.get::<_, String>(3), "needs_input");
    assert_eq!(row.get::<_, String>(4), "local");
    assert_eq!(row.get::<_, i64>(5), capacity.reserved_bytes);
    assert_eq!(row.get::<_, i64>(6), capacity.reserved_objects);
    assert_eq!(row.get::<_, String>(7), "active");
    let capacity_expiry = row.get::<_, i64>(8);
    assert_eq!(
        capacity_expiry,
        row.get::<_, i64>(9)
            .saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
    );

    pool.get_pg()
        .expect("get PostgreSQL replay setup connection")
        .execute(
            "UPDATE jobs_submission_evidence_capacity SET expires_at_ms = $4
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3",
            &[
                &fixture.account_id,
                &fixture.application.id,
                &fixture.run_id,
                &now_ms().saturating_add(10_000),
            ],
        )
        .expect("shorten PostgreSQL local capacity before replay");
    let terminal_session = list_browser_sessions(&pool, &fixture.account_id)
        .expect("list PostgreSQL browser sessions")
        .into_iter()
        .find(|candidate| candidate.id == fixture.run_id)
        .expect("find PostgreSQL terminal browser session");
    let replayed = finalize_local_side_effect_unknown(
        &pool,
        &fixture.account_id,
        &fixture.application.id,
        &fixture.run_id,
        &fixture.ticket_hash,
        &capacity,
        receipt,
        &terminal_session,
    )
    .expect("replay exact PostgreSQL uncertain result");
    assert_eq!(
        serde_json::to_value(replayed).expect("serialize PostgreSQL replay"),
        serde_json::to_value(finalized).expect("serialize PostgreSQL first result")
    );
    let replay_expiry: i64 = pool
        .get_pg()
        .expect("get PostgreSQL replay assertion connection")
        .query_one(
            "SELECT expires_at_ms FROM jobs_submission_evidence_capacity
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3",
            &[
                &fixture.account_id,
                &fixture.application.id,
                &fixture.run_id,
            ],
        )
        .expect("query PostgreSQL replay capacity")
        .get(0);
    assert_eq!(replay_expiry, capacity_expiry);

    cleanup(&pool, &fixture);
}

#[test]
#[serial_test::serial]
fn postgres_corrupted_operational_hold_sha_blocks_release_and_listing() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let event_id = format!("pg-corrupt-sha-held-{suffix}");
    let scope_id = format!("acct-pg-corrupt-sha-{suffix}");
    insert_postgres_operational_hold_account(&pool, &scope_id, &suffix);
    let held = operational_hold_request(
        &event_id,
        OperationalCapability::RunnerClaim,
        OperationalHoldScopeKind::Account,
        &scope_id,
        OperationalHoldTransition::Held,
        0,
        None,
    );
    let opened = append_operational_hold_event(&pool, &held, "pg-corruption-reviewer")
        .expect("append PostgreSQL SHA-corruption hold");
    let _cleanup = PostgresOperationalHoldCaseCleanup {
        pool: pool.clone(),
        capability: held.capability,
        scope_kind: held.scope_kind,
        scope_id: scope_id.clone(),
        account_id: scope_id.clone(),
    };
    let canonical_event_base64url = pool
        .get_pg()
        .expect("get PostgreSQL SHA-corruption setup connection")
        .query_one(
            "SELECT canonical_event_base64url FROM jobs_operational_hold_events
              WHERE event_id = $1",
            &[&event_id],
        )
        .expect("query PostgreSQL SHA-corruption canonical event")
        .get::<_, String>(0);
    corrupt_postgres_operational_hold_canonical_fields(
        &pool,
        &event_id,
        &"f".repeat(64),
        &canonical_event_base64url,
    );

    assert_postgres_canonical_corruption_blocks_every_release_path(
        &pool,
        &held,
        &opened.state,
        &format!("pg-corrupt-sha-raw-release-{suffix}"),
        &format!("pg-corrupt-sha-ref-release-{suffix}"),
    );
}

#[test]
#[serial_test::serial]
fn postgres_canonical_bytes_conflicting_with_projection_block_release_and_listing() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let event_id = format!("pg-corrupt-projection-held-{suffix}");
    let scope_id = format!("acct-pg-corrupt-projection-{suffix}");
    insert_postgres_operational_hold_account(&pool, &scope_id, &suffix);
    let held = operational_hold_request(
        &event_id,
        OperationalCapability::RunnerClaim,
        OperationalHoldScopeKind::Account,
        &scope_id,
        OperationalHoldTransition::Held,
        0,
        None,
    );
    let opened = append_operational_hold_event(&pool, &held, "pg-corruption-reviewer")
        .expect("append PostgreSQL projection-corruption hold");
    let _cleanup = PostgresOperationalHoldCaseCleanup {
        pool: pool.clone(),
        capability: held.capability,
        scope_kind: held.scope_kind,
        scope_id: scope_id.clone(),
        account_id: scope_id.clone(),
    };
    let canonical_event_base64url = pool
        .get_pg()
        .expect("get PostgreSQL projection-corruption setup connection")
        .query_one(
            "SELECT canonical_event_base64url FROM jobs_operational_hold_events
              WHERE event_id = $1",
            &[&event_id],
        )
        .expect("query PostgreSQL projection-corruption canonical event")
        .get::<_, String>(0);
    let mut canonical = decode_operational_hold_event(&canonical_event_base64url)
        .expect("decode PostgreSQL projection-corruption canonical event");
    canonical.reason_code = OperationalHoldReasonCode::Maintenance;
    validate_canonical_operational_hold_event(&canonical)
        .expect("mutated PostgreSQL canonical event remains structurally valid");
    let (event_sha256, canonical_event_base64url) = operational_hold_event_identity(&canonical)
        .expect("re-encode valid PostgreSQL projection-corruption canonical event");
    assert_eq!(
        decode_operational_hold_event(&canonical_event_base64url)
            .expect("decode re-encoded PostgreSQL projection-corruption event"),
        canonical
    );
    corrupt_postgres_operational_hold_canonical_fields(
        &pool,
        &event_id,
        &event_sha256,
        &canonical_event_base64url,
    );

    assert_postgres_canonical_corruption_blocks_every_release_path(
        &pool,
        &held,
        &opened.state,
        &format!("pg-corrupt-projection-raw-release-{suffix}"),
        &format!("pg-corrupt-projection-ref-release-{suffix}"),
    );
}

#[test]
#[serial_test::serial]
fn postgres_operational_holds_preserve_history_refs_and_encrypted_context() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let mut fixture = local_authority_fixture(&pool, "operational-hold-context");
    let raw_region = format!("MÜNCHEN-{suffix}");
    let normalized_region = format!("münchen-{suffix}");
    fixture.posting.location = raw_region.clone();
    let encrypted_posting = to_json(&fixture.posting, "PostgreSQL operational-hold posting")
        .expect("encrypt PostgreSQL operational-hold posting context");
    let stored_posting = pool
        .get_pg()
        .expect("get PostgreSQL operational-hold posting connection")
        .query_one(
            "UPDATE jobs_postings
                SET posting_json = $3, location = $4
              WHERE account_id = $1 AND id = $2
              RETURNING posting_json",
            &[
                &fixture.account_id,
                &fixture.posting.id,
                &encrypted_posting,
                &raw_region,
            ],
        )
        .expect("save encrypted PostgreSQL Unicode-region posting")
        .get::<_, String>(0);
    assert!(stored_posting.starts_with(ENCRYPTED_PAYLOAD_PREFIX));
    assert!(!stored_posting.contains(&raw_region));

    let held_event_id = format!("pg-hold-held-{suffix}");
    let held_request = operational_hold_request(
        &held_event_id,
        OperationalCapability::Generation,
        OperationalHoldScopeKind::Region,
        &raw_region,
        OperationalHoldTransition::Held,
        0,
        None,
    );
    let held = append_operational_hold_event(&pool, &held_request, "pg-operator-1")
        .expect("append first PostgreSQL operational hold");
    assert_eq!(held.state.head_revision, 1);
    assert_eq!(held.state.state, OperationalHoldTransition::Held);
    assert!(!held.replayed);
    assert!(held.state.scope_ref.starts_with("scope-"));
    assert!(!held.state.scope_ref.contains(&normalized_region));

    let replayed = append_operational_hold_event(&pool, &held_request, "pg-operator-1")
        .expect("replay exact PostgreSQL operational hold");
    assert!(replayed.replayed);
    assert_eq!(replayed.state, held.state);
    assert!(matches!(
        append_operational_hold_event(&pool, &held_request, "pg-operator-2"),
        Err(OperationalHoldError::IdentityConflict)
    ));

    let stale_event_id = format!("pg-hold-stale-{suffix}");
    let wrong_predecessor = format!("pg-hold-wrong-predecessor-{suffix}");
    let stale = operational_hold_request(
        &stale_event_id,
        OperationalCapability::Generation,
        OperationalHoldScopeKind::Region,
        &normalized_region,
        OperationalHoldTransition::Released,
        1,
        Some(&wrong_predecessor),
    );
    assert!(matches!(
        append_operational_hold_event(&pool, &stale, "pg-operator-1"),
        Err(OperationalHoldError::Conflict)
    ));

    let escalated_event_id = format!("pg-hold-escalated-{suffix}");
    let escalated_request = operational_hold_request(
        &escalated_event_id,
        OperationalCapability::Generation,
        OperationalHoldScopeKind::Region,
        &normalized_region,
        OperationalHoldTransition::Held,
        1,
        Some(&held_event_id),
    );
    let escalated = append_operational_hold_event(&pool, &escalated_request, "pg-operator-1")
        .expect("append PostgreSQL held-to-held escalation");
    assert_eq!(escalated.state.head_revision, 2);
    assert_eq!(escalated.state.scope_ref, held.state.scope_ref);

    let held_evaluation = {
        let mut conn = pool
            .get_pg()
            .expect("get PostgreSQL encrypted-context connection");
        let mut tx = conn
            .transaction()
            .expect("begin PostgreSQL encrypted-context transaction");
        let context = operational_hold_context_for_application_postgres_tx(
            &mut tx,
            &fixture.account_id,
            &fixture.application.id,
            Some("local"),
            None,
            None,
        )
        .expect("derive PostgreSQL operational context from encrypted rows");
        assert!(context.matches(OperationalHoldScopeKind::Region, &normalized_region));
        let evaluation = evaluate_operational_capability_postgres_tx(
            &mut tx,
            OperationalCapability::Generation,
            &context,
        )
        .expect("evaluate held PostgreSQL Unicode-region context");
        tx.commit()
            .expect("commit PostgreSQL encrypted-context evaluation");
        evaluation
    };
    let OperationalCapabilityEvaluation::Held(block) = held_evaluation else {
        panic!("PostgreSQL Unicode-region context must observe its active hold");
    };
    assert_eq!(block.capability, OperationalCapability::Generation);
    assert_eq!(block.scope_kind, OperationalHoldScopeKind::Region);
    assert_eq!(block.scope_id, normalized_region);
    assert_eq!(block.head_revision, 2);

    let index = pool
        .get_pg()
        .expect("get PostgreSQL operational-hold index connection")
        .query_one(
            "SELECT idx.indisvalid, idx.indisready, pg_get_indexdef(idx.indexrelid)
               FROM pg_index idx
               JOIN pg_class relation ON relation.oid = idx.indexrelid
               JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
              WHERE namespace.nspname = current_schema()
                AND relation.relname = 'idx_jobs_operational_hold_heads_refs'",
            &[],
        )
        .expect("query PostgreSQL operational-hold reference index");
    assert!(index.get::<_, bool>(0));
    assert!(index.get::<_, bool>(1));
    let index_definition = index.get::<_, String>(2).to_ascii_lowercase();
    assert!(index_definition.contains("(capability, scope_kind, scope_ref, head_revision)"));

    let released_event_id = format!("pg-hold-released-{suffix}");
    let release = AppendOperationalHoldEventByRefRequest {
        event_id: released_event_id.clone(),
        capability: OperationalCapability::Generation,
        scope_kind: OperationalHoldScopeKind::Region,
        scope_ref: escalated.state.scope_ref.clone(),
        transition: OperationalHoldTransition::Released,
        reason_code: OperationalHoldReasonCode::ManualRelease,
        reason_ref: Some("PG-606".to_string()),
        expected_head_revision: escalated.state.head_revision,
        expected_current_event_ref: escalated.state.current_event_ref.clone(),
    };
    let released = append_operational_hold_event_by_ref(&pool, &release, "pg-operator-1")
        .expect("release PostgreSQL operational hold by indexed opaque refs");
    assert_eq!(released.state.head_revision, 3);
    assert_eq!(released.state.state, OperationalHoldTransition::Released);
    assert!(!released.replayed);
    assert_eq!(released.state.scope_ref, held.state.scope_ref);
    let release_replay = append_operational_hold_event_by_ref(&pool, &release, "pg-operator-1")
        .expect("replay exact PostgreSQL by-ref release");
    assert!(release_replay.replayed);
    assert_eq!(release_replay.state, released.state);

    let released_evaluation = {
        let mut conn = pool
            .get_pg()
            .expect("get PostgreSQL released-context connection");
        let mut tx = conn
            .transaction()
            .expect("begin PostgreSQL released-context transaction");
        let context = operational_hold_context_for_application_postgres_tx(
            &mut tx,
            &fixture.account_id,
            &fixture.application.id,
            Some("local"),
            None,
            None,
        )
        .expect("rebuild PostgreSQL released operational context");
        let evaluation = evaluate_operational_capability_postgres_tx(
            &mut tx,
            OperationalCapability::Generation,
            &context,
        )
        .expect("evaluate released PostgreSQL Unicode-region context");
        tx.commit()
            .expect("commit PostgreSQL released-context evaluation");
        evaluation
    };
    assert_eq!(
        released_evaluation,
        OperationalCapabilityEvaluation::Allowed
    );

    let redundant_event_id = format!("pg-hold-redundant-release-{suffix}");
    let redundant_release = operational_hold_request(
        &redundant_event_id,
        OperationalCapability::Generation,
        OperationalHoldScopeKind::Region,
        &normalized_region,
        OperationalHoldTransition::Released,
        released.state.head_revision,
        Some(&released_event_id),
    );
    assert!(matches!(
        append_operational_hold_event(&pool, &redundant_release, "pg-operator-1"),
        Err(OperationalHoldError::Conflict)
    ));

    let mut conn = pool
        .get_pg()
        .expect("get PostgreSQL operational-hold assertion connection");
    let event_update_error = conn
        .execute(
            "UPDATE jobs_operational_hold_events
                SET reason_code = 'maintenance'
              WHERE event_id = $1",
            &[&held_event_id],
        )
        .expect_err("PostgreSQL operational-hold history must reject UPDATE");
    assert!(event_update_error.as_db_error().is_some());
    let event_delete_error = conn
        .execute(
            "DELETE FROM jobs_operational_hold_events WHERE event_id = $1",
            &[&held_event_id],
        )
        .expect_err("PostgreSQL operational-hold history must reject DELETE");
    assert!(event_delete_error.as_db_error().is_some());
    let head_update_error = conn
        .execute(
            "UPDATE jobs_operational_hold_heads SET state = 'held'
              WHERE capability = 'generation' AND scope_kind = 'region' AND scope_id = $1",
            &[&normalized_region],
        )
        .expect_err("PostgreSQL operational-hold head must reject non-monotonic UPDATE");
    assert!(head_update_error.as_db_error().is_some());
    let head_delete_error = conn
        .execute(
            "DELETE FROM jobs_operational_hold_heads
              WHERE capability = 'generation' AND scope_kind = 'region' AND scope_id = $1",
            &[&normalized_region],
        )
        .expect_err("PostgreSQL operational-hold head must reject DELETE");
    assert!(head_delete_error.as_db_error().is_some());

    let history = conn
        .query(
            "SELECT event_id, revision_no, previous_revision_no, predecessor_event_id,
                    transition, event_ref
               FROM jobs_operational_hold_events
              WHERE capability = 'generation' AND scope_kind = 'region' AND scope_id = $1
              ORDER BY revision_no",
            &[&normalized_region],
        )
        .expect("query PostgreSQL operational-hold ancestry")
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, i64>(1),
                row.get::<_, Option<i64>>(2),
                row.get::<_, Option<String>>(3),
                row.get::<_, String>(4),
                row.get::<_, String>(5),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0],
        (
            held_event_id.clone(),
            1,
            None,
            None,
            "held".to_string(),
            held.state.current_event_ref.clone(),
        )
    );
    assert_eq!(
        history[1],
        (
            escalated_event_id.clone(),
            2,
            Some(1),
            Some(held_event_id.clone()),
            "held".to_string(),
            escalated.state.current_event_ref.clone(),
        )
    );
    assert_eq!(
        history[2],
        (
            released_event_id.clone(),
            3,
            Some(2),
            Some(escalated_event_id),
            "released".to_string(),
            released.state.current_event_ref.clone(),
        )
    );
    let failed_event_count = conn
        .query_one(
            "SELECT COUNT(*) FROM jobs_operational_hold_events
              WHERE event_id IN ($1, $2)",
            &[&stale_event_id, &redundant_event_id],
        )
        .expect("count rejected PostgreSQL operational-hold events")
        .get::<_, i64>(0);
    assert_eq!(failed_event_count, 0, "rejected CAS events must not orphan");
    let head = conn
        .query_one(
            "SELECT head_revision, current_event_id, current_event_ref, state, scope_ref
               FROM jobs_operational_hold_heads
              WHERE capability = 'generation' AND scope_kind = 'region' AND scope_id = $1",
            &[&normalized_region],
        )
        .expect("query final PostgreSQL operational-hold head");
    assert_eq!(head.get::<_, i64>(0), 3);
    assert_eq!(head.get::<_, String>(1), released_event_id);
    assert_eq!(head.get::<_, String>(2), released.state.current_event_ref);
    assert_eq!(head.get::<_, String>(3), "released");
    assert_eq!(head.get::<_, String>(4), released.state.scope_ref);
    drop(conn);

    cleanup(&pool, &fixture);
}

#[test]
#[serial_test::serial]
fn postgres_operational_hold_advisory_lock_serializes_two_connections() {
    let Some(pool) = postgres_pool() else {
        return;
    };

    let mut admission_conn = pool
        .get_pg()
        .expect("get PostgreSQL operational admission-lock connection");
    let mut admission_tx = admission_conn
        .transaction()
        .expect("begin PostgreSQL operational admission transaction");
    let admission_pid = admission_tx
        .query_one("SELECT pg_backend_pid()", &[])
        .expect("query PostgreSQL admission backend pid")
        .get::<_, i32>(0);
    lock_operational_hold_shared_postgres_tx(&mut admission_tx)
        .expect("acquire PostgreSQL shared operational admission lock");

    let mut mutation_conn = pool
        .get_pg()
        .expect("get PostgreSQL operational mutation-lock connection");
    let mut mutation_tx = mutation_conn
        .transaction()
        .expect("begin PostgreSQL operational mutation transaction");
    let mutation_pid = mutation_tx
        .query_one("SELECT pg_backend_pid()", &[])
        .expect("query PostgreSQL mutation backend pid")
        .get::<_, i32>(0);
    assert_ne!(admission_pid, mutation_pid);
    let exclusive_acquired = mutation_tx
        .query_one(
            "SELECT pg_try_advisory_xact_lock(
                hashtextextended('bluey-jobs-operational-holds-v1', 0)
             )",
            &[],
        )
        .expect("try PostgreSQL exclusive operational lock behind admission")
        .get::<_, bool>(0);
    assert!(!exclusive_acquired);
    admission_tx
        .commit()
        .expect("release PostgreSQL shared operational admission lock");
    mutation_tx
        .query_one(POSTGRES_OPERATIONAL_HOLD_EXCLUSIVE_LOCK_SQL, &[])
        .expect("acquire PostgreSQL exclusive operational lock after admission commits");

    let mut admission_retry_tx = admission_conn
        .transaction()
        .expect("begin PostgreSQL admission retry transaction");
    let shared_acquired = admission_retry_tx
        .query_one(
            "SELECT pg_try_advisory_xact_lock_shared(
                hashtextextended('bluey-jobs-operational-holds-v1', 0)
             )",
            &[],
        )
        .expect("try PostgreSQL shared admission lock behind mutation")
        .get::<_, bool>(0);
    assert!(!shared_acquired);
    mutation_tx
        .commit()
        .expect("release PostgreSQL exclusive operational mutation lock");
    lock_operational_hold_shared_postgres_tx(&mut admission_retry_tx)
        .expect("acquire PostgreSQL shared operational lock after mutation commits");
    admission_retry_tx
        .commit()
        .expect("commit PostgreSQL admission retry transaction");
}

#[test]
#[serial_test::serial]
fn postgres_auto_submit_revocation_and_execution_share_one_fence() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let account_id = format!("acct_auto_submit_fence_{suffix}");
    let track_id = format!("track-auto-submit-fence-{suffix}");
    pool.get_pg()
        .expect("get PostgreSQL Auto-submit fence fixture connection")
        .execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ($1, $2, 'hash', 0)",
            &[
                &account_id,
                &format!("auto-submit-fence-{suffix}@example.test"),
            ],
        )
        .expect("insert PostgreSQL Auto-submit fence account");

    let mut execution_conn = pool
        .get_pg()
        .expect("get PostgreSQL Auto-submit execution connection");
    let mut execution_tx = execution_conn
        .transaction()
        .expect("begin PostgreSQL Auto-submit execution transaction");
    lock_discovery_account_shared_postgres(&mut execution_tx, &account_id)
        .expect("lock execution discovery account");
    lock_account_policy_inputs_postgres(&mut execution_tx, &account_id, false)
        .expect("lock execution account-policy inputs");
    lock_auto_submit_authority_postgres(&mut execution_tx, &account_id, &track_id, false)
        .expect("lock execution Auto-submit authority");

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let revoke_pool = pool.clone();
    let revoke_account_id = account_id.clone();
    let revoke_track_id = track_id.clone();
    let revoke_worker = std::thread::spawn(move || {
        started_tx.send(()).expect("signal revocation start");
        finished_tx
            .send(revoke_auto_submit(
                &revoke_pool,
                &revoke_account_id,
                &revoke_track_id,
            ))
            .expect("send revocation result");
    });
    started_rx.recv().expect("wait for revocation start");
    assert!(
        finished_rx
            .recv_timeout(std::time::Duration::from_millis(250))
            .is_err(),
        "revocation must wait while execution holds shared authority"
    );
    execution_tx
        .commit()
        .expect("release PostgreSQL Auto-submit execution fence");
    assert!(!finished_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("revocation completes after execution")
        .expect("revoke Auto-submit after execution"));
    revoke_worker
        .join()
        .expect("join Auto-submit revocation worker");

    let mut revoke_conn = pool
        .get_pg()
        .expect("get PostgreSQL Auto-submit revocation connection");
    let mut revoke_tx = revoke_conn
        .transaction()
        .expect("begin PostgreSQL Auto-submit revocation transaction");
    lock_discovery_account_shared_postgres(&mut revoke_tx, &account_id)
        .expect("lock revocation discovery account");
    lock_account_policy_inputs_postgres(&mut revoke_tx, &account_id, false)
        .expect("lock revocation account-policy inputs");
    lock_auto_submit_authority_postgres(&mut revoke_tx, &account_id, &track_id, true)
        .expect("lock exclusive Auto-submit revocation authority");

    let mut validation_conn = pool
        .get_pg()
        .expect("get PostgreSQL Auto-submit validation connection");
    let mut validation_tx = validation_conn
        .transaction()
        .expect("begin PostgreSQL Auto-submit validation transaction");
    lock_discovery_account_shared_postgres(&mut validation_tx, &account_id)
        .expect("lock validation discovery account");
    lock_account_policy_inputs_postgres(&mut validation_tx, &account_id, false)
        .expect("lock validation account-policy inputs");
    let shared_acquired = validation_tx
        .query_one(
            "SELECT pg_try_advisory_xact_lock_shared(hashtextextended($1, 0))",
            &[&format!("{account_id}:{track_id}:auto-submit")],
        )
        .expect("try execution validation behind revocation")
        .get::<_, bool>(0);
    assert!(
        !shared_acquired,
        "execution validation must wait while revocation holds exclusive authority"
    );
    revoke_tx
        .commit()
        .expect("release PostgreSQL Auto-submit revocation fence");
    lock_auto_submit_authority_postgres(&mut validation_tx, &account_id, &track_id, false)
        .expect("lock execution validation after revocation commits");
    validation_tx
        .commit()
        .expect("commit PostgreSQL Auto-submit validation transaction");

    pool.get_pg()
        .expect("get PostgreSQL Auto-submit fence cleanup connection")
        .execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
        .expect("delete PostgreSQL Auto-submit fence account");
}

#[test]
#[serial_test::serial]
fn postgres_execution_authority_locks_integrity_before_account_policy() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let fixture = local_authority_fixture(&pool, "integrity-before-policy");

    let mut publisher_conn = pool
        .get_pg()
        .expect("get PostgreSQL integrity publisher connection");
    let mut publisher_tx = publisher_conn
        .transaction()
        .expect("begin PostgreSQL integrity publisher transaction");
    publisher_tx
        .query_one(
            "SELECT singleton_id FROM jobs_job_integrity_control
              WHERE singleton_id = 1 FOR UPDATE",
            &[],
        )
        .expect("hold exclusive PostgreSQL integrity publication fence");

    let execution_pool = pool.clone();
    let execution_account_id = fixture.account_id.clone();
    let execution_application = fixture.application.clone();
    let (waiting_tx, waiting_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let execution_worker = std::thread::spawn(move || {
        let result = (|| -> Result<bool> {
            let mut conn = execution_pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.batch_execute(
                "SET LOCAL lock_timeout = '5s';
                 SET LOCAL statement_timeout = '10s';",
            )?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, &execution_account_id)?;
            let backend_pid = tx
                .query_one("SELECT pg_backend_pid()", &[])?
                .get::<_, i32>(0);
            waiting_tx
                .send(backend_pid)
                .expect("signal execution prelocks");
            let authorized = resolve_current_execution_authority_postgres_after_prelock(
                &mut tx,
                &execution_account_id,
                &execution_application,
                ExecutionAuthorityRunner::Local,
            )?
            .authorized;
            tx.rollback()?;
            Ok(authorized)
        })();
        finished_tx
            .send(result)
            .expect("send execution-authority result");
    });

    let execution_pid = waiting_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("execution acquired H -> M -> ATS -> D prelocks");
    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let waiting_on_lock = publisher_tx
            .query_one(
                "SELECT COALESCE((
                    SELECT wait_event_type = 'Lock'
                      FROM pg_stat_activity WHERE pid = $1
                 ), FALSE)",
                &[&execution_pid],
            )
            .expect("observe execution publication-fence wait")
            .get::<_, bool>(0);
        if waiting_on_lock {
            break;
        }
        assert!(
            std::time::Instant::now() < wait_deadline,
            "execution never waited on the integrity publication fence"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        finished_rx
            .recv_timeout(std::time::Duration::from_millis(250))
            .is_err(),
        "execution must remain behind the publication fence"
    );

    publisher_tx
        .batch_execute("SET LOCAL lock_timeout = '500ms'")
        .expect("bound PostgreSQL policy-lock assertion");
    publisher_tx
        .query_one(
            "SELECT account_id FROM jobs_profiles
              WHERE account_id = $1 FOR UPDATE",
            &[&fixture.account_id],
        )
        .expect("publisher can lock policy while execution waits before policy");
    publisher_tx
        .commit()
        .expect("release PostgreSQL integrity and policy locks together");

    assert!(finished_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("execution completes after canonical-order locks release")
        .expect("execution authority resolves without a lock cycle"));
    execution_worker
        .join()
        .expect("join PostgreSQL execution-authority worker");
    cleanup(&pool, &fixture);
}

#[test]
#[serial_test::serial]
fn postgres_operational_context_holds_account_fence_through_admission_commit() {
    let Some(pool) = postgres_pool() else {
        return;
    };
    let fixture = local_authority_fixture(&pool, "operational-context-account-fence");

    let mut admission_conn = pool
        .get_pg()
        .expect("get PostgreSQL context admission connection");
    let mut admission_tx = admission_conn
        .transaction()
        .expect("begin PostgreSQL context admission transaction");
    lock_operational_hold_shared_postgres_tx(&mut admission_tx)
        .expect("acquire PostgreSQL operational-hold admission lock");
    let context = operational_hold_context_for_application_postgres_tx(
        &mut admission_tx,
        &fixture.account_id,
        &fixture.application.id,
        Some("local"),
        None,
        None,
    )
    .expect("derive fenced PostgreSQL operational context");
    assert!(context.matches(OperationalHoldScopeKind::Account, &fixture.account_id));

    let mut mutation_conn = pool
        .get_pg()
        .expect("get PostgreSQL discovery-account mutation connection");
    let mut mutation_tx = mutation_conn
        .transaction()
        .expect("begin PostgreSQL discovery-account mutation transaction");
    let exclusive_acquired = mutation_tx
        .query_one(
            "SELECT pg_try_advisory_xact_lock(
                hashtextextended('jobs-discovery-account:' || $1, 0)
             )",
            &[&fixture.account_id],
        )
        .expect("try PostgreSQL discovery-account mutation lock behind admission")
        .get::<_, bool>(0);
    assert!(!exclusive_acquired);

    admission_tx
        .commit()
        .expect("release PostgreSQL context account fence");
    lock_discovery_account_postgres(&mut mutation_tx, &fixture.account_id)
        .expect("acquire PostgreSQL discovery-account lock after admission commits");
    mutation_tx
        .commit()
        .expect("commit PostgreSQL discovery-account mutation transaction");
    cleanup(&pool, &fixture);
}

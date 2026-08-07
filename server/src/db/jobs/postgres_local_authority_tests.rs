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
    drop(conn);
    Some(pool)
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

    let profile = default_profile(&email);
    save_profile(pool, &account_id, &profile).expect("save PostgreSQL Jobs profile");
    set_entitlement_plan(pool, &account_id, "pro")
        .expect("enable PostgreSQL local-browser entitlement");
    let canonical_url = format!("https://boards.greenhouse.io/acme/jobs/{label}-{suffix}");
    let posting = upsert_posting(
        pool,
        &account_id,
        &greenhouse_posting(&canonical_url),
        &profile,
        &JobPreferences::default(),
    )
    .expect("save PostgreSQL Greenhouse posting");
    let (application, _) =
        prepare_application(pool, &account_id, &posting.id, "factual", "review_first")
            .expect("prepare PostgreSQL application");
    let application = update_application(
        pool,
        &account_id,
        &application.id,
        "queued",
        Some("review_first"),
    )
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
    let mut application = assign_application_run(pool, &account_id, &application.id, &run_id)
        .expect("bind PostgreSQL application run")
        .expect("bound PostgreSQL application run");
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .expect("frozen application identity")
        .to_string();
    let browser_profile_id = execution_browser_profile_id(&account_id, &identity_id);
    let identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .expect("frozen application email")
        .to_string();
    let resume = get_resume_version(
        pool,
        &account_id,
        application
            .resume_version_id
            .as_deref()
            .expect("fixture application has a resume"),
    )
    .expect("load PostgreSQL resume")
    .expect("PostgreSQL resume exists");
    let approved_packet = json!({
        "applicationId": application.id,
        "jobId": application.job_id,
        "resumeVersionId": resume.id,
        "resumeContent": resume.content,
        "coverLetterContent": application.cover_letter,
        "answers": {},
        "verifiedClaimIds": [],
        "applicationIdentityId": identity_id,
        "applicationEmail": identity_email,
        "browserProfileId": browser_profile_id,
    });
    let approved_job = json!({
        "externalId": posting.external_id,
        "canonicalUrl": posting.canonical_url,
        "company": posting.company,
        "title": posting.title,
        "location": posting.location,
        "workplace": posting.workplace,
        "description": posting.description,
        "source": posting.source,
        "compensation": posting.compensation,
    });
    let admission = json!({ "kind": "review_approval" });
    let checksum =
        approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
            .expect("checksum PostgreSQL approved execution");
    application.receipt["approved_execution"] = json!({
        "schema_version": 2,
        "approved_at_ms": now_ms(),
        "checksum": checksum,
        "admission": admission,
        "packet": approved_packet,
        "job": approved_job,
    });
    application = replace_application_receipt(
        pool,
        &account_id,
        &application.id,
        application.receipt.clone(),
    )
    .expect("save PostgreSQL approved execution")
    .expect("PostgreSQL approved application exists");
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

    LocalAuthorityFixture {
        account_id,
        application,
        posting,
        run_id,
        ticket_hash,
        identity_id,
    }
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
    upsert_browser_session(pool, &fixture.account_id, &running_session(fixture))
        .expect("start PostgreSQL browser session");
}

fn cleanup(pool: &DbPool, fixture: &LocalAuthorityFixture) {
    pool.get_pg()
        .expect("get PostgreSQL cleanup connection")
        .execute("DELETE FROM accounts WHERE id = $1", &[&fixture.account_id])
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

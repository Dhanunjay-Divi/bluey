use super::production_positive_authority_fixture::{
    install_production_positive_job_authorities, save_production_positive_verified_import,
};
use super::*;

#[test]
fn intervention_answer_uses_visible_question_and_requires_packet_reapproval() {
    let intervention: Intervention = serde_json::from_value(json!({
        "id": "intervention-1", "application_id": "application-1",
        "kind": "unknown_question", "status": "open", "title": "One detail is missing",
        "resolution_kind": "answer", "metadata": {"receipt": {"intervention": {
            "field": "Are you authorized to work in Canada?"
        }}}
    }))
    .unwrap();
    let application: JobApplication = serde_json::from_value(json!({
        "id": "application-1", "job_id": "job-1", "state": "needs_input",
        "run_id": "run-1", "receipt": {"approved_execution": {"checksum": "old-checksum"}}
    }))
    .unwrap();
    let (revision, old_run) =
        prepare_intervention_answer_revision(intervention.clone(), application.clone(), "Yes", 123)
            .unwrap();
    assert_eq!(revision.question, "Are you authorized to work in Canada?");
    assert_eq!(
        revision.application.answers[0]["key"],
        normalize_answer_memory_key(&revision.question)
    );
    assert_eq!(
        revision.application.answers[0]["question"],
        revision.question
    );
    assert_eq!(revision.application.state, "awaiting_review");
    assert!(revision
        .application
        .receipt
        .get("approved_execution")
        .is_none());
    assert_eq!(
        revision.application.receipt["packet_revision"]["reapproval_required"],
        true
    );
    assert_eq!(
        revision.application.receipt["packet_revision"]["invalidated_packet_checksum"],
        "old-checksum"
    );
    assert_eq!(old_run.as_deref(), Some("run-1"));
    assert!(revision.application.run_id.is_none());

    let mut explicit = intervention.clone();
    explicit.metadata["receipt"]["intervention"]["question"] =
        json!("Are you authorized to work in the US?");
    let (revision, _) =
        prepare_intervention_answer_revision(explicit, application.clone(), "Yes", 124).unwrap();
    assert_eq!(revision.question, "Are you authorized to work in the US?");
    assert_eq!(
        revision.application.answers[0]["key"],
        normalize_answer_memory_key(&revision.question)
    );
    assert!(prepare_intervention_answer_revision(
        intervention.clone(),
        application.clone(),
        "  ",
        125
    )
    .is_err());
    let mut empty = intervention;
    empty.title.clear();
    empty.metadata["receipt"]["intervention"]["field"] = json!("  ");
    assert!(prepare_intervention_answer_revision(empty, application, "Yes", 126).is_err());
}

#[test]
fn ambiguous_legacy_selector_requires_review_without_mutation() {
    let intervention: Intervention = serde_json::from_value(json!({
        "id": "intervention-1", "application_id": "application-1",
        "kind": "unknown_question", "status": "open", "title": "One detail is missing",
        "resolution_kind": "answer", "metadata": {"receipt": {"intervention": {
            "field": "email", "question": "What is your manager's email?"
        }}}
    }))
    .unwrap();
    let application: JobApplication = serde_json::from_value(json!({
        "id": "application-1", "job_id": "job-1", "state": "needs_input",
        "run_id": "run-1", "answers": [{"key":"email", "question":"Your email", "value":"old@example.test"}],
        "receipt": {"approved_execution": {"checksum": "old-checksum"}}
    })).unwrap();
    let before = serde_json::to_value((&intervention, &application)).unwrap();
    let error = prepare_intervention_answer_revision(
        intervention.clone(),
        application.clone(),
        "new@example.test",
        123,
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("intervention_selector_answer_review_required"));
    assert_eq!(
        serde_json::to_value((&intervention, &application)).unwrap(),
        before
    );

    let mut same_question = application;
    same_question.answers[0]["question"] = json!("What is your manager's email?");
    let (revision, _) =
        prepare_intervention_answer_revision(intervention, same_question, "new@example.test", 124)
            .unwrap();
    assert_eq!(revision.application.answers.len(), 1);
    assert_eq!(
        revision.application.answers[0]["key"],
        "what is your manager s email"
    );
    assert_eq!(revision.application.answers[0]["value"], "new@example.test");
    assert!(revision
        .application
        .receipt
        .get("approved_execution")
        .is_none());
}

fn memory(scope: &str, scope_id: Option<&str>) -> AnswerMemory {
    AnswerMemory {
        id: String::new(),
        key: String::new(),
        question: "Why apply?".into(),
        value: "I value this work".into(),
        scope: scope.into(),
        scope_id: scope_id.map(str::to_owned),
        confirmed: true,
        source: "settings".into(),
        created_at_ms: 0,
        updated_at_ms: 0,
        last_used_at_ms: None,
        use_count: 0,
    }
}

#[test]
fn generic_memory_refuses_reserved_scope_and_preserves_legacy_edit_delete() {
    let pool = tests::test_pool();
    let account = "acct-jobs";
    let reserved = format!("company-v1-{}", "a".repeat(64));
    assert!(save_answer_memory(&pool, account, &memory("company", Some(&reserved))).is_err());
    let mut mismatched = memory("account", None);
    mismatched.key = "different question".into();
    assert!(save_answer_memory(&pool, account, &mismatched).is_err());
    assert!(list_answer_memory(&pool, account).unwrap().is_empty());
    let mut legacy = save_answer_memory(&pool, account, &memory("company", Some("acme"))).unwrap();
    legacy.value = "Updated answer".into();
    let edited = save_answer_memory(&pool, account, &legacy).unwrap();
    assert_eq!(edited.id, legacy.id);
    assert_eq!(edited.value, "Updated answer");
    assert!(delete_answer_memory(&pool, account, &legacy.id).unwrap());
    assert!(list_answer_memory(&pool, account).unwrap().is_empty());
}

#[test]
fn protected_existing_row_cannot_be_overwritten_by_generic_or_another_account() {
    let pool = tests::test_pool();
    let account = "acct-jobs";
    let scope = format!("company-v1-{}", "b".repeat(64));
    let mut forged = memory("company", Some(&scope));
    forged.id = "historical-reserved-row".into();
    forged.key = normalize_answer_memory_key(&forged.question);
    let hash = private_lookup_hash(
        &format!("jobs-answer-memory:{account}:company:{scope}"),
        &forged.key,
    )
    .unwrap();
    let payload = to_json(&forged, "answer memory").unwrap();
    // Historical row is seeded only as an adversarial negative, not as verified authority.
    pool.get().unwrap().execute(
        "INSERT INTO jobs_answer_memory(id,account_id,scope,scope_id,question_hash,answer_json,created_at_ms,updated_at_ms) VALUES (?1,?2,'company',?3,?4,?5,0,0)",
        params![forged.id, account, scope, hash, payload],
    ).unwrap();
    let before = serde_json::to_value(list_answer_memory(&pool, account).unwrap()).unwrap();
    let mut replacement = forged.clone();
    replacement.scope = "account".into();
    replacement.scope_id = None;
    replacement.value = "replacement".into();
    assert!(save_answer_memory(&pool, account, &replacement).is_err());
    pool.get().unwrap().execute(
        "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining,email_verified_at) VALUES ('other-account','other@example.com','hash',0,'2026-08-30T00:00:00Z')", [],
    ).unwrap();
    assert!(save_answer_memory(&pool, "other-account", &replacement).is_err());
    assert_eq!(
        serde_json::to_value(list_answer_memory(&pool, account).unwrap()).unwrap(),
        before
    );
    assert!(list_answer_memory(&pool, "other-account")
        .unwrap()
        .is_empty());
}

#[test]
fn lookup_requires_owned_current_verified_job_and_failed_lookup_does_not_write() {
    let pool = tests::test_pool();
    let before = serde_json::to_value(list_answer_memory(&pool, "acct-jobs").unwrap()).unwrap();
    assert!(lookup_answer_memory_company_scope(&pool, "acct-jobs", "missing-job").is_err());
    assert!(save_verified_company_answer_memory(
        &pool,
        "acct-jobs",
        "missing-job",
        "company-v1-invalid",
        &memory("company", None)
    )
    .is_err());
    assert_eq!(
        serde_json::to_value(list_answer_memory(&pool, "acct-jobs").unwrap()).unwrap(),
        before
    );
}

#[test]
fn verified_company_writer_uses_current_real_authority_and_preserves_legacy_record() {
    let pool = tests::test_pool();
    pool.get()
        .unwrap()
        .execute(
            "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
         VALUES (?1,?2,?3,0)",
            rusqlite::params!["other-account", "other@example.com", "hash"],
        )
        .unwrap();
    exercise_verified_company_writer(&pool);
}

// Shared assertions run against each independently initialized database backend.
fn exercise_verified_company_writer(pool: &DbPool) {
    let account = "acct-jobs";
    let profile = save_profile(pool, account, &default_profile("jobs@example.com")).unwrap();
    let preferences = save_preferences(pool, account, &JobPreferences::default()).unwrap();
    let now = now_ms();
    let posting = JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse_import".into(),
        external_id: "650953001".into(),
        company: "Acme".into(),
        title: "Software Engineer".into(),
        location: "New York, NY".into(),
        workplace: "hybrid".into(),
        canonical_url: "https://boards.greenhouse.io/acme/jobs/650953001".into(),
        description: "Build reliable products with Rust and TypeScript.".into(),
        compensation: "$170k-$200k".into(),
        employment_type: "full_time".into(),
        track_id: "track-default".into(),
        match_score: 90,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: Some(now),
        last_verified_at_ms: Some(now),
        availability_status: "active".into(),
        status: "matched".into(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    };
    let (posting, managed) =
        save_production_positive_verified_import(pool, account, &posting, &profile, &preferences);
    let positive = install_production_positive_job_authorities(
        pool,
        account,
        &posting,
        &managed,
        "acme.com",
        "memory-writer",
    );
    let scope = lookup_answer_memory_company_scope(pool, account, &positive.posting.id).unwrap();
    assert_eq!(scope.job_id, positive.posting.id);
    assert_eq!(scope.domain, positive.integrity.canonical_employer_domain);
    let legacy = save_answer_memory(pool, account, &memory("company", Some("acme"))).unwrap();
    let mut upgrade = legacy.clone();
    upgrade.scope_id = Some(scope.scope_id.clone());
    let bound = save_verified_company_answer_memory(
        pool,
        account,
        &scope.job_id,
        &scope.scope_id,
        &upgrade,
    )
    .unwrap();
    assert_ne!(bound.id, legacy.id);
    assert_eq!(list_answer_memory(pool, account).unwrap().len(), 2);
    let replay =
        save_verified_company_answer_memory(pool, account, &scope.job_id, &scope.scope_id, &bound)
            .unwrap();
    assert_eq!(replay.id, bound.id);
    assert_eq!(replay.scope_id, bound.scope_id);
    assert_eq!(replay.question, bound.question);
    assert_eq!(replay.value, bound.value);
    let before = serde_json::to_value(list_answer_memory(pool, account).unwrap()).unwrap();
    let mut attempted_rebind = bound.clone();
    attempted_rebind.scope = "account".into();
    attempted_rebind.scope_id = None;
    assert!(save_answer_memory(pool, account, &attempted_rebind).is_err());
    assert!(save_answer_memory(pool, "other-account", &attempted_rebind).is_err());
    assert!(list_answer_memory(pool, "other-account")
        .unwrap()
        .is_empty());
    let mut bad_key = bound.clone();
    bad_key.key = "different question".into();
    assert!(save_verified_company_answer_memory(
        pool,
        account,
        &scope.job_id,
        &scope.scope_id,
        &bad_key,
    )
    .is_err());
    assert!(save_verified_company_answer_memory(
        pool,
        account,
        &scope.job_id,
        "company-v1-wrong",
        &bound
    )
    .is_err());
    assert!(lookup_answer_memory_company_scope(pool, "other-account", &scope.job_id).is_err());
    assert_eq!(
        serde_json::to_value(list_answer_memory(pool, account).unwrap()).unwrap(),
        before
    );
}

#[test]
fn postgres_verified_company_memory_lookup_save_replay_when_owned() {
    const CASE: &str = "db::jobs::answer_memory_writer_tests::postgres_verified_company_memory_lookup_save_replay_when_owned";
    let Some(url) = crate::db::owned_postgres_test::owned_url(CASE) else {
        return;
    };
    let pool = crate::db::open_postgres_pool(&url).expect("open owned PostgreSQL pool");
    crate::db::run_migrations(&pool).expect("migrate owned PostgreSQL database");
    let mut conn = pool.get_pg().expect("owned PostgreSQL connection");
    conn.execute(
        "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining,email_verified_at)
         VALUES ($1,$2,$3,0,TIMESTAMPTZ '2026-08-30 00:00:00+00')",
        &[&"acct-jobs", &"jobs@example.com", &"hash"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
         VALUES ($1,$2,$3,0)",
        &[&"other-account", &"other@example.com", &"hash"],
    )
    .unwrap();
    conn.execute(
        "UPDATE jobs_public_beta_cohorts SET state='closed_to_new', hard_cap=1,
         assigned_count=1, revision=1 WHERE id='public-v1'",
        &[],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO jobs_public_beta_enrollments (cohort_id,account_id,source,admitted_at_ms)
         VALUES ('public-v1','acct-jobs','admin',1)",
        &[],
    )
    .unwrap();
    drop(conn);
    let identity =
        ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com").unwrap();
    upsert_track(
        &pool,
        "acct-jobs",
        &CareerTrack {
            id: "track-default".into(),
            name: "Software engineering".into(),
            role: "Software Engineer".into(),
            locations: vec!["New York, NY".into()],
            remote_preference: "hybrid_ok".into(),
            application_identity_id: Some(identity.id),
            policy: CareerTrackPolicy {
                role_family: "software_engineering".into(),
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    exercise_verified_company_writer(&pool);
    // Snapshot admission and sequential replay only: no revocation race proof.
    println!("phase656_owned_company_memory_postgres_complete");
}

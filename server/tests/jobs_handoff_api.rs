#![cfg(test)]

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use bluey_server::{
    api,
    auth::{self, jwt::TokenKind},
    config::{Config, ServerDbBackend, TrialAbuseConfig, UpstreamKeys},
    db::{accounts::Account, open_pool, run_migrations, DbPool},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const JWT_SECRET: &str = "jobs-handoff-test-secret-at-least-32-chars";

#[tokio::test]
async fn jobs_desktop_handoff_is_authenticated_bound_redacted_and_single_use() {
    let (router, pool, owner, other) = harness();
    seed_submitted_application(&pool, &owner.id);
    let owner_token = auth::jwt::issue(JWT_SECRET, &owner.id, TokenKind::Access).unwrap();
    let other_token = auth::jwt::issue(JWT_SECRET, &other.id, TokenKind::Access).unwrap();
    let issue_path = "/api/jobs/applications/application-1/bluey-handoff";
    let redeem_path = "/api/jobs/bluey-handoffs/redeem";

    let unauthenticated_issue = router
        .clone()
        .oneshot(Request::post(issue_path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthenticated_issue.status(), StatusCode::UNAUTHORIZED);
    let unauthenticated_redeem = router
        .clone()
        .oneshot(
            Request::post(redeem_path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"nonce":"invalid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated_redeem.status(), StatusCode::UNAUTHORIZED);

    let issued_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let issue = router
        .clone()
        .oneshot(
            Request::post(issue_path)
                .header(header::AUTHORIZATION, format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(issue.status(), StatusCode::OK);
    assert_eq!(
        issue.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let issue_body: Value =
        serde_json::from_slice(&to_bytes(issue.into_body(), 256 * 1024).await.unwrap()).unwrap();
    let nonce = issue_body["nonce"].as_str().unwrap();
    assert_eq!(nonce.len(), 43);
    assert_eq!(issue_body["expires_in_seconds"], 90);
    assert!(issue_body["expires_at_ms"].as_i64().unwrap() >= issued_at + 89_000);
    assert!(issue_body["expires_at_ms"].as_i64().unwrap() <= issued_at + 91_000);

    let deep_link = reqwest::Url::parse(issue_body["deep_link_url"].as_str().unwrap()).unwrap();
    assert_eq!(deep_link.scheme(), "bluey");
    assert_eq!(deep_link.host_str(), Some("jobs"));
    assert_eq!(deep_link.path(), "/interview-prep");
    let query = deep_link.query_pairs().collect::<Vec<_>>();
    assert_eq!(query.len(), 1);
    assert_eq!(query[0].0, "nonce");
    assert_eq!(query[0].1, nonce);
    assert!(!deep_link.as_str().contains("application-1"));
    assert!(!deep_link.as_str().contains("receipt-1"));

    let (stored_hash, stored_snapshot): (String, String) = pool
        .get()
        .unwrap()
        .query_row(
            "SELECT nonce_hash, snapshot_json FROM jobs_bluey_handoffs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_ne!(stored_hash, nonce);
    assert!(!stored_hash.contains(nonce));
    assert!(stored_snapshot.starts_with("bluey-jobs:v1:"));

    let mismatch = redeem(&router, &other_token, nonce).await;
    assert_eq!(mismatch.status(), StatusCode::GONE);

    let redeemed = redeem(&router, &owner_token, nonce).await;
    assert_eq!(redeemed.status(), StatusCode::OK);
    assert_eq!(
        redeemed.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let body: Value =
        serde_json::from_slice(&to_bytes(redeemed.into_body(), 256 * 1024).await.unwrap()).unwrap();
    assert_eq!(body["application_id"], "application-1");
    assert_eq!(
        body.pointer("/snapshot/application/receipt_id"),
        Some(&json!("receipt-1"))
    );
    assert_eq!(
        body.pointer("/snapshot/application/submission_fingerprint"),
        Some(&json!("c".repeat(64)))
    );
    assert_eq!(
        body.pointer("/snapshot/grounding/resume_checksum"),
        Some(&json!("resume-checksum-1"))
    );
    assert_eq!(
        body.pointer("/snapshot/grounding/resume_document_sha256"),
        Some(&json!("a".repeat(64)))
    );
    assert_eq!(
        body.pointer("/snapshot/application/verified_claim_ids"),
        Some(&json!(["fact-1"]))
    );
    let encoded = serde_json::to_string(&body["snapshot"]).unwrap();
    assert!(encoded.contains("Platform Engineer"));
    assert!(encoded.contains("Built reliable systems"));
    assert!(!encoded.contains("candidate@example.com"));
    assert!(!encoded.contains("jobs/receipts/"));
    assert!(!encoded.contains("boards.greenhouse.io"));

    let replay = redeem(&router, &owner_token, nonce).await;
    assert_eq!(replay.status(), StatusCode::GONE);

    let malformed = redeem(&router, &owner_token, "short").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
}

async fn redeem(router: &axum::Router, token: &str, nonce: &str) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::post("/api/jobs/bluey-handoffs/redeem")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "nonce": nonce })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}

fn harness() -> (axum::Router, DbPool, Account, Account) {
    let db_path = std::env::temp_dir().join(format!(
        "bluey-jobs-handoff-api-{}.db",
        uuid::Uuid::new_v4()
    ));
    let pool = open_pool(&db_path).unwrap();
    run_migrations(&pool).unwrap();
    let owner = Account::create(&pool, "handoff-owner-api@example.com", "stub").unwrap();
    let other = Account::create(&pool, "handoff-other-api@example.com", "stub").unwrap();
    let config = Config {
        port: 0,
        db_path,
        db_backend: ServerDbBackend::Sqlite,
        database_url: None,
        jwt_secret: JWT_SECRET.to_string(),
        public_url: "https://bluey.test".to_string(),
        stripe_secret_key: None,
        stripe_webhook_secret: None,
        upstream: UpstreamKeys::default(),
        upstream_spend_guard: None,
        smtp: None,
        admin_emails: Vec::new(),
        trial_abuse: TrialAbuseConfig::default(),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: false,
        object_storage: None,
        log_storage: None,
    };
    (
        api::build_jobs_router(pool.clone(), config),
        pool,
        owner,
        other,
    )
}

fn seed_submitted_application(pool: &DbPool, account_id: &str) {
    let receipt = json!({
        "schemaVersion": 1,
        "receiptId": "receipt-1",
        "accountId": account_id,
        "applicationId": "application-1",
        "runId": "run-1",
        "_bluey_server_submission_fingerprint_v1": "c".repeat(64),
        "job": {
            "externalId": "provider-job-1",
            "canonicalUrl": "https://boards.greenhouse.io/acme/jobs/1?candidate=private",
            "company": "Acme",
            "title": "Platform Engineer",
            "location": "New York, NY",
            "workplace": "hybrid",
            "description": "Build reliable systems.",
            "source": "greenhouse"
        },
        "packet": {
            "jobId": "job-1",
            "resumeVersionId": "resume-1",
            "answers": {
                "motivation": "I enjoy dependable systems.",
                "candidate_email": "candidate@example.com"
            },
            "verifiedClaimIds": ["fact-1"]
        },
        "documents": [{
            "kind": "resume",
            "versionId": "resume-1",
            "storageKey": "jobs/receipts/receipt-1/resume.pdf",
            "sha256": "a".repeat(64)
        }],
        "result": {
            "status": "submitted",
            "confirmationText": "Application received",
            "submittedAt": "2026-07-12T12:00:00Z"
        },
        "screenshotKeys": ["jobs/receipts/receipt-1/confirmation.png"]
    });
    let application = json!({
        "id": "application-1",
        "job_id": "job-1",
        "resume_version_id": "resume-1",
        "state": "submitted",
        "submission_mode": "review_first",
        "match_score": 100,
        "answers": [],
        "cover_letter": "",
        "receipt": receipt,
        "run_id": "run-1",
        "created_at_ms": 1,
        "updated_at_ms": 2,
        "submitted_at_ms": 2
    });
    let resume_content = json!({
        "contact": { "name": "Candidate", "email": "candidate@example.com" },
        "summary": "Built reliable systems.",
        "skills": ["Rust", "TypeScript"]
    });
    let resume_evidence = json!({
        "id": "resume-evidence-1",
        "application_id": "application-1",
        "kind": "resume",
        "label": "Submitted resume",
        "provider": "greenhouse",
        "file_name": "resume.pdf",
        "media_type": "application/pdf",
        "storage_key": "jobs/receipts/receipt-1/resume.pdf",
        "sha256": "a".repeat(64),
        "resume_version_id": "resume-1",
        "occurred_at_ms": 2,
        "metadata": {},
        "created_at_ms": 2
    });
    let confirmation_evidence = json!({
        "id": "confirmation-evidence-1",
        "application_id": "application-1",
        "kind": "submission_confirmation",
        "label": "Application received",
        "provider": "greenhouse",
        "file_name": "confirmation.png",
        "media_type": "image/png",
        "storage_key": "jobs/receipts/receipt-1/confirmation.png",
        "sha256": "b".repeat(64),
        "resume_version_id": "resume-1",
        "occurred_at_ms": 2,
        "metadata": {},
        "created_at_ms": 2
    });

    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO jobs_postings (
            id, account_id, canonical_key, posting_json, source, canonical_url,
            company, title, location, match_score, status, created_at_ms, updated_at_ms
         ) VALUES ('job-1', ?1, 'job-1', '{}', 'greenhouse', '', 'Acme',
                   'Platform Engineer', 'New York, NY', 100, 'matched', 1, 1)",
        [account_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO jobs_resume_versions (
            id, account_id, job_id, version_no, mode, content_json, diff_json,
            claim_ids_json, checksum, created_at_ms
         ) VALUES ('resume-1', ?1, 'job-1', 1, 'factual', ?2, '{}',
                   '[\"fact-1\"]', 'resume-checksum-1', 1)",
        rusqlite::params![account_id, resume_content.to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO jobs_applications (
            id, account_id, job_id, resume_version_id, state, application_json,
            created_at_ms, updated_at_ms, submitted_at_ms
         ) VALUES ('application-1', ?1, 'job-1', 'resume-1', 'submitted', ?2, 1, 2, 2)",
        rusqlite::params![account_id, application.to_string()],
    )
    .unwrap();
    for (id, hash, payload) in [
        ("resume-evidence-1", "resume-event-1", resume_evidence),
        (
            "confirmation-evidence-1",
            "confirmation-event-1",
            confirmation_evidence,
        ),
    ] {
        conn.execute(
            "INSERT INTO jobs_application_evidence (
                id, account_id, application_id, kind, provider_event_hash,
                evidence_json, occurred_at_ms, created_at_ms
             ) VALUES (?1, ?2, 'application-1',
                       CASE WHEN ?1 = 'resume-evidence-1' THEN 'resume' ELSE 'submission_confirmation' END,
                       ?3, ?4, 2, 2)",
            rusqlite::params![id, account_id, hash, payload.to_string()],
        )
        .unwrap();
    }
}

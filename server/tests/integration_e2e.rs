//! Codex Stage 23: end-to-end integration tests with wiremock.
//!
//! Spins up the server's full Axum router pointed at wiremock instances
//! that stand in for OpenAI / Anthropic / Deepgram / Stripe. Exercises
//! the customer money-path top-to-bottom.

#![cfg(test)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::json;
use serial_test::serial;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use tower::ServiceExt;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use bluey_server::auth;
use bluey_server::config::{
    Config, ObjectStorageConfig, SmtpConfig, UpstreamKeys, UpstreamSpendGuard,
};
use bluey_server::db::accounts::Account;
use bluey_server::db::jobs::{
    self, BrowserSession, DiscoverySourceInput, Intervention, JobPosting, JobPreferences,
};
use bluey_server::db::usage::{self, UsageEvent};
use bluey_server::db::{idempotency, open_pool, run_migrations, DbPool};
use cue_core::prompt_contracts::{
    LEGACY_MANAGED_PROVIDER_BASE_CONTRACT_V0_1_99_TO_101, MANAGED_PROVIDER_BASE_CONTRACT,
};

/// Test harness: starts wiremocks, builds an AppState pointed at them,
/// returns the axum Router ready for ServiceExt::oneshot.
struct Harness {
    pub router: axum::Router,
    pub jobs_router: axum::Router,
    pub pool: DbPool,
    pub openai: MockServer,
    pub anthropic: MockServer,
    pub stripe: MockServer,
    pub square: MockServer,
    pub deepgram: MockServer,
    pub mail: MockServer,
}

type HmacSha256 = Hmac<Sha256>;

struct SignedWorkerRequest<'a> {
    path: &'a str,
    scope: &'a str,
    worker_id: &'a str,
    timestamp: u64,
    nonce: &'a str,
    signed_body: &'a [u8],
    actual_body: &'a [u8],
    signing_key: &'a str,
}

fn signed_worker_request(input: SignedWorkerRequest<'_>) -> Request<Body> {
    let content_sha256 = hex::encode(Sha256::digest(input.signed_body));
    let canonical = format!(
        "bluey-jobs-worker-v1\n{}\n{}\n{}\nbluey-jobs-api\n{}\nPOST\n{}\n{content_sha256}",
        input.timestamp, input.nonce, input.worker_id, input.scope, input.path,
    );
    let mut mac = HmacSha256::new_from_slice(input.signing_key.as_bytes()).unwrap();
    mac.update(canonical.as_bytes());
    Request::post(input.path)
        .header("x-bluey-jobs-worker-id", input.worker_id)
        .header("x-bluey-jobs-worker-timestamp", input.timestamp.to_string())
        .header("x-bluey-jobs-worker-nonce", input.nonce)
        .header("x-bluey-jobs-worker-audience", "bluey-jobs-api")
        .header("x-bluey-jobs-worker-scope", input.scope)
        .header("x-bluey-jobs-worker-content-sha256", content_sha256)
        .header(
            "x-bluey-jobs-worker-signature",
            hex::encode(mac.finalize().into_bytes()),
        )
        .body(Body::from(input.actual_body.to_vec()))
        .unwrap()
}

fn pcm16_mono_wav(seconds: u32) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 16_000;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    const BLOCK_ALIGN: u16 = CHANNELS * (BITS_PER_SAMPLE / 8);
    const BYTE_RATE: u32 = SAMPLE_RATE * BLOCK_ALIGN as u32;

    let data_len = BYTE_RATE.checked_mul(seconds).unwrap();
    let riff_len = 36_u32.checked_add(data_len).unwrap();
    let mut wav = Vec::with_capacity((riff_len + 8) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&BYTE_RATE.to_le_bytes());
    wav.extend_from_slice(&BLOCK_ALIGN.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.resize((riff_len + 8) as usize, 0);
    wav
}

#[tokio::test]
#[serial]
async fn jobs_worker_signatures_reject_replay_expiry_and_body_tampering() {
    const SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef";
    const PATH: &str = "/api/jobs/internal/discovery/lease";
    const NONCE: &str = "abcdef0123456789abcdef0123456789";
    std::env::set_var("BLUEY_JOBS_WORKER_SIGNING_KEY", SIGNING_KEY);
    let harness = boot_harness().await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let accepted = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: PATH,
            scope: "discovery",
            worker_id: "integration-signed-worker",
            timestamp: now,
            nonce: NONCE,
            signed_body: b"",
            actual_body: b"",
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::NO_CONTENT);

    let replayed = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: PATH,
            scope: "discovery",
            worker_id: "integration-signed-worker",
            timestamp: now,
            nonce: NONCE,
            signed_body: b"",
            actual_body: b"",
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(replayed.status(), StatusCode::UNAUTHORIZED);

    let expired = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: PATH,
            scope: "discovery",
            worker_id: "integration-signed-worker",
            timestamp: now - 91,
            nonce: "abcdef0123456789abcdef0123456790",
            signed_body: b"",
            actual_body: b"",
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);

    let tampered = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: PATH,
            scope: "discovery",
            worker_id: "integration-signed-worker",
            timestamp: now,
            nonce: "abcdef0123456789abcdef0123456791",
            signed_body: b"",
            actual_body: br#"{"forged":true}"#,
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(tampered.status(), StatusCode::UNAUTHORIZED);
    std::env::remove_var("BLUEY_JOBS_WORKER_SIGNING_KEY");
}

#[tokio::test]
#[serial]
async fn jobs_authenticated_reads_return_retry_after_when_the_bucket_is_exhausted() {
    std::env::set_var("BLUEY_LIMIT_JOBS_READ_PER_MIN", "1");
    std::env::set_var("BLUEY_LIMIT_JOBS_READ_PER_MIN_BURST", "1");
    let harness = boot_harness().await;
    let access = signup_and_login(
        &harness,
        "jobs-rate-limit@example.com",
        "valid-password-123",
    )
    .await;

    let first = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let limited = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().get("retry-after").is_some());
    std::env::remove_var("BLUEY_LIMIT_JOBS_READ_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_JOBS_READ_PER_MIN_BURST");
}

#[tokio::test]
#[serial]
async fn jobs_local_run_delivery_is_rate_limited_before_ticket_enumeration() {
    std::env::set_var("BLUEY_LIMIT_JOBS_RUN_PER_MIN", "1");
    std::env::set_var("BLUEY_LIMIT_JOBS_RUN_PER_MIN_BURST", "1");
    let harness = boot_harness().await;
    let request = || {
        Request::post("/api/jobs/local-runs/missing-run/claim")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "ticket": "a".repeat(64) })).unwrap(),
            ))
            .unwrap()
    };

    let first = harness
        .jobs_router
        .clone()
        .oneshot(request())
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::NOT_FOUND);

    let limited = harness
        .jobs_router
        .clone()
        .oneshot(request())
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().get("retry-after").is_some());
    std::env::remove_var("BLUEY_LIMIT_JOBS_RUN_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_JOBS_RUN_PER_MIN_BURST");
}

#[tokio::test]
#[serial]
async fn jobs_cross_account_match_ids_are_indistinguishable_from_missing_ids() {
    let harness = boot_harness().await;
    let owner = signup_and_login(&harness, "jobs-owner@example.com", "valid-password-123").await;
    let other = signup_and_login(&harness, "jobs-other@example.com", "valid-password-123").await;
    let saved = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/matches")
                .header("authorization", format!("Bearer {owner}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "canonical_url": "https://careers.example.com/acme/tenant-test",
                        "pasted_description": "Build reliable services.",
                        "company": "Acme",
                        "title": "Software Engineer",
                        "location": "New York, NY",
                        "workplace": "hybrid"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let body = axum::body::to_bytes(saved.into_body(), 64 * 1024)
        .await
        .unwrap();
    let posting: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let posting_id = posting["id"].as_str().unwrap();

    let cross_tenant = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(format!("/api/jobs/matches/{posting_id}"))
                .header("authorization", format!("Bearer {other}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let missing = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/matches/job-does-not-exist")
                .header("authorization", format!("Bearer {other}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_tenant.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let cross_body = axum::body::to_bytes(cross_tenant.into_body(), 64 * 1024)
        .await
        .unwrap();
    let missing_body = axum::body::to_bytes(missing.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(cross_body, missing_body);
}

#[tokio::test]
#[serial]
async fn jobs_candidate_feedback_is_server_owned_and_tenant_scoped() {
    let harness = boot_harness().await;
    let owner =
        signup_and_login(&harness, "feedback-owner@example.com", "valid-password-123").await;
    let other =
        signup_and_login(&harness, "feedback-other@example.com", "valid-password-123").await;
    let saved = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/matches")
                .header("authorization", format!("Bearer {owner}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "canonical_url": "https://careers.example.com/acme/feedback-test",
                        "pasted_description": "Build reliable customer workflows.",
                        "company": "Acme",
                        "title": "Software Engineer",
                        "location": "New York, NY",
                        "workplace": "hybrid"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let body = axum::body::to_bytes(saved.into_body(), 64 * 1024)
        .await
        .unwrap();
    let posting: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let posting_id = posting["id"].as_str().unwrap();

    let feedback = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/candidate-events")
                .header("authorization", format!("Bearer {owner}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "event_type": "match_feedback",
                        "job_id": posting_id,
                        "action": "pass",
                        "reasons": ["location"],
                        "note": "The commute is too long."
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(feedback.status(), StatusCode::OK);
    let body = axum::body::to_bytes(feedback.into_body(), 64 * 1024)
        .await
        .unwrap();
    let event: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(event["status"], "recorded");
    assert!(event["id"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(event["created_at_ms"]
        .as_i64()
        .is_some_and(|value| value > 0));

    let workspace = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .header("authorization", format!("Bearer {owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspace.status(), StatusCode::OK);
    let body = axum::body::to_bytes(workspace.into_body(), 256 * 1024)
        .await
        .unwrap();
    let workspace: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(workspace["candidate_events"].as_array().unwrap().len(), 1);

    let cross_tenant = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/candidate-events")
                .header("authorization", format!("Bearer {other}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "event_type": "match_feedback",
                        "job_id": posting_id,
                        "action": "pass"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_tenant.status(), StatusCode::NOT_FOUND);

    let invalid = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/candidate-events")
                .header("authorization", format!("Bearer {owner}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "event_type": "match_feedback",
                        "job_id": posting_id,
                        "action": "silently_delete"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn standalone_jobs_router_exposes_health_and_protects_customer_data() {
    let harness = boot_harness().await;
    let health = harness
        .jobs_router
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let workspace = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspace.status(), StatusCode::UNAUTHORIZED);

    let main_api_route = harness
        .jobs_router
        .clone()
        .oneshot(Request::get("/account/me").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(main_api_route.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn jobs_discovery_worker_requires_auth_and_persists_a_complete_snapshot() {
    const WORKER_TOKEN: &str = "jobs-discovery-worker-test-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let harness = boot_harness().await;
    let password_hash = auth::password::hash_password("valid-password-123").unwrap();
    let account = Account::create(
        &harness.pool,
        "jobs-discovery-worker@example.com",
        &password_hash,
    )
    .unwrap();
    let source = jobs::upsert_discovery_source(
        &harness.pool,
        &account.id,
        &DiscoverySourceInput {
            track_id: String::new(),
            provider: "greenhouse".to_string(),
            source_key: "acme".to_string(),
            company: "Acme".to_string(),
            run_interval_ms: 15 * 60 * 1_000,
        },
    )
    .unwrap();

    let unauthorized = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/discovery/lease")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let unauthorized_status = unauthorized.status();

    let lease_response = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/discovery/lease")
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("x-bluey-jobs-worker-id", "integration-worker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let lease_status = lease_response.status();
    let lease_body = axum::body::to_bytes(lease_response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let lease: serde_json::Value = serde_json::from_slice(&lease_body).unwrap();
    let completion_body = json!({
        "lease_token": lease["lease_token"],
        "replay_key": lease["replay_key"],
        "scheduled_for_ms": lease["scheduled_for_ms"],
        "complete_snapshot": true,
        "jobs": [{
            "external_id": "job-123",
            "canonical_url": "https://boards.greenhouse.io/acme/jobs/job-123?utm_source=test",
            "company": "Worker supplied company is not authoritative",
            "title": "Software Engineer",
            "location": "New York, NY",
            "workplace": "hybrid",
            "description": "Build reliable systems.",
            "compensation": "$170k-$200k",
            "posted_at_ms": 1,
            "payload_hash": "a".repeat(64)
        }]
    });
    let completed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/discovery/{}/complete",
                source.id
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&completion_body).unwrap()))
            .unwrap(),
        )
        .await
        .unwrap();
    let completion_status = completed.status();
    let postings = jobs::list_postings(&harness.pool, &account.id).unwrap();
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");

    assert_eq!(unauthorized_status, StatusCode::UNAUTHORIZED);
    assert_eq!(lease_status, StatusCode::OK);
    assert_eq!(lease["source"]["id"], source.id);
    assert_eq!(completion_status, StatusCode::OK);
    assert_eq!(postings.len(), 1);
    assert_eq!(postings[0].company, "Acme");
    assert_eq!(
        postings[0].canonical_url,
        "https://boards.greenhouse.io/acme/jobs/job-123"
    );
}

#[tokio::test]
#[serial]
async fn jobs_fact_route_owns_provenance_confirmation_and_timestamps() {
    let harness = boot_harness().await;
    let email = "jobs-facts-authority@example.com";
    let access = signup_and_login(&harness, email, "valid-password-123").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .unwrap();
    let forged = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/facts")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "category": "employment",
                        "label": "Forged imported claim",
                        "value": "Shipped it",
                        "source": "resume_import",
                        "verification_status": "confirmed",
                        "confirmed_by": "bluey_internal",
                        "confirmed_at_ms": 1,
                        "schema_version": 99,
                        "created_at_ms": 1,
                        "updated_at_ms": 1
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forged.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(jobs::list_facts(&harness.pool, &account.id)
        .unwrap()
        .is_empty());

    let saved = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/facts")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "category": "employment",
                        "label": "Direct user claim",
                        "value": "Shipped it"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let saved_body = axum::body::to_bytes(saved.into_body(), 64 * 1024)
        .await
        .unwrap();
    let saved_fact: serde_json::Value = serde_json::from_slice(&saved_body).unwrap();
    assert_eq!(saved_fact["source"], "user_entry");
    assert_eq!(saved_fact["verification_status"], "confirmed");
    assert_eq!(saved_fact["confirmed_by"], "user");
    assert_eq!(saved_fact["schema_version"], 1);
    assert!(saved_fact["confirmed_at_ms"].as_i64().unwrap() > 1);
    assert!(saved_fact["created_at_ms"].as_i64().unwrap() > 1);

    let imported = jobs::upsert_fact(
        &harness.pool,
        &account.id,
        &jobs::CareerFact {
            id: "server-import-proposal".to_string(),
            category: "employment".to_string(),
            label: "Imported proposal".to_string(),
            value: json!("Needs review"),
            source: "resume_import".to_string(),
            verification_status: "needs_confirmation".to_string(),
            confirmed_at_ms: None,
            confirmed_by: None,
            schema_version: 1,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    let generic_confirmation = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/facts")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "id": imported.id,
                        "category": "employment",
                        "label": "Imported proposal",
                        "value": "Confirm through generic save"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generic_confirmation.status(), StatusCode::CONFLICT);
    let imported_after = jobs::list_facts(&harness.pool, &account.id)
        .unwrap()
        .into_iter()
        .find(|fact| fact.id == "server-import-proposal")
        .unwrap();
    assert_eq!(imported_after.verification_status, "needs_confirmation");
    assert_eq!(imported_after.confirmed_by, None);
}

async fn setup_execution_lease_run(harness: &Harness) -> (String, String, String, String) {
    let email = "jobs-execution-lease@example.com";
    let access_token = signup_and_login(harness, email, "valid-password-123").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .unwrap();
    let profile = jobs::default_profile(&account.email);
    jobs::save_profile(&harness.pool, &account.id, &profile).unwrap();
    let identity =
        jobs::ensure_primary_application_identity(&harness.pool, &account.id, &account.email)
            .unwrap();
    let track = jobs::upsert_track(
        &harness.pool,
        &account.id,
        &jobs::CareerTrack {
            id: "track-execution-lease".to_string(),
            name: "Platform engineering".to_string(),
            role: "Platform Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: Some(identity.id),
            policy: jobs::CareerTrackPolicy {
                role_family: "software_engineering".to_string(),
                ..jobs::CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let posting = jobs::upsert_posting(
        &harness.pool,
        &account.id,
        &JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse".to_string(),
            external_id: "lease-integration".to_string(),
            company: "Acme".to_string(),
            title: "Platform Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/lease-integration".to_string(),
            description: "Build reliable systems.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            track_id: track.id,
            match_score: 92,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(now),
            last_verified_at_ms: Some(now),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            eligibility: None,
        },
        &profile,
        &JobPreferences::default(),
    )
    .unwrap();
    let (application, _) = jobs::prepare_application(
        &harness.pool,
        &account.id,
        &posting.id,
        "factual",
        "review_first",
    )
    .unwrap();
    let approved = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/applications/{}/approve", application.id))
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let application = jobs::get_application(&harness.pool, &account.id, &application.id)
        .unwrap()
        .unwrap();
    let run_id = "cloud-run-integration-lease".to_string();
    jobs::upsert_browser_session(
        &harness.pool,
        &account.id,
        &BrowserSession {
            id: run_id.clone(),
            runner: "cloud".to_string(),
            status: "queued".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Waiting for a browser".to_string(),
            application_id: Some(application.id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    let application =
        jobs::assign_application_run(&harness.pool, &account.id, &application.id, &run_id)
            .unwrap()
            .unwrap();
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let browser_profile_id = jobs::execution_browser_profile_id(&account.id, identity_id);
    (account.id, application.id, run_id, browser_profile_id)
}

#[tokio::test]
#[serial]
async fn jobs_customer_routes_cannot_forge_submission_evidence_or_submitted_state() {
    let harness = boot_harness().await;
    let (account_id, application_id, _, _) = setup_execution_lease_run(&harness).await;
    let auth = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let access_token = auth["access_token"].as_str().unwrap();

    let evidence_write = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/applications/{application_id}/evidence"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "kind": "confirmation",
                        "label": "Forged confirmation",
                        "storage_key": "forged/confirmation.png",
                        "sha256": "0".repeat(64),
                        "media_type": "image/png"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence_write.status(), StatusCode::METHOD_NOT_ALLOWED);

    let submitted_write = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::patch(format!("/api/jobs/applications/{application_id}"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "state": "submitted",
                        "submission_mode": "auto_submit"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(submitted_write.status(), StatusCode::CONFLICT);

    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_ne!(application.state, "submitted");
    assert!(application.submitted_at_ms.is_none());
    assert!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .is_empty()
    );
}

fn valid_receipt_pdf() -> Vec<u8> {
    b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\nstartxref\n0\n%%EOF\n".to_vec()
}

fn valid_receipt_png() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&[8, 2, 0, 0, 0]);
    png.extend_from_slice(&[0, 0, 0, 0]);
    png
}

fn cloud_receipt_request(
    harness: &Harness,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> serde_json::Value {
    use sha2::{Digest, Sha256};

    let application = jobs::get_application(&harness.pool, account_id, application_id)
        .unwrap()
        .unwrap();
    let posting = jobs::get_posting(&harness.pool, account_id, &application.job_id)
        .unwrap()
        .unwrap();
    let resume_id = application.resume_version_id.as_deref().unwrap();
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let application_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let approved_packet = application
        .receipt
        .pointer("/approved_execution/packet")
        .and_then(serde_json::Value::as_object)
        .unwrap();
    let approved_packet_checksum = application
        .receipt
        .pointer("/approved_execution/checksum")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let pdf = valid_receipt_pdf();
    let png = valid_receipt_png();
    let pdf_sha = hex::encode(Sha256::digest(&pdf));
    let png_sha = hex::encode(Sha256::digest(&png));
    let resume_key = "local-run/documents/resume.pdf";
    let screenshot_key = "local-run/final.png";
    json!({
        "account_id": account_id,
        "receipt": {
            "schemaVersion": 1,
            "receiptId": format!("receipt-{run_id}"),
            "accountId": account_id,
            "applicationId": application_id,
            "runId": run_id,
            "runner": "cloud",
            "generatedAt": "2026-07-12T12:00:00Z",
            "applicationIdentityId": identity_id,
            "browserProfileId": jobs::execution_browser_profile_id(account_id, identity_id),
            "adapter": "greenhouse",
            "adapterVersion": "1.0.0",
            "packet": {
                "jobId": application.job_id,
                "resumeVersionId": resume_id,
                "applicationEmail": application_email,
                "answers": approved_packet.get("answers").cloned().unwrap_or_else(|| json!({})),
                "verifiedClaimIds": approved_packet
                    .get("verifiedClaimIds")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
                "approvedPacketChecksum": approved_packet_checksum
            },
            "job": { "canonicalUrl": posting.canonical_url },
            "documents": [{
                "kind": "resume",
                "versionId": resume_id,
                "storageKey": resume_key,
                "sha256": pdf_sha,
                "mediaType": "application/pdf"
            }],
            "events": [],
            "result": {
                "status": "submitted",
                "confirmationText": "Application received",
                "confirmationUrl": "https://boards.greenhouse.io/acme/confirmation",
                "submittedAt": "2026-07-12T12:00:00Z"
            },
            "screenshotKeys": [screenshot_key]
        },
        "evidence_objects": [{
            "original_key": resume_key,
            "kind": "resume",
            "media_type": "application/pdf",
            "sha256": pdf_sha,
            "bytes_base64": base64::engine::general_purpose::STANDARD.encode(pdf)
        }, {
            "original_key": screenshot_key,
            "kind": "screenshot",
            "media_type": "image/png",
            "sha256": png_sha,
            "bytes_base64": base64::engine::general_purpose::STANDARD.encode(png)
        }]
    })
}

fn prepare_cloud_submission(
    harness: &Harness,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
) {
    jobs::reserve_application_attempt(&harness.pool, account_id, application_id, "cloud").unwrap();
    jobs::update_application(&harness.pool, account_id, application_id, "running", None)
        .unwrap()
        .unwrap();
    let lease = jobs::claim_execution_lease(
        &harness.pool,
        account_id,
        application_id,
        run_id,
        browser_profile_id,
        "receipt-integration-worker",
    )
    .unwrap();
    jobs::start_irreversible_submission(
        &harness.pool,
        account_id,
        application_id,
        run_id,
        &lease.lease_token,
        lease.fence,
    )
    .unwrap();
    jobs::finish_execution_lease(
        &harness.pool,
        account_id,
        application_id,
        run_id,
        &lease.lease_token,
        lease.fence,
        "submitted",
    )
    .unwrap();
}

async fn post_cloud_receipt(
    harness: &Harness,
    worker_token: &str,
    application_id: &str,
    body: &serde_json::Value,
) -> axum::response::Response {
    harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/applications/{application_id}/receipt"
            ))
            .header("authorization", format!("Bearer {worker_token}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
#[serial]
async fn jobs_execution_lease_routes_require_worker_auth_and_fence_submit() {
    const WORKER_TOKEN: &str = "jobs-execution-lease-worker-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let harness = boot_harness().await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    let claim_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "run_id": run_id,
        "browser_profile_id": browser_profile_id,
        "owner_id": "integration-worker-one"
    });

    let unauthorized = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/execution-leases/claim")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let forged_scope = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/execution-leases/claim")
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "application_id": application_id,
                        "run_id": run_id,
                        "browser_profile_id": "forged:browser-profile",
                        "owner_id": "integration-worker-one"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forged_scope.status(), StatusCode::CONFLICT);

    let invalid_claim = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/execution-leases/claim")
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "application_id": application_id,
                        "run_id": run_id,
                        "browser_profile_id": browser_profile_id,
                        "owner_id": ""
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_claim.status(), StatusCode::BAD_REQUEST);

    let missing_binding = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/execution-leases/claim")
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "application_id": application_id,
                        "run_id": "different-cloud-run",
                        "browser_profile_id": browser_profile_id,
                        "owner_id": "integration-worker-one"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_binding.status(), StatusCode::NOT_FOUND);

    let claimed = harness
        .router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/execution-leases/claim")
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(claimed.status(), StatusCode::OK);
    let claimed_body = axum::body::to_bytes(claimed.into_body(), 64 * 1024)
        .await
        .unwrap();
    let lease: serde_json::Value = serde_json::from_slice(&claimed_body).unwrap();
    assert_eq!(lease["run_id"], run_id);
    assert_eq!(lease["phase"], "prepared");
    let lease_token = lease["lease_token"].as_str().unwrap();
    let fence = lease["fence"].as_i64().unwrap();

    let heartbeat = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/execution-leases/{run_id}/heartbeat"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "account_id": account_id,
                    "application_id": application_id,
                    "lease_token": lease_token,
                    "fence": fence
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(heartbeat.status(), StatusCode::OK);

    let running = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/applications/{application_id}/state"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "account_id": account_id,
                    "state": "running"
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(running.status(), StatusCode::OK);

    let premature_failure = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/applications/{application_id}/state"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "account_id": account_id,
                    "state": "failed"
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(premature_failure.status(), StatusCode::CONFLICT);

    let irreversible_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "lease_token": lease_token,
        "fence": fence,
        "action": "submit"
    });
    let irreversible = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/execution-leases/{run_id}/irreversible"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&irreversible_body).unwrap()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(irreversible.status(), StatusCode::OK);
    let irreversible_bytes = axum::body::to_bytes(irreversible.into_body(), 64 * 1024)
        .await
        .unwrap();
    let irreversible_value: serde_json::Value =
        serde_json::from_slice(&irreversible_bytes).unwrap();
    assert_eq!(irreversible_value["phase"], "click_started");

    let replay = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/execution-leases/{run_id}/irreversible"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&irreversible_body).unwrap()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CONFLICT);

    let reconciled_failure = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/applications/{application_id}/state"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "account_id": account_id,
                    "state": "failed"
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reconciled_failure.status(), StatusCode::OK);
    let reconciled_bytes = axum::body::to_bytes(reconciled_failure.into_body(), 64 * 1024)
        .await
        .unwrap();
    let reconciled_value: serde_json::Value = serde_json::from_slice(&reconciled_bytes).unwrap();
    assert_eq!(reconciled_value["state"], "side_effect_unknown");

    let failed_finish = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/execution-leases/{run_id}/finish"
            ))
            .header("authorization", format!("Bearer {WORKER_TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "account_id": account_id,
                    "application_id": application_id,
                    "lease_token": lease_token,
                    "fence": fence,
                    "outcome": "failed"
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed_finish.status(), StatusCode::CONFLICT);

    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_execution_leases SET lease_expires_at_ms = ?2 WHERE run_id = ?1",
            rusqlite::params![run_id, chrono::Utc::now().timestamp_millis() - 1],
        )
        .unwrap();
    let finish_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "lease_token": lease_token,
        "fence": fence,
        "outcome": "submitted"
    });
    for _ in 0..2 {
        let finished = harness
            .router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/jobs/internal/execution-leases/{run_id}/finish"
                ))
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&finish_body).unwrap()))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(finished.status(), StatusCode::NO_CONTENT);
    }
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_cloud_receipt_requires_an_exact_terminal_submitted_lease_binding() {
    const WORKER_TOKEN: &str = "jobs-receipt-lease-test-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let harness = boot_harness().await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    jobs::reserve_application_attempt(&harness.pool, &account_id, &application_id, "cloud")
        .unwrap();
    jobs::update_application(&harness.pool, &account_id, &application_id, "running", None)
        .unwrap()
        .unwrap();
    let body = cloud_receipt_request(&harness, &account_id, &application_id, &run_id);

    let missing = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(missing.status(), StatusCode::CONFLICT);

    jobs::claim_execution_lease(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "nonterminal-receipt-worker",
    )
    .unwrap();
    let nonterminal = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(nonterminal.status(), StatusCode::CONFLICT);

    let mut mismatched = body;
    mismatched["receipt"]["runId"] = json!("forged-cloud-run");
    let mismatched_response =
        post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &mismatched).await;
    assert_eq!(mismatched_response.status(), StatusCode::BAD_REQUEST);
    assert!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .is_empty()
    );
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_cloud_receipt_is_atomic_and_exactly_idempotent() {
    const WORKER_TOKEN: &str = "jobs-receipt-idempotency-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let harness = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint,
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    prepare_cloud_submission(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let body = cloud_receipt_request(&harness, &account_id, &application_id, &run_id);
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/screenshot-[0-9a-f]{20}$";
    Mock::given(method("PUT"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(resume_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(valid_receipt_pdf(), "application/pdf"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(screenshot_path))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(screenshot_path))
        .respond_with(ResponseTemplate::new(200).set_body_raw(valid_receipt_png(), "image/png"))
        .expect(1)
        .mount(&object_store)
        .await;

    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "DELETE FROM jobs_browser_sessions WHERE id = ?1",
            rusqlite::params![run_id],
        )
        .unwrap();
    jobs::upsert_browser_session(
        &harness.pool,
        &account_id,
        &BrowserSession {
            id: "stale-cloud-session".to_string(),
            runner: "cloud".to_string(),
            status: "running".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Old run".to_string(),
            application_id: Some(application_id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    let missing_session = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(missing_session.status(), StatusCode::CONFLICT);
    assert!(object_store.received_requests().await.unwrap().is_empty());
    jobs::upsert_browser_session(
        &harness.pool,
        &account_id,
        &BrowserSession {
            id: run_id.clone(),
            runner: "cloud".to_string(),
            status: "running".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Submitting application".to_string(),
            application_id: Some(application_id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();

    let first = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(first.status(), StatusCode::OK);
    let replay = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(replay.status(), StatusCode::OK);
    let mut changed = body.clone();
    changed["receipt"]["result"]["confirmationText"] = json!("Different receipt content");
    let conflicting = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &changed).await;
    assert_eq!(conflicting.status(), StatusCode::CONFLICT);

    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "submitted");
    assert!(application
        .receipt
        .get("_bluey_server_submission_fingerprint_v1")
        .and_then(serde_json::Value::as_str)
        .is_some());
    assert_eq!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .len(),
        2
    );
    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_eq!(reservation.status, "submitted");
    let session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|session| session.id == run_id)
        .unwrap();
    assert_eq!(session.status, "complete");
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_receipt_deletes_request_owned_uploads_after_partial_failure() {
    const WORKER_TOKEN: &str = "jobs-receipt-cleanup-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let harness = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint,
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    prepare_cloud_submission(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let body = cloud_receipt_request(&harness, &account_id, &application_id, &run_id);
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/screenshot-[0-9a-f]{20}$";
    Mock::given(method("PUT"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(resume_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(valid_receipt_pdf(), "application/pdf"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(screenshot_path))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;

    let response = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .is_empty()
    );
    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "running");
    assert!(application
        .receipt
        .get("_bluey_server_submission_fingerprint_v1")
        .is_none());
    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_ne!(reservation.status, "submitted");
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

async fn resolve_intervention_request(
    harness: &Harness,
    access_token: &str,
    intervention_id: &str,
    body: serde_json::Value,
) -> axum::response::Response {
    harness
        .router
        .clone()
        .oneshot(
            Request::patch(format!("/api/jobs/interventions/{intervention_id}"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
#[serial]
async fn jobs_submission_approval_accepts_only_the_stored_provider_final_review() {
    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
    const WORKFLOW_TOKEN: &str = "jobs-workflow-test-token";
    let harness = boot_harness().await;
    let (account_id, application_id, run_id, _) = setup_execution_lease_run(&harness).await;
    jobs::update_application(&harness.pool, &account_id, &application_id, "running", None).unwrap();
    jobs::update_application(
        &harness.pool,
        &account_id,
        &application_id,
        "needs_input",
        None,
    )
    .unwrap();
    let access_token =
        auth::jwt::issue(JWT_SECRET, &account_id, auth::jwt::TokenKind::Access).unwrap();

    let generic = jobs::save_intervention(
        &harness.pool,
        &account_id,
        &Intervention {
            id: String::new(),
            application_id: Some(application_id.clone()),
            kind: "browser_takeover".to_string(),
            status: "open".to_string(),
            title: "Finish this application".to_string(),
            detail: "Review the preserved browser.".to_string(),
            choices: Vec::new(),
            resolution_kind: "browser_takeover".to_string(),
            resume_after_resolution: true,
            provider: String::new(),
            provider_message_id: String::new(),
            expires_at_ms: None,
            metadata: json!({
                "_bluey_worker_receipt_v1": true,
                "receipt": {
                    "status": "needs_input",
                    "issues": [],
                    "intervention": {
                        "kind": "browser_takeover",
                        "title": "Finish this application",
                        "detail": "Review the preserved browser.",
                        "takeoverUrl": "https://takeover.example/session",
                        "resolution": { "kind": "browser_takeover", "resumeAfter": true }
                    }
                }
            }),
            created_at_ms: 0,
            resolved_at_ms: None,
        },
    )
    .unwrap();
    let rejected = resolve_intervention_request(
        &harness,
        &access_token,
        &generic.id,
        json!({ "status": "resolved", "action": "approve_submission" }),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let title = "Review the Greenhouse application";
    let detail = "Review every employer-facing field and document in the preserved form, then approve submission.";
    let final_review = jobs::save_intervention(
        &harness.pool,
        &account_id,
        &Intervention {
            id: String::new(),
            application_id: Some(application_id.clone()),
            kind: "browser_takeover".to_string(),
            status: "open".to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            choices: Vec::new(),
            resolution_kind: "browser_takeover".to_string(),
            resume_after_resolution: true,
            provider: String::new(),
            provider_message_id: String::new(),
            expires_at_ms: None,
            metadata: json!({
                "_bluey_worker_receipt_v1": true,
                "receipt": {
                    "status": "needs_input",
                    "issues": [],
                    "intervention": {
                        "kind": "browser_takeover",
                        "title": title,
                        "detail": detail,
                        "takeoverUrl": "https://takeover.example/session",
                        "resolution": { "kind": "browser_takeover", "resumeAfter": true }
                    }
                }
            }),
            created_at_ms: 0,
            resolved_at_ms: None,
        },
    )
    .unwrap();
    let answer_rejected = resolve_intervention_request(
        &harness,
        &access_token,
        &final_review.id,
        json!({
            "status": "resolved",
            "action": "approve_submission",
            "answer": "ignore the stored final review"
        }),
    )
    .await;
    assert_eq!(answer_rejected.status(), StatusCode::BAD_REQUEST);

    std::env::set_var("BLUEY_JOBS_WORKFLOW_ORIGIN", harness.openai.uri());
    std::env::set_var("BLUEY_JOBS_WORKFLOW_TOKEN", WORKFLOW_TOKEN);
    let resume_path = format!("/workflows/applications/{account_id}/{run_id}/resume");
    Mock::given(method("POST"))
        .and(path(resume_path.as_str()))
        .and(header(
            "authorization",
            format!("Bearer {WORKFLOW_TOKEN}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "resumed": true })))
        .expect(1)
        .mount(&harness.openai)
        .await;

    let approved = resolve_intervention_request(
        &harness,
        &access_token,
        &final_review.id,
        json!({ "status": "resolved", "action": "approve_submission" }),
    )
    .await;
    let approved_status = approved.status();
    let approved_bytes = axum::body::to_bytes(approved.into_body(), 64 * 1024)
        .await
        .unwrap();
    let approved_value: serde_json::Value = serde_json::from_slice(&approved_bytes).unwrap();
    let requests = harness.openai.received_requests().await.unwrap();
    let resume_request = requests
        .iter()
        .find(|request| request.url.path() == resume_path)
        .unwrap();
    let resume_body: serde_json::Value = serde_json::from_slice(&resume_request.body).unwrap();
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_ORIGIN");
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_TOKEN");

    assert_eq!(approved_status, StatusCode::OK);
    assert_eq!(approved_value["intervention"]["status"], "approved");
    assert_eq!(approved_value["application"]["state"], "queued");
    assert_eq!(resume_body["action"], "approve_submission");
    assert_eq!(resume_body["field"], "");
    assert_eq!(resume_body["answer"], "");
}

#[tokio::test]
#[serial]
async fn jobs_local_submit_resume_recovers_after_consume_before_marker() {
    use sha2::{Digest, Sha256};

    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
    let harness = boot_harness().await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    jobs::set_entitlement_plan(&harness.pool, &account_id, "pro").unwrap();
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "DELETE FROM jobs_browser_sessions WHERE id = ?1",
            rusqlite::params![run_id],
        )
        .unwrap();
    jobs::upsert_browser_session(
        &harness.pool,
        &account_id,
        &BrowserSession {
            id: run_id.clone(),
            runner: "local".to_string(),
            status: "queued".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Waiting for Bluey Browser".to_string(),
            application_id: Some(application_id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    let ticket = "b".repeat(64);
    let ticket_hash = hex::encode(Sha256::digest(ticket.as_bytes()));
    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    let posting = jobs::get_posting(&harness.pool, &account_id, &application.job_id)
        .unwrap()
        .unwrap();
    let application_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    jobs::save_local_run_ticket(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &ticket_hash,
        &ticket,
        json!({
            "runId": run_id,
            "accountId": account_id,
            "applicationId": application_id,
            "jobId": application.job_id,
            "applicationIdentityId": application_identity_id,
            "browserProfileId": browser_profile_id,
            "runner": "local",
            "url": posting.canonical_url
        }),
        chrono::Utc::now().timestamp_millis() + 60_000,
    )
    .unwrap();

    let claimed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(claimed.status(), StatusCode::OK);
    let claimed_bytes = axum::body::to_bytes(claimed.into_body(), 64 * 1024)
        .await
        .unwrap();
    let claim: serde_json::Value = serde_json::from_slice(&claimed_bytes).unwrap();
    let result_capability = claim["_blueyCapabilities"]["result"]
        .as_str()
        .unwrap()
        .to_string();
    let resume_capability = claim["_blueyCapabilities"]["resume"]
        .as_str()
        .unwrap()
        .to_string();
    let submit_capability = claim["_blueyCapabilities"]["submit"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(result_capability, resume_capability);
    assert_ne!(result_capability, submit_capability);
    assert_ne!(resume_capability, submit_capability);

    let swapped_operation = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &resume_capability,
                        "receipt": { "status": "failed" }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(swapped_operation.status(), StatusCode::NOT_FOUND);

    let title = "Review the Greenhouse application";
    let detail = "Review every employer-facing field and document in the preserved form, then approve submission.";
    let final_review_receipt = json!({
        "status": "needs_input",
        "issues": [],
        "intervention": {
            "kind": "browser_takeover",
            "title": title,
            "detail": detail,
            "takeoverUrl": format!("bluey-jobs://resume/{run_id}?ticket={ticket}"),
            "resolution": { "kind": "browser_takeover", "resumeAfter": true }
        }
    });
    let paused = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &result_capability,
                        "receipt": final_review_receipt
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paused.status(), StatusCode::OK);
    let intervention = jobs::list_interventions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|item| item.title == title && item.status == "open")
        .unwrap();

    let unapproved_resume = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "capability": &resume_capability })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unapproved_resume.status(), StatusCode::CONFLICT);

    let unapproved_authority = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "capability": &submit_capability })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unapproved_authority.status(), StatusCode::CONFLICT);

    let unapproved_submit = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "ticket": ticket,
                        "receipt": { "status": "submitted", "issues": [] }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unapproved_submit.status(), StatusCode::CONFLICT);

    let access_token =
        auth::jwt::issue(JWT_SECRET, &account_id, auth::jwt::TokenKind::Access).unwrap();
    let approved = resolve_intervention_request(
        &harness,
        &access_token,
        &intervention.id,
        json!({ "status": "resolved", "action": "approve_submission" }),
    )
    .await;
    assert_eq!(approved.status(), StatusCode::OK);
    let approved_bytes = axum::body::to_bytes(approved.into_body(), 64 * 1024)
        .await
        .unwrap();
    let approved_value: serde_json::Value = serde_json::from_slice(&approved_bytes).unwrap();
    assert_eq!(
        approved_value["local_resume"]["action"],
        "approve_submission"
    );

    let consumed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(consumed.status(), StatusCode::OK);
    let consumed_bytes = axum::body::to_bytes(consumed.into_body(), 64 * 1024)
        .await
        .unwrap();
    let consumed_value: serde_json::Value = serde_json::from_slice(&consumed_bytes).unwrap();
    assert_eq!(consumed_value["action"], "approve_submission");
    assert_eq!(consumed_value["intervention_id"], intervention.id);
    let consumed_at_ms: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT consumed_at_ms FROM jobs_local_run_resume_actions WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| row.get(0),
        )
        .unwrap();

    let replayed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replayed.status(), StatusCode::OK);
    let replayed_at_ms: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT consumed_at_ms FROM jobs_local_run_resume_actions WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(replayed_at_ms, consumed_at_ms);
    let consumption_events: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM jobs_run_events
              WHERE run_id = ?1 AND event_type = 'local_resume_approval_consumed'",
            rusqlite::params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(consumption_events, 1);
    assert!(jobs::local_submission_approval_consumed(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
    )
    .unwrap());

    let authorize_submit = || {
        Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "capability": &submit_capability })).unwrap(),
            ))
            .unwrap()
    };
    let authorized = harness
        .router
        .clone()
        .oneshot(authorize_submit())
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    let authorized_body = axum::body::to_bytes(authorized.into_body(), 64 * 1024)
        .await
        .unwrap();
    let authorized_value: serde_json::Value = serde_json::from_slice(&authorized_body).unwrap();
    assert_eq!(authorized_value["authorized"], true);

    jobs::set_entitlement_plan(&harness.pool, &account_id, "free").unwrap();
    let revoked = harness
        .router
        .clone()
        .oneshot(authorize_submit())
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::CONFLICT);
    jobs::set_entitlement_plan(&harness.pool, &account_id, "pro").unwrap();

    let future_expiry = chrono::Utc::now().timestamp_millis() + 60_000;
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_local_run_resume_actions SET expires_at_ms = ?2 WHERE run_id = ?1",
            rusqlite::params![run_id, chrono::Utc::now().timestamp_millis() - 1],
        )
        .unwrap();
    let expired = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::CONFLICT);
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_local_run_resume_actions SET expires_at_ms = ?2 WHERE run_id = ?1",
            rusqlite::params![run_id, future_expiry],
        )
        .unwrap();

    let terminal = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "ticket": ticket,
                        "receipt": { "status": "failed" }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(terminal.status(), StatusCode::OK);
    let after_terminal = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_terminal.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial]
async fn jobs_local_side_effect_unknown_is_terminal_and_requires_reconciliation() {
    use sha2::{Digest, Sha256};

    let harness = boot_harness().await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    jobs::set_entitlement_plan(&harness.pool, &account_id, "pro").unwrap();
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "DELETE FROM jobs_browser_sessions WHERE id = ?1",
            rusqlite::params![run_id],
        )
        .unwrap();
    jobs::upsert_browser_session(
        &harness.pool,
        &account_id,
        &BrowserSession {
            id: run_id.clone(),
            runner: "local".to_string(),
            status: "queued".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Waiting for Bluey Browser".to_string(),
            application_id: Some(application_id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    jobs::reserve_application_attempt(&harness.pool, &account_id, &application_id, "local")
        .unwrap();
    let ticket = "c".repeat(64);
    let ticket_hash = hex::encode(Sha256::digest(ticket.as_bytes()));
    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    let posting = jobs::get_posting(&harness.pool, &account_id, &application.job_id)
        .unwrap()
        .unwrap();
    let application_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    jobs::save_local_run_ticket(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &ticket_hash,
        &ticket,
        json!({
            "runId": run_id,
            "accountId": account_id,
            "applicationId": application_id,
            "jobId": application.job_id,
            "applicationIdentityId": application_identity_id,
            "browserProfileId": browser_profile_id,
            "runner": "local",
            "url": posting.canonical_url
        }),
        chrono::Utc::now().timestamp_millis() + 60_000,
    )
    .unwrap();
    let claimed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(claimed.status(), StatusCode::OK);

    let uncertain_body = json!({
        "ticket": ticket,
        "receipt": {
            "status": "side_effect_unknown",
            "issues": [{ "field": "submission", "message": "Outcome unknown" }],
            "intervention": {
                "kind": "browser_takeover",
                "title": "Reconcile this submission",
                "detail": "Check the employer portal before taking another action.",
                "takeoverUrl": format!("bluey-jobs://resume/{run_id}?ticket={ticket}"),
                "resolution": { "kind": "browser_takeover", "resumeAfter": false }
            },
            "screenshotPath": "/private/local/path.png"
        },
        "receiptBundle": {
            "receiptId": "must-not-be-finalized",
            "result": { "status": "submitted" }
        },
        "evidenceObjects": []
    });
    let uncertain = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&uncertain_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(uncertain.status(), StatusCode::OK);
    let uncertain_bytes = axum::body::to_bytes(uncertain.into_body(), 64 * 1024)
        .await
        .unwrap();
    let uncertain_application: serde_json::Value =
        serde_json::from_slice(&uncertain_bytes).unwrap();
    assert_eq!(uncertain_application["state"], "side_effect_unknown");
    assert_eq!(
        uncertain_application["receipt"]["local_reconciliation"]["receipt"]["intervention"]
            ["title"],
        "Reconcile this submission"
    );
    assert!(
        uncertain_application["receipt"]["local_reconciliation"]["receipt"]
            .get("screenshotPath")
            .is_none()
    );
    assert_ne!(
        uncertain_application["receipt"]["receiptId"],
        "must-not-be-finalized"
    );
    assert!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .is_empty()
    );
    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_eq!(reservation.status, "side_effect_unknown");
    let session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|session| session.id == run_id)
        .unwrap();
    assert_eq!(session.status, "needs_input");
    assert_eq!(
        session.current_step,
        "Submission outcome needs reconciliation"
    );
    let ticket_after = jobs::get_local_run_ticket_by_hash(&harness.pool, &run_id, &ticket_hash)
        .unwrap()
        .unwrap();
    assert_eq!(ticket_after.status, "side_effect_unknown");

    let replay = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&uncertain_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    let downgrade = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "ticket": ticket,
                        "receipt": { "status": "failed" }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(downgrade.status(), StatusCode::CONFLICT);
    let resume_replay = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "ticket": ticket })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resume_replay.status(), StatusCode::CONFLICT);
    let stored = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, "side_effect_unknown");
}

async fn boot_harness() -> Harness {
    boot_harness_with_upstream(UpstreamKeys {
        openai_api_key: Some("sk-test-openai".to_string()),
        anthropic_api_key: Some("sk-test-anthropic".to_string()),
        gemini_api_key: None,
        deepseek_api_key: None,
        zai_api_key: None,
        deepgram_api_key: Some("dg-test".to_string()),
        ollama_base_url: None,
    })
    .await
}

async fn boot_harness_with_upstream(upstream: UpstreamKeys) -> Harness {
    boot_harness_with_upstream_and_admin_emails(upstream, vec![]).await
}

async fn boot_harness_with_upstream_and_admin_emails(
    upstream: UpstreamKeys,
    admin_emails: Vec<String>,
) -> Harness {
    boot_harness_with_options(
        upstream,
        admin_emails,
        Some(UpstreamSpendGuard {
            limit_cents: 100_000_000,
            window_hours: 24,
        }),
    )
    .await
}

async fn boot_harness_with_options(
    upstream: UpstreamKeys,
    admin_emails: Vec<String>,
    upstream_spend_guard: Option<UpstreamSpendGuard>,
) -> Harness {
    boot_harness_with_config(upstream, admin_emails, upstream_spend_guard, |_| {}).await
}

async fn boot_harness_with_config(
    upstream: UpstreamKeys,
    admin_emails: Vec<String>,
    upstream_spend_guard: Option<UpstreamSpendGuard>,
    configure: impl FnOnce(&mut Config),
) -> Harness {
    let openai = MockServer::start().await;
    let anthropic = MockServer::start().await;
    let stripe = MockServer::start().await;
    let square = MockServer::start().await;
    let deepgram = MockServer::start().await;
    let mail = MockServer::start().await;

    let path = std::env::temp_dir().join(format!("bluey-e2e-{}.db", uuid::Uuid::new_v4()));
    let pool = open_pool(&path).unwrap();
    run_migrations(&pool).unwrap();

    let mut config = Config {
        port: 0,
        db_path: path,
        db_backend: bluey_server::config::ServerDbBackend::Sqlite,
        database_url: None,
        jwt_secret: "test-secret-at-least-32-chars-long-xxx".to_string(),
        public_url: "http://localhost:8080".to_string(),
        stripe_secret_key: Some("sk_test_e2e".to_string()),
        stripe_webhook_secret: Some("whsec_test_e2e".to_string()),
        smtp: Some(SmtpConfig {
            host: "smtp.resend.com".to_string(),
            port: 587,
            username: Some("resend".to_string()),
            password: Some("test-resend-key".to_string()),
            from: "Bluey <hello@bluey.sh>".to_string(),
            starttls: true,
        }),
        upstream,
        upstream_spend_guard,
        admin_emails,
        trial_abuse: bluey_server::config::TrialAbuseConfig::default(),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: false,
        object_storage: None,
        log_storage: None,
    };
    configure(&mut config);

    // Override upstream URLs by env. The dispatcher reads from
    // hard-coded URLs today; for the wiremock harness we need the
    // dispatcher to honor BLUEY_TEST_OPENAI_URL etc. That's a small
    // patch to dispatcher.rs covered in Stage 23 commit so this test
    // can hit the mock.
    std::env::set_var("BLUEY_ROUTE_POLICY", "quality_first");
    std::env::set_var("BLUEY_ANSWER_PLAN_ROUTING", "0");
    std::env::set_var("BLUEY_TEST_OPENAI_URL", openai.uri());
    std::env::set_var("BLUEY_TEST_ANTHROPIC_URL", anthropic.uri());
    std::env::set_var("BLUEY_TEST_STRIPE_URL", stripe.uri());
    std::env::set_var("BLUEY_TEST_SQUARE_URL", square.uri());
    std::env::set_var("BLUEY_TEST_DEEPGRAM_URL", deepgram.uri());
    std::env::set_var("BLUEY_RESEND_API_BASE_URL", mail.uri());

    let jobs_router = bluey_server::api::build_jobs_router(pool.clone(), config.clone());
    let router = bluey_server::api::build_router(pool.clone(), config);

    Harness {
        router,
        jobs_router,
        pool,
        openai,
        anthropic,
        stripe,
        square,
        deepgram,
        mail,
    }
}

fn sample_usage(request_id: &str, bluey_cost_cents: i64) -> UsageEvent {
    UsageEvent {
        request_id: request_id.to_string(),
        kind: "llm".to_string(),
        task_type: None,
        lane: Some("instant".to_string()),
        provider: Some("openai".to_string()),
        model: Some("gpt-5.4-mini".to_string()),
        input_tokens: 10,
        output_tokens: 5,
        latency_ms: 20,
        cost_cents_to_bluey: bluey_cost_cents,
        cost_cents_to_customer: bluey_cost_cents,
        was_speculative: false,
        was_fallback: false,
    }
}

async fn signup_and_login(harness: &Harness, email: &str, password: &str) -> String {
    let normalized_email = email.trim().to_lowercase();
    let password_hash = auth::password::hash_password(password).unwrap();
    Account::create(&harness.pool, &normalized_email, &password_hash).unwrap();
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts SET email_verified_at = datetime('now') WHERE email = ?1",
            rusqlite::params![normalized_email],
        )
        .unwrap();
    let auth = login(harness, &normalized_email, password).await;
    auth["access_token"].as_str().unwrap().to_string()
}

fn set_square_billing_env() {
    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var("SQUARE_SANDBOX_APPLICATION_ID", "sandbox-app");
}

fn clear_square_billing_env() {
    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_APPLICATION_ID");
}

fn square_reload_reference_id_for_test(account_id: &str) -> String {
    let compact = account_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect::<String>();
    format!("br_{compact}")
}

fn square_signature_for_test(secret: &str, body: &str) -> String {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let url = "http://localhost:8080/billing/square/webhook";
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

fn stripe_signature_for_test(secret: &str, body: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let t = chrono::Utc::now().timestamp().to_string();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{t}.{body}").as_bytes());
    let v1 = hex::encode(mac.finalize().into_bytes());
    format!("t={t},v1={v1}")
}

async fn login(harness: &Harness, email: &str, password: &str) -> serde_json::Value {
    let req = Request::post("/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200, "login failed");
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn assert_admin_customers_allowed(harness: &Harness, access: &str) {
    let req = Request::get("/admin/customers")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = harness.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

fn extract_six_digit_code(text: &str) -> Option<String> {
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            run.push(ch);
            if run.len() == 6 {
                return Some(run);
            }
        } else {
            run.clear();
        }
    }
    None
}

async fn signup_with_otp(harness: &Harness, email: &str, password: &str) -> serde_json::Value {
    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header("Authorization", "Bearer test-resend-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"email-otp"})))
        .mount(&harness.mail)
        .await;

    let start = Request::post("/auth/signup/start")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": password,
                "terms_accepted": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(start).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let requests = harness.mail.received_requests().await.unwrap();
    let mail_body: serde_json::Value =
        serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    assert_eq!(
        mail_body["to"][0].as_str().unwrap(),
        email.trim().to_lowercase()
    );
    assert_eq!(mail_body["subject"], "Your Bluey verification code");
    let code = extract_six_digit_code(mail_body["text"].as_str().unwrap()).unwrap();

    let confirm = Request::post("/auth/signup/confirm")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "otp": code
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(confirm).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn start_trial(harness: &Harness, device_fingerprint: &str) -> serde_json::Value {
    let req = Request::post("/auth/trial/start")
        .header("content-type", "application/json")
        .header("user-agent", "bluey-e2e-trial")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "device_fingerprint": device_fingerprint,
                "terms_accepted": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "trial start failed");
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
#[serial]
async fn signup_otp_email_confirms_and_marks_email_verified() {
    let h = boot_harness().await;

    let email = "otp-smoke@bluey.sh";
    let password = "longenoughpw";
    let auth = signup_with_otp(&h, email, password).await;
    assert_eq!(auth["account"]["email"], email);
    assert!(auth["access_token"].as_str().unwrap().len() > 20);

    let conn = h.pool.get().unwrap();
    let verified_at: Option<String> = conn
        .query_row(
            "SELECT email_verified_at FROM accounts WHERE email = ?1",
            rusqlite::params![email],
            |r| r.get(0),
        )
        .unwrap();
    assert!(verified_at.is_some());
}

#[tokio::test]
#[serial]
async fn trial_start_requires_and_records_terms_acceptance() {
    let h = boot_harness().await;

    let req = Request::post("/auth/trial/start")
        .header("content-type", "application/json")
        .header("user-agent", "bluey-e2e-trial")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "device_fingerprint": "trial-device-terms-missing"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["error"].as_str(), Some("terms_required"));

    let auth = start_trial(&h, "trial-device-terms-accepted").await;
    let account_id = auth["account"]["id"].as_str().unwrap();
    let conn = h.pool.get().unwrap();
    let row: (i64, String, String, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(MIN(terms_text_hash), ''),
                    COALESCE(MIN(privacy_text_hash), ''),
                    SUM(CASE WHEN email_hash IS NOT NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN user_agent_hash IS NOT NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN retention_expires_at IS NOT NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN accepted_at IS NOT NULL THEN 1 ELSE 0 END)
               FROM legal_acceptances
              WHERE account_id = ?1
                AND purpose = 'trial_terms_privacy'
                AND terms_version = '2026-07-09'
                AND privacy_version = '2026-07-09'",
            rusqlite::params![account_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row.0, 1);
    assert!(row.1.starts_with("sha256:"));
    assert!(row.2.starts_with("sha256:"));
    assert_eq!(row.3, 1);
    assert_eq!(row.4, 1);
    assert_eq!(row.5, 1);
    assert_eq!(row.6, 1);
}

#[tokio::test]
#[serial]
async fn signup_after_account_delete_reuses_email_without_new_trial() {
    let h = boot_harness().await;

    let email = "delete-resignup@bluey.sh";
    let auth = signup_with_otp(&h, email, "longenoughpw").await;
    assert_eq!(auth["account"]["trial_seconds_remaining"], 900);
    let access = auth["access_token"].as_str().unwrap();

    let req = Request::post("/account/delete")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_none());

    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header("Authorization", "Bearer test-resend-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"email-otp-2"})))
        .mount(&h.mail)
        .await;

    let start = Request::post("/auth/signup/start")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": "longenoughpw",
                "terms_accepted": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(start).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let start_body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(start_body["trial_seconds"], 0);
    assert_eq!(
        start_body["no_trial_reason"].as_str(),
        Some("email_trial_already_used")
    );

    let requests = h.mail.received_requests().await.unwrap();
    let mail_body: serde_json::Value =
        serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    let code = extract_six_digit_code(mail_body["text"].as_str().unwrap()).unwrap();

    let confirm = Request::post("/auth/signup/confirm")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "otp": code
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(confirm).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let recreated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(recreated["account"]["email"], email);
    assert_eq!(recreated["account"]["trial_seconds_remaining"], 0);

    let conn = h.pool.get().unwrap();
    let (count, trial_seconds): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(trial_seconds_remaining), 0)
               FROM accounts
              WHERE email = ?1",
            rusqlite::params![email],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(trial_seconds, 0);
}

#[tokio::test]
#[serial]
async fn trial_start_creates_temporary_account_with_fifteen_minutes() {
    let h = boot_harness().await;
    let auth = start_trial(&h, "trial-device-create").await;
    assert_eq!(auth["account"]["is_temporary"], true);
    assert_eq!(auth["account"]["trial_seconds_remaining"], 900);
    assert!(auth["password"].as_str().unwrap().len() >= 12);

    let access = auth["access_token"].as_str().unwrap();
    let req = Request::get("/account/me")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let me: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(me["is_temporary"], true);
    assert_eq!(me["trial_seconds_remaining"], 900);

    let account_id = auth["account"]["id"].as_str().unwrap();
    let conn = h.pool.get().unwrap();
    let (is_temporary, trial_seconds): (i64, i64) = conn
        .query_row(
            "SELECT is_temporary, trial_seconds_remaining FROM accounts WHERE id = ?1",
            rusqlite::params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(is_temporary, 1);
    assert_eq!(trial_seconds, 900);

    let req = Request::post("/auth/trial/start")
        .header("content-type", "application/json")
        .header("user-agent", "bluey-e2e-trial")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "device_fingerprint": "trial-device-create",
                "terms_accepted": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
#[serial]
async fn expired_temporary_account_cannot_access_or_refresh() {
    let h = boot_harness().await;
    let auth = start_trial(&h, "trial-device-expired").await;
    let account_id = auth["account"]["id"].as_str().unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts SET temporary_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
            rusqlite::params![account_id],
        )
        .unwrap();

    let access = auth["access_token"].as_str().unwrap();
    let req = Request::get("/account/me")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let refresh = auth["refresh_token"].as_str().unwrap();
    let req = Request::post("/auth/refresh")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "refresh_token": refresh })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn temporary_trial_converts_to_verified_account() {
    let h = boot_harness().await;
    let auth = start_trial(&h, "trial-device-convert").await;
    let access = auth["access_token"].as_str().unwrap();
    let account_id = auth["account"]["id"].as_str().unwrap().to_string();

    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header("Authorization", "Bearer test-resend-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"trial-convert"})))
        .mount(&h.mail)
        .await;

    let email = "saved-trial@example.com";
    let req = Request::post("/auth/trial/convert/start")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": "longenoughpw",
                "terms_accepted": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let requests = h.mail.received_requests().await.unwrap();
    let mail_body: serde_json::Value =
        serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    let code = extract_six_digit_code(mail_body["text"].as_str().unwrap()).unwrap();

    let req = Request::post("/auth/trial/convert/confirm")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "otp": code
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let converted: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(converted["account"]["email"], email);
    assert_eq!(converted["account"]["is_temporary"], false);

    let conn = h.pool.get().unwrap();
    let (stored_email, is_temporary, verified_at): (String, i64, Option<String>) = conn
        .query_row(
            "SELECT email, is_temporary, email_verified_at FROM accounts WHERE id = ?1",
            rusqlite::params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored_email, email);
    assert_eq!(is_temporary, 0);
    assert!(verified_at.is_some());
}

#[tokio::test]
#[serial]
async fn signup_start_requires_turnstile_when_flagged() {
    let h = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.require_turnstile = true;
        config.turnstile_site_key = Some("test-site-key".to_string());
        config.turnstile_secret_key = None;
    })
    .await;

    let req = Request::post("/auth/signup/start")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": "captcha-required@example.com",
                "password": "longenoughpw",
                "terms_accepted": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        parsed["error"].as_str(),
        Some("captcha verification is required but not configured")
    );
}

#[tokio::test]
#[serial]
async fn configured_admin_email_signup_gets_admin_access() {
    let h = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec!["owner@bluey.sh".to_string()],
    )
    .await;

    let auth = signup_with_otp(&h, " Owner@Bluey.SH ", "longenoughpw").await;
    let access = auth["access_token"].as_str().unwrap();
    assert_admin_customers_allowed(&h, access).await;

    let req = Request::get("/account/me")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let me: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(me["email"], "owner@bluey.sh");
    assert_eq!(me["is_admin"], true);
}

#[tokio::test]
#[serial]
async fn legacy_signup_endpoint_is_retired() {
    let h = boot_harness().await;

    let req = Request::post("/auth/signup")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": "legacy-signup@bluey.sh",
                "password": "longenoughpw"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
#[serial]
async fn configured_admin_email_login_promotes_existing_account() {
    let h = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec!["late-admin@bluey.sh".to_string()],
    )
    .await;

    let password_hash = auth::password::hash_password("longenoughpw").unwrap();
    let account = Account::create(&h.pool, "late-admin@bluey.sh", &password_hash).unwrap();
    assert!(!account.is_admin);

    let auth = login(&h, "late-admin@bluey.sh", "longenoughpw").await;
    assert_eq!(auth["account"]["is_admin"], true);
    let access = auth["access_token"].as_str().unwrap();
    assert_admin_customers_allowed(&h, access).await;
}

#[tokio::test]
#[serial]
async fn router_complete_happy_path_with_mocked_openai() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "happy@example.com", "longenoughpw").await;

    // Mock OpenAI Chat Completions.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "Hello back!"}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 4}
        })))
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "test-req-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "hello",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
#[serial]
async fn router_embed_consumes_trial_seconds_and_records_bluey_cost() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "trial-embed@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "embedding": [0.1, 0.2, 0.3] },
                { "embedding": [0.4, 0.5, 0.6] }
            ],
            "usage": { "prompt_tokens": 1500 }
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/embed/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "trial-embed-1",
                "inputs": [
                    "resume summary chunk",
                    "interview transcript chunk"
                ]
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let embed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(embed["cost_cents"], 0);
    assert_eq!(embed["input_tokens"], 1500);

    let conn = h.pool.get().unwrap();
    let (trial_remaining, root_bluey_cost, customer_cost): (i64, i64, i64) = conn
        .query_row(
            "SELECT a.trial_seconds_remaining,
                    u.cost_cents_to_bluey,
                    u.cost_cents_to_customer
               FROM accounts a
               JOIN usage_events u ON u.account_id = a.id
              WHERE a.email = ?1 AND u.request_id = ?2 AND u.kind = 'embed'",
            rusqlite::params!["trial-embed@example.com", "trial-embed-1"],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(trial_remaining, 898);
    assert_eq!(root_bluey_cost, 0);
    assert_eq!(customer_cost, 0);
    let attempt_bluey_cost: i64 = conn
        .query_row(
            "SELECT cost_cents_to_bluey FROM usage_events
              WHERE account_id = (SELECT id FROM accounts WHERE email = ?1)
                AND request_id = 'trial-embed-1:embed-attempt:0'
                AND kind = 'embed_attempt'",
            rusqlite::params!["trial-embed@example.com"],
            |row| row.get(0),
        )
        .unwrap();
    let attempt_provenance: String = conn
        .query_row(
            "SELECT usage_provenance FROM jobs_provider_cost_holds",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let expected_attempt_bluey_cost =
        bluey_server::pricing::lookup("openai", "text-embedding-3-small")
            .map(|pricing| bluey_server::pricing::compute_cost(pricing, 1_500, 0).0)
            .unwrap();
    assert_eq!(attempt_bluey_cost, expected_attempt_bluey_cost);
    assert_eq!(attempt_provenance, "exact");
}

#[tokio::test]
#[serial]
async fn router_complete_rejects_billing_restricted_account() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "restricted-complete@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["restricted-complete@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    Account::restrict_billing(
        &h.pool,
        &account_id,
        "charge.dispute.created",
        Some("evt_restricted_complete"),
    )
    .unwrap();

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "test-restricted-complete",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "hello",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"].as_str().unwrap().contains("paused"));
}

#[tokio::test]
#[serial]
async fn router_complete_upstream_spend_guard_blocks_before_provider_hit() {
    let h = boot_harness_with_options(
        UpstreamKeys {
            openai_api_key: Some("sk-test-openai".to_string()),
            anthropic_api_key: Some("sk-test-anthropic".to_string()),
            gemini_api_key: None,
            deepseek_api_key: None,
            zai_api_key: None,
            deepgram_api_key: Some("dg-test".to_string()),
            ollama_base_url: None,
        },
        vec![],
        Some(UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        }),
    )
    .await;
    let access = signup_and_login(&h, "budget@example.com", "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, "budget@example.com")
        .unwrap()
        .unwrap();
    usage::record(&h.pool, &account.id, &sample_usage("spent", 10)).unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "should not happen"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .expect(0)
        .mount(&h.openai)
        .await;

    for (path, request_id) in [
        ("/router/complete", "budget-guard"),
        ("/router/complete/stream", "budget-guard-stream"),
    ] {
        let req = Request::post(path)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": request_id,
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "hello",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["reason"], "upstream_spend_guard", "{path}");
        assert_eq!(v["retry_after_secs"], 60, "{path}");
    }
}

#[tokio::test]
#[serial]
async fn router_complete_idempotency_replay_returns_cached() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "idem@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "First call"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2}
        })))
        .expect(1) // critical: only ONE upstream hit even on retry.
        .mount(&h.openai)
        .await;

    for _ in 0..2 {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": "test-idem-1",
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "hi",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }
}

#[tokio::test]
#[serial]
async fn router_complete_accepts_exact_legacy_signed_release_contract() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "legacy-contract@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "Legacy client still works"}}],
            "usage": {"prompt_tokens": 8, "completion_tokens": 4}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "legacy-managed-contract-1",
                "system": LEGACY_MANAGED_PROVIDER_BASE_CONTRACT_V0_1_99_TO_101,
                "user": "Give me a concise status update.",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn router_complete_stream_proxies_openai_deltas_then_billing() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stream-openai@example.com", "longenoughpw").await;

    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"stream\"}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":4}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    let first_delta = body.find("Hello ").expect("missing first upstream delta");
    let second_delta = body.find("stream").expect("missing second upstream delta");
    let billing = body.find("event: billing").expect("missing billing event");
    assert!(first_delta < billing, "delta must arrive before billing");
    assert!(second_delta < billing, "delta must arrive before billing");
    assert!(body.contains("\"text\":\"Hello stream\""));
    assert!(body.contains("\"provider\":\"openai\""));
    assert!(body.contains("\"model\":\"gpt-5.4-mini\""));
    assert!(body.contains("\"input_tokens\":12"));
    assert!(body.contains("\"output_tokens\":4"));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
#[serial]
async fn router_complete_stream_releases_multiple_guarded_deltas_losslessly() {
    let h = boot_harness().await;
    let access = signup_and_login(
        &h,
        "stream-openai-multiple-deltas@example.com",
        "longenoughpw",
    )
    .await;

    let first = "Start with a clear API contract, explicit ownership, durable state, bounded retries, idempotency, structured logs, metrics, traces, dashboards, alerts, and a tested rollback path. ";
    let second = "Then canary the worker, verify latency and error budgets, reconcile every uncertain outcome, and keep the previous release ready until production evidence is stable. ";
    let third =
        "Finally, document the failure modes and practice recovery before increasing traffic.";
    let stream = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n\
         data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n\
         data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n\
         data: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":12,\"completion_tokens\":80}}}}\n\n\
         data: [DONE]\n\n",
        serde_json::to_string(first).unwrap(),
        serde_json::to_string(second).unwrap(),
        serde_json::to_string(third).unwrap(),
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        body.matches("data: {\"choices\"").count() >= 2,
        "guarded streaming collapsed back to one full-answer delta: {body}"
    );
    assert!(body.contains("Start with a clear API contract"));
    assert!(body.contains("Finally, document the failure modes"));
    assert!(body.contains("event: billing"));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
#[serial]
async fn router_complete_stream_openai_error_frame_is_retryable() {
    let h = boot_harness().await;
    let email = "stream-openai-error@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;

    let stream = concat!(
        "data: {\"error\":{\"message\":\"provider overloaded\",\"type\":\"rate_limit_error\"}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-error-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 502);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "upstream provider error; please retry");
    assert_eq!(body["reason"], "upstream_error");

    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    let replay = idempotency::reserve(&h.pool, &account.id, "stream-openai-error-1").unwrap();
    assert_eq!(replay, idempotency::ReserveOutcome::FreshReservation);
}

#[tokio::test]
#[serial]
async fn router_complete_stream_falls_back_after_pre_output_provider_error() {
    std::env::set_var("BLUEY_ROUTE_POLICY", "quality_first");
    let h = boot_harness().await;
    let access =
        signup_and_login(&h, "stream-pre-output-fallback@example.com", "longenoughpw").await;

    let anthropic_stream = concat!(
        "event: error\n",
        "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"provider overloaded\"}}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(anthropic_stream),
        )
        .expect(1)
        .mount(&h.anthropic)
        .await;

    let openai_stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Fallback answer\"}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":3}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-pre-output-fallback-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer reliably",
                "lane": "balanced"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    assert!(body.contains("Fallback answer"));
    assert!(body.contains("\"provider\":\"openai\""));
    assert!(body.contains("event: billing"));
    assert!(!body.contains("event: error"));
    std::env::remove_var("BLUEY_ROUTE_POLICY");
}

#[tokio::test]
#[serial]
async fn router_complete_stream_openai_truncated_after_delta_is_not_billed_or_released() {
    let h = boot_harness().await;
    let email = "stream-openai-truncated@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;

    let stream = "data: {\"choices\":[{\"delta\":{\"content\":\"Partial answer\"}}]}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-truncated-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("Partial answer"));
    assert!(body.contains("event: error"));
    assert!(body.contains("upstream_stream_error"));
    assert!(!body.contains("event: billing"));

    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    let replay = idempotency::reserve(&h.pool, &account.id, "stream-openai-truncated-1").unwrap();
    assert_eq!(replay, idempotency::ReserveOutcome::CachedFailed);
}

#[tokio::test]
#[serial]
async fn router_complete_stream_openai_length_finish_reports_output_truncated() {
    let h = boot_harness().await;
    let email = "stream-openai-length@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;

    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Partial answer that reached the configured output budget\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-length-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer quickly",
                "lane": "instant",
                "max_tokens": 64
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("Partial answer"));
    assert!(body.contains("event: error"));
    assert!(body.contains("upstream_output_truncated"));
    assert!(!body.contains("event: billing"));

    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    let replay = idempotency::reserve(&h.pool, &account.id, "stream-openai-length-1").unwrap();
    assert_eq!(replay, idempotency::ReserveOutcome::CachedFailed);
}

#[tokio::test]
#[serial]
async fn router_complete_stream_idempotency_replays_cached_stream_without_upstream() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stream-idem@example.com", "longenoughpw").await;

    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Cached \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"stream\"}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":3}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    for _ in 0..2 {
        let req = Request::post("/router/complete/stream")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": "stream-idem-1",
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "answer quickly",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Cached "));
        assert!(body.contains("stream"));
        assert!(body.contains("event: billing"));
        assert!(body.contains("\"text\":\"Cached stream\""));
        assert!(body.contains("data: [DONE]"));
    }
}

#[tokio::test]
#[serial]
async fn router_complete_stream_proxies_anthropic_messages_sse() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stream-anthropic@example.com", "longenoughpw").await;

    let stream = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":20,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Deep \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"answer\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":6}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.anthropic)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-anthropic-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer a normal technical question",
                "lane": "balanced"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    let first_delta = body.find("Deep ").expect("missing first upstream delta");
    let second_delta = body.find("answer").expect("missing second upstream delta");
    let billing = body.find("event: billing").expect("missing billing event");
    assert!(first_delta < billing, "delta must arrive before billing");
    assert!(second_delta < billing, "delta must arrive before billing");
    assert!(body.contains("\"text\":\"Deep answer\""));
    assert!(body.contains("\"provider\":\"anthropic\""));
    assert!(body.contains("\"model\":\"claude-sonnet-4-6\""));
    assert!(body.contains("\"input_tokens\":20"));
    assert!(body.contains("\"output_tokens\":6"));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
#[serial]
async fn router_complete_stream_anthropic_truncated_after_delta_is_not_billed_or_released() {
    let h = boot_harness().await;
    let email = "stream-anthropic-truncated@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;

    let stream = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":20,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Partial deep answer\"}}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.anthropic)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-anthropic-truncated-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer a normal technical question",
                "lane": "balanced"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("Partial deep answer"));
    assert!(body.contains("event: error"));
    assert!(body.contains("upstream_stream_error"));
    assert!(!body.contains("event: billing"));

    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    let replay =
        idempotency::reserve(&h.pool, &account.id, "stream-anthropic-truncated-1").unwrap();
    assert_eq!(replay, idempotency::ReserveOutcome::CachedFailed);
}

#[tokio::test]
#[serial]
async fn router_complete_falls_back_when_preferred_provider_429s() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "fallback@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .expect(1)
        .mount(&h.anthropic)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "fallback answer"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "fallback-429-1",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer a normal technical question",
                "lane": "balanced"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "fallback answer");
    assert_eq!(v["provider"], "openai");
    assert_eq!(v["model"], "gpt-5.5");
}

#[tokio::test]
#[serial]
async fn router_complete_retries_next_openai_key_on_429_without_customer_wait() {
    let upstream = UpstreamKeys {
        openai_api_key: Some("sk-openai-a,sk-openai-b".to_string()),
        anthropic_api_key: Some("sk-test-anthropic".to_string()),
        gemini_api_key: None,
        deepseek_api_key: None,
        zai_api_key: None,
        deepgram_api_key: Some("dg-test".to_string()),
        ollama_base_url: None,
    };
    let request_id = "openai-keypool-429-1";
    let ordered_keys =
        upstream.key_candidates("openai", &format!("llm:{request_id}:openai:gpt-5.4-mini"));
    assert_eq!(ordered_keys.len(), 2);

    let h = boot_harness_with_upstream(upstream).await;
    let access = signup_and_login(&h, "keypool@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            format!("Bearer {}", ordered_keys[0].secret),
        ))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "45")
                .set_body_string("rate limited"),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            format!("Bearer {}", ordered_keys[1].secret),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "second key answered"}}],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": request_id,
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "second key answered");
    assert_eq!(v["provider"], "openai");
    assert_eq!(v["model"], "gpt-5.4-mini");
}

#[tokio::test]
#[serial]
async fn router_complete_short_waits_account_llm_burst_guard() {
    std::env::set_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN", "60");
    std::env::set_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN_BURST", "1");
    let h = boot_harness_with_upstream(UpstreamKeys {
        openai_api_key: None,
        anthropic_api_key: Some("sk-test-anthropic".to_string()),
        gemini_api_key: None,
        deepseek_api_key: None,
        zai_api_key: None,
        deepgram_api_key: Some("dg-test".to_string()),
        ollama_base_url: None,
    })
    .await;
    std::env::remove_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN_BURST");
    let access = signup_and_login(&h, "account-burst-shortwait@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "burst guard answered"}],
            "usage": {"input_tokens": 10, "output_tokens": 4}
        })))
        .expect(2)
        .mount(&h.anthropic)
        .await;

    for request_id in ["account-burst-shortwait-1", "account-burst-shortwait-2"] {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": request_id,
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "answer a normal technical question",
                    "lane": "balanced"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["text"], "burst guard answered");
        assert_eq!(v["provider"], "anthropic");
    }
}

#[tokio::test]
#[serial]
async fn router_complete_reports_upstream_error_after_capacity_skip() {
    std::env::set_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN", "60");
    std::env::set_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN_BURST", "1");
    let h = boot_harness().await;
    std::env::remove_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN_BURST");
    let access = signup_and_login(&h, "fallback-capacity@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "first anthropic ok"}],
            "usage": {"input_tokens": 10, "output_tokens": 4}
        })))
        .expect(1)
        .mount(&h.anthropic)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("temporary openai failure"))
        .expect(2)
        .mount(&h.openai)
        .await;

    for (request_id, expected_status) in [("capacity-skip-1", 200), ("capacity-skip-2", 502)] {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": request_id,
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "answer a normal technical question",
                    "lane": "balanced"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), expected_status);
        if expected_status == 502 {
            let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["error"], "upstream provider error; please retry");
            assert!(v.get("retry_after_secs").is_none());
        }
    }
}

#[tokio::test]
#[serial]
async fn router_complete_enforces_account_burst_before_second_upstream_hit() {
    std::env::set_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN", "60");
    std::env::set_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN_BURST", "1");
    std::env::set_var("BLUEY_CAPACITY_SHORT_WAIT_MAX_SECS", "0");
    let h = boot_harness().await;
    std::env::remove_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN_BURST");

    let access = signup_and_login(&h, "burst@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "first ok"}}],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    for (idx, expected_status) in [(1, 200), (2, 429)] {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": format!("burst-limit-{idx}"),
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "hi",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), expected_status);
        if idx == 2 {
            let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["reason"], "account_llm_busy");
            assert!(v["retry_after_secs"].as_u64().unwrap_or(0) >= 1);
        }
    }
    std::env::remove_var("BLUEY_CAPACITY_SHORT_WAIT_MAX_SECS");
}

#[tokio::test]
#[serial]
async fn billing_checkout_uses_mocked_stripe() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stripe@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "cs_test_123",
            "url": "https://checkout.stripe.test/session/cs_test_123"
        })))
        .expect(1)
        .mount(&h.stripe)
        .await;

    let req = Request::post("/billing/checkout")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({ "amount_cents": 3000 })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v["checkout_url"],
        "https://checkout.stripe.test/session/cs_test_123"
    );
}

#[tokio::test]
#[serial]
async fn auth_link_mint_then_exchange_roundtrip() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "link@example.com", "longenoughpw").await;

    let req = Request::post("/auth/link/mint")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from("{}".as_bytes().to_vec()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let code = v["link_code"].as_str().unwrap().to_string();
    assert!(v["deep_link_url"]
        .as_str()
        .unwrap()
        .starts_with("bluey://link?code="));

    let req = Request::post("/auth/link/exchange")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"code": code})).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
#[serial]
async fn auth_device_poll_is_single_use_after_approval() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "device-once@example.com", "longenoughpw").await;

    let req = Request::post("/auth/device/start")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let started: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        started["verification_uri"].as_str().unwrap(),
        "http://localhost:8080/login"
    );
    let device_code = started["device_code"].as_str().unwrap();
    let user_code = started["user_code"].as_str().unwrap();

    let req = Request::post("/auth/device/approve")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "user_code": user_code })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let poll_body = serde_json::to_vec(&json!({ "device_code": device_code })).unwrap();
    let req = Request::post("/auth/device/poll")
        .header("content-type", "application/json")
        .body(Body::from(poll_body.clone()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::post("/auth/device/poll")
        .header("content-type", "application/json")
        .body(Body::from(poll_body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn auth_device_approve_cannot_overwrite_approved_code() {
    let h = boot_harness().await;
    let first_access = signup_and_login(&h, "device-owner@example.com", "longenoughpw").await;
    let second_access = signup_and_login(&h, "device-attacker@example.com", "longenoughpw").await;

    let req = Request::post("/auth/device/start")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let started: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let device_code = started["device_code"].as_str().unwrap();
    let user_code = started["user_code"].as_str().unwrap();

    let approve_body = serde_json::to_vec(&json!({ "user_code": user_code })).unwrap();
    let req = Request::post("/auth/device/approve")
        .header("authorization", format!("Bearer {first_access}"))
        .header("content-type", "application/json")
        .body(Body::from(approve_body.clone()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::post("/auth/device/approve")
        .header("authorization", format!("Bearer {second_access}"))
        .header("content-type", "application/json")
        .body(Body::from(approve_body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = Request::post("/auth/device/poll")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "device_code": device_code })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let auth: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(auth["account"]["email"], "device-owner@example.com");
}

#[tokio::test]
#[serial]
async fn account_delete_requires_typed_delete_and_credit_loss_consent() {
    let h = boot_harness().await;
    let email = "delete-confirm@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;

    let req = Request::post("/account/delete")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_some());

    let req = Request::post("/account/delete")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": false
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_some());

    let req = Request::post("/account/delete")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_none());
}

#[tokio::test]
#[serial]
async fn sync_batch_session_bundle_and_rag_roundtrip() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "sync@example.com", "longenoughpw").await;
    let session_id = "550e8400-e29b-41d4-a716-446655440001";

    let batch = json!({
        "sessions": [{
            "session_id": session_id,
            "title": "Cloud sync test",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000,
            "last_active_at_ms": 2000,
            "answer_style": "be concise",
            "metadata": {"source": "test"}
        }],
        "transcript_segments": [{
            "segment_id": "seg-cloud-1",
            "session_id": session_id,
            "speaker": "system",
            "source": "system",
            "text": "We discussed queue backpressure and cache stampede controls.",
            "ts_ms": 1500,
            "is_final": true
        }],
        "cue_responses": [{
            "response_id": "resp-cloud-1",
            "session_id": session_id,
            "kind": "answer",
            "text": "Use bounded queues, retries, and admission control.",
            "ts_ms": 1600,
            "provider": "bluey-managed-instant",
            "model": "gpt-5.4-mini",
            "cost_label": "$0.01 · balance $29.99"
        }],
        "context_artifacts": [{
            "artifact_id": "ctx-cloud-1",
            "session_id": session_id,
            "kind": "document",
            "title": "Architecture brief",
            "text_preview": "The architecture uses bounded queues.",
            "created_at_ms": 1400
        }],
        "rag_chunks": [{
            "chunk_id": "chunk-cloud-1",
            "session_id": session_id,
            "source_kind": "transcript",
            "source_id": "seg-cloud-1",
            "chunk_index": 0,
            "text": "queue backpressure cache stampede",
            "updated_at_ms": 1500
        }]
    });

    let req = Request::post("/sync/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(serde_json::to_vec(&batch).unwrap()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        200,
        "sync batch rejected: {}",
        String::from_utf8_lossy(&body)
    );

    let req = Request::get("/sync/sessions")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(listed["sessions"][0]["session_id"], session_id);

    let req = Request::get(format!("/sync/sessions/{session_id}"))
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let bundle: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        bundle["transcript_segments"][0]["segment_id"],
        "seg-cloud-1"
    );
    assert_eq!(
        bundle["cue_responses"][0]["cost_label"],
        "$0.01 · balance $29.99"
    );

    let req = Request::post("/rag/query")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "query": "cache stampede",
                "top_k": 3
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let rag: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(rag["matches"][0]["chunk_id"], "chunk-cloud-1");
}

#[tokio::test]
#[serial]
async fn account_export_zip_contains_readable_bundle() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "export-zip@example.com", "longenoughpw").await;

    let batch = json!({
        "sessions": [{
            "session_id": "sess-export-1",
            "title": "Export test",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000,
            "last_active_at_ms": 2000,
            "answer_style": "be concise"
        }],
        "transcript_segments": [{
            "segment_id": "seg-export-1",
            "session_id": "sess-export-1",
            "speaker": "system",
            "source": "system",
            "text": "Secret transcript only for account export.",
            "ts_ms": 1500,
            "is_final": true
        }],
        "cue_responses": [{
            "response_id": "resp-export-1",
            "session_id": "sess-export-1",
            "kind": "answer",
            "text": "Export answer text.",
            "ts_ms": 1600,
            "provider": "bluey-managed-instant",
            "model": "gpt-5.4-mini"
        }],
        "context_artifacts": [{
            "artifact_id": "ctx-export-1",
            "session_id": "sess-export-1",
            "kind": "document",
            "title": "Export brief",
            "text_preview": "Export preview.",
            "created_at_ms": 1400
        }]
    });
    let req = Request::post("/sync/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(serde_json::to_vec(&batch).unwrap()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::get("/account/export?format=zip&include_objects=false")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let body = axum::body::to_bytes(resp.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(body.to_vec())).unwrap();
    assert!(archive.by_name("account-export.json").is_ok());
    assert!(archive.by_name("manifest.json").is_ok());

    let mut transcript = String::new();
    archive
        .by_name("sessions/transcript.md")
        .unwrap()
        .read_to_string(&mut transcript)
        .unwrap();
    assert!(transcript.contains("Secret transcript only for account export."));

    let mut answers = String::new();
    archive
        .by_name("sessions/answers.md")
        .unwrap()
        .read_to_string(&mut answers)
        .unwrap();
    assert!(answers.contains("Export answer text."));
    let event_count: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM ops_audit_events
             WHERE event_type = 'account.export' AND status = 'completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(event_count, 1);
}

#[tokio::test]
#[serial]
async fn legacy_artifact_upload_derives_only_an_existing_live_parent() {
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let h = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint,
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    let email = "legacy-object-parent@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let session_id = uuid::Uuid::new_v4().to_string();
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let bytes = b"legacy artifact bytes";
    let hash = bluey_server::object_storage::sha256_hex(bytes);
    let object_key = format!(
        "bluey-cloud/accounts/{}/context/{artifact_id}/sha256/{hash}",
        account.id
    );
    h.pool
        .get()
        .unwrap()
        .execute(
            "INSERT INTO cloud_sessions (
                account_id, session_id, title, status, created_at_ms, updated_at_ms,
                metadata_json
             ) VALUES (?1, ?2, 'Legacy parent', 'active', 1, 1, '{}')",
            rusqlite::params![account.id, session_id],
        )
        .unwrap();

    let legacy_upload = || {
        Request::post(format!("/sync/artifacts/{artifact_id}/object"))
            .header("authorization", format!("Bearer {access}"))
            .header("content-type", "text/plain")
            .body(Body::from(bytes.as_slice()))
            .unwrap()
    };
    let response = h.router.clone().oneshot(legacy_upload()).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "missing-header clients must sync metadata before bytes"
    );
    let staged_count: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM object_uploads
              WHERE account_id = ?1 AND logical_id = ?2",
            rusqlite::params![account.id, artifact_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(staged_count, 0, "unbound bytes must never be staged");

    h.pool
        .get()
        .unwrap()
        .execute(
            "INSERT INTO cloud_context_artifacts (
                account_id, artifact_id, session_id, kind, title,
                created_at_ms, updated_at_ms, metadata_json
             ) VALUES (?1, ?2, ?3, 'document', 'Legacy artifact', 1, 1, '{}')",
            rusqlite::params![account.id, artifact_id, session_id],
        )
        .unwrap();
    Mock::given(method("PUT"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;

    let response = h.router.clone().oneshot(legacy_upload()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let stored_parent: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT session_id FROM object_uploads
              WHERE account_id = ?1 AND logical_id = ?2",
            rusqlite::params![account.id, artifact_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_parent, session_id);
}

#[tokio::test]
#[serial]
async fn artifact_upload_retries_from_durable_outbox_and_is_idempotent() {
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let h = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint,
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    let email = "durable-object-upload@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let session_id = uuid::Uuid::new_v4().to_string();
    let other_session_id = uuid::Uuid::new_v4().to_string();
    {
        let conn = h.pool.get().unwrap();
        for (id, title) in [
            (&session_id, "Artifact parent"),
            (&other_session_id, "Other parent"),
        ] {
            conn.execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    metadata_json
                 ) VALUES (?1, ?2, ?3, 'active', 1, 1, '{}')",
                rusqlite::params![account.id, id, title],
            )
            .unwrap();
        }
    }
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let bytes = b"stable artifact bytes";
    let hash = bluey_server::object_storage::sha256_hex(bytes);
    let object_key = format!(
        "bluey-cloud/accounts/{}/context/{artifact_id}/sha256/{hash}",
        account.id
    );

    Mock::given(method("PUT"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&object_store)
        .await;

    let upload = || {
        Request::post(format!("/sync/artifacts/{artifact_id}/object"))
            .header("authorization", format!("Bearer {access}"))
            .header("content-type", "text/plain")
            .header("x-bluey-session-id", &session_id)
            .body(Body::from(bytes.as_slice()))
            .unwrap()
    };
    let resp = h.router.clone().oneshot(upload()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    let durable_state: (String, String, i64) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT u.state, o.state, o.attempt_count
               FROM object_uploads u
               JOIN object_storage_outbox o ON o.upload_id = u.id AND o.operation = 'put'
              WHERE u.account_id = ?1 AND u.logical_id = ?2",
            rusqlite::params![account.id, artifact_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(durable_state, ("pending".into(), "retry".into(), 1));

    object_store.reset().await;
    Mock::given(method("PUT"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    let resp = h.router.clone().oneshot(upload()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = h.router.clone().oneshot(upload()).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "ready retry must skip R2 PUT"
    );

    let conflicting = Request::post(format!("/sync/artifacts/{artifact_id}/object"))
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "text/plain")
        .header("x-bluey-session-id", &session_id)
        .body(Body::from("different bytes"))
        .unwrap();
    let resp = h.router.clone().oneshot(conflicting).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let conflicting_parent = Request::post(format!("/sync/artifacts/{artifact_id}/object"))
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "text/plain")
        .header("x-bluey-session-id", &other_session_id)
        .body(Body::from(bytes.as_slice()))
        .unwrap();
    let resp = h.router.clone().oneshot(conflicting_parent).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "an idempotent object id cannot change parent sessions"
    );

    let final_state: (String, String, i64, i64) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT u.state, o.state, d.reserved_bytes, d.reserved_objects
               FROM object_uploads u
               JOIN object_storage_outbox o ON o.upload_id = u.id AND o.operation = 'put'
               JOIN object_upload_daily_usage d ON d.account_id = u.account_id
              WHERE u.account_id = ?1 AND u.logical_id = ?2",
            rusqlite::params![account.id, artifact_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        final_state,
        ("ready".into(), "completed".into(), bytes.len() as i64, 1)
    );

    object_store.reset().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;
    let req = Request::post("/account/delete")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_none());
}

#[tokio::test]
#[serial]
async fn audit_upload_requires_owned_session_and_live_billing() {
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let h = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.log_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint,
            bucket: "logs".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "prod/logs".to_string(),
            retention_days: 180,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    let owner_access = signup_and_login(&h, "audit-owner@example.com", "longenoughpw").await;
    let other_access = signup_and_login(&h, "audit-other@example.com", "longenoughpw").await;
    let owner = Account::fetch_by_email(&h.pool, "audit-owner@example.com")
        .unwrap()
        .unwrap();
    let session_id = uuid::Uuid::new_v4().to_string();
    let batch = json!({
        "sessions": [{
            "session_id": session_id,
            "title": "Audit owner session",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000
        }],
        "cue_responses": [{
            "response_id": "audit-owner-answer",
            "session_id": session_id,
            "kind": "answer",
            "text": "owned answer",
            "ts_ms": 1500
        }]
    });
    let req = Request::post("/sync/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {owner_access}"))
        .body(Body::from(serde_json::to_vec(&batch).unwrap()))
        .unwrap();
    assert_eq!(
        h.router.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK
    );

    let bundle_id = "audit-owned-bundle";
    let bundle = br#"{"schema_version":1}"#;
    let hash = bluey_server::object_storage::sha256_hex(bundle);
    let key = format!(
        "prod/logs/accounts/{}/sessions/{session_id}/audit/{bundle_id}/sha256/{hash}.json",
        owner.id
    );
    Mock::given(method("PUT"))
        .and(path(format!("/logs/{key}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;

    let audit_request = |token: &str, bundle_id: &str| {
        Request::post(format!("/sync/session-audit/{session_id}/{bundle_id}"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(bundle.as_slice()))
            .unwrap()
    };
    let resp = h
        .router
        .clone()
        .oneshot(audit_request(&other_access, bundle_id))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = h
        .router
        .clone()
        .oneshot(audit_request(&owner_access, bundle_id))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let indexed: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM diagnostic_log_chunks
              WHERE account_id = ?1 AND session_id = ?2 AND object_key = ?3",
            rusqlite::params![owner.id, session_id, key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexed, 1);

    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
                SET billing_restricted = 1, billing_restriction_reason = 'charge.dispute.created'
              WHERE id = ?1",
            rusqlite::params![owner.id],
        )
        .unwrap();
    let resp = h
        .router
        .clone()
        .oneshot(audit_request(&owner_access, "blocked-bundle"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial]
async fn delete_account_deletes_artifact_objects_before_account_rows() {
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let h = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint,
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    let email = "delete-objects@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let object_key = format!(
        "bluey-cloud/accounts/{}/context/ctx-object-delete-1",
        account.id
    );

    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;

    let batch = json!({
        "sessions": [{
            "session_id": "sess-delete-1",
            "title": "Delete object test",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000,
            "last_active_at_ms": 2000,
            "answer_style": "be concise"
        }],
        "context_artifacts": [{
            "artifact_id": "ctx-object-delete-1",
            "session_id": "sess-delete-1",
            "kind": "document",
            "title": "Delete me",
            "text_preview": "private bytes",
            "created_at_ms": 1400,
            "metadata": {
                "object_key": object_key,
                "object_size_bytes": 12,
                "object_content_type": "text/plain",
                "object_sha256": "abc"
            }
        }]
    });
    let req = Request::post("/sync/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(serde_json::to_vec(&batch).unwrap()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::post("/account/delete")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let ack: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(ack["object_count_deleted"], 1);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_none());
    let event_count: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM ops_audit_events
             WHERE event_type = 'account.delete' AND status = 'completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(event_count, 1);
}

#[tokio::test]
#[serial]
async fn admin_support_bundle_is_redacted() {
    let h = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec!["admin-support@example.com".to_string()],
    )
    .await;
    let user_access = signup_and_login(&h, "support-user@example.com", "longenoughpw").await;
    let admin_access = signup_and_login(&h, "admin-support@example.com", "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, "support-user@example.com")
        .unwrap()
        .expect("support target account should exist");

    let batch = json!({
        "sessions": [{
            "session_id": "sess-support-1",
            "title": "Support secret title",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000,
            "last_active_at_ms": 2000,
            "answer_style": "be concise"
        }],
        "transcript_segments": [{
            "segment_id": "seg-support-1",
            "session_id": "sess-support-1",
            "speaker": "system",
            "source": "system",
            "text": "never leak this support transcript",
            "ts_ms": 1500,
            "is_final": true
        }],
        "cue_responses": [{
            "response_id": "resp-support-1",
            "session_id": "sess-support-1",
            "kind": "answer",
            "text": "never leak this support answer",
            "ts_ms": 1600,
            "provider": "bluey-managed-instant",
            "model": "gpt-5.4-mini"
        }]
    });
    let req = Request::post("/sync/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {user_access}"))
        .body(Body::from(serde_json::to_vec(&batch).unwrap()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::get(format!("/admin/support/accounts/{}", account.id))
        .header("authorization", format!("Bearer {admin_access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body_text.contains("never leak this support transcript"));
    assert!(!body_text.contains("never leak this support answer"));
    assert!(!body_text.contains("support-user@example.com"));
    let bundle: serde_json::Value = serde_json::from_str(&body_text).unwrap();
    assert_eq!(bundle["counts"]["transcript_segments"], 1);
    assert_eq!(bundle["counts"]["cue_responses"], 1);

    let req = Request::get("/admin/ops/events?limit=5")
        .header("authorization", format!("Bearer {admin_access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let events_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(events_text.contains("admin.support_bundle"));
    assert!(!events_text.contains("never leak this support transcript"));
}

#[tokio::test]
#[serial]
async fn router_transcribe_happy_path_with_mocked_deepgram() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stt@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/listen"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "metadata": { "duration": 1.4 },
            "results": {
                "channels": [
                    { "alternatives": [ { "transcript": "hello from deepgram" } ] }
                ]
            }
        })))
        .expect(1)
        .mount(&h.deepgram)
        .await;

    let req = Request::post("/router/transcribe?request_id=test-stt-1")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "audio/wav")
        .body(Body::from(pcm16_mono_wav(1)))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "hello from deepgram");
    assert_eq!(v["provider"], "deepgram");
    assert_eq!(v["model"], "nova-3");
    assert_eq!(v["duration_seconds"], 2);
}

#[tokio::test]
#[serial]
async fn router_transcribe_falls_back_to_openai_when_deepgram_fails() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stt-fallback@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/listen"))
        .respond_with(ResponseTemplate::new(503).set_body_string("deepgram busy"))
        .expect(1)
        .mount(&h.deepgram)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "text": "hello from openai fallback"
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/transcribe?request_id=test-stt-fallback-1")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "audio/wav")
        .body(Body::from(pcm16_mono_wav(1)))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "hello from openai fallback");
    assert_eq!(v["provider"], "openai");
    assert_eq!(v["model"], "gpt-4o-mini-transcribe");
    assert_eq!(v["duration_seconds"], 1);
}

#[tokio::test]
#[serial]
async fn router_transcribe_spend_guard_returns_typed_503_before_provider_hit() {
    let h = boot_harness_with_options(
        UpstreamKeys {
            openai_api_key: Some("sk-test-openai".to_string()),
            anthropic_api_key: Some("sk-test-anthropic".to_string()),
            gemini_api_key: None,
            deepseek_api_key: None,
            zai_api_key: None,
            deepgram_api_key: Some("dg-test".to_string()),
            ollama_base_url: None,
        },
        vec![],
        Some(UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        }),
    )
    .await;
    let access = signup_and_login(&h, "stt-budget@example.com", "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, "stt-budget@example.com")
        .unwrap()
        .unwrap();
    usage::record(&h.pool, &account.id, &sample_usage("stt-budget-spent", 10)).unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/listen"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&h.deepgram)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/transcribe?request_id=stt-budget-guard")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "audio/wav")
        .body(Body::from(pcm16_mono_wav(1)))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["reason"], "upstream_spend_guard");
    assert_eq!(v["retry_after_secs"], 60);

    let (trial_seconds, reservation_status): (i64, String) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT a.trial_seconds_remaining, r.status
               FROM accounts a
               JOIN usage_reservations r ON r.account_id = a.id
              WHERE a.id = ?1 AND r.request_id = 'stt-budget-guard'",
            rusqlite::params![account.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(trial_seconds, 900);
    assert_eq!(reservation_status, "released");
    assert!(matches!(
        idempotency::reserve(&h.pool, &account.id, "stt-budget-guard").unwrap(),
        idempotency::ReserveOutcome::FreshReservation
    ));
}

#[tokio::test]
#[serial]
async fn billing_portal_creates_session_via_mocked_stripe() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "portal@example.com", "longenoughpw").await;

    // Seed: customer must have stripe_customer_id (the portal endpoint
    // 400s if missing). We poke it directly into the DB to simulate a
    // prior successful Checkout.
    let conn = h.pool.get().unwrap();
    conn.execute(
        "UPDATE accounts SET stripe_customer_id = ?1 WHERE email = ?2",
        rusqlite::params!["cus_test_portal", "portal@example.com"],
    )
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/billing_portal/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "bps_test",
            "url": "https://billing.stripe.com/p/session/test_session_url"
        })))
        .expect(1)
        .mount(&h.stripe)
        .await;

    let req = Request::post("/billing/portal")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["portal_url"]
        .as_str()
        .unwrap()
        .starts_with("https://billing.stripe.com/"));
}

#[tokio::test]
#[serial]
async fn billing_checkout_creates_square_payment_link_when_square_enabled() {
    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");

    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-checkout@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v2/online-checkout/payment-links"))
        .and(header("Square-Version", "2025-04-16"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "payment_link": {
                "id": "LNK_TEST",
                "url": "https://square.link/u/bluey-test"
            }
        })))
        .expect(1)
        .mount(&h.square)
        .await;

    let req = Request::post("/billing/checkout")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "amount_cents": 3000
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["checkout_url"], "https://square.link/u/bluey-test");

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
}

#[tokio::test]
#[serial]
async fn billing_checkout_rejects_negative_reload_before_provider_call() {
    set_square_billing_env();
    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-negative-checkout@example.com", "longenoughpw").await;

    let req = Request::post("/billing/checkout")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "amount_cents": -1500
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "minimum reload is $15");

    clear_square_billing_env();
}

#[tokio::test]
#[serial]
async fn billing_square_webhook_credits_completed_order() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "square-whsec");

    let h = boot_harness().await;
    let _access = signup_and_login(&h, "square-webhook@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_credit_1",
        "type": "order.updated",
        "data": {
            "object": {
                "order": {
                    "id": "order_1",
                    "state": "COMPLETED",
                    "reference_id": format!("bluey_reload:{account_id}"),
                    "metadata": {
                        "bluey_account_id": account_id,
                        "bluey_amount_cents": "3000"
                    },
                    "total_money": {"amount": 3000, "currency": "USD"},
                    "tenders": [{
                        "payment_id": "payment_square_1",
                        "amount_money": {"amount": 3000, "currency": "USD"}
                    }]
                }
            }
        }
    }))
    .unwrap();

    let url = "http://localhost:8080/billing/square/webhook";
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"square-whsec").unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents FROM accounts WHERE email = ?1",
            rusqlite::params!["square-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(balance, 3000);

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn billing_checkout_requires_verified_email_before_first_payment() {
    set_square_billing_env();
    let h = boot_harness().await;
    let email = "unverified-checkout@example.com";
    let password_hash = auth::password::hash_password("longenoughpw").unwrap();
    Account::create(&h.pool, email, &password_hash).unwrap();
    let auth = login(&h, email, "longenoughpw").await;
    let access = auth["access_token"].as_str().unwrap();

    let req = Request::post("/billing/checkout")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "amount_cents": 3000 })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"].as_str().unwrap().contains("Verify your email"));

    clear_square_billing_env();
}

#[tokio::test]
#[serial]
async fn billing_square_refund_restricts_account_and_revokes_remaining_credit() {
    set_square_billing_env();
    std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "square-whsec");
    let h = boot_harness().await;
    let _access = signup_and_login(&h, "square-refund@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-refund@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    bluey_server::db::balance::credit_processor_payment(
        &h.pool,
        &account_id,
        3000,
        "square",
        "payment_square_refund_1",
    )
    .unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
                SET auto_topup_enabled = 1,
                    square_customer_id = 'cus_square_refund',
                    square_card_id = 'ccof:square_refund',
                    square_card_brand = 'VISA',
                    square_card_last4 = '4242'
              WHERE id = ?1",
            rusqlite::params![account_id],
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_refund_1",
        "type": "refund.updated",
        "data": {
            "object": {
                "refund": {
                    "id": "refund_square_1",
                    "payment_id": "payment_square_refund_1",
                    "status": "COMPLETED"
                }
            }
        }
    }))
    .unwrap();
    let signature = square_signature_for_test("square-whsec", &body);
    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let row: (i64, i64, i64, Option<String>, Option<String>) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents, billing_restricted, auto_topup_enabled,
                    square_card_id, billing_restriction_reason
               FROM accounts WHERE id = ?1",
            rusqlite::params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(row.0, 0);
    assert_eq!(row.1, 1);
    assert_eq!(row.2, 0);
    assert!(row.3.is_none());
    assert!(row.4.unwrap().contains("refund.updated"));

    clear_square_billing_env();
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn billing_stripe_dispute_restricts_account_and_revokes_remaining_credit() {
    let h = boot_harness().await;
    let _access = signup_and_login(&h, "stripe-dispute@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["stripe-dispute@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    bluey_server::db::balance::credit_processor_payment(
        &h.pool,
        &account_id,
        3000,
        "stripe",
        "pi_dispute_1",
    )
    .unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
                SET auto_topup_enabled = 1,
                    stripe_customer_id = 'cus_dispute',
                    stripe_payment_method_id = 'pm_dispute'
              WHERE id = ?1",
            rusqlite::params![account_id],
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "id": "evt_stripe_dispute_1",
        "type": "charge.dispute.created",
        "data": {
            "object": {
                "id": "dp_1",
                "payment_intent": "pi_dispute_1"
            }
        }
    }))
    .unwrap();
    let signature = stripe_signature_for_test("whsec_test_e2e", &body);
    let req = Request::post("/billing/webhook")
        .header("Stripe-Signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let row: (i64, i64, i64, Option<String>, Option<String>) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents, billing_restricted, auto_topup_enabled,
                    stripe_payment_method_id, billing_restriction_reason
               FROM accounts WHERE id = ?1",
            rusqlite::params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(row.0, 0);
    assert_eq!(row.1, 1);
    assert_eq!(row.2, 0);
    assert!(row.3.is_none());
    assert!(row.4.unwrap().contains("charge.dispute.created"));
}

#[tokio::test]
#[serial]
async fn billing_stripe_auto_reload_succeeded_webhook_credits_exactly_once() {
    let h = boot_harness().await;
    let _access =
        signup_and_login(&h, "stripe-auto-reload-webhook@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["stripe-auto-reload-webhook@example.com"],
            |row| row.get(0),
        )
        .unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
                SET auto_topup_enabled = 1,
                    auto_topup_threshold_cents = 500,
                    auto_topup_amount_cents = 1500,
                    stripe_customer_id = 'cus_auto_webhook',
                    stripe_payment_method_id = 'pm_auto_webhook'
              WHERE id = ?1",
            rusqlite::params![account_id],
        )
        .unwrap();
    let attempt = bluey_server::db::stripe_auto_reload::reserve_if_eligible(&h.pool, &account_id)
        .unwrap()
        .unwrap();
    let attempt = bluey_server::db::stripe_auto_reload::attach_payment_intent(
        &h.pool,
        &attempt.id,
        "pi_auto_webhook_1",
    )
    .unwrap();

    for event_id in ["evt_auto_webhook_1", "evt_auto_webhook_2"] {
        let body = serde_json::to_string(&json!({
            "id": event_id,
            "type": "payment_intent.succeeded",
            "data": {
                "object": {
                    "id": "pi_auto_webhook_1",
                    "amount": 1500,
                    "amount_received": 1500,
                    "currency": "usd",
                    "customer": "cus_auto_webhook",
                    "payment_method": "pm_auto_webhook",
                    "status": "succeeded",
                    "latest_charge": "ch_auto_webhook_1",
                    "metadata": {
                        "bluey_kind": "auto_topup",
                        "bluey_auto_reload_version": "v1",
                        "bluey_auto_reload_attempt_id": attempt.id,
                        "bluey_account_id": account_id,
                        "bluey_amount_cents": "1500"
                    }
                }
            }
        }))
        .unwrap();
        let signature = stripe_signature_for_test("whsec_test_e2e", &body);
        let request = Request::post("/billing/webhook")
            .header("Stripe-Signature", signature)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = h.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let row: (i64, i64, String) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT a.balance_cents,
                    (SELECT COUNT(*) FROM credit_batches
                      WHERE stripe_charge_id = 'stripe:pi_auto_webhook_1'),
                    r.status
               FROM accounts a
               JOIN stripe_auto_reload_attempts r ON r.account_id = a.id
              WHERE a.id = ?1",
            rusqlite::params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, (1500, 1, "succeeded".to_string()));
}

#[tokio::test]
#[serial]
async fn billing_square_webhook_rejects_amount_mismatch_without_credit() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "square-whsec");

    let h = boot_harness().await;
    let _access = signup_and_login(&h, "square-mismatch@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-mismatch@example.com"],
            |r| r.get(0),
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_amount_mismatch_1",
        "type": "order.updated",
        "data": {
            "object": {
                "order": {
                    "id": "order_amount_mismatch_1",
                    "state": "COMPLETED",
                    "reference_id": format!("bluey_reload:{account_id}"),
                    "metadata": {
                        "bluey_account_id": account_id,
                        "bluey_amount_cents": "3000"
                    },
                    "total_money": {"amount": 1500, "currency": "USD"},
                    "tenders": [{
                        "payment_id": "payment_square_amount_mismatch_1",
                        "amount_money": {"amount": 1500, "currency": "USD"}
                    }]
                }
            }
        }
    }))
    .unwrap();

    let url = "http://localhost:8080/billing/square/webhook";
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"square-whsec").unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 500);

    let balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents FROM accounts WHERE email = ?1",
            rusqlite::params!["square-mismatch@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(balance, 0);

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn billing_square_webhook_rejects_account_reference_mismatch_without_credit() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "square-whsec");

    let h = boot_harness().await;
    let _access_a = signup_and_login(&h, "square-ref-a@example.com", "longenoughpw").await;
    let _access_b = signup_and_login(&h, "square-ref-b@example.com", "longenoughpw").await;
    let account_a: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-ref-a@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    let account_b: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-ref-b@example.com"],
            |r| r.get(0),
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_reference_mismatch_1",
        "type": "order.updated",
        "data": {
            "object": {
                "order": {
                    "id": "order_reference_mismatch_1",
                    "state": "COMPLETED",
                    "reference_id": format!("bluey_reload:{account_a}"),
                    "metadata": {
                        "bluey_account_id": account_b,
                        "bluey_amount_cents": "3000"
                    },
                    "total_money": {"amount": 3000, "currency": "USD"},
                    "tenders": [{
                        "payment_id": "payment_square_reference_mismatch_1",
                        "amount_money": {"amount": 3000, "currency": "USD"}
                    }]
                }
            }
        }
    }))
    .unwrap();

    let url = "http://localhost:8080/billing/square/webhook";
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"square-whsec").unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 500);

    let total_balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COALESCE(SUM(balance_cents), 0) FROM accounts WHERE email IN (?1, ?2)",
            rusqlite::params!["square-ref-a@example.com", "square-ref-b@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(total_balance, 0);

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn billing_square_webhook_rejects_production_signature_while_checkout_is_sandbox() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let url = "http://localhost:8080/billing/square/webhook";
    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_WEBHOOK_NOTIFICATION_URL", url);
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var(
        "SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY",
        "sandbox-square-whsec",
    );
    std::env::set_var(
        "SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY",
        "prod-square-whsec",
    );

    let h = boot_harness().await;
    let _access = signup_and_login(&h, "square-prod-webhook@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-prod-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_prod_credit_1",
        "type": "order.updated",
        "data": {
            "object": {
                "order": {
                    "id": "order_prod_1",
                    "state": "COMPLETED",
                    "reference_id": format!("bluey_reload:{account_id}"),
                    "metadata": {
                        "bluey_account_id": account_id,
                        "bluey_amount_cents": "1500"
                    },
                    "total_money": {"amount": 1500, "currency": "USD"},
                    "tenders": [{
                        "payment_id": "payment_square_prod_1",
                        "amount_money": {"amount": 1500, "currency": "USD"}
                    }]
                }
            }
        }
    }))
    .unwrap();

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"prod-square-whsec").unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);

    let balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents FROM accounts WHERE email = ?1",
            rusqlite::params!["square-prod-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(balance, 0);

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_WEBHOOK_NOTIFICATION_URL");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
    std::env::remove_var("SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn square_auto_reload_requires_saved_card_then_enables() {
    set_square_billing_env();
    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-autoreload@example.com", "longenoughpw").await;

    let req = Request::get("/account/me")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let me: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(me["billing_provider"], "square");
    assert_eq!(me["auto_topup_enabled"], false);
    assert_eq!(me["auto_topup_threshold_cents"], 500);
    assert_eq!(me["auto_topup_amount_cents"], 1500);
    assert_eq!(me["auto_topup_available"], false);
    assert_eq!(me["square_application_id"], "sandbox-app");
    assert_eq!(me["square_location_id"], "sandbox-location");
    assert_eq!(me["square_environment"], "sandbox");

    let req = Request::patch("/account/billing")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "auto_topup_enabled": false,
                "auto_topup_threshold_cents": 500,
                "auto_topup_amount_cents": -1500
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::patch("/account/billing")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "auto_topup_enabled": false,
                "auto_topup_threshold_cents": -500,
                "auto_topup_amount_cents": 1500
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::patch("/account/billing")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "auto_topup_enabled": false,
                "auto_topup_threshold_cents": 500,
                "auto_topup_amount_cents": 1400
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::patch("/account/billing")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "auto_topup_enabled": false,
                "auto_topup_threshold_cents": 1500,
                "auto_topup_amount_cents": 1500
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::patch("/account/billing")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "auto_topup_enabled": true })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .and(header("Square-Version", "2025-04-16"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "customer": { "id": "cus_square_autoreload" }
        })))
        .expect(1)
        .mount(&h.square)
        .await;

    Mock::given(method("POST"))
        .and(path("/v2/cards"))
        .and(header("Square-Version", "2025-04-16"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "card": {
                "id": "ccof:square_card_1",
                "card_brand": "VISA",
                "last_4": "4242"
            }
        })))
        .expect(1)
        .mount(&h.square)
        .await;

    let req = Request::post("/billing/square/card")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "source_id": "cnon:sandbox-card-nonce" })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let me: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(me["auto_topup_enabled"], false);
    assert_eq!(me["auto_topup_available"], true);
    assert_eq!(me["saved_payment_method_label"], "VISA ending 4242");

    let req = Request::patch("/account/billing")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "auto_topup_enabled": true,
                "auto_topup_threshold_cents": 500,
                "auto_topup_amount_cents": 1500
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let me: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(me["auto_topup_enabled"], true);
    assert_eq!(me["auto_topup_available"], true);
    assert_eq!(me["auto_topup_threshold_cents"], 500);
    assert_eq!(me["auto_topup_amount_cents"], 1500);

    clear_square_billing_env();
}

#[tokio::test]
#[serial]
async fn square_pay_with_saved_card_adds_balance_and_updates_auto_reload() {
    set_square_billing_env();
    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-direct-pay@example.com", "longenoughpw").await;

    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-direct-pay@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
                SET square_customer_id = 'cus_square_direct',
                    square_card_id = 'ccof:square_direct',
                    square_card_brand = 'VISA',
                    square_card_last4 = '4242'
              WHERE id = ?1",
            rusqlite::params![account_id],
        )
        .unwrap();

    Mock::given(method("POST"))
        .and(path("/v2/payments"))
        .and(header("Square-Version", "2025-04-16"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "payment": {
                "id": "payment_square_direct_1",
                "status": "COMPLETED",
                "amount_money": {"amount": 1500, "currency": "USD"},
                "reference_id": square_reload_reference_id_for_test(&account_id),
                "customer_id": "cus_square_direct"
            }
        })))
        .expect(1)
        .mount(&h.square)
        .await;

    let req = Request::post("/billing/square/pay")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "amount_cents": 1500,
                "use_saved_card": true,
                "auto_topup_enabled": true,
                "auto_topup_threshold_cents": 500,
                "auto_topup_amount_cents": 1500,
                "client_request_id": "direct-pay-test-1"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["credited"], true);
    assert_eq!(v["payment_status"], "COMPLETED");
    assert_eq!(v["account"]["balance_cents"], 1500);
    assert_eq!(v["account"]["auto_topup_enabled"], true);
    assert_eq!(v["account"]["auto_topup_threshold_cents"], 500);
    assert_eq!(v["account"]["auto_topup_amount_cents"], 1500);
    assert_eq!(v["account"]["auto_topup_available"], true);
    assert_eq!(
        v["account"]["saved_payment_method_label"],
        "VISA ending 4242"
    );

    let row: (i64, i64, i64, i64) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents, auto_topup_enabled, auto_topup_threshold_cents,
                    auto_topup_amount_cents
               FROM accounts WHERE id = ?1",
            rusqlite::params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(row, (1500, 1, 500, 1500));

    clear_square_billing_env();
}

#[tokio::test]
#[serial]
async fn billing_portal_400s_without_stripe_customer() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "no-cus@example.com", "longenoughpw").await;

    let req = Request::post("/billing/portal")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
#[serial]
async fn auto_topup_off_by_default_does_not_fire_charge() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "topup@example.com", "longenoughpw").await;

    // Drain trial seconds + set a low balance. New accounts default to
    // manual reload, so even a post-deduct low balance must not charge.
    let conn = h.pool.get().unwrap();
    conn.execute(
        "UPDATE accounts SET trial_seconds_remaining = 0, balance_cents = 100 WHERE email = ?1",
        rusqlite::params!["topup@example.com"],
    )
    .unwrap();

    // No Stripe mock for /v1/payment_intents — if topup fired, it would
    // hit a non-existent endpoint and the test would still pass because
    // the spawn is fire-and-forget. But we assert the wiremock has
    // received ZERO matching POSTs to /v1/payment_intents.
    Mock::given(method("POST"))
        .and(path("/v1/payment_intents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "pi_should_not_fire",
            "status": "succeeded"
        })))
        .expect(0) // strict: must NOT be called.
        .mount(&h.stripe)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "topup-test-no-pm",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "hi",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    // Either 200 (cue completed despite low balance via trial absorb) or
    // 402 (insufficient balance). Both are acceptable — the assertion is
    // that the Stripe payment_intents mock was NOT called.
    assert!(resp.status() == 200 || resp.status() == 402);

    // Give the spawned topup task a chance to run if it would.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

#[tokio::test]
#[serial]
async fn square_mode_never_runs_legacy_stripe_auto_topup() {
    set_square_billing_env();

    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-no-stripe-topup@example.com", "longenoughpw").await;

    // Simulate a legacy account that still has Stripe card metadata and
    // auto-topup enabled. Square is the active provider, so this must not
    // charge the old Stripe path.
    let conn = h.pool.get().unwrap();
    conn.execute(
        "UPDATE accounts
            SET trial_seconds_remaining = 0,
                balance_cents = 100,
                auto_topup_enabled = 1,
                stripe_customer_id = 'cus_legacy',
                stripe_payment_method_id = 'pm_legacy'
          WHERE email = ?1",
        rusqlite::params!["square-no-stripe-topup@example.com"],
    )
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/payment_intents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "pi_should_not_fire_in_square_mode",
            "status": "succeeded"
        })))
        .expect(0)
        .mount(&h.stripe)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "square-mode-no-stripe-topup",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "hi",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert!(resp.status() == 200 || resp.status() == 402);

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    clear_square_billing_env();
}

#[tokio::test]
#[serial]
async fn square_auto_reload_charges_saved_card_when_threshold_crosses() {
    set_square_billing_env();
    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-card-topup@example.com", "longenoughpw").await;

    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-card-topup@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
                SET trial_seconds_remaining = 0,
                    balance_cents = 1000,
                    auto_topup_enabled = 1,
                    auto_topup_threshold_cents = 1200,
                    auto_topup_amount_cents = 1500,
                    square_customer_id = 'cus_square_topup',
                    square_card_id = 'ccof:square_card_topup',
                    square_card_brand = 'VISA',
                    square_card_last4 = '4242'
              WHERE email = ?1",
            rusqlite::params!["square-card-topup@example.com"],
        )
        .unwrap();

    Mock::given(method("POST"))
        .and(path("/v2/payments"))
        .and(header("Square-Version", "2025-04-16"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "payment": {
                "id": "payment_square_autoreload_1",
                "status": "COMPLETED",
                "amount_money": {"amount": 1500, "currency": "USD"},
                "reference_id": square_reload_reference_id_for_test(&account_id),
                "customer_id": "cus_square_topup"
            }
        })))
        .expect(1)
        .mount(&h.square)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "square-card-topup-trigger",
                "system": MANAGED_PROVIDER_BASE_CONTRACT,
                "user": "hi",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    for _ in 0..30 {
        let balance: i64 = h
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT balance_cents FROM accounts WHERE id = ?1",
                rusqlite::params![&account_id],
                |r| r.get(0),
            )
            .unwrap();
        if balance >= 2400 {
            clear_square_billing_env();
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents FROM accounts WHERE id = ?1",
            rusqlite::params![&account_id],
            |r| r.get(0),
        )
        .unwrap();
    clear_square_billing_env();
    panic!("expected Square auto reload to credit balance, found {balance}");
}

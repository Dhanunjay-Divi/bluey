//! Codex Stage 23: end-to-end integration tests with wiremock.
//!
//! Spins up the server's full Axum router pointed at wiremock instances
//! that stand in for OpenAI / Anthropic / Deepgram / Stripe. Exercises
//! the customer money-path top-to-bottom.

#![cfg(test)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use ed25519_dalek::{Signer as _, SigningKey as Ed25519SigningKey};
use hmac::{Hmac, Mac};
use serde_json::json;
use serial_test::serial;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Cursor, Read};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tower::ServiceExt;
use wiremock::matchers::{header, method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use bluey_server::auth;
use bluey_server::config::{
    Config, ObjectStorageConfig, SmtpConfig, UpstreamKeys, UpstreamSpendGuard,
};
use bluey_server::db::accounts::Account;
use bluey_server::db::jobs::{
    self, ApplicationEvidence, BrowserSession, DiscoverySourceInput, Intervention,
    JobDiscoveryEvidence, JobPosting, JobPreferences,
};
use bluey_server::db::jobs_beta_access::{
    self as jobs_beta_access, PublicBetaAccessReason, PublicBetaCohortState, PublicBetaCohortUpdate,
};
use bluey_server::db::object_uploads::NewSubmissionEvidenceCapacity;
use bluey_server::db::usage::{self, UsageEvent};
use bluey_server::db::{idempotency, open_pool, run_migrations, DbPool};
use bluey_server::object_storage::UploadLimits;
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

const TEST_BROWSER_SERVER_RELEASE_ID: &str = "server-603.1";
const TEST_BROWSER_RELEASE_KEY_INDEX: usize = 6;
const TEST_PUBLIC_BETA_AUDIT_ACTOR: &str = "integration-public-beta-admin";
const EXECUTION_LEASE_SOURCE_RESUME_BYTES: &[u8] =
    b"Exact source resume bytes for execution integration tests";

struct BrowserBuildProofFixture {
    descriptor: String,
    signature: String,
    descriptor_sha256: String,
}

fn browser_release_authority_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../jobs/browser/fixtures/release-authority-v1.json"
    ))
    .expect("parse shared Browser release authority fixture")
}

fn browser_build_proof_fixture(platform: &str, architecture: &str) -> BrowserBuildProofFixture {
    let fixture = browser_release_authority_fixture();
    let shared_descriptor = fixture["descriptor"]
        .as_str()
        .expect("shared Browser build descriptor");
    let shared_signature = fixture["signature"]
        .as_str()
        .expect("shared Browser build signature");
    let (descriptor, signature) = if platform == "darwin" && architecture == "arm64" {
        (shared_descriptor.to_string(), shared_signature.to_string())
    } else {
        assert!(matches!(
            (platform, architecture),
            ("darwin", "x64") | ("windows", "x64")
        ));
        let shared_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(shared_descriptor)
            .expect("decode shared Browser build descriptor");
        let shared_text = String::from_utf8(shared_bytes).expect("UTF-8 Browser build descriptor");
        let canonical = shared_text.replacen(
            "platform=darwin\narchitecture=arm64\n",
            &format!("platform={platform}\narchitecture={architecture}\n"),
            1,
        );
        assert_ne!(canonical, shared_text);
        // Mirrors the deterministic release-key fixture used by the shared Node vector.
        let seed = std::array::from_fn(|offset| {
            ((TEST_BROWSER_RELEASE_KEY_INDEX * 37 + offset) % 256) as u8
        });
        let signing_key = Ed25519SigningKey::from_bytes(&seed);
        let expected_public_key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing_key.verifying_key().to_bytes());
        assert_eq!(fixture["publicKey"], expected_public_key);
        (
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical.as_bytes()),
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signing_key.sign(canonical.as_bytes()).to_bytes()),
        )
    };
    let descriptor_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&descriptor)
        .expect("decode target Browser build descriptor");
    let mut digest = Sha256::new();
    digest.update(&descriptor_bytes);
    digest.update(b"signature=");
    digest.update(signature.as_bytes());
    digest.update(b"\n");
    let descriptor_sha256 = hex::encode(digest.finalize());
    if platform == "darwin" && architecture == "arm64" {
        assert_eq!(fixture["descriptorSha256"], descriptor_sha256);
    }
    BrowserBuildProofFixture {
        descriptor,
        signature,
        descriptor_sha256,
    }
}

fn canonical_browser_build_proof() -> serde_json::Value {
    let proof = browser_build_proof_fixture("darwin", "arm64");
    json!({
        "descriptor": proof.descriptor,
        "signature": proof.signature,
    })
}

fn local_browser_claim_nonce(
    run_id: &str,
    ticket: &str,
    build_proof: &serde_json::Value,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-browser-claim-v1\0");
    digest.update(run_id.as_bytes());
    digest.update(b"\0");
    digest.update(ticket.as_bytes());
    digest.update(b"\0");
    digest.update(
        build_proof["descriptor"]
            .as_str()
            .expect("Browser claim descriptor")
            .as_bytes(),
    );
    digest.update(b"\0");
    digest.update(
        build_proof["signature"]
            .as_str()
            .expect("Browser claim signature")
            .as_bytes(),
    );
    hex::encode(digest.finalize())
}

fn local_browser_claim_body_with_proof(
    run_id: &str,
    ticket: &str,
    build_proof: serde_json::Value,
) -> serde_json::Value {
    let claim_nonce = local_browser_claim_nonce(run_id, ticket, &build_proof);
    json!({
        "ticket": ticket,
        "claimNonce": claim_nonce,
        "buildProof": build_proof,
    })
}

fn local_browser_claim_body(run_id: &str, ticket: &str) -> serde_json::Value {
    local_browser_claim_body_with_proof(run_id, ticket, canonical_browser_build_proof())
}

fn legacy_local_run_capability(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    operation: &str,
    expires_at_ms: i64,
) -> String {
    let claims = json!({
        "version": 1,
        "audience": "bluey-jobs-local-run",
        "account_id": account_id,
        "application_id": application_id,
        "run_id": run_id,
        "browser_profile_id": browser_profile_id,
        "operation": operation,
        "expires_at_ms": expires_at_ms,
        "nonce": "phase603legacyrecoverynonce0001",
    });
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());
    let key =
        std::env::var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY").expect("local Browser capability key");
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    format!("{payload}.{}", hex::encode(mac.finalize().into_bytes()))
}

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

fn signed_worker_json_request(
    path: &str,
    scope: &str,
    worker_id: &str,
    timestamp: u64,
    nonce: &str,
    body: &serde_json::Value,
    signing_key: &str,
) -> Request<Body> {
    let bytes = serde_json::to_vec(body).unwrap();
    let mut request = signed_worker_request(SignedWorkerRequest {
        path,
        scope,
        worker_id,
        timestamp,
        nonce,
        signed_body: &bytes,
        actual_body: &bytes,
        signing_key,
    });
    request.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    request
}

fn debug_worker_oversized_json_request(
    path: &str,
    token: &str,
    minimum_body_bytes: usize,
) -> Request<Body> {
    let body = serde_json::to_vec(&json!({
        "padding": "x".repeat(minimum_body_bytes),
    }))
    .unwrap();
    assert!(body.len() > minimum_body_bytes);
    Request::post(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn assert_workflow_command_response_headers(headers: &axum::http::HeaderMap) {
    assert_eq!(headers.get("content-type").unwrap(), "application/json");
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
}

fn ats_certification_mutation_count(pool: &DbPool) -> i64 {
    pool.get()
        .unwrap()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM jobs_ats_certification_trust_policies) +
                (SELECT COUNT(*) FROM jobs_ats_certification_trust_keys) +
                (SELECT COUNT(*) FROM jobs_ats_certification_trust_head) +
                (SELECT COUNT(*) FROM jobs_ats_certification_layout_observations) +
                (SELECT COUNT(*) FROM jobs_ats_certification_evidence) +
                (SELECT COUNT(*) FROM jobs_ats_certification_manifests) +
                (SELECT COUNT(*) FROM jobs_ats_certification_manifest_layouts) +
                (SELECT COUNT(*) FROM jobs_ats_certification_manifest_check_results) +
                (SELECT COUNT(*) FROM jobs_ats_certification_manifest_evidence) +
                (SELECT COUNT(*) FROM jobs_ats_certification_runtime_targets) +
                (SELECT COUNT(*) FROM jobs_ats_certification_activations) +
                (SELECT COUNT(*) FROM jobs_ats_certification_head_transitions) +
                (SELECT COUNT(*) FROM jobs_ats_certification_heads) +
                (SELECT COUNT(*) FROM jobs_ats_certification_revocations) +
                (SELECT COUNT(*) FROM jobs_ats_certification_quarantine_commands) +
                (SELECT COUNT(*) FROM jobs_ats_certification_quarantine_heads) +
                (SELECT COUNT(*) FROM jobs_ats_certification_circuit_events) +
                (SELECT COUNT(*) FROM jobs_ats_certification_circuit_heads) +
                (SELECT COUNT(*) FROM jobs_application_ats_certification_bindings) +
                (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations)",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn runner_volume_http_payload_sha256(
    path: &str,
    worker_id: &str,
    payload_fields: &[(&str, &str)],
) -> String {
    let mut canonical = format!(
        "bluey-jobs-runner-volume-http-payload-v1\nmethod=POST\npath={path}\nworker_id={worker_id}\n"
    );
    for (name, value) in payload_fields {
        canonical.push_str(name);
        canonical.push('=');
        canonical.push_str(value);
        canonical.push('\n');
    }
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

struct SignedRunnerVolumeFixture {
    signing_key: Ed25519SigningKey,
    worker_id: String,
    volume_id: String,
    enrollment_epoch: i64,
    process_instance_id: String,
    key_fingerprint: String,
    resource_fingerprint: String,
}

struct SignedRunnerProcessRuntimeFixture {
    grant: jobs::RunnerProcessRuntimeGrantClaim,
    runtime_sha256: String,
}

fn runner_volume_instance_claim_payload_sha256(
    path: &str,
    worker_id: &str,
    runtime_grant: &jobs::RunnerProcessRuntimeGrantClaim,
    runtime_sha256: &str,
) -> String {
    let runtime_grant_token_sha256 =
        hex::encode(Sha256::digest(runtime_grant.grant_token.as_bytes()));
    runner_volume_http_payload_sha256(
        path,
        worker_id,
        &[
            ("runtime_grant_id", runtime_grant.grant_id.as_str()),
            (
                "runtime_grant_token_sha256",
                runtime_grant_token_sha256.as_str(),
            ),
            ("runtime_sha256", runtime_sha256),
        ],
    )
}

fn enroll_unattested_offline_runner_volume(
    pool: &DbPool,
    seed_byte: u8,
    label: &str,
) -> SignedRunnerVolumeFixture {
    let signing_key = Ed25519SigningKey::from_bytes(&[seed_byte; 32]);
    let public_key_base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(signing_key.verifying_key().to_bytes());
    let volume_id = jobs::runner_volume_id_from_public_key(&public_key_base64url).unwrap();
    let key_fingerprint = jobs::runner_volume_key_fingerprint(&public_key_base64url).unwrap();
    let process_instance_id =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([seed_byte.wrapping_add(1); 32]);
    let grant_token =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([seed_byte.wrapping_add(2); 32]);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let worker_id = format!("offline-worker-{label}");
    let provider_resource_id = format!("offline-resource-{label}");
    let resource_fingerprint = hex::encode(Sha256::digest(provider_resource_id.as_bytes()));
    let grant_id = format!("offline-grant-{label}");
    jobs::create_runner_volume_admission_grant(
        pool,
        &jobs::NewRunnerVolumeAdmissionGrant {
            grant_id: grant_id.clone(),
            token: grant_token.clone(),
            expected_worker_id: worker_id.clone(),
            provider: "integration".to_string(),
            provider_resource_id: provider_resource_id.clone(),
            resource_fingerprint: resource_fingerprint.clone(),
            authorization_ref: format!("offline-authorization-{label}"),
            created_by: "phase-602-integration-admin".to_string(),
            expires_at_ms: now_ms + 600_000,
            created_at_ms: now_ms,
        },
    )
    .unwrap();
    let proof = jobs::sign_runner_volume_enrollment_proof(
        &signing_key,
        jobs::NewRunnerVolumeEnrollmentProof {
            admission_grant_id: grant_id,
            volume_id: volume_id.clone(),
            worker_id: worker_id.clone(),
            provider: "integration".to_string(),
            provider_resource_id,
            resource_fingerprint: resource_fingerprint.clone(),
            enrollment_epoch: 1,
            public_key_base64url,
            key_fingerprint: key_fingerprint.clone(),
            legacy_artifact_count: 0,
            requested_at_ms: now_ms,
        },
    )
    .unwrap();
    let enrolled = jobs::enroll_runner_volume(
        pool,
        &jobs::EnrollRunnerVolumeRequest {
            grant_token,
            proof,
            enrolled_at_ms: now_ms,
        },
    )
    .unwrap();
    assert_eq!(enrolled.status, "reconciling");
    assert!(enrolled.active_instance_id.is_none());
    let attestation_count: i64 = pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM jobs_runner_volume_storage_attestations \
              WHERE volume_id = ?1",
            rusqlite::params![volume_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attestation_count, 0);
    SignedRunnerVolumeFixture {
        signing_key,
        worker_id,
        volume_id,
        enrollment_epoch: 1,
        process_instance_id,
        key_fingerprint,
        resource_fingerprint,
    }
}

fn authorize_empty_legacy_runner_inventory(
    pool: &DbPool,
    reconciliation_id: &str,
) -> jobs::RunnerVolumeFleetStatus {
    let fleet = jobs::runner_volume_fleet_status(pool).unwrap();
    if fleet.legacy_inventory_state == "ready" {
        return fleet;
    }
    assert!(matches!(
        fleet.legacy_inventory_state.as_str(),
        "unknown" | "reconciling"
    ));
    let now_ms = chrono::Utc::now().timestamp_millis();
    let reconciling = jobs::record_runner_legacy_inventory_authority(
        pool,
        &jobs::RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: reconciliation_id.to_string(),
            authority_state: "reconciling".to_string(),
            expected_predecessor_generation: fleet.legacy_inventory_generation,
            expected_predecessor_authority_id: fleet.legacy_inventory_authority_id,
            expected_predecessor_authority_sha256: fleet.legacy_inventory_authority_sha256,
            root_count: 0,
            root_set_sha256: jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
            scope_ref: "phase-602-integration-empty-legacy-roots".to_string(),
            evidence_ref: "phase-602-integration-inventory-reconciling".to_string(),
            evidence_sha256: hex::encode(Sha256::digest(
                b"phase-602-integration-inventory-reconciling",
            )),
            authorized_by: "phase-602-integration-admin".to_string(),
            recorded_at_ms: now_ms,
        },
    )
    .unwrap()
    .authority;
    jobs::record_runner_legacy_inventory_authority(
        pool,
        &jobs::RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: reconciliation_id.to_string(),
            authority_state: "ready".to_string(),
            expected_predecessor_generation: reconciling.authority_generation,
            expected_predecessor_authority_id: Some(reconciling.authority_id),
            expected_predecessor_authority_sha256: Some(reconciling.authority_sha256),
            root_count: 0,
            root_set_sha256: jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
            scope_ref: reconciling.scope_ref,
            evidence_ref: "phase-602-integration-inventory-ready".to_string(),
            evidence_sha256: hex::encode(Sha256::digest(b"phase-602-integration-inventory-ready")),
            authorized_by: "phase-602-integration-admin".to_string(),
            recorded_at_ms: now_ms + 1,
        },
    )
    .unwrap();
    let ready = jobs::runner_volume_fleet_status(pool).unwrap();
    assert_eq!(ready.legacy_inventory_state, "ready");
    ready
}

fn canonical_workflow_cleanup_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => serde_json::to_string(value).unwrap(),
        serde_json::Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_workflow_cleanup_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        serde_json::Value::Object(fields) => {
            let mut fields = fields
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        canonical_workflow_cleanup_json(value)
                    )
                })
                .collect::<Vec<_>>();
            fields.sort();
            format!("{{{}}}", fields.join(","))
        }
    }
}

fn workflow_cleanup_test_digest(domain: &[u8], value: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(canonical_workflow_cleanup_json(value).as_bytes());
    hex::encode(digest.finalize())
}

fn empty_workflow_inventory_receipt(
    lease: &jobs::JobsLegacyInventoryPageLease,
) -> serde_json::Value {
    let targets = json!([]);
    let targets_digest =
        workflow_cleanup_test_digest(b"bluey-jobs-legacy-inventory-targets-v3\0", &targets);
    let mut receipt = json!({
        "schemaVersion": 3,
        "operation": "legacy_inventory_page",
        "cleanupRequestId": lease.cleanup_request_id,
        "inventoryGenerationId": lease.inventory_generation_id,
        "namespace": lease.namespace,
        "workflowType": lease.workflow_type,
        "visibilityCutoffMs": lease.visibility_cutoff_ms,
        "queryDigest": lease.query_digest,
        "scanPass": lease.scan_pass,
        "pageIndex": lease.page_index,
        "predecessorPageDigest": lease.predecessor_page_digest,
        "pageToken": lease.page_token,
        "cleanupFence": lease.cleanup_fence,
        "outcome": "page",
        "targetsDigest": targets_digest,
        "targets": targets,
        "nextPageToken": null,
        "exhausted": true,
    });
    let page_digest =
        workflow_cleanup_test_digest(b"bluey-jobs-legacy-inventory-page-v3\0", &receipt);
    receipt["pageDigest"] = json!(page_digest);
    receipt
}

async fn authorize_empty_workflow_cleanup_inventory(
    pool: &DbPool,
) -> jobs::JobsLegacyInventoryAuthorityRef {
    const CONFIRMATION_AGE_MS: i64 = 1_000;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let prepared = jobs::prepare_jobs_legacy_inventory_generation(
        pool,
        &jobs::PrepareJobsLegacyInventoryGeneration {
            namespace: "bluey-jobs-account-delete-test".to_string(),
            visibility_cutoff_ms: 1_783_900_800_000,
            confirmation_age_ms: CONFIRMATION_AGE_MS,
            now_ms,
        },
    )
    .unwrap();
    if prepared.state == jobs::JobsLegacyInventoryState::Complete {
        return prepared.authority;
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let Some(lease) = jobs::claim_jobs_workflow_cleanup_work(
            pool,
            "account-delete-cleanup-test-owner",
            now_ms,
            30_000,
        )
        .unwrap() else {
            assert!(
                tokio::time::Instant::now() < deadline,
                "empty workflow inventory did not become eligible on the DB clock"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        };
        let jobs::JobsWorkflowCleanupWorkLease::LegacyInventoryPage(page) = &lease else {
            panic!("empty workflow inventory helper claimed unexpected target work");
        };
        assert_eq!(page.page_index, 0);
        let receipt = empty_workflow_inventory_receipt(page);
        jobs::mark_jobs_workflow_cleanup_request_started(pool, &lease, now_ms).unwrap();
        let state = jobs::record_jobs_workflow_cleanup_receipt(
            pool,
            &lease,
            &receipt,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
        if state == jobs::JobsWorkflowCleanupReceiptState::InventoryComplete {
            return prepared.authority;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "empty workflow inventory did not complete its two-pass proof"
        );
    }
}

fn confirmed_account_delete_request(access: &str) -> Request<Body> {
    Request::post("/account/delete")
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
        .unwrap()
}

async fn fence_account_deletion_then_revalidate_workflow_cleanup(harness: &Harness, access: &str) {
    let response = harness
        .router
        .clone()
        .oneshot(confirmed_account_delete_request(access))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        pending["state"].as_str(),
        Some("pending_workflow_cleanup" | "pending_workflow_cleanup_configuration")
    ));
    assert_eq!(pending["object_count_deleted"], 0);
    authorize_empty_workflow_cleanup_inventory(&harness.pool).await;
}

struct TestEnvironmentGuard {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl TestEnvironmentGuard {
    fn install(values: &[(&'static str, String)]) -> Self {
        let previous = values
            .iter()
            .map(|(name, value)| {
                let previous = std::env::var_os(name);
                std::env::set_var(name, value);
                (*name, previous)
            })
            .collect();
        Self { previous }
    }
}

impl Drop for TestEnvironmentGuard {
    fn drop(&mut self) {
        for (name, value) in self.previous.iter().rev() {
            if let Some(value) = value {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }
}

fn browser_release_root_trust_anchor_json() -> String {
    let fixture = browser_release_authority_fixture();
    let encoded_policy = fixture
        .pointer("/trustPolicy/canonical")
        .and_then(serde_json::Value::as_str)
        .expect("canonical Browser trust policy fixture");
    let policy_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded_policy)
        .expect("decode canonical Browser trust policy fixture");
    let policy: serde_json::Value =
        serde_json::from_slice(&policy_bytes).expect("parse canonical Browser trust policy");
    let root_threshold = policy["roles"]
        .as_array()
        .and_then(|roles| {
            roles
                .iter()
                .find(|role| role["role"] == "root")
                .and_then(|role| role["threshold"].as_i64())
        })
        .expect("root trust threshold");
    let root_keys = policy["keys"]
        .as_array()
        .expect("Browser trust keys")
        .iter()
        .filter(|key| key["role"] == "root")
        .map(|key| {
            (
                key["keyId"].as_str().expect("root key id").to_string(),
                key["publicKey"]
                    .as_str()
                    .expect("root public key")
                    .to_string(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    json!({ "threshold": root_threshold, "keys": root_keys }).to_string()
}

fn enable_local_browser_distribution_for_test(pool: &DbPool, label: &str) -> TestEnvironmentGuard {
    let fleet =
        authorize_empty_legacy_runner_inventory(pool, &format!("phase-603-local-browser-{label}"));
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut cutover = jobs::RecordRunnerVolumeFleetCutoverRequest {
        cutover_state: "reconciling".to_string(),
        expected_enrollment_generation: fleet.enrollment_generation,
        expected_purge_generation: fleet.purge_generation,
        expected_tombstone_generation: fleet.tombstone_generation,
        expected_destruction_generation: fleet.destruction_generation,
        expected_legacy_reconciliation_generation: fleet.legacy_reconciliation_generation,
        expected_storage_attestation_generation: fleet.storage_attestation_generation,
        expected_storage_attestation_count: fleet.storage_attestation_count,
        expected_storage_attestation_set_sha256: fleet.storage_attestation_set_sha256,
        expected_legacy_inventory_generation: fleet.legacy_inventory_generation,
        expected_legacy_inventory_reconciliation_id: fleet
            .legacy_inventory_reconciliation_id
            .expect("legacy inventory reconciliation id"),
        expected_legacy_inventory_authority_id: fleet
            .legacy_inventory_authority_id
            .expect("legacy inventory authority id"),
        expected_legacy_inventory_authority_sha256: fleet
            .legacy_inventory_authority_sha256
            .expect("legacy inventory authority digest"),
        expected_legacy_inventory_root_count: fleet
            .legacy_inventory_root_count
            .expect("legacy inventory root count"),
        expected_legacy_inventory_root_set_sha256: fleet
            .legacy_inventory_root_set_sha256
            .expect("legacy inventory root-set digest"),
        expected_non_destroyed_volume_count: fleet.non_destroyed_volume_count,
        expected_destruction_count: fleet.destruction_count,
        expected_unresolved_legacy_volume_count: fleet.unresolved_legacy_volume_count,
        evidence_ref: format!("phase-603-local-browser-{label}"),
        evidence_sha256: hex::encode(Sha256::digest(label.as_bytes())),
        authorized_by: "phase-603-integration-admin".to_string(),
        cutover_at_ms: now_ms,
        now_ms,
    };
    jobs::record_runner_volume_fleet_cutover(pool, &cutover)
        .expect("record reconciling Browser distribution fleet cutover");
    cutover.cutover_state = "ready".to_string();
    cutover.now_ms += 1;
    jobs::record_runner_volume_fleet_cutover(pool, &cutover)
        .expect("record ready Browser distribution fleet cutover");
    let ready = jobs::runner_volume_fleet_status(pool).expect("load Browser distribution fleet");
    assert_eq!(ready.cutover_state, "ready");
    assert_eq!(ready.legacy_inventory_state, "ready");
    assert_eq!(
        ready.storage_attestation_count,
        ready.non_destroyed_volume_count
    );
    assert_eq!(
        ready.attested_reconciled_volume_count,
        ready.non_destroyed_volume_count
    );

    TestEnvironmentGuard::install(&[
        (
            "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
            "1".to_string(),
        ),
        (
            "BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID",
            TEST_BROWSER_SERVER_RELEASE_ID.to_string(),
        ),
        (
            "BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY",
            "phase-603-local-browser-capability-key".to_string(),
        ),
        (
            "BLUEY_JOBS_BROWSER_ROOT_TRUST_ANCHOR_JSON",
            browser_release_root_trust_anchor_json(),
        ),
    ])
}

fn decoded_browser_release_authority(
    fixture: &serde_json::Value,
    authority: &str,
    field: &str,
) -> serde_json::Value {
    let encoded = fixture[authority][field]
        .as_str()
        .unwrap_or_else(|| panic!("missing {authority}.{field} Browser release fixture"));
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .unwrap_or_else(|_| panic!("decode {authority}.{field} Browser release fixture"));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("parse {authority}.{field} Browser release fixture"))
}

fn seed_canonical_browser_release_authority(pool: &DbPool, account_id: &str) {
    let fixture = browser_release_authority_fixture();
    let manifest = decoded_browser_release_authority(&fixture, "manifest", "canonical");
    let envelope = |authority: &str| jobs::BrowserReleaseAuthorityEnvelope {
        canonical_base64url: fixture[authority]["canonical"]
            .as_str()
            .unwrap()
            .to_string(),
        signature_set_base64url: fixture[authority]["signatureSet"]
            .as_str()
            .unwrap()
            .to_string(),
    };
    jobs::import_browser_release_trust_policy(
        pool,
        &envelope("trustPolicy"),
        "phase-603-integration-admin",
    )
    .expect("import canonical Browser trust policy");

    let build_proofs = [("darwin", "arm64"), ("darwin", "x64"), ("windows", "x64")]
        .into_iter()
        .map(|(platform, architecture)| {
            let proof = browser_build_proof_fixture(platform, architecture);
            let expected_sha256 = manifest["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|artifact| {
                    artifact["platform"] == platform && artifact["architecture"] == architecture
                })
                .and_then(|artifact| artifact["buildDescriptorSha256"].as_str())
                .unwrap();
            assert_eq!(proof.descriptor_sha256, expected_sha256);
            jobs::BrowserBuildProof {
                descriptor: proof.descriptor,
                signature: proof.signature,
            }
        })
        .collect();
    jobs::import_browser_release_manifest(
        pool,
        &jobs::BrowserReleaseManifestImportRequest {
            canonical_base64url: fixture["manifest"]["canonical"]
                .as_str()
                .unwrap()
                .to_string(),
            signature_set_base64url: fixture["manifest"]["signatureSet"]
                .as_str()
                .unwrap()
                .to_string(),
            build_proofs,
        },
        "phase-603-integration-admin",
    )
    .expect("import canonical Browser release manifest");
    jobs::import_browser_release_activation(
        pool,
        &envelope("activation"),
        "phase-603-integration-admin",
    )
    .expect("import canonical Browser release activation");
    let status = jobs::apply_browser_release_activation(
        pool,
        &jobs::ApplyBrowserReleaseActivationRequest {
            activation_sha256: fixture["activation"]["sha256"]
                .as_str()
                .unwrap()
                .to_string(),
            expected_head_revision: 0,
            expected_transition_sha256: None,
        },
        "phase-603-integration-admin",
    )
    .expect("apply canonical Browser release activation");
    assert!(status.available);
    jobs::assign_browser_release_account_channel(
        pool,
        account_id,
        &jobs::AssignBrowserReleaseChannelRequest {
            assignment_generation: 1,
            predecessor_assignment_sha256: None,
            channel: "beta".to_string(),
            reason_ref: "phase-603-integration".to_string(),
            assigned_at_ms: chrono::Utc::now().timestamp_millis(),
        },
        "phase-603-integration-admin",
    )
    .expect("assign canonical Browser release channel");
}

fn append_canonical_browser_release_revocation(pool: &DbPool) {
    let fixture = browser_release_authority_fixture();
    let result = jobs::append_browser_release_revocation(
        pool,
        &jobs::BrowserReleaseAuthorityEnvelope {
            canonical_base64url: fixture["revocation"]["canonical"]
                .as_str()
                .expect("canonical Browser release revocation")
                .to_string(),
            signature_set_base64url: fixture["revocation"]["signatureSet"]
                .as_str()
                .expect("Browser release revocation signature set")
                .to_string(),
        },
        "phase-603-integration-incident",
    )
    .expect("append canonical Browser release revocation");
    assert_eq!(result.authority_kind, "revocation");
    assert!(!result.replayed);
}

fn local_browser_claim_state(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> (String, String, String, String, i64, i64, i64) {
    pool.get()
        .unwrap()
        .query_row(
            "SELECT ticket.status, application.state, reservation.status, session.status,
                    (SELECT COUNT(*) FROM jobs_run_events event
                      WHERE event.run_id = ticket.id
                        AND event.event_type = 'local_browser_claimed'),
                    (SELECT COUNT(*) FROM jobs_local_run_release_bindings binding
                      WHERE binding.run_id = ticket.id),
                    (SELECT COUNT(*) FROM jobs_local_run_claim_replays replay
                      WHERE replay.run_id = ticket.id)
               FROM jobs_local_run_tickets ticket
               JOIN jobs_applications application
                 ON application.account_id = ticket.account_id
                AND application.id = ticket.application_id
               JOIN jobs_attempt_reservations reservation
                 ON reservation.account_id = ticket.account_id
                AND reservation.application_id = ticket.application_id
               JOIN jobs_browser_sessions session
                 ON session.account_id = ticket.account_id AND session.id = ticket.id
              WHERE ticket.account_id = ?1 AND ticket.application_id = ?2 AND ticket.id = ?3",
            rusqlite::params![account_id, application_id, run_id],
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
        .unwrap()
}

fn signed_runner_volume_proof(
    fixture: &SignedRunnerVolumeFixture,
    operation: &str,
    request_id: &str,
    issued_at_ms: i64,
    payload_sha256: String,
) -> jobs::RunnerVolumeAuthorityProof {
    jobs::sign_runner_volume_authority_proof(
        &fixture.signing_key,
        jobs::NewRunnerVolumeAuthorityProof {
            operation: operation.to_string(),
            request_id: request_id.to_string(),
            volume_id: fixture.volume_id.clone(),
            enrollment_epoch: fixture.enrollment_epoch,
            process_instance_id: fixture.process_instance_id.clone(),
            issued_at_ms,
            payload_sha256,
        },
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn execution_claim_payload_sha256(
    worker_id: &str,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    owner_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    runtime_grant_id: &str,
    runtime_sha256: &str,
) -> String {
    let enrollment_epoch = enrollment_epoch.to_string();
    runner_volume_http_payload_sha256(
        "/api/jobs/internal/execution-leases/claim",
        worker_id,
        &[
            ("account_id", account_id),
            ("application_id", application_id),
            ("run_id", run_id),
            ("browser_profile_id", browser_profile_id),
            ("owner_id", owner_id),
            ("volume_id", volume_id),
            ("enrollment_epoch", &enrollment_epoch),
            ("process_instance_id", process_instance_id),
            ("runtime_grant_id", runtime_grant_id),
            ("runtime_sha256", runtime_sha256),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn signed_execution_claim_body(
    fixture: &SignedRunnerVolumeFixture,
    runtime: &SignedRunnerProcessRuntimeFixture,
    signing_key: &Ed25519SigningKey,
    worker_id: &str,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    owner_id: &str,
    request_id: &str,
) -> serde_json::Value {
    let issued_at_ms = chrono::Utc::now().timestamp_millis();
    let payload_sha256 = execution_claim_payload_sha256(
        worker_id,
        account_id,
        application_id,
        run_id,
        browser_profile_id,
        owner_id,
        &fixture.volume_id,
        fixture.enrollment_epoch,
        &fixture.process_instance_id,
        &runtime.grant.grant_id,
        &runtime.runtime_sha256,
    );
    let volume_proof = jobs::sign_runner_volume_authority_proof(
        signing_key,
        jobs::NewRunnerVolumeAuthorityProof {
            operation: "execution_lease_claim".to_string(),
            request_id: request_id.to_string(),
            volume_id: fixture.volume_id.clone(),
            enrollment_epoch: fixture.enrollment_epoch,
            process_instance_id: fixture.process_instance_id.clone(),
            issued_at_ms,
            payload_sha256,
        },
    )
    .unwrap();
    json!({
        "account_id": account_id,
        "application_id": application_id,
        "run_id": run_id,
        "browser_profile_id": browser_profile_id,
        "owner_id": owner_id,
        "volume_id": fixture.volume_id,
        "enrollment_epoch": fixture.enrollment_epoch,
        "process_instance_id": fixture.process_instance_id,
        "runtime_grant_id": runtime.grant.grant_id,
        "runtime_sha256": runtime.runtime_sha256,
        "volume_proof": volume_proof
    })
}

async fn setup_signed_runner_volume(
    harness: &Harness,
    worker_signing_key: &str,
    worker_timestamp: u64,
) -> (SignedRunnerVolumeFixture, SignedRunnerProcessRuntimeFixture) {
    const WORKER_ID: &str = "signed-execution-worker";
    let signing_key = Ed25519SigningKey::from_bytes(&[61_u8; 32]);
    let public_key_base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(signing_key.verifying_key().to_bytes());
    let volume_id = jobs::runner_volume_id_from_public_key(&public_key_base64url).unwrap();
    let key_fingerprint = jobs::runner_volume_key_fingerprint(&public_key_base64url).unwrap();
    let process_instance_id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([62_u8; 32]);
    let grant_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([63_u8; 32]);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let resource_fingerprint = hex::encode(Sha256::digest(b"phase-602-http-volume"));
    let grant_id = "runner-volume-grant-http-602";
    jobs::create_runner_volume_admission_grant(
        &harness.pool,
        &jobs::NewRunnerVolumeAdmissionGrant {
            grant_id: grant_id.to_string(),
            token: grant_token.clone(),
            expected_worker_id: WORKER_ID.to_string(),
            provider: "integration".to_string(),
            provider_resource_id: "phase-602-http-volume".to_string(),
            resource_fingerprint: resource_fingerprint.clone(),
            authorization_ref: "phase-602-http-admission".to_string(),
            created_by: "phase-602-integration-admin".to_string(),
            expires_at_ms: now_ms + 600_000,
            created_at_ms: now_ms,
        },
    )
    .unwrap();
    let enrollment_proof = jobs::sign_runner_volume_enrollment_proof(
        &signing_key,
        jobs::NewRunnerVolumeEnrollmentProof {
            admission_grant_id: grant_id.to_string(),
            volume_id: volume_id.clone(),
            worker_id: WORKER_ID.to_string(),
            provider: "integration".to_string(),
            provider_resource_id: "phase-602-http-volume".to_string(),
            resource_fingerprint: resource_fingerprint.clone(),
            enrollment_epoch: 1,
            public_key_base64url,
            key_fingerprint: key_fingerprint.clone(),
            legacy_artifact_count: 0,
            requested_at_ms: now_ms,
        },
    )
    .unwrap();
    let enrollment = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/runner-volumes/enroll",
            "runner-volume",
            WORKER_ID,
            worker_timestamp,
            "runner-volume-enrollment-0001",
            &json!({
                "grantToken": grant_token,
                "proof": enrollment_proof
            }),
            worker_signing_key,
        ))
        .await
        .unwrap();
    let enrollment_status = enrollment.status();
    let enrollment_body = axum::body::to_bytes(enrollment.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        enrollment_status,
        StatusCode::OK,
        "runner-volume enrollment failed: {}",
        String::from_utf8_lossy(&enrollment_body)
    );
    let enrolled: serde_json::Value = serde_json::from_slice(&enrollment_body).unwrap();
    assert_eq!(enrolled["volumeId"], volume_id);
    assert_eq!(enrolled["status"], "reconciling");

    let runtime_grant = jobs::RunnerProcessRuntimeGrantClaim {
        grant_id: "runner-process-runtime-grant-http-604".to_string(),
        grant_token: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([64_u8; 32]),
        runtime: jobs::RunnerProcessRuntimeAttestation {
            runner_image_sha256: hex::encode(Sha256::digest(b"phase-604-http-runner-image")),
            runner_build_id: "runner-604-integration".to_string(),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: hex::encode(Sha256::digest(
                b"phase-604-http-automation-bundle",
            )),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "123456".to_string(),
            chromium_executable_sha256: hex::encode(Sha256::digest(
                b"phase-604-http-chromium-executable",
            )),
        },
    };
    let stored_runtime_grant = jobs::create_runner_process_runtime_grant(
        &harness.pool,
        &jobs::NewRunnerProcessRuntimeGrant {
            grant_id: runtime_grant.grant_id.clone(),
            token: runtime_grant.grant_token.clone(),
            expected_worker_id: WORKER_ID.to_string(),
            runtime: runtime_grant.runtime.clone(),
            authorization_ref: "phase-604-http-runtime-admission".to_string(),
            created_by: "phase-604-integration-admin".to_string(),
            expires_at_ms: now_ms + 600_000,
            created_at_ms: now_ms,
        },
    )
    .unwrap();
    let runtime_sha256 = jobs::runner_process_runtime_sha256(&runtime_grant.runtime).unwrap();
    assert_eq!(stored_runtime_grant.runtime_sha256, runtime_sha256);

    let fixture = SignedRunnerVolumeFixture {
        signing_key,
        worker_id: WORKER_ID.to_string(),
        volume_id,
        enrollment_epoch: 1,
        process_instance_id,
        key_fingerprint,
        resource_fingerprint,
    };
    let runtime_fixture = SignedRunnerProcessRuntimeFixture {
        grant: runtime_grant,
        runtime_sha256,
    };
    let instance_path = format!(
        "/api/jobs/internal/runner-volumes/{}/instances/claim",
        fixture.volume_id
    );
    let instance_proof = signed_runner_volume_proof(
        &fixture,
        "instance_claim",
        "runner-volume-instance-claim-0001",
        chrono::Utc::now().timestamp_millis(),
        runner_volume_instance_claim_payload_sha256(
            &instance_path,
            WORKER_ID,
            &runtime_fixture.grant,
            &runtime_fixture.runtime_sha256,
        ),
    );
    let instance = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            &instance_path,
            "runner-volume",
            WORKER_ID,
            worker_timestamp,
            "runner-volume-instance-http-0001",
            &json!({
            "proof": instance_proof,
            "runtimeGrant": runtime_fixture.grant,
            }),
            worker_signing_key,
        ))
        .await
        .unwrap();
    let instance_status = instance.status();
    let instance_body = axum::body::to_bytes(instance.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        instance_status,
        StatusCode::OK,
        "runner-volume instance claim failed: {}",
        String::from_utf8_lossy(&instance_body)
    );
    let instance_lease: serde_json::Value = serde_json::from_slice(&instance_body).unwrap();
    assert_eq!(instance_lease["volumeId"], fixture.volume_id);
    assert_eq!(
        instance_lease["processInstanceId"],
        fixture.process_instance_id
    );
    assert_eq!(
        instance_lease["runtimeGrantId"],
        runtime_fixture.grant.grant_id
    );
    assert_eq!(
        instance_lease["runtimeSha256"],
        runtime_fixture.runtime_sha256
    );

    let poll_path = format!(
        "/api/jobs/internal/runner-volumes/{}/commands/poll",
        fixture.volume_id
    );
    let poll_proof = signed_runner_volume_proof(
        &fixture,
        "purge_poll",
        "runner-volume-command-poll-0001",
        chrono::Utc::now().timestamp_millis(),
        runner_volume_http_payload_sha256(
            &poll_path,
            WORKER_ID,
            &[("after_command_id", ""), ("limit", "100")],
        ),
    );
    let poll = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            &poll_path,
            "runner-volume",
            WORKER_ID,
            worker_timestamp,
            "runner-volume-command-http-0001",
            &json!({ "afterCommandId": null, "proof": poll_proof, "limit": 100 }),
            worker_signing_key,
        ))
        .await
        .unwrap();
    let poll_status = poll.status();
    let poll_body = axum::body::to_bytes(poll.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        poll_status,
        StatusCode::OK,
        "runner-volume command poll failed: {}",
        String::from_utf8_lossy(&poll_body)
    );
    let poll_result: serde_json::Value = serde_json::from_slice(&poll_body).unwrap();
    assert_eq!(poll_result["commands"], json!([]));
    assert_eq!(poll_result["ready"], false);
    assert_eq!(poll_result["storageAttestationRequired"], true);

    let attestation_observed_at_ms = chrono::Utc::now().timestamp_millis();
    let attestation = jobs::sign_runner_volume_storage_attestation(
        &fixture.signing_key,
        jobs::NewRunnerVolumeStorageAttestation {
            attestation_id: "integration-storage-attestation-1".to_string(),
            volume_id: fixture.volume_id.clone(),
            volume_key_fingerprint: fixture.key_fingerprint.clone(),
            resource_fingerprint: fixture.resource_fingerprint.clone(),
            enrollment_epoch: fixture.enrollment_epoch,
            enrollment_generation: poll_result["enrollmentGeneration"].as_i64().unwrap(),
            process_instance_id: fixture.process_instance_id.clone(),
            predecessor_attestation_generation: poll_result["predecessorAttestationGeneration"]
                .as_i64()
                .unwrap(),
            predecessor_attestation_sha256: poll_result["predecessorAttestationSha256"]
                .as_str()
                .unwrap()
                .to_string(),
            required_tombstone_generation: poll_result["requiredTombstoneGeneration"]
                .as_i64()
                .unwrap(),
            reconciled_tombstone_generation: poll_result["reconciledTombstoneGeneration"]
                .as_i64()
                .unwrap(),
            storage_evidence_version: jobs::RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            subject_storage_layout_version: jobs::RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
            root_device_id: "unix:602:integration-device".to_string(),
            root_link_count: 7,
            root_entry_count: 0,
            root_file_bytes: "0".to_string(),
            root_sha256: jobs::EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
            subject_storage_subject_count: 0,
            subject_storage_subject_set_sha256: jobs::EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                .to_string(),
            subject_storage_scope_count: 0,
            subject_storage_complete_root_entry_count: 0,
            subject_storage_complete_root_file_bytes: "0".to_string(),
            subject_storage_complete_root_sha256:
                jobs::EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256.to_string(),
            locator_count: 0,
            resident_locator_count: 0,
            locator_set_sha256: jobs::EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
            legacy_inventory_version: jobs::RUNNER_LEGACY_INVENTORY_VERSION,
            legacy_artifact_count: 0,
            legacy_artifact_bytes: "0".to_string(),
            legacy_artifact_set_sha256: jobs::EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256.to_string(),
            unclassified_root_count: 0,
            runner_build_id: "runner-602".to_string(),
            observed_at_ms: attestation_observed_at_ms,
        },
    )
    .unwrap();
    let attestation_sha256 = attestation.attestation_sha256().unwrap();
    let attestation_path = format!(
        "/api/jobs/internal/runner-volumes/{}/storage-attestations",
        fixture.volume_id
    );
    let attestation_proof = signed_runner_volume_proof(
        &fixture,
        "storage_attestation",
        "runner-volume-storage-attestation-0001",
        attestation_observed_at_ms,
        runner_volume_http_payload_sha256(
            &attestation_path,
            WORKER_ID,
            &[("attestation_sha256", attestation_sha256.as_str())],
        ),
    );
    let attestation_response = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            &attestation_path,
            "runner-volume",
            WORKER_ID,
            worker_timestamp,
            "runner-volume-storage-attestation-http-0001",
            &json!({ "proof": attestation_proof, "attestation": attestation }),
            worker_signing_key,
        ))
        .await
        .unwrap();
    let attestation_status = attestation_response.status();
    let attestation_body = axum::body::to_bytes(attestation_response.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        attestation_status,
        StatusCode::OK,
        "runner-volume storage attestation failed: {}",
        String::from_utf8_lossy(&attestation_body)
    );
    let attestation_result: serde_json::Value = serde_json::from_slice(&attestation_body).unwrap();
    assert_eq!(attestation_result["volumeStatus"], "active");
    assert_eq!(attestation_result["attestationSha256"], attestation_sha256);

    let fleet = authorize_empty_legacy_runner_inventory(
        &harness.pool,
        "phase-602-signed-runner-volume-inventory",
    );
    assert_eq!(fleet.non_destroyed_volume_count, 1);
    assert_eq!(fleet.attested_reconciled_volume_count, 1);
    assert_eq!(fleet.unresolved_legacy_volume_count, 0);
    let cutover_at_ms = chrono::Utc::now().timestamp_millis();
    let cutover = |cutover_state: &str, now_ms: i64| jobs::RecordRunnerVolumeFleetCutoverRequest {
        cutover_state: cutover_state.to_string(),
        expected_enrollment_generation: fleet.enrollment_generation,
        expected_purge_generation: fleet.purge_generation,
        expected_tombstone_generation: fleet.tombstone_generation,
        expected_destruction_generation: fleet.destruction_generation,
        expected_legacy_reconciliation_generation: fleet.legacy_reconciliation_generation,
        expected_storage_attestation_generation: fleet.storage_attestation_generation,
        expected_storage_attestation_count: fleet.storage_attestation_count,
        expected_storage_attestation_set_sha256: fleet.storage_attestation_set_sha256.clone(),
        expected_legacy_inventory_generation: fleet.legacy_inventory_generation,
        expected_legacy_inventory_reconciliation_id: fleet
            .legacy_inventory_reconciliation_id
            .clone()
            .unwrap(),
        expected_legacy_inventory_authority_id: fleet
            .legacy_inventory_authority_id
            .clone()
            .unwrap(),
        expected_legacy_inventory_authority_sha256: fleet
            .legacy_inventory_authority_sha256
            .clone()
            .unwrap(),
        expected_legacy_inventory_root_count: fleet.legacy_inventory_root_count.unwrap(),
        expected_legacy_inventory_root_set_sha256: fleet
            .legacy_inventory_root_set_sha256
            .clone()
            .unwrap(),
        expected_non_destroyed_volume_count: fleet.non_destroyed_volume_count,
        expected_destruction_count: fleet.destruction_count,
        expected_unresolved_legacy_volume_count: fleet.unresolved_legacy_volume_count,
        evidence_ref: "phase-602-http-cutover".to_string(),
        evidence_sha256: hex::encode(Sha256::digest(b"phase-602-http-cutover")),
        authorized_by: "phase-602-integration-admin".to_string(),
        cutover_at_ms,
        now_ms,
    };
    jobs::record_runner_volume_fleet_cutover(&harness.pool, &cutover("reconciling", cutover_at_ms))
        .unwrap();
    jobs::record_runner_volume_fleet_cutover(&harness.pool, &cutover("ready", cutover_at_ms + 1))
        .unwrap();
    let ready = jobs::runner_volume_fleet_status(&harness.pool).unwrap();
    assert_eq!(ready.cutover_state, "ready");
    assert_eq!(
        ready.cutover_enrollment_generation,
        Some(fleet.enrollment_generation)
    );

    (fixture, runtime_fixture)
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
async fn ats_certification_routes_isolate_admins_workers_paths_and_bodies() {
    const SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef";
    const WORKER_PATH: &str = "/api/jobs/internal/ats-certifications/layout-observations";
    const ADMIN_LAYOUT_PATH: &str = "/admin/jobs/ats-certifications/layout-observations";
    const ADMIN_MANIFEST_PATH: &str = "/admin/jobs/ats-certifications/manifests";
    std::env::set_var("BLUEY_JOBS_WORKER_SIGNING_KEY", SIGNING_KEY);
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", "legacy-debug-token");
    let harness = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec!["ats-admin@bluey.sh".to_string()],
    )
    .await;
    let admin_access = signup_and_login(&harness, "ats-admin@bluey.sh", "valid-password-123").await;
    let customer_access =
        signup_and_login(&harness, "ats-customer@bluey.sh", "valid-password-123").await;
    assert_eq!(ats_certification_mutation_count(&harness.pool), 0);

    let unsigned_envelope = json!({
        "canonicalBase64url": "AA",
        "authorizationBase64url": "AA",
    });
    let unsigned_bytes = serde_json::to_vec(&unsigned_envelope).unwrap();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for (index, router) in [harness.router.clone(), harness.jobs_router.clone()]
        .into_iter()
        .enumerate()
    {
        let nonce = format!("abcdef0123456789abcdef01234568{index:02}");
        let accepted_transport = router
            .clone()
            .oneshot(signed_worker_json_request(
                WORKER_PATH,
                "ats-layout-observation",
                "ats-integration-worker",
                now_secs,
                &nonce,
                &unsigned_envelope,
                SIGNING_KEY,
            ))
            .await
            .unwrap();
        assert_eq!(accepted_transport.status(), StatusCode::NOT_FOUND);
        let accepted_body = axum::body::to_bytes(accepted_transport.into_body(), 4 * 1024)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(accepted_body.to_vec()).unwrap(),
            "The ATS certification authority was not found."
        );

        let replayed = router
            .clone()
            .oneshot(signed_worker_json_request(
                WORKER_PATH,
                "ats-layout-observation",
                "ats-integration-worker",
                now_secs,
                &nonce,
                &unsigned_envelope,
                SIGNING_KEY,
            ))
            .await
            .unwrap();
        assert_eq!(replayed.status(), StatusCode::UNAUTHORIZED);
    }

    let tampered = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: WORKER_PATH,
            scope: "ats-layout-observation",
            worker_id: "ats-integration-worker",
            timestamp: now_secs,
            nonce: "abcdef0123456789abcdef0123456810",
            signed_body: &unsigned_bytes,
            actual_body: br#"{"canonicalBase64url":"forged"}"#,
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(tampered.status(), StatusCode::UNAUTHORIZED);

    let mut wrong_path = signed_worker_request(SignedWorkerRequest {
        path: "/api/jobs/internal/ats-certifications/layout-observations/extra",
        scope: "ats-layout-observation",
        worker_id: "ats-integration-worker",
        timestamp: now_secs,
        nonce: "abcdef0123456789abcdef0123456811",
        signed_body: &unsigned_bytes,
        actual_body: &unsigned_bytes,
        signing_key: SIGNING_KEY,
    });
    *wrong_path.uri_mut() = WORKER_PATH.parse().unwrap();
    let wrong_path = harness
        .jobs_router
        .clone()
        .oneshot(wrong_path)
        .await
        .unwrap();
    assert_eq!(wrong_path.status(), StatusCode::UNAUTHORIZED);

    let wrong_scope = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            WORKER_PATH,
            "discovery",
            "ats-integration-worker",
            now_secs,
            "abcdef0123456789abcdef0123456812",
            &unsigned_envelope,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(wrong_scope.status(), StatusCode::UNAUTHORIZED);

    let unknown_field = json!({
        "canonicalBase64url": "AA",
        "authorizationBase64url": "AA",
        "accountId": "must-not-be-accepted",
    });
    let unknown_field = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            WORKER_PATH,
            "ats-layout-observation",
            "ats-integration-worker",
            now_secs,
            "abcdef0123456789abcdef0123456813",
            &unknown_field,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(unknown_field.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let oversized_body = vec![b'x'; 256 * 1024 + 1];
    let oversized = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: WORKER_PATH,
            scope: "ats-layout-observation",
            worker_id: "ats-integration-worker",
            timestamp: now_secs,
            nonce: "abcdef0123456789abcdef0123456814",
            signed_body: &oversized_body,
            actual_body: &oversized_body,
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::UNAUTHORIZED);

    for authorization in [
        format!("Bearer {customer_access}"),
        format!("Bearer {admin_access}"),
        "Bearer legacy-debug-token".to_string(),
    ] {
        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post(WORKER_PATH)
                    .header("authorization", authorization)
                    .header("content-type", "application/json")
                    .body(Body::from(unsigned_bytes.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let customer_admin_write = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(ADMIN_MANIFEST_PATH)
                .header("authorization", format!("Bearer {customer_access}"))
                .header("content-type", "application/json")
                .body(Body::from(unsigned_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer_admin_write.status(), StatusCode::FORBIDDEN);

    let worker_admin_write = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_request(SignedWorkerRequest {
            path: ADMIN_LAYOUT_PATH,
            scope: "ats-layout-observation",
            worker_id: "ats-integration-worker",
            timestamp: now_secs,
            nonce: "abcdef0123456789abcdef0123456815",
            signed_body: &unsigned_bytes,
            actual_body: &unsigned_bytes,
            signing_key: SIGNING_KEY,
        }))
        .await
        .unwrap();
    assert_eq!(worker_admin_write.status(), StatusCode::UNAUTHORIZED);

    let aggregate = json!({
        "manifest": unsigned_envelope,
        "evidence": [],
    });
    for router in [harness.router.clone(), harness.jobs_router.clone()] {
        let admin_manifest = router
            .oneshot(
                Request::post(ADMIN_MANIFEST_PATH)
                    .header("authorization", format!("Bearer {admin_access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&aggregate).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_manifest.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(admin_manifest.into_body(), 4 * 1024)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(body.to_vec()).unwrap(),
            "The ATS certification authority was not found."
        );
    }

    let aggregate_unknown_field = json!({
        "manifest": {
            "canonicalBase64url": "AA",
            "authorizationBase64url": "AA",
            "accountId": "must-not-be-accepted",
        },
        "evidence": [],
    });
    let aggregate_unknown_field = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(ADMIN_MANIFEST_PATH)
                .header("authorization", format!("Bearer {admin_access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&aggregate_unknown_field).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        aggregate_unknown_field.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let oversized_admin_body = vec![b'x'; 256 * 1024 + 1];
    let oversized_admin = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(ADMIN_MANIFEST_PATH)
                .header("authorization", format!("Bearer {admin_access}"))
                .header("content-type", "application/json")
                .body(Body::from(oversized_admin_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_admin.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let now_ms = (now_secs as i64) * 1_000;
    let target_status_path = format!(
        "/admin/jobs/ats-certifications/targets/greenhouse:acme:123/status?\
         canonicalUrl=https%3A%2F%2Fboards.greenhouse.io%2Facme%2Fjobs%2F123&\
         discoveryProvider=greenhouse&discoveryTargetKey=greenhouse%3Aacme%3A123&\
         discoveryObservedAtMs={now_ms}&originalSourceProvider=greenhouse&\
         originalSourceTargetKey=greenhouse%3Aacme%3A123&\
         originalSourceObservedAtMs={now_ms}&channel=general"
    )
    .replace(' ', "");
    for router in [harness.router.clone(), harness.jobs_router.clone()] {
        let customer_status = router
            .clone()
            .oneshot(
                Request::get(&target_status_path)
                    .header("authorization", format!("Bearer {customer_access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(customer_status.status(), StatusCode::FORBIDDEN);

        let admin_status = router
            .clone()
            .oneshot(
                Request::get(&target_status_path)
                    .header("authorization", format!("Bearer {admin_access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_status.status(), StatusCode::OK);
        let body = axum::body::to_bytes(admin_status.into_body(), 16 * 1024)
            .await
            .unwrap();
        let status: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status["schemaVersion"], 1);
        assert_eq!(status["provider"], "greenhouse");
        assert_eq!(status["status"], "review_only");
        assert_eq!(status["canaryAvailable"], false);
        assert!(status["targetKeySha256"].as_str().unwrap().len() == 64);
        assert!(status.get("targetKey").is_none());
        assert!(status.get("accountId").is_none());
        assert!(status.get("evidenceSha256s").is_none());
    }

    let unknown_query = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(format!("{target_status_path}&accountId=forbidden"))
                .header("authorization", format!("Bearer {admin_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_query.status(), StatusCode::BAD_REQUEST);

    let path_mismatch = target_status_path.replace(
        "targets/greenhouse:acme:123/status",
        "targets/greenhouse:other:123/status",
    );
    let path_mismatch = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(path_mismatch)
                .header("authorization", format!("Bearer {admin_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(path_mismatch.status(), StatusCode::NOT_FOUND);

    assert_eq!(ats_certification_mutation_count(&harness.pool), 0);
    let ats_audit_count: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM ops_audit_events
              WHERE event_type LIKE 'jobs_ats_certification_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ats_audit_count, 0);
    std::env::remove_var("BLUEY_JOBS_WORKER_SIGNING_KEY");
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
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
    let _rate_limit = TestEnvironmentGuard::install(&[
        ("BLUEY_LIMIT_JOBS_RUN_PER_MIN", "1".to_string()),
        ("BLUEY_LIMIT_JOBS_RUN_PER_MIN_BURST", "1".to_string()),
    ]);
    let harness = boot_harness().await;
    let _distribution =
        enable_local_browser_distribution_for_test(&harness.pool, "claim-rate-limit");
    let ticket = "a".repeat(64);
    let request = || {
        Request::post("/api/jobs/local-runs/missing-run/claim")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&local_browser_claim_body("missing-run", &ticket)).unwrap(),
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
async fn jobs_selected_curated_verification_refuses_unowned_or_unsupported_requests() {
    const FLAG: &str = "BLUEY_JOBS_SELECTED_CURATED_VERIFICATION_ENABLED";
    let _off = TestEnvironmentGuard::install(&[(FLAG, "0".to_string())]);
    let harness = boot_harness().await;
    let email = "curated-route-owner@example.com";
    let owner_token = signup_and_login(&harness, email, "valid-password-123").await;
    let foreign_token = signup_and_login(
        &harness,
        "curated-route-foreign@example.com",
        "valid-password-123",
    )
    .await;
    let owner = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .unwrap();
    let identity =
        jobs::ensure_primary_application_identity(&harness.pool, &owner.id, email).unwrap();
    jobs::upsert_track(
        &harness.pool,
        &owner.id,
        &jobs::CareerTrack {
            id: "selected-route-track".to_string(),
            name: "Synthetic engineering search".to_string(),
            role: "Software Engineer".to_string(),
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
    let source = jobs::ensure_managed_curated_discovery_source(&harness.pool, &owner.id)
        .unwrap()
        .unwrap();
    let lease = jobs::lease_due_discovery_source(&harness.pool, "selected-route-fixture")
        .unwrap()
        .unwrap();
    assert_eq!(lease.source.id, source.id);
    jobs::complete_discovery_run(
        &harness.pool,
        &source.id,
        &lease.lease_token,
        &lease.replay_key,
        lease.scheduled_for_ms,
        &[jobs::DiscoveredJobInput {
            external_id: "selected-route-feed-record".to_string(),
            canonical_url: "https://careers.example.invalid/fictional-posting".to_string(),
            title: "Software Engineer".to_string(),
            company: "Fictional Employer Fixture".to_string(),
            source_catalog_id: "feed-simplify-new-grad".to_string(),
            requires_original_revalidation: true,
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            description: String::new(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            engagement_type: "w2".to_string(),
            posted_at_ms: None,
        }],
        true,
    )
    .unwrap();
    let original = jobs::list_postings(&harness.pool, &owner.id)
        .unwrap()
        .remove(0);
    let endpoint = format!("/api/jobs/matches/{}/verify-original", original.id);
    let selected_request =
        |router: axum::Router, url: &str, bearer: Option<&str>, content: Body| {
            let mut builder = Request::post(url);
            if let Some(bearer) = bearer {
                builder = builder.header("authorization", format!("Bearer {bearer}"));
            }
            async move {
                router
                    .oneshot(builder.body(content).unwrap())
                    .await
                    .unwrap()
            }
        };
    let disabled = selected_request(
        harness.jobs_router.clone(),
        &endpoint,
        Some(&owner_token),
        Body::empty(),
    )
    .await;
    assert_eq!(disabled.status(), StatusCode::NOT_FOUND);
    let _enabled = TestEnvironmentGuard::install(&[(FLAG, "1".to_string())]);
    let unauthorized =
        selected_request(harness.jobs_router.clone(), &endpoint, None, Body::empty()).await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    for (url, content, expected) in [
        (
            format!("{endpoint}?canonical_url=fixture"),
            Body::empty(),
            StatusCode::BAD_REQUEST,
        ),
        (endpoint.clone(), Body::from("x"), StatusCode::BAD_REQUEST),
        (
            endpoint.clone(),
            Body::from("{}"),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            endpoint.clone(),
            Body::empty(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let response = selected_request(
            harness.jobs_router.clone(),
            &url,
            Some(&owner_token),
            content,
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    let foreign = selected_request(
        harness.jobs_router.clone(),
        &endpoint,
        Some(&foreign_token),
        Body::empty(),
    )
    .await;
    let missing = selected_request(
        harness.jobs_router.clone(),
        "/api/jobs/matches/missing-fixture/verify-original",
        Some(&foreign_token),
        Body::empty(),
    )
    .await;
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(foreign.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        body,
        axum::body::to_bytes(missing.into_body(), 1024)
            .await
            .unwrap()
    );
    assert_eq!(
        body,
        axum::body::to_bytes(disabled.into_body(), 1024)
            .await
            .unwrap()
    );

    let current = jobs::list_postings(&harness.pool, &owner.id).unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(
        serde_json::to_value(&current[0]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    let sources = jobs::list_discovery_sources(&harness.pool, &owner.id).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].id, source.id);
    assert!(jobs::list_applications(&harness.pool, &owner.id)
        .unwrap()
        .is_empty());
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

fn jobs_policy_write_counts(pool: &DbPool, account_id: &str) -> (i64, i64, i64, i64) {
    let conn = pool.get().unwrap();
    conn.query_row(
        "SELECT
            (SELECT COUNT(*) FROM jobs_profiles WHERE account_id = ?1),
            (SELECT COUNT(*) FROM jobs_preferences WHERE account_id = ?1),
            (SELECT COUNT(*) FROM jobs_tracks WHERE account_id = ?1),
            (SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1)",
        rusqlite::params![account_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .unwrap()
}

#[tokio::test]
#[serial]
async fn jobs_taxonomy_http_contract_authenticates_and_fences_track_mutations() {
    let harness = boot_harness().await;
    let email = "jobs-taxonomy-contract@example.com";
    let access = signup_and_login(&harness, email, "valid-password-123").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .unwrap();

    let unauthorized = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/taxonomy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let taxonomy = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/taxonomy")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(taxonomy.status(), StatusCode::OK);
    let taxonomy_body = axum::body::to_bytes(taxonomy.into_body(), 256 * 1024)
        .await
        .unwrap();
    let taxonomy: serde_json::Value = serde_json::from_slice(&taxonomy_body).unwrap();
    let taxonomy_version = taxonomy["taxonomyVersion"].as_str().unwrap();
    let taxonomy_sha256 = taxonomy["taxonomySha256"].as_str().unwrap();
    assert_eq!(
        taxonomy_version,
        bluey_server::jobs_taxonomy::taxonomy_version()
    );
    assert_eq!(
        taxonomy_sha256,
        bluey_server::jobs_taxonomy::taxonomy_sha256()
    );
    assert_eq!(
        taxonomy["registry"]["taxonomy_version"],
        taxonomy["taxonomyVersion"]
    );
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        (0, 0, 0, 0)
    );

    let valid_track = json!({
        "id": "phase-613-http-track",
        "name": "Software engineering",
        "role": "Software Engineer",
        "locations": ["New York, NY"],
        "remote_preference": "hybrid_ok",
        "policy": {
            "employment_types": ["full_time"]
        },
        "active": true
    });
    let missing_binding = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/tracks")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&valid_track).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_binding.status(), StatusCode::CONFLICT);
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        (0, 0, 0, 0)
    );

    let stale_binding = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/tracks")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .header("x-bluey-jobs-taxonomy-version", taxonomy_version)
                .header("x-bluey-jobs-taxonomy-sha256", "0".repeat(64))
                .body(Body::from(serde_json::to_vec(&valid_track).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_binding.status(), StatusCode::CONFLICT);
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        (0, 0, 0, 0)
    );

    for (field, value) in [("remote_preference", "Remote or hybrid"), ("role", "PM")] {
        let mut invalid_track = valid_track.clone();
        invalid_track[field] = json!(value);
        let invalid = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post("/api/jobs/tracks")
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .header("x-bluey-jobs-taxonomy-version", taxonomy_version)
                    .header("x-bluey-jobs-taxonomy-sha256", taxonomy_sha256)
                    .body(Body::from(serde_json::to_vec(&invalid_track).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            jobs_policy_write_counts(&harness.pool, &account.id),
            (0, 0, 0, 0)
        );
    }

    let created = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/tracks")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .header("x-bluey-jobs-taxonomy-version", taxonomy_version)
                .header("x-bluey-jobs-taxonomy-sha256", taxonomy_sha256)
                .body(Body::from(serde_json::to_vec(&valid_track).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created_body = axum::body::to_bytes(created.into_body(), 128 * 1024)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&created_body).unwrap();
    assert_eq!(created["id"], "phase-613-http-track");
    assert_eq!(created["role"], "Software Engineer");
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        (0, 0, 1, 1)
    );

    let mut update = valid_track.clone();
    update["name"] = json!("Changed without current taxonomy");
    let stale_update = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::put("/api/jobs/tracks/phase-613-http-track")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .header("x-bluey-jobs-taxonomy-version", taxonomy_version)
                .header("x-bluey-jobs-taxonomy-sha256", "0".repeat(64))
                .body(Body::from(serde_json::to_vec(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_update.status(), StatusCode::CONFLICT);
    assert_eq!(
        jobs::list_tracks(&harness.pool, &account.id).unwrap()[0].name,
        "Software engineering"
    );

    update["name"] = json!("Platform engineering");
    let updated = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::put("/api/jobs/tracks/phase-613-http-track")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .header("x-bluey-jobs-taxonomy-version", taxonomy_version)
                .header("x-bluey-jobs-taxonomy-sha256", taxonomy_sha256)
                .body(Body::from(serde_json::to_vec(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        jobs::list_tracks(&harness.pool, &account.id).unwrap()[0].name,
        "Platform engineering"
    );

    let before_invalid_onboarding = jobs_policy_write_counts(&harness.pool, &account.id);
    let invalid_onboarding = json!({
        "profile": {
            "full_name": "Taylor Rivera",
            "current_location": "New York, NY",
            "education": [{ "school": "State University" }]
        },
        "preferences": {
            "location_policy": "ask",
            "daily_limit": 10,
            "time_zone_offset_minutes": 0
        },
        "track": {
            "id": "phase-613-invalid-onboarding-track",
            "name": "Ambiguous product role",
            "role": "TPM",
            "locations": ["New York, NY"],
            "remote_preference": "remote_or_hybrid",
            "policy": { "employment_types": ["full_time"] },
            "active": true
        }
    });
    let invalid_onboarding = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/onboarding/complete")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .header("x-bluey-jobs-taxonomy-version", taxonomy_version)
                .header("x-bluey-jobs-taxonomy-sha256", taxonomy_sha256)
                .body(Body::from(serde_json::to_vec(&invalid_onboarding).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_onboarding.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        before_invalid_onboarding
    );
}

#[tokio::test]
#[serial]
async fn jobs_track_write_is_retry_safe_when_curated_source_enrollment_fails_late() {
    let harness = boot_harness().await;
    let email = "jobs-track-late-enrollment@example.com";
    let access = signup_and_login(&harness, email, "valid-password-123").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .unwrap();
    harness
        .pool
        .get()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER test_reject_curated_source
             BEFORE INSERT ON jobs_discovery_sources
             WHEN NEW.provider = 'curated_feed'
             BEGIN
               SELECT RAISE(ABORT, 'injected curated source enrollment failure');
             END;",
        )
        .unwrap();
    let track = json!({
        "id": "phase-613-retry-safe-track",
        "name": "Software engineering",
        "role": "Software Engineer",
        "locations": ["New York, NY"],
        "remote_preference": "hybrid_ok",
        "policy": { "employment_types": ["full_time"] },
        "active": true
    });

    for _ in 0..2 {
        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post("/api/jobs/tracks")
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .header(
                        "x-bluey-jobs-taxonomy-version",
                        bluey_server::jobs_taxonomy::taxonomy_version(),
                    )
                    .header(
                        "x-bluey-jobs-taxonomy-sha256",
                        bluey_server::jobs_taxonomy::taxonomy_sha256(),
                    )
                    .body(Body::from(serde_json::to_vec(&track).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        (0, 0, 1, 0),
        "a retry must reuse the stable Track ID without duplicating policy state",
    );

    harness
        .pool
        .get()
        .unwrap()
        .execute_batch("DROP TRIGGER test_reject_curated_source;")
        .unwrap();
    let workspace = harness
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
    assert_eq!(workspace.status(), StatusCode::OK);
    assert_eq!(
        jobs_policy_write_counts(&harness.pool, &account.id),
        (0, 0, 1, 1),
        "the next safe workspace read repairs managed source enrollment",
    );
}

#[tokio::test]
#[serial]
async fn public_beta_status_and_customer_gate_have_shared_standalone_parity() {
    let harness = boot_harness().await;
    let access = signup_and_login(
        &harness,
        "jobs-public-beta-parity@example.com",
        "valid-password-123",
    )
    .await;

    for router in [&harness.router, &harness.jobs_router] {
        let unauthenticated = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/beta-access")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            unauthenticated.headers()[axum::http::header::CACHE_CONTROL],
            "private, no-store, max-age=0"
        );
        assert_eq!(
            unauthenticated.headers()[axum::http::header::PRAGMA],
            "no-cache"
        );
        assert!(unauthenticated
            .headers()
            .get_all(axum::http::header::VARY)
            .iter()
            .any(|value| value == "Authorization"));

        let status = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/beta-access")
                    .header("authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status.status(), StatusCode::OK);
        assert_eq!(
            response_json(status).await,
            json!({
                "schemaVersion": 1,
                "access": "admitted",
                "reason": "admitted"
            })
        );

        let workspace = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/workspace")
                    .header("authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(workspace.status(), StatusCode::OK);
    }
}

#[tokio::test]
#[serial]
async fn public_beta_requires_verified_non_temporary_accounts_and_master_flag() {
    let harness = boot_harness().await;
    let password = "valid-password-123";
    let password_hash = auth::password::hash_password(password).unwrap();
    let unverified = Account::create(
        &harness.pool,
        "jobs-public-beta-unverified@example.com",
        &password_hash,
    )
    .unwrap();
    // Login correctly rejects an unverified account, so mint a test-only
    // access token to exercise the authorization layer's independent
    // eligibility check.
    let unverified_access = auth::jwt::issue(
        "test-secret-at-least-32-chars-long-xxx",
        &unverified.id,
        auth::jwt::TokenKind::Access,
    )
    .unwrap();

    let verification = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/beta-access")
                .header("authorization", format!("Bearer {unverified_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verification.status(), StatusCode::OK);
    assert_eq!(
        response_json(verification).await,
        json!({
            "schemaVersion": 1,
            "access": "not_admitted",
            "reason": "verification_required"
        })
    );

    let blocked_workspace = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .header("authorization", format!("Bearer {unverified_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_workspace.status(), StatusCode::FORBIDDEN);

    let temporary = Account::create(
        &harness.pool,
        "jobs-public-beta-temporary@example.com",
        &password_hash,
    )
    .unwrap();
    let temporary_expires_at = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts
             SET email_verified_at = datetime('now'), is_temporary = 1,
                 temporary_expires_at = ?2
             WHERE id = ?1",
            rusqlite::params![temporary.id, temporary_expires_at],
        )
        .unwrap();
    let temporary_access = auth::jwt::issue(
        "test-secret-at-least-32-chars-long-xxx",
        &temporary.id,
        auth::jwt::TokenKind::Access,
    )
    .unwrap();
    let temporary_status = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/beta-access")
                .header("authorization", format!("Bearer {temporary_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(temporary_status.status(), StatusCode::OK);
    assert_eq!(
        response_json(temporary_status).await["reason"],
        "verification_required"
    );

    let verified_access = signup_and_login(
        &harness,
        "jobs-public-beta-master-off@example.com",
        password,
    )
    .await;
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "0");
    for path in ["/api/jobs/beta-access", "/api/jobs/workspace"] {
        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", format!("Bearer {verified_access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");
}

#[tokio::test]
#[serial]
async fn public_beta_gate_preserves_local_recovery_and_worker_authority_boundaries() {
    let harness = boot_harness().await;
    let _environment = TestEnvironmentGuard::install(&[
        (
            "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
            "0".to_string(),
        ),
        ("BLUEY_JOBS_BETA_ENABLED", "0".to_string()),
    ]);
    let claim_body = json!({
        "ticket": "invalid",
        "claimNonce": "0".repeat(64),
        "buildProof": {
            "descriptor": "invalid",
            "signature": "invalid"
        }
    });

    let dark_local_runner = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/local-runs/not-authorized/claim")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dark_local_runner.status(), StatusCode::BAD_REQUEST);
    let dark_local_runner_body = axum::body::to_bytes(dark_local_runner.into_body(), 4 * 1024)
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&dark_local_runner_body).contains("Invalid Browser claim nonce.")
    );

    let gated_resume = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/local-runs/not-authorized/resume")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(gated_resume.status(), StatusCode::NOT_FOUND);
    let gated_resume_body = axum::body::to_bytes(gated_resume.into_body(), 4 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&gated_resume_body).contains("Bluey Jobs beta is not enabled."));

    let independently_authenticated_worker = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/internal/discovery/lease")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        independently_authenticated_worker.status(),
        StatusCode::UNAUTHORIZED
    );

    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");
    let paused_local_runner = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/local-runs/not-authorized/claim")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paused_local_runner.status(), StatusCode::BAD_REQUEST);
    let paused_local_runner_body = axum::body::to_bytes(paused_local_runner.into_body(), 4 * 1024)
        .await
        .unwrap();
    let paused_local_runner_body = String::from_utf8_lossy(&paused_local_runner_body);
    assert!(paused_local_runner_body.contains("Invalid Browser claim nonce."));
    assert!(!paused_local_runner_body.contains("Bluey Jobs beta is not enabled."));
}

#[tokio::test]
#[serial]
async fn public_beta_database_failure_returns_only_closed_unavailable_projection() {
    let harness = boot_harness().await;
    let access = signup_and_login(
        &harness,
        "jobs-public-beta-db-failure@example.com",
        "valid-password-123",
    )
    .await;
    harness
        .pool
        .get()
        .unwrap()
        .execute("DROP TABLE jobs_public_beta_cohorts", [])
        .unwrap();

    for path in ["/api/jobs/beta-access", "/api/jobs/workspace"] {
        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response_json(response).await,
            json!({
                "schemaVersion": 1,
                "access": "not_admitted",
                "reason": "unavailable"
            })
        );
    }
}

#[tokio::test]
#[serial]
async fn public_beta_admin_routes_are_cas_bound_redacted_and_audited() {
    let admin_email = "jobs-public-beta-admin@example.com";
    let harness = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec![admin_email.to_string()],
    )
    .await;
    let admin_access = signup_and_login(&harness, admin_email, "valid-password-123").await;
    let member_email = "jobs-public-beta-member@example.com";
    let member_access = signup_and_login(&harness, member_email, "valid-password-123").await;
    let member = Account::fetch_by_email(&harness.pool, member_email)
        .unwrap()
        .unwrap();

    let cohort = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/admin/jobs/public-beta")
                .header("authorization", format!("Bearer {admin_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cohort.status(), StatusCode::OK);
    let cohort = response_json(cohort).await;
    let revision = cohort["revision"].as_i64().unwrap();
    assert!(cohort.get("cohortId").is_none());

    let stale = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::put("/admin/jobs/public-beta")
                .header("authorization", format!("Bearer {admin_access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "expectedRevision": revision - 1,
                        "state": "open",
                        "opensAtMs": cohort["opensAtMs"],
                        "closesAtMs": cohort["closesAtMs"],
                        "hardCap": cohort["hardCap"]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(response_json(stale).await["error"], "revision_conflict");

    let grant = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(format!(
                "/admin/jobs/public-beta/accounts/{}/grant",
                member.id
            ))
            .header("authorization", format!("Bearer {admin_access}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "expectedRevision": revision })).unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(grant.status(), StatusCode::OK);
    let grant_body = response_json(grant).await;
    assert_eq!(
        grant_body,
        json!({ "schemaVersion": 1, "access": "admitted", "reason": "admitted" })
    );
    assert!(!grant_body.to_string().contains(&member.id));

    let denied = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::put(format!(
                "/admin/jobs/public-beta/accounts/{}/override",
                member.id
            ))
            .header("authorization", format!("Bearer {admin_access}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "expectedRevision": 0,
                    "denied": true
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::OK);
    let denied = response_json(denied).await;
    assert_eq!(denied["denied"], true);
    assert_eq!(denied["revision"], 1);

    let denied_grant = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(format!(
                "/admin/jobs/public-beta/accounts/{}/grant",
                member.id
            ))
            .header("authorization", format!("Bearer {admin_access}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "expectedRevision": revision })).unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied_grant.status(), StatusCode::OK);
    assert_eq!(
        response_json(denied_grant).await,
        json!({
            "schemaVersion": 1,
            "access": "not_admitted",
            "reason": "denied"
        })
    );

    let member_status = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/beta-access")
                .header("authorization", format!("Bearer {member_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member_status.status(), StatusCode::OK);
    assert_eq!(response_json(member_status).await["reason"], "denied");

    let cleared = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::put(format!(
                "/admin/jobs/public-beta/accounts/{}/override",
                member.id
            ))
            .header("authorization", format!("Bearer {admin_access}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "expectedRevision": 1,
                    "denied": false
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cleared.status(), StatusCode::OK);
    assert_eq!(response_json(cleared).await["denied"], false);

    let metrics = harness
        .router
        .clone()
        .oneshot(
            Request::get("/admin/metrics")
                .header("authorization", format!("Bearer {admin_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(metrics.status(), StatusCode::OK);
    let metrics = axum::body::to_bytes(metrics.into_body(), 64 * 1024)
        .await
        .unwrap();
    let metrics = String::from_utf8(metrics.to_vec()).unwrap();
    assert!(metrics.contains("bluey_jobs_public_beta_slots_assigned_total"));
    assert!(metrics.contains("bluey_jobs_public_beta_active_denials"));
    assert!(!metrics.contains(&member.id));
    assert!(!metrics.contains(member_email));

    let (target_hash, actor_hash, metadata): (String, String, String) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT account_id_hash, actor_account_id_hash, metadata_json
             FROM ops_audit_events
             WHERE event_type = 'jobs.public_beta.account_override'
               AND status = 'completed'
             ORDER BY created_at DESC
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(target_hash, cue_core::account_id_hash_prefix(&member.id));
    let admin = Account::fetch_by_email(&harness.pool, admin_email)
        .unwrap()
        .unwrap();
    assert_eq!(actor_hash, cue_core::account_id_hash_prefix(&admin.id));
    assert!(!metadata.contains(&member.id));
    assert!(!metadata.contains(member_email));
    assert!(!metadata.contains(admin_email));
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
    setup_execution_lease_run_for_runner(harness, "cloud").await
}

struct PrepareHttpSeed {
    access_token: String,
    account_id: String,
    posting_id: String,
}

fn pinned_prepare_proxy_path(path: std::ffi::OsString, sha256: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(path);
    assert!(path.is_absolute());
    assert_eq!(std::fs::canonicalize(&path).unwrap(), path);
    let mut file = std::fs::File::open(&path).unwrap();
    assert!(file.metadata().unwrap().is_file());
    let mut sha = Sha256::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let read = file.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        sha.update(&buffer[..read]);
    }
    assert_eq!(hex::encode(sha.finalize()), sha256);
    path
}

async fn start_prepare_proxy(
    node: &std::path::Path,
    script: &std::path::Path,
    upstream_port: u16,
) -> (tokio::process::Child, u16) {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    let mut child = tokio::process::Command::new(node)
        .env_clear()
        .arg(script)
        .arg("--serve")
        .arg(upstream_port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("start exact owned Prepare proxy");
    let mut ready = BufReader::new(child.stdout.take().unwrap()).take(161);
    let mut line = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        ready.read_until(b'\n', &mut line),
    )
    .await
    .expect("owned proxy readiness deadline")
    .expect("read owned proxy readiness");
    assert!(line.len() <= 160 && line.ends_with(b"\n"));
    let reply: serde_json::Value = serde_json::from_slice(&line).unwrap();
    assert_eq!(reply["event"], "bluey_jobs_prepare_proxy_ready");
    let port = u16::try_from(reply["port"].as_u64().unwrap()).unwrap();
    assert!(port >= 1024 && port != upstream_port);
    (child, port)
}

fn prepare_effect_census(harness: &Harness, account: &str) -> (i64, i64, i64, i64) {
    harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT accounts.balance_cents, entitlement.used_packets,
                    (SELECT count(*) FROM jobs_packet_metering WHERE account_id=?1),
                    (SELECT count(*) FROM jobs_workflow_commands WHERE account_id=?1)
               FROM accounts JOIN jobs_entitlements entitlement
                 ON entitlement.account_id=accounts.id WHERE accounts.id=?1",
            rusqlite::params![account],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct PrepareRefusalEffectCensus {
    balance_cents: i64,
    used_packets: i64,
    beta_enrollments: i64,
    beta_admitted_at_ms: i64,
    application_identities: i64,
    application_identity_updated_at_ms: i64,
    auto_submit_authorizations: i64,
    applications: i64,
    resume_versions: i64,
    profile_evidence_revisions: i64,
    resume_claim_evidence: i64,
    resume_generations: i64,
    generation_allowances: i64,
    provider_cost_holds: i64,
    usage_events: i64,
    packet_metering: i64,
    workflow_commands: i64,
    attempt_reservations: i64,
    browser_sessions: i64,
}

fn prepare_refusal_effect_census(
    harness: &Harness,
    account_id: &str,
) -> PrepareRefusalEffectCensus {
    harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT account.balance_cents, entitlement.used_packets,
                    (SELECT COUNT(*) FROM jobs_public_beta_enrollments
                      WHERE account_id = ?1),
                    (SELECT COALESCE(MAX(admitted_at_ms), -1)
                       FROM jobs_public_beta_enrollments WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_application_identities
                      WHERE account_id = ?1),
                    (SELECT COALESCE(MAX(updated_at_ms), -1)
                       FROM jobs_application_identities WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_auto_submit_authorizations
                      WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_applications WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_resume_versions WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_profile_evidence_revisions
                      WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_resume_claim_evidence WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_resume_generations WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_generation_allowance_reservations
                      WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_provider_cost_holds),
                    (SELECT COUNT(*) FROM usage_events WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_packet_metering WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_workflow_commands WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_attempt_reservations WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_browser_sessions WHERE account_id = ?1)
               FROM accounts account
               JOIN jobs_entitlements entitlement ON entitlement.account_id = account.id
              WHERE account.id = ?1",
            rusqlite::params![account_id],
            |row| {
                Ok(PrepareRefusalEffectCensus {
                    balance_cents: row.get(0)?,
                    used_packets: row.get(1)?,
                    beta_enrollments: row.get(2)?,
                    beta_admitted_at_ms: row.get(3)?,
                    application_identities: row.get(4)?,
                    application_identity_updated_at_ms: row.get(5)?,
                    auto_submit_authorizations: row.get(6)?,
                    applications: row.get(7)?,
                    resume_versions: row.get(8)?,
                    profile_evidence_revisions: row.get(9)?,
                    resume_claim_evidence: row.get(10)?,
                    resume_generations: row.get(11)?,
                    generation_allowances: row.get(12)?,
                    provider_cost_holds: row.get(13)?,
                    usage_events: row.get(14)?,
                    packet_metering: row.get(15)?,
                    workflow_commands: row.get(16)?,
                    attempt_reservations: row.get(17)?,
                    browser_sessions: row.get(18)?,
                })
            },
        )
        .unwrap()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PrepareProviderCensus {
    openai: usize,
    anthropic: usize,
    stripe: usize,
    square: usize,
    deepgram: usize,
    mail: usize,
}

async fn prepare_provider_census(harness: &Harness) -> PrepareProviderCensus {
    PrepareProviderCensus {
        openai: harness.openai.received_requests().await.unwrap().len(),
        anthropic: harness.anthropic.received_requests().await.unwrap().len(),
        stripe: harness.stripe.received_requests().await.unwrap().len(),
        square: harness.square.received_requests().await.unwrap().len(),
        deepgram: harness.deepgram.received_requests().await.unwrap().len(),
        mail: harness.mail.received_requests().await.unwrap().len(),
    }
}

#[tokio::test]
#[serial]
async fn prepare_auto_submit_refuses_c2c_before_track_authorization_without_effects() {
    const C2C_MESSAGE: &str = concat!(
        "Bluey does not submit C2C applications. ",
        "Confirm the arrangement with the employer before applying yourself."
    );
    const W2_AUTHORIZATION_MESSAGE: &str =
        "Enable Auto-submit on this Career Track before queuing applications.";

    for (employment_type, expected_status, expected_message, expect_candidate_only) in [
        ("contract c2c", StatusCode::CONFLICT, C2C_MESSAGE, true),
        (
            "contract w2",
            StatusCode::CONFLICT,
            W2_AUTHORIZATION_MESSAGE,
            false,
        ),
    ] {
        let harness = boot_harness_with_upstream(UpstreamKeys::default()).await;
        let seed = setup_execution_lease_seed_for_employment(
            &harness,
            "cloud",
            true,
            None,
            None,
            employment_type,
        )
        .await;
        let posting = jobs::get_posting(&harness.pool, &seed.account_id, &seed.posting_id)
            .unwrap()
            .unwrap();
        let eligibility =
            jobs::evaluate_job_eligibility(&harness.pool, &seed.account_id, &posting, true, None)
                .unwrap();
        assert_eq!(eligibility.capability, "certified", "{eligibility:#?}");
        assert!(eligibility.can_prepare, "{eligibility:#?}");
        assert!(eligibility.hard_failures.is_empty(), "{eligibility:#?}");
        assert_eq!(
            eligibility
                .review_reasons
                .iter()
                .any(|reason| reason.code == "c2c_submission_requires_candidate"),
            expect_candidate_only,
            "{eligibility:#?}"
        );
        assert_eq!(eligibility.can_auto_submit, !expect_candidate_only);

        let effects_before = prepare_refusal_effect_census(&harness, &seed.account_id);
        assert_eq!(effects_before.balance_cents, 0);
        assert_eq!(effects_before.used_packets, 0);
        assert_eq!(effects_before.beta_enrollments, 1);
        assert!(effects_before.beta_admitted_at_ms > 0);
        assert_eq!(effects_before.application_identities, 1);
        assert!(effects_before.application_identity_updated_at_ms > 0);
        assert_eq!(effects_before.auto_submit_authorizations, 0);
        assert_eq!(effects_before.applications, 0);
        assert_eq!(effects_before.resume_versions, 0);
        assert_eq!(effects_before.profile_evidence_revisions, 0);
        assert_eq!(effects_before.resume_claim_evidence, 0);
        assert_eq!(effects_before.resume_generations, 0);
        assert_eq!(effects_before.generation_allowances, 0);
        assert_eq!(effects_before.provider_cost_holds, 0);
        assert_eq!(effects_before.usage_events, 0);
        assert_eq!(effects_before.packet_metering, 0);
        assert_eq!(effects_before.workflow_commands, 0);
        assert_eq!(effects_before.attempt_reservations, 0);
        assert_eq!(effects_before.browser_sessions, 0);
        let providers_before = prepare_provider_census(&harness).await;
        assert_eq!(providers_before, PrepareProviderCensus::default());
        assert_eq!(providers_before.openai, 0);
        assert_eq!(providers_before.anthropic, 0);
        assert_eq!(providers_before.stripe, 0);
        assert_eq!(providers_before.square, 0);
        assert_eq!(providers_before.deepgram, 0);
        assert_eq!(providers_before.mail, 0);

        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post("/api/jobs/applications")
                    .header("authorization", format!("Bearer {}", seed.access_token))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "job_id": seed.posting_id,
                            "mode": "factual",
                            "submission_mode": "auto_submit",
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected_status, "{employment_type}");
        let body = axum::body::to_bytes(response.into_body(), 4 * 1024)
            .await
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            expected_message,
            "{employment_type}"
        );

        assert_eq!(
            prepare_refusal_effect_census(&harness, &seed.account_id),
            effects_before,
            "{employment_type}"
        );
        assert_eq!(
            prepare_provider_census(&harness).await,
            providers_before,
            "{employment_type}"
        );
        if !expect_candidate_only {
            // W2 reaches its own missing-authorization refusal; it must not
            // inherit the C2C candidate-only explanation.
            assert_ne!(expected_message, C2C_MESSAGE);
        }
    }
}

#[tokio::test]
#[serial]
async fn prepare_application_native_tcp_accepts_direct_and_browser_shaped_headers_once_per_fresh_fixture(
) {
    use std::time::{Duration, Instant};

    let proxy = match (
        std::env::var_os("BLUEY_JOBS_TEST_PREPARE_PROXY_NODE"),
        std::env::var_os("BLUEY_JOBS_TEST_PREPARE_PROXY_SCRIPT"),
    ) {
        (None, None) => None,
        (Some(node), Some(script)) => Some((
            pinned_prepare_proxy_path(
                node,
                "2cc28a1880d0774616959f2210ee739bffa6ac695c887e8988a496a21271f6c0",
            ),
            pinned_prepare_proxy_path(
                script,
                "166963458f5c41842ee23d13e45165bbc80777df7dd1491d1116023d3ceffe13",
            ),
        )),
        _ => panic!("owned Prepare proxy requires both explicit input paths"),
    };
    let cases = ["direct", "direct_browser_like"];
    for case in cases
        .into_iter()
        .chain(proxy.as_ref().map(|_| "proxied_browser_like"))
    {
        let harness = boot_harness().await;
        let seed = setup_execution_lease_seed(&harness, "local", true, None, None).await;
        assert!(jobs::list_applications(&harness.pool, &seed.account_id)
            .unwrap()
            .is_empty());
        let _ = jobs::get_entitlement(&harness.pool, &seed.account_id).unwrap();
        let before = prepare_effect_census(&harness, &seed.account_id);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let census = Arc::clone(&requests);
        let router = harness.jobs_router.clone().layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let census = Arc::clone(&census);
                async move {
                    if request.method() == axum::http::Method::POST
                        && request.uri().path() == "/api/jobs/applications"
                    {
                        census.fetch_add(1, Ordering::Relaxed);
                    }
                    next.run(request).await
                }
            },
        ));
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        let (mut proxy_child, request_port) = if case == "proxied_browser_like" {
            let (node, script) = proxy.as_ref().unwrap();
            let (child, port) = start_prepare_proxy(node, script, address.port()).await;
            (Some(child), port)
        } else {
            (None, address.port())
        };
        let origin = format!("http://127.0.0.1:{request_port}");
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let mut request = client
            .post(format!("{origin}/api/jobs/applications"))
            .bearer_auth(&seed.access_token)
            .json(&json!({
                "job_id": seed.posting_id,
                "mode": "factual",
                "submission_mode": "review_first"
            }));
        if case != "direct" {
            request = request
                .header("origin", &origin)
                .header("sec-fetch-mode", "cors")
                .header("sec-fetch-site", "same-origin");
        }
        let started = Instant::now();
        let response = request.send().await.unwrap_or_else(|error| {
            panic!(
                "{case}: Prepare transport failed; api_entries={}, timeout={}",
                requests.load(Ordering::Relaxed),
                error.is_timeout()
            )
        });
        assert_eq!(response.status(), reqwest::StatusCode::OK, "{case}");
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(requests.load(Ordering::Relaxed), 1);
        assert!(body.get("metering").is_none());
        let application_id = body["application"]["id"].as_str().unwrap();
        let resume_id = body["resume_version"]["id"].as_str().unwrap();
        let applications = jobs::list_applications(&harness.pool, &seed.account_id).unwrap();
        assert_eq!(applications.len(), 1);
        let application = &applications[0];
        assert_eq!(application.id, application_id);
        assert_eq!(application.job_id, seed.posting_id);
        assert_eq!(application.state, "awaiting_review");
        assert!(application.submitted_at_ms.is_none());
        let resume = jobs::get_resume_version(&harness.pool, &seed.account_id, resume_id)
            .unwrap()
            .unwrap();
        assert_eq!(application.resume_version_id.as_deref(), Some(resume_id));
        assert_eq!(resume.id, resume_id);
        assert_eq!(resume.mode, "factual");
        assert!(jobs::list_browser_sessions(&harness.pool, &seed.account_id)
            .unwrap()
            .is_empty());
        assert!(
            jobs::list_attempt_reservations(&harness.pool, &seed.account_id)
                .unwrap()
                .is_empty()
        );
        assert_eq!(prepare_effect_census(&harness, &seed.account_id), before);
        println!(
            "owned_prepare_tcp_case={case} api_entries=1 elapsed_ms={}",
            started.elapsed().as_millis()
        );
        drop(client);
        if let Some(child) = &mut proxy_child {
            drop(child.stdin.take());
            assert!(tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success());
        }
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

async fn setup_execution_lease_seed(
    harness: &Harness,
    runner_kind: &str,
    complete_onboarding: bool,
    source_store: Option<&MockServer>,
    verified_signup: Option<(&str, serde_json::Value)>,
) -> PrepareHttpSeed {
    setup_execution_lease_seed_for_employment(
        harness,
        runner_kind,
        complete_onboarding,
        source_store,
        verified_signup,
        "full_time",
    )
    .await
}

async fn setup_execution_lease_seed_for_employment(
    harness: &Harness,
    runner_kind: &str,
    complete_onboarding: bool,
    source_store: Option<&MockServer>,
    verified_signup: Option<(&str, serde_json::Value)>,
    employment_type: &str,
) -> PrepareHttpSeed {
    assert!(matches!(runner_kind, "local" | "cloud"));
    let (employment_types, engagement_types) = match employment_type {
        "full_time" => (vec!["full_time".to_string()], Vec::new()),
        "contract c2c" => (vec!["contract".to_string()], vec!["c2c".to_string()]),
        "contract w2" => (vec!["contract".to_string()], vec!["w2".to_string()]),
        other => panic!("unsupported execution-lease employment fixture: {other}"),
    };
    let (email, verified_signup) = match verified_signup {
        Some((email, signup)) => (email, Some(signup)),
        None => ("jobs-execution-lease@example.com", None),
    };
    let access_token = if let Some(signup) = verified_signup {
        assert!(source_store.is_some());
        assert_eq!(signup["account"]["email"], email);
        signup["access_token"].as_str().unwrap().to_string()
    } else if source_store.is_some() {
        let signup = signup_with_otp(harness, email, "valid-password-123").await;
        assert_eq!(signup["account"]["email"], email);
        assert_eq!(harness.mail.received_requests().await.unwrap().len(), 1);
        signup["access_token"].as_str().unwrap().to_string()
    } else {
        signup_and_login(harness, email, "valid-password-123").await
    };
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .unwrap();
    assert!(account.email_verified_at.is_some());
    let public_beta = jobs_beta_access::evaluate_or_enroll_public_beta(&harness.pool, &account.id)
        .expect("admit the execution fixture account to the open public-beta cohort");
    assert_eq!(public_beta.reason, PublicBetaAccessReason::Admitted);
    let entitlement_plan = if runner_kind == "local" {
        "pro"
    } else {
        "cloud"
    };
    jobs::set_entitlement_plan(&harness.pool, &account.id, entitlement_plan).unwrap();
    let profile = if let Some(object_store) = source_store {
        let stored = mount_jobs_resume_source_round_trip(object_store, 1, 1).await;
        let response = harness
            .jobs_router
            .clone()
            .oneshot(jobs_resume_source_upload_request(
                &access_token,
                "00000000-0000-4000-8000-000000000011",
                "execution-lease-source-resume.txt",
                EXECUTION_LEASE_SOURCE_RESUME_BYTES,
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "same-account resume upload"
        );
        let body = response_json(response).await;
        assert_eq!(*stored.lock().unwrap(), EXECUTION_LEASE_SOURCE_RESUME_BYTES);
        let mut profile = jobs::get_profile(&harness.pool, &account.id, &account.email).unwrap();
        assert_eq!(body["asset"]["id"], profile.source_resume_asset_id);
        assert_eq!(
            profile.source_resume_sha256,
            hex::encode(Sha256::digest(EXECUTION_LEASE_SOURCE_RESUME_BYTES))
        );
        if complete_onboarding {
            // Job, profile and runner entitlement are synthetic fixture inputs.
            profile.onboarding_complete = true;
            profile = jobs::save_profile(&harness.pool, &account.id, &profile).unwrap();
        }
        profile
    } else {
        seed_execution_lease_source_resume(harness, &account, complete_onboarding)
    };
    let profile = if employment_type == "full_time" {
        profile
    } else {
        let mut profile = profile;
        profile.skills = ["Rust", "TypeScript", "PostgreSQL"]
            .map(str::to_string)
            .to_vec();
        jobs::save_profile(&harness.pool, &account.id, &profile).unwrap()
    };
    let identity =
        jobs::ensure_primary_application_identity(&harness.pool, &account.id, &account.email)
            .unwrap();
    let mut preference_input = JobPreferences {
        employment_types: employment_types.clone(),
        engagement_types: engagement_types.clone(),
        ..JobPreferences::default()
    };
    if employment_type != "full_time" {
        preference_input.sponsorship = "not_required".to_string();
    }
    let preferences =
        jobs::save_preferences(&harness.pool, &account.id, &preference_input).unwrap();
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
                employment_types,
                engagement_types,
                ..jobs::CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    assert_eq!(track.policy.authority.review_state, "approved");
    assert!(track.policy.authority.policy_revision_no > 0);
    let now = chrono::Utc::now().timestamp_millis();
    let mut posting_input = JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse".to_string(),
        external_id: "lease-integration".to_string(),
        company: "Acme".to_string(),
        title: "Platform Engineer".to_string(),
        location: "New York, NY".to_string(),
        workplace: "hybrid".to_string(),
        canonical_url: "https://boards.greenhouse.io/acme/jobs/lease-integration".to_string(),
        description: if employment_type == "full_time" {
            "Build reliable systems.".to_string()
        } else {
            "Build reliable systems with Rust, TypeScript, and PostgreSQL.".to_string()
        },
        compensation: "$170k-$200k".to_string(),
        employment_type: employment_type.to_string(),
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
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    };
    posting_input.canonical_key = jobs::canonical_job_key(&posting_input);
    let posting = jobs::install_integration_test_production_positive_job_authorities(
        jobs::IntegrationTestProductionPositiveJobAuthoritiesRequest {
            pool: &harness.pool,
            account_id: &account.id,
            posting: &posting_input,
            profile: &profile,
            preferences: &preferences,
            canonical_employer_domain: "acme.com",
            suffix: "execution-lease-integration",
            runner_kind,
        },
    );
    PrepareHttpSeed {
        access_token,
        account_id: account.id,
        posting_id: posting.id,
    }
}

fn seed_execution_lease_source_resume(
    harness: &Harness,
    account: &Account,
    complete_onboarding: bool,
) -> jobs::CareerProfile {
    let source_resume_sha256 = hex::encode(Sha256::digest(EXECUTION_LEASE_SOURCE_RESUME_BYTES));
    let source_resume = jobs::ResumeSourceAsset {
        id: "execution-lease-source-resume".to_string(),
        file_name: "execution-lease-source-resume.txt".to_string(),
        media_type: "text/plain".to_string(),
        file_type: "txt".to_string(),
        storage_key: format!(
            "bluey-cloud/accounts/{}/jobs/resumes/execution-lease-source-resume/sha256/{}.txt",
            account.id, source_resume_sha256
        ),
        sha256: source_resume_sha256,
        size_bytes: EXECUTION_LEASE_SOURCE_RESUME_BYTES.len() as i64,
        page_count: None,
        template_status: "text_only".to_string(),
        created_at_ms: chrono::Utc::now().timestamp_millis(),
        updated_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    let mut profile = jobs::default_profile(&account.email);
    if complete_onboarding {
        profile.onboarding_complete = true;
    }
    profile.source_resume_name = source_resume.file_name.clone();
    profile.source_resume_asset_id = source_resume.id.clone();
    profile.source_resume_sha256 = source_resume.sha256.clone();
    profile.source_resume_media_type = source_resume.media_type.clone();
    profile.source_resume_template_status = source_resume.template_status.clone();
    jobs::save_resume_source_asset(&harness.pool, &account.id, &source_resume, &profile)
        .unwrap()
        .1
}

async fn setup_execution_lease_run_for_runner(
    harness: &Harness,
    runner_kind: &str,
) -> (String, String, String, String) {
    let seed = setup_execution_lease_seed(harness, runner_kind, false, None, None).await;
    queue_execution_lease_fixture(harness, runner_kind, seed, false).await
}

async fn queue_execution_lease_fixture(
    harness: &Harness,
    runner_kind: &str,
    seed: PrepareHttpSeed,
    prepare_via_api: bool,
) -> (String, String, String, String) {
    let PrepareHttpSeed {
        access_token,
        account_id,
        posting_id,
    } = seed;
    let application = if prepare_via_api {
        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post("/api/jobs/applications")
                    .header("authorization", format!("Bearer {access_token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "job_id": posting_id,
                            "mode": "factual",
                            "submission_mode": "review_first",
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "same-account Prepare");
        let prepared = response_json(response).await;
        let application_id = prepared["application"]["id"].as_str().unwrap();
        assert_eq!(prepared["resume_version"]["mode"], "factual");
        let stored = jobs::get_application(&harness.pool, &account_id, application_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.job_id, posting_id);
        assert_eq!(stored.state, "awaiting_review");
        assert_eq!(
            stored.resume_version_id.as_deref(),
            prepared["resume_version"]["id"].as_str()
        );
        stored
    } else {
        jobs::prepare_application(
            &harness.pool,
            &account_id,
            &posting_id,
            "factual",
            "review_first",
        )
        .unwrap()
        .0
    };
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
    if approved.status() != StatusCode::OK {
        let status = approved.status();
        let body = axum::body::to_bytes(approved.into_body(), 64 * 1024)
            .await
            .unwrap();
        panic!(
            "execution lease fixture approval failed with {status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    if prepare_via_api {
        let approved = jobs::get_application(&harness.pool, &account_id, &application.id)
            .unwrap()
            .unwrap();
        assert!(approved.submitted_at_ms.is_none());
        assert!(approved.run_id.is_none());
        assert!(jobs::list_browser_sessions(&harness.pool, &account_id)
            .unwrap()
            .is_empty());
    }
    jobs::reserve_application_attempt(&harness.pool, &account_id, &application.id, runner_kind)
        .unwrap();
    let application = jobs::get_application(&harness.pool, &account_id, &application.id)
        .unwrap()
        .unwrap();
    let run_id = "cloud-run-integration-lease".to_string();
    jobs::upsert_browser_session(
        &harness.pool,
        &account_id,
        &BrowserSession {
            id: run_id.clone(),
            runner: runner_kind.to_string(),
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
    let mut application =
        jobs::assign_application_run(&harness.pool, &account_id, &application.id, &run_id)
            .unwrap()
            .unwrap();
    application.state = "queued".to_string();
    application.updated_at_ms = chrono::Utc::now().timestamp_millis();
    persist_test_application(harness, &account_id, &application);
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let browser_profile_id = jobs::execution_browser_profile_id(&account_id, identity_id);
    (account_id, application.id, run_id, browser_profile_id)
}

struct EvidenceDownloadFixture {
    account_id: String,
    application_id: String,
    resume: ApplicationEvidence,
    receipt: ApplicationEvidence,
    confirmation: ApplicationEvidence,
    resume_bytes: Vec<u8>,
    receipt_bytes: Vec<u8>,
    confirmation_bytes: Vec<u8>,
}

fn persist_test_application(
    harness: &Harness,
    account_id: &str,
    application: &jobs::JobApplication,
) {
    let payload = serde_json::to_string(application).unwrap();
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_applications
                SET state = ?3, application_json = ?4, updated_at_ms = ?5,
                    submitted_at_ms = ?6
              WHERE account_id = ?1 AND id = ?2",
            rusqlite::params![
                account_id,
                &application.id,
                &application.state,
                payload,
                application.updated_at_ms,
                application.submitted_at_ms,
            ],
        )
        .unwrap();
}

fn persist_test_application_evidence(
    harness: &Harness,
    account_id: &str,
    evidence: &ApplicationEvidence,
) {
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_application_evidence SET evidence_json = ?3
              WHERE account_id = ?1 AND id = ?2",
            rusqlite::params![
                account_id,
                &evidence.id,
                serde_json::to_string(evidence).unwrap()
            ],
        )
        .unwrap();
}

async fn setup_application_evidence_download(harness: &Harness) -> EvidenceDownloadFixture {
    let (account_id, application_id, run_id, _) = setup_execution_lease_run(harness).await;
    let mut application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    let resume_version_id = application.resume_version_id.clone().unwrap();
    let resume_bytes = valid_receipt_pdf();
    let resume_sha256 = hex::encode(Sha256::digest(&resume_bytes));
    let resume_key = format!(
        "bluey-cloud/accounts/{account_id}/jobs/applications/{application_id}/evidence/resume.pdf"
    );
    let confirmation_bytes = valid_receipt_png();
    let confirmation_sha256 = hex::encode(Sha256::digest(&confirmation_bytes));
    let confirmation_key = format!(
        "bluey-cloud/accounts/{account_id}/jobs/applications/{application_id}/evidence/confirmation.png"
    );
    let receipt_id = format!("receipt-download-{application_id}");
    let fingerprint = "d".repeat(64);
    let frozen_job = application
        .receipt
        .pointer("/approved_execution/job")
        .cloned()
        .expect("approved application must contain a frozen job snapshot");
    let confirmation_url = format!(
        "{}/confirmation",
        frozen_job["canonicalUrl"]
            .as_str()
            .expect("approved job has a canonical URL")
            .trim_end_matches('/')
    );
    let receipt_without_object = json!({
        "schemaVersion": 1,
        "receiptId": receipt_id,
        "accountId": account_id,
        "applicationId": application_id,
        "runId": run_id,
        "runner": "cloud",
        "adapter": "greenhouse",
        "adapterVersion": "2026.07.1-beta.1",
        "packet": { "jobId": application.job_id },
        "job": &frozen_job,
        "documents": [{
            "kind": "resume",
            "versionId": &resume_version_id,
            "storageKey": &resume_key,
            "sha256": &resume_sha256,
            "mediaType": "application/pdf"
        }],
        "events": [],
        "result": {
            "status": "submitted",
            "submitHttpStatus": 302,
            "confirmationText": "Application received",
            "confirmationUrl": &confirmation_url,
            "submittedAt": "2026-08-04T12:00:00Z",
            "issues": []
        },
        "finalUrl": &confirmation_url,
        "screenshotKeys": [&confirmation_key],
        "evidenceObjects": [{
            "kind": "screenshot",
            "storageKey": &confirmation_key,
            "sha256": &confirmation_sha256,
            "mediaType": "image/png",
            "sizeBytes": confirmation_bytes.len()
        }, {
            "kind": "resume",
            "storageKey": &resume_key,
            "sha256": &resume_sha256,
            "mediaType": "application/pdf",
            "sizeBytes": resume_bytes.len()
        }],
        "_bluey_server_submission_fingerprint_v1": &fingerprint,
        "_bluey_server_submission_authority_v1": {
            "schemaVersion": 1,
            "runner": "cloud"
        }
    });
    let receipt_bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "bundleId": fingerprint,
        "accountId": account_id,
        "applicationId": application_id,
        "receiptId": receipt_id,
        "job": &frozen_job,
        "resume": { "id": resume_version_id },
        "receipt": receipt_without_object
    }))
    .unwrap();
    let receipt_sha256 = hex::encode(Sha256::digest(&receipt_bytes));
    let receipt_key = format!(
        "bluey-cloud/accounts/{account_id}/jobs/applications/{application_id}/evidence/receipt.json"
    );
    let receipt_size = i64::try_from(receipt_bytes.len()).unwrap();
    let resume_size = i64::try_from(resume_bytes.len()).unwrap();
    let confirmation_size = i64::try_from(confirmation_bytes.len()).unwrap();
    let mut final_receipt = receipt_without_object;
    final_receipt["receiptObject"] = json!({
        "storageKey": &receipt_key,
        "sha256": &receipt_sha256,
        "mediaType": "application/json",
        "sizeBytes": receipt_size,
        "schemaVersion": 1
    });
    let now = chrono::Utc::now().timestamp_millis();
    application.receipt = final_receipt;
    application.state = "submitted".to_string();
    application.submitted_at_ms = Some(now);
    application.updated_at_ms = now;
    persist_test_application(harness, &account_id, &application);

    let resume = jobs::save_application_evidence(
        &harness.pool,
        &account_id,
        &ApplicationEvidence {
            id: "download-resume-evidence".to_string(),
            application_id: application_id.clone(),
            kind: "resume".to_string(),
            label: "Resume submitted".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "../../submitted\r\nresume.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            storage_key: resume_key,
            sha256: resume_sha256,
            resume_version_id: Some(resume_version_id.clone()),
            occurred_at_ms: now,
            metadata: json!({
                "attached_to_submission": true,
                "receipt_id": receipt_id,
                "size_bytes": resume_size
            }),
            created_at_ms: now,
        },
    )
    .unwrap();
    let receipt = jobs::save_application_evidence(
        &harness.pool,
        &account_id,
        &ApplicationEvidence {
            id: "download-receipt-evidence".to_string(),
            application_id: application_id.clone(),
            kind: "application_receipt".to_string(),
            label: "Application receipt bundle".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "../../bad\r\nname.json".to_string(),
            media_type: "application/json".to_string(),
            storage_key: receipt_key,
            sha256: receipt_sha256,
            resume_version_id: Some(resume_version_id.clone()),
            occurred_at_ms: now,
            metadata: json!({
                "immutable": true,
                "receipt_id": receipt_id,
                "schema_version": 1,
                "size_bytes": receipt_size,
                "runner": "cloud",
                "run_id": run_id
            }),
            created_at_ms: now,
        },
    )
    .unwrap();
    let confirmation = jobs::save_application_evidence(
        &harness.pool,
        &account_id,
        &ApplicationEvidence {
            id: "download-confirmation-evidence".to_string(),
            application_id: application_id.clone(),
            kind: "submission_confirmation".to_string(),
            label: "Application received".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "submission-confirmation.png".to_string(),
            media_type: "image/png".to_string(),
            storage_key: confirmation_key.clone(),
            sha256: confirmation_sha256,
            resume_version_id: Some(resume_version_id),
            occurred_at_ms: now,
            metadata: json!({
                "confirmation": "Application received",
                "screenshot_keys": [confirmation_key],
                "evidence_strength": "browser_confirmed",
                "receipt_id": receipt_id,
                "size_bytes": confirmation_size
            }),
            created_at_ms: now,
        },
    )
    .unwrap();
    EvidenceDownloadFixture {
        account_id,
        application_id,
        resume,
        receipt,
        confirmation,
        resume_bytes,
        receipt_bytes,
        confirmation_bytes,
    }
}

#[tokio::test]
#[serial]
async fn jobs_evidence_download_is_authenticated_tenant_scoped_and_verified() {
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
    let fixture = setup_application_evidence_download(&harness).await;
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{}", fixture.resume.storage_key)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture.resume_bytes.clone(), "application/pdf"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{}", fixture.receipt.storage_key)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture.receipt_bytes.clone(), "application/json"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/bucket/{}",
            fixture.confirmation.storage_key
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture.confirmation_bytes.clone(), "image/png"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    let receipt_path = format!(
        "/api/jobs/applications/{}/evidence/{}/download",
        fixture.application_id, fixture.receipt.id
    );
    let unauthenticated = harness
        .jobs_router
        .clone()
        .oneshot(Request::get(&receipt_path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let owner = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let owner_token = owner["access_token"].as_str().unwrap();
    let missing = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/jobs/applications/{}/evidence/not-present/download",
                fixture.application_id
            ))
            .header("authorization", format!("Bearer {owner_token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let other_token = signup_and_login(
        &harness,
        "jobs-evidence-download-other@example.com",
        "valid-password-123",
    )
    .await;
    let cross_account = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&receipt_path)
                .header("authorization", format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_account.status(), StatusCode::NOT_FOUND);

    let resume_response = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/jobs/applications/{}/evidence/{}/download",
                fixture.application_id, fixture.resume.id
            ))
            .header("authorization", format!("Bearer {owner_token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resume_response.status(), StatusCode::OK);
    assert_eq!(
        resume_response.headers()[axum::http::header::CONTENT_TYPE],
        "application/pdf"
    );
    assert_eq!(
        resume_response.headers()[axum::http::header::CONTENT_DISPOSITION],
        "attachment; filename=\"submitted-resume.pdf\""
    );
    let resume_body = axum::body::to_bytes(resume_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(resume_body.as_ref(), fixture.resume_bytes);

    let receipt_response = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&receipt_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(receipt_response.status(), StatusCode::OK);
    assert_eq!(
        receipt_response.headers()[axum::http::header::CONTENT_TYPE],
        "application/json"
    );
    assert_eq!(
        receipt_response.headers()[axum::http::header::CONTENT_DISPOSITION],
        "attachment; filename=\"bad-name.json\""
    );
    assert_eq!(
        receipt_response.headers()[axum::http::header::CACHE_CONTROL],
        "private, no-store"
    );
    assert_eq!(
        receipt_response.headers()["x-content-type-options"],
        "nosniff"
    );
    let receipt_body = axum::body::to_bytes(receipt_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(receipt_body.as_ref(), fixture.receipt_bytes);

    let confirmation_response = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/jobs/applications/{}/evidence/{}/download",
                fixture.application_id, fixture.confirmation.id
            ))
            .header("authorization", format!("Bearer {owner_token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirmation_response.status(), StatusCode::OK);
    assert_eq!(
        confirmation_response.headers()[axum::http::header::CONTENT_TYPE],
        "image/png"
    );
    let confirmation_body = axum::body::to_bytes(confirmation_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(confirmation_body.as_ref(), fixture.confirmation_bytes);
}

#[tokio::test]
#[serial]
async fn jobs_evidence_download_fails_closed_on_tamper_media_missing_object_and_bad_scope() {
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
    let fixture = setup_application_evidence_download(&harness).await;
    let receipt_path = format!(
        "/api/jobs/applications/{}/evidence/{}/download",
        fixture.application_id, fixture.receipt.id
    );
    let resume_path = format!(
        "/api/jobs/applications/{}/evidence/{}/download",
        fixture.application_id, fixture.resume.id
    );
    let confirmation_path = format!(
        "/api/jobs/applications/{}/evidence/{}/download",
        fixture.application_id, fixture.confirmation.id
    );
    let structurally_invalid_receipt_bytes = b"{}".to_vec();
    let structurally_invalid_receipt_sha256 =
        hex::encode(Sha256::digest(&structurally_invalid_receipt_bytes));
    let structurally_invalid_receipt_size =
        i64::try_from(structurally_invalid_receipt_bytes.len()).unwrap();
    let mut structurally_invalid_receipt = fixture.receipt.clone();
    structurally_invalid_receipt.sha256 = structurally_invalid_receipt_sha256.clone();
    structurally_invalid_receipt.metadata["size_bytes"] = json!(structurally_invalid_receipt_size);
    persist_test_application_evidence(&harness, &fixture.account_id, &structurally_invalid_receipt);
    let mut application =
        jobs::get_application(&harness.pool, &fixture.account_id, &fixture.application_id)
            .unwrap()
            .unwrap();
    application.receipt["receiptObject"]["sha256"] = json!(structurally_invalid_receipt_sha256);
    application.receipt["receiptObject"]["sizeBytes"] = json!(structurally_invalid_receipt_size);
    persist_test_application(&harness, &fixture.account_id, &application);
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{}", fixture.receipt.storage_key)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(structurally_invalid_receipt_bytes, "application/json"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/bucket/{}",
            fixture.confirmation.storage_key
        )))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&object_store)
        .await;
    let owner = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let owner_token = owner["access_token"].as_str().unwrap();

    let tampered = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&receipt_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tampered.status(), StatusCode::CONFLICT);

    persist_test_application_evidence(&harness, &fixture.account_id, &fixture.receipt);
    application.receipt["receiptObject"]["sha256"] = json!(fixture.receipt.sha256);
    application.receipt["receiptObject"]["sizeBytes"] =
        fixture.receipt.metadata["size_bytes"].clone();
    persist_test_application(&harness, &fixture.account_id, &application);
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{}", fixture.receipt.storage_key)))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture.receipt_bytes.clone(), "text/plain"),
        )
        .with_priority(1)
        .expect(1)
        .mount(&object_store)
        .await;
    let wrong_media_type = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&receipt_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_media_type.status(), StatusCode::CONFLICT);

    let missing = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&confirmation_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::BAD_GATEWAY);

    let structurally_invalid_png_bytes = b"not a valid PNG".to_vec();
    let structurally_invalid_png_sha256 =
        hex::encode(Sha256::digest(&structurally_invalid_png_bytes));
    let structurally_invalid_png_size =
        i64::try_from(structurally_invalid_png_bytes.len()).unwrap();
    let mut structurally_invalid_confirmation = fixture.confirmation.clone();
    structurally_invalid_confirmation.sha256 = structurally_invalid_png_sha256.clone();
    structurally_invalid_confirmation.metadata["size_bytes"] = json!(structurally_invalid_png_size);
    persist_test_application_evidence(
        &harness,
        &fixture.account_id,
        &structurally_invalid_confirmation,
    );
    let confirmation_manifest = application.receipt["evidenceObjects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["storageKey"] == fixture.confirmation.storage_key)
        .unwrap();
    confirmation_manifest["sha256"] = json!(structurally_invalid_png_sha256);
    confirmation_manifest["sizeBytes"] = json!(structurally_invalid_png_size);
    persist_test_application(&harness, &fixture.account_id, &application);
    Mock::given(method("GET"))
        .and(path(format!(
            "/bucket/{}",
            fixture.confirmation.storage_key
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(structurally_invalid_png_bytes, "image/png"),
        )
        .with_priority(1)
        .expect(1)
        .mount(&object_store)
        .await;
    let invalid_png = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&confirmation_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_png.status(), StatusCode::CONFLICT);

    persist_test_application_evidence(&harness, &fixture.account_id, &fixture.confirmation);
    let confirmation_manifest = application.receipt["evidenceObjects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["storageKey"] == fixture.confirmation.storage_key)
        .unwrap();
    confirmation_manifest["sha256"] = json!(fixture.confirmation.sha256);
    confirmation_manifest["sizeBytes"] = fixture.confirmation.metadata["size_bytes"].clone();
    persist_test_application(&harness, &fixture.account_id, &application);

    let structurally_invalid_pdf_bytes = b"%PDF-1.7\nmissing cross-reference and trailer".to_vec();
    let structurally_invalid_pdf_sha256 =
        hex::encode(Sha256::digest(&structurally_invalid_pdf_bytes));
    let structurally_invalid_pdf_size =
        i64::try_from(structurally_invalid_pdf_bytes.len()).unwrap();
    let mut structurally_invalid_resume = fixture.resume.clone();
    structurally_invalid_resume.sha256 = structurally_invalid_pdf_sha256.clone();
    structurally_invalid_resume.metadata["size_bytes"] = json!(structurally_invalid_pdf_size);
    persist_test_application_evidence(&harness, &fixture.account_id, &structurally_invalid_resume);
    let resume_document = application.receipt["documents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["storageKey"] == fixture.resume.storage_key)
        .unwrap();
    resume_document["sha256"] = json!(structurally_invalid_pdf_sha256);
    let resume_manifest = application.receipt["evidenceObjects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["storageKey"] == fixture.resume.storage_key)
        .unwrap();
    resume_manifest["sha256"] = json!(structurally_invalid_resume.sha256);
    resume_manifest["sizeBytes"] = json!(structurally_invalid_pdf_size);
    persist_test_application(&harness, &fixture.account_id, &application);
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{}", fixture.resume.storage_key)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(structurally_invalid_pdf_bytes, "application/pdf"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    let invalid_pdf = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&resume_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_pdf.status(), StatusCode::CONFLICT);

    persist_test_application_evidence(&harness, &fixture.account_id, &fixture.resume);
    let resume_document = application.receipt["documents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["storageKey"] == fixture.resume.storage_key)
        .unwrap();
    resume_document["sha256"] = json!(fixture.resume.sha256);
    let resume_manifest = application.receipt["evidenceObjects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["storageKey"] == fixture.resume.storage_key)
        .unwrap();
    resume_manifest["sha256"] = json!(fixture.resume.sha256);
    resume_manifest["sizeBytes"] = fixture.resume.metadata["size_bytes"].clone();
    persist_test_application(&harness, &fixture.account_id, &application);

    let mut invalid_size = fixture.receipt.clone();
    invalid_size.metadata["size_bytes"] = json!(0);
    persist_test_application_evidence(&harness, &fixture.account_id, &invalid_size);
    let invalid_metadata = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&receipt_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_metadata.status(), StatusCode::CONFLICT);

    let mut cross_scope = fixture.receipt.clone();
    cross_scope.storage_key =
        "bluey-cloud/accounts/not-the-owner/jobs/evidence/receipt.json".to_string();
    persist_test_application_evidence(&harness, &fixture.account_id, &cross_scope);
    application.receipt["receiptObject"]["storageKey"] = json!(cross_scope.storage_key);
    persist_test_application(&harness, &fixture.account_id, &application);
    let invalid_scope = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&receipt_path)
                .header("authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_scope.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial]
async fn jobs_worker_round_trips_a_fenced_encrypted_browser_profile_snapshot() {
    const SIGNING_KEY: &str = "jobs-profile-snapshot-signing-key-at-least-32-bytes";
    const WORKER_ID: &str = "browser-profile-integration-worker";
    std::env::set_var("BLUEY_JOBS_WORKER_SIGNING_KEY", SIGNING_KEY);

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
            max_object_bytes: 25 * 1024 * 1024,
        });
    })
    .await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    let lease = jobs::claim_execution_lease(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        WORKER_ID,
    )
    .unwrap();
    let restore_path = format!("/api/jobs/internal/execution-leases/{run_id}/profile/restore");
    let store_path = format!("/api/jobs/internal/execution-leases/{run_id}/profile/store");
    let access_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "browser_profile_id": browser_profile_id,
        "lease_token": lease.lease_token,
        "fence": lease.fence
    });
    let timestamp = u64::try_from(chrono::Utc::now().timestamp()).unwrap();

    let empty_restore = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            &restore_path,
            "execution",
            WORKER_ID,
            timestamp,
            "profile-empty-restore-nonce-0001",
            &access_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(empty_restore.status(), StatusCode::NO_CONTENT);

    let encrypted = b"BLUEYJP2encrypted-integration-browser-profile";
    let sha256 = hex::encode(Sha256::digest(encrypted));
    let size_bytes = i64::try_from(encrypted.len()).unwrap();
    let profile_scope = hex::encode(Sha256::digest(browser_profile_id.as_bytes()));
    let object_key = format!(
        "bluey-cloud/accounts/{account_id}/jobs/browser-profiles/\
         {profile_scope}/generation/1/sha256/{sha256}.enc"
    );
    Mock::given(method("PUT"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            encrypted.as_slice(),
            "application/vnd.bluey.browser-profile+encrypted",
        ))
        .expect(3)
        .mount(&object_store)
        .await;

    let store_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "browser_profile_id": browser_profile_id,
        "lease_token": lease.lease_token,
        "fence": lease.fence,
        "expected_generation": 0,
        "envelope_version": 2,
        "sha256": sha256,
        "size_bytes": size_bytes,
        "encrypted_snapshot_base64": base64::engine::general_purpose::STANDARD.encode(encrypted)
    });
    let stored = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            &store_path,
            "execution",
            WORKER_ID,
            timestamp,
            "profile-store-nonce-0000000002",
            &store_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    let stored_status = stored.status();
    let stored_body = axum::body::to_bytes(stored.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        stored_status,
        StatusCode::OK,
        "browser profile store failed: {}",
        String::from_utf8_lossy(&stored_body)
    );
    let stored_json: serde_json::Value = serde_json::from_slice(&stored_body).unwrap();
    assert_eq!(stored_json["browser_profile_id"], browser_profile_id);
    assert_eq!(stored_json["generation"], 1);
    assert_eq!(stored_json["sha256"], sha256);
    assert_eq!(stored_json["size_bytes"], size_bytes);
    assert_eq!(stored_json["envelope_version"], 2);

    let replay = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            &store_path,
            "execution",
            WORKER_ID,
            timestamp,
            "profile-store-replay-nonce-00003",
            &store_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);

    let restored = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            &restore_path,
            "execution",
            WORKER_ID,
            timestamp,
            "profile-restore-nonce-00000004",
            &access_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(restored.status(), StatusCode::OK);
    let restored_body = axum::body::to_bytes(restored.into_body(), 64 * 1024)
        .await
        .unwrap();
    let restored_json: serde_json::Value = serde_json::from_slice(&restored_body).unwrap();
    assert_eq!(restored_json["browser_profile_id"], browser_profile_id);
    assert_eq!(restored_json["generation"], 1);
    assert_eq!(restored_json["sha256"], sha256);
    assert_eq!(restored_json["size_bytes"], size_bytes);
    assert_eq!(restored_json["envelope_version"], 2);
    assert_eq!(
        restored_json["encrypted_snapshot_base64"],
        base64::engine::general_purpose::STANDARD.encode(encrypted)
    );

    let forged_access = json!({
        "account_id": account_id,
        "application_id": application_id,
        "browser_profile_id": browser_profile_id,
        "lease_token": "forged-browser-profile-token",
        "fence": lease.fence
    });
    let forged = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            &restore_path,
            "execution",
            WORKER_ID,
            timestamp,
            "profile-forged-lease-nonce-0005",
            &forged_access,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(forged.status(), StatusCode::CONFLICT);

    let wrong_profile_access = json!({
        "account_id": account_id,
        "application_id": application_id,
        "browser_profile_id": "browser-profile-from-another-identity",
        "lease_token": lease.lease_token,
        "fence": lease.fence
    });
    let wrong_profile = harness
        .jobs_router
        .clone()
        .oneshot(signed_worker_json_request(
            &restore_path,
            "execution",
            WORKER_ID,
            timestamp,
            "profile-cross-scope-nonce-000006",
            &wrong_profile_access,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(wrong_profile.status(), StatusCode::CONFLICT);

    object_store.verify().await;
    std::env::remove_var("BLUEY_JOBS_WORKER_SIGNING_KEY");
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
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for object in [
        b"<< /Type /Catalog /Pages 2 0 R >>".as_slice(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".as_slice(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>".as_slice(),
    ] {
        offsets.push(pdf.len());
        let object_number = offsets.len();
        pdf.extend_from_slice(format!("{object_number} 0 obj\n").as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    pdf
}

fn valid_receipt_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0]).unwrap();
    }
    bytes
}

async fn mount_receipt_bundle_store(object_store: &MockServer) -> Arc<Mutex<Vec<u8>>> {
    mount_receipt_bundle_store_with_readback(object_store, false).await
}

async fn mount_receipt_bundle_store_with_readback(
    object_store: &MockServer,
    tamper: bool,
) -> Arc<Mutex<Vec<u8>>> {
    let bundle_path = r"^/bucket/bluey-cloud/accounts/[^/]+/jobs/applications/[^/]+/receipts/[^/]+/bundles/[^/]+/sha256/[0-9a-f]{64}\.json$";
    let stored_body = Arc::new(Mutex::new(Vec::new()));
    let put_body = Arc::clone(&stored_body);
    Mock::given(method("PUT"))
        .and(path_regex(bundle_path))
        .respond_with(move |request: &wiremock::Request| {
            *put_body.lock().unwrap() = request.body.clone();
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(object_store)
        .await;
    let get_body = Arc::clone(&stored_body);
    Mock::given(method("GET"))
        .and(path_regex(bundle_path))
        .respond_with(move |_request: &wiremock::Request| {
            let body = if tamper {
                b"{}".to_vec()
            } else {
                get_body.lock().unwrap().clone()
            };
            ResponseTemplate::new(200).set_body_raw(body, "application/json")
        })
        .expect(1)
        .mount(object_store)
        .await;
    stored_body
}

fn cloud_receipt_request(
    harness: &Harness,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> serde_json::Value {
    use sha2::{Digest, Sha256};

    let application = jobs::get_application(&harness.pool, account_id, application_id)
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
    let approved_job = application
        .receipt
        .pointer("/approved_execution/job")
        .cloned()
        .unwrap();
    let confirmation_url = format!(
        "{}/confirmation",
        approved_job["canonicalUrl"]
            .as_str()
            .expect("approved job has a canonical URL")
            .trim_end_matches('/')
    );
    let pdf = valid_receipt_pdf();
    let png = valid_receipt_png();
    let pdf_sha = hex::encode(Sha256::digest(&pdf));
    let png_sha = hex::encode(Sha256::digest(&png));
    let resume_key = "local-run/documents/resume.pdf";
    let screenshot_key = "local-run/final.png";
    json!({
        "account_id": account_id,
        "lease_token": lease_token,
        "fence": fence,
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
            "adapterVersion": "2026.07.1-beta.1",
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
            "job": approved_job,
            "documents": [{
                "kind": "resume",
                "versionId": resume_id,
                "storageKey": resume_key,
                "sha256": pdf_sha,
                "mediaType": "application/pdf"
            }],
            "events": [{
                "id": format!("{run_id}:provider-receipt"),
                "occurredAt": "2026-07-12T12:00:00Z",
                "type": "greenhouse_state_transition",
                "detail": {
                    "state": "receipt",
                    "status": "submitted",
                    "capability": "beta_review"
                }
            }],
            "result": {
                "status": "submitted",
                "submitHttpStatus": 302,
                "confirmationText": "Application received",
                "confirmationUrl": &confirmation_url,
                "submittedAt": "2026-07-12T12:00:00Z",
                "issues": []
            },
            "finalUrl": &confirmation_url,
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
) -> (String, i64) {
    jobs::reserve_application_attempt(&harness.pool, account_id, application_id, "cloud").unwrap();
    jobs::update_application(&harness.pool, account_id, application_id, "running", None)
        .unwrap()
        .unwrap();
    jobs::update_attempt_reservation_status(&harness.pool, account_id, application_id, "running")
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
    let capacity = cloud_submission_evidence_capacity(account_id, application_id, run_id);
    let final_submit_proof = final_submit_proof(harness, account_id, application_id);
    jobs::start_irreversible_submission(
        &harness.pool,
        account_id,
        application_id,
        run_id,
        &lease.lease_token,
        lease.fence,
        &final_submit_proof,
        &capacity,
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
    (lease.lease_token, lease.fence)
}

fn prepare_cloud_side_effect_unknown(
    harness: &Harness,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
) -> (String, i64) {
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
        "receipt-reconciliation-worker",
    )
    .unwrap();
    let capacity = cloud_submission_evidence_capacity(account_id, application_id, run_id);
    let final_submit_proof = final_submit_proof(harness, account_id, application_id);
    jobs::start_irreversible_submission(
        &harness.pool,
        account_id,
        application_id,
        run_id,
        &lease.lease_token,
        lease.fence,
        &final_submit_proof,
        &capacity,
    )
    .unwrap();
    jobs::update_application(
        &harness.pool,
        account_id,
        application_id,
        "side_effect_unknown",
        None,
    )
    .unwrap()
    .unwrap();
    jobs::update_attempt_reservation_status(
        &harness.pool,
        account_id,
        application_id,
        "side_effect_unknown",
    )
    .unwrap();
    jobs::upsert_browser_session(
        &harness.pool,
        account_id,
        &BrowserSession {
            id: run_id.to_string(),
            runner: "cloud".to_string(),
            status: "needs_input".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Submission outcome needs reconciliation".to_string(),
            application_id: Some(application_id.to_string()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();
    jobs::finish_execution_lease(
        &harness.pool,
        account_id,
        application_id,
        run_id,
        &lease.lease_token,
        lease.fence,
        "side_effect_unknown",
    )
    .unwrap();
    (lease.lease_token, lease.fence)
}

fn cloud_submission_evidence_capacity(
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> NewSubmissionEvidenceCapacity {
    submission_evidence_capacity(account_id, application_id, run_id, "cloud")
}

fn local_submission_evidence_capacity(
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> NewSubmissionEvidenceCapacity {
    submission_evidence_capacity(account_id, application_id, run_id, "local")
}

fn submission_evidence_capacity(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
) -> NewSubmissionEvidenceCapacity {
    const MAX_OBJECT_BYTES: i64 = 1024 * 1024;
    const MAX_RECEIPT_EVIDENCE_BYTES: i64 = 40 * 1024 * 1024;
    let now = chrono::Utc::now().timestamp_millis();
    NewSubmissionEvidenceCapacity {
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        runner: runner.to_string(),
        reserved_bytes: MAX_RECEIPT_EVIDENCE_BYTES + MAX_OBJECT_BYTES,
        reserved_objects: 13,
        expires_at_ms: now.saturating_add(jobs::SUBMISSION_RECONCILIATION_GRACE_MS),
        now_ms: now,
        limits: UploadLimits {
            max_object_bytes: MAX_OBJECT_BYTES,
            max_account_bytes: 128 * 1024 * 1024,
            max_daily_bytes: 128 * 1024 * 1024,
            max_account_objects: 100,
        },
    }
}

fn final_submit_proof(
    harness: &Harness,
    account_id: &str,
    application_id: &str,
) -> jobs::FinalSubmitProof {
    let application = jobs::get_application(&harness.pool, account_id, application_id)
        .unwrap()
        .expect("final-submit application exists");
    assert!(
        application
            .receipt
            .pointer("/approved_execution/packet/coverLetterContent")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|value| value.trim().is_empty()),
        "integration proof helper only supports a resume-only packet"
    );
    let canonical_url = application
        .receipt
        .pointer("/approved_execution/job/canonicalUrl")
        .and_then(serde_json::Value::as_str)
        .expect("final-submit application has a frozen canonical URL");
    let provider_url = reqwest::Url::parse(canonical_url).expect("final-submit URL is valid");
    let provider_segments = provider_url
        .path_segments()
        .expect("final-submit URL has path segments")
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(provider_segments.get(1), Some(&"jobs"));
    let provider_job_key = format!(
        "greenhouse:{}:{}",
        provider_segments.first().expect("Greenhouse tenant"),
        provider_segments.get(2).expect("Greenhouse job")
    );
    let resume_sha256 = hex::encode(Sha256::digest(valid_receipt_pdf()));
    jobs::FinalSubmitProof {
        schema_version: 3,
        adapter: "greenhouse".to_string(),
        adapter_version: "2026.07.1-beta.1".to_string(),
        control: "greenhouse_submit_application".to_string(),
        job: jobs::FinalSubmitJobProof {
            approved_canonical_url: canonical_url.to_string(),
            page_url: canonical_url.to_string(),
        },
        target: jobs::FinalSubmitTargetProof {
            action_url: canonical_url.to_string(),
            method: "post".to_string(),
            enctype: "multipart/form-data".to_string(),
            form_target: "_self".to_string(),
            provider_job_key,
            form_identity: r#"[0,"application-form","","","","",""]"#.to_string(),
        },
        files: vec![jobs::FinalSubmitFileProof {
            field_name: "resume".to_string(),
            name: format!("resume-{resume_sha256}.pdf"),
            byte_length: valid_receipt_pdf().len() as i64,
            sha256: resume_sha256.clone(),
        }],
        fields: vec![jobs::FinalSubmitFieldProof {
            field_name: "candidate_name".to_string(),
            value_byte_length: 0,
            value_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
        }],
        part_order: vec![
            jobs::FinalSubmitPartOrderProof {
                kind: "field".to_string(),
                index: 0,
            },
            jobs::FinalSubmitPartOrderProof {
                kind: "file".to_string(),
                index: 0,
            },
        ],
        documents: vec![jobs::FinalSubmitDocumentProof {
            kind: "resume".to_string(),
            version_id: application.resume_version_id,
            sha256: resume_sha256,
        }],
        certification: None,
        observed_surface: None,
    }
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
    const SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef";
    configure_runner_volume_purge_test_policy();
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    std::env::set_var("BLUEY_JOBS_WORKER_SIGNING_KEY", SIGNING_KEY);
    let harness = boot_harness_with_config(UpstreamKeys::default(), vec![], None, |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: "https://objects.example.test".to_string(),
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (volume, runtime) = setup_signed_runner_volume(&harness, SIGNING_KEY, now).await;
    let clone_instance_id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([65_u8; 32]);
    let instance_path = format!(
        "/api/jobs/internal/runner-volumes/{}/instances/claim",
        volume.volume_id
    );
    let clone_proof = jobs::sign_runner_volume_authority_proof(
        &volume.signing_key,
        jobs::NewRunnerVolumeAuthorityProof {
            operation: "instance_claim".to_string(),
            request_id: "runner-volume-clone-claim-proof-0001".to_string(),
            volume_id: volume.volume_id.clone(),
            enrollment_epoch: volume.enrollment_epoch,
            process_instance_id: clone_instance_id,
            issued_at_ms: chrono::Utc::now().timestamp_millis(),
            payload_sha256: runner_volume_instance_claim_payload_sha256(
                &instance_path,
                &volume.worker_id,
                &runtime.grant,
                &runtime.runtime_sha256,
            ),
        },
    )
    .unwrap();
    let concurrent_clone = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            &instance_path,
            "runner-volume",
            &volume.worker_id,
            now,
            "runner-volume-clone-claim-http-0001",
            &json!({
                "proof": clone_proof,
                "runtimeGrant": runtime.grant,
            }),
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(concurrent_clone.status(), StatusCode::CONFLICT);
    let claim_body = signed_execution_claim_body(
        &volume,
        &runtime,
        &volume.signing_key,
        &volume.worker_id,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        &volume.worker_id,
        "execution-lease-claim-success-0001",
    );

    let mut oversized_claim = claim_body.clone();
    oversized_claim["padding"] = json!("x".repeat(70 * 1024));
    let oversized_bytes = serde_json::to_vec(&oversized_claim).unwrap();
    assert!(oversized_bytes.len() > 64 * 1024);
    let mut oversized_request = signed_worker_request(SignedWorkerRequest {
        path: "/api/jobs/internal/execution-leases/claim",
        scope: "execution",
        worker_id: &volume.worker_id,
        timestamp: now,
        nonce: "execution-claim-body-limit-0001",
        signed_body: &oversized_bytes,
        actual_body: &oversized_bytes,
        signing_key: SIGNING_KEY,
    });
    oversized_request.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    let oversized = harness
        .router
        .clone()
        .oneshot(oversized_request)
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let owner_mismatch_body = signed_execution_claim_body(
        &volume,
        &runtime,
        &volume.signing_key,
        &volume.worker_id,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "forged-execution-owner",
        "execution-owner-binding-proof-0001",
    );
    let signed_owner_mismatch = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/execution-leases/claim",
            "execution",
            "signed-execution-worker",
            now,
            "execution-owner-binding-0001",
            &owner_mismatch_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(signed_owner_mismatch.status(), StatusCode::BAD_REQUEST);
    let lease_count: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM jobs_execution_leases WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lease_count, 0);

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

    let forged_volume_key = Ed25519SigningKey::from_bytes(&[91_u8; 32]);
    let fleet_hmac_only_body = signed_execution_claim_body(
        &volume,
        &runtime,
        &forged_volume_key,
        &volume.worker_id,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        &volume.worker_id,
        "execution-forged-volume-proof-0001",
    );
    let fleet_hmac_only = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/execution-leases/claim",
            "execution",
            &volume.worker_id,
            now,
            "execution-fleet-hmac-only-0001",
            &fleet_hmac_only_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(fleet_hmac_only.status(), StatusCode::UNAUTHORIZED);

    let forged_profile_body = signed_execution_claim_body(
        &volume,
        &runtime,
        &volume.signing_key,
        &volume.worker_id,
        &account_id,
        &application_id,
        &run_id,
        "forged:browser-profile",
        &volume.worker_id,
        "execution-forged-profile-proof-0001",
    );
    let forged_scope = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/execution-leases/claim",
            "execution",
            &volume.worker_id,
            now,
            "execution-forged-profile-http-0001",
            &forged_profile_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(forged_scope.status(), StatusCode::CONFLICT);

    let invalid_claim_body = signed_execution_claim_body(
        &volume,
        &runtime,
        &volume.signing_key,
        &volume.worker_id,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "",
        "execution-invalid-owner-proof-0001",
    );
    let invalid_claim = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/execution-leases/claim",
            "execution",
            &volume.worker_id,
            now,
            "execution-invalid-owner-http-0001",
            &invalid_claim_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(invalid_claim.status(), StatusCode::BAD_REQUEST);

    let missing_binding_body = signed_execution_claim_body(
        &volume,
        &runtime,
        &volume.signing_key,
        &volume.worker_id,
        &account_id,
        &application_id,
        "different-cloud-run",
        &browser_profile_id,
        &volume.worker_id,
        "execution-missing-run-proof-0001",
    );
    let missing_binding = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/execution-leases/claim",
            "execution",
            &volume.worker_id,
            now,
            "execution-missing-run-http-0001",
            &missing_binding_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    assert_eq!(missing_binding.status(), StatusCode::NOT_FOUND);
    let unclaimed_rows: (i64, i64, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM jobs_execution_leases WHERE run_id = ?1),
                (SELECT COUNT(*) FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1),
                (SELECT COUNT(*) FROM jobs_runner_volume_residencies)",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(unclaimed_rows, (0, 0, 0));

    let claimed = harness
        .router
        .clone()
        .oneshot(signed_worker_json_request(
            "/api/jobs/internal/execution-leases/claim",
            "execution",
            &volume.worker_id,
            now,
            "execution-lease-claim-http-0001",
            &claim_body,
            SIGNING_KEY,
        ))
        .await
        .unwrap();
    let claimed_status = claimed.status();
    let claimed_body = axum::body::to_bytes(claimed.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        claimed_status,
        StatusCode::OK,
        "signed runner-volume execution claim failed: {}",
        String::from_utf8_lossy(&claimed_body)
    );
    let lease: serde_json::Value = serde_json::from_slice(&claimed_body).unwrap();
    let grant_keys = lease
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        grant_keys,
        [
            "enrollment_epoch",
            "fence",
            "lease_expires_at_ms",
            "lease_token",
            "phase",
            "process_instance_id",
            "purge_subject",
            "run_id",
            "runtime_grant_id",
            "runtime_sha256",
            "volume_id",
            "volume_key_fingerprint",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    assert_eq!(lease["run_id"], run_id);
    assert_eq!(lease["phase"], "prepared");
    assert_eq!(lease["volume_id"], volume.volume_id);
    assert_eq!(lease["enrollment_epoch"], volume.enrollment_epoch);
    assert_eq!(lease["process_instance_id"], volume.process_instance_id);
    assert_eq!(lease["volume_key_fingerprint"], volume.key_fingerprint);
    assert_eq!(lease["runtime_grant_id"], runtime.grant.grant_id);
    assert_eq!(lease["runtime_sha256"], runtime.runtime_sha256);
    let purge_subject = lease["purge_subject"].as_str().unwrap().to_string();
    let lease_token = lease["lease_token"].as_str().unwrap();
    let fence = lease["fence"].as_i64().unwrap();
    let irreversible_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "lease_token": lease_token,
        "fence": fence,
        "action": "submit",
        "final_submit_proof": final_submit_proof(&harness, &account_id, &application_id)
    });
    let managed_release = jobs::ManagedCloudReleaseMemoAuthority {
        version: 1,
        execution: jobs::ManagedCloudExecutionAuthority {
            binding_sha256: "2".repeat(64),
            admission: jobs::ManagedCloudAdmissionAuthority {
                scope: jobs::ManagedCloudScope {
                    environment: "production".to_string(),
                    region: "us-east-1".to_string(),
                    channel: "canary".to_string(),
                },
                head_revision: 1,
                transition_sha256: "a".repeat(64),
                activation_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
                cohort_sha256: "d".repeat(64),
                trust_generation: 1,
                channel_sequence: 1,
                release_id: "release-effect-fence".to_string(),
                release_sequence: 1,
                task_queue_sha256: "e".repeat(64),
                failure_converter_sha256: "f".repeat(64),
                readiness_sha256: "1".repeat(64),
                activation_expires_at_ms: chrono::Utc::now().timestamp_millis() + 60_000,
                resolved_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        },
    };
    let managed_release_sha256 = jobs::managed_cloud_release_memo_sha256(&managed_release).unwrap();
    let managed_effect_body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "lease_token": lease_token,
        "fence": fence,
        "workflow_request_id": "wfreq-v2-12345678-1234-5abc-8def-123456789abc",
        "managed_cloud_release": managed_release,
        "managed_cloud_release_sha256": managed_release_sha256,
        "managed_cloud_runtime_instance_id": "runtime-instance-effect-fence",
        "managed_cloud_runtime_instance_epoch": 1
    });
    let mut managed_irreversible_body = irreversible_body.clone();
    managed_irreversible_body.as_object_mut().unwrap().extend(
        managed_effect_body
            .as_object()
            .unwrap()
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "lease_token" | "fence"))
            .map(|(key, value)| (key.clone(), value.clone())),
    );

    let durable_binding: (String, i64, String, String) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT volume_id, volume_epoch, process_instance_id, purge_subject_sha256
               FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(durable_binding.0, volume.volume_id);
    assert_eq!(durable_binding.1, volume.enrollment_epoch);
    assert_eq!(durable_binding.2, volume.process_instance_id);
    assert_eq!(
        durable_binding.3,
        jobs::runner_purge_subject_sha256(&purge_subject).unwrap()
    );
    let durable_residency: (String, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT state, purge_generation FROM jobs_runner_volume_residencies
              WHERE purge_subject = ?1 AND volume_id = ?2 AND volume_epoch = ?3",
            rusqlite::params![purge_subject, volume.volume_id, volume.enrollment_epoch],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(durable_residency, ("resident".to_string(), 0));

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

    let replacement_instance_id =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([64_u8; 32]);
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_runner_volumes SET active_instance_id = ?2 WHERE volume_id = ?1",
            rusqlite::params![volume.volume_id, replacement_instance_id],
        )
        .unwrap();
    let fenced_heartbeat = harness
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
    assert_eq!(fenced_heartbeat.status(), StatusCode::CONFLICT);
    let fenced_irreversible = harness
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
    assert_eq!(fenced_irreversible.status(), StatusCode::CONFLICT);
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_runner_volumes SET active_instance_id = ?2 WHERE volume_id = ?1",
            rusqlite::params![volume.volume_id, volume.process_instance_id],
        )
        .unwrap();

    let managed_effect_path =
        format!("/api/jobs/internal/execution-leases/{run_id}/authorize-managed-effect");
    let irreversible_path = format!("/api/jobs/internal/execution-leases/{run_id}/irreversible");
    let effect_fence_admin = Account::create_with_admin(
        &harness.pool,
        "jobs-effect-fence-admin@example.com",
        "integration-only-password-hash",
        true,
    )
    .expect("create the public-beta effect-fence audit actor");
    let beta_override = jobs_beta_access::get_public_beta_override(&harness.pool, &account_id)
        .expect("load initial public-beta override");
    jobs_beta_access::set_public_beta_override_audited(
        &harness.pool,
        &account_id,
        beta_override.revision,
        true,
        &effect_fence_admin.id,
    )
    .expect("deny the admitted account before an external effect");
    for (operation, path, body) in [
        (
            "authorize",
            managed_effect_path.as_str(),
            &managed_effect_body,
        ),
        (
            "submit",
            irreversible_path.as_str(),
            &managed_irreversible_body,
        ),
    ] {
        let response = harness
            .router
            .clone()
            .oneshot(signed_worker_json_request(
                path,
                "execution",
                &volume.worker_id,
                now,
                &format!("execution-public-beta-denied-{operation}-0001"),
                body,
                SIGNING_KEY,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
    let beta_override = jobs_beta_access::get_public_beta_override(&harness.pool, &account_id)
        .expect("load denied public-beta override");
    jobs_beta_access::set_public_beta_override_audited(
        &harness.pool,
        &account_id,
        beta_override.revision,
        false,
        &effect_fence_admin.id,
    )
    .expect("clear the account denial");

    let cohort = jobs_beta_access::get_public_beta_cohort(&harness.pool).unwrap();
    jobs_beta_access::update_public_beta_cohort_audited(
        &harness.pool,
        PublicBetaCohortUpdate {
            expected_revision: cohort.revision,
            state: PublicBetaCohortState::Suspended,
            opens_at_ms: cohort.opens_at_ms,
            closes_at_ms: cohort.closes_at_ms,
            hard_cap: cohort.hard_cap,
        },
        &effect_fence_admin.id,
    )
    .expect("suspend the cohort between lease claim and external effect");
    for (operation, path, body) in [
        (
            "authorize",
            managed_effect_path.as_str(),
            &managed_effect_body,
        ),
        (
            "submit",
            irreversible_path.as_str(),
            &managed_irreversible_body,
        ),
    ] {
        let response = harness
            .router
            .clone()
            .oneshot(signed_worker_json_request(
                path,
                "execution",
                &volume.worker_id,
                now,
                &format!("execution-public-beta-suspended-{operation}-0001"),
                body,
                SIGNING_KEY,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    let cohort = jobs_beta_access::get_public_beta_cohort(&harness.pool).unwrap();
    jobs_beta_access::update_public_beta_cohort_audited(
        &harness.pool,
        PublicBetaCohortUpdate {
            expected_revision: cohort.revision,
            state: PublicBetaCohortState::Open,
            opens_at_ms: cohort.opens_at_ms,
            closes_at_ms: cohort.closes_at_ms,
            hard_cap: cohort.hard_cap,
        },
        &effect_fence_admin.id,
    )
    .expect("restore the admitted cohort before the master-off race");
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "0");
    for (operation, path, body) in [
        (
            "authorize",
            managed_effect_path.as_str(),
            &managed_effect_body,
        ),
        (
            "submit",
            irreversible_path.as_str(),
            &managed_irreversible_body,
        ),
    ] {
        let response = harness
            .router
            .clone()
            .oneshot(signed_worker_json_request(
                path,
                "execution",
                &volume.worker_id,
                now,
                &format!("execution-public-beta-master-off-{operation}-0001"),
                body,
                SIGNING_KEY,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");
    let pre_effect_phase: String = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT phase FROM jobs_execution_leases WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pre_effect_phase, "prepared");

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
    let irreversible_status = irreversible.status();
    let irreversible_bytes = axum::body::to_bytes(irreversible.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        irreversible_status,
        StatusCode::OK,
        "irreversible transition failed: {}",
        String::from_utf8_lossy(&irreversible_bytes)
    );
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
    let uncertain_session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|session| session.id == run_id)
        .unwrap();
    assert_eq!(uncertain_session.status, "needs_input");
    assert_eq!(
        uncertain_session.current_step,
        "Submission outcome needs reconciliation"
    );

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
    std::env::remove_var("BLUEY_JOBS_WORKER_SIGNING_KEY");
}

#[tokio::test]
#[serial]
async fn jobs_checkpoint_reconciliation_route_is_authenticated_and_token_fenced() {
    const WORKER_TOKEN: &str = "jobs-checkpoint-reconciliation-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let harness = boot_harness().await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    let owner_id = "checkpoint-integration-worker";
    let claim = jobs::claim_execution_lease(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        owner_id,
    )
    .unwrap();
    let path = format!("/api/jobs/internal/execution-leases/{run_id}/reconcile-checkpoint");
    let body = json!({
        "account_id": account_id,
        "application_id": application_id,
        "owner_id": owner_id,
        "lease_token": claim.lease_token,
        "fence": claim.fence,
        "checkpoint_version": 2,
        "checkpoint_phase": "prepared"
    });

    let unauthorized = harness
        .router
        .clone()
        .oneshot(
            Request::post(&path)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let mut missing_token = body.clone();
    missing_token.as_object_mut().unwrap().remove("lease_token");
    let missing_token_response = harness
        .router
        .clone()
        .oneshot(
            Request::post(&path)
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&missing_token).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_token_response.status(), StatusCode::BAD_REQUEST);

    let mut forged = body.clone();
    forged["lease_token"] = json!("forged-checkpoint-token");
    let forged_response = harness
        .router
        .clone()
        .oneshot(
            Request::post(&path)
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&forged).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forged_response.status(), StatusCode::CONFLICT);

    for _ in 0..2 {
        let released = harness
            .router
            .clone()
            .oneshot(
                Request::post(&path)
                    .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = released.status();
        let bytes = axum::body::to_bytes(released.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "checkpoint reconciliation failed: {}",
            String::from_utf8_lossy(&bytes)
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["phase"], "released");
    }
    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "failed");
    assert!(application.submitted_at_ms.is_none());

    let unsafe_harness = boot_harness().await;
    let (unsafe_account_id, unsafe_application_id, unsafe_run_id, unsafe_browser_profile_id) =
        setup_execution_lease_run(&unsafe_harness).await;
    let unsafe_claim = jobs::claim_execution_lease(
        &unsafe_harness.pool,
        &unsafe_account_id,
        &unsafe_application_id,
        &unsafe_run_id,
        &unsafe_browser_profile_id,
        owner_id,
    )
    .unwrap();
    let unsafe_path =
        format!("/api/jobs/internal/execution-leases/{unsafe_run_id}/reconcile-checkpoint");
    let unsafe_body = json!({
        "account_id": unsafe_account_id,
        "application_id": unsafe_application_id,
        "owner_id": owner_id,
        "lease_token": unsafe_claim.lease_token,
        "fence": unsafe_claim.fence,
        "checkpoint_version": 2,
        "checkpoint_phase": "final_submit_started"
    });
    let uncertain = unsafe_harness
        .router
        .clone()
        .oneshot(
            Request::post(&unsafe_path)
                .header("authorization", format!("Bearer {WORKER_TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&unsafe_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(uncertain.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(uncertain.into_body(), 64 * 1024)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["phase"], "side_effect_unknown");
    let application = jobs::get_application(
        &unsafe_harness.pool,
        &unsafe_account_id,
        &unsafe_application_id,
    )
    .unwrap()
    .unwrap();
    assert_eq!(application.state, "side_effect_unknown");
    assert!(application.submitted_at_ms.is_none());
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
    let body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        "missing-lease-token",
        1,
    );

    let missing = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(missing.status(), StatusCode::CONFLICT);

    let lease = jobs::claim_execution_lease(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "nonterminal-receipt-worker",
    )
    .unwrap();
    let exact_nonterminal_body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease.lease_token,
        lease.fence,
    );
    let nonterminal = post_cloud_receipt(
        &harness,
        WORKER_TOKEN,
        &application_id,
        &exact_nonterminal_body,
    )
    .await;
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
async fn jobs_cloud_side_effect_unknown_can_be_reconciled_not_submitted() {
    const WORKER_TOKEN: &str = "jobs-cloud-owner-reconciliation-token";
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let harness = boot_harness().await;
    let (account_id, application_id, run_id, browser_profile_id) =
        setup_execution_lease_run(&harness).await;
    let (lease_token, fence) = prepare_cloud_side_effect_unknown(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );

    let auth = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let access_token = auth["access_token"].as_str().unwrap();
    let reconciliation_path =
        format!("/api/jobs/applications/{application_id}/reconcile-submission");
    let reconciliation_body = json!({
        "outcome": "not_submitted",
        "confirmed": true
    });
    for _ in 0..2 {
        let reconciled = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post(&reconciliation_path)
                    .header("authorization", format!("Bearer {access_token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&reconciliation_body).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reconciled.status(), StatusCode::OK);
    }

    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "failed");
    assert_eq!(
        application
            .receipt
            .pointer("/submission_reconciliation/outcome")
            .and_then(serde_json::Value::as_str),
        Some("not_submitted")
    );
    assert_eq!(
        jobs::execution_lease_phase_for_application(
            &harness.pool,
            &account_id,
            &application_id,
            &run_id,
        )
        .unwrap()
        .as_deref(),
        Some("released")
    );
    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_eq!(reservation.status, "released");
    let session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|session| session.id == run_id)
        .unwrap();
    assert_eq!(session.status, "failed");
    assert_eq!(session.current_step, "Confirmed not submitted");
    let evidence_capacity_state: String = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT state FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![&account_id, &application_id, &run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(evidence_capacity_state, "released");

    let receipt = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease_token,
        fence,
    );
    let late_receipt = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &receipt).await;
    assert_eq!(late_receipt.status(), StatusCode::CONFLICT);
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_cloud_side_effect_unknown_accepts_late_trusted_receipt() {
    const WORKER_TOKEN: &str = "jobs-cloud-late-receipt-token";
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
    let (lease_token, fence) = prepare_cloud_side_effect_unknown(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease_token,
        fence,
    );
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-screenshot-[0-9a-f]{20}$";
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
    let _bundle_body = mount_receipt_bundle_store(&object_store).await;

    let accepted = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(accepted.status(), StatusCode::OK);
    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "submitted");
    assert!(application.submitted_at_ms.is_some());
    assert_eq!(
        jobs::execution_lease_phase_for_application(
            &harness.pool,
            &account_id,
            &application_id,
            &run_id,
        )
        .unwrap()
        .as_deref(),
        Some("submitted")
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
    assert_eq!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .len(),
        3
    );
    let connection = harness.pool.get().unwrap();
    assert_eq!(
        connection
            .execute(
                "DELETE FROM jobs_submission_evidence_capacity
                  WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                rusqlite::params![&account_id, &application_id, &run_id],
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .execute(
                "DELETE FROM jobs_execution_leases
                  WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                rusqlite::params![&account_id, &application_id, &run_id],
            )
            .unwrap(),
        1
    );
    drop(connection);
    let exact_replay = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(exact_replay.status(), StatusCode::OK);
    let mut wrong_authority = body.clone();
    wrong_authority["lease_token"] = json!("z".repeat(43));
    let wrong_authority_replay =
        post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &wrong_authority).await;
    assert_eq!(wrong_authority_replay.status(), StatusCode::CONFLICT);

    let auth = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let access_token = auth["access_token"].as_str().unwrap();
    let owner_downgrade = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/applications/{application_id}/reconcile-submission"
            ))
            .header("authorization", format!("Bearer {access_token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "outcome": "not_submitted",
                    "confirmed": true
                }))
                .unwrap(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(owner_downgrade.status(), StatusCode::CONFLICT);
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_cloud_receipt_is_atomic_and_exactly_idempotent() {
    jobs_cloud_receipt_same_account(None).await;
}

#[tokio::test]
#[serial]
async fn jobs_cloud_receipt_after_local_smtp_signup() {
    // The regular suite runs the same journey with its existing HTTP mail mock.
    // The separately admitted SMTP pass requires the exact fixture toolchain.
    let Some(node) = std::env::var_os("BLUEY_TEST_LOCAL_SMTP_NODE") else {
        return;
    };
    jobs_cloud_receipt_same_account(Some(node.into())).await;
}

async fn jobs_cloud_receipt_same_account(smtp_node: Option<std::path::PathBuf>) {
    const WORKER_TOKEN: &str = "jobs-receipt-idempotency-token";
    const SMTP_EMAIL: &str = "candidate@bluey-qa.test";
    const PASSWORD: &str = "valid-password-123";
    let smtp = match smtp_node {
        Some(node) => Some(LocalSignupSmtp::start(node).await),
        None => None,
    };
    let smtp_port = smtp.as_ref().map(|fixture| fixture.port);
    let email = if smtp.is_some() {
        SMTP_EMAIL
    } else {
        "jobs-execution-lease@example.com"
    };
    std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", WORKER_TOKEN);
    let object_store = MockServer::start().await;
    let endpoint = object_store.uri();
    let upstream = UpstreamKeys {
        openai_api_key: Some("synthetic-receipt-prep-wiremock".to_string()),
        ..UpstreamKeys::default()
    };
    // Exercise actual spend accounting against the loopback-only mock.
    let harness = boot_harness_with_config(
        upstream,
        vec![],
        Some(UpstreamSpendGuard {
            limit_cents: 100,
            window_hours: 24,
        }),
        |config| {
            if let Some(port) = smtp_port {
                config.smtp = Some(SmtpConfig {
                    host: "127.0.0.1".into(),
                    port,
                    username: None,
                    password: None,
                    from: "Bluey Local QA <no-reply@bluey-qa.test>".into(),
                    starttls: false,
                });
            }
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
        },
    )
    .await;
    let verified_signup = if let Some(smtp) = smtp {
        assert!(Account::fetch_by_email(&harness.pool, email)
            .unwrap()
            .is_none());
        start_signup_otp(&harness, email, PASSWORD).await;
        let code = smtp.take_single_code_and_close().await;
        assert!(Account::fetch_by_email(&harness.pool, email)
            .unwrap()
            .is_none());
        let response = confirm_signup_otp(&harness, email, &code).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "local SMTP OTP confirmation"
        );
        let signup = response_json(response).await;
        assert_eq!(
            confirm_signup_otp(&harness, email, &code).await.status(),
            StatusCode::NOT_FOUND,
            "consumed signup code must not create another account"
        );
        assert!(harness.mail.received_requests().await.unwrap().is_empty());
        println!("jobs_fixture_local_smtp_signup_confirmed_once");
        Some((email, signup))
    } else {
        None
    };
    let seed = setup_execution_lease_seed(
        &harness,
        "cloud",
        true,
        Some(&object_store),
        verified_signup,
    )
    .await;
    assert!(harness.openai.received_requests().await.unwrap().is_empty());
    let candidate_access = seed.access_token.clone();
    let source = jobs::get_resume_source_asset(&harness.pool, &seed.account_id)
        .unwrap()
        .unwrap();
    let (account_id, application_id, run_id, browser_profile_id) =
        queue_execution_lease_fixture(&harness, "cloud", seed, true).await;
    let (lease_token, fence) = prepare_cloud_submission(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let pre_submission_application =
        jobs::get_application(&harness.pool, &account_id, &application_id)
            .unwrap()
            .unwrap();
    let pre_submission_receipt = pre_submission_application.receipt.clone();
    let approved_resume = pre_submission_receipt
        .pointer("/approved_execution/packet/resumeContent")
        .unwrap();
    let source_authority = approved_resume
        .pointer("/provenance/career_track_policy_authority")
        .unwrap();
    assert_eq!(source_authority["source_resume_asset_id"], source.id);
    assert_eq!(source_authority["source_resume_sha256"], source.sha256);
    let source_requests = object_store.received_requests().await.unwrap();
    assert_eq!(source_requests.len(), 2);
    for (request, method) in source_requests.iter().zip(["PUT", "GET"]) {
        assert_eq!(request.method.as_str(), method);
        assert_eq!(
            request.url.path(),
            format!("/bucket/{}", source.storage_key)
        );
    }
    println!("jobs_fixture_signup_upload_prepare_same_account");
    let frozen_job = pre_submission_receipt
        .pointer("/approved_execution/job")
        .cloned()
        .expect("approved execution must freeze the job snapshot");
    let body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease_token,
        fence,
    );
    let denied = post_cloud_receipt(&harness, &candidate_access, &application_id, &body).await;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let after_denial = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_denial.state, pre_submission_application.state);
    assert!(after_denial.submitted_at_ms.is_none());
    assert_eq!(after_denial.receipt, pre_submission_receipt);
    assert!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        object_store.received_requests().await.unwrap().len(),
        source_requests.len(),
        "a valid candidate session must not trigger worker evidence I/O"
    );
    println!("jobs_fixture_candidate_receipt_publication_refused");
    let mut live_posting = jobs::get_posting(
        &harness.pool,
        &account_id,
        &pre_submission_application.job_id,
    )
    .unwrap()
    .unwrap();
    live_posting.company = "Mutable Company Name".to_string();
    live_posting.title = "Mutable Posting Title".to_string();
    live_posting.canonical_url =
        "https://boards.greenhouse.io/acme/jobs/mutated-after-submit".to_string();
    let valid_posting_json: String = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
            rusqlite::params![&account_id, &pre_submission_application.job_id],
            |row| row.get(0),
        )
        .unwrap();
    let intentionally_invalid_posting_json = serde_json::to_string(&live_posting).unwrap();
    let replace_posting_fixture = |posting_json: &str| {
        assert_eq!(
            harness
                .pool
                .get()
                .unwrap()
                .execute(
                    "UPDATE jobs_postings SET posting_json = ?3 WHERE account_id = ?1 AND id = ?2",
                    rusqlite::params![
                        &account_id,
                        &pre_submission_application.job_id,
                        posting_json
                    ],
                )
                .unwrap(),
            1
        );
    };
    // The malformed live URL must not change the historic receipt.
    replace_posting_fixture(&intentionally_invalid_posting_json);
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-screenshot-[0-9a-f]{20}$";
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
    let bundle_body = mount_receipt_bundle_store(&object_store).await;

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
    assert_eq!(
        object_store.received_requests().await.unwrap().len(),
        source_requests.len(),
        "missing run may not touch receipt storage"
    );
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

    let mut missing_submit_status = body.clone();
    missing_submit_status["receipt"]["result"]
        .as_object_mut()
        .unwrap()
        .remove("submitHttpStatus");
    let missing_submit_status_response = post_cloud_receipt(
        &harness,
        WORKER_TOKEN,
        &application_id,
        &missing_submit_status,
    )
    .await;
    assert_eq!(
        missing_submit_status_response.status(),
        StatusCode::BAD_REQUEST
    );

    let mut not_modified_submit_status = body.clone();
    not_modified_submit_status["receipt"]["result"]["submitHttpStatus"] = json!(304);
    let not_modified_submit_status_response = post_cloud_receipt(
        &harness,
        WORKER_TOKEN,
        &application_id,
        &not_modified_submit_status,
    )
    .await;
    assert_eq!(
        not_modified_submit_status_response.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        object_store.received_requests().await.unwrap().len(),
        source_requests.len(),
        "invalid submit status may not touch receipt storage"
    );

    let first = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    let first_status = first.status();
    let first_bytes = axum::body::to_bytes(first.into_body(), 256 * 1024)
        .await
        .unwrap();
    assert_eq!(
        first_status,
        StatusCode::OK,
        "receipt failed: {}",
        String::from_utf8_lossy(&first_bytes)
    );
    let replay = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(replay.status(), StatusCode::OK);
    let mut wrong_authority = body.clone();
    wrong_authority["lease_token"] = json!("z".repeat(43));
    let wrong_authority_replay =
        post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &wrong_authority).await;
    assert_eq!(wrong_authority_replay.status(), StatusCode::CONFLICT);
    let mut changed = body.clone();
    changed["receipt"]["result"]["confirmationText"] = json!("Different receipt content");
    let conflicting = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &changed).await;
    assert_eq!(conflicting.status(), StatusCode::CONFLICT);

    let bundle_bytes = bundle_body.lock().unwrap().clone();
    let bundle: serde_json::Value = serde_json::from_slice(&bundle_bytes).unwrap();
    assert_eq!(bundle.get("job"), Some(&frozen_job));
    assert_eq!(bundle.pointer("/receipt/job"), Some(&frozen_job));
    assert_eq!(
        bundle
            .pointer("/receipt/result/submitHttpStatus")
            .and_then(serde_json::Value::as_i64),
        Some(302)
    );
    assert_ne!(
        bundle
            .pointer("/job/title")
            .and_then(serde_json::Value::as_str),
        Some("Mutable Posting Title")
    );
    assert_eq!(
        bundle
            .pointer("/schemaVersion")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        bundle.pointer("/receipt/_bluey_server_submission_authority_v1/preSubmissionReceipt"),
        Some(&pre_submission_receipt)
    );
    assert_eq!(
        bundle
            .pointer("/receipt/_bluey_server_submission_authority_v1/preSubmissionReceipt/approved_execution/packet")
            .and_then(serde_json::Value::as_object),
        pre_submission_receipt
            .pointer("/approved_execution/packet")
            .and_then(serde_json::Value::as_object)
    );

    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "submitted");
    assert_eq!(
        application
            .receipt
            .pointer("/result/submitHttpStatus")
            .and_then(serde_json::Value::as_i64),
        Some(302)
    );
    assert!(application
        .receipt
        .get("_bluey_server_submission_fingerprint_v1")
        .and_then(serde_json::Value::as_str)
        .is_some());
    let receipt_object = application
        .receipt
        .get("receiptObject")
        .and_then(serde_json::Value::as_object)
        .expect("submitted application must retain its immutable receipt object");
    let receipt_storage_key = receipt_object
        .get("storageKey")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let receipt_sha256 = receipt_object
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let expected_receipt_sha256 = hex::encode(Sha256::digest(&bundle_bytes));
    assert_eq!(receipt_sha256, expected_receipt_sha256);
    assert!(receipt_storage_key.ends_with(&format!("/sha256/{receipt_sha256}.json")));
    assert_eq!(
        receipt_object
            .get("mediaType")
            .and_then(serde_json::Value::as_str),
        Some("application/json")
    );
    assert_eq!(
        receipt_object
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        receipt_object
            .get("sizeBytes")
            .and_then(serde_json::Value::as_u64),
        Some(bundle_bytes.len() as u64)
    );
    let stored_manifest = bundle
        .pointer("/receipt/evidenceObjects")
        .and_then(serde_json::Value::as_array)
        .expect("immutable bundle must carry its exact evidence manifest");
    assert_eq!(stored_manifest.len(), 2);
    for (kind, expected_sha256, expected_size, media_type) in [
        (
            "resume",
            hex::encode(Sha256::digest(valid_receipt_pdf())),
            valid_receipt_pdf().len() as u64,
            "application/pdf",
        ),
        (
            "screenshot",
            hex::encode(Sha256::digest(valid_receipt_png())),
            valid_receipt_png().len() as u64,
            "image/png",
        ),
    ] {
        let stored = stored_manifest
            .iter()
            .find(|item| item.get("kind").and_then(serde_json::Value::as_str) == Some(kind))
            .expect("each employer-facing evidence object must be manifested");
        assert_eq!(
            stored.get("sha256").and_then(serde_json::Value::as_str),
            Some(expected_sha256.as_str())
        );
        assert_eq!(
            stored.get("sizeBytes").and_then(serde_json::Value::as_u64),
            Some(expected_size)
        );
        assert_eq!(
            stored.get("mediaType").and_then(serde_json::Value::as_str),
            Some(media_type)
        );
        let storage_key = stored
            .get("storageKey")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        assert!(storage_key.contains(&format!("-{kind}-")));
    }
    let evidence =
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id)).unwrap();
    assert_eq!(evidence.len(), 3);
    let resume_evidence = evidence
        .iter()
        .find(|item| item.kind == "resume")
        .expect("submitted resume evidence must be present");
    assert_eq!(
        resume_evidence.file_name,
        "Acme-Platform-Engineer-resume.pdf"
    );
    let receipt_evidence = evidence
        .iter()
        .find(|item| item.kind == "application_receipt")
        .expect("immutable receipt evidence must be committed atomically");
    assert_eq!(receipt_evidence.storage_key, receipt_storage_key);
    assert_eq!(receipt_evidence.sha256, receipt_sha256);
    assert_eq!(
        receipt_evidence.resume_version_id,
        application.resume_version_id
    );
    let (ready_uploads, total_uploads): (i64, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT SUM(CASE WHEN state = 'ready' THEN 1 ELSE 0 END), COUNT(*)
               FROM object_uploads
              WHERE account_id = ?1",
            rusqlite::params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((ready_uploads, total_uploads), (4, 4));
    let account_lifetime_objects: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM object_uploads
              WHERE account_id = ?1 AND expires_at_ms = ?2
                AND json_extract(metadata_json, '$.retention_policy') =
                    'account_lifetime_until_deletion'",
            rusqlite::params![&account_id, i64::MAX],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(account_lifetime_objects, 4);
    let retained_source = jobs::get_resume_source_asset(&harness.pool, &account_id)
        .unwrap()
        .unwrap();
    assert_eq!(retained_source.id, source.id);
    assert_eq!(retained_source.sha256, source.sha256);
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
    println!("jobs_fixture_mock_receipt_published_and_replayed");

    // Use the receipt just recorded above, not an invented submitted row.
    // The harness overrides the one configured model provider to a local mock.
    assert_eq!(
        std::env::var("BLUEY_TEST_OPENAI_URL").as_deref(),
        Ok(harness.openai.uri().as_str())
    );
    assert!(harness.openai.received_requests().await.unwrap().is_empty());
    let mock_content = "Practice draft: explain one design decision and how you verified it.";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": mock_content}}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 15}
        })))
        .expect(1)
        .mount(&harness.openai)
        .await;
    let auth = login(&harness, email, PASSWORD).await;
    let access = auth["access_token"].as_str().unwrap();
    assert_eq!(auth["account"]["id"], account_id);
    let expected_fingerprint =
        hex::encode(Sha256::digest(application.receipt.to_string().as_bytes()));
    // The normal workspace reads live source authority as well as history.
    // Restore its exact valid input without changing the submitted record.
    replace_posting_fixture(&valid_posting_json);
    // Scope diagnostics to this single synthetic request. Keep the production
    // error response generic; never emit its successful workspace body.
    use tracing::instrument::WithSubscriber;
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_test_writer()
        .without_time()
        .finish();
    let workspace = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .with_subscriber(subscriber)
        .await
        .unwrap();
    let workspace_status = workspace.status();
    let workspace_private_no_store = workspace
        .headers()
        .get("cache-control")
        .is_some_and(|value| value == "private, no-store");
    let workspace_bytes = axum::body::to_bytes(workspace.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert!(
        workspace_status == StatusCode::OK,
        "candidate workspace returned {workspace_status}"
    );
    assert!(workspace_private_no_store);
    let workspace_text = std::str::from_utf8(&workspace_bytes).unwrap();
    assert!(!workspace_text.contains(WORKER_TOKEN));
    assert!(!workspace_text.contains(&lease_token));
    let workspace: serde_json::Value = serde_json::from_str(workspace_text).unwrap();
    assert_eq!(workspace["profile"]["source_resume_asset_id"], source.id);
    assert_eq!(workspace["profile"]["source_resume_sha256"], source.sha256);
    let projected = workspace["applications"].as_array().unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0]["id"], application_id);
    assert_eq!(projected[0]["job_id"], application.job_id);
    assert_eq!(projected[0]["state"], "submitted");
    assert_eq!(
        projected[0]["resume_version_id"].as_str(),
        application.resume_version_id.as_deref()
    );
    assert_eq!(
        projected[0]["receipt"]["receiptId"],
        application.receipt["receiptId"]
    );
    assert_eq!(projected[0]["receipt"]["accountId"], account_id);
    let projected_evidence = workspace["application_evidence"].as_array().unwrap();
    assert_eq!(projected_evidence.len(), evidence.len());
    for row in &evidence {
        assert!(projected_evidence.iter().any(|item| {
            item["id"] == row.id
                && item["application_id"] == application_id
                && item["kind"] == row.kind
                && item["sha256"] == row.sha256
        }));
    }
    let receipt_fingerprint = workspace["interview_prep_receipts"][application_id.as_str()]
        .as_str()
        .unwrap();
    assert_eq!(receipt_fingerprint, expected_fingerprint);
    println!("jobs_fixture_candidate_workspace_receipt_observed");
    // Prep must continue to use the frozen job even when live input is bad.
    replace_posting_fixture(&intentionally_invalid_posting_json);
    let prep_path = format!("/api/jobs/applications/{application_id}/interview-prep");
    let prep_body = serde_json::to_string(&json!({
        "prepare": true,
        "receipt_fingerprint": receipt_fingerprint,
    }))
    .unwrap();
    let resume = jobs::get_resume_version(
        &harness.pool,
        &account_id,
        application.resume_version_id.as_deref().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(
        !resume.claim_ids.is_empty(),
        "the prepared resume must carry individually generated evidence IDs"
    );
    let verified_claim_ids = application
        .receipt
        .pointer("/packet/verifiedClaimIds")
        .unwrap();
    assert_eq!(
        verified_claim_ids,
        &json!([]),
        "generated evidence IDs are not independently confirmed facts"
    );
    assert_eq!(
        application.receipt.pointer(&format!(
            "/{}/preSubmissionReceipt/approved_execution/packet/verifiedClaimIds",
            jobs::SERVER_SUBMISSION_AUTHORITY_KEY,
        )),
        Some(verified_claim_ids),
        "the receipt must preserve the approved packet's exact verified claims"
    );
    let mut account_scope = Sha256::new();
    account_scope.update(b"bluey-provider-cost-hold-v1\0account");
    account_scope.update((account_id.len() as u64).to_be_bytes());
    account_scope.update(account_id.as_bytes());
    let account_scope = hex::encode(account_scope.finalize());
    // Balance, trial, root usage, provider-attempt usage, terminal reservation, cost hold.
    let census = || -> [i64; 6] {
        harness
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT a.balance_cents, a.trial_seconds_remaining,
                    (SELECT COUNT(*) FROM usage_events u WHERE u.account_id = a.id
                      AND u.kind = 'llm' AND u.request_id LIKE ?2),
                    (SELECT COUNT(*) FROM usage_events u WHERE u.account_id = a.id
                      AND u.kind = 'llm_attempt' AND u.request_id LIKE ?2),
                    (SELECT COUNT(*) FROM usage_reservations r WHERE r.account_id = a.id
                      AND r.status = 'settled' AND r.request_id LIKE ?2),
                    (SELECT COUNT(*) FROM jobs_provider_cost_holds h WHERE h.account_scope_hash = ?3
                      AND h.status = 'settled')
                 FROM accounts a WHERE a.id = ?1",
                rusqlite::params![
                    &account_id,
                    "bluey-internal:jobs-interview-prep-v3-%",
                    &account_scope
                ],
                |row| {
                    Ok([
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ])
                },
            )
            .unwrap()
    };
    let before = census();
    assert_eq!(before[0], 0);
    assert!(before[1] > 0);
    assert_eq!(&before[2..5], &[0, 0, 0]);
    let mut draft_id: Option<String> = None;
    let mut first_census = None;
    for _ in 0..2 {
        let response = harness
            .jobs_router
            .clone()
            .oneshot(
                Request::post(prep_path.as_str())
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(prep_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "frozen receipt interview prep: {}",
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(240)
                .collect::<String>()
        );
        let draft: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(draft["schema_version"], 1);
        assert_eq!(draft["application_id"], application_id.as_str());
        assert_eq!(draft["content"], mock_content);
        assert_eq!(draft["provider"], "openai");
        assert_eq!(draft["cost_cents"], 0);
        assert_eq!(draft["balance_cents_after"], 0);
        let after = census();
        assert_eq!(draft["trial_seconds_remaining"], after[1]);
        if let Some(first) = first_census {
            assert_eq!(after, first, "replay cannot settle usage again");
        } else {
            assert_eq!(after[0], before[0]);
            assert!(
                after[1] < before[1],
                "trial allowance should be consumed once"
            );
            assert_eq!(&after[2..5], &[1, 1, 1]);
            assert_eq!(after[5], before[5] + 1);
            first_census = Some(after);
        }
        assert_eq!(
            draft["grounding"]["receipt_id"],
            application.receipt["receiptId"]
        );
        assert_eq!(
            draft["grounding"]["receipt_fingerprint"],
            receipt_fingerprint
        );
        assert_eq!(draft["grounding"]["resume_version_id"], resume.id.as_str());
        assert_eq!(
            draft["grounding"]["resume_checksum"],
            resume.checksum.as_str()
        );
        assert_eq!(
            draft["grounding"]["resume_document_sha256"],
            resume_evidence.sha256.as_str()
        );
        let id = draft["id"].as_str().unwrap();
        assert!(id.starts_with("prep-") && id.len() == 29);
        if let Some(previous) = &draft_id {
            assert_eq!(id, previous.as_str());
        } else {
            draft_id = Some(id.to_string());
        }
    }
    let upstream_requests = harness.openai.received_requests().await.unwrap();
    assert_eq!(
        upstream_requests.len(),
        1,
        "draft replay must not dispatch twice"
    );
    let provider_body = std::str::from_utf8(&upstream_requests[0].body).unwrap();
    assert!(provider_body.contains(frozen_job["title"].as_str().unwrap()));
    assert!(!provider_body.contains("Mutable Posting Title"));
    assert!(!provider_body.contains(email));
    let after_prep = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_prep.state, "submitted");
    assert_eq!(after_prep.receipt, application.receipt);
}

#[tokio::test]
#[serial]
async fn jobs_concurrent_identical_cloud_receipts_replay_after_winner_commits() {
    const WORKER_TOKEN: &str = "jobs-receipt-concurrent-replay-token";
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
    let (lease_token, fence) = prepare_cloud_submission(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease_token,
        fence,
    );
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-screenshot-[0-9a-f]{20}$";
    Mock::given(method("PUT"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..=2)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(resume_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(valid_receipt_pdf(), "application/pdf"),
        )
        .expect(1..=2)
        .mount(&object_store)
        .await;
    let screenshot_put_calls = Arc::new(AtomicUsize::new(0));
    let screenshot_put_responder = Arc::clone(&screenshot_put_calls);
    Mock::given(method("PUT"))
        .and(path_regex(screenshot_path))
        .respond_with(move |_request: &wiremock::Request| {
            let response = ResponseTemplate::new(200);
            if screenshot_put_responder.fetch_add(1, Ordering::SeqCst) == 0 {
                response.set_delay(std::time::Duration::from_millis(500))
            } else {
                response
            }
        })
        .expect(1..=2)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(screenshot_path))
        .respond_with(ResponseTemplate::new(200).set_body_raw(valid_receipt_png(), "image/png"))
        .expect(1..=2)
        .mount(&object_store)
        .await;
    let _bundle_body = mount_receipt_bundle_store(&object_store).await;

    let first = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body);
    let second = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body);
    let (first, second) = tokio::join!(first, second);
    let mut committed = 0;
    let mut retryable = 0;
    for response in [first, second] {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap();
        match status {
            StatusCode::OK => committed += 1,
            StatusCode::BAD_GATEWAY => retryable += 1,
            _ => panic!(
                "concurrent exact receipt returned {status}: {}",
                String::from_utf8_lossy(&bytes)
            ),
        }
    }
    assert!(committed >= 1, "one concurrent request must commit");
    for _ in 0..retryable {
        let replay = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
        assert_eq!(replay.status(), StatusCode::OK);
    }

    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "submitted");
    assert_eq!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .len(),
        3
    );
    let (ready_uploads, total_uploads): (i64, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT SUM(CASE WHEN state = 'ready' THEN 1 ELSE 0 END), COUNT(*)
               FROM object_uploads
              WHERE account_id = ?1",
            rusqlite::params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((ready_uploads, total_uploads), (3, 3));
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_receipt_rejects_tampered_bundle_without_deleting_protected_retry_set() {
    const WORKER_TOKEN: &str = "jobs-receipt-tamper-token";
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
    let (lease_token, fence) = prepare_cloud_submission(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease_token,
        fence,
    );
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-screenshot-[0-9a-f]{20}$";
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
    let _bundle_body = mount_receipt_bundle_store_with_readback(&object_store, true).await;
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
    assert!(application.receipt.get("receiptObject").is_none());
    assert!(application
        .receipt
        .get("_bluey_server_submission_fingerprint_v1")
        .is_none());
    let protected_retry_states = harness
        .pool
        .get()
        .unwrap()
        .prepare("SELECT state FROM object_uploads WHERE account_id = ?1 ORDER BY created_at_ms")
        .unwrap()
        .query_map(rusqlite::params![account_id], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        protected_retry_states,
        vec!["pending", "pending", "pending"]
    );
    let capacity: (String, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT state, consumed_objects FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![account_id, application_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(capacity, ("active".to_string(), 3));
    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_ne!(reservation.status, "submitted");
    std::env::remove_var("BLUEY_JOBS_WORKER_TOKEN");
}

#[tokio::test]
#[serial]
async fn jobs_receipt_exact_retry_reuses_partial_uploads_without_duplicates() {
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
    let (lease_token, fence) = prepare_cloud_submission(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
    );
    let body = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        &lease_token,
        fence,
    );
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-screenshot-[0-9a-f]{20}$";
    Mock::given(method("PUT"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(resume_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(valid_receipt_pdf(), "application/pdf"),
        )
        .expect(2)
        .mount(&object_store)
        .await;
    let screenshot_put_attempts = Arc::new(AtomicUsize::new(0));
    let screenshot_put_responder = Arc::clone(&screenshot_put_attempts);
    Mock::given(method("PUT"))
        .and(path_regex(screenshot_path))
        .respond_with(move |_request: &wiremock::Request| {
            if screenshot_put_responder.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(500)
            } else {
                ResponseTemplate::new(200)
            }
        })
        .expect(2)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(screenshot_path))
        .respond_with(ResponseTemplate::new(200).set_body_raw(valid_receipt_png(), "image/png"))
        .expect(1)
        .mount(&object_store)
        .await;
    let _bundle_body = mount_receipt_bundle_store(&object_store).await;

    let first = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    assert_eq!(first.status(), StatusCode::BAD_GATEWAY);
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
    let pending_states = harness
        .pool
        .get()
        .unwrap()
        .prepare("SELECT state FROM object_uploads WHERE account_id = ?1 ORDER BY created_at_ms")
        .unwrap()
        .query_map(rusqlite::params![account_id], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(pending_states, vec!["pending", "pending"]);
    let capacity_before_retry: (String, i64, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT state, consumed_bytes, consumed_objects
               FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![account_id, application_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(capacity_before_retry.0, "active");
    assert_eq!(capacity_before_retry.2, 2);

    let retry = post_cloud_receipt(&harness, WORKER_TOKEN, &application_id, &body).await;
    let retry_status = retry.status();
    let retry_bytes = axum::body::to_bytes(retry.into_body(), 256 * 1024)
        .await
        .unwrap();
    assert_eq!(
        retry_status,
        StatusCode::OK,
        "exact receipt retry failed: {}",
        String::from_utf8_lossy(&retry_bytes)
    );
    assert_eq!(screenshot_put_attempts.load(Ordering::SeqCst), 2);
    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application.state, "submitted");
    let publication: (i64, i64, String, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT
                SUM(CASE WHEN state = 'ready' THEN 1 ELSE 0 END),
                COUNT(*),
                (SELECT state FROM jobs_submission_evidence_capacity
                  WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3),
                (SELECT consumed_objects FROM jobs_submission_evidence_capacity
                  WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3)
               FROM object_uploads WHERE account_id = ?1",
            rusqlite::params![account_id, application_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(publication, (3, 3, "committed".to_string(), 3));
    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_eq!(reservation.status, "submitted");
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
async fn jobs_intervention_answer_revises_packet_without_resuming_runner() {
    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
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
    let mut session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|item| item.id == run_id)
        .unwrap();
    session.status = "needs_input".to_string();
    session.current_step = "Waiting for a required answer".to_string();
    jobs::upsert_browser_session(&harness.pool, &account_id, &session).unwrap();

    let intervention = jobs::save_intervention(
        &harness.pool,
        &account_id,
        &Intervention {
            id: String::new(),
            application_id: Some(application_id.clone()),
            kind: "unknown_question".to_string(),
            status: "open".to_string(),
            title: "Years of production Rust experience?".to_string(),
            detail: "This answer will be included in the application packet.".to_string(),
            choices: Vec::new(),
            resolution_kind: "answer".to_string(),
            resume_after_resolution: true,
            provider: String::new(),
            provider_message_id: String::new(),
            expires_at_ms: None,
            metadata: json!({
                "receipt": {
                    "intervention": {
                        "field": "years_of_rust",
                        "question": "Years of production Rust experience?"
                    }
                }
            }),
            created_at_ms: 0,
            resolved_at_ms: None,
        },
    )
    .unwrap();
    let access_token =
        auth::jwt::issue(JWT_SECRET, &account_id, auth::jwt::TokenKind::Access).unwrap();

    let previous_answers = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap()
        .answers;

    let response = resolve_intervention_request(
        &harness,
        &access_token,
        &intervention.id,
        json!({
            "status": "resolved",
            "action": "answer",
            "answer": "Three years",
            "remember": true,
            "scope": "account"
        }),
    )
    .await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "answer resolution failed: {}",
        String::from_utf8_lossy(&bytes)
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["intervention"]["status"], "resolved");
    assert_eq!(value["application"]["state"], "awaiting_review");
    assert!(value["application"]["run_id"].is_null());
    assert_eq!(
        value["application"]["answers"][0]["key"],
        "years of production rust experience"
    );
    assert_eq!(
        value["application"]["answers"][0]["question"],
        "Years of production Rust experience?"
    );
    assert_eq!(value["application"]["answers"][0]["value"], "Three years");
    assert_eq!(
        value["application"]["receipt"]["final_answers"][0]["value"],
        "Three years"
    );
    assert!(value["local_resume"].is_null());
    assert_eq!(value["answer_memory"]["value"], "Three years");

    let application = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert!(application.receipt.get("approved_execution").is_none());
    assert_eq!(
        application.receipt["final_answers"][0]["key"],
        "years of production rust experience"
    );
    assert_eq!(
        application.receipt["final_answers"][0]["value"],
        "Three years"
    );
    assert_eq!(
        application.receipt["packet_revision"]["reason"],
        "intervention_answer"
    );
    assert_eq!(
        application.receipt["packet_revision"]["reapproval_required"],
        true
    );
    let revision_id = application.receipt["packet_revision"]["answer_revision_id"]
        .as_str()
        .expect("persisted answer revision pointer");
    let other = signup_and_login(
        &harness,
        "answer-history-other@example.test",
        "valid-password-123",
    )
    .await;
    let other_account = Account::fetch_by_email(&harness.pool, "answer-history-other@example.test")
        .unwrap()
        .unwrap();
    assert_eq!(
        jobs_beta_access::evaluate_or_enroll_public_beta(&harness.pool, &other_account.id)
            .unwrap()
            .reason,
        PublicBetaAccessReason::Admitted
    );
    for router in [harness.router.clone(), harness.jobs_router.clone()] {
        let uri = format!("/api/jobs/answer-revisions/{revision_id}");
        let history = router
            .clone()
            .oneshot(
                Request::get(&uri)
                    .header("authorization", format!("Bearer {access_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(history.status(), StatusCode::OK);
        assert_eq!(
            history.headers().get("cache-control").unwrap(),
            "private, no-store"
        );
        let history: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(history.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(history["previous_answers"], json!(previous_answers));
        assert_eq!(history["revised_answers"], json!(application.answers));
        assert_eq!(history["intervention_id"], intervention.id);
        let denied = router
            .clone()
            .oneshot(
                Request::get(&uri)
                    .header("authorization", format!("Bearer {other}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let missing = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/answer-revisions/missing")
                    .header("authorization", format!("Bearer {other}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::NOT_FOUND);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            axum::body::to_bytes(denied.into_body(), 1024)
                .await
                .unwrap(),
            axum::body::to_bytes(missing.into_body(), 1024)
                .await
                .unwrap()
        );
        let unauth = router
            .oneshot(Request::get(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    }
    let session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|item| item.id == run_id)
        .unwrap();
    assert_eq!(session.status, "paused");
    assert_eq!(
        session.current_step,
        "Application kit changed; review required"
    );
    assert!(!harness
        .openai
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(
            |request| request.url.path().contains("/workflows/applications/")
                && request.url.path().ends_with("/resume")
        ));
}

#[tokio::test]
#[serial]
async fn jobs_cloud_run_fails_closed_without_managed_launch_authority() {
    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
    let _managed_environment = TestEnvironmentGuard::install(&[
        (
            "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
            "1".to_string(),
        ),
        (
            "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED",
            "1".to_string(),
        ),
        (
            "BLUEY_JOBS_WORKFLOW_ORIGIN",
            "http://127.0.0.1:8787".to_string(),
        ),
        (
            "BLUEY_JOBS_WORKFLOW_TOKEN",
            "phase611-workflow-token-at-least-32-bytes".to_string(),
        ),
        (
            "BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT",
            "staging".to_string(),
        ),
        ("BLUEY_JOBS_MANAGED_CLOUD_REGION", "us-east-1".to_string()),
        ("BLUEY_JOBS_MANAGED_CLOUD_CHANNEL", "canary".to_string()),
    ]);
    let harness = boot_harness().await;
    let (account_id, application_id, _run_id, _) = setup_execution_lease_run(&harness).await;
    jobs::set_entitlement_plan(&harness.pool, &account_id, "cloud").unwrap();
    let _ready_fleet =
        enable_local_browser_distribution_for_test(&harness.pool, "managed-cloud-denied");
    let access_token =
        auth::jwt::issue(JWT_SECRET, &account_id, auth::jwt::TokenKind::Access).unwrap();

    let application_before = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    let sessions_before =
        serde_json::to_value(jobs::list_browser_sessions(&harness.pool, &account_id).unwrap())
            .unwrap();
    let used_packets_before = jobs::get_entitlement(&harness.pool, &account_id)
        .unwrap()
        .used_packets;
    let requests_before = harness.openai.received_requests().await.unwrap().len();
    let count_rows = |table: &str| -> i64 {
        let query = format!("SELECT COUNT(*) FROM {table} WHERE account_id = ?1");
        harness
            .pool
            .get()
            .unwrap()
            .query_row(&query, rusqlite::params![account_id], |row| row.get(0))
            .unwrap()
    };
    let commands_before = count_rows("jobs_workflow_commands");
    let reservations_before = count_rows("jobs_attempt_reservations");
    let managed_binding_count = || -> i64 {
        harness
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*)
                   FROM jobs_managed_cloud_workflow_bindings binding
                   JOIN jobs_workflow_commands command ON command.id = binding.command_id
                  WHERE command.account_id = ?1",
                rusqlite::params![account_id],
                |row| row.get(0),
            )
            .unwrap()
    };
    let bindings_before = managed_binding_count();

    let denied = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/applications/{application_id}/runs"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"runner":"cloud"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::SERVICE_UNAVAILABLE);
    let denied_body = axum::body::to_bytes(denied.into_body(), 64 * 1024)
        .await
        .unwrap();
    let denied_message = String::from_utf8(denied_body.to_vec()).unwrap();
    assert!(denied_message.contains(
        "Background runner is included in your plan but has not been enabled for this release."
    ));
    assert!(denied_message.contains(
        "Use Review first; Bluey will prepare the exact resume and answers for handoff."
    ));

    let application_after = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(application_after.state, application_before.state);
    assert_eq!(application_after.run_id, application_before.run_id);
    assert_eq!(application_after.receipt, application_before.receipt);
    assert_eq!(
        serde_json::to_value(jobs::list_browser_sessions(&harness.pool, &account_id).unwrap())
            .unwrap(),
        sessions_before
    );
    assert_eq!(count_rows("jobs_workflow_commands"), commands_before);
    assert_eq!(count_rows("jobs_attempt_reservations"), reservations_before);
    assert_eq!(managed_binding_count(), bindings_before);
    assert_eq!(
        jobs::get_entitlement(&harness.pool, &account_id)
            .unwrap()
            .used_packets,
        used_packets_before
    );
    assert_eq!(
        harness.openai.received_requests().await.unwrap().len(),
        requests_before
    );
}

#[tokio::test]
#[serial]
async fn jobs_workflow_command_routes_enforce_bounded_request_bodies() {
    const DEBUG_WORKER_TOKEN: &str = "phase611-debug-worker-body-limit-token";
    let _worker_environment = TestEnvironmentGuard::install(&[(
        "BLUEY_JOBS_WORKER_TOKEN",
        DEBUG_WORKER_TOKEN.to_string(),
    )]);
    let harness = boot_harness().await;
    let request_id = "wfreq-v2-12345678-1234-5abc-8def-123456789abc";
    let prepare_path =
        format!("/api/jobs/internal/workflow-commands/{request_id}/intervention/prepare");
    let oversized_prepare = harness
        .jobs_router
        .clone()
        .oneshot(debug_worker_oversized_json_request(
            &prepare_path,
            DEBUG_WORKER_TOKEN,
            256 * 1024,
        ))
        .await
        .unwrap();
    assert_eq!(oversized_prepare.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_workflow_command_response_headers(oversized_prepare.headers());

    for path in [
        format!("/api/jobs/internal/workflow-commands/{request_id}/materialize"),
        format!("/api/jobs/internal/workflow-commands/{request_id}/intervention/wfint-v2-123456789012/publish"),
        format!("/api/jobs/internal/workflow-commands/{request_id}/finalize"),
    ] {
        let oversized = harness
            .jobs_router
            .clone()
            .oneshot(debug_worker_oversized_json_request(
                &path,
                DEBUG_WORKER_TOKEN,
                16 * 1024,
            ))
            .await
            .unwrap();
        assert_eq!(
            oversized.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "workflow command route did not enforce the 16 KiB limit: {path}"
        );
        assert_workflow_command_response_headers(oversized.headers());
    }
}

#[tokio::test]
#[serial]
async fn jobs_local_queue_publishes_exact_snapshot_and_refuses_legacy_ticket_replay() {
    fn ticket_snapshot(ticket: &jobs::LocalRunTicket) -> serde_json::Value {
        json!({"id":ticket.id,"account":ticket.account_id,"application":ticket.application_id,
            "hash":ticket.ticket_hash,"secret":ticket.ticket_secret,"payload":ticket.payload,
            "status":ticket.status,"expiry":ticket.expires_at_ms,
            "created":ticket.created_at_ms,"updated":ticket.updated_at_ms})
    }
    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
    let harness = boot_harness().await;
    let (account_id, application_id, _, _) =
        setup_execution_lease_run_for_runner(&harness, "local").await;
    // Use the ordinary answer revision and reapproval lifecycle to retire the
    // fixture's prior run. Do not clear authority with an ad-hoc SQL update.
    jobs::update_application(
        &harness.pool,
        &account_id,
        &application_id,
        "needs_input",
        None,
    )
    .unwrap();
    let intervention = jobs::save_intervention(
        &harness.pool,
        &account_id,
        &serde_json::from_value(json!({
            "id":"", "application_id":application_id, "kind":"unknown_question",
            "status":"open", "title":"Why this role?", "detail":"Confirm the application answer.",
            "resolution_kind":"answer", "metadata":{"receipt":{"intervention":{
                "field":"why_this_role", "question":"Why this role?"
            }}}
        }))
        .unwrap(),
    )
    .unwrap();
    jobs::resolve_intervention_answer_for_review(
        &harness.pool,
        &account_id,
        &intervention.id,
        "The work matches my confirmed experience.",
    )
    .unwrap();
    let access = auth::jwt::issue(JWT_SECRET, &account_id, auth::jwt::TokenKind::Access).unwrap();
    let _distribution =
        enable_local_browser_distribution_for_test(&harness.pool, "publication-http");
    seed_canonical_browser_release_authority(&harness.pool, &account_id);
    let approved = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/applications/{application_id}/approve"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let original = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert!(original.run_id.is_none());
    let mut run_id = String::new();
    let mut retained_ticket: Option<jobs::LocalRunTicket> = None;
    for router in [harness.router.clone(), harness.jobs_router.clone()] {
        let response = router
            .oneshot(
                Request::post(format!("/api/jobs/applications/{application_id}/runs"))
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"runner":"local"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        let observed_run = body["run_id"].as_str().unwrap();
        if !run_id.is_empty() {
            assert_eq!(observed_run, run_id);
        }
        run_id = observed_run.to_string();
        assert_eq!(body["application"]["run_id"], run_id);
        assert_eq!(body["browser_session"]["id"], run_id);
        let ticket = jobs::get_local_run_ticket(&harness.pool, &account_id, &run_id)
            .unwrap()
            .unwrap();
        assert_eq!(ticket.payload["browserSessionId"], run_id);
        assert_eq!(ticket.payload["runId"], run_id);
        if let Some(previous) = &retained_ticket {
            assert_eq!(ticket_snapshot(&ticket), ticket_snapshot(previous));
        }
        retained_ticket = Some(ticket);
    }
    let ticket = retained_ticket.unwrap();
    let mut legacy_payload = ticket.payload.clone();
    legacy_payload["browserSessionId"] = json!(format!("local-{application_id}"));
    // Explicit malformed legacy-ticket negative fixture, not a positive
    // authority issuer. Publication must neither adopt nor reset these bytes.
    jobs::save_local_run_ticket(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
        &ticket.ticket_hash,
        &ticket.ticket_secret,
        legacy_payload,
        ticket.expires_at_ms,
    )
    .unwrap();
    let before = ticket_snapshot(
        &jobs::get_local_run_ticket(&harness.pool, &account_id, &run_id)
            .unwrap()
            .unwrap(),
    );
    let app_before = serde_json::to_value(
        jobs::get_application(&harness.pool, &account_id, &application_id).unwrap(),
    )
    .unwrap();
    let response = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/applications/{application_id}/runs"))
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"runner":"local"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        ticket_snapshot(
            &jobs::get_local_run_ticket(&harness.pool, &account_id, &run_id)
                .unwrap()
                .unwrap()
        ),
        before
    );
    assert_eq!(
        serde_json::to_value(
            jobs::get_application(&harness.pool, &account_id, &application_id).unwrap()
        )
        .unwrap(),
        app_before
    );
    assert!(harness.openai.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
#[serial]
async fn jobs_local_submit_resume_recovers_after_consume_before_marker() {
    jobs_local_submit_resume_recovers_after_consume_before_marker_case(false).await;
}

#[tokio::test]
#[serial]
async fn jobs_legacy_v1_local_result_resume_and_submitted_replay_remain_recovery_only() {
    jobs_local_submit_resume_recovers_after_consume_before_marker_case(true).await;
}

async fn jobs_local_submit_resume_recovers_after_consume_before_marker_case(
    use_legacy_capabilities: bool,
) {
    use sha2::{Digest, Sha256};

    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
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
        setup_execution_lease_run_for_runner(&harness, "local").await;
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
    let ticket_expires_at_ms = chrono::Utc::now().timestamp_millis()
        + if use_legacy_capabilities {
            15_000
        } else {
            60_000
        };
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
        ticket_expires_at_ms,
    )
    .unwrap();

    let _distribution =
        enable_local_browser_distribution_for_test(&harness.pool, "submit-resume-claim");
    seed_canonical_browser_release_authority(&harness.pool, &account_id);
    let queued_claim_state =
        local_browser_claim_state(&harness.pool, &account_id, &application_id, &run_id);
    let mut rejected_build_proof = canonical_browser_build_proof();
    let descriptor_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(rejected_build_proof["descriptor"].as_str().unwrap())
        .unwrap();
    let descriptor_text = String::from_utf8(descriptor_bytes).unwrap();
    let tampered_descriptor = descriptor_text.replacen("source_commit=1", "source_commit=2", 1);
    assert_ne!(tampered_descriptor, descriptor_text);
    rejected_build_proof["descriptor"] = json!(
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tampered_descriptor.as_bytes())
    );
    let rejected_claim_body =
        local_browser_claim_body_with_proof(&run_id, &ticket, rejected_build_proof);
    let rejected_claim = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&rejected_claim_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected_claim.status(), StatusCode::UPGRADE_REQUIRED);
    assert_eq!(
        local_browser_claim_state(&harness.pool, &account_id, &application_id, &run_id),
        queued_claim_state
    );

    let claim_body = local_browser_claim_body(&run_id, &ticket);
    {
        let _closed_effects = TestEnvironmentGuard::install(&[
            ("BLUEY_JOBS_BETA_ENABLED", "0".to_string()),
            (
                "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
                "0".to_string(),
            ),
        ]);
        let fresh_claim_while_closed = harness
            .router
            .clone()
            .oneshot(
                Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(fresh_claim_while_closed.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            local_browser_claim_state(&harness.pool, &account_id, &application_id, &run_id),
            queued_claim_state
        );
    }

    let claimed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let claimed_status = claimed.status();
    let claimed_bytes = axum::body::to_bytes(claimed.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        claimed_status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&claimed_bytes)
    );
    let claimed_state =
        local_browser_claim_state(&harness.pool, &account_id, &application_id, &run_id);
    assert_eq!(
        claimed_state,
        (
            "claimed".to_string(),
            "running".to_string(),
            "running".to_string(),
            "running".to_string(),
            1,
            1,
            1,
        )
    );
    {
        let _closed_effects = TestEnvironmentGuard::install(&[
            ("BLUEY_JOBS_BETA_ENABLED", "0".to_string()),
            (
                "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
                "0".to_string(),
            ),
        ]);
        let exact_claim_replay = harness
            .router
            .clone()
            .oneshot(
                Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&claim_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(exact_claim_replay.status(), StatusCode::OK);
        let exact_claim_replay_bytes =
            axum::body::to_bytes(exact_claim_replay.into_body(), 64 * 1024)
                .await
                .unwrap();
        assert_eq!(exact_claim_replay_bytes, claimed_bytes);
    }
    assert_eq!(
        local_browser_claim_state(&harness.pool, &account_id, &application_id, &run_id),
        claimed_state
    );

    let claim: serde_json::Value = serde_json::from_slice(&claimed_bytes).unwrap();
    let issued_result_capability = claim["_blueyCapabilities"]["result"]
        .as_str()
        .unwrap()
        .to_string();
    let issued_resume_capability = claim["_blueyCapabilities"]["resume"]
        .as_str()
        .unwrap()
        .to_string();
    let submit_capability = claim["_blueyCapabilities"]["submit"]
        .as_str()
        .unwrap()
        .to_string();
    let issued_result_payload = issued_result_capability.split_once('.').unwrap().0;
    let issued_result_claims: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(issued_result_payload)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(issued_result_claims["version"], 2);
    assert!(issued_result_claims["release"].is_object());

    let result_capability = if use_legacy_capabilities {
        legacy_local_run_capability(
            &account_id,
            &application_id,
            &run_id,
            &browser_profile_id,
            "result",
            ticket_expires_at_ms,
        )
    } else {
        issued_result_capability
    };
    let resume_capability = if use_legacy_capabilities {
        legacy_local_run_capability(
            &account_id,
            &application_id,
            &run_id,
            &browser_profile_id,
            "resume",
            ticket_expires_at_ms,
        )
    } else {
        issued_resume_capability
    };
    let legacy_submit_capability = use_legacy_capabilities.then(|| {
        legacy_local_run_capability(
            &account_id,
            &application_id,
            &run_id,
            &browser_profile_id,
            "submit",
            ticket_expires_at_ms,
        )
    });
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
    {
        let _closed_effects = TestEnvironmentGuard::install(&[
            ("BLUEY_JOBS_BETA_ENABLED", "0".to_string()),
            (
                "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
                "0".to_string(),
            ),
        ]);
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
    }
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

    let submit_proof = final_submit_proof(&harness, &account_id, &application_id);
    let raw_ticket_submit = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "ticket": &ticket,
                        "final_submit_proof": &submit_proof
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(raw_ticket_submit.status(), StatusCode::NOT_FOUND);
    if let Some(legacy_submit_capability) = legacy_submit_capability.as_ref() {
        let legacy_submit = harness
            .router
            .clone()
            .oneshot(
                Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "capability": legacy_submit_capability,
                            "final_submit_proof": &submit_proof
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(legacy_submit.status(), StatusCode::NOT_FOUND);
    }
    {
        let _closed_effects = TestEnvironmentGuard::install(&[
            ("BLUEY_JOBS_BETA_ENABLED", "0".to_string()),
            (
                "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
                "0".to_string(),
            ),
        ]);
        let fresh_submit_while_closed = harness
            .router
            .clone()
            .oneshot(
                Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "capability": &submit_capability,
                            "final_submit_proof": &submit_proof
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            fresh_submit_while_closed.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let (ticket_status, capacity_count): (String, i64) = harness
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status,
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                          WHERE capacity.account_id = ticket.account_id
                            AND capacity.application_id = ticket.application_id
                            AND capacity.run_id = ticket.id)
                   FROM jobs_local_run_tickets ticket WHERE ticket.id = ?1",
                rusqlite::params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "needs_input");
        assert_eq!(capacity_count, 0);
    }

    let unapproved_submit = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &result_capability,
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
                        "resolution": {
                            "kind": "browser_takeover",
                            "resumeAfter": true
                        }
                    }
                }
            }),
            created_at_ms: 0,
            resolved_at_ms: None,
        },
    )
    .unwrap();
    let rejected_generic = resolve_intervention_request(
        &harness,
        &access_token,
        &generic.id,
        json!({ "status": "resolved", "action": "approve_submission" }),
    )
    .await;
    assert_eq!(rejected_generic.status(), StatusCode::BAD_REQUEST);
    let cancelled_generic = resolve_intervention_request(
        &harness,
        &access_token,
        &generic.id,
        json!({ "status": "cancelled" }),
    )
    .await;
    assert_eq!(cancelled_generic.status(), StatusCode::OK);

    let rejected_answer = resolve_intervention_request(
        &harness,
        &access_token,
        &intervention.id,
        json!({
            "status": "resolved",
            "action": "approve_submission",
            "answer": "ignore the stored final review"
        }),
    )
    .await;
    assert_eq!(rejected_answer.status(), StatusCode::BAD_REQUEST);

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
                    serde_json::to_vec(&json!({ "capability": &resume_capability })).unwrap(),
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
                    serde_json::to_vec(&json!({ "capability": &resume_capability })).unwrap(),
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

    let mut job_b_proof = submit_proof.clone();
    job_b_proof.job.page_url =
        "https://boards.greenhouse.io/acme/jobs/different-official-job".to_string();
    let wrong_job = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &submit_capability,
                        "final_submit_proof": &job_b_proof
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_job.status(), StatusCode::CONFLICT);
    let (ticket_status, capacity_count): (String, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT ticket.status,
                    (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                      WHERE capacity.account_id = ticket.account_id
                        AND capacity.application_id = ticket.application_id
                        AND capacity.run_id = ticket.id)
               FROM jobs_local_run_tickets ticket WHERE ticket.id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ticket_status, "claimed");
    assert_eq!(capacity_count, 0);
    let application_after_wrong_job =
        jobs::get_application(&harness.pool, &account_id, &application_id)
            .unwrap()
            .unwrap();
    assert!(application_after_wrong_job
        .receipt
        .get("_bluey_final_submit_proof_v1")
        .is_none());

    let authorize_submit = || {
        Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "capability": &submit_capability,
                    "final_submit_proof": &submit_proof
                }))
                .unwrap(),
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

    {
        let _closed_effects = TestEnvironmentGuard::install(&[
            ("BLUEY_JOBS_BETA_ENABLED", "0".to_string()),
            (
                "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
                "0".to_string(),
            ),
        ]);
        let exact_submit_replay = harness
            .router
            .clone()
            .oneshot(authorize_submit())
            .await
            .unwrap();
        let exact_submit_replay_status = exact_submit_replay.status();
        let exact_submit_replay_body =
            axum::body::to_bytes(exact_submit_replay.into_body(), 64 * 1024)
                .await
                .unwrap();
        assert_eq!(
            exact_submit_replay_status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&exact_submit_replay_body)
        );
        assert_eq!(exact_submit_replay_body, authorized_body);
    }

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
                    serde_json::to_vec(&json!({ "capability": &resume_capability })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        expired.status(),
        if use_legacy_capabilities {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::CONFLICT
        }
    );
    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_local_run_resume_actions SET expires_at_ms = ?2 WHERE run_id = ?1",
            rusqlite::params![run_id, future_expiry],
        )
        .unwrap();

    if use_legacy_capabilities {
        let connection = harness.pool.get().unwrap();
        assert_eq!(
            connection
                .execute(
                    "DELETE FROM jobs_local_run_release_bindings
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    rusqlite::params![run_id, account_id, application_id],
                )
                .unwrap(),
            1
        );
        drop(connection);
        assert!(
            jobs::get_local_run_browser_release_binding(&harness.pool, &account_id, &run_id,)
                .unwrap()
                .is_none()
        );
        let wait_ms = ticket_expires_at_ms
            .saturating_sub(chrono::Utc::now().timestamp_millis())
            .max(0)
            .saturating_add(5) as u64;
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        assert!(chrono::Utc::now().timestamp_millis() > ticket_expires_at_ms);
        assert_eq!(
            jobs::get_local_run_ticket(&harness.pool, &account_id, &run_id)
                .unwrap()
                .unwrap()
                .status,
            "click_started"
        );
    }

    let mut receipt_request = cloud_receipt_request(
        &harness,
        &account_id,
        &application_id,
        &run_id,
        "unused-local-token",
        0,
    );
    let mut receipt_bundle = receipt_request["receipt"].take();
    receipt_bundle["runner"] = json!("local");
    let evidence_objects = receipt_request["evidence_objects"].take();
    let receipt_pdf = valid_receipt_pdf();
    let receipt_png = valid_receipt_png();
    let resume_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-resume-[0-9a-f]{20}$";
    let screenshot_path = r"^/bucket/bluey-cloud/accounts/[^/]+/context/jobs/[^/]+/receipts/[^/]+/[0-9]+-screenshot-[0-9a-f]{20}$";
    Mock::given(method("PUT"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(resume_path))
        .respond_with(ResponseTemplate::new(200).set_body_raw(receipt_pdf, "application/pdf"))
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
        .respond_with(ResponseTemplate::new(200).set_body_raw(receipt_png, "image/png"))
        .expect(1)
        .mount(&object_store)
        .await;
    let stored_bundle = mount_receipt_bundle_store(&object_store).await;
    let terminal_body = json!({
        "capability": &result_capability,
        "receipt": { "status": "submitted", "issues": [] },
        "receiptBundle": receipt_bundle,
        "evidenceObjects": evidence_objects,
    });
    let terminal = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&terminal_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let terminal_status = terminal.status();
    let terminal_bytes = axum::body::to_bytes(terminal.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert_eq!(
        terminal_status,
        StatusCode::OK,
        "local receipt failed: {}",
        String::from_utf8_lossy(&terminal_bytes)
    );
    let terminal_application: serde_json::Value = serde_json::from_slice(&terminal_bytes).unwrap();
    assert_eq!(terminal_application["state"], "submitted");
    assert_eq!(terminal_application["receipt"]["runner"], "local");
    assert_eq!(
        terminal_application["receipt"]["_bluey_server_submission_authority_v1"]
            ["executionAuthority"]["kind"],
        "local_run_ticket"
    );
    let execution_authority = terminal_application["receipt"]
        ["_bluey_server_submission_authority_v1"]["executionAuthority"]
        .as_object()
        .unwrap();
    if use_legacy_capabilities {
        assert_eq!(execution_authority.len(), 4);
        assert!(!execution_authority.contains_key("browserRelease"));
    } else {
        assert_eq!(execution_authority.len(), 5);
        assert_eq!(execution_authority["browserRelease"]["schemaVersion"], 2);
        assert_eq!(
            execution_authority["browserRelease"]["releaseId"],
            "browser-release-603-1"
        );
    }
    assert!(!stored_bundle.lock().unwrap().is_empty());
    assert_eq!(
        jobs::list_application_evidence(&harness.pool, &account_id, Some(&application_id))
            .unwrap()
            .len(),
        3
    );

    let connection = harness.pool.get().unwrap();
    assert_eq!(
        connection
            .execute(
                "DELETE FROM jobs_submission_evidence_capacity
                  WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                rusqlite::params![&account_id, &application_id, &run_id],
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .execute(
                "DELETE FROM jobs_local_run_tickets
                  WHERE account_id = ?1 AND application_id = ?2 AND id = ?3",
                rusqlite::params![&account_id, &application_id, &run_id],
            )
            .unwrap(),
        1
    );
    drop(connection);

    let exact_replay = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&terminal_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exact_replay.status(), StatusCode::OK);

    let mut conflicting_body = terminal_body;
    conflicting_body["receiptBundle"]["result"]["confirmationText"] =
        json!("A different confirmation");
    let conflicting_replay = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&conflicting_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflicting_replay.status(), StatusCode::CONFLICT);

    let after_terminal = harness
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
    assert_eq!(after_terminal.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn jobs_local_side_effect_unknown_is_terminal_and_requires_reconciliation() {
    use sha2::{Digest, Sha256};

    const JWT_SECRET: &str = "test-secret-at-least-32-chars-long-xxx";
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
        setup_execution_lease_run_for_runner(&harness, "local").await;
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
    let ticket_expires_at_ms = chrono::Utc::now().timestamp_millis() + 15_000;
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
        ticket_expires_at_ms,
    )
    .unwrap();
    let _distribution =
        enable_local_browser_distribution_for_test(&harness.pool, "side-effect-unknown-claim");
    seed_canonical_browser_release_authority(&harness.pool, &account_id);
    let legacy_result_capability = legacy_local_run_capability(
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "result",
        ticket_expires_at_ms,
    );
    let legacy_resume_capability = legacy_local_run_capability(
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "resume",
        ticket_expires_at_ms,
    );
    let legacy_submit_capability = legacy_local_run_capability(
        &account_id,
        &application_id,
        &run_id,
        &browser_profile_id,
        "submit",
        ticket_expires_at_ms,
    );
    let queued_legacy_result = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &legacy_result_capability,
                        "receipt": { "status": "failed" }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(queued_legacy_result.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        jobs::get_local_run_ticket(&harness.pool, &account_id, &run_id)
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );
    let claimed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/claim"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&local_browser_claim_body(&run_id, &ticket)).unwrap(),
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
    let submit_capability = claim["_blueyCapabilities"]["submit"]
        .as_str()
        .unwrap()
        .to_string();
    let result_payload = result_capability.split_once('.').unwrap().0;
    let result_claims: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(result_payload)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result_claims["version"], 2);
    assert!(result_claims["release"].is_object());

    let review_title = "Review the Greenhouse application";
    let review_detail = concat!(
        "Review every employer-facing field and document in the preserved form, ",
        "then approve submission."
    );
    let final_review = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &legacy_result_capability,
                        "receipt": {
                            "status": "needs_input",
                            "issues": [],
                            "intervention": {
                                "kind": "browser_takeover",
                                "title": review_title,
                                "detail": review_detail,
                                "takeoverUrl": format!(
                                    "bluey-jobs://resume/{run_id}?ticket={ticket}"
                                ),
                                "resolution": {
                                    "kind": "browser_takeover",
                                    "resumeAfter": true
                                }
                            }
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(final_review.status(), StatusCode::OK);
    let intervention = jobs::list_interventions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|item| item.title == review_title && item.status == "open")
        .unwrap();
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
    let consumed = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "capability": &legacy_resume_capability }))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(consumed.status(), StatusCode::OK);
    assert!(jobs::local_submission_approval_consumed(
        &harness.pool,
        &account_id,
        &application_id,
        &run_id,
    )
    .unwrap());

    append_canonical_browser_release_revocation(&harness.pool);
    assert!(matches!(
        jobs::local_browser_release_availability(
            &harness.pool,
            &account_id,
            TEST_BROWSER_SERVER_RELEASE_ID,
        )
        .unwrap(),
        jobs::LocalBrowserReleaseAvailability::Unavailable { .. }
    ));
    let submit_proof = final_submit_proof(&harness, &account_id, &application_id);
    let legacy_submit = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &legacy_submit_capability,
                        "final_submit_proof": &submit_proof
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(legacy_submit.status(), StatusCode::NOT_FOUND);
    let blocked_submit = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/authorize-submit"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &submit_capability,
                        "final_submit_proof": &submit_proof
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_submit.status(), StatusCode::CONFLICT);
    let blocked_submit_bytes = axum::body::to_bytes(blocked_submit.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&blocked_submit_bytes).contains("no longer authorized to submit")
    );
    let (ticket_status, capacity_count): (String, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT ticket.status,
                    (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                      WHERE capacity.account_id = ticket.account_id
                        AND capacity.application_id = ticket.application_id
                        AND capacity.run_id = ticket.id)
               FROM jobs_local_run_tickets ticket WHERE ticket.id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ticket_status, "claimed");
    assert_eq!(capacity_count, 0);

    let initial_capacity =
        local_submission_evidence_capacity(&account_id, &application_id, &run_id);
    bluey_server::db::object_uploads::reserve_submission_evidence_capacity(
        &harness.pool,
        &initial_capacity,
    )
    .unwrap();
    let mut pre_603_application =
        jobs::get_application(&harness.pool, &account_id, &application_id)
            .unwrap()
            .unwrap();
    pre_603_application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY] =
        serde_json::to_value(&submit_proof).unwrap();
    jobs::replace_application_receipt(
        &harness.pool,
        &account_id,
        &application_id,
        pre_603_application.receipt,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        harness
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'click_started', updated_at_ms = ?2
                  WHERE id = ?1 AND status = 'claimed'",
                rusqlite::params![run_id, chrono::Utc::now().timestamp_millis()],
            )
            .unwrap(),
        1
    );
    let wait_ms = ticket_expires_at_ms
        .saturating_sub(chrono::Utc::now().timestamp_millis())
        .max(0)
        .saturating_add(5) as u64;
    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
    assert!(chrono::Utc::now().timestamp_millis() > ticket_expires_at_ms);

    let uncertain_body = json!({
        "capability": &legacy_result_capability,
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
    let (capacity_runner, reserved_bytes, reserved_objects, capacity_state, capacity_expiry): (
        String,
        i64,
        i64,
        String,
        i64,
    ) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT runner, reserved_bytes, reserved_objects, state, expires_at_ms
               FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![account_id, application_id, run_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(capacity_runner, "local");
    assert_eq!(reserved_bytes, initial_capacity.reserved_bytes);
    assert_eq!(reserved_objects, initial_capacity.reserved_objects);
    assert_eq!(capacity_state, "active");
    assert!(
        capacity_expiry
            >= ticket_after
                .expires_at_ms
                .saturating_add(jobs::SUBMISSION_RECONCILIATION_GRACE_MS)
    );

    let downgrade = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &legacy_result_capability,
                        "receipt": { "status": "failed" }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(downgrade.status(), StatusCode::NOT_FOUND);
    let resume_replay = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/resume"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "capability": &legacy_resume_capability }))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resume_replay.status(), StatusCode::NOT_FOUND);
    let stored = jobs::get_application(&harness.pool, &account_id, &application_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, "side_effect_unknown");

    harness
        .pool
        .get()
        .unwrap()
        .execute(
            "UPDATE jobs_submission_evidence_capacity SET expires_at_ms = ?4
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![
                account_id,
                application_id,
                run_id,
                ticket_after.expires_at_ms.saturating_add(10_000),
            ],
        )
        .unwrap();
    let wait_ms = ticket_after
        .expires_at_ms
        .saturating_sub(chrono::Utc::now().timestamp_millis())
        .max(0)
        .saturating_add(5) as u64;
    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
    assert!(chrono::Utc::now().timestamp_millis() > ticket_after.expires_at_ms);
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
    let replay_capacity_expiry: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT expires_at_ms FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![account_id, application_id, run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        replay_capacity_expiry,
        ticket_after
            .expires_at_ms
            .saturating_add(jobs::SUBMISSION_RECONCILIATION_GRACE_MS)
    );

    let reconciliation_path =
        format!("/api/jobs/applications/{application_id}/reconcile-submission");
    let reconciliation_body = json!({
        "outcome": "not_submitted",
        "confirmed": true
    });
    let unauthenticated = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(&reconciliation_path)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&reconciliation_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let other_access = signup_and_login(
        &harness,
        "jobs-execution-lease-other@example.com",
        "valid-password-123",
    )
    .await;
    let cross_account = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(&reconciliation_path)
                .header("authorization", format!("Bearer {other_access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&reconciliation_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_account.status(), StatusCode::NOT_FOUND);

    let auth = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let access = auth["access_token"].as_str().unwrap();
    let unconfirmed = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(&reconciliation_path)
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "outcome": "not_submitted",
                        "confirmed": false
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unconfirmed.status(), StatusCode::BAD_REQUEST);

    let reconciled = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(&reconciliation_path)
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&reconciliation_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reconciled.status(), StatusCode::OK);
    let reconciled_bytes = axum::body::to_bytes(reconciled.into_body(), 64 * 1024)
        .await
        .unwrap();
    let reconciled_application: serde_json::Value =
        serde_json::from_slice(&reconciled_bytes).unwrap();
    assert_eq!(reconciled_application["state"], "failed");
    assert_eq!(
        reconciled_application["receipt"]["submission_reconciliation"]["outcome"],
        "not_submitted"
    );
    assert_eq!(
        reconciled_application["receipt"]["submission_reconciliation"]["resolved_by"],
        "account_owner"
    );
    let resolved_at_ms = reconciled_application["receipt"]["submission_reconciliation"]
        ["resolved_at_ms"]
        .as_i64()
        .unwrap();

    let reservation = jobs::list_attempt_reservations(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.application_id == application_id)
        .unwrap();
    assert_eq!(reservation.status, "released");
    let session = jobs::list_browser_sessions(&harness.pool, &account_id)
        .unwrap()
        .into_iter()
        .find(|session| session.id == run_id)
        .unwrap();
    assert_eq!(session.status, "failed");
    assert_eq!(session.current_step, "Confirmed not submitted");
    assert!(session.takeover_url.is_none());
    let ticket_after = jobs::get_local_run_ticket_by_hash(&harness.pool, &run_id, &ticket_hash)
        .unwrap()
        .unwrap();
    assert_eq!(ticket_after.status, "failed");
    let capacity_state: String = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT state FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
            rusqlite::params![account_id, application_id, run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(capacity_state, "released");

    let idempotent = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post(&reconciliation_path)
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&reconciliation_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idempotent.status(), StatusCode::OK);
    let idempotent_bytes = axum::body::to_bytes(idempotent.into_body(), 64 * 1024)
        .await
        .unwrap();
    let idempotent_application: serde_json::Value =
        serde_json::from_slice(&idempotent_bytes).unwrap();
    assert_eq!(
        idempotent_application["receipt"]["submission_reconciliation"]["resolved_at_ms"],
        resolved_at_ms
    );

    let conflicting_late_result = harness
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/jobs/local-runs/{run_id}/result"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "capability": &legacy_result_capability,
                        "receipt": { "status": "submitted", "issues": [] }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflicting_late_result.status(), StatusCode::NOT_FOUND);
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
    pool.get()
        .unwrap()
        .execute(
            "INSERT INTO accounts (
                id, email, password_hash, is_admin, trial_seconds_remaining, email_verified_at
             ) VALUES (?1, 'integration-public-beta-admin@example.com',
                       'integration-only-password-hash', 1, 0, datetime('now'))",
            rusqlite::params![TEST_PUBLIC_BETA_AUDIT_ACTOR],
        )
        .expect("seed the public-beta audit actor as a real admin account");
    let cohort = jobs_beta_access::get_public_beta_cohort(&pool).unwrap();
    let now_ms = chrono::Utc::now().timestamp_millis();
    jobs_beta_access::update_public_beta_cohort_audited(
        &pool,
        PublicBetaCohortUpdate {
            expected_revision: cohort.revision,
            state: PublicBetaCohortState::Open,
            opens_at_ms: Some(now_ms - 60_000),
            closes_at_ms: Some(now_ms + 24 * 60 * 60 * 1_000),
            hard_cap: 1_000,
        },
        TEST_PUBLIC_BETA_AUDIT_ACTOR,
    )
    .unwrap();

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
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");

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

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
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

async fn start_signup_otp(harness: &Harness, email: &str, password: &str) {
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
}

async fn confirm_signup_otp(
    harness: &Harness,
    email: &str,
    code: &str,
) -> axum::response::Response {
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
    harness.router.clone().oneshot(confirm).await.unwrap()
}

struct LocalSignupSmtp {
    child: tokio::process::Child,
    port: u16,
    output: tokio::io::BufReader<tokio::process::ChildStdout>,
}

impl LocalSignupSmtp {
    async fn start(node: std::path::PathBuf) -> Self {
        use std::io::Write as _;
        use std::process::Stdio;

        assert_eq!(
            (std::env::consts::OS, std::env::consts::ARCH),
            ("macos", "aarch64")
        );
        let run_root = std::path::PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").unwrap());
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        assert_eq!(source.parent(), Some(run_root.as_path()));
        assert_eq!(node.canonicalize().unwrap(), node);
        assert_eq!(
            hex::encode(Sha256::digest(std::fs::read(&node).unwrap())),
            "2cc28a1880d0774616959f2210ee739bffa6ac695c887e8988a496a21271f6c0"
        );
        assert!(std::env::var_os("BLUEY_MAIL_TRANSPORT").is_none());
        assert!(std::env::var_os("BLUEY_DEV_LOG_AUTH_LINKS").is_none());
        let fixture = run_root.join("local-smtp-child");
        std::fs::create_dir(&fixture).unwrap();
        for (name, bytes) in [
            (
                "local-qa-smtp-test-child.mjs",
                include_bytes!("../../jobs/scripts/local-qa-smtp-test-child.mjs").as_slice(),
            ),
            (
                "local-qa-smtp.mjs",
                include_bytes!("../../jobs/scripts/local-qa-smtp.mjs").as_slice(),
            ),
        ] {
            let path = fixture.join(name);
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            file.write_all(bytes).unwrap();
            drop(file);
            assert!(std::fs::read(path).unwrap() == bytes);
        }
        let mut child = tokio::process::Command::new(node)
            .arg(fixture.join("local-qa-smtp-test-child.mjs"))
            .current_dir(&run_root)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("start private local SMTP fixture");
        let mut output = tokio::io::BufReader::new(child.stdout.take().unwrap());
        let line = Self::frame(&mut output).await;
        let port = line
            .strip_prefix("ready ")
            .and_then(|v| v.strip_suffix('\n'))
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|port| *port > 1024)
            .expect("local SMTP listener did not report a bounded port");
        println!(
            "jobs_fixture_local_smtp_started pid={} port={port}",
            child.id().unwrap()
        );
        Self {
            child,
            port,
            output,
        }
    }

    async fn frame(output: &mut tokio::io::BufReader<tokio::process::ChildStdout>) -> String {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt};

        let mut text = String::new();
        let size = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            output.take(32).read_line(&mut text),
        )
        .await
        .expect("local SMTP private frame timed out")
        .expect("read local SMTP private frame");
        assert!(size < 32, "local SMTP private frame overflow");
        text
    }

    async fn take_single_code_and_close(mut self) -> String {
        use tokio::io::AsyncWriteExt;

        let mut channel = self.child.stdin.take().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            channel.write_all(b"take\n").await?;
            channel.shutdown().await
        })
        .await
        .expect("local SMTP consume timed out")
        .expect("consume local SMTP code");
        drop(channel);
        let line = Self::frame(&mut self.output).await;
        let code = line
            .strip_prefix("done ")
            .and_then(|value| value.strip_suffix('\n'))
            .filter(|value| value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit()))
            .expect("local SMTP did not capture exactly one signup code");
        assert!(
            Self::frame(&mut self.output).await.is_empty(),
            "unexpected SMTP output"
        );
        let status = tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait())
            .await
            .expect("local SMTP child did not stop")
            .expect("reap local SMTP child");
        assert!(status.success(), "local SMTP child failed");
        let closed = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.port)),
        )
        .await
        .expect("local SMTP closure check timed out");
        assert!(
            matches!(closed, Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused)
        );
        code.to_string()
    }
}

async fn signup_with_otp(harness: &Harness, email: &str, password: &str) -> serde_json::Value {
    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header("Authorization", "Bearer test-resend-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"email-otp"})))
        .mount(&harness.mail)
        .await;

    start_signup_otp(harness, email, password).await;
    let requests = harness.mail.received_requests().await.unwrap();
    let mail_body: serde_json::Value =
        serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    assert_eq!(
        mail_body["to"][0].as_str().unwrap(),
        email.trim().to_lowercase()
    );
    assert_eq!(mail_body["subject"], "Your Bluey verification code");
    let code = extract_six_digit_code(mail_body["text"].as_str().unwrap()).unwrap();
    let resp = confirm_signup_otp(harness, email, &code).await;
    assert_eq!(resp.status(), StatusCode::OK);
    response_json(resp).await
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
    let object_store = MockServer::start().await;
    let h = boot_account_delete_storage_harness(&object_store).await;

    let email = "delete-resignup@bluey.sh";
    let auth = signup_with_otp(&h, email, "longenoughpw").await;
    assert_eq!(auth["account"]["trial_seconds_remaining"], 900);
    let access = auth["access_token"].as_str().unwrap();
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    fence_account_deletion_then_revalidate_workflow_cleanup(&h, access).await;
    mount_empty_account_namespace_sweep(&object_store, &account.id).await;

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
async fn router_complete_upstream_spend_guard_counts_overlapping_provider_hold() {
    let h = boot_harness_with_options(
        UpstreamKeys {
            openai_api_key: Some("sk-test-openai".to_string()),
            anthropic_api_key: None,
            gemini_api_key: None,
            deepseek_api_key: None,
            zai_api_key: None,
            deepgram_api_key: None,
            ollama_base_url: None,
        },
        vec![],
        Some(UpstreamSpendGuard {
            // A one-token gpt-5.4-mini request has a two-cent conservative
            // Bluey ceiling (one rounded cent plus the safety margin). This
            // admits exactly one provider attempt at a time.
            limit_cents: 2,
            window_hours: 24,
        }),
    )
    .await;
    let email = "overlap-spend-guard@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    assert_eq!(
        h.pool
            .get()
            .unwrap()
            .execute(
                "UPDATE accounts SET trial_seconds_remaining = 0 WHERE id = ?1",
                [&account.id],
            )
            .unwrap(),
        1
    );
    assert!(bluey_server::db::balance::credit_internal(
        &h.pool,
        &account.id,
        100,
        "synthetic overlapping provider hold",
    )
    .unwrap());

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(5))
                .set_body_json(json!({
                    "choices": [{"message": {"content": "Done."}}],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                })),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let completion_request = |request_id: &str| {
        Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": request_id,
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "u",
                    "lane": "instant",
                    "max_tokens": 1
                }))
                .unwrap(),
            ))
            .unwrap()
    };
    let first_request = completion_request("overlap-spend-first");
    let second_request = completion_request("overlap-spend-second");
    let first_router = h.router.clone();
    let first = tokio::spawn(async move { first_router.oneshot(first_request).await.unwrap() });

    let projected_cost = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let (total, held, projected): (i64, i64, i64) = h
                .pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*),
                            COALESCE(SUM(CASE WHEN status = 'held' THEN 1 ELSE 0 END), 0),
                            COALESCE(MAX(projected_cost_cents), 0)
                       FROM jobs_provider_cost_holds",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            let provider_requests = h.openai.received_requests().await.unwrap().len();
            if (total, held, provider_requests) == (1, 1, 1) {
                break projected;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first provider attempt did not reach the durable held state");
    assert_eq!(projected_cost, 2);

    let second = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        h.router.clone().oneshot(second_request),
    )
    .await
    .expect("overlapping request reached the delayed provider")
    .unwrap();
    assert_eq!(second.status(), StatusCode::SERVICE_UNAVAILABLE);
    let second_body = response_json(second).await;
    assert_eq!(second_body["reason"], "upstream_spend_guard");
    assert_eq!(h.openai.received_requests().await.unwrap().len(), 1);

    let conn = h.pool.get().unwrap();
    let (total, held): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN status = 'held' THEN 1 ELSE 0 END), 0)
               FROM jobs_provider_cost_holds",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((total, held), (1, 1));
    let second_reservation_status: String = conn
        .query_row(
            "SELECT status FROM usage_reservations
              WHERE account_id = ?1 AND request_id = 'overlap-spend-second'",
            rusqlite::params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(second_reservation_status, "released");
    let second_cache_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM request_idempotency
              WHERE account_id = ?1 AND request_id = 'overlap-spend-second'",
            rusqlite::params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(second_cache_rows, 0, "temporary refusal must allow a retry");
    drop(conn);

    let first = tokio::time::timeout(std::time::Duration::from_secs(8), first)
        .await
        .expect("first provider response did not settle")
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    h.openai.verify().await;

    let conn = h.pool.get().unwrap();
    let provider_hold: (i64, i64, String, String, String) = conn
        .query_row(
            "SELECT projected_cost_cents, settled_cost_cents, status, provider,
                    usage_provenance
               FROM jobs_provider_cost_holds",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        provider_hold,
        (2, 1, "settled".into(), "openai".into(), "exact".into())
    );
    let first_reservation_status: String = conn
        .query_row(
            "SELECT status FROM usage_reservations
              WHERE account_id = ?1 AND request_id = 'overlap-spend-first'",
            rusqlite::params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(first_reservation_status, "settled");
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
    const REQUEST: &str = "test-idem-1";
    const INITIAL_CENTS: i64 = 100;
    let h = boot_harness().await;
    let email = "idem@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    assert_eq!(
        h.pool
            .get()
            .unwrap()
            .execute(
                "UPDATE accounts SET trial_seconds_remaining = 0 WHERE id = ?1",
                [&account.id],
            )
            .unwrap(),
        1
    );
    assert!(bluey_server::db::balance::credit_internal(
        &h.pool,
        &account.id,
        INITIAL_CENTS,
        "synthetic completion replay",
    )
    .unwrap());

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "First call"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2}
        })))
        .expect(1) // critical: only ONE upstream hit even on retry.
        .mount(&h.openai)
        .await;

    let request = || {
        Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": REQUEST,
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "hi",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap()
    };
    let read_completion = || async {
        let response = h.router.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    };
    let snapshot = || {
        // One SQLite statement reads all accounting layers at one snapshot.
        // This fresh fixture has no other provider-hold writer.
        let raw: String = h.pool.get().unwrap().query_row(
            "SELECT json_object(
                'balance', a.balance_cents, 'reserved_balance', a.reserved_cents,
                'trial', a.trial_seconds_remaining,
                'credits', (SELECT json_group_array(json_object('id', id,
                     'amount', amount_cents, 'remaining', remaining_cents))
                    FROM (SELECT * FROM credit_batches WHERE account_id = ?1 ORDER BY id)),
                'reservations', (SELECT json_group_array(json_object('request', request_id,
                     'kind', kind, 'status', status, 'attempt', attempt, 'upstream_estimate', estimated_upstream_cents,
                     'reserved', reserved_cents, 'actual', actual_customer_cents, 'settled', settled_cents,
                     'refund', refunded_cents, 'reason', terminal_reason, 'balance', balance_cents_after,
                     'settled_at', settled_at_ms))
                    FROM (SELECT * FROM usage_reservations WHERE account_id = ?1 ORDER BY request_id)),
                'holds', (SELECT json_group_array(json_object('id', request_scope_hash,
                     'status', status, 'provider', provider, 'model', model, 'projected', projected_cost_cents,
                     'settled', settled_cost_cents, 'provenance', usage_provenance, 'updated', updated_at_ms))
                    FROM (SELECT * FROM jobs_provider_cost_holds ORDER BY request_scope_hash)),
                'events', (SELECT json_group_array(json_object('id', id, 'request', request_id,
                     'kind', kind, 'origin', origin, 'provider', provider, 'model', model,
                     'input', input_tokens, 'output', output_tokens, 'bluey', cost_cents_to_bluey,
                     'customer', cost_cents_to_customer))
                    FROM (SELECT * FROM usage_events WHERE account_id = ?1 ORDER BY request_id, id)),
                'cache', (SELECT json_group_array(json_object('request', request_id, 'status', status,
                     'response', json(response_json), 'http', http_status, 'completed_at', completed_at))
                    FROM (SELECT * FROM request_idempotency WHERE account_id = ?1 ORDER BY request_id)),
                'balance_entries', (SELECT json_group_array(json_object('id', id, 'type', event_type,
                     'request', request_id, 'amount', amount_cents, 'before', balance_cents_before,
                     'after', balance_cents_after)) FROM (SELECT * FROM balance_ledger_entries
                         WHERE account_id = ?1 ORDER BY id)))
               FROM accounts a WHERE a.id = ?1",
            [&account.id],
            |row| row.get(0),
        ).unwrap();
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()
    };

    let initial = snapshot();
    assert_eq!(initial["balance"], INITIAL_CENTS);
    assert_eq!(initial["reserved_balance"], 0);
    assert_eq!(initial["trial"], 0);
    for field in ["reservations", "holds", "events", "cache"] {
        assert_eq!(initial[field], json!([]), "unexpected baseline {field}");
    }
    let first = read_completion().await;
    let durable = snapshot();
    let model = first["model"].as_str().unwrap();
    assert_eq!(first["provider"], "openai");
    assert_eq!(first["input_tokens"], 5);
    assert_eq!(first["output_tokens"], 2);
    let pricing = bluey_server::pricing::lookup("openai", model).unwrap();
    let (upstream, customer) = bluey_server::pricing::compute_cost(pricing, 5, 2);
    assert!(upstream > 0 && customer > 0, "must exercise a real debit");
    assert_eq!(first["cost_cents"], customer);
    assert_eq!(first["balance_cents_after"], INITIAL_CENTS - customer);
    assert_eq!(durable["balance"], INITIAL_CENTS - customer);
    assert_eq!(durable["reserved_balance"], 0);
    assert_eq!(durable["trial"], 0);
    assert_eq!(durable["credits"].as_array().unwrap().len(), 1);
    assert_eq!(durable["credits"][0]["id"], initial["credits"][0]["id"]);
    assert_eq!(durable["credits"][0]["amount"], INITIAL_CENTS);
    assert_eq!(durable["credits"][0]["remaining"], INITIAL_CENTS - customer);

    assert_eq!(durable["reservations"].as_array().unwrap().len(), 1);
    let reservation = &durable["reservations"][0];
    assert_eq!(reservation["request"], REQUEST);
    assert_eq!(reservation["kind"], "llm");
    assert_eq!(reservation["status"], "settled");
    assert_eq!(reservation["attempt"], 1);
    assert_eq!(reservation["upstream_estimate"], 0);
    assert_eq!(reservation["actual"], customer);
    assert_eq!(reservation["settled"], customer);
    assert_eq!(reservation["reason"], "completed");
    assert_eq!(reservation["balance"], INITIAL_CENTS - customer);
    let reserved = reservation["reserved"].as_i64().unwrap();
    let refund = reservation["refund"].as_i64().unwrap();
    assert_eq!(reserved, customer + refund);
    let entries = durable["balance_entries"].as_array().unwrap();
    let request_entries = entries
        .iter()
        .filter(|entry| entry["request"] == REQUEST)
        .collect::<Vec<_>>();
    assert_eq!(request_entries.len(), 2);
    for (kind, amount, before, after) in [
        (
            "usage_reserve",
            -reserved,
            INITIAL_CENTS,
            INITIAL_CENTS - reserved,
        ),
        (
            "usage_settle",
            refund,
            INITIAL_CENTS - reserved,
            INITIAL_CENTS - customer,
        ),
    ] {
        let entry = request_entries
            .iter()
            .find(|entry| entry["type"] == kind)
            .unwrap();
        assert_eq!(entry["amount"], amount);
        assert_eq!(entry["before"], before);
        assert_eq!(entry["after"], after);
    }

    assert_eq!(durable["holds"].as_array().unwrap().len(), 1);
    let hold = &durable["holds"][0];
    assert_eq!(hold["status"], "settled");
    assert_eq!(hold["provenance"], "exact");
    assert_eq!(hold["provider"], "openai");
    assert_eq!(hold["model"], model);
    assert!(hold["projected"].as_i64().unwrap() > 0);
    assert_eq!(hold["settled"], upstream);

    let events = durable["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    for (event, kind, request_id, bluey_cost, customer_cost) in [
        (&events[0], "llm", REQUEST.to_string(), 0, customer),
        (
            &events[1],
            "llm_attempt",
            format!("{REQUEST}:llm-attempt:0"),
            upstream,
            0,
        ),
    ] {
        assert_eq!(event["request"], request_id);
        assert_eq!(event["kind"], kind);
        assert_eq!(event["origin"], "server");
        assert_eq!(event["provider"], "openai");
        assert_eq!(event["model"], model);
        assert_eq!(event["input"], 5);
        assert_eq!(event["output"], 2);
        assert_eq!(event["bluey"], bluey_cost);
        assert_eq!(event["customer"], customer_cost);
    }
    assert_eq!(durable["cache"].as_array().unwrap().len(), 1);
    assert_eq!(durable["cache"][0]["request"], REQUEST);
    assert_eq!(durable["cache"][0]["status"], "complete");
    assert_eq!(durable["cache"][0]["http"], 200);
    assert_eq!(durable["cache"][0]["response"], first);

    assert_eq!(read_completion().await, first);
    assert_eq!(snapshot(), durable, "replay changed durable accounting");
    h.openai.verify().await;
}

#[tokio::test]
#[serial]
async fn router_complete_rolls_back_paid_settlement_when_idempotency_completion_refuses() {
    const REQUEST: &str = "settlement-idempotency-rollback";
    const INITIAL_CENTS: i64 = 100;
    let h = boot_harness().await;
    let email = "settlement-rollback@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    h.pool
        .get()
        .unwrap()
        .execute(
            "UPDATE accounts SET trial_seconds_remaining = 0 WHERE id = ?1",
            [&account.id],
        )
        .unwrap();
    assert!(bluey_server::db::balance::credit_internal(
        &h.pool,
        &account.id,
        INITIAL_CENTS,
        "synthetic settlement rollback",
    )
    .unwrap());

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "One provider result"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    // This trigger models the last durable mutation refusing the response cache.
    // The production settlement path must roll back every customer-side mutation,
    // while the independently committed provider-cost hold remains authoritative.
    h.pool
        .get()
        .unwrap()
        .execute_batch(&format!(
            "CREATE TRIGGER fail_test_idempotency_completion
             BEFORE UPDATE ON request_idempotency
             WHEN NEW.account_id = '{}' AND NEW.request_id = '{}'
             BEGIN
               SELECT RAISE(ABORT, 'synthetic idempotency completion refusal');
             END;",
            account.id, REQUEST,
        ))
        .unwrap();

    let request = || {
        Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": REQUEST,
                    "system": MANAGED_PROVIDER_BASE_CONTRACT,
                    "user": "hi",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap()
    };
    let response = h.router.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(error["reason"], "usage_settlement_pending");

    let conn = h.pool.get().unwrap();
    let (balance, reserved): (i64, i64) = conn
        .query_row(
            "SELECT balance_cents, reserved_cents FROM accounts WHERE id = ?1",
            [&account.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(reserved > 0, "the paid admission must remain held");
    assert_eq!(balance + reserved, INITIAL_CENTS);
    let credit_remaining: i64 = conn
        .query_row(
            "SELECT remaining_cents FROM credit_batches WHERE account_id = ?1",
            [&account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        credit_remaining, INITIAL_CENTS,
        "settlement must not consume FIFO credit"
    );
    let reservation: (String, i64, i64, i64) = conn
        .query_row(
            "SELECT status, actual_customer_cents, settled_cents, refunded_cents
               FROM usage_reservations WHERE account_id = ?1 AND request_id = ?2",
            rusqlite::params![account.id, REQUEST],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(reservation, ("reserved".into(), 0, 0, 0));
    let cache: (String, Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT status, response_json, http_status FROM request_idempotency
               WHERE account_id = ?1 AND request_id = ?2",
            rusqlite::params![account.id, REQUEST],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(cache, ("in_progress".into(), None, None));
    let settles: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_ledger_entries
               WHERE account_id = ?1 AND request_id = ?2 AND event_type = 'usage_settle'",
            rusqlite::params![account.id, REQUEST],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(settles, 0);
    let root_usage: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM usage_events WHERE account_id = ?1 AND request_id = ?2",
            rusqlite::params![account.id, REQUEST],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(root_usage, 0);
    let attempt_usage: (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MIN(kind) FROM usage_events
               WHERE account_id = ?1 AND request_id = ?2",
            rusqlite::params![account.id, format!("{REQUEST}:llm-attempt:0")],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(attempt_usage, (1, "llm_attempt".into()));
    let provider_hold: (i64, String, String) = conn
        .query_row(
            "SELECT COUNT(*), MIN(status), MIN(usage_provenance) FROM jobs_provider_cost_holds",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(provider_hold, (1, "settled".into(), "exact".into()));
    drop(conn);

    h.pool
        .get()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_test_idempotency_completion;")
        .unwrap();
    let retry = h.router.clone().oneshot(request()).await.unwrap();
    assert_eq!(retry.status(), StatusCode::CONFLICT);
    let retry_body = axum::body::to_bytes(retry.into_body(), 64 * 1024)
        .await
        .unwrap();
    let retry_error: serde_json::Value = serde_json::from_slice(&retry_body).unwrap();
    assert_eq!(retry_error["reason"], "request_in_progress");
    h.openai.verify().await;
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
    let object_store = MockServer::start().await;
    let h = boot_account_delete_storage_harness(&object_store).await;
    let email = "delete-confirm@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");

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

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
    mount_empty_account_namespace_sweep(&object_store, &account.id).await;

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
    let completed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(completed["deleted"], true);
    assert_eq!(completed["state"], "deleted");
    assert!(completed["deleted_at"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_none());
}

#[tokio::test]
#[serial]
async fn account_delete_without_cleanup_head_fences_then_later_binds_exact_authority() {
    let h = boot_harness().await;
    let email = "delete-workflow-cleanup-not-configured@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");

    let response = h
        .router
        .clone()
        .oneshot(
            Request::post("/account/delete")
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
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["state"], "pending_workflow_cleanup_configuration");
    assert_eq!(pending["object_count_deleted"], 0);

    let intent = bluey_server::db::account_data::account_deletion_intent(&h.pool, &account.id)
        .unwrap()
        .expect("missing cleanup configuration must still fence the account");
    let runner_purge_count: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM jobs_runner_purge_requests WHERE account_id = ?1",
            rusqlite::params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(runner_purge_count, 0);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let prepared = jobs::prepare_jobs_legacy_inventory_generation(
        &h.pool,
        &jobs::PrepareJobsLegacyInventoryGeneration {
            namespace: "bluey-jobs-delete-test".to_string(),
            visibility_cutoff_ms: 1_783_900_800_000,
            confirmation_age_ms: 1_000,
            now_ms,
        },
    )
    .unwrap();
    let rebound = bluey_server::db::account_data::begin_account_deletion_with_workflow_cleanup(
        &h.pool,
        &account.id,
        now_ms + 1,
        &prepared.authority,
    )
    .unwrap()
    .expect("fenced account should still exist");
    assert!(matches!(
        rebound.deletion,
        bluey_server::db::account_data::BeginAccountDeletionResult::Ready(_)
    ));
    let cleanup = rebound
        .workflow_cleanup
        .expect("retry must attach exact workflow cleanup authority");
    assert_eq!(cleanup.account_generation, intent.requested_at_ms);
    assert_eq!(cleanup.legacy_authority, prepared.authority);
    assert_eq!(cleanup.object_sweep_deleted_count, 0);
}

#[tokio::test]
#[serial]
async fn account_delete_pending_workflow_cleanup_never_starts_object_or_runner_sweep() {
    let object_store = MockServer::start().await;
    let storage = account_delete_storage_config(object_store.uri());
    let h = boot_harness_with_config(UpstreamKeys::default(), vec![], None, move |config| {
        config.object_storage = Some(storage);
    })
    .await;
    let now_ms = chrono::Utc::now().timestamp_millis();
    jobs::prepare_jobs_legacy_inventory_generation(
        &h.pool,
        &jobs::PrepareJobsLegacyInventoryGeneration {
            namespace: "bluey-jobs-pending-delete-test".to_string(),
            visibility_cutoff_ms: 1_783_900_800_000,
            confirmation_age_ms: 1_000,
            now_ms,
        },
    )
    .unwrap();
    let email = "delete-pending-workflow-cleanup@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let object_key = format!(
        "bluey-cloud/accounts/{}/context/pending-workflow-cleanup",
        account.id
    );
    let sync = Request::post("/sync/batch")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "sessions": [{
                    "session_id": "pending-workflow-cleanup-session",
                    "title": "Pending workflow cleanup",
                    "status": "active",
                    "created_at_ms": 1000,
                    "updated_at_ms": 2000
                }],
                "context_artifacts": [{
                    "artifact_id": "pending-workflow-cleanup-artifact",
                    "session_id": "pending-workflow-cleanup-session",
                    "kind": "document",
                    "title": "Must remain intact",
                    "text_preview": "private bytes",
                    "created_at_ms": 1400,
                    "metadata": {"object_key": object_key}
                }]
            }))
            .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        h.router.clone().oneshot(sync).await.unwrap().status(),
        StatusCode::OK
    );

    let response = h
        .router
        .clone()
        .oneshot(
            Request::post("/account/delete")
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
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["state"], "pending_workflow_cleanup");
    assert_eq!(pending["object_count_deleted"], 0);
    assert!(pending["request_id"]
        .as_str()
        .is_some_and(|value| value.starts_with("wfcleanupgen-v3-")));

    let intent = bluey_server::db::account_data::account_deletion_intent(&h.pool, &account.id)
        .unwrap()
        .expect("pending workflow cleanup must retain deletion fence");
    let cleanup = jobs::get_jobs_workflow_cleanup_deletion_status(
        &h.pool,
        &account.id,
        intent.requested_at_ms,
    )
    .unwrap()
    .expect("pending deletion must retain workflow-cleanup binding");
    assert!(!cleanup.complete);
    assert_eq!(cleanup.object_sweep_deleted_count, 0);
    let runner_purge_count: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM jobs_runner_purge_requests WHERE account_id = ?1",
            rusqlite::params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(runner_purge_count, 0);
    assert!(object_store.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
#[serial]
async fn account_delete_without_storage_preserves_account_and_deletion_fence() {
    configure_runner_volume_purge_test_policy();
    let h = boot_harness().await;
    authorize_empty_legacy_runner_inventory(
        &h.pool,
        "phase-602-storage-unavailable-delete-inventory",
    );
    authorize_empty_workflow_cleanup_inventory(&h.pool).await;
    let email = "delete-storage-unavailable@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
    let request = Request::post("/account/delete")
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
    let response = h.router.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(Account::fetch_by_id(&h.pool, &account.id)
        .unwrap()
        .is_some());
    assert!(
        bluey_server::db::account_data::account_deletion_intent(&h.pool, &account.id)
            .unwrap()
            .is_some(),
        "storage configuration failure must retain the durable deletion fence"
    );

    let response = h
        .router
        .clone()
        .oneshot(
            Request::post("/sync/batch")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("fenced new writes and launches"));

    let response = h
        .router
        .clone()
        .oneshot(
            Request::get("/account/me")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let retry = Request::post("/account/delete")
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
    let response = h.router.clone().oneshot(retry).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
#[serial]
async fn account_delete_uses_audit_fallback_and_sweeps_shared_namespace_once() {
    let object_store = MockServer::start().await;
    let h = boot_account_delete_storage_harness(&object_store).await;
    let email = "delete-audit-fallback@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let diagnostic_key = format!(
        "bluey-cloud/accounts/{}/sessions/delete/audit/fallback.json",
        account.id
    );
    let orphan_key = format!(
        "bluey-cloud/accounts/{}/audit/unindexed-orphan.json",
        account.id
    );
    let now_ms = chrono::Utc::now().timestamp_millis();
    bluey_server::db::diagnostic_logs::record_chunk(
        &h.pool,
        bluey_server::db::diagnostic_logs::DiagnosticLogChunkInput {
            id: Some("delete-audit-fallback".to_string()),
            account_id: Some(account.id.clone()),
            workspace_id: None,
            session_id: None,
            session_code: None,
            kind: "audit".to_string(),
            storage: "r2".to_string(),
            object_key: Some(diagnostic_key.clone()),
            local_path: None,
            bytes: 12,
            sha256: Some("a".repeat(64)),
            created_at_ms: now_ms,
            expires_at_ms: now_ms + 60_000,
            metadata_json: json!({}),
        },
    )
    .unwrap();

    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{diagnostic_key}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{orphan_key}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;

    let account_prefix = format!("bluey-cloud/accounts/{}/", account.id);
    let list_calls = Arc::new(AtomicUsize::new(0));
    let list_responder_calls = Arc::clone(&list_calls);
    let account_prefix_for_listing = account_prefix.clone();
    let orphan_key_for_listing = orphan_key.clone();
    Mock::given(method("GET"))
        .and(path("/bucket"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", account_prefix))
        .respond_with(move |_request: &wiremock::Request| {
            if list_responder_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_string(account_object_list_xml(
                    &account_prefix_for_listing,
                    &[&orphan_key_for_listing],
                ))
            } else {
                ResponseTemplate::new(200)
                    .set_body_string(account_object_list_xml(&account_prefix_for_listing, &[]))
            }
        })
        .expect(2)
        .mount(&object_store)
        .await;

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
    let request = Request::post("/account/delete")
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
    let response = h.router.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let ack: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(ack["object_count_deleted"], 2);
    assert_eq!(list_calls.load(Ordering::SeqCst), 2);
    assert!(Account::fetch_by_id(&h.pool, &account.id)
        .unwrap()
        .is_none());
}

#[tokio::test]
#[serial]
async fn account_delete_freezes_unattested_offline_volume_and_returns_durable_pending() {
    configure_runner_volume_purge_test_policy();
    let harness = boot_harness().await;
    let email = "delete-unattested-offline-volume@example.com";
    let access = signup_and_login(&harness, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .expect("account should exist");
    let volume = enroll_unattested_offline_runner_volume(&harness.pool, 87, "delete-pending");
    let fleet = authorize_empty_legacy_runner_inventory(
        &harness.pool,
        "phase-602-offline-delete-inventory",
    );
    authorize_empty_workflow_cleanup_inventory(&harness.pool).await;
    assert_eq!(fleet.non_destroyed_volume_count, 1);
    assert_eq!(fleet.storage_attestation_count, 0);

    let delete_request = || {
        Request::post("/account/delete")
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
            .unwrap()
    };
    fence_account_deletion_then_revalidate_workflow_cleanup(&harness, &access).await;
    let response = harness
        .router
        .clone()
        .oneshot(delete_request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response.headers().get("retry-after").unwrap(), "5");
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["state"], "pending_runner_volume_purge");
    assert_eq!(pending["required_target_count"], 1);
    assert_eq!(pending["resolved_target_count"], 0);
    let deletion_request_id = pending["request_id"].as_str().unwrap();

    let frozen: (i64, String) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*), MIN(t.volume_id) \
               FROM jobs_runner_purge_targets t \
               JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
              WHERE r.deletion_request_id = ?1",
            rusqlite::params![deletion_request_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(frozen, (1, volume.volume_id));
    assert!(Account::fetch_by_id(&harness.pool, &account.id)
        .unwrap()
        .is_some());
    assert!(
        bluey_server::db::account_data::account_deletion_intent(&harness.pool, &account.id,)
            .unwrap()
            .is_some()
    );

    let replay = harness
        .router
        .clone()
        .oneshot(delete_request())
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    let replay_body = axum::body::to_bytes(replay.into_body(), 64 * 1024)
        .await
        .unwrap();
    let replay_pending: serde_json::Value = serde_json::from_slice(&replay_body).unwrap();
    assert_eq!(replay_pending["request_id"], deletion_request_id);
    assert_eq!(replay_pending["required_target_count"], 1);
    assert_eq!(replay_pending["resolved_target_count"], 0);
}

#[tokio::test]
#[serial]
async fn account_delete_fences_then_waits_for_legacy_runner_reconciliation() {
    configure_runner_volume_purge_test_policy();
    let admin_email = "delete-cloud-runner-admin@example.com";
    let h = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec![admin_email.to_string()],
    )
    .await;
    authorize_empty_workflow_cleanup_inventory(&h.pool).await;
    let admin_access = signup_and_login(&h, admin_email, "longenoughpw").await;
    let email = "delete-cloud-runner-cleanup@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    jobs::upsert_browser_session(
        &h.pool,
        &account.id,
        &BrowserSession {
            id: "delete-cloud-runner-session".to_string(),
            runner: "cloud".to_string(),
            status: "needs_input".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Waiting for user input".to_string(),
            application_id: None,
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .unwrap();

    let delete_request = || {
        Request::post("/account/delete")
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
            .unwrap()
    };
    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
    let response = h.router.clone().oneshot(delete_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response.headers().get("retry-after").unwrap(), "5");
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["deleted"], false);
    assert_eq!(pending["state"], "pending_runner_legacy_inventory");
    let request_id = pending["request_id"].as_str().unwrap().to_string();
    assert!(!request_id.is_empty());
    assert!(pending.get("required_target_count").is_none());
    assert!(pending.get("resolved_target_count").is_none());
    assert!(pending.get("legacy_unresolved_count").is_none());
    assert!(Account::fetch_by_id(&h.pool, &account.id)
        .unwrap()
        .is_some());
    assert!(
        bluey_server::db::account_data::account_deletion_intent(&h.pool, &account.id)
            .unwrap()
            .is_some(),
        "legacy runner reconciliation must happen after the durable deletion fence"
    );

    let reconciliation_id = "phase-602-http-account-delete-inventory";
    let scope_ref = "phase-602-http-empty-legacy-root-scope";
    let authority_request = |body: serde_json::Value| {
        Request::post("/admin/jobs/runner-volumes/fleet/legacy-inventory-authority")
            .header("authorization", format!("Bearer {admin_access}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };
    let response = h
        .router
        .clone()
        .oneshot(authority_request(json!({
            "reconciliationId": reconciliation_id,
            "authorityState": "reconciling",
            "expectedPredecessorGeneration": 0,
            "rootCount": 0,
            "rootSetSha256": jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256,
            "scopeRef": scope_ref,
            "evidenceRef": "phase-602-http-account-delete-inventory-reconciling",
            "evidenceSha256": hex::encode(Sha256::digest(
                b"phase-602-http-account-delete-inventory-reconciling"
            ))
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let reconciling: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(reconciling["disposition"], "applied");
    assert_eq!(reconciling["authority"]["authorityState"], "reconciling");

    let response = h.router.clone().oneshot(delete_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let still_reconciling: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        still_reconciling["state"],
        "pending_runner_legacy_inventory"
    );
    assert_eq!(still_reconciling["request_id"], request_id);
    assert!(still_reconciling.get("required_target_count").is_none());
    assert!(still_reconciling.get("resolved_target_count").is_none());
    assert!(still_reconciling.get("legacy_unresolved_count").is_none());

    let response = h
        .router
        .clone()
        .oneshot(authority_request(json!({
            "reconciliationId": reconciliation_id,
            "authorityState": "ready",
            "expectedPredecessorGeneration": reconciling["authority"]
                ["authorityGeneration"],
            "expectedPredecessorAuthorityId": reconciling["authority"]["authorityId"],
            "expectedPredecessorAuthoritySha256": reconciling["authority"]
                ["authoritySha256"],
            "rootCount": 0,
            "rootSetSha256": jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256,
            "scopeRef": scope_ref,
            "evidenceRef": "phase-602-http-account-delete-inventory-ready",
            "evidenceSha256": hex::encode(Sha256::digest(
                b"phase-602-http-account-delete-inventory-ready"
            ))
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let ready: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(ready["disposition"], "applied");
    assert_eq!(ready["authority"]["authorityState"], "ready");

    let response = h.router.clone().oneshot(delete_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["state"], "pending_runner_volume_purge");
    assert_eq!(pending["request_id"], request_id);
    assert!(pending.get("required_target_count").is_some());
    assert!(pending.get("resolved_target_count").is_some());
    assert!(pending["legacy_unresolved_count"].as_i64().unwrap() >= 1);

    let response = h
        .router
        .clone()
        .oneshot(authority_request(json!({
            "reconciliationId": "phase-602-http-account-delete-successor",
            "authorityState": "reconciling",
            "expectedPredecessorGeneration": ready["authority"]["authorityGeneration"],
            "expectedPredecessorAuthorityId": ready["authority"]["authorityId"],
            "expectedPredecessorAuthoritySha256": ready["authority"]["authoritySha256"],
            "rootCount": 0,
            "rootSetSha256": jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256,
            "scopeRef": scope_ref,
            "evidenceRef": "phase-602-http-account-delete-successor-reconciling",
            "evidenceSha256": hex::encode(Sha256::digest(
                b"phase-602-http-account-delete-successor-reconciling"
            ))
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let successor_reconciling: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        successor_reconciling["authority"]["authorityState"],
        "reconciling"
    );

    let response = h
        .router
        .clone()
        .oneshot(authority_request(json!({
            "reconciliationId": "phase-602-http-account-delete-successor",
            "authorityState": "ready",
            "expectedPredecessorGeneration": successor_reconciling["authority"]
                ["authorityGeneration"],
            "expectedPredecessorAuthorityId": successor_reconciling["authority"]["authorityId"],
            "expectedPredecessorAuthoritySha256": successor_reconciling["authority"]
                ["authoritySha256"],
            "rootCount": 0,
            "rootSetSha256": jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256,
            "scopeRef": scope_ref,
            "evidenceRef": "phase-602-http-account-delete-successor-ready",
            "evidenceSha256": hex::encode(Sha256::digest(
                b"phase-602-http-account-delete-successor-ready"
            ))
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = h.router.clone().oneshot(delete_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let successor_pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(successor_pending["state"], "pending_runner_volume_purge");
    assert_eq!(successor_pending["request_id"], request_id);
    assert!(
        successor_pending["required_target_count"].as_i64().unwrap()
            >= pending["required_target_count"].as_i64().unwrap()
    );
    assert!(
        successor_pending["legacy_unresolved_count"]
            .as_i64()
            .unwrap()
            >= pending["legacy_unresolved_count"].as_i64().unwrap()
    );
    let connection = h.pool.get().unwrap();
    let attempt_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM jobs_runner_purge_requests \
              WHERE account_id = ?1 AND deletion_request_id = ?2",
            rusqlite::params![account.id, request_id],
            |row| row.get(0),
        )
        .unwrap();
    let superseded_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM jobs_runner_purge_requests \
              WHERE account_id = ?1 AND deletion_request_id = ?2 AND state = 'superseded'",
            rusqlite::params![account.id, request_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempt_count, 2);
    assert_eq!(superseded_count, 1);
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

#[derive(Clone)]
struct JobsPortabilityObject {
    logical_id: String,
    artifact_class: &'static str,
    title: &'static str,
    storage_key: String,
    media_type: &'static str,
    bytes: Vec<u8>,
}

struct JobsPortabilityFixture {
    account_id: String,
    access_token: String,
    application_id: String,
    objects: Vec<JobsPortabilityObject>,
}

fn account_object_list_xml(prefix: &str, keys: &[&str]) -> String {
    let contents = keys
        .iter()
        .map(|key| format!("<Contents><Key>{key}</Key></Contents>"))
        .collect::<String>();
    format!(
        "<ListBucketResult><Prefix>{prefix}</Prefix><KeyCount>{}</KeyCount>\
         <IsTruncated>false</IsTruncated>{contents}</ListBucketResult>",
        keys.len()
    )
}

fn account_delete_storage_config(endpoint_url: String) -> ObjectStorageConfig {
    ObjectStorageConfig {
        endpoint_url,
        bucket: "bucket".to_string(),
        access_key_id: "ak".to_string(),
        secret_access_key: "secret".to_string(),
        region: "auto".to_string(),
        key_prefix: "bluey-cloud".to_string(),
        retention_days: 365,
        max_object_bytes: 1024 * 1024,
    }
}

fn configure_runner_volume_purge_test_policy() {
    std::env::set_var(
        "BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID",
        "round602-test-key",
    );
    std::env::set_var(
        "BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
    );
    std::env::set_var(
        "BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON",
        r#"[{"keyId":"round602-test-key","publicKeyBase64url":"A6EHv_POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg"}]"#,
    );
    std::env::set_var("BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID", "runner-602.0");
}

async fn boot_account_delete_storage_harness(object_store: &MockServer) -> Harness {
    configure_runner_volume_purge_test_policy();
    let storage_config = account_delete_storage_config(object_store.uri());
    let harness = boot_harness_with_config(UpstreamKeys::default(), vec![], None, move |config| {
        config.object_storage = Some(storage_config);
    })
    .await;
    authorize_empty_legacy_runner_inventory(
        &harness.pool,
        "phase-602-account-delete-storage-inventory",
    );
    authorize_empty_workflow_cleanup_inventory(&harness.pool).await;
    harness
}

async fn mount_empty_account_namespace_sweep(object_store: &MockServer, account_id: &str) {
    let account_prefix = format!("bluey-cloud/accounts/{account_id}/");
    Mock::given(method("GET"))
        .and(path("/bucket"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", account_prefix.clone()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(account_object_list_xml(&account_prefix, &[])),
        )
        .expect(1)
        .mount(object_store)
        .await;
}

async fn boot_jobs_portability_harness(object_store: &MockServer) -> Harness {
    configure_runner_volume_purge_test_policy();
    let endpoint_url = object_store.uri();
    let harness = boot_harness_with_config(UpstreamKeys::default(), vec![], None, move |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url: endpoint_url.clone(),
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
        config.log_storage = Some(ObjectStorageConfig {
            endpoint_url,
            bucket: "logs".to_string(),
            access_key_id: "log-ak".to_string(),
            secret_access_key: "log-secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-logs".to_string(),
            retention_days: 180,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await;
    authorize_empty_legacy_runner_inventory(&harness.pool, "phase-602-jobs-portability-inventory");
    authorize_empty_workflow_cleanup_inventory(&harness.pool).await;
    harness
}

async fn seed_jobs_portability_fixture(harness: &Harness) -> JobsPortabilityFixture {
    let evidence = setup_application_evidence_download(harness).await;
    let auth = login(
        harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let access_token = auth["access_token"].as_str().unwrap().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let resume_source = jobs::get_resume_source_asset(&harness.pool, &evidence.account_id)
        .unwrap()
        .unwrap();
    let resume_source_id = resume_source.id;
    let resume_source_bytes = EXECUTION_LEASE_SOURCE_RESUME_BYTES.to_vec();
    let resume_source_sha256 = hex::encode(Sha256::digest(&resume_source_bytes));
    assert_eq!(resume_source.sha256, resume_source_sha256);
    let resume_source_key = resume_source.storage_key;
    let browser_profile_id = "portability-browser-profile";
    let browser_profile_bytes = b"BLUEYJP2 encrypted browser profile export bytes".to_vec();
    let browser_profile_sha256 = hex::encode(Sha256::digest(&browser_profile_bytes));
    let browser_profile_key = format!(
        "bluey-cloud/accounts/{}/jobs/browser-profiles/{browser_profile_id}/generation/1/sha256/{browser_profile_sha256}.enc",
        evidence.account_id
    );

    let connection = harness.pool.get().unwrap();
    connection
        .execute(
            "INSERT INTO jobs_browser_profile_snapshots (
                account_id, browser_profile_id, generation, object_key, sha256, size_bytes,
                envelope_version, writer_run_id, writer_fence, updated_at_ms
             ) VALUES (?1, ?2, 1, ?3, ?4, ?5, 2, 'portability-run', 1, ?6)",
            rusqlite::params![
                &evidence.account_id,
                browser_profile_id,
                &browser_profile_key,
                &browser_profile_sha256,
                browser_profile_bytes.len() as i64,
                now_ms,
            ],
        )
        .unwrap();

    let objects = vec![
        JobsPortabilityObject {
            logical_id: format!("jobs-submission-bundle:{}", evidence.receipt.sha256),
            artifact_class: "jobs_submission_evidence",
            title: "Application receipt bundle",
            storage_key: evidence.receipt.storage_key.clone(),
            media_type: "application/json",
            bytes: evidence.receipt_bytes,
        },
        JobsPortabilityObject {
            logical_id: format!("jobs-submission-resume:{}", evidence.resume.sha256),
            artifact_class: "jobs_submission_evidence",
            title: "Resume submitted",
            storage_key: evidence.resume.storage_key,
            media_type: "application/pdf",
            bytes: evidence.resume_bytes,
        },
        JobsPortabilityObject {
            logical_id: format!(
                "jobs-submission-confirmation:{}",
                evidence.confirmation.sha256
            ),
            artifact_class: "jobs_submission_evidence",
            title: "Application confirmation screenshot",
            storage_key: evidence.confirmation.storage_key,
            media_type: "image/png",
            bytes: evidence.confirmation_bytes,
        },
        JobsPortabilityObject {
            logical_id: format!("jobs-resume-source:{resume_source_id}"),
            artifact_class: "jobs_resume_source",
            title: "Source resume",
            storage_key: resume_source_key,
            media_type: "text/plain",
            bytes: resume_source_bytes,
        },
        JobsPortabilityObject {
            logical_id: format!(
                "jobs-browser-profile:{browser_profile_id}:1:2:{browser_profile_sha256}"
            ),
            artifact_class: "jobs_browser_profile_snapshot",
            title: "Encrypted Bluey Browser profile snapshot",
            storage_key: browser_profile_key,
            media_type: "application/vnd.bluey.browser-profile+encrypted",
            bytes: browser_profile_bytes,
        },
    ];

    for (index, object) in objects.iter().enumerate() {
        let upload_id = format!("portability-upload-{index}");
        let object_sha256 = hex::encode(Sha256::digest(&object.bytes));
        let object_created_at_ms = now_ms + index as i64;
        connection
            .execute(
                "INSERT INTO object_uploads (
                    id, account_id, object_kind, logical_id, session_id, storage_scope,
                    object_key, size_bytes, sha256, content_type, expires_at_ms, state,
                    metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
                 ) VALUES (?1, ?2, 'artifact', ?3, NULL, 'artifact', ?4, ?5, ?6, ?7,
                           ?8, 'ready', ?9, ?10, ?10, ?10, NULL)",
                rusqlite::params![
                    &upload_id,
                    &evidence.account_id,
                    &object.logical_id,
                    &object.storage_key,
                    object.bytes.len() as i64,
                    object_sha256,
                    object.media_type,
                    i64::MAX,
                    serde_json::to_string(&json!({
                        "artifact_class": object.artifact_class,
                        "title": object.title,
                        "retention_policy": "account_lifetime_until_deletion"
                    }))
                    .unwrap(),
                    object_created_at_ms,
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO object_storage_outbox (
                    id, upload_id, account_id, operation, state, attempt_count,
                    next_attempt_at_ms, last_error, created_at_ms, updated_at_ms, completed_at_ms
                 ) VALUES (?1, ?2, ?3, 'put', 'completed', 1, ?4, NULL, ?4, ?4, ?4)",
                rusqlite::params![
                    format!("portability-put-{index}"),
                    &upload_id,
                    &evidence.account_id,
                    object_created_at_ms,
                ],
            )
            .unwrap();
    }
    drop(connection);

    assert_eq!(
        bluey_server::db::account_data::artifact_object_refs(&harness.pool, &evidence.account_id,)
            .unwrap()
            .len(),
        objects.len()
    );
    JobsPortabilityFixture {
        account_id: evidence.account_id,
        access_token,
        application_id: evidence.application_id,
        objects,
    }
}

fn acknowledge_jobs_portability_cloud_cleanup(harness: &Harness, account_id: &str) {
    let mut connection = harness.pool.get().unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute(
            "DELETE FROM jobs_execution_leases WHERE account_id = ?1",
            rusqlite::params![account_id],
        )
        .unwrap();
    transaction
        .execute(
            "DELETE FROM jobs_browser_sessions
              WHERE account_id = ?1 AND runner = 'cloud'",
            rusqlite::params![account_id],
        )
        .unwrap();
    transaction.commit().unwrap();
}

fn resolve_pending_jobs_runner_legacy(
    harness: &Harness,
    pending: &serde_json::Value,
) -> jobs::RunnerPurgeRequestStatus {
    let request_id = pending["request_id"].as_str().unwrap();
    let legacy_unresolved_count = pending["legacy_unresolved_count"].as_i64().unwrap();
    assert!(legacy_unresolved_count > 0);
    let (_, status) = jobs::resolve_runner_volume_purge_legacy(
        &harness.pool,
        &jobs::ResolveRunnerPurgeLegacyRequest {
            request_id: request_id.to_string(),
            expected_legacy_unresolved_count: legacy_unresolved_count,
            resolution_ref: "phase-602-portability-legacy-resolution".to_string(),
            resolution_sha256: hex::encode(Sha256::digest(
                b"phase-602-portability-legacy-resolution",
            )),
            resolved_by: "phase-602-integration-admin".to_string(),
            resolved_at_ms: chrono::Utc::now().timestamp_millis(),
        },
    )
    .unwrap();
    assert_eq!(status.request_id, request_id);
    assert_eq!(status.legacy_unresolved_count, 0);
    status
}

#[tokio::test]
#[serial]
async fn account_export_zip_includes_verified_jobs_objects_and_fails_closed_on_tamper() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_portability_harness(&object_store).await;
    let fixture = seed_jobs_portability_fixture(&harness).await;
    let tampered_index = fixture.objects.len() - 1;

    for (index, object) in fixture.objects.iter().enumerate() {
        let object_path = format!("/bucket/{}", object.storage_key);
        if index == tampered_index {
            let valid_bytes = object.bytes.clone();
            let media_type = object.media_type.to_string();
            let calls = Arc::new(AtomicUsize::new(0));
            let responder_calls = Arc::clone(&calls);
            Mock::given(method("GET"))
                .and(path(object_path))
                .respond_with(move |_request: &wiremock::Request| {
                    match responder_calls.fetch_add(1, Ordering::SeqCst) {
                        0 => ResponseTemplate::new(200)
                            .set_body_raw(valid_bytes.clone(), &media_type),
                        1 => {
                            let mut body = valid_bytes.clone();
                            body[0] ^= 0xff;
                            ResponseTemplate::new(200).set_body_raw(body, &media_type)
                        }
                        2 => ResponseTemplate::new(200)
                            .set_body_raw(valid_bytes.clone(), "application/octet-stream"),
                        _ => ResponseTemplate::new(200).set_body_raw(
                            valid_bytes[..valid_bytes.len() - 1].to_vec(),
                            &media_type,
                        ),
                    }
                })
                .expect(4)
                .mount(&object_store)
                .await;
        } else {
            Mock::given(method("GET"))
                .and(path(object_path))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw(object.bytes.clone(), object.media_type),
                )
                .expect(4)
                .mount(&object_store)
                .await;
        }
    }

    let request = || {
        Request::get("/account/export?format=zip&include_objects=true")
            .header("authorization", format!("Bearer {}", fixture.access_token))
            .body(Body::empty())
            .unwrap()
    };
    let response = harness.router.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(body.to_vec())).unwrap();
    let mut account_export = String::new();
    archive
        .by_name("account-export.json")
        .unwrap()
        .read_to_string(&mut account_export)
        .unwrap();
    assert!(account_export.contains(&fixture.application_id));

    let mut manifest_text = String::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest_text)
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["include_objects_requested"], true);
    assert_eq!(
        manifest["objects"].as_array().unwrap().len(),
        fixture.objects.len()
    );
    for object in &fixture.objects {
        assert!(!manifest_text.contains(&object.storage_key));
        let exported = manifest["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["artifact_id"] == object.logical_id)
            .expect("every Jobs object must be represented in the export manifest");
        assert_eq!(exported["included"], true);
        assert_eq!(exported["content_type"], object.media_type);
        assert_eq!(exported["size_bytes"], object.bytes.len() as i64);
        assert_eq!(
            exported["sha256"],
            hex::encode(Sha256::digest(&object.bytes))
        );
        let zip_path = exported["zip_path"].as_str().unwrap();
        let mut exported_bytes = Vec::new();
        archive
            .by_name(zip_path)
            .unwrap()
            .read_to_end(&mut exported_bytes)
            .unwrap();
        assert_eq!(exported_bytes, object.bytes);
    }

    for _ in 0..3 {
        let tampered = harness.router.clone().oneshot(request()).await.unwrap();
        assert_eq!(tampered.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let tampered_body = axum::body::to_bytes(tampered.into_body(), 64 * 1024)
            .await
            .unwrap();
        let tampered_body = String::from_utf8_lossy(&tampered_body);
        for object in &fixture.objects {
            assert!(!tampered_body.contains(&object.storage_key));
        }
    }
}

#[tokio::test]
#[serial]
async fn account_export_zip_includes_legacy_jobs_evidence_without_size_or_media_authority() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_portability_harness(&object_store).await;
    let fixture = setup_application_evidence_download(&harness).await;
    let auth = login(
        &harness,
        "jobs-execution-lease@example.com",
        "valid-password-123",
    )
    .await;
    let access_token = auth["access_token"].as_str().unwrap();

    for mut evidence in
        jobs::list_application_evidence(&harness.pool, &fixture.account_id, None).unwrap()
    {
        evidence.media_type.clear();
        evidence
            .metadata
            .as_object_mut()
            .unwrap()
            .remove("size_bytes");
        persist_test_application_evidence(&harness, &fixture.account_id, &evidence);
    }

    let legacy_objects = [
        (
            fixture.resume.id.as_str(),
            fixture.resume.storage_key.as_str(),
            fixture.resume_bytes.as_slice(),
            "application/pdf",
        ),
        (
            fixture.receipt.id.as_str(),
            fixture.receipt.storage_key.as_str(),
            fixture.receipt_bytes.as_slice(),
            "application/json",
        ),
        (
            fixture.confirmation.id.as_str(),
            fixture.confirmation.storage_key.as_str(),
            fixture.confirmation_bytes.as_slice(),
            "image/png",
        ),
    ];
    let source_resume = jobs::get_resume_source_asset(&harness.pool, &fixture.account_id)
        .unwrap()
        .unwrap();
    for (_, object_key, bytes, media_type) in legacy_objects {
        Mock::given(method("GET"))
            .and(path(format!("/bucket/{object_key}")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), media_type))
            .expect(1)
            .mount(&object_store)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!("/bucket/{}", source_resume.storage_key)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EXECUTION_LEASE_SOURCE_RESUME_BYTES, "text/plain"),
        )
        .expect(1)
        .mount(&object_store)
        .await;

    let response = harness
        .router
        .clone()
        .oneshot(
            Request::get("/account/export?format=zip&include_objects=true")
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    if status != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        panic!(
            "legacy Jobs evidence export failed with {status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(body.to_vec())).unwrap();
    let mut manifest_text = String::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest_text)
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["objects"].as_array().unwrap().len(), 4);
    for (artifact_id, object_key, bytes, _) in legacy_objects {
        assert!(!manifest_text.contains(object_key));
        let exported = manifest["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["artifact_id"] == artifact_id)
            .expect("legacy Jobs evidence must be represented in the export manifest");
        assert!(exported["size_bytes"].is_null());
        assert!(exported["content_type"].is_null());
        assert_eq!(exported["sha256"], hex::encode(Sha256::digest(bytes)));
        let mut exported_bytes = Vec::new();
        archive
            .by_name(exported["zip_path"].as_str().unwrap())
            .unwrap()
            .read_to_end(&mut exported_bytes)
            .unwrap();
        assert_eq!(exported_bytes, bytes);
    }
    let exported_source = manifest["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["artifact_id"] == source_resume.id)
        .expect("the Track-bound source resume must be represented in the export manifest");
    assert_eq!(exported_source["size_bytes"], source_resume.size_bytes);
    assert_eq!(exported_source["content_type"], source_resume.media_type);
    assert_eq!(exported_source["sha256"], source_resume.sha256);
    let mut exported_source_bytes = Vec::new();
    archive
        .by_name(exported_source["zip_path"].as_str().unwrap())
        .unwrap()
        .read_to_end(&mut exported_source_bytes)
        .unwrap();
    assert_eq!(exported_source_bytes, EXECUTION_LEASE_SOURCE_RESUME_BYTES);
}

#[tokio::test]
#[serial]
async fn delete_account_removes_every_jobs_object_after_persisting_the_deletion_fence() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_portability_harness(&object_store).await;
    let fixture = seed_jobs_portability_fixture(&harness).await;
    acknowledge_jobs_portability_cloud_cleanup(&harness, &fixture.account_id);
    let fence_observed = Arc::new(AtomicUsize::new(0));
    let account_prefix = format!("bluey-cloud/accounts/{}/", fixture.account_id);
    let legacy_orphan_key = format!("{account_prefix}jobs/legacy/orphan-profile.enc");
    let log_prefix = format!("bluey-logs/accounts/{}/", fixture.account_id);
    let legacy_log_orphan_key = format!("{log_prefix}audit/legacy/orphan-log.json");

    for (index, object) in fixture.objects.iter().enumerate() {
        let mock =
            Mock::given(method("DELETE")).and(path(format!("/bucket/{}", object.storage_key)));
        if index == 0 {
            let pool = harness.pool.clone();
            let account_id = fixture.account_id.clone();
            let observed = Arc::clone(&fence_observed);
            mock.respond_with(move |_request: &wiremock::Request| {
                let connection = pool.get().unwrap();
                let (intent_count, upload_count): (i64, i64) = connection
                    .query_row(
                        "SELECT
                            (SELECT COUNT(*) FROM account_deletion_intents WHERE account_id = ?1),
                            (SELECT COUNT(*) FROM object_uploads WHERE account_id = ?1)",
                        rusqlite::params![&account_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap();
                if (intent_count, upload_count) == (1, 5) {
                    observed.store(1, Ordering::SeqCst);
                }
                ResponseTemplate::new(204)
            })
            .expect(1)
            .mount(&object_store)
            .await;
        } else {
            mock.respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&object_store)
                .await;
        }
    }

    let list_calls = Arc::new(AtomicUsize::new(0));
    let list_responder_calls = Arc::clone(&list_calls);
    let account_prefix_for_listing = account_prefix.clone();
    let orphan_key_for_listing = legacy_orphan_key.clone();
    Mock::given(method("GET"))
        .and(path("/bucket"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", account_prefix))
        .respond_with(move |_request: &wiremock::Request| {
            if list_responder_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_string(account_object_list_xml(
                    &account_prefix_for_listing,
                    &[&orphan_key_for_listing],
                ))
            } else {
                ResponseTemplate::new(200)
                    .set_body_string(account_object_list_xml(&account_prefix_for_listing, &[]))
            }
        })
        .expect(2)
        .mount(&object_store)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{legacy_orphan_key}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;

    let log_list_calls = Arc::new(AtomicUsize::new(0));
    let log_list_responder_calls = Arc::clone(&log_list_calls);
    let log_prefix_for_listing = log_prefix.clone();
    let log_orphan_key_for_listing = legacy_log_orphan_key.clone();
    Mock::given(method("GET"))
        .and(path("/logs"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", log_prefix))
        .respond_with(move |_request: &wiremock::Request| {
            if log_list_responder_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_string(account_object_list_xml(
                    &log_prefix_for_listing,
                    &[&log_orphan_key_for_listing],
                ))
            } else {
                ResponseTemplate::new(200)
                    .set_body_string(account_object_list_xml(&log_prefix_for_listing, &[]))
            }
        })
        .expect(2)
        .mount(&object_store)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/logs/{legacy_log_orphan_key}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&object_store)
        .await;

    fence_account_deletion_then_revalidate_workflow_cleanup(&harness, &fixture.access_token).await;
    let delete_request = || {
        Request::post("/account/delete")
            .header("authorization", format!("Bearer {}", fixture.access_token))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "confirm_text": "DELETE",
                    "accept_data_loss": true,
                    "accept_credit_loss": true
                }))
                .unwrap(),
            ))
            .unwrap()
    };
    let pending = harness
        .router
        .clone()
        .oneshot(delete_request())
        .await
        .unwrap();
    assert_eq!(pending.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(pending.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["deleted"], false);
    assert_eq!(pending["state"], "pending_runner_volume_purge");
    resolve_pending_jobs_runner_legacy(&harness, &pending);

    let response = harness
        .router
        .clone()
        .oneshot(delete_request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let ack: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(ack["deleted"], true);
    assert_eq!(ack["state"], "deleted");
    assert!(ack["deleted_at"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert_eq!(ack["object_count_deleted"], fixture.objects.len() + 2);
    assert_eq!(fence_observed.load(Ordering::SeqCst), 1);
    assert_eq!(list_calls.load(Ordering::SeqCst), 2);
    assert_eq!(log_list_calls.load(Ordering::SeqCst), 2);
    assert!(Account::fetch_by_id(&harness.pool, &fixture.account_id)
        .unwrap()
        .is_none());

    let connection = harness.pool.get().unwrap();
    let remaining: (i64, i64, i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM object_uploads WHERE account_id = ?1),
                (SELECT COUNT(*) FROM object_storage_outbox WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_application_evidence WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_resume_source_assets WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_browser_profile_snapshots WHERE account_id = ?1),
                (SELECT COUNT(*) FROM account_deletion_intents WHERE account_id = ?1)",
            rusqlite::params![&fixture.account_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(remaining, (0, 0, 0, 0, 0, 0));
}

#[tokio::test]
#[serial]
async fn delete_account_prefix_purge_failure_preserves_fence_and_database_rows() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_portability_harness(&object_store).await;
    let fixture = seed_jobs_portability_fixture(&harness).await;
    acknowledge_jobs_portability_cloud_cleanup(&harness, &fixture.account_id);

    for object in &fixture.objects {
        Mock::given(method("DELETE"))
            .and(path(format!("/bucket/{}", object.storage_key)))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&object_store)
            .await;
    }
    let account_prefix = format!("bluey-cloud/accounts/{}/", fixture.account_id);
    Mock::given(method("GET"))
        .and(path("/bucket"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", account_prefix.clone()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(account_object_list_xml(&account_prefix, &[])),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path("/logs"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param(
            "prefix",
            format!("bluey-logs/accounts/{}/", fixture.account_id),
        ))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&object_store)
        .await;

    fence_account_deletion_then_revalidate_workflow_cleanup(&harness, &fixture.access_token).await;
    let delete_request = || {
        Request::post("/account/delete")
            .header("authorization", format!("Bearer {}", fixture.access_token))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "confirm_text": "DELETE",
                    "accept_data_loss": true,
                    "accept_credit_loss": true
                }))
                .unwrap(),
            ))
            .unwrap()
    };
    let pending = harness
        .router
        .clone()
        .oneshot(delete_request())
        .await
        .unwrap();
    assert_eq!(pending.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(pending.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["deleted"], false);
    assert_eq!(pending["state"], "pending_runner_volume_purge");
    resolve_pending_jobs_runner_legacy(&harness, &pending);

    let response = harness
        .router
        .clone()
        .oneshot(delete_request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    for object in &fixture.objects {
        assert!(!body.contains(&object.storage_key));
    }
    assert!(Account::fetch_by_id(&harness.pool, &fixture.account_id)
        .unwrap()
        .is_some());

    let connection = harness.pool.get().unwrap();
    let preserved: (i64, i64, i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM object_uploads WHERE account_id = ?1),
                (SELECT COUNT(*) FROM object_storage_outbox WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_application_evidence WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_resume_source_assets WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_browser_profile_snapshots WHERE account_id = ?1),
                (SELECT COUNT(*) FROM account_deletion_intents WHERE account_id = ?1)",
            rusqlite::params![&fixture.account_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(preserved, (5, 5, 3, 1, 1, 1));
}

const JOBS_RESUME_SOURCE_OBJECT_PATH: &str =
    r"^/bucket/bluey-cloud/accounts/[^/]+/jobs/resumes/[^/]+/sha256/[0-9a-f]{64}\.txt$";

async fn boot_jobs_resume_storage_harness(object_store: &MockServer) -> Harness {
    let endpoint_url = object_store.uri();
    boot_harness_with_config(UpstreamKeys::default(), vec![], None, move |config| {
        config.object_storage = Some(ObjectStorageConfig {
            endpoint_url,
            bucket: "bucket".to_string(),
            access_key_id: "ak".to_string(),
            secret_access_key: "secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 365,
            max_object_bytes: 1024 * 1024,
        });
    })
    .await
}

fn jobs_resume_source_upload_request(
    access_token: &str,
    request_id: &str,
    file_name: &str,
    bytes: &[u8],
) -> Request<Body> {
    Request::post("/api/jobs/resume-source")
        .header("authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": request_id,
                "file_name": file_name,
                "media_type": "text/plain",
                "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes)
            }))
            .unwrap(),
        ))
        .unwrap()
}

fn jobs_resume_source_upload_request_with_profile(
    access_token: &str,
    request_id: &str,
    file_name: &str,
    bytes: &[u8],
    profile: &jobs::CareerProfile,
) -> Request<Body> {
    Request::post("/api/jobs/resume-source")
        .header("authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": request_id,
                "file_name": file_name,
                "media_type": "text/plain",
                "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
                "profile": profile,
            }))
            .unwrap(),
        ))
        .unwrap()
}

async fn mount_jobs_resume_source_round_trip(
    object_store: &MockServer,
    expected_puts: u64,
    expected_gets: u64,
) -> Arc<Mutex<Vec<u8>>> {
    let stored_bytes = Arc::new(Mutex::new(Vec::new()));
    let put_bytes = Arc::clone(&stored_bytes);
    Mock::given(method("PUT"))
        .and(path_regex(JOBS_RESUME_SOURCE_OBJECT_PATH))
        .respond_with(move |request: &wiremock::Request| {
            *put_bytes.lock().unwrap() = request.body.clone();
            ResponseTemplate::new(200)
        })
        .expect(expected_puts)
        .mount(object_store)
        .await;
    let get_bytes = Arc::clone(&stored_bytes);
    Mock::given(method("GET"))
        .and(path_regex(JOBS_RESUME_SOURCE_OBJECT_PATH))
        .respond_with(move |_request: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_raw(get_bytes.lock().unwrap().clone(), "text/plain")
        })
        .expect(expected_gets)
        .mount(object_store)
        .await;
    stored_bytes
}

#[tokio::test]
#[serial]
async fn jobs_resume_source_upload_readback_and_replacement_use_the_durable_ledger() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_resume_storage_harness(&object_store).await;
    let email = "jobs-resume-source-ledger@example.com";
    let access_token = signup_and_login(&harness, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .expect("account should exist");
    let first_bytes = b"First exact source resume";
    let replacement_bytes = b"Replacement exact source resume";
    mount_jobs_resume_source_round_trip(&object_store, 2, 3).await;

    let first_response = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000001",
            "first-resume.txt",
            first_bytes,
        ))
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body = axum::body::to_bytes(first_response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let first_value: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
    let first_id = first_value["asset"]["id"].as_str().unwrap().to_string();
    let first_asset = jobs::get_resume_source_asset(&harness.pool, &account.id)
        .unwrap()
        .expect("first source resume should be installed");
    assert_eq!(first_asset.id, first_id);
    assert_eq!(first_asset.sha256, hex::encode(Sha256::digest(first_bytes)));
    assert_eq!(first_asset.size_bytes, first_bytes.len() as i64);

    let replay_response = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000001",
            "first-resume.txt",
            first_bytes,
        ))
        .await
        .unwrap();
    assert_eq!(replay_response.status(), StatusCode::OK);
    let replay_body = axum::body::to_bytes(replay_response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let replay_value: serde_json::Value = serde_json::from_slice(&replay_body).unwrap();
    assert_eq!(replay_value["asset"]["id"], first_id);

    let changed_replay = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000001",
            "changed-under-same-request.txt",
            b"different bytes",
        ))
        .await
        .unwrap();
    assert_eq!(changed_replay.status(), StatusCode::CONFLICT);

    let replacement_response = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000002",
            "replacement-resume.txt",
            replacement_bytes,
        ))
        .await
        .unwrap();
    assert_eq!(replacement_response.status(), StatusCode::OK);
    let replacement_body = axum::body::to_bytes(replacement_response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let replacement_value: serde_json::Value = serde_json::from_slice(&replacement_body).unwrap();
    let replacement_id = replacement_value["asset"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let replacement_asset = jobs::get_resume_source_asset(&harness.pool, &account.id)
        .unwrap()
        .expect("replacement source resume should be installed");
    assert_eq!(replacement_asset.id, replacement_id);
    assert_ne!(replacement_asset.id, first_asset.id);
    assert_ne!(replacement_asset.storage_key, first_asset.storage_key);
    assert_eq!(
        replacement_asset.sha256,
        hex::encode(Sha256::digest(replacement_bytes))
    );
    assert_eq!(
        replacement_value["profile"]["source_resume_asset_id"],
        replacement_id
    );
    assert_eq!(
        replacement_value["profile"]["source_resume_sha256"],
        replacement_asset.sha256
    );

    let conn = harness.pool.get().unwrap();
    let first_ledger: (
        String,
        String,
        String,
        i64,
        String,
        i64,
        String,
        String,
        Option<String>,
        String,
    ) = conn
        .query_row(
            "SELECT upload.logical_id, upload.object_key, upload.sha256,
                    upload.size_bytes, upload.content_type, upload.expires_at_ms,
                    upload.state, put_outbox.state, delete_outbox.state,
                    upload.metadata_json
               FROM object_uploads upload
               JOIN object_storage_outbox put_outbox
                 ON put_outbox.upload_id = upload.id AND put_outbox.operation = 'put'
               LEFT JOIN object_storage_outbox delete_outbox
                 ON delete_outbox.upload_id = upload.id
                AND delete_outbox.operation = 'delete'
              WHERE upload.account_id = ?1 AND upload.object_key = ?2",
            rusqlite::params![&account.id, &first_asset.storage_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(first_ledger.0, format!("jobs-resume-source:{first_id}"));
    assert_eq!(first_ledger.1, first_asset.storage_key);
    assert_eq!(first_ledger.2, first_asset.sha256);
    assert_eq!(first_ledger.3, first_bytes.len() as i64);
    assert_eq!(first_ledger.4, "text/plain");
    assert_eq!(first_ledger.5, i64::MAX);
    assert_eq!(first_ledger.6, "delete_pending");
    assert_eq!(first_ledger.7, "completed");
    assert_eq!(first_ledger.8.as_deref(), Some("pending"));
    let first_metadata: serde_json::Value = serde_json::from_str(&first_ledger.9).unwrap();
    assert_eq!(first_metadata["artifact_class"], "jobs_resume_source");
    assert_eq!(first_metadata["jobs_resume_source_asset_id"], first_id);
    assert_eq!(
        first_metadata["retention_policy"],
        "account_lifetime_until_deletion"
    );

    let replacement_ledger: (String, String, String, i64, String, Option<String>) = conn
        .query_row(
            "SELECT upload.logical_id, upload.sha256, upload.state,
                    upload.expires_at_ms, put_outbox.state, delete_outbox.state
               FROM object_uploads upload
               JOIN object_storage_outbox put_outbox
                 ON put_outbox.upload_id = upload.id AND put_outbox.operation = 'put'
               LEFT JOIN object_storage_outbox delete_outbox
                 ON delete_outbox.upload_id = upload.id
                AND delete_outbox.operation = 'delete'
              WHERE upload.account_id = ?1 AND upload.object_key = ?2",
            rusqlite::params![&account.id, &replacement_asset.storage_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        replacement_ledger.0,
        format!("jobs-resume-source:{replacement_id}")
    );
    assert_eq!(replacement_ledger.1, replacement_asset.sha256);
    assert_eq!(replacement_ledger.2, "ready");
    assert_eq!(replacement_ledger.3, i64::MAX);
    assert_eq!(replacement_ledger.4, "completed");
    assert_eq!(replacement_ledger.5, None);
    drop(conn);

    let requests = object_store.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        vec!["PUT", "GET", "GET", "PUT", "GET"]
    );
    assert_eq!(requests[0].body, first_bytes);
    assert_eq!(requests[3].body, replacement_bytes);
    assert!(requests.iter().all(|request| request.method != "DELETE"));
}

#[tokio::test]
#[serial]
async fn jobs_resume_source_explicit_profile_replay_preserves_newer_profile_edits() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_resume_storage_harness(&object_store).await;
    let email = "jobs-resume-source-profile-replay@example.com";
    let access_token = signup_and_login(&harness, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .expect("account should exist");
    let base_profile =
        jobs::save_profile(&harness.pool, &account.id, &jobs::default_profile(email)).unwrap();
    let mut requested_profile = base_profile;
    requested_profile.headline = "Profile captured with upload".to_string();
    let bytes = b"Explicit profile source resume";
    mount_jobs_resume_source_round_trip(&object_store, 1, 2).await;
    let request_id = "00000000-0000-4000-8000-000000000010";

    let first = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request_with_profile(
            &access_token,
            request_id,
            "profile-resume.txt",
            bytes,
            &requested_profile,
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_body = axum::body::to_bytes(first.into_body(), 64 * 1024)
        .await
        .unwrap();
    let first_value: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
    assert_eq!(
        first_value["profile"]["headline"],
        "Profile captured with upload"
    );

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let mut newer_profile = jobs::get_profile(&harness.pool, &account.id, email).unwrap();
    newer_profile.headline = "Newer cross-tab profile edit".to_string();
    let newer_profile = jobs::save_profile(&harness.pool, &account.id, &newer_profile).unwrap();

    let replay = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request_with_profile(
            &access_token,
            request_id,
            "profile-resume.txt",
            bytes,
            &requested_profile,
        ))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    let replay_body = axum::body::to_bytes(replay.into_body(), 64 * 1024)
        .await
        .unwrap();
    let replay_value: serde_json::Value = serde_json::from_slice(&replay_body).unwrap();
    assert_eq!(replay_value["profile"]["headline"], newer_profile.headline);
    assert_eq!(
        replay_value["profile"]["updated_at_ms"],
        newer_profile.updated_at_ms
    );
    assert_eq!(
        jobs::get_profile(&harness.pool, &account.id, email)
            .unwrap()
            .headline,
        newer_profile.headline
    );

    let mut changed_request = requested_profile;
    changed_request.headline = "Changed under the same request id".to_string();
    let conflict = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request_with_profile(
            &access_token,
            request_id,
            "profile-resume.txt",
            bytes,
            &changed_request,
        ))
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial]
async fn jobs_resume_source_upload_corrupt_readback_keeps_the_previous_database_reference() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_resume_storage_harness(&object_store).await;
    let email = "jobs-resume-source-corrupt-readback@example.com";
    let access_token = signup_and_login(&harness, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .expect("account should exist");
    mount_jobs_resume_source_round_trip(&object_store, 1, 1).await;

    let original_bytes = b"Original verified resume";
    let original_response = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000003",
            "original-resume.txt",
            original_bytes,
        ))
        .await
        .unwrap();
    assert_eq!(original_response.status(), StatusCode::OK);
    let original_asset = jobs::get_resume_source_asset(&harness.pool, &account.id)
        .unwrap()
        .expect("original source resume should be installed");

    object_store.reset().await;
    Mock::given(method("PUT"))
        .and(path_regex(JOBS_RESUME_SOURCE_OBJECT_PATH))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&object_store)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(JOBS_RESUME_SOURCE_OBJECT_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(b"object-store corruption".to_vec(), "text/plain"),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    let replacement_bytes = b"New resume that must not become authoritative";
    let replacement_sha256 = hex::encode(Sha256::digest(replacement_bytes));
    let rejected = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000004",
            "unverified-replacement.txt",
            replacement_bytes,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_GATEWAY);

    let current_asset = jobs::get_resume_source_asset(&harness.pool, &account.id)
        .unwrap()
        .expect("the prior verified source resume should remain installed");
    assert_eq!(current_asset.id, original_asset.id);
    assert_eq!(current_asset.storage_key, original_asset.storage_key);
    assert_eq!(current_asset.sha256, original_asset.sha256);
    let profile = jobs::get_profile(&harness.pool, &account.id, email).unwrap();
    assert_eq!(profile.source_resume_asset_id, original_asset.id);
    assert_eq!(profile.source_resume_sha256, original_asset.sha256);

    let rejected_ledger: (String, String, String, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT upload.state, put_outbox.state, upload.sha256,
                    COUNT(delete_outbox.id)
               FROM object_uploads upload
               JOIN object_storage_outbox put_outbox
                 ON put_outbox.upload_id = upload.id AND put_outbox.operation = 'put'
               LEFT JOIN object_storage_outbox delete_outbox
                 ON delete_outbox.upload_id = upload.id
                AND delete_outbox.operation = 'delete'
              WHERE upload.account_id = ?1 AND upload.sha256 = ?2
              GROUP BY upload.state, put_outbox.state, upload.sha256",
            rusqlite::params![&account.id, &replacement_sha256],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(rejected_ledger.0, "pending");
    assert_eq!(rejected_ledger.1, "retry");
    assert_eq!(rejected_ledger.2, replacement_sha256);
    assert_eq!(rejected_ledger.3, 0);
    let rejected_reference_count: i64 = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM jobs_resume_source_assets
              WHERE account_id = ?1 AND sha256 = ?2",
            rusqlite::params![&account.id, &replacement_sha256],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rejected_reference_count, 0);
    let requests = object_store.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method.as_str(), "PUT");
    assert_eq!(requests[0].body, replacement_bytes);
    assert_eq!(requests[1].method.as_str(), "GET");
    assert!(requests.iter().all(|request| request.method != "DELETE"));
}

#[tokio::test]
#[serial]
async fn jobs_template_docx_download_patches_verified_source_and_redacts_provider_failure() {
    use std::io::Write as _;

    const DOCX_MEDIA_TYPE: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    const SOURCE_BULLET: &str = "Built reliable APIs";
    const TAILORED_BULLET: &str = "Built reliable APIs for payment workflows";
    const OBJECT_PATH: &str =
        r"^/bucket/bluey-cloud/accounts/[^/]+/jobs/resumes/[^/]+/sha256/[0-9a-f]{64}\.docx$";

    let mut docx = zip::ZipWriter::new(Cursor::new(Vec::new()));
    docx.start_file(
        "[Content_Types].xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    docx.write_all(b"<Types/>").unwrap();
    docx.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    docx.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>{SOURCE_BULLET}</w:t></w:r></w:p></w:body>
</w:document>"#,
        )
        .as_bytes(),
    )
    .unwrap();
    let source_docx = docx.finish().unwrap().into_inner();

    let object_store = MockServer::start().await;
    let harness = boot_jobs_resume_storage_harness(&object_store).await;
    let email = "jobs-template-docx-readback@example.com";
    let access_token = signup_and_login(&harness, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .expect("account should exist");
    let stored_bytes = Arc::new(Mutex::new(Vec::new()));
    let put_bytes = Arc::clone(&stored_bytes);
    Mock::given(method("PUT"))
        .and(path_regex(OBJECT_PATH))
        .respond_with(move |request: &wiremock::Request| {
            *put_bytes.lock().unwrap() = request.body.clone();
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&object_store)
        .await;
    let get_bytes = Arc::clone(&stored_bytes);
    Mock::given(method("GET"))
        .and(path_regex(OBJECT_PATH))
        .respond_with(move |_request: &wiremock::Request| {
            ResponseTemplate::new(200)
                .set_body_raw(get_bytes.lock().unwrap().clone(), DOCX_MEDIA_TYPE)
        })
        .expect(1)
        .mount(&object_store)
        .await;

    let upload = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::post("/api/jobs/resume-source")
                .header("authorization", format!("Bearer {access_token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "request_id": "00000000-0000-4000-8000-000000000006",
                        "file_name": "candidate.docx",
                        "media_type": DOCX_MEDIA_TYPE,
                        "bytes_base64": base64::engine::general_purpose::STANDARD
                            .encode(&source_docx),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);
    let source = jobs::get_resume_source_asset(&harness.pool, &account.id)
        .unwrap()
        .expect("DOCX source should be installed");
    assert_eq!(source.file_type, "docx");
    assert_eq!(source.template_status, "exact_docx");
    assert_eq!(source.sha256, hex::encode(Sha256::digest(&source_docx)));
    assert_eq!(
        stored_bytes.lock().unwrap().as_slice(),
        source_docx.as_slice()
    );
    object_store.verify().await;

    let job_id = "template-docx-job";
    let resume_version_id = "template-docx-resume-v1";
    let now = chrono::Utc::now().timestamp_millis();
    let diff = json!({
        "experience_rewrites": [{
            "before": SOURCE_BULLET,
            "after": TAILORED_BULLET,
            "source_evidence_ids": ["employment:synthetic:highlight:0"]
        }]
    });
    let conn = harness.pool.get().unwrap();
    conn.execute(
        "INSERT INTO jobs_postings (
            id, account_id, canonical_key, posting_json, source, canonical_url,
            company, title, location, match_score, status, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, '{}', 'test', '', 'Synthetic', 'Engineer', '', 100,
                   'matched', ?4, ?4)",
        rusqlite::params![job_id, &account.id, job_id, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO jobs_resume_versions (
            id, account_id, job_id, version_no, mode, content_json, diff_json,
            claim_ids_json, checksum, created_at_ms
         ) VALUES (?1, ?2, ?3, 1, 'enhance', '{}', ?4, '[]', ?5, ?6)",
        rusqlite::params![
            resume_version_id,
            &account.id,
            job_id,
            serde_json::to_string(&diff).unwrap(),
            hex::encode(Sha256::digest(b"template-docx-resume-v1")),
            now,
        ],
    )
    .unwrap();
    drop(conn);

    object_store.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(OBJECT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_raw(source_docx.clone(), DOCX_MEDIA_TYPE))
        .expect(1)
        .mount(&object_store)
        .await;
    let route = format!("/api/jobs/resume-versions/{resume_version_id}/template-docx");
    let download = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&route)
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        download.headers().get("content-type").unwrap(),
        DOCX_MEDIA_TYPE
    );
    assert_eq!(
        download.headers().get("content-disposition").unwrap(),
        "attachment; filename=\"candidate-bluey-v1.docx\""
    );
    assert_eq!(
        download.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let downloaded = axum::body::to_bytes(download.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_ne!(downloaded.as_ref(), source_docx.as_slice());
    let mut archive = zip::ZipArchive::new(Cursor::new(downloaded.as_ref())).unwrap();
    let mut document = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut document)
        .unwrap();
    assert!(document.contains(TAILORED_BULLET));
    assert!(!document.contains(&format!(">{SOURCE_BULLET}<")));
    object_store.verify().await;

    object_store.reset().await;
    const PROVIDER_MARKER: &str = "synthetic-private-provider-diagnostic";
    Mock::given(method("GET"))
        .and(path_regex(OBJECT_PATH))
        .respond_with(ResponseTemplate::new(503).set_body_string(PROVIDER_MARKER))
        .expect(1)
        .mount(&object_store)
        .await;
    let unavailable = harness
        .jobs_router
        .clone()
        .oneshot(
            Request::get(&route)
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    let unavailable_body = axum::body::to_bytes(unavailable.into_body(), 16 * 1024)
        .await
        .unwrap();
    assert_eq!(
        unavailable_body.as_ref(),
        b"Bluey could not load the original resume right now."
    );
    assert!(!unavailable_body
        .windows(PROVIDER_MARKER.len())
        .any(|window| { window == PROVIDER_MARKER.as_bytes() }));
    object_store.verify().await;
}

#[tokio::test]
#[serial]
async fn jobs_resume_source_upload_is_fenced_before_object_mutation_during_account_deletion() {
    let object_store = MockServer::start().await;
    let harness = boot_jobs_resume_storage_harness(&object_store).await;
    let email = "jobs-resume-source-delete-fence@example.com";
    let access_token = signup_and_login(&harness, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&harness.pool, email)
        .unwrap()
        .expect("account should exist");
    let deletion = bluey_server::db::account_data::begin_account_deletion(
        &harness.pool,
        &account.id,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    assert!(matches!(
        deletion,
        Some(bluey_server::db::account_data::BeginAccountDeletionResult::Ready(_))
    ));

    let response = harness
        .jobs_router
        .clone()
        .oneshot(jobs_resume_source_upload_request(
            &access_token,
            "00000000-0000-4000-8000-000000000005",
            "fenced-resume.txt",
            b"These bytes must never reach object storage",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(
        body.contains("fenced new writes and launches"),
        "unexpected fenced resume-upload response: {body}"
    );
    assert!(object_store.received_requests().await.unwrap().is_empty());

    let (upload_count, source_reference_count): (i64, i64) = harness
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM object_uploads WHERE account_id = ?1),
                (SELECT COUNT(*) FROM jobs_resume_source_assets WHERE account_id = ?1)",
            rusqlite::params![&account.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((upload_count, source_reference_count), (0, 0));
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
    configure_runner_volume_purge_test_policy();
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
    authorize_empty_legacy_runner_inventory(&h.pool, "phase-602-durable-upload-delete-inventory");
    authorize_empty_workflow_cleanup_inventory(&h.pool).await;
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

    // The shared sync reader must still serve a durable object after Jobs
    // adopted the stricter immutable-evidence reader. Verify the real response
    // contract and ensure upstream failures never expose provider text.
    let object_path = format!("/bucket/{object_key}");
    let download = || {
        Request::get(format!("/sync/artifacts/{artifact_id}/object"))
            .header("authorization", format!("Bearer {access}"))
            .body(Body::empty())
            .unwrap()
    };
    Mock::given(method("GET"))
        .and(path(object_path.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), "text/plain"))
        .expect(1)
        .mount(&object_store)
        .await;
    let response = h.router.clone().oneshot(download()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert_eq!(response.headers()["cache-control"], "private, max-age=60");
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap()
            .as_ref(),
        bytes
    );

    object_store.verify().await;
    object_store.reset().await;
    Mock::given(method("GET"))
        .and(path(object_path))
        .respond_with(ResponseTemplate::new(503).set_body_string("synthetic private provider note"))
        .expect(1)
        .mount(&object_store)
        .await;
    let response = h.router.clone().oneshot(download()).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap()
            .as_ref(),
        b"sync failed"
    );
    object_store.verify().await;

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
    let account_prefix = format!("bluey-cloud/accounts/{}/", account.id);
    Mock::given(method("GET"))
        .and(path("/bucket"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", account_prefix.clone()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(account_object_list_xml(&account_prefix, &[])),
        )
        .expect(1)
        .mount(&object_store)
        .await;
    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
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
    configure_runner_volume_purge_test_policy();
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
    authorize_empty_legacy_runner_inventory(&h.pool, "phase-602-artifact-object-delete-inventory");
    authorize_empty_workflow_cleanup_inventory(&h.pool).await;
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
    let account_prefix = format!("bluey-cloud/accounts/{}/", account.id);
    Mock::given(method("GET"))
        .and(path("/bucket"))
        .and(query_param("list-type", "2"))
        .and(query_param("max-keys", "1000"))
        .and(query_param("prefix", account_prefix.clone()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(account_object_list_xml(&account_prefix, &[])),
        )
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

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
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
    let cascade_token_count: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT COUNT(*)
               FROM jobs_workflow_cleanup_hard_delete_cascade_tokens
              WHERE account_id = ?1",
            rusqlite::params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cascade_token_count, 0);
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
async fn account_delete_retains_fence_when_workflow_authority_drifts_after_object_sweep_starts() {
    let object_store = MockServer::start().await;
    let h = boot_account_delete_storage_harness(&object_store).await;
    let email = "delete-workflow-drift-after-sweep@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let object_key = format!(
        "bluey-cloud/accounts/{}/context/workflow-drift-object",
        account.id
    );
    let batch = json!({
        "sessions": [{
            "session_id": "workflow-drift-session",
            "title": "Workflow drift",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000
        }],
        "context_artifacts": [{
            "artifact_id": "workflow-drift-object",
            "session_id": "workflow-drift-session",
            "kind": "document",
            "title": "Delete before drift",
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
    let response = h
        .router
        .clone()
        .oneshot(
            Request::post("/sync/batch")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
    let drift_observed = Arc::new(AtomicUsize::new(0));
    let responder_drift = Arc::clone(&drift_observed);
    let drift_pool = h.pool.clone();
    let drift_authority = jobs::current_jobs_legacy_inventory_authority(&h.pool)
        .unwrap()
        .expect("post-sweep drift test requires current legacy authority");
    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(move |_request: &wiremock::Request| {
            jobs::request_jobs_legacy_inventory_revalidation(
                &drift_pool,
                &drift_authority,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
            let lease = jobs::claim_jobs_workflow_cleanup_work(
                &drift_pool,
                "post-sweep-drift-test-owner",
                chrono::Utc::now().timestamp_millis(),
                30_000,
            )
            .unwrap()
            .expect("late global revalidation must claim an inventory page");
            assert!(matches!(
                lease,
                jobs::JobsWorkflowCleanupWorkLease::LegacyInventoryPage(_)
            ));
            responder_drift.store(1, Ordering::SeqCst);
            ResponseTemplate::new(204)
        })
        .expect(1)
        .mount(&object_store)
        .await;

    let response = h
        .router
        .clone()
        .oneshot(
            Request::post("/account/delete")
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
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["state"], "pending_workflow_cleanup_revalidation");
    assert_eq!(pending["object_count_deleted"], 1);
    assert_eq!(drift_observed.load(Ordering::SeqCst), 1);
    assert!(Account::fetch_by_id(&h.pool, &account.id)
        .unwrap()
        .is_some());
    let intent = bluey_server::db::account_data::account_deletion_intent(&h.pool, &account.id)
        .unwrap()
        .expect("post-sweep drift must retain deletion fence");
    let cleanup = jobs::get_jobs_workflow_cleanup_deletion_status(
        &h.pool,
        &account.id,
        intent.requested_at_ms,
    )
    .unwrap()
    .expect("post-sweep drift must retain cleanup state");
    assert!(!cleanup.complete);
    assert!(cleanup.object_sweep_started_at_ms.is_some());
    assert_eq!(cleanup.object_sweep_deleted_count, 1);
}

#[tokio::test]
#[serial]
async fn account_delete_remembers_sweep_start_when_progress_write_fails_after_delete() {
    let object_store = MockServer::start().await;
    let h = boot_account_delete_storage_harness(&object_store).await;
    let email = "delete-workflow-progress-loss@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let object_key = format!(
        "bluey-cloud/accounts/{}/context/workflow-progress-loss-object",
        account.id
    );
    let batch = json!({
        "sessions": [{
            "session_id": "workflow-progress-loss-session",
            "title": "Workflow progress loss",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000
        }],
        "context_artifacts": [{
            "artifact_id": "workflow-progress-loss-object",
            "session_id": "workflow-progress-loss-session",
            "kind": "document",
            "title": "Delete before progress loss",
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
    let response = h
        .router
        .clone()
        .oneshot(
            Request::post("/sync/batch")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
    let delete_observed = Arc::new(AtomicUsize::new(0));
    let responder_observed = Arc::clone(&delete_observed);
    let drift_pool = h.pool.clone();
    let responder_account_id = account.id.clone();
    let drift_authority = jobs::current_jobs_legacy_inventory_authority(&h.pool)
        .unwrap()
        .expect("progress-loss drift test requires current legacy authority");
    Mock::given(method("DELETE"))
        .and(path(format!("/bucket/{object_key}")))
        .respond_with(move |_request: &wiremock::Request| {
            let intent = bluey_server::db::account_data::account_deletion_intent(
                &drift_pool,
                &responder_account_id,
            )
            .unwrap()
            .expect("external delete must observe the durable deletion fence");
            let cleanup = jobs::get_jobs_workflow_cleanup_deletion_status(
                &drift_pool,
                &responder_account_id,
                intent.requested_at_ms,
            )
            .unwrap()
            .expect("external delete must observe sweep authorization");
            assert!(cleanup.object_sweep_started_at_ms.is_some());
            jobs::request_jobs_legacy_inventory_revalidation(
                &drift_pool,
                &drift_authority,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
            let lease = jobs::claim_jobs_workflow_cleanup_work(
                &drift_pool,
                "progress-loss-drift-test-owner",
                chrono::Utc::now().timestamp_millis(),
                30_000,
            )
            .unwrap()
            .expect("late global revalidation must claim an inventory page");
            assert!(matches!(
                lease,
                jobs::JobsWorkflowCleanupWorkLease::LegacyInventoryPage(_)
            ));
            drift_pool
                .get()
                .unwrap()
                .execute_batch(
                    "CREATE TRIGGER fail_workflow_sweep_progress_after_delete
                     BEFORE INSERT ON jobs_workflow_cleanup_object_sweep_progress
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated progress response loss');
                     END;",
                )
                .unwrap();
            responder_observed.store(1, Ordering::SeqCst);
            ResponseTemplate::new(204)
        })
        .expect(1)
        .mount(&object_store)
        .await;

    let delete_request = || {
        Request::post("/account/delete")
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
            .unwrap()
    };
    let response = h.router.clone().oneshot(delete_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(delete_observed.load(Ordering::SeqCst), 1);

    let intent = bluey_server::db::account_data::account_deletion_intent(&h.pool, &account.id)
        .unwrap()
        .expect("progress loss must retain the deletion fence");
    let cleanup = jobs::get_jobs_workflow_cleanup_deletion_status(
        &h.pool,
        &account.id,
        intent.requested_at_ms,
    )
    .unwrap()
    .expect("progress loss must retain cleanup state");
    assert!(!cleanup.complete);
    assert!(cleanup.object_sweep_started_at_ms.is_some());
    assert_eq!(cleanup.object_sweep_deleted_count, 0);

    h.pool
        .get()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_workflow_sweep_progress_after_delete")
        .unwrap();
    let response = h.router.clone().oneshot(delete_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["state"], "pending_workflow_cleanup_revalidation");
    assert_eq!(pending["object_count_deleted"], 0);
    assert!(Account::fetch_by_id(&h.pool, &account.id)
        .unwrap()
        .is_some());
}

#[tokio::test]
#[serial]
async fn delete_account_establishes_a_durable_fence_before_waiting_for_an_active_put() {
    configure_runner_volume_purge_test_policy();
    let h = boot_harness().await;
    authorize_empty_legacy_runner_inventory(&h.pool, "phase-602-active-put-delete-inventory");
    authorize_empty_workflow_cleanup_inventory(&h.pool).await;
    let email = "delete-active-put@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, email)
        .unwrap()
        .expect("account should exist");
    let now_ms = chrono::Utc::now().timestamp_millis();
    h.pool
        .get()
        .unwrap()
        .execute(
            "INSERT INTO object_uploads (
                id, account_id, object_kind, logical_id, storage_scope,
                object_key, size_bytes, sha256, content_type, expires_at_ms,
                state, metadata_json, created_at_ms, updated_at_ms
             ) VALUES (
                'active-account-delete-put', ?1, 'artifact', 'active-delete-artifact',
                'artifact', ?2, 12, ?3, 'application/octet-stream', ?4,
                'pending', '{}', ?5, ?5
             )",
            rusqlite::params![
                &account.id,
                format!(
                    "bluey-cloud/accounts/{}/context/active-delete-artifact",
                    account.id
                ),
                "a".repeat(64),
                now_ms + 60_000,
                now_ms,
            ],
        )
        .unwrap();

    fence_account_deletion_then_revalidate_workflow_cleanup(&h, &access).await;
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
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(resp.headers().get("retry-after").unwrap(), "5");
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["deleted"], false);
    assert_eq!(pending["state"], "pending_upload_drain");
    assert_eq!(pending["retry_after_ms"], 5_000);
    assert!(Account::fetch_by_email(&h.pool, email).unwrap().is_some());
    let intent: (i64, i64) = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT fresh_in_flight_puts, requested_at_ms
               FROM account_deletion_intents WHERE account_id = ?1",
            rusqlite::params![&account.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(intent.0, 1);
    assert!(intent.1 >= now_ms);
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

#[tokio::test]
#[serial]
async fn company_memory_http_auth_and_legacy_requests_fail_closed_on_both_routers() {
    let harness = boot_harness().await;
    let access =
        signup_and_login(&harness, "memory-owner@example.test", "valid-password-123").await;
    let other = signup_and_login(&harness, "memory-other@example.test", "valid-password-123").await;
    for email in ["memory-owner@example.test", "memory-other@example.test"] {
        let account = Account::fetch_by_email(&harness.pool, email)
            .unwrap()
            .unwrap();
        assert_eq!(
            jobs_beta_access::evaluate_or_enroll_public_beta(&harness.pool, &account.id)
                .unwrap()
                .reason,
            PublicBetaAccessReason::Admitted,
        );
    }
    for (index, router) in [harness.router.clone(), harness.jobs_router.clone()]
        .into_iter()
        .enumerate()
    {
        let unauth = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/answer-company-scopes/missing-job")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
        let missing = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/answer-company-scopes/missing-job")
                    .header("authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let question = format!("Synthetic question {index}");
        let answer = json!({"question":question,"value":"synthetic answer","scope":"account"});
        let created = router
            .clone()
            .oneshot(
                Request::post("/api/jobs/answers")
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&answer).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let saved: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(created.into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        let id = saved["id"].as_str().unwrap();
        let updated = router
            .clone()
            .oneshot(
                Request::put(format!("/api/jobs/answers/{id}"))
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&saved).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(updated.status(), StatusCode::OK);
        let denied = router
            .clone()
            .oneshot(
                Request::put(format!("/api/jobs/answers/{id}"))
                    .header("authorization", format!("Bearer {other}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&saved).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::NOT_FOUND);
        let mut partial = saved.clone();
        partial["company_job_id"] = json!("missing-job");
        let mut reverse_partial = saved.clone();
        reverse_partial["expected_company_scope"] = json!("company-v1-example");
        let before = jobs::list_answer_memory(
            &harness.pool,
            &Account::fetch_by_email(&harness.pool, "memory-owner@example.test")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap();
        for body in [&partial, &reverse_partial] {
            for request in [
                Request::post("/api/jobs/answers"),
                Request::put(format!("/api/jobs/answers/{id}")),
            ] {
                let response = router
                    .clone()
                    .oneshot(
                        request
                            .header("authorization", format!("Bearer {access}"))
                            .header("content-type", "application/json")
                            .body(Body::from(serde_json::to_vec(body).unwrap()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            }
        }
        let after = jobs::list_answer_memory(
            &harness.pool,
            &Account::fetch_by_email(&harness.pool, "memory-owner@example.test")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap();
        assert!(serde_json::to_value(before).unwrap() == serde_json::to_value(after).unwrap());
    }
}

#[cfg(feature = "integration-test-support")]
#[tokio::test]
#[serial]
async fn company_memory_http_uses_signed_current_employer_on_both_routers() {
    let harness = boot_harness().await;
    let access = signup_and_login(
        &harness,
        "verified-memory-owner@example.test",
        "valid-password-123",
    )
    .await;
    let other = signup_and_login(
        &harness,
        "verified-memory-other@example.test",
        "valid-password-123",
    )
    .await;
    let owner = Account::fetch_by_email(&harness.pool, "verified-memory-owner@example.test")
        .unwrap()
        .unwrap();
    for email in [
        "verified-memory-owner@example.test",
        "verified-memory-other@example.test",
    ] {
        let account = Account::fetch_by_email(&harness.pool, email)
            .unwrap()
            .unwrap();
        assert_eq!(
            jobs_beta_access::evaluate_or_enroll_public_beta(&harness.pool, &account.id)
                .unwrap()
                .reason,
            PublicBetaAccessReason::Admitted,
        );
    }
    let profile = jobs::save_profile(
        &harness.pool,
        &owner.id,
        &jobs::default_profile(&owner.email),
    )
    .unwrap();
    let preferences =
        jobs::save_preferences(&harness.pool, &owner.id, &JobPreferences::default()).unwrap();
    let identity =
        jobs::ensure_primary_application_identity(&harness.pool, &owner.id, &owner.email).unwrap();
    let track = jobs::upsert_track(
        &harness.pool,
        &owner.id,
        &jobs::CareerTrack {
            id: "memory-http-track".into(),
            name: "Software engineering".into(),
            role: "Software Engineer".into(),
            locations: vec!["New York, NY".into()],
            remote_preference: "hybrid_ok".into(),
            application_identity_id: Some(identity.id),
            policy: jobs::CareerTrackPolicy {
                role_family: "software_engineering".into(),
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
    let mut posting = JobPosting {
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
        track_id: track.id,
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
    posting.canonical_key = jobs::canonical_job_key(&posting);
    let posting = jobs::install_integration_test_production_positive_job_authorities(
        jobs::IntegrationTestProductionPositiveJobAuthoritiesRequest {
            pool: &harness.pool,
            account_id: &owner.id,
            posting: &posting,
            profile: &profile,
            preferences: &preferences,
            canonical_employer_domain: "acme.com",
            suffix: "memory-http",
            runner_kind: "local",
        },
    );
    for (index, router) in [harness.router.clone(), harness.jobs_router.clone()]
        .into_iter()
        .enumerate()
    {
        let uri = format!("/api/jobs/answer-company-scopes/{}", posting.id);
        let lookup = router
            .clone()
            .oneshot(
                Request::get(&uri)
                    .header("authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(lookup.status(), StatusCode::OK);
        assert_eq!(
            lookup.headers().get("cache-control").unwrap(),
            "private, no-store"
        );
        let scope: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(lookup.into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(scope["job_id"] == posting.id && scope["domain"] == "acme.com");
        let scope_id = scope["scope_id"].as_str().unwrap();
        assert!(scope_id.starts_with("company-v1-") && scope_id.len() == 75);
        let cross = router
            .clone()
            .oneshot(
                Request::get(&uri)
                    .header("authorization", format!("Bearer {other}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let missing = router
            .clone()
            .oneshot(
                Request::get("/api/jobs/answer-company-scopes/missing-job")
                    .header("authorization", format!("Bearer {other}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross.status(), StatusCode::NOT_FOUND);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let cross_body = axum::body::to_bytes(cross.into_body(), 64 * 1024)
            .await
            .unwrap();
        let missing_body = axum::body::to_bytes(missing.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert!(cross_body == missing_body);
        let input = json!({"question":format!("Verified question {index}"),"value":"synthetic answer",
            "scope":"company","scope_id":scope_id,"company_job_id":posting.id,
            "expected_company_scope":scope_id});
        let created = router
            .clone()
            .oneshot(
                Request::post("/api/jobs/answers")
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&input).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let mut saved: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(created.into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(saved["scope_id"] == scope_id);
        let id = saved["id"].as_str().unwrap().to_owned();
        saved["company_job_id"] = json!(posting.id);
        saved["expected_company_scope"] = json!(scope_id);
        let updated = router
            .clone()
            .oneshot(
                Request::put(format!("/api/jobs/answers/{id}"))
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&saved).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(updated.status(), StatusCode::OK);
        let other_account =
            Account::fetch_by_email(&harness.pool, "verified-memory-other@example.test")
                .unwrap()
                .unwrap();
        let owner_before = jobs::list_answer_memory(&harness.pool, &owner.id).unwrap();
        let other_before = jobs::list_answer_memory(&harness.pool, &other_account.id).unwrap();
        let denied_post = router
            .clone()
            .oneshot(
                Request::post("/api/jobs/answers")
                    .header("authorization", format!("Bearer {other}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&input).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied_post.status(), StatusCode::NOT_FOUND);
        let denied_put = router
            .clone()
            .oneshot(
                Request::put(format!("/api/jobs/answers/{id}"))
                    .header("authorization", format!("Bearer {other}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&saved).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied_put.status(), StatusCode::NOT_FOUND);
        let owner_after = jobs::list_answer_memory(&harness.pool, &owner.id).unwrap();
        let other_after = jobs::list_answer_memory(&harness.pool, &other_account.id).unwrap();
        assert!(
            serde_json::to_value(owner_before).unwrap()
                == serde_json::to_value(owner_after).unwrap()
        );
        assert!(
            serde_json::to_value(other_before).unwrap()
                == serde_json::to_value(other_after).unwrap()
        );
        let before = jobs::list_answer_memory(&harness.pool, &owner.id).unwrap();
        saved["expected_company_scope"] = json!("company-v1-wrong");
        let conflict = router
            .clone()
            .oneshot(
                Request::put(format!("/api/jobs/answers/{id}"))
                    .header("authorization", format!("Bearer {access}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&saved).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let after = jobs::list_answer_memory(&harness.pool, &owner.id).unwrap();
        assert!(serde_json::to_value(before).unwrap() == serde_json::to_value(after).unwrap());
    }
}

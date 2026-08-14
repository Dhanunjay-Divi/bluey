#![cfg(test)]

use std::ffi::OsString;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use base64::Engine;
use bluey_server::{
    api::build_jobs_router,
    auth::jwt::{self, TokenKind},
    config::{Config, ServerDbBackend, TrialAbuseConfig, UpstreamKeys},
    db::{
        accounts::Account,
        jobs::{self, JobApplication, JobDiscoveryEvidence, JobPosting, JobPreferences},
        open_pool, run_migrations, DbPool,
    },
};
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{json, Value};
use serial_test::serial;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use wiremock::MockServer;

const TEST_SECRET: &str = "jobs-runner-plan-matrix-secret-32-bytes";
const JOBS_DATA_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_BROWSER_SERVER_RELEASE_ID: &str = "server-603.1";
const TEST_BROWSER_RELEASE_KEY_INDEX: usize = 6;

struct BrowserBuildProofFixture {
    descriptor: String,
    signature: String,
    descriptor_sha256: String,
}

fn browser_release_authority_fixture() -> Value {
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
        let seed = std::array::from_fn(|offset| {
            ((TEST_BROWSER_RELEASE_KEY_INDEX * 37 + offset) % 256) as u8
        });
        let signing_key = SigningKey::from_bytes(&seed);
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

fn browser_release_root_trust_anchor_json() -> String {
    let fixture = browser_release_authority_fixture();
    let encoded_policy = fixture
        .pointer("/trustPolicy/canonical")
        .and_then(Value::as_str)
        .expect("canonical Browser trust policy fixture");
    let policy_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded_policy)
        .expect("decode canonical Browser trust policy fixture");
    let policy: Value =
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

struct TestContext {
    router: Router,
    pool: DbPool,
    db_path: std::path::PathBuf,
}

struct TestAccount {
    account: Account,
    token: String,
}

struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

fn authorize_empty_legacy_runner_inventory(
    pool: &DbPool,
    reconciliation_id: &str,
) -> jobs::RunnerVolumeFleetStatus {
    let fleet = jobs::runner_volume_fleet_status(pool).expect("load matrix runner fleet");
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
            scope_ref: "phase-603-plan-matrix-empty-legacy-roots".to_string(),
            evidence_ref: "phase-603-plan-matrix-inventory-reconciling".to_string(),
            evidence_sha256: hex::encode(Sha256::digest(
                b"phase-603-plan-matrix-inventory-reconciling",
            )),
            authorized_by: "phase-603-plan-matrix-admin".to_string(),
            recorded_at_ms: now_ms,
        },
    )
    .expect("record reconciling matrix legacy inventory authority")
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
            evidence_ref: "phase-603-plan-matrix-inventory-ready".to_string(),
            evidence_sha256: hex::encode(Sha256::digest(b"phase-603-plan-matrix-inventory-ready")),
            authorized_by: "phase-603-plan-matrix-admin".to_string(),
            recorded_at_ms: now_ms + 1,
        },
    )
    .expect("record ready matrix legacy inventory authority");
    let ready = jobs::runner_volume_fleet_status(pool).expect("reload matrix runner fleet");
    assert_eq!(ready.legacy_inventory_state, "ready");
    ready
}

fn enable_local_browser_distribution_for_test(pool: &DbPool) {
    let fleet = authorize_empty_legacy_runner_inventory(pool, "phase-603-plan-matrix");
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
            .expect("matrix legacy inventory reconciliation id"),
        expected_legacy_inventory_authority_id: fleet
            .legacy_inventory_authority_id
            .expect("matrix legacy inventory authority id"),
        expected_legacy_inventory_authority_sha256: fleet
            .legacy_inventory_authority_sha256
            .expect("matrix legacy inventory authority digest"),
        expected_legacy_inventory_root_count: fleet
            .legacy_inventory_root_count
            .expect("matrix legacy inventory root count"),
        expected_legacy_inventory_root_set_sha256: fleet
            .legacy_inventory_root_set_sha256
            .expect("matrix legacy inventory root-set digest"),
        expected_non_destroyed_volume_count: fleet.non_destroyed_volume_count,
        expected_destruction_count: fleet.destruction_count,
        expected_unresolved_legacy_volume_count: fleet.unresolved_legacy_volume_count,
        evidence_ref: "phase-603-plan-matrix-cutover".to_string(),
        evidence_sha256: hex::encode(Sha256::digest(b"phase-603-plan-matrix-cutover")),
        authorized_by: "phase-603-plan-matrix-admin".to_string(),
        cutover_at_ms: now_ms,
        now_ms,
    };
    jobs::record_runner_volume_fleet_cutover(pool, &cutover)
        .expect("record reconciling matrix runner fleet cutover");
    cutover.cutover_state = "ready".to_string();
    cutover.now_ms += 1;
    jobs::record_runner_volume_fleet_cutover(pool, &cutover)
        .expect("record ready matrix runner fleet cutover");
    let ready = jobs::runner_volume_fleet_status(pool).expect("load ready matrix runner fleet");
    assert_eq!(ready.cutover_state, "ready");
    assert_eq!(ready.legacy_inventory_state, "ready");
    assert_eq!(
        ready.attested_reconciled_volume_count,
        ready.non_destroyed_volume_count
    );
}

fn seed_canonical_browser_release_registry(pool: &DbPool) {
    let fixture = browser_release_authority_fixture();
    let envelope = |authority: &str| jobs::BrowserReleaseAuthorityEnvelope {
        canonical_base64url: fixture[authority]["canonical"]
            .as_str()
            .expect("canonical Browser release authority")
            .to_string(),
        signature_set_base64url: fixture[authority]["signatureSet"]
            .as_str()
            .expect("Browser release authority signature set")
            .to_string(),
    };
    jobs::import_browser_release_trust_policy(
        pool,
        &envelope("trustPolicy"),
        "phase-603-plan-matrix-admin",
    )
    .expect("import matrix Browser trust policy");

    let manifest_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(
            fixture["manifest"]["canonical"]
                .as_str()
                .expect("canonical Browser manifest"),
        )
        .expect("decode canonical Browser manifest");
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).expect("parse canonical Browser manifest");
    let build_proofs = [("darwin", "arm64"), ("darwin", "x64"), ("windows", "x64")]
        .into_iter()
        .map(|(platform, architecture)| {
            let proof = browser_build_proof_fixture(platform, architecture);
            let expected_sha256 = manifest["artifacts"]
                .as_array()
                .expect("Browser manifest artifacts")
                .iter()
                .find(|artifact| {
                    artifact["platform"] == platform && artifact["architecture"] == architecture
                })
                .and_then(|artifact| artifact["buildDescriptorSha256"].as_str())
                .expect("target Browser manifest descriptor");
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
                .expect("canonical Browser manifest")
                .to_string(),
            signature_set_base64url: fixture["manifest"]["signatureSet"]
                .as_str()
                .expect("Browser manifest signature set")
                .to_string(),
            build_proofs,
        },
        "phase-603-plan-matrix-admin",
    )
    .expect("import matrix Browser release manifest");
    jobs::import_browser_release_activation(
        pool,
        &envelope("activation"),
        "phase-603-plan-matrix-admin",
    )
    .expect("import matrix Browser release activation");
    let status = jobs::apply_browser_release_activation(
        pool,
        &jobs::ApplyBrowserReleaseActivationRequest {
            activation_sha256: fixture["activation"]["sha256"]
                .as_str()
                .expect("Browser activation digest")
                .to_string(),
            expected_head_revision: 0,
            expected_transition_sha256: None,
        },
        "phase-603-plan-matrix-admin",
    )
    .expect("apply matrix Browser release activation");
    assert!(status.available);
    assert_eq!(status.release_id.as_deref(), Some("browser-release-603-1"));
}

fn assign_browser_release_channel(pool: &DbPool, account_id: &str) {
    jobs::assign_browser_release_account_channel(
        pool,
        account_id,
        &jobs::AssignBrowserReleaseChannelRequest {
            assignment_generation: 1,
            predecessor_assignment_sha256: None,
            channel: "beta".to_string(),
            reason_ref: "phase-603-plan-matrix".to_string(),
            assigned_at_ms: chrono::Utc::now().timestamp_millis(),
        },
        "phase-603-plan-matrix-admin",
    )
    .expect("assign matrix Browser release channel");
}

impl TestContext {
    fn boot() -> Self {
        let db_path = std::env::temp_dir().join(format!(
            "bluey-jobs-runner-plan-matrix-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&db_path).expect("open matrix database");
        run_migrations(&pool).expect("run matrix migrations");
        let config = Config {
            port: 0,
            db_path: db_path.clone(),
            db_backend: ServerDbBackend::Sqlite,
            database_url: None,
            jwt_secret: TEST_SECRET.to_string(),
            public_url: "http://localhost:8080".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            smtp: None,
            upstream: UpstreamKeys::default(),
            upstream_spend_guard: None,
            admin_emails: Vec::new(),
            trial_abuse: TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: None,
            log_storage: None,
        };
        let router = build_jobs_router(pool.clone(), config);
        Self {
            router,
            pool,
            db_path,
        }
    }

    fn account(&self, label: &str, plan: &str) -> TestAccount {
        let email = format!("jobs-runner-{label}-{}@example.com", uuid::Uuid::new_v4());
        let account = Account::create(&self.pool, &email, "unused-test-password-hash")
            .expect("create matrix account");
        jobs::set_entitlement_plan(&self.pool, &account.id, plan).expect("set matrix entitlement");
        let token = jwt::issue(TEST_SECRET, &account.id, TokenKind::Access)
            .expect("issue matrix access token");
        TestAccount { account, token }
    }

    fn prepare(&self, account: &Account, label: &str) -> JobApplication {
        let profile = jobs::default_profile(&account.email);
        jobs::save_profile(&self.pool, &account.id, &profile).expect("save matrix profile");
        let identity =
            jobs::ensure_primary_application_identity(&self.pool, &account.id, &account.email)
                .expect("create matrix application identity");
        let track = jobs::upsert_track(
            &self.pool,
            &account.id,
            &jobs::CareerTrack {
                id: format!("track-{label}"),
                name: "Software engineering".to_string(),
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
        .expect("save matrix career track");
        let now = chrono::Utc::now().timestamp_millis();
        let slug = format!("{label}-{}", uuid::Uuid::new_v4().simple());
        let mut posting_input = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse".to_string(),
            external_id: slug.clone(),
            company: format!("Matrix {label} {slug}"),
            title: "Platform Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: format!("https://boards.greenhouse.io/matrix{label}/jobs/{slug}"),
            description: "Build reliable distributed systems.".to_string(),
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
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        posting_input.canonical_key = jobs::canonical_job_key(&posting_input);
        posting_input.discovery_evidence = JobDiscoveryEvidence::verified_original_source(
            posting_input.canonical_key.clone(),
            format!("matrix-employer:{slug}"),
            Some("boards.greenhouse.io".to_string()),
            now,
            "a".repeat(64),
        );
        let posting = jobs::upsert_posting(
            &self.pool,
            &account.id,
            &posting_input,
            &profile,
            &JobPreferences::default(),
        )
        .expect("save matrix posting");
        let (application, _) = jobs::prepare_application(
            &self.pool,
            &account.id,
            &posting.id,
            "factual",
            "review_first",
        )
        .expect("prepare matrix application");
        assert_eq!(application.state, "awaiting_review");
        application
    }

    async fn approve(&self, application: &JobApplication, token: &str) -> (StatusCode, Value) {
        self.post(
            &format!("/api/jobs/applications/{}/approve", application.id),
            token,
            None,
        )
        .await
    }

    async fn queue(
        &self,
        application: &JobApplication,
        token: &str,
        runner: &str,
    ) -> (StatusCode, Value) {
        self.post(
            &format!("/api/jobs/applications/{}/runs", application.id),
            token,
            Some(json!({ "runner": runner })),
        )
        .await
    }

    async fn post(&self, uri: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
        let request = Request::post(uri).header("authorization", format!("Bearer {token}"));
        let request = if let Some(body) = body {
            request
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&body).expect("serialize request"),
                ))
                .expect("build JSON request")
        } else {
            request.body(Body::empty()).expect("build empty request")
        };
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("execute matrix request");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 256 * 1024)
            .await
            .expect("read matrix response");
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(self.db_path.with_extension("db-shm"));
        let _ = std::fs::remove_file(self.db_path.with_extension("db-wal"));
    }
}

#[test]
fn jobs_api_handlers_have_no_direct_workflow_gateway_boundary() {
    let api_source = include_str!("../src/api/jobs.rs");
    assert!(!api_source.contains("/workflows/applications"));
    assert!(!api_source.contains("signal_workflow_resume"));
    assert!(!api_source.contains("WORKFLOW_START_TIMEOUT"));
    assert!(api_source.contains("stage_cloud_workflow_start"));
    assert!(api_source.contains("stage_cloud_workflow_resume"));
    assert!(api_source.contains("mark_jobs_workflow_execution_submitted"));

    let dispatcher_source = include_str!("../src/jobs_workflow_dispatch.rs");
    assert!(dispatcher_source.contains("join(\"workflow-commands\")"));
    assert!(!dispatcher_source.contains("workflows/applications"));
}

#[tokio::test]
#[serial]
async fn free_pro_cloud_runner_entitlement_matrix_is_enforced() {
    let _env = EnvGuard::capture(&[
        "BLUEY_JOBS_BETA_ENABLED",
        "BLUEY_JOBS_DATA_KEY",
        "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
        "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
        "BLUEY_JOBS_WORKFLOW_ORIGIN",
        "BLUEY_JOBS_WORKFLOW_TOKEN",
        "BLUEY_JOBS_BROWSER_ROOT_TRUST_ANCHOR_JSON",
        "BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID",
        "BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY",
    ]);
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");
    std::env::set_var("BLUEY_JOBS_DATA_KEY", JOBS_DATA_KEY);
    std::env::set_var("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED", "1");
    std::env::set_var("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED", "1");
    std::env::set_var(
        "BLUEY_JOBS_BROWSER_ROOT_TRUST_ANCHOR_JSON",
        browser_release_root_trust_anchor_json(),
    );
    std::env::set_var(
        "BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID",
        TEST_BROWSER_SERVER_RELEASE_ID,
    );
    std::env::set_var(
        "BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY",
        "phase-603-plan-matrix-capability-key",
    );
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_ORIGIN");
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_TOKEN");

    let ctx = TestContext::boot();
    enable_local_browser_distribution_for_test(&ctx.pool);
    seed_canonical_browser_release_registry(&ctx.pool);

    for (plan, expected_local, expected_cloud) in [
        ("free", false, false),
        ("pro", true, false),
        ("cloud", true, true),
    ] {
        let account = ctx.account(&format!("policy-{plan}"), plan);
        let entitlement =
            jobs::get_entitlement(&ctx.pool, &account.account.id).expect("read matrix entitlement");
        assert_eq!(entitlement.plan, plan);
        assert_eq!(entitlement.local_browser, expected_local);
        assert_eq!(entitlement.cloud_browser, expected_cloud);
    }

    // Review-first is a hard boundary even for the fully entitled Cloud plan.
    let review = ctx.account("review-boundary", "cloud");
    let awaiting_review = ctx.prepare(&review.account, "review-boundary");
    let (local_status, _) = ctx.queue(&awaiting_review, &review.token, "local").await;
    let (cloud_status, _) = ctx.queue(&awaiting_review, &review.token, "cloud").await;
    assert_eq!(local_status, StatusCode::CONFLICT);
    assert_eq!(cloud_status, StatusCode::CONFLICT);
    assert_eq!(
        jobs::get_entitlement(&ctx.pool, &review.account.id)
            .expect("read review entitlement")
            .used_packets,
        0
    );

    // Free may approve a reviewed packet, but neither browser runner is included.
    let free = ctx.account("free", "free");
    let free_application = ctx.prepare(&free.account, "free");
    let (free_approval, _) = ctx.approve(&free_application, &free.token).await;
    assert_eq!(free_approval, StatusCode::OK);
    let (free_local, _) = ctx.queue(&free_application, &free.token, "local").await;
    let (free_cloud, _) = ctx.queue(&free_application, &free.token, "cloud").await;
    assert_eq!(free_local, StatusCode::PAYMENT_REQUIRED);
    assert_eq!(free_cloud, StatusCode::PAYMENT_REQUIRED);

    // Every build requires the explicit distribution flag in addition to release authority.
    let pro_disabled = ctx.account("pro-distribution-disabled", "pro");
    let pro_disabled_application = ctx.prepare(&pro_disabled.account, "pro-distribution-disabled");
    let (approval_status, _) = ctx
        .approve(&pro_disabled_application, &pro_disabled.token)
        .await;
    assert_eq!(approval_status, StatusCode::OK);
    std::env::set_var("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED", "0");
    let (disabled_status, _) = ctx
        .queue(&pro_disabled_application, &pro_disabled.token, "local")
        .await;
    assert_eq!(disabled_status, StatusCode::SERVICE_UNAVAILABLE);
    std::env::set_var("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED", "1");

    // Pro receives local only. Approval meters once, and queue/retry paths are idempotent.
    let pro = ctx.account("pro", "pro");
    assign_browser_release_channel(&ctx.pool, &pro.account.id);
    let pro_release = jobs::local_browser_release_availability(
        &ctx.pool,
        &pro.account.id,
        TEST_BROWSER_SERVER_RELEASE_ID,
    )
    .expect("load assigned Pro Browser release");
    assert!(matches!(
        pro_release,
        jobs::LocalBrowserReleaseAvailability::Available {
            ref channel,
            ref release_id,
            ref artifact_origin,
            ..
        } if channel == "beta"
            && release_id == "browser-release-603-1"
            && artifact_origin == "https://bluey.sh"
    ));
    let pro_application = ctx.prepare(&pro.account, "pro");
    let (approval_status, approval_body) = ctx.approve(&pro_application, &pro.token).await;
    assert_eq!(approval_status, StatusCode::OK);
    assert_eq!(approval_body["metering"]["newly_metered"], true);
    assert_eq!(approval_body["metering"]["used_packets"], 1);
    let (second_approval_status, _) = ctx.approve(&pro_application, &pro.token).await;
    assert_eq!(second_approval_status, StatusCode::CONFLICT);
    let (pro_local_status, pro_local_body) = ctx.queue(&pro_application, &pro.token, "local").await;
    assert_eq!(pro_local_status, StatusCode::OK);
    assert!(pro_local_body["launch_url"]
        .as_str()
        .is_some_and(|url| url.starts_with("bluey-jobs://run/")));
    let (pro_cloud_status, _) = ctx.queue(&pro_application, &pro.token, "cloud").await;
    assert_eq!(pro_cloud_status, StatusCode::PAYMENT_REQUIRED);
    let pro_entitlement =
        jobs::get_entitlement(&ctx.pool, &pro.account.id).expect("read Pro metering");
    assert_eq!(pro_entitlement.used_packets, 1);
    let replay = jobs::commit_packet(&ctx.pool, &pro.account.id, &pro_application.id)
        .expect("replay Pro packet commit");
    assert!(!replay.newly_metered);
    assert_eq!(replay.used_packets, 1);

    // Cloud includes both runners. The request handler only stages durable work; it never
    // depends on gateway configuration or performs gateway I/O.
    let cloud = ctx.account("cloud", "cloud");
    assign_browser_release_channel(&ctx.pool, &cloud.account.id);
    let cloud_local_application = ctx.prepare(&cloud.account, "cloud-local");
    let (cloud_local_approval, _) = ctx.approve(&cloud_local_application, &cloud.token).await;
    assert_eq!(cloud_local_approval, StatusCode::OK);
    let (cloud_local_status, _) = ctx
        .queue(&cloud_local_application, &cloud.token, "local")
        .await;
    assert_eq!(cloud_local_status, StatusCode::OK);

    let cloud_gateway_application = ctx.prepare(&cloud.account, "cloud-gateway");
    let (cloud_gateway_approval, _) = ctx.approve(&cloud_gateway_application, &cloud.token).await;
    assert_eq!(cloud_gateway_approval, StatusCode::OK);
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_TOKEN");
    let (staged_status, staged_body) = ctx
        .queue(&cloud_gateway_application, &cloud.token, "cloud")
        .await;
    assert_eq!(staged_status, StatusCode::OK, "{staged_body}");
    assert_eq!(staged_body["application"]["state"], "queued");
    assert_eq!(staged_body["browser_session"]["runner"], "cloud");
    assert!(staged_body["browser_session"]["id"]
        .as_str()
        .is_some_and(|session_id| session_id.starts_with("cloud-")));
    assert!(staged_body["workflow_id"]
        .as_str()
        .is_some_and(|workflow_id| workflow_id.starts_with("bluey-jobs-v2-")));
    assert_eq!(staged_body["workflow_command"]["schema_version"], 2);
    assert_eq!(staged_body["workflow_command"]["operation"], "start");
    assert_eq!(staged_body["workflow_command"]["state"], "pending");
    assert_eq!(staged_body["workflow_command"]["replayed"], false);

    let gateway = MockServer::start().await;
    std::env::set_var("BLUEY_JOBS_WORKFLOW_ORIGIN", gateway.uri());
    std::env::set_var("BLUEY_JOBS_WORKFLOW_TOKEN", "matrix-workflow-token");
    let (replay_status, replay_body) = ctx
        .queue(&cloud_gateway_application, &cloud.token, "cloud")
        .await;
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(
        replay_body["workflow_command"]["command_id"],
        staged_body["workflow_command"]["command_id"]
    );
    assert_eq!(
        replay_body["workflow_command"]["request_id"],
        staged_body["workflow_command"]["request_id"]
    );
    assert_eq!(
        replay_body["workflow_command"]["workflow_id"],
        staged_body["workflow_command"]["workflow_id"]
    );
    assert_eq!(replay_body["workflow_command"]["replayed"], true);
    assert!(gateway.received_requests().await.unwrap().is_empty());
}

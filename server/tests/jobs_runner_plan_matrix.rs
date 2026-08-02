#![cfg(test)]

use std::ffi::OsString;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
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
use serde_json::{json, Value};
use serial_test::serial;
use tower::ServiceExt;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const TEST_SECRET: &str = "jobs-runner-plan-matrix-secret-32-bytes";
const JOBS_DATA_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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
    ]);
    std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");
    std::env::set_var("BLUEY_JOBS_DATA_KEY", JOBS_DATA_KEY);
    std::env::set_var("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED", "1");
    std::env::set_var("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED", "1");
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_ORIGIN");
    std::env::remove_var("BLUEY_JOBS_WORKFLOW_TOKEN");

    let ctx = TestContext::boot();

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

    // In production builds Pro's local runner also requires distribution to be enabled.
    // Debug builds intentionally bypass this release gate, so the focused proof is run
    // with `cargo test --release --test jobs_runner_plan_matrix`.
    if !cfg!(debug_assertions) {
        let pro_disabled = ctx.account("pro-distribution-disabled", "pro");
        let pro_disabled_application =
            ctx.prepare(&pro_disabled.account, "pro-distribution-disabled");
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
    }

    // Pro receives local only. Approval meters once, and queue/retry paths are idempotent.
    let pro = ctx.account("pro", "pro");
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

    // Cloud includes both runners, but the cloud path is unavailable without its gateway.
    let cloud = ctx.account("cloud", "cloud");
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
    let (missing_gateway_status, _) = ctx
        .queue(&cloud_gateway_application, &cloud.token, "cloud")
        .await;
    assert_eq!(missing_gateway_status, StatusCode::SERVICE_UNAVAILABLE);

    let gateway = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/workflows/applications"))
        .and(header("authorization", "Bearer matrix-workflow-token"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&gateway)
        .await;
    std::env::set_var("BLUEY_JOBS_WORKFLOW_ORIGIN", gateway.uri());
    std::env::set_var("BLUEY_JOBS_WORKFLOW_TOKEN", "matrix-workflow-token");
    let (cloud_status, cloud_body) = ctx
        .queue(&cloud_gateway_application, &cloud.token, "cloud")
        .await;
    assert_eq!(cloud_status, StatusCode::OK);
    assert_eq!(cloud_body["application"]["state"], "queued");
    assert_eq!(cloud_body["browser_session"]["runner"], "cloud");
    assert!(cloud_body["workflow_id"]
        .as_str()
        .is_some_and(|workflow_id| workflow_id.starts_with("bluey-jobs:")));
}

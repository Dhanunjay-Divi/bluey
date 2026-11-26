//! Same-attempt SmartRecruiters worker HTTP fixture. Never mounted in a build.

use super::*;
use crate::config::{Config, ServerDbBackend};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const SYNTHETIC_KEY: &str = "synthetic-smartrecruiters-wire-signing-key-v1";
const SIGNING_KEY_ENV: &str = "BLUEY_JOBS_WORKER_SIGNING_KEY";

struct OwnedSigningKey;

impl OwnedSigningKey {
    fn install() -> Self {
        assert!(std::env::var_os(SIGNING_KEY_ENV).is_none());
        assert!(std::env::var_os("BLUEY_JOBS_WORKER_SIGNING_KEY_PREVIOUS").is_none());
        assert!(std::env::var_os("BLUEY_JOBS_WORKER_TOKEN").is_none());
        assert!(std::env::var_os("BLUEY_REDIS_URL").is_none());
        std::env::set_var(SIGNING_KEY_ENV, SYNTHETIC_KEY);
        Self
    }
}

impl Drop for OwnedSigningKey {
    fn drop(&mut self) {
        std::env::remove_var(SIGNING_KEY_ENV);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpTrace {
    assignment_id: String,
    attempt_id: String,
    receipt_sha256: String,
    completion_requests: usize,
    replayed: bool,
}

fn isolated_config(pool: &DbPool, origin: &str) -> Config {
    Config {
        port: 0,
        db_path: PathBuf::new(),
        db_backend: match pool {
            DbPool::Sqlite(_) => ServerDbBackend::Sqlite,
            DbPool::Postgres(_) => ServerDbBackend::Postgres,
        },
        database_url: None,
        jwt_secret: "jobs-worker-http-test-only-jwt-key".to_string(),
        public_url: origin.to_string(),
        stripe_secret_key: None,
        stripe_webhook_secret: None,
        upstream: Default::default(),
        upstream_spend_guard: None,
        smtp: None,
        admin_emails: Vec::new(),
        trial_abuse: Default::default(),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: false,
        object_storage: None,
        log_storage: None,
    }
}

pub(super) fn exercise(
    pool: &DbPool,
    posting: &JobPosting,
    current: &AssignmentSnapshot,
    binding: &OriginalSourceVerifierBinding,
    runtime_identity_sha256: &str,
) {
    let node = worker_wire_node().expect("owned fixture requires the pinned Node executable");
    let run_root = std::env::var_os("BLUEY_TEST_RUN_ROOT").expect("canonical launcher required");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = source.join("jobs/scripts/fixtures/nonprod-smartrecruiters-worker-http.mjs");
    assert!(
        source.starts_with(&run_root),
        "worker HTTP requires staged source"
    );
    let before = worker_wire_effect_census(pool);
    let _key = OwnedSigningKey::install();
    let trace = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .expect("owned worker HTTP listener");
            let endpoint = listener.local_addr().unwrap();
            assert!(
                endpoint.ip().is_loopback() && endpoint.port() > 1024 && endpoint.port() != 5432
            );
            let origin = format!("http://{endpoint}");
            let router =
                crate::api::build_jobs_router(pool.clone(), isolated_config(pool, &origin));
            let serving = tokio::spawn(async move { axum::serve(listener, router).await });
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap();
            let unsigned = client
                .post(format!(
                    "{origin}/api/jobs/internal/original-source-verifications/lease"
                ))
                .json(&serde_json::json!({ "binding": binding }))
                .send()
                .await
                .expect("owned worker HTTP unsigned control");
            assert_eq!(unsigned.status(), reqwest::StatusCode::UNAUTHORIZED);
            let mut worker = tokio::process::Command::new(node)
                .current_dir(source)
                .env_clear()
                .env("BLUEY_TEST_RUN_ROOT", run_root)
                .arg(fixture)
                .arg("--synthetic-http-retry")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .expect("start owned worker HTTP fixture");
            let input = serde_json::json!({
                "binding": binding,
                "runtimeIdentitySha256": runtime_identity_sha256,
                "origin": origin,
            });
            worker
                .stdin
                .take()
                .unwrap()
                .write_all(serde_json::to_string(&input).unwrap().as_bytes())
                .await
                .expect("send one synthetic worker HTTP fixture request");
            let output = tokio::time::timeout(Duration::from_secs(12), worker.wait_with_output())
                .await
                .expect("worker HTTP fixture deadline")
                .expect("owned worker HTTP result");
            serving.abort();
            assert!(serving
                .await
                .expect_err("router stopped before teardown")
                .is_cancelled());
            let after_close = tokio::time::timeout(
                Duration::from_secs(1),
                tokio::net::TcpStream::connect(endpoint),
            )
            .await
            .expect("loopback closure readback deadline");
            assert_eq!(
                after_close
                    .expect_err("owned HTTP listener still reachable")
                    .kind(),
                std::io::ErrorKind::ConnectionRefused
            );
            assert!(
                output.status.success() && output.stdout.len() <= 4_096,
                "worker HTTP fixture did not finish: {:?}",
                output.status.code()
            );
            serde_json::from_slice::<HttpTrace>(&output.stdout).expect("worker HTTP evidence")
        });
    assert_eq!(trace.assignment_id, current.assignment_id);
    assert_eq!(trace.completion_requests, 2);
    assert!(trace.replayed);
    let attempt = attempt_snapshot(pool, &trace.attempt_id);
    assert_eq!(attempt.assignment_id, current.assignment_id);
    assert_eq!(attempt.worker_id, binding.worker_id);
    assert_eq!(attempt.runtime_instance_id, binding.runtime_instance_id);
    assert_eq!(
        attempt.runtime_authority_sha256,
        binding.runtime_authority_sha256
    );
    let state = rejected_predecessor_snapshot(pool, &current.assignment_id, &trace.attempt_id);
    assert_eq!(state.assignment.state, "idle");
    assert_eq!(state.assignment.active_attempt_id, None);
    assert_eq!(state.attempt_rows, 1);
    assert_eq!(state.observation_rows, 1);
    assert_eq!(state.receipt_rows, 1);
    assert_eq!(state.head_rows, 1);
    assert_eq!(state.transition_rows, 1);
    let head = resolve_original_source_verification_head(pool, ACCOUNT_ID, &posting.id)
        .unwrap()
        .expect("HTTP verification published a source head");
    assert_eq!(head.assignment_id, current.assignment_id);
    assert_eq!(head.subject_sha256, current.subject_sha256);
    assert_eq!(head.receipt_sha256, trace.receipt_sha256);
    assert_eq!(head.result, "verified_open");
    assert_eq!(head.assurance, "original_verified");
    assert_eq!(
        head.canonical_application_url.as_deref(),
        Some("https://jobs.smartrecruiters.com/acme/smart-1-platform-engineer?oga=true")
    );
    let eligibility = evaluate_job_eligibility(pool, ACCOUNT_ID, posting, true, None).unwrap();
    assert!(
        !eligibility.can_queue_local
            && !eligibility.can_queue_cloud
            && !eligibility.can_auto_submit
    );
    assert_eq!(worker_wire_effect_census(pool), before);
}

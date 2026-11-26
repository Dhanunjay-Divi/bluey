//! Loopback-only receipt publication fixture for the owned review-first test.

#[cfg(test)]
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
#[cfg(test)]
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

use crate::{
    config::ObjectStorageConfig,
    db::{self, jobs},
    jobs_review_first_mock_run_fixture::MockRun,
};

const MAX_BODY_BYTES: usize = 512 * 1024;
const SAMPLE_SHA256: &str = "92b41ea87bbc39031cac457edb51ba6693bb9e5773b0e18f5c370fee0e2a1cdb";
#[doc(hidden)]
pub const DURABLE_MOCK_EFFECT_FLAGS: [&str; 12] = [
    "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
    "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
    "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED",
    "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED",
    "BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED",
    "BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED",
    "BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED",
    "BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED",
    "BLUEY_JOBS_MAILBOX_SYNC_ENABLED",
    "BLUEY_JOBS_MODEL_GENERATION_ENABLED",
    "BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED",
    "BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED",
];
#[cfg(test)]
const OBJECT_BUCKET: &str = "review-first-receipts";
#[cfg(test)]
const OBJECT_PREFIX: &str = "review-first-fixture";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurableMockReceiptRequest {
    pub version: u8,
    pub account_id: String,
    pub application_id: String,
    pub job_id: String,
    pub resume_id: String,
    pub source_asset_id: String,
    pub source_sha256: String,
    pub expected_system_identifier: String,
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}

impl DurableMockReceiptRequest {
    pub fn valid(&self) -> bool {
        self.version == 1
            && [
                &self.account_id,
                &self.application_id,
                &self.resume_id,
                &self.source_asset_id,
            ]
            .into_iter()
            .all(|text| {
                uuid::Uuid::parse_str(text).is_ok_and(|value| {
                    value.get_version_num() == 4 && value.to_string() == text.as_str()
                })
            })
            && id(&self.job_id)
            && self.source_sha256 == SAMPLE_SHA256
            && self
                .expected_system_identifier
                .parse::<u64>()
                .is_ok_and(|value| {
                    value != 0 && value.to_string() == self.expected_system_identifier
                })
    }
}

/// Run only inside the pinned debug executable after physical custody and
/// request consumption. This constructs an in-process router, not a listener.
#[doc(hidden)]
pub async fn publish_durable_mock_receipt(
    pool: &db::DbPool,
    draft: &DurableMockReceiptRequest,
    data_directory: &std::path::Path,
    database_port: u16,
    store: ObjectStorageConfig,
    jwt_secret: String,
    worker_token: &str,
) -> Result<Value> {
    use crate::auth::jwt::{issue, TokenKind};
    use crate::config::{ServerDbBackend, TrialAbuseConfig, UpstreamKeys};
    anyhow::ensure!(
        draft.valid()
            && matches!(pool, db::DbPool::Postgres(_))
            && hex64(&jwt_secret)
            && hex64(worker_token)
            && jwt_secret != worker_token
            && std::env::var("BLUEY_JOBS_WORKER_TOKEN").as_deref() == Ok(worker_token)
            && std::env::var_os("BLUEY_DATABASE_URL").is_none()
            && std::env::var("BLUEY_JOBS_BETA_ENABLED").as_deref() == Ok("1")
            && DURABLE_MOCK_EFFECT_FLAGS
                .iter()
                .all(|key| std::env::var(key).as_deref() == Ok("0")),
        "durable mock actor environment refused"
    );
    let root =
        std::path::PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").context("mock root")?);
    anyhow::ensure!(
        root.parent()
            .and_then(std::path::Path::parent)
            .and_then(std::path::Path::to_str)
            .is_some_and(|path| matches!(path, "/tmp" | "/private/tmp"))
            && root
                .parent()
                .and_then(std::path::Path::file_name)
                .and_then(|v| v.to_str())
                .is_some_and(|v| v.starts_with("bluey-test-runs-"))
            && std::fs::canonicalize(&root)? == root
            && data_directory == root.join("bluey-jobs-qa-draft/postgres")
            && std::fs::canonicalize(data_directory)? == data_directory,
        "durable mock actor root refused"
    );
    let store_port = store
        .endpoint_url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|text| text.parse::<u16>().ok().filter(|p| p.to_string() == text));
    anyhow::ensure!(
        database_port > 1024
            && database_port != 5432
            && store_port.is_some_and(|port| port > 1024 && port != 5432 && port != database_port),
        "durable mock actor requires distinct local ports"
    );
    let before = db::run_blocking_db(|| -> Result<jobs::JobApplication> {
        let row = pool.get_pg()?.query_one(
            "SELECT current_setting('data_directory'), host(inet_server_addr()), inet_server_port(),
                    current_database(), current_user, system_identifier::text FROM pg_control_system()",
            &[],
        )?;
        anyhow::ensure!(
            Some(row.try_get::<_, String>(0)?.as_str()) == data_directory.to_str()
                && row.try_get::<_, String>(1)? == "127.0.0.1"
                && row.try_get::<_, i32>(2)? == i32::from(database_port)
                && row.try_get::<_, String>(3)? == "bluey_jobs_qa"
                && row.try_get::<_, String>(4)? == "bluey_qa_local"
                && row.try_get::<_, String>(5)? == draft.expected_system_identifier,
            "durable mock actor database mismatch"
        );
        let account = db::accounts::Account::fetch_by_id(pool, &draft.account_id)?
            .context("synthetic candidate missing")?;
        let source =
            jobs::get_resume_source_asset(pool, &draft.account_id)?.context("mock source")?;
        let resume = jobs::get_resume_version(pool, &draft.account_id, &draft.resume_id)?
            .context("mock resume")?;
        let rows = jobs::list_applications(pool, &draft.account_id)?;
        let [application] = rows.as_slice() else {
            anyhow::bail!("mock requires one draft");
        };
        let provenance = resume
            .content
            .pointer("/provenance/career_track_policy_authority");
        anyhow::ensure!(
            account.email == "candidate@bluey-qa.test"
                && account.email_verified_at.is_some()
                && !account.is_admin
                && !account.is_temporary
                && account.balance_cents == 0
                && source.id == draft.source_asset_id
                && source.sha256 == draft.source_sha256
                && source.file_name == "bluey-qa-resume.txt"
                && source.file_type == "txt"
                && source.media_type == "text/plain"
                && source.template_status == "text_only"
                && source.size_bytes == 368
                && resume.id == draft.resume_id
                && resume.job_id == draft.job_id
                && resume.mode == "factual"
                && provenance.and_then(|v| v["source_resume_asset_id"].as_str())
                    == Some(draft.source_asset_id.as_str())
                && provenance.and_then(|v| v["source_resume_sha256"].as_str())
                    == Some(draft.source_sha256.as_str())
                && application.id == draft.application_id
                && application.job_id == draft.job_id
                && application.resume_version_id.as_deref() == Some(draft.resume_id.as_str())
                && application.state == "awaiting_review"
                && application.submission_mode == "review_first"
                && application.run_id.is_none()
                && application.submitted_at_ms.is_none()
                && application.receipt.get("approved_execution").is_none()
                && jobs::list_attempt_reservations(pool, &draft.account_id)?.is_empty(),
            "durable mock actor draft mismatch"
        );
        Ok(application.clone())
    })?;
    let token = issue(&jwt_secret, &draft.account_id, TokenKind::Access)?;
    let config = crate::Config {
        port: 0,
        db_path: ":memory:".into(),
        db_backend: ServerDbBackend::Postgres,
        database_url: None,
        jwt_secret,
        public_url: "http://127.0.0.1".into(),
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
        object_storage: Some(store),
        log_storage: None,
    };
    let router = crate::api::build_jobs_router(pool.clone(), config);
    let approval = router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/applications/{}/approve",
                draft.application_id
            ))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from("{}"))?,
        )
        .await
        .context("mock approval request")?;
    let approved = decode_ok(approval, "mock approval").await?;
    anyhow::ensure!(
        approved.pointer("/application/id").and_then(Value::as_str)
            == Some(draft.application_id.as_str())
            && approved
                .pointer("/application/state")
                .and_then(Value::as_str)
                == Some("queued")
            && approved
                .pointer("/application/resume_version_id")
                .and_then(Value::as_str)
                == before.resume_version_id.as_deref(),
        "mock approval did not freeze the draft"
    );
    let pdf = receipt_pdf()?;
    let (staged, application) = db::run_blocking_db(|| -> Result<_> {
        let staged = super::jobs_review_first_mock_run_fixture::stage_bound_durable_mock_run(
            pool,
            &draft.account_id,
            &draft.application_id,
            &pdf,
        )?;
        let application = jobs::get_application(pool, &draft.account_id, &draft.application_id)?
            .context("mock staged application")?;
        Ok((staged, application))
    })?;
    anyhow::ensure!(
        application.state == "side_effect_unknown"
            && application.run_id.as_deref() == Some(staged.run_id.as_str())
    );
    let body = receipt_request(
        &application,
        &draft.account_id,
        &draft.application_id,
        &staged,
        &pdf,
    )?;
    let denied = post_receipt(&router, &token, &draft.application_id, &body).await?;
    anyhow::ensure!(
        denied.status() == StatusCode::UNAUTHORIZED,
        "mock candidate receipt denied"
    );
    let published = decode_ok(
        post_receipt(&router, worker_token, &draft.application_id, &body).await?,
        "mock worker receipt",
    )
    .await?;
    let replay = decode_ok(
        post_receipt(&router, worker_token, &draft.application_id, &body).await?,
        "mock worker exact replay",
    )
    .await?;
    anyhow::ensure!(published == replay, "mock receipt replay differs");
    let workspace = candidate_workspace(
        &router,
        &token,
        &[worker_token, &staged.lease_token, &token],
    )
    .await?;
    let [row] = workspace["applications"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    else {
        anyhow::bail!("mock workspace must expose one row");
    };
    anyhow::ensure!(
        row["id"] == draft.application_id
            && row["job_id"] == draft.job_id
            && row["state"] == "submitted"
            && row["submission_mode"] == "review_first"
            && row["run_id"] == staged.run_id
            && row["resume_version_id"] == draft.resume_id
            && row
                .pointer("/receipt/packet/resumeVersionId")
                .and_then(Value::as_str)
                == Some(draft.resume_id.as_str()),
        "mock workspace did not expose bound receipt"
    );
    let object = row
        .pointer("/receipt/receiptObject/sha256")
        .and_then(Value::as_str)
        .filter(|s| hex64(s))
        .context("mock receipt object digest")?;
    let fingerprint = row
        .pointer("/receipt/_bluey_server_submission_fingerprint_v1")
        .and_then(Value::as_str)
        .filter(|s| hex64(s))
        .context("mock receipt fingerprint")?;
    Ok(json!({
        "accountId":draft.account_id, "applicationId":draft.application_id, "jobId":draft.job_id,
        "resumeId":draft.resume_id, "sourceAssetId":draft.source_asset_id, "sourceSha256":draft.source_sha256,
        "runId":staged.run_id, "receiptObjectSha256":object, "submissionFingerprintSha256":fingerprint,
    }))
}

#[cfg(test)]
pub(crate) async fn verify_mock_receipt(
    pool: &crate::db::DbPool,
    mut config: crate::Config,
    candidate: (&str, &str),
    source: &jobs::ResumeSourceAsset,
    application_id: &str,
    worker_token: &str,
) -> Result<()> {
    let (account_id, candidate_token) = candidate;
    let pdf = receipt_pdf()?;
    let staged = db::run_blocking_db(|| {
        super::jobs_review_first_mock_run_fixture::stage_mock_run(
            pool,
            account_id,
            application_id,
            &pdf,
        )
    })?;
    let application = db::run_blocking_db(|| {
        jobs::get_application(pool, account_id, application_id)?
            .context("staged review-first application")
    })?;
    anyhow::ensure!(
        application.submission_mode == "review_first"
            && application.run_id.as_deref() == Some(staged.run_id.as_str())
            && application.state == "side_effect_unknown",
        "review-first fixture must reach the guarded uncertain submission phase"
    );

    let store = MockServer::start().await;
    anyhow::ensure!(
        store.address().ip().is_loopback(),
        "receipt store must be loopback"
    );
    let objects = Arc::new(Mutex::new(BTreeMap::<String, (Vec<u8>, String)>::new()));
    config.object_storage = Some(ObjectStorageConfig {
        endpoint_url: store.uri(),
        bucket: OBJECT_BUCKET.to_string(),
        access_key_id: "fixture-only".to_string(),
        secret_access_key: "fixture-only".to_string(),
        region: "auto".to_string(),
        key_prefix: OBJECT_PREFIX.to_string(),
        retention_days: 1,
        max_object_bytes: 1024 * 1024,
    });
    let router = crate::api::build_router(pool.clone(), config);
    let body = receipt_request(&application, account_id, application_id, &staged, &pdf)?;
    let receipt_id = body["receipt"]["receiptId"]
        .as_str()
        .context("mock receipt id")?;
    mount_receipt_store(
        &store,
        Arc::clone(&objects),
        account_id,
        application_id,
        receipt_id,
    )
    .await;

    let before = db::run_blocking_db(|| -> Result<Value> {
        Ok(json!({
            "application": jobs::get_application(pool, account_id, application_id)?,
            "evidence": jobs::list_application_evidence(pool, account_id, Some(application_id))?,
        }))
    })?;
    let denied = post_receipt(&router, candidate_token, application_id, &body).await?;
    anyhow::ensure!(
        denied.status() == StatusCode::UNAUTHORIZED,
        "candidate receipt must deny"
    );
    let _ = to_bytes(denied.into_body(), MAX_BODY_BYTES).await?;
    let after_denial = db::run_blocking_db(|| -> Result<Value> {
        Ok(json!({
            "application": jobs::get_application(pool, account_id, application_id)?,
            "evidence": jobs::list_application_evidence(pool, account_id, Some(application_id))?,
        }))
    })?;
    anyhow::ensure!(
        before == after_denial,
        "candidate receipt denial changed state"
    );
    anyhow::ensure!(
        objects
            .lock()
            .map_err(|_| anyhow::anyhow!("receipt object lock"))?
            .is_empty(),
        "candidate receipt denial touched the object store"
    );
    anyhow::ensure!(
        store
            .received_requests()
            .await
            .context("read denied receipt object requests")?
            .is_empty(),
        "candidate receipt denial made an object-store request"
    );

    let published = post_receipt(&router, worker_token, application_id, &body).await?;
    let published_body = decode_ok(published, "review-first receipt publish").await?;
    anyhow::ensure!(
        published_body["id"] == application_id
            && published_body["state"] == "submitted"
            && published_body["submission_mode"] == "review_first",
        "receipt did not submit exact review-first application"
    );
    let stored_after_publish = objects
        .lock()
        .map_err(|_| anyhow::anyhow!("receipt object lock"))?
        .clone();
    anyhow::ensure!(
        stored_after_publish.len() == 3,
        "receipt must write two evidence objects and one bundle"
    );
    let request_count_after_publish = store
        .received_requests()
        .await
        .context("read published receipt object requests")?
        .len();
    anyhow::ensure!(
        request_count_after_publish == 6,
        "receipt must put and read back exactly three objects"
    );

    let replay = post_receipt(&router, worker_token, application_id, &body).await?;
    let replay_body = decode_ok(replay, "review-first receipt replay").await?;
    anyhow::ensure!(
        replay_body["id"] == application_id && replay_body["state"] == "submitted",
        "receipt replay changed result"
    );
    anyhow::ensure!(
        *objects
            .lock()
            .map_err(|_| anyhow::anyhow!("receipt object lock"))?
            == stored_after_publish,
        "receipt replay wrote additional evidence"
    );
    anyhow::ensure!(
        store
            .received_requests()
            .await
            .context("read replay receipt object requests")?
            .len()
            == request_count_after_publish,
        "receipt replay made additional object-store requests"
    );

    let workspace = candidate_workspace(
        &router,
        candidate_token,
        &[worker_token, staged.lease_token.as_str()],
    )
    .await?;
    let applications = workspace["applications"]
        .as_array()
        .context("candidate workspace applications")?;
    anyhow::ensure!(
        applications.len() == 1
            && applications[0]["id"] == application_id
            && applications[0]["state"] == "submitted"
            && applications[0]["submission_mode"] == "review_first"
            && applications[0]["resume_version_id"].as_str()
                == application.resume_version_id.as_deref()
            && applications[0]
                .pointer("/receipt/packet/resumeVersionId")
                .and_then(Value::as_str)
                == application.resume_version_id.as_deref(),
        "candidate workspace missing submitted review-first provenance"
    );
    anyhow::ensure!(
        workspace["profile"]["source_resume_asset_id"] == source.id
            && workspace["profile"]["source_resume_sha256"] == source.sha256,
        "candidate workspace source provenance mismatch"
    );
    db::run_blocking_db(|| {
        let mut connection = pool.get_pg()?;
        let counts = connection.query_one(
            "SELECT (SELECT count(*) FROM jobs_auto_submit_authorizations WHERE account_id=$1),
                     (SELECT count(*) FROM jobs_workflow_commands WHERE account_id=$1),
                     (SELECT count(*) FROM jobs_application_evidence WHERE account_id=$1 AND application_id=$2)",
            &[&account_id, &application_id],
        )?;
        anyhow::ensure!(
            counts.get::<_, i64>(0) == 0
                && counts.get::<_, i64>(1) == 0
                && counts.get::<_, i64>(2) == 3,
            "receipt created auto authority or workflow dispatch"
        );
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
type ReceiptStoreObjects = Arc<Mutex<BTreeMap<String, (Vec<u8>, String)>>>;

#[cfg(test)]
async fn mount_receipt_store(
    store: &MockServer,
    objects: ReceiptStoreObjects,
    account_id: &str,
    application_id: &str,
    receipt_id: &str,
) {
    let tenant = format!("/{OBJECT_BUCKET}/{OBJECT_PREFIX}/accounts/{account_id}");
    let artifact = format!("{tenant}/context/jobs/{application_id}/receipts/");
    let application_scope = hex::encode(Sha256::digest(application_id.as_bytes()));
    let receipt_scope = hex::encode(Sha256::digest(receipt_id.as_bytes()));
    let bundle =
        format!("{tenant}/jobs/applications/{application_scope}/receipts/{receipt_scope}/bundles/");
    let put_artifact = artifact.clone();
    let put_bundle = bundle.clone();
    let writes = Arc::clone(&objects);
    Mock::given(method("PUT"))
        .respond_with(move |request: &wiremock::Request| {
            let path = request.url.path();
            if !receipt_store_key(path, &put_artifact, &put_bundle) {
                return ResponseTemplate::new(404);
            }
            let content_type = request
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let expected_sha256 = request
                .headers
                .get("x-amz-content-sha256")
                .and_then(|value| value.to_str().ok());
            let actual_sha256 = hex::encode(Sha256::digest(&request.body));
            if expected_sha256 != Some(actual_sha256.as_str())
                || !matches!(
                    content_type.as_str(),
                    "application/pdf" | "image/png" | "application/json"
                )
            {
                return ResponseTemplate::new(400);
            }
            let Ok(mut objects) = writes.lock() else {
                return ResponseTemplate::new(500);
            };
            if objects
                .insert(path.to_string(), (request.body.clone(), content_type))
                .is_some()
            {
                return ResponseTemplate::new(409);
            }
            ResponseTemplate::new(200)
        })
        .mount(store)
        .await;
    Mock::given(method("GET"))
        .respond_with(move |request: &wiremock::Request| {
            if !receipt_store_key(request.url.path(), &artifact, &bundle) {
                return ResponseTemplate::new(404);
            }
            let Ok(objects) = objects.lock() else {
                return ResponseTemplate::new(500);
            };
            match objects.get(request.url.path()) {
                Some((bytes, content_type)) => {
                    ResponseTemplate::new(200).set_body_raw(bytes.clone(), content_type)
                }
                None => ResponseTemplate::new(404),
            }
        })
        .mount(store)
        .await;
}

#[cfg(test)]
fn receipt_store_key(path: &str, artifact: &str, bundle: &str) -> bool {
    fn digest(value: &str, length: usize) -> bool {
        value.len() == length
            && value
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    }

    if let Some((fingerprint, leaf)) = path
        .strip_prefix(artifact)
        .and_then(|suffix| suffix.split_once('/'))
    {
        return digest(fingerprint, 64)
            && ["0-resume-", "1-screenshot-"].into_iter().any(|prefix| {
                leaf.strip_prefix(prefix)
                    .is_some_and(|checksum| digest(checksum, 20))
            });
    }
    path.strip_prefix(bundle)
        .and_then(|suffix| suffix.split_once("/sha256/"))
        .is_some_and(|(bundle_id, leaf)| {
            digest(bundle_id, 64)
                && leaf
                    .strip_suffix(".json")
                    .is_some_and(|value| digest(value, 64))
        })
}

fn receipt_request(
    application: &jobs::JobApplication,
    account_id: &str,
    application_id: &str,
    staged: &MockRun,
    pdf: &[u8],
) -> Result<Value> {
    let MockRun {
        run_id,
        lease_token,
        fence,
        adapter_version,
    } = staged;
    let resume_id = application
        .resume_version_id
        .as_deref()
        .context("review-first receipt resume")?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("review-first receipt identity")?;
    let application_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .context("review-first receipt email")?;
    let approved = application
        .receipt
        .get("approved_execution")
        .and_then(Value::as_object)
        .context("review-first approved execution")?;
    let packet = approved
        .get("packet")
        .and_then(Value::as_object)
        .context("approved packet")?;
    let checksum = approved
        .get("checksum")
        .and_then(Value::as_str)
        .context("approved checksum")?;
    let job = approved.get("job").cloned().context("approved job")?;
    let canonical_url = job["canonicalUrl"]
        .as_str()
        .context("approved canonical url")?;
    let confirmation_url = format!("{}/confirmation", canonical_url.trim_end_matches('/'));
    let png = receipt_png()?;
    let pdf_sha = hex::encode(Sha256::digest(pdf));
    let png_sha = hex::encode(Sha256::digest(&png));
    let resume_key = "review-first/documents/resume.pdf";
    let screenshot_key = "review-first/final.png";
    Ok(json!({
        "account_id": account_id, "lease_token": lease_token, "fence": fence,
        "receipt": {"schemaVersion": 1, "receiptId": format!("review-first-receipt-{run_id}"),
            "accountId": account_id, "applicationId": application_id, "runId": run_id, "runner": "cloud",
            "generatedAt": "2026-11-22T12:00:00Z", "applicationIdentityId": identity_id,
            "browserProfileId": jobs::execution_browser_profile_id(account_id, identity_id),
            "adapter": "greenhouse", "adapterVersion": adapter_version,
            "packet": {"jobId": application.job_id, "resumeVersionId": resume_id, "applicationEmail": application_email,
                "answers": packet.get("answers").cloned().unwrap_or_else(|| json!({})),
                "verifiedClaimIds": packet.get("verifiedClaimIds").cloned().unwrap_or_else(|| json!([])), "approvedPacketChecksum": checksum},
            "job": job, "documents": [{"kind":"resume", "versionId":resume_id, "storageKey":resume_key, "sha256":pdf_sha, "mediaType":"application/pdf"}],
            "events": [{"id":format!("{run_id}:provider-receipt"), "occurredAt":"2026-11-22T12:00:00Z", "type":"greenhouse_state_transition",
                "detail":{"state":"receipt", "status":"submitted", "capability":"beta_review"}}],
            "result":{"status":"submitted", "submitHttpStatus":302, "confirmationText":"Application received", "confirmationUrl":confirmation_url,
                "submittedAt":"2026-11-22T12:00:00Z", "issues":[]}, "finalUrl":confirmation_url, "screenshotKeys":[screenshot_key]},
        "evidence_objects": [
            {"original_key":resume_key, "kind":"resume", "media_type":"application/pdf", "sha256":pdf_sha, "bytes_base64":base64::engine::general_purpose::STANDARD.encode(pdf)},
            {"original_key":screenshot_key, "kind":"screenshot", "media_type":"image/png", "sha256":png_sha, "bytes_base64":base64::engine::general_purpose::STANDARD.encode(png)}]
    }))
}

async fn post_receipt(
    router: &axum::Router,
    token: &str,
    application_id: &str,
    body: &Value,
) -> Result<axum::response::Response> {
    router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/jobs/internal/applications/{application_id}/receipt"
            ))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(body)?))?,
        )
        .await
        .context("send review-first receipt")
}

async fn candidate_workspace(
    router: &axum::Router,
    token: &str,
    forbidden: &[&str],
) -> Result<Value> {
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/jobs/workspace")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await
        .context("read candidate workspace")?;
    anyhow::ensure!(
        response
            .headers()
            .get("cache-control")
            .is_some_and(|value| value == "private, no-store"),
        "candidate workspace must be private and non-cacheable"
    );
    anyhow::ensure!(
        response.status() == StatusCode::OK,
        "candidate workspace status mismatch"
    );
    let body = to_bytes(response.into_body(), MAX_BODY_BYTES).await?;
    let text = std::str::from_utf8(&body).context("candidate workspace UTF-8")?;
    anyhow::ensure!(
        forbidden.iter().all(|value| !text.contains(value)),
        "candidate workspace exposed worker authority"
    );
    serde_json::from_slice(&body).context("decode candidate workspace")
}

async fn decode_ok(response: axum::response::Response, stage: &str) -> Result<Value> {
    anyhow::ensure!(
        response.status() == StatusCode::OK,
        "{stage} status mismatch"
    );
    serde_json::from_slice(&to_bytes(response.into_body(), MAX_BODY_BYTES).await?)
        .with_context(|| format!("decode {stage}"))
}

fn receipt_pdf() -> Result<Vec<u8>> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for object in [
        b"<< /Type /Catalog /Pages 2 0 R >>".as_slice(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".as_slice(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>".as_slice(),
    ] {
        offsets.push(pdf.len());
        let number = offsets.len();
        pdf.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    anyhow::ensure!(pdf.starts_with(b"%PDF-") && lopdf::Document::load_mem(&pdf).is_ok());
    Ok(pdf)
}

fn receipt_png() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&[0, 0, 0])?;
    writer.finish()?;
    Ok(bytes)
}

#[cfg(test)]
mod durable_tests {
    use super::*;

    #[test]
    fn durable_receipt_request_binds_all_ids_and_sample() {
        let mut draft = DurableMockReceiptRequest {
            version: 1,
            account_id: "11111111-1111-4111-8111-111111111111".into(),
            application_id: "22222222-2222-4222-8222-222222222222".into(),
            job_id: "job-synthetic".into(),
            resume_id: "33333333-3333-4333-8333-333333333333".into(),
            source_asset_id: "44444444-4444-4444-8444-444444444444".into(),
            source_sha256: SAMPLE_SHA256.into(),
            expected_system_identifier: "123456789".into(),
        };
        assert!(draft.valid());
        draft.job_id = "job/unbound".into();
        assert!(!draft.valid());
        draft.job_id = "job-synthetic".into();
        draft.source_sha256 = "0".repeat(64);
        assert!(!draft.valid());
        draft.source_sha256 = SAMPLE_SHA256.into();
        draft.expected_system_identifier = "01".into();
        assert!(!draft.valid());
        draft.expected_system_identifier = "123456789".into();
        let mut input = serde_json::to_value(draft).unwrap();
        input["workerToken"] = json!("refuse");
        assert!(serde_json::from_value::<DurableMockReceiptRequest>(input).is_err());
    }
}

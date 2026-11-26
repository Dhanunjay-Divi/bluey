//! Test-only staging for one owned review-first mock receipt journey.
//!
//! This uses the normal debug database lease APIs to reach a deliberately
//! reconciliable `side_effect_unknown` state. It does not start a worker,
//! browser, dispatcher, network action, or employer submission, and it never
//! creates a terminal receipt itself.

use anyhow::{Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::db::{
    self,
    jobs::{
        self, BrowserSession, FinalSubmitDocumentProof, FinalSubmitFieldProof,
        FinalSubmitFileProof, FinalSubmitJobProof, FinalSubmitPartOrderProof, FinalSubmitProof,
        FinalSubmitTargetProof,
    },
    object_uploads::NewSubmissionEvidenceCapacity,
};
use crate::object_storage::UploadLimits;

#[cfg(test)]
const OWNED_CASE: &str =
    crate::db::owned_postgres_test::VERIFIED_SIGNUP_REVIEW_FIRST_MOCK_RECEIPT_CASE;
const CANDIDATE_EMAIL: &str = "candidate@bluey-qa.test";
const SAMPLE_SOURCE_SHA256: &str =
    "92b41ea87bbc39031cac457edb51ba6693bb9e5773b0e18f5c370fee0e2a1cdb";
const SAMPLE_SOURCE_BYTES: i64 = 368;
const ADAPTER_VERSION: &str = "2026.07.1-beta.1";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

pub(crate) struct MockRun {
    pub(crate) run_id: String,
    pub(crate) lease_token: String,
    pub(crate) fence: i64,
    pub(crate) adapter_version: String,
}

#[cfg(test)]
pub(super) fn stage_mock_run(
    pool: &db::DbPool,
    account_id: &str,
    application_id: &str,
    pdf: &[u8],
) -> Result<MockRun> {
    require_owned_case(pool, account_id, application_id, pdf)?;
    stage_bound_durable_mock_run(pool, account_id, application_id, pdf)
}

// The debug integration broker supplies separate private PostgreSQL custody,
// one-shot and source-authority admission before calling this function. This
// function still binds its mutable work to the exact synthetic candidate,
// canonical IDs and retained source facts; it never accepts a route, worker
// credential or effect flag.
pub(crate) fn stage_bound_durable_mock_run(
    pool: &db::DbPool,
    account_id: &str,
    application_id: &str,
    pdf: &[u8],
) -> Result<MockRun> {
    require_bound_durable_input(pool, account_id, application_id, pdf)?;
    let application = jobs::get_application(pool, account_id, application_id)?
        .context("owned review-first application missing")?;
    require_approved_review_first_application(pool, account_id, application_id, &application, pdf)?;

    let reservation = jobs::reserve_application_attempt(pool, account_id, application_id, "cloud")?;
    anyhow::ensure!(
        reservation.runner == "cloud" && reservation.status == "reserved",
        "owned review-first attempt did not become the cloud reservation"
    );

    let run_id = format!("review-first-mock-{}", uuid::Uuid::new_v4());
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| uuid::Uuid::parse_str(value).is_ok())
        .context("owned review-first application identity")?;
    let browser_profile_id = jobs::execution_browser_profile_id(account_id, identity_id);
    let session = BrowserSession {
        id: run_id.clone(),
        runner: "cloud".to_string(),
        status: "queued".to_string(),
        current_company: "Synthetic Qualification Employer".to_string(),
        current_step: "Synthetic fixture: no browser started".to_string(),
        application_id: Some(application_id.to_string()),
        takeover_url: None,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    jobs::upsert_browser_session(pool, account_id, &session)?;
    jobs::assign_application_run(pool, account_id, application_id, &run_id)?
        .context("owned review-first application disappeared before run assignment")?;
    jobs::update_application(pool, account_id, application_id, "running", None)?
        .context("owned review-first application disappeared before staging")?;
    anyhow::ensure!(
        jobs::update_attempt_reservation_status(pool, account_id, application_id, "running")?,
        "owned review-first attempt did not enter running"
    );

    let lease = jobs::claim_execution_lease(
        pool,
        account_id,
        application_id,
        &run_id,
        &browser_profile_id,
        "review-first-mock-receipt-fixture",
    )?;
    let proof = schema_v3_proof(&application, pdf)?;
    let capacity = capacity(account_id, application_id, &run_id);
    let irreversible = jobs::start_irreversible_submission(
        pool,
        account_id,
        application_id,
        &run_id,
        &lease.lease_token,
        lease.fence,
        &proof,
        &capacity,
    )?;
    anyhow::ensure!(
        irreversible.ats_certified_receipt_authority.is_none(),
        "schema-v3 review-first staging must not mint ATS receipt authority"
    );

    // This is the existing recovery control: a real state transition proves
    // the receipt consumer handles an uncertain effect, rather than claiming
    // a submission happened. The later fixture owns the mock receipt only.
    jobs::update_application(
        pool,
        account_id,
        application_id,
        "side_effect_unknown",
        None,
    )?
    .context("owned review-first application disappeared during reconciliation staging")?;
    anyhow::ensure!(
        jobs::update_attempt_reservation_status(
            pool,
            account_id,
            application_id,
            "side_effect_unknown",
        )?,
        "owned review-first attempt did not enter side-effect-unknown"
    );
    jobs::upsert_browser_session(
        pool,
        account_id,
        &BrowserSession {
            status: "needs_input".to_string(),
            current_step: "Synthetic fixture: receipt reconciliation required".to_string(),
            ..session
        },
    )?;
    jobs::finish_execution_lease(
        pool,
        account_id,
        application_id,
        &run_id,
        &lease.lease_token,
        lease.fence,
        "side_effect_unknown",
    )?;

    Ok(MockRun {
        run_id,
        lease_token: lease.lease_token,
        fence: lease.fence,
        adapter_version: ADAPTER_VERSION.to_string(),
    })
}

fn canonical_uuid_v4(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .is_ok_and(|parsed| parsed.get_version_num() == 4 && parsed.to_string() == value)
}

fn require_bound_durable_input(
    pool: &db::DbPool,
    account_id: &str,
    application_id: &str,
    pdf: &[u8],
) -> Result<()> {
    anyhow::ensure!(
        matches!(pool, db::DbPool::Postgres(_))
            && canonical_uuid_v4(account_id)
            && canonical_uuid_v4(application_id)
            && !pdf.is_empty()
            && pdf.len() <= 12 * 1024 * 1024,
        "review-first durable mock run requires PostgreSQL and canonical owned bindings"
    );
    let account = crate::db::accounts::Account::fetch_by_id(pool, account_id)?
        .context("owned review-first candidate missing")?;
    anyhow::ensure!(
        account.email == CANDIDATE_EMAIL
            && account.email_verified_at.is_some()
            && !account.is_admin
            && !account.is_temporary,
        "review-first durable mock run requires the exact verified synthetic candidate"
    );
    let source = jobs::get_resume_source_asset(pool, account_id)?
        .context("owned review-first source resume missing")?;
    anyhow::ensure!(
        source.file_name == "bluey-qa-resume.txt"
            && source.media_type == "text/plain"
            && source.size_bytes == SAMPLE_SOURCE_BYTES
            && source.sha256 == SAMPLE_SOURCE_SHA256,
        "review-first durable mock run requires the exact 368-byte synthetic source"
    );
    Ok(())
}

#[cfg(test)]
fn require_owned_case(
    pool: &db::DbPool,
    account_id: &str,
    application_id: &str,
    pdf: &[u8],
) -> Result<()> {
    anyhow::ensure!(
        matches!(pool, db::DbPool::Postgres(_))
            && std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref() == Ok("1")
            && std::env::args().skip(1).eq([
                OWNED_CASE,
                "--exact",
                "--nocapture",
                "--test-threads=1"
            ])
            && canonical_uuid_v4(account_id)
            && canonical_uuid_v4(application_id)
            && !pdf.is_empty()
            && pdf.len() <= 12 * 1024 * 1024,
        "review-first mock-run fixture requires the exact owned PostgreSQL case"
    );
    let account = crate::db::accounts::Account::fetch_by_id(pool, account_id)?
        .context("owned review-first candidate missing")?;
    anyhow::ensure!(
        account.email == CANDIDATE_EMAIL
            && account.email_verified_at.is_some()
            && !account.is_admin
            && !account.is_temporary,
        "review-first mock-run fixture requires the verified synthetic candidate"
    );
    Ok(())
}

fn require_approved_review_first_application(
    pool: &db::DbPool,
    account_id: &str,
    application_id: &str,
    application: &jobs::JobApplication,
    pdf: &[u8],
) -> Result<()> {
    let source = jobs::get_resume_source_asset(pool, account_id)?
        .context("owned review-first source resume missing")?;
    anyhow::ensure!(
        application.id == application_id
            && application.state == "queued"
            && application.submission_mode == "review_first"
            && application.run_id.is_none()
            && application.resume_version_id.is_some()
            && !jobs::application_has_frozen_ats_certification(application)
            && source.file_name == "bluey-qa-resume.txt"
            && source.media_type == "text/plain"
            && application
                .receipt
                .pointer("/approved_execution/packet/resumeContent/provenance/career_track_policy_authority/source_resume_asset_id")
                .and_then(serde_json::Value::as_str)
                == Some(source.id.as_str())
            && application
                .receipt
                .pointer("/approved_execution/packet/resumeContent/provenance/career_track_policy_authority/source_resume_sha256")
                .and_then(serde_json::Value::as_str)
                == Some(source.sha256.as_str())
            && application
                .receipt
                .pointer("/approved_execution/job/title")
                .and_then(serde_json::Value::as_str)
                == Some("Data Engineer")
            && application
                .receipt
                .pointer("/approved_execution/job/location")
                .and_then(serde_json::Value::as_str)
                == Some("New York, NY")
            && !pdf.is_empty(),
        "review-first mock-run input is not the exact approved synthetic application"
    );
    Ok(())
}

fn schema_v3_proof(application: &jobs::JobApplication, pdf: &[u8]) -> Result<FinalSubmitProof> {
    let canonical_url = application
        .receipt
        .pointer("/approved_execution/job/canonicalUrl")
        .and_then(serde_json::Value::as_str)
        .context("approved review-first canonical URL")?;
    let resume_version_id = application
        .resume_version_id
        .as_deref()
        .context("approved review-first resume version")?;
    let sha256 = hex::encode(Sha256::digest(pdf));
    let provider_job_key = jobs::final_submit_provider_job_key("greenhouse", canonical_url)
        .map_err(|_| anyhow::anyhow!("approved review-first Greenhouse target"))?;
    Ok(FinalSubmitProof {
        schema_version: 3,
        adapter: "greenhouse".to_string(),
        adapter_version: ADAPTER_VERSION.to_string(),
        control: "greenhouse_submit_application".to_string(),
        job: FinalSubmitJobProof {
            approved_canonical_url: canonical_url.to_string(),
            page_url: canonical_url.to_string(),
        },
        target: FinalSubmitTargetProof {
            action_url: canonical_url.to_string(),
            method: "post".to_string(),
            enctype: "multipart/form-data".to_string(),
            form_target: "_self".to_string(),
            provider_job_key,
            form_identity: r#"[0,"application-form","","","","",""]"#.to_string(),
        },
        files: vec![FinalSubmitFileProof {
            field_name: "resume".to_string(),
            name: format!("resume-{sha256}.pdf"),
            byte_length: i64::try_from(pdf.len()).context("bounded review-first PDF length")?,
            sha256: sha256.clone(),
        }],
        fields: vec![FinalSubmitFieldProof {
            field_name: "candidate_name".to_string(),
            value_byte_length: 0,
            value_sha256: EMPTY_SHA256.to_string(),
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
            version_id: Some(resume_version_id.to_string()),
            sha256,
        }],
        certification: None,
        observed_surface: None,
    })
}

fn capacity(account_id: &str, application_id: &str, run_id: &str) -> NewSubmissionEvidenceCapacity {
    let now_ms = Utc::now().timestamp_millis();
    NewSubmissionEvidenceCapacity {
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        runner: "cloud".to_string(),
        reserved_bytes: 41 * 1024 * 1024,
        reserved_objects: 13,
        expires_at_ms: now_ms.saturating_add(jobs::SUBMISSION_RECONCILIATION_GRACE_MS),
        now_ms,
        limits: UploadLimits {
            max_object_bytes: 1024 * 1024,
            max_account_bytes: 128 * 1024 * 1024,
            max_daily_bytes: 128 * 1024 * 1024,
            max_account_objects: 100,
        },
    }
}

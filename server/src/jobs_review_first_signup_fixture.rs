//! Test-only verified-signup review-first approval fixture.
//!
//! This reaches the normal authenticated Jobs endpoints and uses synthetic
//! runner authority fixtures, but starts no dispatcher, worker process,
//! browser, or employer-facing action.

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::db::{self, jobs};

const MAX_BODY_BYTES: usize = 512 * 1024;
const TRACK_ID: &str = "review-first-data-engineer";

pub(super) async fn review_first_approve(
    pool: &db::DbPool,
    database_url: &str,
    candidate: &super::owned_signup_fixture::SignedUpCandidate,
    source: &jobs::ResumeSourceAsset,
) -> Result<String> {
    let router = crate::api::build_jobs_router(
        pool.clone(),
        super::owned_signup_fixture::base_config(database_url, super::JOURNEY_JWT_SECRET),
    );
    let workspace = get_json(
        &router,
        &candidate.access_token,
        "/api/jobs/workspace",
        "workspace",
    )
    .await?;
    let mut profile = workspace
        .get("profile")
        .cloned()
        .context("verified candidate workspace profile")?;
    let identity_id = workspace
        .get("application_identities")
        .and_then(Value::as_array)
        .and_then(|identities| identities.iter().find_map(|identity| identity.get("id")))
        .and_then(Value::as_str)
        .filter(|value| uuid::Uuid::parse_str(value).is_ok())
        .context("verified candidate workspace identity")?
        .to_string();
    anyhow::ensure!(
        profile["headline"] == "Data Engineer"
            && profile["source_resume_asset_id"] == source.id
            && profile["source_resume_sha256"] == source.sha256,
        "workspace must expose the imported Data Engineer profile and exact source authority"
    );
    profile["current_location"] = json!("New York, NY");
    profile["resume_mode"] = json!("factual");
    profile["default_submission_mode"] = json!("review_first");
    profile["onboarding_complete"] = json!(true);

    let mut preferences = workspace
        .get("preferences")
        .cloned()
        .context("verified candidate workspace preferences")?;
    preferences["desired_roles"] = json!(["Data Engineer"]);
    preferences["desired_locations"] = json!(["New York, NY"]);
    preferences["location_policy"] = json!("willing_to_relocate");
    preferences["remote_preference"] = json!("hybrid_ok");
    preferences["employment_types"] = json!(["full_time"]);
    preferences["sponsorship"] = json!("not_required");

    let track = json!({
        "id": TRACK_ID,
        "name": "Synthetic Data Engineering",
        "role": "Data Engineer",
        "locations": ["New York, NY"],
        "remote_preference": "hybrid_ok",
        "application_identity_id": identity_id,
        "policy": {
            "role_family": "data_engineering",
            "employment_types": ["full_time"],
            "engagement_types": [],
            "work_authorizations": [],
        },
        "active": true,
        "match_count": 0,
        "created_at_ms": 0,
        "updated_at_ms": 0,
    });
    let onboarding = post_json(
        &router,
        &candidate.access_token,
        "/api/jobs/onboarding/complete",
        json!({"profile": profile, "preferences": preferences, "track": track}),
        true,
        "onboarding",
    )
    .await?;
    let approved_track = onboarding
        .get("tracks")
        .and_then(Value::as_array)
        .and_then(|tracks| {
            tracks
                .iter()
                .find(|track| track.get("id") == Some(&json!(TRACK_ID)))
        })
        .context("saved review-first track")?;
    anyhow::ensure!(
        approved_track.pointer("/policy/authority/review_state") == Some(&json!("approved"))
            && approved_track.pointer("/policy/authority/source_resume_asset_id")
                == Some(&json!(source.id))
            && approved_track.pointer("/policy/authority/source_resume_sha256")
                == Some(&json!(source.sha256)),
        "onboarding must preserve exact approved source authority"
    );

    let (bootstrap, _private_key) = db::run_blocking_db(|| {
        jobs::integration_test_runner_control_plane::bootstrap_review_first_mock_approval(
            pool,
            &jobs::integration_test_runner_control_plane::RunnerControlPlaneBootstrapRequest {
                version: 1,
                operation: "bootstrap".to_string(),
                account_id: candidate.account_id.clone(),
                track_id: TRACK_ID.to_string(),
                worker_id: "review-first-approval-worker".to_string(),
            },
            [7_u8; 32],
        )
    })?;
    let prepared = post_json(
        &router,
        &candidate.access_token,
        "/api/jobs/applications",
        json!({"job_id": bootstrap.job_id, "mode": "factual", "submission_mode": "review_first"}),
        false,
        "prepare",
    )
    .await?;
    let application_id = prepared
        .pointer("/application/id")
        .and_then(Value::as_str)
        .filter(|value| uuid::Uuid::parse_str(value).is_ok())
        .context("prepared review-first application")?
        .to_string();
    anyhow::ensure!(
        prepared.pointer("/application/state") == Some(&json!("awaiting_review"))
            && prepared.pointer("/application/submission_mode") == Some(&json!("review_first"))
            && prepared.pointer("/application/job_id") == Some(&json!(bootstrap.job_id))
            && prepared.pointer("/resume_version/mode") == Some(&json!("factual"))
            && prepared.pointer("/resume_version/content/provenance/career_track_policy_authority/source_resume_asset_id")
                == Some(&json!(source.id))
            && prepared.pointer("/resume_version/content/provenance/career_track_policy_authority/source_resume_sha256")
                == Some(&json!(source.sha256)),
        "authenticated preparation must remain a factual review-first draft"
    );
    let before = db::run_blocking_db(|| {
        jobs::get_application(pool, &candidate.account_id, &application_id)
            .context("prepared application before approval")
            .and_then(|application| serde_json::to_value(application).map_err(Into::into))
    })?;
    let approval_path = format!("/api/jobs/applications/{application_id}/approve");
    require_denied(
        &router,
        None,
        &approval_path,
        "unauthenticated review-first approval",
    )
    .await?;
    require_denied(
        &router,
        Some("invalid-fixture-token"),
        &approval_path,
        "invalid-token review-first approval",
    )
    .await?;
    let after_denials = db::run_blocking_db(|| {
        jobs::get_application(pool, &candidate.account_id, &application_id)
            .context("prepared application after denied approval")
            .and_then(|application| serde_json::to_value(application).map_err(Into::into))
    })?;
    anyhow::ensure!(
        before == after_denials,
        "denied approval must not change the draft"
    );

    let approved = post_json(
        &router,
        &candidate.access_token,
        &approval_path,
        json!({}),
        false,
        "approval",
    )
    .await?;
    anyhow::ensure!(
        approved.pointer("/application/id") == Some(&json!(application_id))
            && approved.pointer("/application/state") == Some(&json!("queued"))
            && approved.pointer("/application/submission_mode") == Some(&json!("review_first"))
            && approved.pointer("/application/receipt/approved_execution/packet/applicationId")
                == Some(&json!(application_id))
            && approved.pointer("/application/receipt/approved_execution/packet/resumeContent/provenance/career_track_policy_authority/source_resume_asset_id")
                == Some(&json!(source.id))
            && approved.pointer("/application/receipt/approved_execution/packet/resumeContent/provenance/career_track_policy_authority/source_resume_sha256")
                == Some(&json!(source.sha256)),
        "approved review-first application must be queued with its exact frozen packet"
    );
    db::run_blocking_db(|| {
        let application = jobs::get_application(pool, &candidate.account_id, &application_id)?
            .context("approved review-first application")?;
        anyhow::ensure!(
            application.state == "queued"
                && application.submission_mode == "review_first"
                && application.resume_version_id.is_some()
                && application.receipt.get("submitted_at_ms").is_none(),
            "approval must not create a submitted application"
        );
        let mut connection = pool.get_pg()?;
        let auto_count: i64 = connection
            .query_one(
                "SELECT count(*) FROM jobs_auto_submit_authorizations WHERE account_id=$1",
                &[&candidate.account_id],
            )?
            .get(0);
        let workflow_count: i64 = connection
            .query_one(
                "SELECT count(*) FROM jobs_workflow_commands WHERE account_id=$1",
                &[&candidate.account_id],
            )?
            .get(0);
        let effects = connection.query_one(
            "SELECT
                (SELECT count(*) FROM jobs_execution_leases WHERE account_id=$1),
                (SELECT count(*) FROM jobs_browser_sessions WHERE account_id=$1),
                (SELECT count(*) FROM jobs_local_run_tickets WHERE account_id=$1),
                (SELECT count(*) FROM jobs_application_evidence WHERE account_id=$1),
                (SELECT count(*) FROM jobs_workflow_executions WHERE account_id=$1)",
            &[&candidate.account_id],
        )?;
        let reservation = connection.query_one(
            "SELECT status, runner FROM jobs_attempt_reservations WHERE account_id=$1 AND application_id=$2",
            &[&candidate.account_id, &application_id],
        )?;
        anyhow::ensure!(
            auto_count == 0
                && workflow_count == 0
                && (0..5).all(|column| effects.get::<_, i64>(column) == 0)
                && reservation.get::<_, String>(0) == "reserved"
                && reservation.get::<_, String>(1) == "unassigned",
            "review-first approval must retain one unassigned reservation without any execution or receipt rows"
        );
        Ok(())
    })?;
    Ok(application_id)
}

async fn get_json(router: &axum::Router, token: &str, path: &str, stage: &str) -> Result<Value> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    decode_ok(response, stage).await
}

async fn post_json(
    router: &axum::Router,
    token: &str,
    path: &str,
    body: Value,
    taxonomy_headers: bool,
    stage: &str,
) -> Result<Value> {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json");
    if taxonomy_headers {
        request = request
            .header(
                "x-bluey-jobs-taxonomy-version",
                crate::jobs_taxonomy::taxonomy_version(),
            )
            .header(
                "x-bluey-jobs-taxonomy-sha256",
                crate::jobs_taxonomy::taxonomy_sha256(),
            );
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(serde_json::to_vec(&body)?))?)
        .await?;
    decode_ok(response, stage).await
}

async fn require_denied(
    router: &axum::Router,
    token: Option<&str>,
    path: &str,
    stage: &str,
) -> Result<()> {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from("{}"))?)
        .await?;
    anyhow::ensure!(
        response.status() == StatusCode::UNAUTHORIZED,
        "{stage} must be denied"
    );
    let _ = to_bytes(response.into_body(), MAX_BODY_BYTES).await?;
    Ok(())
}

async fn decode_ok(response: axum::response::Response, stage: &str) -> Result<Value> {
    let status = response.status();
    anyhow::ensure!(
        status == StatusCode::OK,
        "{stage} status mismatch: {status}"
    );
    serde_json::from_slice(&to_bytes(response.into_body(), MAX_BODY_BYTES).await?)
        .with_context(|| format!("decode {stage} response"))
}

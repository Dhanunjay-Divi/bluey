//! Authenticated Bluey Jobs API.
//!
//! The web product shares Bluey identity and balance, while Jobs records,
//! automation state, and packet metering remain isolated under `/api/jobs`.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::{delete, get, patch, post, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use subtle::ConstantTimeEq;

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{
        self, AnswerMemory, ApplicationEvidence, ApplicationIdentity, BrowserSession, CareerFact,
        CareerProfile, CareerTrack, Intervention, JobApplication, JobPosting, JobPreferences,
        JobsEntitlement, JobsIntegration, JobsWorkspace, MailboxConnection, PacketCommitResult,
        ResumeVersion, RunEvent,
    },
};

type ApiError = (StatusCode, String);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs/workspace", get(workspace))
        .route("/api/jobs/profile", get(profile).put(save_profile))
        .route("/api/jobs/facts", get(facts).post(save_fact))
        .route("/api/jobs/facts/:fact_id", delete(delete_fact))
        .route(
            "/api/jobs/preferences",
            get(preferences).put(save_preferences),
        )
        .route("/api/jobs/tracks", get(tracks).post(save_track))
        .route(
            "/api/jobs/tracks/:track_id",
            put(update_track).delete(delete_track),
        )
        .route("/api/jobs/matches", get(matches).post(save_match))
        .route("/api/jobs/matches/:job_id", get(match_detail))
        .route(
            "/api/jobs/applications",
            get(applications).post(prepare_application),
        )
        .route(
            "/api/jobs/applications/:application_id",
            patch(update_application),
        )
        .route(
            "/api/jobs/applications/:application_id/commit",
            post(commit_application_packet),
        )
        .route(
            "/api/jobs/applications/:application_id/runs",
            post(queue_application_run),
        )
        .route(
            "/api/jobs/applications/:application_id/evidence",
            get(application_evidence).post(save_application_evidence),
        )
        .route(
            "/api/jobs/resume-versions/:resume_version_id",
            get(resume_version),
        )
        .route(
            "/api/jobs/browser-sessions",
            get(browser_sessions).post(save_browser_session),
        )
        .route(
            "/api/jobs/interventions",
            get(interventions).post(save_intervention),
        )
        .route(
            "/api/jobs/interventions/:intervention_id",
            patch(update_intervention),
        )
        .route(
            "/api/jobs/answers",
            get(answer_memory).post(save_answer_memory),
        )
        .route(
            "/api/jobs/answers/:answer_id",
            put(update_answer_memory).delete(delete_answer_memory),
        )
        .route(
            "/api/jobs/integrations",
            get(integrations).put(save_integration),
        )
        .route(
            "/api/jobs/application-identities",
            get(application_identities).post(create_application_identity),
        )
        .route(
            "/api/jobs/application-identities/:identity_id",
            put(update_application_identity).delete(remove_application_identity),
        )
        .route(
            "/api/jobs/application-identities/:identity_id/verify",
            post(verify_application_identity),
        )
        .route(
            "/api/jobs/application-identities/:identity_id/resend",
            post(resend_application_identity),
        )
        .route(
            "/api/jobs/mailbox-connections",
            get(mailbox_connections).post(request_mailbox_connection),
        )
        .route(
            "/api/jobs/mailbox-connections/:connection_id",
            delete(remove_mailbox_connection),
        )
        .route("/api/jobs/entitlements", get(entitlements))
        .route("/api/jobs/runs/:run_id/events", get(run_events))
        .route_layer(axum::middleware::from_fn(require_jobs_beta))
}

pub fn worker_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/runs/:run_id/events",
            post(worker_run_event),
        )
        .route(
            "/api/jobs/internal/applications/:application_id/state",
            post(worker_application_state),
        )
        .route(
            "/api/jobs/internal/applications/:application_id/interventions",
            post(worker_intervention),
        )
        .route(
            "/api/jobs/internal/applications/:application_id/receipt",
            post(worker_receipt),
        )
        .route_layer(axum::middleware::from_fn(require_jobs_worker))
}

pub fn local_runner_router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs/local-runs/:run_id/claim", post(claim_local_run))
        .route(
            "/api/jobs/local-runs/:run_id/result",
            post(save_local_run_result),
        )
        .route_layer(axum::middleware::from_fn(require_jobs_beta))
}

pub fn admin_router() -> Router<AppState> {
    Router::new().route(
        "/admin/jobs/entitlements/:account_id",
        patch(set_account_entitlement),
    )
}

async fn require_jobs_beta(request: Request<Body>, next: Next) -> Result<Response, ApiError> {
    let enabled = cfg!(debug_assertions)
        || std::env::var("BLUEY_JOBS_BETA_ENABLED")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
            .unwrap_or(false);
    if !enabled {
        return Err((
            StatusCode::NOT_FOUND,
            "Bluey Jobs beta is not enabled.".to_string(),
        ));
    }
    Ok(next.run(request).await)
}

async fn require_jobs_worker(request: Request<Body>, next: Next) -> Result<Response, ApiError> {
    let expected = std::env::var("BLUEY_JOBS_WORKER_TOKEN").unwrap_or_default();
    let supplied = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if expected.is_empty()
        || supplied.as_bytes().len() != expected.as_bytes().len()
        || supplied.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() != 1
    {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
    Ok(next.run(request).await)
}

pub async fn workspace(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<JobsWorkspace>, ApiError> {
    jobs::workspace(&state.pool, &account.id, &account.email)
        .map(Json)
        .map_err(internal)
}

pub async fn profile(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<CareerProfile>, ApiError> {
    jobs::get_profile(&state.pool, &account.id, &account.email)
        .map(Json)
        .map_err(internal)
}

pub async fn save_profile(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(mut profile): Json<CareerProfile>,
) -> Result<Json<CareerProfile>, ApiError> {
    profile.email = account.email.clone();
    validate_profile(&profile)?;
    jobs::save_profile(&state.pool, &account.id, &profile)
        .map(Json)
        .map_err(internal)
}

pub async fn facts(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<CareerFact>>, ApiError> {
    jobs::list_facts(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_fact(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(fact): Json<CareerFact>,
) -> Result<Json<CareerFact>, ApiError> {
    if fact.category.trim().is_empty() || fact.label.trim().is_empty() {
        return bad_request("Choose a category and label for this fact.");
    }
    if !matches!(
        fact.verification_status.as_str(),
        "unverified" | "needs_confirmation" | "confirmed" | "rejected"
    ) {
        return bad_request("Choose a valid confirmation status.");
    }
    jobs::upsert_fact(&state.pool, &account.id, &fact)
        .map(Json)
        .map_err(internal)
}

pub async fn delete_fact(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(fact_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if jobs::delete_fact(&state.pool, &account.id, &fact_id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Career fact not found.".to_string()))
    }
}

pub async fn preferences(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<JobPreferences>, ApiError> {
    jobs::get_preferences(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_preferences(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(preferences): Json<JobPreferences>,
) -> Result<Json<JobPreferences>, ApiError> {
    validate_preferences(&preferences)?;
    jobs::save_preferences(&state.pool, &account.id, &preferences)
        .map(Json)
        .map_err(internal)
}

pub async fn tracks(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<CareerTrack>>, ApiError> {
    jobs::list_tracks(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_track(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(track): Json<CareerTrack>,
) -> Result<Json<CareerTrack>, ApiError> {
    validate_track(&track)?;
    let entitlement = jobs::get_entitlement(&state.pool, &account.id).map_err(internal)?;
    let current = jobs::list_tracks(&state.pool, &account.id).map_err(internal)?;
    if track.id.is_empty()
        && current.iter().filter(|item| item.active).count() as i64 >= entitlement.track_limit
    {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "Your {} plan includes {} Career Track Agent(s).",
                entitlement.plan, entitlement.track_limit
            ),
        ));
    }
    jobs::upsert_track(&state.pool, &account.id, &track)
        .map(Json)
        .map_err(internal)
}

pub async fn update_track(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(track_id): Path<String>,
    Json(mut track): Json<CareerTrack>,
) -> Result<Json<CareerTrack>, ApiError> {
    track.id = track_id;
    validate_track(&track)?;
    jobs::upsert_track(&state.pool, &account.id, &track)
        .map(Json)
        .map_err(internal)
}

pub async fn delete_track(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(track_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if jobs::delete_track(&state.pool, &account.id, &track_id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Career Track not found.".to_string()))
    }
}

pub async fn matches(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<JobPosting>>, ApiError> {
    jobs::list_postings(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn match_detail(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(job_id): Path<String>,
) -> Result<Json<JobPosting>, ApiError> {
    jobs::get_posting(&state.pool, &account.id, &job_id)
        .map_err(internal)?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Job match not found.".to_string()))
}

pub async fn save_match(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(mut posting): Json<JobPosting>,
) -> Result<Json<JobPosting>, ApiError> {
    validate_posting(&posting)?;
    apply_submission_boundary(&mut posting);
    let profile = jobs::get_profile(&state.pool, &account.id, &account.email).map_err(internal)?;
    let preferences = jobs::get_preferences(&state.pool, &account.id).map_err(internal)?;
    jobs::upsert_posting(&state.pool, &account.id, &posting, &profile, &preferences)
        .map(Json)
        .map_err(internal)
}

pub async fn applications(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<JobApplication>>, ApiError> {
    jobs::list_applications(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

#[derive(Debug, Deserialize)]
pub struct PrepareApplicationRequest {
    pub job_id: String,
    #[serde(default = "default_factual")]
    pub mode: String,
    #[serde(default = "default_review_first")]
    pub submission_mode: String,
}

#[derive(Debug, Serialize)]
pub struct PrepareApplicationResponse {
    pub application: JobApplication,
    pub resume_version: ResumeVersion,
}

pub async fn prepare_application(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<PrepareApplicationRequest>,
) -> Result<Json<PrepareApplicationResponse>, ApiError> {
    if !matches!(req.mode.as_str(), "factual" | "enhance") {
        return bad_request("Choose Factual or Enhance.");
    }
    if !matches!(req.submission_mode.as_str(), "review_first" | "auto_submit") {
        return bad_request("Choose Review first or Auto-submit.");
    }
    let profile = jobs::get_profile(&state.pool, &account.id, &account.email).map_err(internal)?;
    if !profile.onboarding_complete {
        return Err((
            StatusCode::CONFLICT,
            "Finish your Career Profile before preparing applications.".to_string(),
        ));
    }
    let (mut application, resume_version) = jobs::prepare_application(
        &state.pool,
        &account.id,
        &req.job_id,
        &req.mode,
        &req.submission_mode,
    )
    .map_err(internal)?;
    if application.state == "queued" {
        if let Err(error) = jobs::commit_packet(&state.pool, &account.id, &application.id) {
            let _ = jobs::update_application(
                &state.pool,
                &account.id,
                &application.id,
                "awaiting_review",
                Some(&application.submission_mode),
            );
            application.state = "awaiting_review".to_string();
            if error.to_string().contains("insufficient Bluey balance") {
                return Err((
                    StatusCode::PAYMENT_REQUIRED,
                    "Add to your Bluey balance before Auto-submit can queue this packet."
                        .to_string(),
                ));
            }
            return Err(internal(error));
        }
    }
    Ok(Json(PrepareApplicationResponse {
        application,
        resume_version,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateApplicationRequest {
    pub state: String,
    #[serde(default)]
    pub submission_mode: Option<String>,
}

pub async fn update_application(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
    Json(req): Json<UpdateApplicationRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    jobs::update_application(
        &state.pool,
        &account.id,
        &application_id,
        &req.state,
        req.submission_mode.as_deref(),
    )
    .map_err(|error| validation_or_internal(error, "Choose a valid application state."))?
    .map(Json)
    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))
}

pub async fn application_evidence(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
) -> Result<Json<Vec<ApplicationEvidence>>, ApiError> {
    if jobs::get_application(&state.pool, &account.id, &application_id)
        .map_err(internal)?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, "Application not found.".to_string()));
    }
    jobs::list_application_evidence(&state.pool, &account.id, Some(&application_id))
        .map(Json)
        .map_err(internal)
}

pub async fn save_application_evidence(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
    Json(mut evidence): Json<ApplicationEvidence>,
) -> Result<Json<ApplicationEvidence>, ApiError> {
    evidence.id.clear();
    evidence.application_id = application_id;
    evidence.kind = evidence.kind.trim().to_ascii_lowercase();
    evidence.provider = evidence.provider.trim().to_ascii_lowercase();
    evidence.created_at_ms = 0;
    jobs::save_application_evidence(&state.pool, &account.id, &evidence)
        .map(Json)
        .map_err(domain_error)
}

pub async fn commit_application_packet(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
) -> Result<Json<PacketCommitResult>, ApiError> {
    jobs::commit_packet(&state.pool, &account.id, &application_id)
        .map(Json)
        .map_err(|error| {
            if error.to_string().contains("insufficient Bluey balance") {
                (
                    StatusCode::PAYMENT_REQUIRED,
                    "Add to your Bluey balance to complete this application packet.".to_string(),
                )
            } else {
                internal(error)
            }
        })
}

#[derive(Debug, Deserialize)]
pub struct QueueApplicationRunRequest {
    #[serde(default = "default_cloud_runner")]
    pub runner: String,
}

#[derive(Debug, Serialize)]
pub struct QueueApplicationRunResponse {
    pub application: JobApplication,
    pub browser_session: BrowserSession,
    pub workflow_id: String,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_url: Option<String>,
}

pub async fn queue_application_run(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
    Json(req): Json<QueueApplicationRunRequest>,
) -> Result<Json<QueueApplicationRunResponse>, ApiError> {
    if !matches!(req.runner.as_str(), "local" | "cloud") {
        return bad_request("Choose the local or cloud runner.");
    }
    let entitlement = jobs::get_entitlement(&state.pool, &account.id).map_err(internal)?;
    if (req.runner == "local" && !entitlement.local_browser)
        || (req.runner == "cloud" && !entitlement.cloud_browser)
    {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "That browser runner is not included in your Jobs plan.".to_string(),
        ));
    }
    let mut application = jobs::get_application(&state.pool, &account.id, &application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    let posting = jobs::get_posting(&state.pool, &account.id, &application.job_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
    let resume_id = application.resume_version_id.clone().ok_or((
        StatusCode::CONFLICT,
        "Create the tailored resume before starting this application.".to_string(),
    ))?;
    let resume = jobs::get_resume_version(&state.pool, &account.id, &resume_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::CONFLICT,
            "Tailored resume not found.".to_string(),
        ))?;
    let identity = application
        .receipt
        .pointer("/application_identity")
        .and_then(Value::as_object)
        .ok_or((
            StatusCode::CONFLICT,
            "Choose and verify the application email before starting.".to_string(),
        ))?;
    let identity_id = identity
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let identity_email = identity
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if identity_id.is_empty() || identity_email.is_empty() {
        return Err((
            StatusCode::CONFLICT,
            "Choose and verify the application email before starting.".to_string(),
        ));
    }
    let existing_run = if matches!(
        application.state.as_str(),
        "queued" | "running" | "needs_input"
    ) {
        if let Some(run_id) = application.run_id.clone() {
            jobs::list_browser_sessions(&state.pool, &account.id)
                .map_err(internal)?
                .into_iter()
                .find(|session| session.id == run_id)
                .map(|session| (run_id, session))
        } else {
            None
        }
    } else {
        None
    };
    if existing_run
        .as_ref()
        .is_some_and(|(_, session)| session.runner != req.runner)
    {
        return Err((
            StatusCode::CONFLICT,
            "This application is already active in another runner.".to_string(),
        ));
    }
    let source = ats_kind(&posting.canonical_url);
    if req.runner == "cloud" && source == "semantic" && posting.source.ends_with("_handoff") {
        return Err((
            StatusCode::CONFLICT,
            "This listing needs a reviewed browser handoff.".to_string(),
        ));
    }
    let run_id = existing_run
        .as_ref()
        .map(|(run_id, _)| run_id.clone())
        .unwrap_or_else(|| {
            application_run_id(
                &account.id,
                &application.id,
                &resume.id,
                application.updated_at_ms,
            )
        });
    jobs::commit_packet(&state.pool, &account.id, &application.id).map_err(domain_error)?;
    if application.state != "queued" {
        application = jobs::update_application(
            &state.pool,
            &account.id,
            &application.id,
            "queued",
            Some(&application.submission_mode),
        )
        .map_err(domain_error)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    }

    let profile = jobs::get_profile(&state.pool, &account.id, &account.email).map_err(internal)?;
    let browser_profile_id = hex::encode(Sha256::digest(
        format!("{}\0{}", account.id, identity_id).as_bytes(),
    ));
    let answers = execution_answers(&profile, &application, &identity_email);
    let workflow_input = json!({
        "accountId": account.id,
        "applicationId": application.id,
        "jobId": posting.id,
        "canonicalJobKey": posting.canonical_key,
        "packetId": resume.id,
        "applicationIdentityId": identity_id,
        "browserProfileId": browser_profile_id,
        "packet": {
            "applicationId": application.id,
            "jobId": posting.id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": application.cover_letter,
            "answers": answers,
            "verifiedClaimIds": resume.claim_ids,
            "applicationIdentityId": identity_id,
            "applicationEmail": identity_email,
            "browserProfileId": browser_profile_id,
        },
        "job": {
            "externalId": posting.external_id,
            "canonicalUrl": posting.canonical_url,
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
            "workplace": normalized_workplace(&posting.workplace),
            "description": posting.description,
            "source": source,
            "compensation": posting.compensation,
        },
        "runner": req.runner,
        "url": posting.canonical_url,
        "idempotencyKey": run_id,
        "runId": run_id,
        "browserSessionId": format!("{}-{}", req.runner, application.id),
    });
    let mut browser_session =
        existing_run
            .map(|(_, session)| session)
            .unwrap_or_else(|| BrowserSession {
                id: run_id.clone(),
                runner: req.runner.clone(),
                status: "queued".to_string(),
                current_company: posting.company.clone(),
                current_step: "Waiting for a browser".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            });
    browser_session = jobs::upsert_browser_session(&state.pool, &account.id, &browser_session)
        .map_err(internal)?;
    application = jobs::assign_application_run(&state.pool, &account.id, &application.id, &run_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;

    if req.runner == "local" {
        let ticket = if let Some(existing) =
            jobs::get_local_run_ticket(&state.pool, &account.id, &run_id).map_err(internal)?
        {
            existing
        } else {
            let secret = random_local_run_ticket();
            let hash = hex::encode(Sha256::digest(secret.as_bytes()));
            jobs::save_local_run_ticket(
                &state.pool,
                &account.id,
                &application.id,
                &run_id,
                &hash,
                &secret,
                workflow_input,
                jobs::now_ms() + 24 * 60 * 60 * 1_000,
            )
            .map_err(internal)?
        };
        return Ok(Json(QueueApplicationRunResponse {
            application,
            browser_session,
            workflow_id: String::new(),
            run_id: run_id.clone(),
            launch_url: Some(format!(
                "bluey-jobs://run/{run_id}?ticket={}",
                ticket.ticket_secret
            )),
        }));
    }

    let gateway_origin = std::env::var("BLUEY_JOBS_WORKFLOW_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
    let gateway_token = std::env::var("BLUEY_JOBS_WORKFLOW_TOKEN").unwrap_or_default();
    if gateway_token.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "The cloud runner is temporarily unavailable.".to_string(),
        ));
    }
    let response = reqwest::Client::new()
        .post(format!(
            "{}/workflows/applications",
            gateway_origin.trim_end_matches('/')
        ))
        .bearer_auth(gateway_token)
        .json(&workflow_input)
        .send()
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "Bluey Jobs workflow gateway failed");
            (
                StatusCode::BAD_GATEWAY,
                "Bluey could not start the cloud application. Try again.".to_string(),
            )
        })?;
    if !response.status().is_success() && response.status() != StatusCode::CONFLICT {
        tracing::error!(status = %response.status(), "Bluey Jobs workflow gateway rejected run");
        return Err((
            StatusCode::BAD_GATEWAY,
            "Bluey could not start the cloud application. Try again.".to_string(),
        ));
    }
    let workflow_id = format!("bluey-jobs:{}:{}", account.id, run_id);
    Ok(Json(QueueApplicationRunResponse {
        application,
        browser_session,
        workflow_id,
        run_id,
        launch_url: None,
    }))
}

fn application_run_id(
    account_id: &str,
    application_id: &str,
    resume_version_id: &str,
    application_updated_at_ms: i64,
) -> String {
    let input = format!(
        "{}\0{}\0{}\0{}",
        account_id, application_id, resume_version_id, application_updated_at_ms
    );
    hex::encode(Sha256::digest(input.as_bytes()))[..40].to_string()
}

fn random_local_run_ticket() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS random source");
    hex::encode(bytes)
}

pub async fn resume_version(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(resume_version_id): Path<String>,
) -> Result<Json<ResumeVersion>, ApiError> {
    jobs::get_resume_version(&state.pool, &account.id, &resume_version_id)
        .map_err(internal)?
        .map(Json)
        .ok_or((
            StatusCode::NOT_FOUND,
            "Resume version not found.".to_string(),
        ))
}

pub async fn browser_sessions(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<BrowserSession>>, ApiError> {
    jobs::list_browser_sessions(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_browser_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(session): Json<BrowserSession>,
) -> Result<Json<BrowserSession>, ApiError> {
    if !matches!(session.runner.as_str(), "local" | "cloud") {
        return bad_request("Choose the local or cloud browser.");
    }
    if !matches!(
        session.status.as_str(),
        "queued" | "running" | "needs_input" | "paused" | "complete" | "failed"
    ) {
        return bad_request("Choose a valid browser run status.");
    }
    if let Some(application_id) = session.application_id.as_deref() {
        if jobs::get_application(&state.pool, &account.id, application_id)
            .map_err(internal)?
            .is_none()
        {
            return Err((StatusCode::NOT_FOUND, "Application not found.".to_string()));
        }
    }
    if let Some(url) = session.takeover_url.as_deref() {
        if !(url.starts_with("https://") || url.starts_with("bluey-jobs://")) {
            return bad_request("Use a secure browser takeover link.");
        }
    }
    let entitlement = jobs::get_entitlement(&state.pool, &account.id).map_err(internal)?;
    if (session.runner == "local" && !entitlement.local_browser)
        || (session.runner == "cloud" && !entitlement.cloud_browser)
    {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "This browser runner is not included in your Jobs plan.".to_string(),
        ));
    }
    jobs::upsert_browser_session(&state.pool, &account.id, &session)
        .map(Json)
        .map_err(internal)
}

pub async fn interventions(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<Intervention>>, ApiError> {
    jobs::list_interventions(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_intervention(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(intervention): Json<Intervention>,
) -> Result<Json<Intervention>, ApiError> {
    jobs::save_intervention(&state.pool, &account.id, &intervention)
        .map(Json)
        .map_err(internal)
}

#[derive(Debug, Deserialize)]
pub struct ResolveInterventionRequest {
    pub status: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub remember: bool,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub scope_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InterventionResolutionResult {
    pub intervention: Intervention,
    pub answer_memory: Option<AnswerMemory>,
    pub application: Option<JobApplication>,
}

pub async fn update_intervention(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(intervention_id): Path<String>,
    Json(req): Json<ResolveInterventionRequest>,
) -> Result<Json<InterventionResolutionResult>, ApiError> {
    let intervention = jobs::list_interventions(&state.pool, &account.id)
        .map_err(internal)?
        .into_iter()
        .find(|item| item.id == intervention_id)
        .ok_or((StatusCode::NOT_FOUND, "Intervention not found.".to_string()))?;
    let mut updated = intervention;
    let action = req.action.trim().to_ascii_lowercase();
    let mut remembered_answer = None;
    updated.status = req.status.trim().to_ascii_lowercase();
    if action == "approve_email_otp" {
        if updated.resolution_kind != "email_otp_approval" {
            return bad_request("This intervention does not contain an email verification step.");
        }
        if updated
            .expires_at_ms
            .is_some_and(|expires_at| expires_at <= jobs::now_ms())
        {
            updated.status = "expired".to_string();
        } else {
            updated.status = "approved".to_string();
            let approved_at_ms = jobs::now_ms();
            if let Some(metadata) = updated.metadata.as_object_mut() {
                metadata.insert(
                    "approved_at_ms".to_string(),
                    serde_json::json!(approved_at_ms),
                );
            } else {
                updated.metadata = serde_json::json!({ "approved_at_ms": approved_at_ms });
            }
        }
    } else if action == "answer" {
        if updated.resolution_kind != "answer"
            || !matches!(
                updated.kind.as_str(),
                "unknown_question" | "missing_fact" | "sensitive_question"
            )
        {
            return bad_request("This intervention is not waiting for an application answer.");
        }
        let answer = req.answer.trim();
        if answer.is_empty() {
            return bad_request("Enter the answer Bluey should use.");
        }
        if answer.len() > 10_000 {
            return bad_request("That answer is too long.");
        }
        let answered_at_ms = jobs::now_ms();
        let metadata = updated.metadata.as_object_mut().ok_or((
            StatusCode::BAD_REQUEST,
            "This intervention cannot be answered.".to_string(),
        ))?;
        metadata.insert("resolved_answer".to_string(), serde_json::json!(answer));
        metadata.insert(
            "answered_at_ms".to_string(),
            serde_json::json!(answered_at_ms),
        );
        updated.status = "resolved".to_string();
        if req.remember {
            let question = updated
                .metadata
                .pointer("/receipt/intervention/field")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&updated.title);
            let memory = AnswerMemory {
                id: String::new(),
                key: String::new(),
                question: question.to_string(),
                value: answer.to_string(),
                scope: if req.scope.trim().is_empty() {
                    "account".to_string()
                } else {
                    req.scope.clone()
                },
                scope_id: req.scope_id.clone(),
                confirmed: true,
                source: "intervention".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                last_used_at_ms: None,
                use_count: 0,
            };
            remembered_answer = Some(
                jobs::save_answer_memory(&state.pool, &account.id, &memory)
                    .map_err(domain_error)?,
            );
        }
    }
    let resumed_application = if !action.is_empty()
        && updated.resume_after_resolution
        && matches!(updated.status.as_str(), "approved" | "resolved")
    {
        if let Some(application_id) = updated.application_id.as_deref() {
            jobs::update_application(&state.pool, &account.id, application_id, "queued", None)
                .map_err(domain_error)?
        } else {
            None
        }
    } else {
        None
    };
    let mut saved =
        jobs::save_intervention(&state.pool, &account.id, &updated).map_err(internal)?;
    if let Some(application) = resumed_application.as_ref() {
        if let Some(run_id) = application.run_id.as_deref() {
            let field = saved
                .metadata
                .pointer("/receipt/intervention/field")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Err(error) =
                signal_workflow_resume(&account.id, run_id, &action, field, req.answer.trim()).await
            {
                tracing::error!(error = %error.1, "Bluey Jobs workflow resume failed");
                saved.status = "open".to_string();
                jobs::save_intervention(&state.pool, &account.id, &saved).map_err(internal)?;
                let _ = jobs::update_application(
                    &state.pool,
                    &account.id,
                    &application.id,
                    "needs_input",
                    None,
                );
                return Err(error);
            }
        }
    }
    Ok(Json(InterventionResolutionResult {
        intervention: saved,
        answer_memory: remembered_answer,
        application: resumed_application,
    }))
}

async fn signal_workflow_resume(
    account_id: &str,
    run_id: &str,
    action: &str,
    field: &str,
    answer: &str,
) -> Result<(), ApiError> {
    let origin = std::env::var("BLUEY_JOBS_WORKFLOW_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
    let token = std::env::var("BLUEY_JOBS_WORKFLOW_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "The application runner is temporarily unavailable.".to_string(),
        ));
    }
    let response = reqwest::Client::new()
        .post(format!(
            "{}/workflows/applications/{}/{}/resume",
            origin.trim_end_matches('/'),
            account_id,
            run_id
        ))
        .bearer_auth(token)
        .json(&json!({ "action": action, "field": field, "answer": answer }))
        .send()
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_GATEWAY,
                "Bluey could not resume the application. Try again.".to_string(),
            )
        })?;
    if !response.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "Bluey could not resume the application. Try again.".to_string(),
        ));
    }
    Ok(())
}

pub async fn answer_memory(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<AnswerMemory>>, ApiError> {
    jobs::list_answer_memory(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_answer_memory(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(answer): Json<AnswerMemory>,
) -> Result<Json<AnswerMemory>, ApiError> {
    jobs::save_answer_memory(&state.pool, &account.id, &answer)
        .map(Json)
        .map_err(domain_error)
}

pub async fn update_answer_memory(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(answer_id): Path<String>,
    Json(mut answer): Json<AnswerMemory>,
) -> Result<Json<AnswerMemory>, ApiError> {
    if !jobs::list_answer_memory(&state.pool, &account.id)
        .map_err(internal)?
        .iter()
        .any(|item| item.id == answer_id)
    {
        return Err((StatusCode::NOT_FOUND, "Saved answer not found.".to_string()));
    }
    answer.id = answer_id;
    jobs::save_answer_memory(&state.pool, &account.id, &answer)
        .map(Json)
        .map_err(domain_error)
}

pub async fn delete_answer_memory(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(answer_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if jobs::delete_answer_memory(&state.pool, &account.id, &answer_id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Saved answer not found.".to_string()))
    }
}

pub async fn integrations(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<JobsIntegration>>, ApiError> {
    jobs::list_integrations(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_integration(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(integration): Json<JobsIntegration>,
) -> Result<Json<JobsIntegration>, ApiError> {
    if !matches!(
        integration.provider.as_str(),
        "google_calendar" | "outlook_calendar"
    ) {
        return bad_request("Choose Google Calendar or Outlook Calendar.");
    }
    if integration.status != "disconnected" {
        return bad_request("Complete provider authorization before connecting this account.");
    }
    jobs::save_integration(&state.pool, &account.id, &integration)
        .map(Json)
        .map_err(internal)
}

const IDENTITY_OTP_TTL_MS: i64 = 10 * 60 * 1_000;

pub async fn application_identities(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<ApplicationIdentity>>, ApiError> {
    let _ = jobs::ensure_primary_application_identity(&state.pool, &account.id, &account.email)
        .map_err(domain_error)?;
    jobs::list_application_identities(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn create_application_identity(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(mut identity): Json<ApplicationIdentity>,
) -> Result<Json<ApplicationIdentity>, ApiError> {
    identity.id.clear();
    identity.email = jobs::normalize_application_email(&identity.email).map_err(domain_error)?;
    identity.label = identity.label.trim().chars().take(60).collect();
    identity.verification_status = "pending".to_string();
    identity.is_default = false;
    identity.created_at_ms = 0;
    identity.updated_at_ms = 0;
    let saved = jobs::save_application_identity(&state.pool, &account.id, &identity)
        .map_err(domain_error)?;
    if saved.verification_status != "verified" {
        send_identity_verification(&state, &account.id, &saved).await?;
    }
    Ok(Json(saved))
}

pub async fn update_application_identity(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(identity_id): Path<String>,
    Json(requested): Json<ApplicationIdentity>,
) -> Result<Json<ApplicationIdentity>, ApiError> {
    let mut existing = jobs::get_application_identity(&state.pool, &account.id, &identity_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Application email not found.".to_string(),
        ))?;
    if jobs::normalize_application_email(&requested.email).map_err(domain_error)? != existing.email
    {
        return bad_request("Add a new application email instead of changing this address.");
    }
    existing.label = requested.label.trim().chars().take(60).collect();
    existing.is_default = requested.is_default;
    jobs::save_application_identity(&state.pool, &account.id, &existing)
        .map(Json)
        .map_err(domain_error)
}

#[derive(Debug, Deserialize)]
pub struct VerifyIdentityRequest {
    pub code: String,
}

pub async fn verify_application_identity(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(identity_id): Path<String>,
    Json(req): Json<VerifyIdentityRequest>,
) -> Result<Json<ApplicationIdentity>, ApiError> {
    if req.code.len() != 6 || !req.code.bytes().all(|byte| byte.is_ascii_digit()) {
        return bad_request("Enter the 6-digit verification code.");
    }
    jobs::verify_application_identity(&state.pool, &account.id, &identity_id, &req.code)
        .map(Json)
        .map_err(domain_error)
}

pub async fn resend_application_identity(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(identity_id): Path<String>,
) -> Result<Json<ApplicationIdentity>, ApiError> {
    let identity = jobs::get_application_identity(&state.pool, &account.id, &identity_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Application email not found.".to_string(),
        ))?;
    send_identity_verification(&state, &account.id, &identity).await?;
    Ok(Json(identity))
}

async fn send_identity_verification(
    state: &AppState,
    account_id: &str,
    identity: &ApplicationIdentity,
) -> Result<(), ApiError> {
    let code = random_six_digit_code();
    jobs::save_identity_verification(
        &state.pool,
        account_id,
        &identity.id,
        &code,
        IDENTITY_OTP_TTL_MS,
    )
    .map_err(domain_error)?;
    match crate::mail::send_jobs_identity_otp(&state.config, &identity.email, &code, 10).await {
        Ok(crate::mail::MailDelivery::Sent) => Ok(()),
        Ok(crate::mail::MailDelivery::NotConfigured) => {
            let _ = jobs::delete_identity_verification(&state.pool, account_id, &identity.id);
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Application email verification is temporarily unavailable.".to_string(),
            ))
        }
        Err(error) => {
            let _ = jobs::delete_identity_verification(&state.pool, account_id, &identity.id);
            tracing::warn!(error = %error, "Bluey Jobs application email delivery failed");
            Err((
                StatusCode::BAD_GATEWAY,
                "Bluey could not send the verification email. Try again.".to_string(),
            ))
        }
    }
}

fn random_six_digit_code() -> String {
    const CODE_SPACE: u32 = 1_000_000;
    let unbiased_zone = u32::MAX - (u32::MAX % CODE_SPACE);
    loop {
        let mut bytes = [0u8; 4];
        getrandom::getrandom(&mut bytes).expect("OS random source");
        let value = u32::from_le_bytes(bytes);
        if value < unbiased_zone {
            return format!("{:06}", value % CODE_SPACE);
        }
    }
}

pub async fn remove_application_identity(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(identity_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !jobs::delete_application_identity(&state.pool, &account.id, &identity_id)
        .map_err(domain_error)?
    {
        return Err((
            StatusCode::NOT_FOUND,
            "Application email not found.".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn mailbox_connections(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<MailboxConnection>>, ApiError> {
    jobs::list_mailbox_connections(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn request_mailbox_connection(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(mut connection): Json<MailboxConnection>,
) -> Result<Json<MailboxConnection>, ApiError> {
    connection.id.clear();
    connection.status = "pending".to_string();
    connection.created_at_ms = 0;
    connection.updated_at_ms = 0;
    let provider_subject = connection.account_label.clone();
    jobs::save_mailbox_connection(&state.pool, &account.id, &connection, &provider_subject)
        .map(Json)
        .map_err(domain_error)
}

pub async fn remove_mailbox_connection(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(connection_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !jobs::delete_mailbox_connection(&state.pool, &account.id, &connection_id)
        .map_err(internal)?
    {
        return Err((
            StatusCode::NOT_FOUND,
            "Connected inbox not found.".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn entitlements(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<JobsEntitlement>, ApiError> {
    jobs::get_entitlement(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

#[derive(Debug, Deserialize)]
pub struct SetEntitlementRequest {
    pub plan: String,
}

pub async fn set_account_entitlement(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    Json(req): Json<SetEntitlementRequest>,
) -> Result<Json<JobsEntitlement>, ApiError> {
    if !matches!(req.plan.as_str(), "free" | "pro" | "cloud") {
        return bad_request("Choose the Free, Pro, or Cloud Jobs plan.");
    }
    jobs::set_entitlement_plan(&state.pool, &account_id, &req.plan)
        .map(Json)
        .map_err(internal)
}

pub async fn run_events(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<RunEvent>>, ApiError> {
    jobs::list_run_events(&state.pool, &account.id, &run_id)
        .map(Json)
        .map_err(internal)
}

#[derive(Debug, Deserialize)]
struct LocalRunAccessRequest {
    ticket: String,
}

#[derive(Debug, Deserialize)]
struct LocalRunResultRequest {
    ticket: String,
    receipt: Value,
    #[serde(default, rename = "receiptBundle")]
    receipt_bundle: Option<Value>,
}

async fn claim_local_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunAccessRequest>,
) -> Result<Json<Value>, ApiError> {
    let hash = local_run_ticket_hash(&req.ticket)?;
    let ticket = jobs::claim_local_run_ticket(&state.pool, &run_id, &hash)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ))?;
    for mut intervention in jobs::list_interventions(&state.pool, &ticket.account_id)
        .map_err(internal)?
        .into_iter()
        .filter(|item| {
            item.application_id.as_deref() == Some(ticket.application_id.as_str())
                && item.status == "open"
        })
    {
        intervention.status = "resolved".to_string();
        intervention.resolved_at_ms = Some(jobs::now_ms());
        jobs::save_intervention(&state.pool, &ticket.account_id, &intervention)
            .map_err(internal)?;
    }
    jobs::update_application(
        &state.pool,
        &ticket.account_id,
        &ticket.application_id,
        "running",
        None,
    )
    .map_err(domain_error)?
    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    update_worker_browser_session(
        &state,
        &ticket.account_id,
        &ticket.application_id,
        "running",
    )?;
    jobs::save_run_event(
        &state.pool,
        &ticket.account_id,
        &ticket.id,
        "local_browser_claimed",
        json!({ "application_id": ticket.application_id }),
    )
    .map_err(internal)?;
    Ok(Json(ticket.payload))
}

async fn save_local_run_result(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunResultRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    let hash = local_run_ticket_hash(&req.ticket)?;
    let ticket = jobs::get_local_run_ticket_by_hash(&state.pool, &run_id, &hash)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ))?;
    if ticket.expires_at_ms <= jobs::now_ms() {
        return Err((
            StatusCode::GONE,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ));
    }
    if matches!(ticket.status.as_str(), "complete" | "failed") {
        return jobs::get_application(&state.pool, &ticket.account_id, &ticket.application_id)
            .map_err(internal)?
            .map(Json)
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()));
    }
    let status = req
        .receipt
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let application = match status {
        "needs_input" => {
            create_intervention_from_receipt(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                req.receipt,
            )?;
            let application = jobs::update_application(
                &state.pool,
                &ticket.account_id,
                &ticket.application_id,
                "needs_input",
                None,
            )
            .map_err(domain_error)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
            update_worker_browser_session(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                "needs_input",
            )?;
            jobs::update_local_run_ticket_status(&state.pool, &run_id, &hash, "needs_input")
                .map_err(internal)?;
            application
        }
        "submitted" => {
            let bundle = req.receipt_bundle.ok_or((
                StatusCode::BAD_REQUEST,
                "The local browser did not return its submission receipt.".to_string(),
            ))?;
            let application = persist_submission_receipt(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                bundle,
            )?;
            jobs::update_local_run_ticket_status(&state.pool, &run_id, &hash, "complete")
                .map_err(internal)?;
            application
        }
        "failed" => {
            let application = jobs::update_application(
                &state.pool,
                &ticket.account_id,
                &ticket.application_id,
                "failed",
                None,
            )
            .map_err(domain_error)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
            update_worker_browser_session(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                "failed",
            )?;
            jobs::update_local_run_ticket_status(&state.pool, &run_id, &hash, "failed")
                .map_err(internal)?;
            application
        }
        _ => return bad_request("Bluey Browser returned an invalid application result."),
    };
    Ok(Json(application))
}

fn local_run_ticket_hash(ticket: &str) -> Result<String, ApiError> {
    if ticket.len() != 64 || !ticket.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err((
            StatusCode::NOT_FOUND,
            "Bluey Browser launch not found.".to_string(),
        ));
    }
    Ok(hex::encode(Sha256::digest(ticket.as_bytes())))
}

#[derive(Debug, Deserialize)]
struct WorkerEventRequest {
    account_id: String,
    #[serde(default)]
    application_id: String,
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    body: Value,
}

async fn worker_run_event(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerEventRequest>,
) -> Result<Json<RunEvent>, ApiError> {
    ensure_worker_application(&state, &req.account_id, &req.application_id)?;
    if req.event_type.trim().is_empty() || req.event_type.len() > 80 {
        return bad_request("Invalid run event.");
    }
    jobs::save_run_event(
        &state.pool,
        &req.account_id,
        &run_id,
        &req.event_type,
        req.body,
    )
    .map(Json)
    .map_err(internal)
}

#[derive(Debug, Deserialize)]
struct WorkerStateRequest {
    account_id: String,
    state: String,
}

async fn worker_application_state(
    State(state): State<AppState>,
    Path(application_id): Path<String>,
    Json(req): Json<WorkerStateRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    let application = jobs::update_application(
        &state.pool,
        &req.account_id,
        &application_id,
        &req.state,
        None,
    )
    .map_err(domain_error)?
    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    update_worker_browser_session(&state, &req.account_id, &application_id, &req.state)?;
    Ok(Json(application))
}

#[derive(Debug, Deserialize)]
struct WorkerInterventionRequest {
    account_id: String,
    receipt: Value,
}

async fn worker_intervention(
    State(state): State<AppState>,
    Path(application_id): Path<String>,
    Json(req): Json<WorkerInterventionRequest>,
) -> Result<Json<Intervention>, ApiError> {
    create_intervention_from_receipt(&state, &req.account_id, &application_id, req.receipt)
        .map(Json)
}

fn create_intervention_from_receipt(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    mut receipt: Value,
) -> Result<Intervention, ApiError> {
    ensure_worker_application(state, account_id, application_id)?;
    if let Some(receipt) = receipt.as_object_mut() {
        receipt.remove("screenshotPath");
    }
    let source = receipt
        .get("intervention")
        .and_then(Value::as_object)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Intervention details are missing.".to_string(),
        ))?;
    let resolution = source.get("resolution").and_then(Value::as_object);
    let takeover_url = source
        .get("takeoverUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    let intervention = Intervention {
        id: String::new(),
        application_id: Some(application_id.to_string()),
        kind: string_value(source, "kind", "browser_takeover"),
        status: "open".to_string(),
        title: string_value(source, "title", "Application needs your input"),
        detail: string_value(source, "detail", "Open the preserved browser to continue."),
        choices: source
            .get("choices")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        resolution_kind: resolution
            .map(|value| string_value(value, "kind", "browser_takeover"))
            .unwrap_or_else(|| "browser_takeover".to_string()),
        resume_after_resolution: resolution
            .and_then(|value| value.get("resumeAfter"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        provider: resolution
            .map(|value| string_value(value, "provider", ""))
            .unwrap_or_default(),
        provider_message_id: String::new(),
        expires_at_ms: None,
        metadata: json!({ "receipt": receipt }),
        created_at_ms: 0,
        resolved_at_ms: None,
    };
    let saved =
        jobs::save_intervention(&state.pool, account_id, &intervention).map_err(internal)?;
    if let Some(takeover_url) = takeover_url {
        if takeover_url.starts_with("https://") || takeover_url.starts_with("bluey-jobs://") {
            if let Some(mut session) = jobs::list_browser_sessions(&state.pool, account_id)
                .map_err(internal)?
                .into_iter()
                .find(|session| {
                    session.application_id.as_deref() == saved.application_id.as_deref()
                })
            {
                session.status = "needs_input".to_string();
                session.current_step = saved.title.clone();
                session.takeover_url = Some(takeover_url);
                jobs::upsert_browser_session(&state.pool, account_id, &session)
                    .map_err(internal)?;
            }
        }
    }
    Ok(saved)
}

#[derive(Debug, Deserialize)]
struct WorkerReceiptRequest {
    account_id: String,
    receipt: Value,
}

async fn worker_receipt(
    State(state): State<AppState>,
    Path(application_id): Path<String>,
    Json(req): Json<WorkerReceiptRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    persist_submission_receipt(&state, &req.account_id, &application_id, req.receipt).map(Json)
}

fn persist_submission_receipt(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    mut receipt: Value,
) -> Result<JobApplication, ApiError> {
    let application = jobs::get_application(&state.pool, account_id, application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    if receipt.get("applicationId").and_then(Value::as_str) != Some(application_id) {
        return bad_request("Receipt does not match this application.");
    }
    let result = receipt.get("result").and_then(Value::as_object).ok_or((
        StatusCode::BAD_REQUEST,
        "Submission result is missing.".to_string(),
    ))?;
    if result.get("status").and_then(Value::as_str) != Some("submitted") {
        return bad_request("Only a confirmed submission can create a final receipt.");
    }
    let confirmation = result
        .get("confirmationText")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Application submitted")
        .to_string();
    let confirmation_url = result.get("confirmationUrl").cloned();
    let submitted_at = result.get("submittedAt").cloned();
    let resume_id = application.resume_version_id.as_deref().ok_or((
        StatusCode::CONFLICT,
        "The submitted resume version is missing.".to_string(),
    ))?;
    let resume = jobs::get_resume_version(&state.pool, account_id, resume_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::CONFLICT,
            "The submitted resume version is missing.".to_string(),
        ))?;
    let posting = jobs::get_posting(&state.pool, account_id, &application.job_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
    sanitize_receipt_storage(&mut receipt, application_id, &resume.id);
    let provider = receipt
        .get("adapter")
        .and_then(Value::as_str)
        .unwrap_or(&posting.source)
        .to_string();
    jobs::save_application_evidence(
        &state.pool,
        account_id,
        &ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: "resume".to_string(),
            label: "Resume submitted".to_string(),
            provider: provider.clone(),
            file_name: format!(
                "{}-{}-resume.pdf",
                safe_file_part(&posting.company),
                safe_file_part(&posting.title)
            ),
            media_type: "application/pdf".to_string(),
            storage_key: format!("jobs/resume-versions/{}", resume.id),
            sha256: resume.checksum.clone(),
            resume_version_id: Some(resume.id.clone()),
            occurred_at_ms: 0,
            metadata: json!({
                "attached_to_submission": true,
                "receipt_id": receipt.get("receiptId"),
                "application_identity_id": receipt.get("applicationIdentityId"),
            }),
            created_at_ms: 0,
        },
    )
    .map_err(internal)?;
    jobs::save_application_evidence(
        &state.pool,
        account_id,
        &ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: "submission_confirmation".to_string(),
            label: confirmation,
            provider,
            file_name: String::new(),
            media_type: "application/json".to_string(),
            storage_key: format!(
                "jobs/receipts/{}",
                receipt
                    .get("receiptId")
                    .and_then(Value::as_str)
                    .unwrap_or(&application_id)
            ),
            sha256: hex::encode(Sha256::digest(receipt.to_string().as_bytes())),
            resume_version_id: Some(resume.id),
            occurred_at_ms: 0,
            metadata: json!({
                "confirmation_url": confirmation_url,
                "submitted_at": submitted_at,
                "screenshot_keys": receipt.get("screenshotKeys"),
            }),
            created_at_ms: 0,
        },
    )
    .map_err(internal)?;
    jobs::replace_application_receipt(&state.pool, account_id, application_id, receipt)
        .map_err(internal)?;
    let application =
        jobs::update_application(&state.pool, account_id, application_id, "submitted", None)
            .map_err(domain_error)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    update_worker_browser_session(state, account_id, application_id, "complete")?;
    Ok(application)
}

fn sanitize_receipt_storage(receipt: &mut Value, application_id: &str, resume_id: &str) {
    if let Some(documents) = receipt.get_mut("documents").and_then(Value::as_array_mut) {
        for (index, document) in documents.iter_mut().enumerate() {
            let Some(document) = document.as_object_mut() else {
                continue;
            };
            let kind = document
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("attachment");
            let storage_key = match kind {
                "resume" => format!("jobs/resume-versions/{resume_id}"),
                "cover_letter" => format!("jobs/applications/{application_id}/cover-letter"),
                _ => format!("jobs/applications/{application_id}/attachments/{index}"),
            };
            document.insert("storageKey".to_string(), Value::String(storage_key));
        }
    }
    if let Some(result) = receipt.get_mut("result").and_then(Value::as_object_mut) {
        result.remove("screenshotPath");
    }
    if let Some(screenshots) = receipt
        .get_mut("screenshotKeys")
        .and_then(Value::as_array_mut)
    {
        screenshots.retain(|value| {
            value
                .as_str()
                .is_some_and(|key| key.starts_with("jobs/") || key.starts_with("r2://"))
        });
    }
}

fn ensure_worker_application(
    state: &AppState,
    account_id: &str,
    application_id: &str,
) -> Result<(), ApiError> {
    if application_id.is_empty()
        || jobs::get_application(&state.pool, account_id, application_id)
            .map_err(internal)?
            .is_none()
    {
        return Err((StatusCode::NOT_FOUND, "Application not found.".to_string()));
    }
    Ok(())
}

fn update_worker_browser_session(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    status: &str,
) -> Result<(), ApiError> {
    let session_status = match status {
        "submitted" => "complete",
        "needs_input" => "needs_input",
        "failed" => "failed",
        "running" => "running",
        other => other,
    };
    if let Some(mut session) = jobs::list_browser_sessions(&state.pool, account_id)
        .map_err(internal)?
        .into_iter()
        .find(|session| session.application_id.as_deref() == Some(application_id))
    {
        session.status = session_status.to_string();
        session.current_step = match session_status {
            "complete" => "Application submitted",
            "needs_input" => "Waiting for your input",
            "failed" => "Run stopped",
            "running" => "Filling application",
            _ => &session.current_step,
        }
        .to_string();
        if matches!(session_status, "complete" | "failed") {
            session.takeover_url = None;
        }
        jobs::upsert_browser_session(&state.pool, account_id, &session).map_err(internal)?;
    }
    Ok(())
}

fn validate_profile(profile: &CareerProfile) -> Result<(), ApiError> {
    if profile.onboarding_complete {
        if profile.full_name.trim().is_empty() {
            return bad_request("Add your full name before finishing setup.");
        }
        if profile.current_location.trim().is_empty() {
            return bad_request("Add your current location before finishing setup.");
        }
        if profile.employment.is_empty() && profile.education.is_empty() {
            return bad_request("Add work experience or education before finishing setup.");
        }
    }
    if !matches!(profile.resume_mode.as_str(), "factual" | "enhance") {
        return bad_request("Choose Factual or Enhance.");
    }
    if !matches!(
        profile.default_submission_mode.as_str(),
        "review_first" | "auto_submit"
    ) {
        return bad_request("Choose Review first or Auto-submit.");
    }
    if !(60..=100).contains(&profile.auto_submit_threshold) {
        return bad_request("Choose an Auto-submit match threshold from 60% to 100%.");
    }
    if !(1..=50).contains(&profile.daily_limit) {
        return bad_request("Choose a daily application limit from 1 to 50.");
    }
    Ok(())
}

fn validate_preferences(preferences: &JobPreferences) -> Result<(), ApiError> {
    if !matches!(
        preferences.location_policy.as_str(),
        "local" | "willing_to_relocate" | "remote_only" | "ask"
    ) {
        return bad_request("Choose how Bluey should handle job locations.");
    }
    if !(1..=50).contains(&preferences.daily_limit) {
        return bad_request("Choose a daily application limit from 1 to 50.");
    }
    Ok(())
}

fn validate_track(track: &CareerTrack) -> Result<(), ApiError> {
    if track.name.trim().is_empty() || track.role.trim().is_empty() {
        return bad_request("Give this Career Track a name and target role.");
    }
    Ok(())
}

fn validate_posting(posting: &JobPosting) -> Result<(), ApiError> {
    if posting.company.trim().is_empty() || posting.title.trim().is_empty() {
        return bad_request("Add the company and role for this job.");
    }
    if !posting.canonical_url.is_empty() {
        let url = reqwest::Url::parse(&posting.canonical_url).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Use a complete http or https job link.".to_string(),
            )
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return bad_request("Use a complete http or https job link.");
        }
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if host == "localhost" || host.ends_with(".local") || is_private_ip_literal(&host) {
            return bad_request("Use a public employer job link.");
        }
    }
    Ok(())
}

fn is_private_ip_literal(host: &str) -> bool {
    let trimmed = host.trim_matches(['[', ']']);
    let Ok(ip) = trimmed.parse::<std::net::IpAddr>() else {
        return false;
    };
    match ip {
        std::net::IpAddr::V4(value) => {
            value.is_private()
                || value.is_loopback()
                || value.is_link_local()
                || value.is_broadcast()
                || value.is_unspecified()
        }
        std::net::IpAddr::V6(value) => {
            value.is_loopback() || value.is_unspecified() || value.is_unique_local()
        }
    }
}

fn apply_submission_boundary(posting: &mut JobPosting) {
    let url = posting.canonical_url.to_lowercase();
    if url.contains("linkedin.com") {
        posting.source = "linkedin_handoff".to_string();
        posting
            .matched_reasons
            .push("Bluey prepares this packet; you submit it on LinkedIn.".to_string());
    } else if url.contains("indeed.com") {
        posting.source = "indeed_handoff".to_string();
        posting
            .matched_reasons
            .push("Bluey prepares this packet; you submit it on Indeed.".to_string());
    }
}

fn default_cloud_runner() -> String {
    "cloud".to_string()
}

fn execution_answers(
    profile: &CareerProfile,
    application: &JobApplication,
    application_email: &str,
) -> BTreeMap<String, String> {
    let mut answers = BTreeMap::new();
    let mut names = profile.full_name.split_whitespace();
    let first_name = names.next().unwrap_or_default();
    let last_name = names.collect::<Vec<_>>().join(" ");
    add_answer(&mut answers, "first_name", first_name);
    add_answer(&mut answers, "last_name", &last_name);
    add_answer(&mut answers, "full_name", &profile.full_name);
    add_answer(&mut answers, "email", application_email);
    add_answer(&mut answers, "phone", &profile.phone);
    add_answer(&mut answers, "location", &profile.current_location);
    add_answer(&mut answers, "address", &profile.street_address);
    add_answer(&mut answers, "linkedin_url", &profile.linkedin_url);
    add_answer(&mut answers, "portfolio_url", &profile.portfolio_url);
    add_answer(
        &mut answers,
        "work_authorization",
        &profile.work_authorization,
    );
    add_answer(
        &mut answers,
        "sponsorship_required",
        match profile.sponsorship_required {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "",
        },
    );
    add_answer(
        &mut answers,
        "salary_expectation",
        &profile.salary_expectation,
    );
    add_answer(&mut answers, "notice_period", &profile.notice_period);
    if let Some(reusable) = profile.reusable_answers.as_object() {
        for (key, value) in reusable {
            if let Some(value) = value.as_str() {
                add_answer(&mut answers, key, value);
            }
        }
    }
    for answer in &application.answers {
        let Some(answer) = answer.as_object() else {
            continue;
        };
        let key = ["key", "question", "field", "name"]
            .iter()
            .find_map(|key| answer.get(*key).and_then(Value::as_str))
            .unwrap_or_default();
        let value = ["value", "answer"]
            .iter()
            .find_map(|key| answer.get(*key).and_then(Value::as_str))
            .unwrap_or_default();
        add_answer(&mut answers, key, value);
    }
    answers
}

fn add_answer(answers: &mut BTreeMap<String, String>, key: &str, value: &str) {
    if !key.trim().is_empty() && !value.trim().is_empty() {
        answers.insert(key.trim().to_string(), value.trim().to_string());
    }
}

fn ats_kind(raw_url: &str) -> &'static str {
    let host = reqwest::Url::parse(raw_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    if host.contains("myworkdayjobs.com") {
        "workday"
    } else if host == "boards.greenhouse.io" || host == "job-boards.greenhouse.io" {
        "greenhouse"
    } else if host == "jobs.lever.co" {
        "lever"
    } else if host == "jobs.ashbyhq.com" {
        "ashby"
    } else if host == "jobs.smartrecruiters.com" || host.ends_with(".smartrecruiters.com") {
        "smartrecruiters"
    } else {
        "semantic"
    }
}

fn normalized_workplace(value: &str) -> &str {
    match value.to_ascii_lowercase().as_str() {
        "onsite" => "onsite",
        "hybrid" => "hybrid",
        "remote" => "remote",
        _ => "unknown",
    }
}

fn string_value(source: &serde_json::Map<String, Value>, key: &str, fallback: &str) -> String {
    source
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn safe_file_part(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(80)
        .collect()
}

fn default_factual() -> String {
    "factual".to_string()
}

fn default_review_first() -> String {
    "review_first".to_string()
}

fn bad_request<T>(message: &str) -> Result<T, ApiError> {
    Err((StatusCode::BAD_REQUEST, message.to_string()))
}

fn validation_or_internal(error: anyhow::Error, validation_message: &str) -> ApiError {
    if error.to_string().contains("invalid application") {
        (StatusCode::BAD_REQUEST, validation_message.to_string())
    } else {
        internal(error)
    }
}

fn domain_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    let status = if message.contains("another Bluey Jobs account") {
        StatusCode::CONFLICT
    } else if message.contains("days old")
        || message.contains("no longer accepting")
        || message.contains("still open before applying")
        || message.contains("before marking this application submitted")
    {
        StatusCode::CONFLICT
    } else if message.contains("limit reached") {
        StatusCode::PAYMENT_REQUIRED
    } else if message.contains("not found") {
        StatusCode::NOT_FOUND
    } else if message.contains("verification")
        || message.contains("verify the")
        || message.contains("choose another")
        || message.contains("complete email")
        || message.contains("Gmail or Outlook")
        || message.contains("wait a minute")
        || message.contains("application evidence")
        || message.contains("evidence needs")
        || message.contains("resume evidence")
        || message.contains("resume version")
        || message.contains("another job")
        || message.contains("submission confirmation")
        || message.contains("answer memory")
        || message.contains("application question")
        || message.contains("answer Bluey should remember")
        || message.contains("where this answer should be reused")
        || message.contains("career track not found")
    {
        StatusCode::BAD_REQUEST
    } else {
        return internal(error);
    };
    (status, message)
}

fn internal(error: anyhow::Error) -> ApiError {
    tracing::error!(error = %error, "Bluey Jobs request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Bluey Jobs could not finish that request. Please try again.".to_string(),
    )
}

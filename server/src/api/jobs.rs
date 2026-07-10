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
            let memory = AnswerMemory {
                id: String::new(),
                key: String::new(),
                question: updated.title.clone(),
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
    let saved = jobs::save_intervention(&state.pool, &account.id, &updated).map_err(internal)?;
    Ok(Json(InterventionResolutionResult {
        intervention: saved,
        answer_memory: remembered_answer,
        application: resumed_application,
    }))
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

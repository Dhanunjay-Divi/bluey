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
        self, BrowserSession, CareerFact, CareerProfile, CareerTrack, Intervention, JobApplication,
        JobPosting, JobPreferences, JobsEntitlement, JobsIntegration, JobsWorkspace,
        PacketCommitResult, ResumeVersion, RunEvent,
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
            "/api/jobs/integrations",
            get(integrations).put(save_integration),
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
}

pub async fn update_intervention(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(intervention_id): Path<String>,
    Json(req): Json<ResolveInterventionRequest>,
) -> Result<Json<Intervention>, ApiError> {
    let intervention = jobs::list_interventions(&state.pool, &account.id)
        .map_err(internal)?
        .into_iter()
        .find(|item| item.id == intervention_id)
        .ok_or((StatusCode::NOT_FOUND, "Intervention not found.".to_string()))?;
    let mut updated = intervention;
    updated.status = req.status;
    jobs::save_intervention(&state.pool, &account.id, &updated)
        .map(Json)
        .map_err(internal)
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
        "gmail" | "outlook_email" | "google_calendar" | "outlook_calendar"
    ) {
        return bad_request("Choose a supported email or calendar provider.");
    }
    if integration.status != "disconnected" {
        return bad_request("Complete provider authorization before connecting this account.");
    }
    jobs::save_integration(&state.pool, &account.id, &integration)
        .map(Json)
        .map_err(internal)
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

fn internal(error: anyhow::Error) -> ApiError {
    tracing::error!(error = %error, "Bluey Jobs request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Bluey Jobs could not finish that request. Please try again.".to_string(),
    )
}

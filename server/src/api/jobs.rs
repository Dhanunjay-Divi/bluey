//! Authenticated Bluey Jobs API.
//!
//! The web product shares Bluey identity and balance, while Jobs records,
//! automation state, and packet metering remain isolated under `/api/jobs`.

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Extension, Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    api::{jobs_import, jobs_resume_generation, AppState},
    auth::AuthedAccount,
    db::jobs::{
        self, normalize_candidate_employment_type, normalize_candidate_engagement_type,
        AnswerMemory, ApplicationEvidence, ApplicationIdentity, BrowserSession, CandidateEvent,
        CareerFact, CareerProfile, CareerTrack, Intervention, JobApplication,
        JobEligibilityDecision, JobPosting, JobPreferences, JobsEntitlement, JobsIntegration,
        JobsWorkspace, PacketCommitResult, ResumeVersion, RunEvent, RunnerAvailability,
        RunnerChannelAvailability,
    },
    object_storage::{sha256_hex, ObjectStorage},
};

pub(super) type ApiError = (StatusCode, String);

// Receipts include the exact resume and confirmation screenshot as base64 so
// the server can verify and persist evidence before accepting "submitted".
const RECEIPT_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const DISCOVERY_SNAPSHOT_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const TRUSTED_WORKER_RECEIPT_KEY: &str = "_bluey_worker_receipt_v1";
const SUBMISSION_FINGERPRINT_KEY: &str = "_bluey_server_submission_fingerprint_v1";
const MAX_RECEIPT_DOCUMENTS: usize = 8;
const MAX_RECEIPT_SCREENSHOTS: usize = 4;
const MAX_RECEIPT_EVIDENCE_OBJECTS: usize = 12;
const MAX_RECEIPT_EVIDENCE_BYTES: usize = 40 * 1024 * 1024;
const MAX_WORKSPACE_DISCOVERY_BACKFILL_POSTINGS: usize = 250;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs/workspace", get(workspace))
        .route("/api/jobs/onboarding/complete", post(complete_onboarding))
        .route("/api/jobs/profile", get(profile).put(save_profile))
        .route(
            "/api/jobs/resume-source",
            get(super::jobs_resume_assets::resume_source)
                .post(super::jobs_resume_assets::upload_resume_source)
                .route_layer(DefaultBodyLimit::max(15 * 1024 * 1024)),
        )
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
        .route(
            "/api/jobs/tracks/:track_id/auto-submit",
            post(authorize_track_auto_submit).delete(revoke_track_auto_submit),
        )
        .route("/api/jobs/matches", get(matches).post(save_match))
        .route("/api/jobs/matches/:job_id", get(match_detail))
        .route(
            "/api/jobs/discovery/catalog",
            get(super::jobs_source_directory::search_catalog),
        )
        .route(
            "/api/jobs/discovery/sources",
            post(super::jobs_source_directory::connect_source),
        )
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
            "/api/jobs/applications/:application_id/approve",
            post(approve_application_packet),
        )
        .route(
            "/api/jobs/applications/:application_id/runs",
            post(queue_application_run),
        )
        .route(
            "/api/jobs/applications/:application_id/evidence",
            get(application_evidence),
        )
        .route(
            "/api/jobs/applications/:application_id/interview-prep",
            post(super::jobs_interview_prep::generate),
        )
        .route(
            "/api/jobs/resume-versions/:resume_version_id",
            get(resume_version),
        )
        .route(
            "/api/jobs/resume-versions/:resume_version_id/template-docx",
            get(super::jobs_resume_assets::download_template_docx),
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
            "/api/jobs/candidate-events",
            get(candidate_events).post(save_candidate_event),
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
            get(super::jobs_mailbox::mailbox_connections),
        )
        .route(
            "/api/jobs/mailbox-oauth/config",
            get(super::jobs_mailbox_oauth::provider_availability),
        )
        .route(
            "/api/jobs/mailbox-oauth/:provider/start",
            post(super::jobs_mailbox_oauth::start_oauth),
        )
        .route(
            "/api/jobs/mailbox-connections/:connection_id",
            delete(super::jobs_mailbox::remove_mailbox_connection),
        )
        .route(
            "/api/jobs/mailbox-connections/:connection_id/sync-state",
            get(super::jobs_mailbox::mailbox_sync_state),
        )
        .route(
            "/api/jobs/mailbox-connections/:connection_id/sync",
            post(super::jobs_mailbox::sync_mailbox_now),
        )
        .route(
            "/api/jobs/mailbox-messages",
            get(super::jobs_mailbox::mailbox_messages),
        )
        .route(
            "/api/jobs/communication-actions",
            get(super::jobs_communication_actions::communication_actions)
                .post(super::jobs_communication_actions::create_communication_action),
        )
        .route(
            "/api/jobs/communication-actions/:action_id",
            get(super::jobs_communication_actions::communication_action),
        )
        .route(
            "/api/jobs/communication-actions/:action_id/approve",
            post(super::jobs_communication_actions::approve_communication_action),
        )
        .route(
            "/api/jobs/communication-actions/:action_id/cancel",
            post(super::jobs_communication_actions::cancel_communication_action),
        )
        .route("/api/jobs/entitlements", get(entitlements))
        .route("/api/jobs/runs/:run_id/events", get(run_events))
        .route_layer(axum::middleware::from_fn(require_jobs_beta))
}

pub fn worker_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/execution-leases/claim",
            post(worker_claim_execution_lease),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/heartbeat",
            post(worker_heartbeat_execution_lease),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/irreversible",
            post(worker_start_irreversible_submission),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/finish",
            post(worker_finish_execution_lease),
        )
        .route(
            "/api/jobs/internal/discovery/lease",
            post(worker_discovery_lease),
        )
        .route(
            "/api/jobs/internal/discovery/:source_id/complete",
            post(worker_discovery_complete)
                .route_layer(DefaultBodyLimit::max(DISCOVERY_SNAPSHOT_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/jobs/internal/discovery/:source_id/fail",
            post(worker_discovery_fail),
        )
        .route(
            "/api/jobs/internal/global-discovery/sources/sync",
            post(worker_global_discovery_source_sync),
        )
        .route(
            "/api/jobs/internal/global-discovery/lease",
            post(worker_global_discovery_lease),
        )
        .route(
            "/api/jobs/internal/global-discovery/:source_id/batches",
            post(worker_global_discovery_batch)
                .route_layer(DefaultBodyLimit::max(DISCOVERY_SNAPSHOT_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/jobs/internal/global-discovery/:source_id/complete",
            post(worker_global_discovery_complete),
        )
        .route(
            "/api/jobs/internal/global-discovery/:source_id/fail",
            post(worker_global_discovery_fail),
        )
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
            post(worker_receipt).route_layer(DefaultBodyLimit::max(RECEIPT_BODY_LIMIT_BYTES)),
        )
}

pub fn local_runner_router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs/local-runs/:run_id/claim", post(claim_local_run))
        .route(
            "/api/jobs/local-runs/:run_id/authorize-submit",
            post(authorize_local_run_submit),
        )
        .route(
            "/api/jobs/local-runs/:run_id/resume",
            post(consume_local_run_resume),
        )
        .route(
            "/api/jobs/local-runs/:run_id/result",
            post(save_local_run_result)
                .route_layer(DefaultBodyLimit::max(RECEIPT_BODY_LIMIT_BYTES)),
        )
        .route_layer(axum::middleware::from_fn(require_jobs_beta))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/entitlements/:account_id",
            patch(set_account_entitlement),
        )
        .route(
            "/admin/jobs/discovery-sources/:account_id",
            post(upsert_account_discovery_source),
        )
        .route(
            "/admin/jobs/discovery-sources/:account_id/:source_id",
            patch(set_account_discovery_source_status),
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

fn jobs_local_browser_distribution_enabled() -> bool {
    cfg!(debug_assertions)
        || std::env::var("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
            .unwrap_or(false)
}

fn jobs_cloud_browser_distribution_enabled() -> bool {
    cfg!(debug_assertions)
        || (std::env::var("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
            .unwrap_or(false)
            && std::env::var("BLUEY_JOBS_WORKFLOW_TOKEN")
                .is_ok_and(|value| !value.trim().is_empty()))
}

fn apply_jobs_distribution_gates(
    entitlement: &mut JobsEntitlement,
    local_browser: bool,
    cloud_browser: bool,
) {
    entitlement.local_browser &= local_browser;
    entitlement.cloud_browser &= cloud_browser;
}

fn runner_channel_availability(
    runner: &str,
    plan: &str,
    plan_included: bool,
    distribution_enabled: bool,
) -> RunnerChannelAvailability {
    if !plan_included {
        let (reason, next_action) = if runner == "local" {
            (
                format!(
                    "Local Auto-submit is not included in the {} Jobs plan.",
                    plan.to_ascii_uppercase()
                ),
                "Choose Pro or Cloud, or keep using Review first.".to_string(),
            )
        } else {
            (
                format!(
                    "Background Auto-submit is not included in the {} Jobs plan.",
                    plan.to_ascii_uppercase()
                ),
                "Choose Cloud, or keep using Review first.".to_string(),
            )
        };
        return RunnerChannelAvailability {
            status: "upgrade_required".to_string(),
            available: false,
            plan_included: false,
            distribution_enabled,
            reason,
            next_action,
        };
    }
    if !distribution_enabled {
        let label = if runner == "local" {
            "Bluey Browser"
        } else {
            "Background runner"
        };
        return RunnerChannelAvailability {
            status: "invited_beta".to_string(),
            available: false,
            plan_included: true,
            distribution_enabled: false,
            reason: format!(
                "{label} is included in your plan but has not been enabled for this release."
            ),
            next_action:
                "Use Review first; Bluey will prepare the exact resume and answers for handoff."
                    .to_string(),
        };
    }
    let (reason, next_action) = if runner == "local" {
        (
            "Bluey Browser is available on this account.".to_string(),
            "Approve a packet, then run it on this computer.".to_string(),
        )
    } else {
        (
            "The background runner is available on this account.".to_string(),
            "Approve a packet, then queue it in the cloud.".to_string(),
        )
    };
    RunnerChannelAvailability {
        status: "available".to_string(),
        available: true,
        plan_included: true,
        distribution_enabled: true,
        reason,
        next_action,
    }
}

fn build_runner_availability(
    entitlement: &JobsEntitlement,
    local_distribution_enabled: bool,
    cloud_distribution_enabled: bool,
) -> RunnerAvailability {
    let local = runner_channel_availability(
        "local",
        &entitlement.plan,
        entitlement.local_browser,
        local_distribution_enabled,
    );
    let cloud = runner_channel_availability(
        "cloud",
        &entitlement.plan,
        entitlement.cloud_browser,
        cloud_distribution_enabled,
    );
    let auto_submit_available = local.available || cloud.available;
    let auto_submit_reason = if local.available && cloud.available {
        "Auto-submit can use either Bluey Browser or the background runner.".to_string()
    } else if local.available {
        "Auto-submit can use Bluey Browser while this computer is running.".to_string()
    } else if cloud.available {
        "Auto-submit can use the background runner while your computer is off.".to_string()
    } else if local.plan_included || cloud.plan_included {
        "Auto-submit is not available in this release because your included runner is still in invited beta. Review first and job-site handoff remain available.".to_string()
    } else {
        "Auto-submit requires a Jobs plan with runner access. Review first remains available."
            .to_string()
    };
    RunnerAvailability {
        local,
        cloud,
        auto_submit_available,
        auto_submit_reason,
    }
}

fn auto_submit_request_error(
    eligibility: &JobEligibilityDecision,
    runners: &RunnerAvailability,
) -> Option<ApiError> {
    if let Some(reason) = eligibility.hard_failures.first() {
        return Some((
            StatusCode::CONFLICT,
            format!(
                "Auto-submit is blocked by your Career Track: {}",
                reason.message
            ),
        ));
    }
    if !eligibility.can_auto_submit {
        let message = match eligibility.capability.as_str() {
            "beta_review" => {
                "Auto-submit is unavailable because this application system is in beta and requires packet review."
            }
            "handoff" => {
                "Auto-submit is unavailable because this site requires a user-controlled handoff after Bluey prepares the application kit."
            }
            "unknown_review" => {
                "Auto-submit is unavailable because this application system has not been certified."
            }
            "blocked" => "Auto-submit is unavailable because Bluey blocks runners on this site.",
            _ => eligibility
                .review_reasons
                .first()
                .map(|reason| reason.message.as_str())
                .unwrap_or(
                    "Auto-submit is unavailable until every application-system and Career Track check passes.",
                ),
        };
        return Some((StatusCode::CONFLICT, message.to_string()));
    }
    if !runners.auto_submit_available {
        let status = if runners.local.plan_included || runners.cloud.plan_included {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::PAYMENT_REQUIRED
        };
        return Some((status, runners.auto_submit_reason.clone()));
    }
    None
}

fn schedule_global_candidate_materialization(
    pool: crate::db::DbPool,
    account_id: String,
    email: String,
    reason: &'static str,
) {
    let account_fingerprint = discovery_log_fingerprint(&account_id);
    std::mem::drop(tokio::task::spawn_blocking(move || {
        match jobs::materialize_global_candidates_for_account(&pool, &account_id, &email) {
            Ok(result) if result.materialized_count > 0 || result.refreshed_count > 0 => {
                tracing::info!(
                    account_fingerprint = %account_fingerprint,
                    reason,
                    considered_count = result.considered_count,
                    materialized_count = result.materialized_count,
                    refreshed_count = result.refreshed_count,
                    skipped_count = result.skipped_count,
                    "Jobs projected shared discovery candidates"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(
                account_fingerprint = %account_fingerprint,
                reason,
                error_category = "global_materialization_failed",
                error = %error,
                "Jobs continued without shared discovery projection"
            ),
        }
    }));
}

pub async fn workspace(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<JobsWorkspace>, ApiError> {
    if let Err(error) = jobs::ensure_managed_curated_discovery_source(&state.pool, &account.id) {
        tracing::warn!(
            account_fingerprint = %discovery_log_fingerprint(&account.id),
            error_category = discovery_enrollment_error_category(&error),
            "Jobs workspace could not ensure managed curated discovery"
        );
    }
    schedule_global_candidate_materialization(
        state.pool.clone(),
        account.id.clone(),
        account.email.clone(),
        "workspace_load",
    );
    let mut workspace =
        jobs::workspace(&state.pool, &account.id, &account.email).map_err(internal)?;
    if backfill_verified_import_discovery_sources(
        &state.pool,
        &account.id,
        &workspace.matches,
        &workspace.profile,
        &workspace.preferences,
    ) > 0
    {
        match jobs::list_discovery_sources(&state.pool, &account.id) {
            Ok(sources) => {
                workspace.discovery_sources = sources
                    .iter()
                    .map(jobs::DiscoverySourceSummary::from)
                    .collect();
            }
            Err(_error) => tracing::warn!(
                account_fingerprint = %discovery_log_fingerprint(&account.id),
                error_category = "source_summary_refresh_failed",
                "Jobs workspace completed without refreshed discovery source summaries"
            ),
        }
    }
    let local_distribution_enabled = jobs_local_browser_distribution_enabled();
    let cloud_distribution_enabled = jobs_cloud_browser_distribution_enabled();
    workspace.runner_availability = build_runner_availability(
        &workspace.entitlement,
        local_distribution_enabled,
        cloud_distribution_enabled,
    );
    apply_jobs_distribution_gates(
        &mut workspace.entitlement,
        local_distribution_enabled,
        cloud_distribution_enabled,
    );
    Ok(Json(workspace))
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
    Json(profile): Json<CareerProfile>,
) -> Result<Json<CareerProfile>, ApiError> {
    validate_profile(&profile)?;
    jobs::save_profile(&state.pool, &account.id, &profile)
        .map(Json)
        .map_err(internal)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteOnboardingRequest {
    pub profile: CareerProfile,
    pub preferences: JobPreferences,
    pub track: CareerTrack,
}

pub async fn complete_onboarding(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(mut input): Json<CompleteOnboardingRequest>,
) -> Result<Json<JobsWorkspace>, ApiError> {
    input.profile.onboarding_step = 6;
    input.profile.onboarding_complete = true;
    validate_profile(&input.profile)?;
    validate_preferences(&input.preferences)?;
    validate_track(&input.track)?;

    let entitlement = jobs::get_entitlement(&state.pool, &account.id).map_err(internal)?;
    let current = jobs::list_tracks(&state.pool, &account.id).map_err(internal)?;
    enforce_track_limit(&input.track, &current, &entitlement)?;

    jobs::save_preferences(&state.pool, &account.id, &input.preferences).map_err(internal)?;
    jobs::upsert_track(&state.pool, &account.id, &input.track).map_err(internal)?;
    jobs::ensure_managed_curated_discovery_source(&state.pool, &account.id).map_err(internal)?;
    // Persist completion last. Retrying after any earlier write is idempotent,
    // while a partial request can never make the portal skip onboarding.
    jobs::save_profile(&state.pool, &account.id, &input.profile).map_err(internal)?;
    schedule_global_candidate_materialization(
        state.pool.clone(),
        account.id.clone(),
        account.email.clone(),
        "onboarding_complete",
    );
    jobs::workspace(&state.pool, &account.id, &account.email)
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveCareerFactRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub category: String,
    pub label: String,
    #[serde(default)]
    pub value: Value,
}

pub async fn save_fact(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(input): Json<SaveCareerFactRequest>,
) -> Result<Json<CareerFact>, ApiError> {
    if input.category.trim().is_empty() || input.label.trim().is_empty() {
        return bad_request("Choose a category and label for this fact.");
    }
    if input.category.len() > 80 || input.label.len() > 240 {
        return bad_request("That career fact is too long.");
    }
    jobs::upsert_user_fact(
        &state.pool,
        &account.id,
        input.id.as_deref(),
        input.category.trim(),
        input.label.trim(),
        input.value,
    )
    .map(Json)
    .map_err(domain_error)
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
    enforce_track_limit(&track, &current, &entitlement)?;
    let saved = jobs::upsert_track(&state.pool, &account.id, &track).map_err(internal)?;
    jobs::ensure_managed_curated_discovery_source(&state.pool, &account.id).map_err(internal)?;
    Ok(Json(saved))
}

pub async fn update_track(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(track_id): Path<String>,
    Json(mut track): Json<CareerTrack>,
) -> Result<Json<CareerTrack>, ApiError> {
    track.id = track_id;
    validate_track(&track)?;
    let current = jobs::list_tracks(&state.pool, &account.id).map_err(internal)?;
    if !current.iter().any(|item| item.id == track.id) {
        return Err((StatusCode::NOT_FOUND, "Career Track not found.".to_string()));
    }
    let saved = jobs::upsert_track(&state.pool, &account.id, &track).map_err(internal)?;
    jobs::ensure_managed_curated_discovery_source(&state.pool, &account.id).map_err(internal)?;
    Ok(Json(saved))
}

pub async fn delete_track(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(track_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    match jobs::delete_track(&state.pool, &account.id, &track_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, "Career Track not found.".to_string())),
        Err(error)
            if error
                .to_string()
                .contains("still has Jobs matches or discovery sources") =>
        {
            Err((
                StatusCode::CONFLICT,
                "Career Track cannot be deleted while it has Jobs matches or discovery sources."
                    .to_string(),
            ))
        }
        Err(error) => Err(internal(error)),
    }
}

pub async fn authorize_track_auto_submit(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(track_id): Path<String>,
) -> Result<Json<jobs::AutoSubmitAuthorization>, ApiError> {
    jobs::authorize_auto_submit(&state.pool, &account.id, &account.email, &track_id)
        .map(Json)
        .map_err(domain_error)
}

pub async fn revoke_track_auto_submit(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(track_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    match jobs::revoke_auto_submit(&state.pool, &account.id, &track_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            "Auto-submit is not enabled on this Career Track.".to_string(),
        )),
        Err(error) => Err(internal(error)),
    }
}

pub async fn matches(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<JobPosting>>, ApiError> {
    let applications = jobs::list_applications(&state.pool, &account.id).map_err(internal)?;
    let mut postings = jobs::list_postings(&state.pool, &account.id).map_err(internal)?;
    for posting in &mut postings {
        let existing_id = applications
            .iter()
            .find(|application| application.job_id == posting.id)
            .map(|application| application.id.as_str());
        posting.eligibility = Some(
            jobs::evaluate_job_eligibility(&state.pool, &account.id, posting, true, existing_id)
                .map_err(internal)?,
        );
    }
    Ok(Json(postings))
}

pub async fn match_detail(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(job_id): Path<String>,
) -> Result<Json<JobPosting>, ApiError> {
    let mut posting = jobs::get_posting(&state.pool, &account.id, &job_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Job match not found.".to_string()))?;
    let application = jobs::list_applications(&state.pool, &account.id)
        .map_err(internal)?
        .into_iter()
        .find(|application| application.job_id == posting.id);
    posting.eligibility = Some(
        jobs::evaluate_job_eligibility(
            &state.pool,
            &account.id,
            &posting,
            true,
            application
                .as_ref()
                .map(|application| application.id.as_str()),
        )
        .map_err(internal)?,
    );
    Ok(Json(posting))
}

pub async fn save_match(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(mut input): Json<UserJobInput>,
) -> Result<Json<JobPosting>, ApiError> {
    // Track ownership is a server-side authority check. Trim once so the
    // persisted match and its discovery source cannot disagree about a track.
    input.track_id = input.track_id.trim().to_string();
    let imported = if input.canonical_url.trim().is_empty() {
        None
    } else {
        jobs_import::import_supported_job(input.canonical_url.trim())
            .await
            .map_err(job_import_error)?
    };
    if imported.is_none() && (input.company.trim().is_empty() || input.title.trim().is_empty()) {
        return bad_request(
            "Bluey cannot import this job site automatically yet. Add the company and role to continue in Review mode.",
        );
    }
    let verified_import = imported.is_some();
    let posting = posting_from_user_input(input, imported);
    validate_posting(&posting)?;
    validate_match_track(&state.pool, &account.id, &posting.track_id)?;
    let discovery_source = if verified_import {
        let source = jobs::discovery_source_input_from_verified_import(
            &posting.source,
            &posting.canonical_url,
            &posting.company,
            &posting.track_id,
        )
        .map_err(discovery_source_validation_error)?;
        if let Some(source) = source.as_ref() {
            jobs::validate_discovery_source_input(&state.pool, &account.id, source)
                .map_err(discovery_source_validation_error)?;
        }
        source
    } else {
        None
    };
    let profile = jobs::get_profile(&state.pool, &account.id, &account.email).map_err(internal)?;
    let preferences = jobs::get_preferences(&state.pool, &account.id).map_err(internal)?;
    // The verified posting and its automatic board are one transaction. A
    // source conflict/failure rolls the posting back, and a posting failure
    // rolls the source back, so no invisible unscheduled state is created.
    let saved = match discovery_source.as_ref() {
        Some(source) => jobs::save_verified_import_posting_with_source(
            &state.pool,
            &account.id,
            &posting,
            source,
            &profile,
            &preferences,
        )
        .map_err(discovery_source_validation_error)?,
        None => jobs::upsert_posting(&state.pool, &account.id, &posting, &profile, &preferences)
            .map_err(internal)?,
    };
    Ok(Json(saved))
}

fn validate_match_track(
    pool: &crate::db::DbPool,
    account_id: &str,
    track_id: &str,
) -> Result<(), ApiError> {
    if track_id.is_empty() {
        return Ok(());
    }
    if jobs::list_tracks(pool, account_id)
        .map_err(internal)?
        .iter()
        .any(|track| track.id == track_id)
    {
        Ok(())
    } else {
        bad_request("Career Track was not found.")
    }
}

fn discovery_source_validation_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("discovery")
        || message.contains("Workday")
        || message.contains("verified job import")
        || message.contains("job is already bound")
    {
        (
            if message.contains("already bound") || message.contains("source limit") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            },
            message,
        )
    } else {
        internal(error)
    }
}

// Discovery enrollment is a retryable convenience after the verified match is
// durable. A discovery DB outage must not turn a successfully saved import into
// a false API failure; replaying the same import retries the idempotent upsert.
#[cfg(test)]
fn try_enroll_verified_import_discovery_source(
    pool: &crate::db::DbPool,
    account_id: &str,
    posting: &JobPosting,
) -> bool {
    match enroll_verified_import_discovery_source(pool, account_id, posting) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                account_fingerprint = %discovery_log_fingerprint(account_id),
                provider = discovery_provider_label(&posting.source),
                source_fingerprint = %discovery_log_fingerprint(&format!("{}\0{}", posting.source, posting.canonical_url)),
                error_category = discovery_enrollment_error_category(&error),
                "verified Jobs import was saved without scheduled discovery enrollment"
            );
            false
        }
    }
}

#[cfg(test)]
fn enroll_verified_import_discovery_source(
    pool: &crate::db::DbPool,
    account_id: &str,
    posting: &JobPosting,
) -> anyhow::Result<()> {
    let Some(source) = jobs::discovery_source_input_from_verified_import(
        &posting.source,
        &posting.canonical_url,
        &posting.company,
        &posting.track_id,
    )?
    else {
        return Ok(());
    };
    jobs::upsert_discovery_source(pool, account_id, &source)?;
    Ok(())
}

fn backfill_verified_import_discovery_sources(
    pool: &crate::db::DbPool,
    account_id: &str,
    postings: &[JobPosting],
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> usize {
    let tracks = match jobs::list_tracks(pool, account_id) {
        Ok(tracks) => tracks,
        Err(error) => {
            tracing::warn!(
                account_fingerprint = %discovery_log_fingerprint(account_id),
                error_category = discovery_enrollment_error_category(&error),
                "Jobs workspace skipped discovery source backfill because Career Tracks could not be read"
            );
            return 0;
        }
    };
    let valid_track_ids = tracks
        .into_iter()
        .map(|track| track.id)
        .collect::<BTreeSet<_>>();
    let existing_sources = match jobs::list_discovery_sources(pool, account_id) {
        Ok(sources) => sources,
        Err(error) => {
            tracing::warn!(
                account_fingerprint = %discovery_log_fingerprint(account_id),
                error_category = discovery_enrollment_error_category(&error),
                "Jobs workspace skipped discovery source backfill because sources could not be read"
            );
            return 0;
        }
    };
    let mut known_bindings = existing_sources
        .iter()
        .map(|source| {
            (
                source.provider.clone(),
                source.source_key.clone(),
                source.track_id.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut source_counts_by_track = BTreeMap::<String, usize>::new();
    for source in &existing_sources {
        *source_counts_by_track
            .entry(source.track_id.clone())
            .or_default() += 1;
    }
    let mut candidates = postings
        .iter()
        .filter(|posting| {
            posting.last_verified_at_ms.is_some() && posting.source.ends_with("_import")
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        (&left.source, &left.canonical_url, &left.track_id).cmp(&(
            &right.source,
            &right.canonical_url,
            &right.track_id,
        ))
    });
    let mut enrolled = 0;

    for posting in candidates
        .into_iter()
        .take(MAX_WORKSPACE_DISCOVERY_BACKFILL_POSTINGS)
    {
        let source = match jobs::discovery_source_input_from_verified_import(
            &posting.source,
            &posting.canonical_url,
            &posting.company,
            &posting.track_id,
        ) {
            Ok(Some(source)) => source,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(
                    account_fingerprint = %discovery_log_fingerprint(account_id),
                    provider = discovery_provider_label(&posting.source),
                    source_fingerprint = %discovery_log_fingerprint(&format!("{}\0{}", posting.source, posting.canonical_url)),
                    error_category = discovery_enrollment_error_category(&error),
                    "Jobs workspace skipped an invalid verified-import discovery source backfill"
                );
                continue;
            }
        };
        if !source.track_id.is_empty() && !valid_track_ids.contains(&source.track_id) {
            continue;
        }
        let binding = (
            source.provider.clone(),
            source.source_key.clone(),
            source.track_id.clone(),
        );
        let binding_exists = known_bindings.contains(&binding);
        match jobs::verified_import_discovery_membership_job_id(
            pool,
            account_id,
            &source.provider,
            &source.source_key,
            &posting.external_id,
        ) {
            Ok(Some(job_id)) if binding_exists && job_id == posting.id => continue,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    account_fingerprint = %discovery_log_fingerprint(account_id),
                    provider = discovery_provider_label(&posting.source),
                    source_fingerprint = %discovery_log_fingerprint(&format!("{}\0{}", posting.source, posting.canonical_url)),
                    error_category = discovery_enrollment_error_category(&error),
                    "Jobs workspace could not inspect a verified-import membership"
                );
                continue;
            }
        }
        if !binding_exists
            && (existing_sources.len() + enrolled >= jobs::DISCOVERY_MAX_SOURCES_PER_ACCOUNT
                || source_counts_by_track
                    .get(&source.track_id)
                    .copied()
                    .unwrap_or_default()
                    >= jobs::DISCOVERY_MAX_SOURCES_PER_TRACK)
        {
            continue;
        }
        match jobs::save_verified_import_posting_with_source(
            pool,
            account_id,
            posting,
            &source,
            profile,
            preferences,
        ) {
            Ok(_) => {
                if !binding_exists {
                    known_bindings.insert(binding);
                    *source_counts_by_track
                        .entry(source.track_id.clone())
                        .or_default() += 1;
                    enrolled += 1;
                }
            }
            Err(error) => tracing::warn!(
                account_fingerprint = %discovery_log_fingerprint(account_id),
                provider = discovery_provider_label(&posting.source),
                source_fingerprint = %discovery_log_fingerprint(&format!("{}\0{}", posting.source, posting.canonical_url)),
                error_category = discovery_enrollment_error_category(&error),
                "Jobs workspace could not backfill a verified-import discovery source"
            ),
        }
    }

    enrolled
}

fn discovery_log_fingerprint(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))[..16].to_string()
}

fn discovery_provider_label(source: &str) -> &'static str {
    match source {
        "greenhouse_import" => "greenhouse",
        "lever_import" => "lever",
        "ashby_import" => "ashby",
        "smartrecruiters_import" => "smartrecruiters",
        "workday_import" => "workday",
        _ => "unknown",
    }
}

fn discovery_enrollment_error_category(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("Career Track") {
        "invalid_track"
    } else if message.contains("configured provider") || message.contains("configured source") {
        "invalid_source"
    } else if message.contains("unsupported") || message.contains("public HTTPS") {
        "invalid_import"
    } else {
        "storage_failure"
    }
}

fn posting_from_user_input(
    input: UserJobInput,
    imported: Option<jobs_import::ImportedJob>,
) -> JobPosting {
    if let Some(imported) = imported {
        let provider = discovery_provider_label(&imported.source);
        let employer_id = format!(
            "{provider}:{}",
            imported
                .company
                .trim()
                .to_ascii_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("-")
        );
        let application_domain = reqwest::Url::parse(&imported.canonical_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string));
        let verified_at_ms = imported.verified_at_ms;
        let evidence_hash = imported.evidence_hash.clone();
        let mut posting = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: imported.source,
            external_id: imported.external_id,
            company: imported.company,
            title: imported.title,
            location: imported.location,
            workplace: imported.workplace,
            canonical_url: imported.canonical_url,
            description: imported.description,
            compensation: imported.compensation,
            employment_type: imported.employment_type,
            track_id: input.track_id,
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: imported.posted_at_ms,
            last_verified_at_ms: Some(verified_at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: jobs::JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        posting.canonical_key = jobs::canonical_job_key(&posting);
        posting.discovery_evidence = jobs::JobDiscoveryEvidence::provider_verified_original_source(
            posting.canonical_key.clone(),
            employer_id,
            application_domain,
            verified_at_ms,
            evidence_hash,
        );
        return posting;
    }
    JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "pasted_link".to_string(),
        external_id: String::new(),
        company: input.company,
        title: input.title,
        location: input.location,
        workplace: input.workplace,
        canonical_url: input.canonical_url,
        description: input.pasted_description,
        compensation: input.compensation,
        employment_type: String::new(),
        track_id: input.track_id,
        match_score: 0,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: None,
        last_verified_at_ms: None,
        availability_status: "unknown".to_string(),
        status: "matched".to_string(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: jobs::JobDiscoveryEvidence::default(),
        eligibility: None,
    }
}

#[derive(Debug, Deserialize)]
pub struct UserJobInput {
    pub canonical_url: String,
    #[serde(default)]
    pub pasted_description: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub workplace: String,
    #[serde(default)]
    pub compensation: String,
    #[serde(default)]
    pub track_id: String,
}

fn job_import_error(error: jobs_import::JobImportError) -> ApiError {
    match error {
        jobs_import::JobImportError::Invalid(message) => (StatusCode::BAD_REQUEST, message),
        jobs_import::JobImportError::NotFound(message) => (StatusCode::GONE, message),
        jobs_import::JobImportError::Temporary(message) => (StatusCode::BAD_GATEWAY, message),
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metering: Option<PacketCommitResult>,
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
    let onboarding_profile =
        jobs::get_profile(&state.pool, &account.id, &account.email).map_err(internal)?;
    if !onboarding_profile.onboarding_complete {
        return Err((
            StatusCode::CONFLICT,
            "Finish your Career Profile before preparing applications.".to_string(),
        ));
    }
    let prepared = jobs::prepare_application_draft(
        &state.pool,
        &account.id,
        &req.job_id,
        &req.mode,
        &req.submission_mode,
    )
    .map_err(internal)?;
    if !prepared.profile.onboarding_complete {
        return Err((
            StatusCode::CONFLICT,
            "Your Career Profile changed while Bluey prepared this application. Try again."
                .to_string(),
        ));
    }
    if req.submission_mode == "auto_submit" {
        jobs::require_valid_auto_submit_authorization(
            &state.pool,
            &account.id,
            &account.email,
            &prepared.posting.track_id,
        )
        .map_err(domain_error)?;
        let eligibility = prepared
            .application
            .receipt
            .get("eligibility")
            .cloned()
            .and_then(|value| serde_json::from_value::<JobEligibilityDecision>(value).ok())
            .ok_or((
                StatusCode::CONFLICT,
                "Auto-submit is unavailable because Bluey could not verify the current application-system and Career Track decision.".to_string(),
            ))?;
        let entitlement = jobs::get_entitlement(&state.pool, &account.id).map_err(internal)?;
        let runners = build_runner_availability(
            &entitlement,
            jobs_local_browser_distribution_enabled(),
            jobs_cloud_browser_distribution_enabled(),
        );
        if let Some(error) = auto_submit_request_error(&eligibility, &runners) {
            return Err(error);
        }
    }
    let generated = jobs_resume_generation::generate(
        &state,
        &account.id,
        &prepared.profile,
        &prepared.posting,
        &prepared.baseline_resume,
    )
    .await
    .map_err(|error| {
        if error
            .to_string()
            .contains("resume generation is already in progress")
        {
            (
                StatusCode::CONFLICT,
                "Bluey is already preparing this resume. Try again in a moment.".to_string(),
            )
        } else {
            internal(error)
        }
    })?;
    let jobs_resume_generation::GeneratedResume {
        content,
        diff,
        cover_letter,
        public_provenance,
    } = generated;
    let (mut application, resume_version) = jobs::finalize_prepared_application_kit(
        &state.pool,
        &account.id,
        &prepared,
        content,
        diff,
        cover_letter,
        public_provenance,
    )
    .map_err(internal)?;
    let mut metering = None;
    if application.state == "queued" {
        application = freeze_approved_execution(
            &state,
            &account.id,
            &account.email,
            &application,
            &prepared.posting,
            &resume_version,
        )?;
        if let Err(error) = jobs::reserve_application_attempt(
            &state.pool,
            &account.id,
            &application.id,
            "unassigned",
        ) {
            let _ = jobs::update_application(
                &state.pool,
                &account.id,
                &application.id,
                "awaiting_review",
                Some(&application.submission_mode),
            );
            application.state = "awaiting_review".to_string();
            return Err(domain_error(error));
        }
        match jobs::commit_packet(&state.pool, &account.id, &application.id) {
            Ok(result) => metering = Some(result),
            Err(error) => {
                let _ = jobs::update_attempt_reservation_status(
                    &state.pool,
                    &account.id,
                    &application.id,
                    "released",
                );
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
    }
    Ok(Json(PrepareApplicationResponse {
        application,
        resume_version,
        metering,
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
    if matches!(
        req.state.as_str(),
        "queued" | "running" | "needs_input" | "side_effect_unknown" | "submitted"
    ) {
        return Err((
            StatusCode::CONFLICT,
            "This application state is controlled by the verified runner workflow.".to_string(),
        ));
    }
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

#[derive(Debug, Serialize)]
pub struct ApproveApplicationResponse {
    pub application: JobApplication,
    pub metering: PacketCommitResult,
}

pub async fn approve_application_packet(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
) -> Result<Json<ApproveApplicationResponse>, ApiError> {
    let mut application = jobs::get_application(&state.pool, &account.id, &application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    if application.state != "awaiting_review" {
        return Err((
            StatusCode::CONFLICT,
            "Only a packet waiting for review can be approved.".to_string(),
        ));
    }
    let posting = jobs::get_posting(&state.pool, &account.id, &application.job_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
    let eligibility = jobs::evaluate_job_eligibility(
        &state.pool,
        &account.id,
        &posting,
        true,
        Some(&application.id),
    )
    .map_err(internal)?;
    if !eligibility.can_queue_local && !eligibility.can_queue_cloud {
        let message = eligibility
            .hard_failures
            .first()
            .or_else(|| eligibility.review_reasons.first())
            .map(|reason| reason.message.clone())
            .unwrap_or_else(|| "This packet cannot be queued yet.".to_string());
        return Err((StatusCode::CONFLICT, message));
    }
    let resume_id = application.resume_version_id.clone().ok_or((
        StatusCode::CONFLICT,
        "Create the tailored resume before approving this packet.".to_string(),
    ))?;
    let resume = jobs::get_resume_version(&state.pool, &account.id, &resume_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::CONFLICT,
            "Tailored resume not found.".to_string(),
        ))?;
    application = freeze_approved_execution(
        &state,
        &account.id,
        &account.email,
        &application,
        &posting,
        &resume,
    )?;
    jobs::reserve_application_attempt(&state.pool, &account.id, &application.id, "unassigned")
        .map_err(domain_error)?;
    let metering =
        jobs::commit_packet(&state.pool, &account.id, &application.id).map_err(|error| {
            let _ = jobs::update_attempt_reservation_status(
                &state.pool,
                &account.id,
                &application.id,
                "released",
            );
            if error.to_string().contains("insufficient Bluey balance") {
                (
                    StatusCode::PAYMENT_REQUIRED,
                    "Add to your Bluey balance before approving this packet.".to_string(),
                )
            } else {
                internal(error)
            }
        })?;
    let application = jobs::update_application(
        &state.pool,
        &account.id,
        &application.id,
        "queued",
        Some(&application.submission_mode),
    )
    .map_err(|error| {
        let _ = jobs::update_attempt_reservation_status(
            &state.pool,
            &account.id,
            &application.id,
            "released",
        );
        domain_error(error)
    })?
    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    Ok(Json(ApproveApplicationResponse {
        application,
        metering,
    }))
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
    let mut application = jobs::get_application(&state.pool, &account.id, &application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    if application.state == "awaiting_review" {
        return Err((
            StatusCode::CONFLICT,
            "Approve this application before choosing a browser runner.".to_string(),
        ));
    }
    if !matches!(
        application.state.as_str(),
        "queued" | "running" | "needs_input"
    ) {
        return Err((
            StatusCode::CONFLICT,
            "Only queued applications can start a browser runner.".to_string(),
        ));
    }
    let entitlement = jobs::get_entitlement(&state.pool, &account.id).map_err(internal)?;
    let runners = build_runner_availability(
        &entitlement,
        jobs_local_browser_distribution_enabled(),
        jobs_cloud_browser_distribution_enabled(),
    );
    let channel = if req.runner == "local" {
        &runners.local
    } else {
        &runners.cloud
    };
    if !channel.available {
        let status = if channel.plan_included {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::PAYMENT_REQUIRED
        };
        return Err((
            status,
            format!("{} {}", channel.reason, channel.next_action),
        ));
    }
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
    let eligibility = jobs::evaluate_job_eligibility(
        &state.pool,
        &account.id,
        &posting,
        true,
        Some(&application.id),
    )
    .map_err(internal)?;
    let runner_allowed = if req.runner == "cloud" {
        eligibility.can_queue_cloud
    } else {
        eligibility.can_queue_local
    };
    if !runner_allowed {
        let message = eligibility
            .hard_failures
            .first()
            .or_else(|| eligibility.review_reasons.first())
            .map(|reason| reason.message.clone())
            .unwrap_or_else(|| "This site is not eligible for that runner.".to_string());
        return Err((StatusCode::CONFLICT, message));
    }
    let (identity_id, identity_email) = approved_application_identity(&application)?;
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
    if req.runner == "cloud" && (source == "semantic" || posting.source.ends_with("_handoff")) {
        return Err((
            StatusCode::CONFLICT,
            "This listing needs a reviewed handoff before cloud automation.".to_string(),
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
    let cloud_gateway = if req.runner == "cloud" {
        let origin = std::env::var("BLUEY_JOBS_WORKFLOW_ORIGIN")
            .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
        let token = std::env::var("BLUEY_JOBS_WORKFLOW_TOKEN").unwrap_or_default();
        if token.is_empty() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "The cloud runner is temporarily unavailable.".to_string(),
            ));
        }
        Some((origin, token))
    } else {
        None
    };
    jobs::reserve_application_attempt(&state.pool, &account.id, &application.id, &req.runner)
        .map_err(domain_error)?;
    jobs::commit_packet(&state.pool, &account.id, &application.id).map_err(|error| {
        let _ = jobs::update_attempt_reservation_status(
            &state.pool,
            &account.id,
            &application.id,
            "released",
        );
        domain_error(error)
    })?;
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

    let (mut frozen_packet, frozen_job, approved_packet_checksum) =
        approved_execution_snapshot(&application)?;
    validate_approved_execution_matches(
        &application,
        &posting,
        &resume,
        &identity_id,
        &identity_email,
        &frozen_packet,
        &frozen_job,
    )?;
    let browser_profile_id = browser_profile_id(&account.id, &identity_id);
    frozen_packet
        .as_object_mut()
        .expect("approved packet is an object")
        .insert(
            "approvedPacketChecksum".to_string(),
            Value::String(approved_packet_checksum),
        );
    let workflow_input = json!({
        "accountId": account.id,
        "applicationId": application.id,
        "jobId": posting.id,
        "canonicalJobKey": posting.canonical_key,
        "packetId": resume.id,
        "applicationIdentityId": identity_id,
        "browserProfileId": browser_profile_id,
        "packet": frozen_packet,
        "job": frozen_job,
        "runner": req.runner,
        "url": posting.canonical_url,
        "idempotencyKey": run_id,
        "runId": run_id,
        "browserSessionId": format!("{}-{}", req.runner, application.id),
    });
    if let Some((gateway_origin, gateway_token)) = cloud_gateway.as_ref() {
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
    }

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

fn browser_profile_id(account_id: &str, identity_id: &str) -> String {
    jobs::execution_browser_profile_id(account_id, identity_id)
}

fn approved_application_identity(
    application: &JobApplication,
) -> Result<(String, String), ApiError> {
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
        .trim()
        .to_string();
    let identity_email = identity
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if identity_id.is_empty() || identity_email.is_empty() {
        return Err((
            StatusCode::CONFLICT,
            "Choose and verify the application email before starting.".to_string(),
        ));
    }
    Ok((identity_id, identity_email))
}

fn freeze_approved_execution(
    state: &AppState,
    account_id: &str,
    account_email: &str,
    application: &JobApplication,
    posting: &JobPosting,
    resume: &ResumeVersion,
) -> Result<JobApplication, ApiError> {
    let (identity_id, identity_email) = approved_application_identity(application)?;
    if application.receipt.get("approved_execution").is_some() {
        let (packet, job, _) = approved_execution_snapshot(application)?;
        validate_approved_execution_matches(
            application,
            posting,
            resume,
            &identity_id,
            &identity_email,
            &packet,
            &job,
        )?;
        return Ok(application.clone());
    }

    let profile = jobs::get_profile(&state.pool, account_id, account_email).map_err(internal)?;
    let answers = execution_answers(&profile, application, &identity_email);
    let verified_claim_ids = confirmed_resume_claim_ids(state, account_id, resume)?;
    let packet = json!({
        "applicationId": application.id,
        "jobId": posting.id,
        "resumeVersionId": resume.id,
        "resumeContent": resume.content,
        "coverLetterContent": application.cover_letter,
        "answers": answers,
        "verifiedClaimIds": verified_claim_ids,
        "applicationIdentityId": identity_id,
        "applicationEmail": identity_email,
        "browserProfileId": browser_profile_id(account_id, &identity_id),
    });
    let job = json!({
        "externalId": posting.external_id,
        "canonicalUrl": posting.canonical_url,
        "company": posting.company,
        "title": posting.title,
        "location": posting.location,
        "workplace": normalized_workplace(&posting.workplace),
        "description": posting.description,
        "source": ats_kind(&posting.canonical_url),
        "compensation": posting.compensation,
    });
    let admission = if application.submission_mode == "auto_submit" {
        let authorization = jobs::require_valid_auto_submit_authorization(
            &state.pool,
            account_id,
            account_email,
            &posting.track_id,
        )
        .map_err(domain_error)?;
        json!({
            "kind": "track_auto_submit",
            "authorization_id": authorization.id,
            "career_track_id": authorization.career_track_id,
            "revision_no": authorization.revision_no,
            "authority_fingerprint": authorization.authority_fingerprint,
        })
    } else {
        json!({
            "kind": "review_approval",
        })
    };
    let checksum = approved_execution_checksum_v2(&packet, &job, &admission)?;
    let approved_execution = json!({
        "schema_version": 2,
        "approved_at_ms": jobs::now_ms(),
        "checksum": checksum,
        "admission": admission,
        "packet": packet,
        "job": job,
    });
    let mut receipt = application.receipt.clone();
    if !receipt.is_object() {
        receipt = json!({});
    }
    receipt
        .as_object_mut()
        .expect("application receipt is an object")
        .insert("approved_execution".to_string(), approved_execution);
    jobs::replace_application_receipt(&state.pool, account_id, &application.id, receipt)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))
}

fn approved_execution_snapshot(
    application: &JobApplication,
) -> Result<(Value, Value, String), ApiError> {
    let approved = application
        .receipt
        .get("approved_execution")
        .and_then(Value::as_object)
        .ok_or((
            StatusCode::CONFLICT,
            "Approve this exact application packet before starting a browser runner.".to_string(),
        ))?;
    let schema_version = approved
        .get("schema_version")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if !matches!(schema_version, 1 | 2) {
        return Err((
            StatusCode::CONFLICT,
            "This approved packet uses an unsupported version. Prepare it again.".to_string(),
        ));
    }
    let packet = approved
        .get("packet")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or((
            StatusCode::CONFLICT,
            "The approved application packet is incomplete. Prepare it again.".to_string(),
        ))?;
    let job = approved
        .get("job")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or((
            StatusCode::CONFLICT,
            "The approved job snapshot is incomplete. Prepare it again.".to_string(),
        ))?;
    let checksum = approved
        .get("checksum")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let expected_checksum = if schema_version == 1 {
        if application.submission_mode == "auto_submit" {
            return Err((
                StatusCode::CONFLICT,
                "Review and enable Auto-submit on this Career Track before starting.".to_string(),
            ));
        }
        approved_execution_checksum(&packet, &job)?
    } else {
        let admission = approved
            .get("admission")
            .filter(|value| value.is_object())
            .ok_or((
                StatusCode::CONFLICT,
                "The application approval proof is incomplete. Prepare it again.".to_string(),
            ))?;
        validate_approved_execution_admission(application, admission)?;
        approved_execution_checksum_v2(&packet, &job, admission)?
    };
    if checksum.len() != 64 || expected_checksum != checksum {
        return Err((
            StatusCode::CONFLICT,
            "The approved application packet changed after review. Prepare it again.".to_string(),
        ));
    }
    Ok((packet, job, checksum))
}

fn approved_execution_checksum(packet: &Value, job: &Value) -> Result<String, ApiError> {
    let canonical = canonical_json_value(&json!({
        "schema_version": 1,
        "packet": packet,
        "job": job,
    }));
    let bytes = serde_json::to_vec(&canonical).map_err(|error| internal(error.into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn approved_execution_checksum_v2(
    packet: &Value,
    job: &Value,
    admission: &Value,
) -> Result<String, ApiError> {
    let canonical = canonical_json_value(&json!({
        "schema_version": 2,
        "admission": admission,
        "packet": packet,
        "job": job,
    }));
    let bytes = serde_json::to_vec(&canonical).map_err(|error| internal(error.into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_approved_execution_admission(
    application: &JobApplication,
    admission: &Value,
) -> Result<(), ApiError> {
    let kind = admission
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if application.submission_mode == "auto_submit" {
        let complete = kind == "track_auto_submit"
            && admission
                .get("authorization_id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            && admission
                .get("career_track_id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            && admission
                .get("revision_no")
                .and_then(Value::as_i64)
                .is_some_and(|value| value > 0)
            && admission
                .get("authority_fingerprint")
                .and_then(Value::as_str)
                .is_some_and(|value| value.len() == 64);
        if !complete {
            return Err((
                StatusCode::CONFLICT,
                "The Auto-submit authorization proof is incomplete. Review the Career Track again."
                    .to_string(),
            ));
        }
    } else if kind != "review_approval" {
        return Err((
            StatusCode::CONFLICT,
            "Approve this exact application packet before starting a browser runner.".to_string(),
        ));
    }
    Ok(())
}

fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical_json_value).collect()),
        Value::Object(values) => {
            let sorted = values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        other => other.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_approved_execution_matches(
    application: &JobApplication,
    posting: &JobPosting,
    resume: &ResumeVersion,
    identity_id: &str,
    identity_email: &str,
    packet: &Value,
    job: &Value,
) -> Result<(), ApiError> {
    let matches = packet.get("applicationId").and_then(Value::as_str)
        == Some(application.id.as_str())
        && packet.get("jobId").and_then(Value::as_str) == Some(posting.id.as_str())
        && packet.get("resumeVersionId").and_then(Value::as_str) == Some(resume.id.as_str())
        && packet.get("applicationIdentityId").and_then(Value::as_str) == Some(identity_id)
        && packet.get("applicationEmail").and_then(Value::as_str) == Some(identity_email)
        && job.get("canonicalUrl").and_then(Value::as_str) == Some(posting.canonical_url.as_str());
    if !matches {
        return Err((
            StatusCode::CONFLICT,
            "The approved packet no longer matches this job, resume, or application email. Prepare it again."
                .to_string(),
        ));
    }
    Ok(())
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
    let runners = build_runner_availability(
        &entitlement,
        jobs_local_browser_distribution_enabled(),
        jobs_cloud_browser_distribution_enabled(),
    );
    let channel = if session.runner == "local" {
        &runners.local
    } else {
        &runners.cloud
    };
    if !channel.available {
        let status = if channel.plan_included {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::PAYMENT_REQUIRED
        };
        return Err((
            status,
            format!("{} {}", channel.reason, channel.next_action),
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
    Json(mut intervention): Json<Intervention>,
) -> Result<Json<Intervention>, ApiError> {
    if let Some(metadata) = intervention.metadata.as_object_mut() {
        metadata.remove(TRUSTED_WORKER_RECEIPT_KEY);
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_resume: Option<jobs::LocalRunResumeAction>,
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
    let original_status = updated.status.clone();
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
    } else if action == "approve_submission" {
        if original_status != "open" {
            return Err((
                StatusCode::CONFLICT,
                "This intervention has already been resolved.".to_string(),
            ));
        }
        if !req.answer.trim().is_empty()
            || req.remember
            || !req.scope.trim().is_empty()
            || req.scope_id.is_some()
        {
            return bad_request("Submission approval cannot include answer data.");
        }
        let application_id = updated.application_id.as_deref().ok_or((
            StatusCode::BAD_REQUEST,
            "This intervention is not attached to an application.".to_string(),
        ))?;
        let application = jobs::get_application(&state.pool, &account.id, application_id)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
        if application.state != "needs_input" {
            return Err((
                StatusCode::CONFLICT,
                "This application is no longer waiting for final review.".to_string(),
            ));
        }
        let posting = jobs::get_posting(&state.pool, &account.id, &application.job_id)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
        if !is_provider_final_review_intervention(&updated, &posting) {
            return bad_request("This intervention is not an approvable final review.");
        }
        updated.status = "approved".to_string();
        let approved_at_ms = jobs::now_ms();
        let metadata = updated.metadata.as_object_mut().ok_or((
            StatusCode::BAD_REQUEST,
            "This intervention is not an approvable final review.".to_string(),
        ))?;
        metadata.insert(
            "approved_at_ms".to_string(),
            serde_json::json!(approved_at_ms),
        );
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
    let mut saved = if action == "approve_submission" {
        Some(jobs::save_intervention(&state.pool, &account.id, &updated).map_err(internal)?)
    } else {
        None
    };
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
    let mut saved = match saved.take() {
        Some(saved) => saved,
        None => jobs::save_intervention(&state.pool, &account.id, &updated).map_err(internal)?,
    };
    let mut local_resume = None;
    if let Some(application) = resumed_application.as_ref() {
        if let Some(run_id) = application.run_id.as_deref() {
            let field = saved
                .metadata
                .pointer("/receipt/intervention/field")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let local_session = if action == "approve_submission" {
                jobs::list_browser_sessions(&state.pool, &account.id)
                    .map_err(internal)?
                    .into_iter()
                    .find(|session| session.id == run_id && session.runner == "local")
            } else {
                None
            };
            if local_session.is_some() {
                let approved = jobs::approve_local_run_resume_action(
                    &state.pool,
                    &account.id,
                    &application.id,
                    run_id,
                    &saved.id,
                )
                .map_err(internal)?;
                let Some(approved) = approved else {
                    saved.status = "open".to_string();
                    jobs::save_intervention(&state.pool, &account.id, &saved).map_err(internal)?;
                    let _ = jobs::update_application(
                        &state.pool,
                        &account.id,
                        &application.id,
                        "needs_input",
                        None,
                    );
                    return Err((
                        StatusCode::CONFLICT,
                        "The local browser run is not waiting for this approval.".to_string(),
                    ));
                };
                local_resume = Some(approved);
            } else {
                let answer = if action == "approve_submission" {
                    ""
                } else {
                    req.answer.trim()
                };
                if let Err(error) =
                    signal_workflow_resume(&account.id, run_id, &action, field, answer).await
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
    }
    Ok(Json(InterventionResolutionResult {
        intervention: saved,
        answer_memory: remembered_answer,
        application: resumed_application,
        local_resume,
    }))
}

fn is_provider_final_review_intervention(
    intervention: &Intervention,
    posting: &JobPosting,
) -> bool {
    if intervention.kind != "browser_takeover"
        || intervention.resolution_kind != "browser_takeover"
        || !intervention.resume_after_resolution
        || !intervention.choices.is_empty()
    {
        return false;
    }
    if intervention
        .metadata
        .get(TRUSTED_WORKER_RECEIPT_KEY)
        .and_then(Value::as_bool)
        != Some(true)
    {
        return false;
    }
    let Some(receipt) = intervention.metadata.get("receipt") else {
        return false;
    };
    if receipt.get("status").and_then(Value::as_str) != Some("needs_input")
        || !receipt
            .get("issues")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return false;
    }
    let Some(source) = receipt.get("intervention").and_then(Value::as_object) else {
        return false;
    };
    let Some(resolution) = source.get("resolution").and_then(Value::as_object) else {
        return false;
    };
    if source.get("kind").and_then(Value::as_str) != Some("browser_takeover")
        || source
            .get("takeoverUrl")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || resolution.get("kind").and_then(Value::as_str) != Some("browser_takeover")
        || resolution.get("resumeAfter").and_then(Value::as_bool) != Some(true)
    {
        return false;
    }
    let title = source.get("title").and_then(Value::as_str);
    let detail = source.get("detail").and_then(Value::as_str);
    if title != Some(intervention.title.as_str()) || detail != Some(intervention.detail.as_str()) {
        return false;
    }
    match ats_kind(&posting.canonical_url) {
        "greenhouse" => {
            title == Some("Review the Greenhouse application")
                && detail
                    == Some(
                        "Review every employer-facing field and document in the preserved form, then approve submission.",
                    )
        }
        "lever" => {
            title == Some("Review this Lever application")
                && detail
                    == Some(
                        "Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review.",
                    )
        }
        _ => false,
    }
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateEventRequest {
    pub event_type: String,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub application_id: Option<String>,
    pub action: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub note: String,
}

pub async fn candidate_events(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<CandidateEvent>>, ApiError> {
    jobs::list_candidate_events(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

pub async fn save_candidate_event(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(input): Json<CandidateEventRequest>,
) -> Result<Json<CandidateEvent>, ApiError> {
    jobs::save_candidate_event(
        &state.pool,
        &account.id,
        &CandidateEvent {
            id: String::new(),
            event_type: input.event_type,
            job_id: input.job_id,
            application_id: input.application_id,
            action: input.action,
            reasons: input.reasons,
            note: input.note,
            status: String::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .map(Json)
    .map_err(domain_error)
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

pub async fn upsert_account_discovery_source(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    Json(input): Json<jobs::DiscoverySourceInput>,
) -> Result<Json<jobs::DiscoverySource>, ApiError> {
    jobs::upsert_discovery_source(&state.pool, &account_id, &input)
        .map(Json)
        .map_err(discovery_domain_error)
}

#[derive(Debug, Deserialize)]
pub struct SetDiscoverySourceStatusRequest {
    pub status: String,
}

pub async fn set_account_discovery_source_status(
    State(state): State<AppState>,
    Path((account_id, source_id)): Path<(String, String)>,
    Json(req): Json<SetDiscoverySourceStatusRequest>,
) -> Result<Json<jobs::DiscoverySource>, ApiError> {
    jobs::set_discovery_source_status(&state.pool, &account_id, &source_id, &req.status)
        .map_err(discovery_domain_error)?
        .map(Json)
        .ok_or((
            StatusCode::NOT_FOUND,
            "Discovery source not found.".to_string(),
        ))
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
struct LocalRunClaimRequest {
    ticket: String,
}

#[derive(Debug, Deserialize)]
struct LocalRunAccessRequest {
    #[serde(default)]
    capability: String,
    #[serde(default)]
    ticket: String,
}

#[derive(Debug, Deserialize)]
struct LocalRunResultRequest {
    #[serde(default)]
    capability: String,
    #[serde(default)]
    ticket: String,
    receipt: Value,
    #[serde(default, rename = "receiptBundle")]
    receipt_bundle: Option<Value>,
    #[serde(default, rename = "evidenceObjects")]
    evidence_objects: Vec<ReceiptEvidenceObject>,
}

async fn claim_local_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunClaimRequest>,
) -> Result<Json<Value>, ApiError> {
    if !jobs_local_browser_distribution_enabled() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser local runs are currently paused.".to_string(),
        ));
    }
    let hash = local_run_ticket_hash(&req.ticket)?;
    let ticket = jobs::claim_authorized_local_run_ticket(&state.pool, &run_id, &hash)
        .map_err(internal)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ))?;
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
    let browser_profile_id = ticket
        .payload
        .get("browserProfileId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or((
            StatusCode::CONFLICT,
            "This Bluey Browser launch is missing its browser profile.".to_string(),
        ))?;
    let result_capability = super::jobs_local_capability::issue(
        &ticket.account_id,
        &ticket.application_id,
        &run_id,
        browser_profile_id,
        "result",
        ticket.expires_at_ms,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "could not issue local result capability");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser could not start securely. Try again.".to_string(),
        )
    })?;
    let resume_capability = super::jobs_local_capability::issue(
        &ticket.account_id,
        &ticket.application_id,
        &run_id,
        browser_profile_id,
        "resume",
        ticket.expires_at_ms,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "could not issue local resume capability");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser could not start securely. Try again.".to_string(),
        )
    })?;
    let submit_capability = super::jobs_local_capability::issue(
        &ticket.account_id,
        &ticket.application_id,
        &run_id,
        browser_profile_id,
        "submit",
        ticket.expires_at_ms,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "could not issue local submit capability");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser could not start securely. Try again.".to_string(),
        )
    })?;
    let mut payload = ticket.payload.as_object().cloned().ok_or((
        StatusCode::CONFLICT,
        "This Bluey Browser launch is invalid.".to_string(),
    ))?;
    payload.insert(
        "_blueyCapabilities".to_string(),
        json!({
            "result": result_capability,
            "resume": resume_capability,
            "submit": submit_capability,
            "expiresAtMs": ticket.expires_at_ms,
        }),
    );
    Ok(Json(Value::Object(payload)))
}

async fn authorize_local_run_submit(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunAccessRequest>,
) -> Result<Json<Value>, ApiError> {
    if !jobs_local_browser_distribution_enabled() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser local runs are currently paused.".to_string(),
        ));
    }
    let ticket =
        authorize_local_run_operation(&state, &run_id, &req.capability, &req.ticket, "submit")?;
    if !jobs::local_run_submit_authorized(&state.pool, &run_id, &ticket.ticket_hash)
        .map_err(internal)?
    {
        return Err((
            StatusCode::CONFLICT,
            "This application is no longer authorized to submit. Return to Bluey Jobs to review it."
                .to_string(),
        ));
    }
    let (application, _) = local_result_binding(&state, &ticket, &run_id)?;
    let posting = jobs::get_posting(&state.pool, &ticket.account_id, &application.job_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
    if matches!(ats_kind(&posting.canonical_url), "greenhouse" | "lever")
        && !jobs::local_submission_approval_consumed(
            &state.pool,
            &ticket.account_id,
            &ticket.application_id,
            &run_id,
        )
        .map_err(internal)?
    {
        return Err((
            StatusCode::CONFLICT,
            "Final submission has not been approved for this local run.".to_string(),
        ));
    }
    Ok(Json(json!({
        "authorized": true,
        "authorizedAtMs": jobs::now_ms(),
    })))
}

async fn consume_local_run_resume(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunAccessRequest>,
) -> Result<Json<jobs::LocalRunResumeAction>, ApiError> {
    let ticket =
        authorize_local_run_operation(&state, &run_id, &req.capability, &req.ticket, "resume")?;
    let action = jobs::consume_local_run_resume_action(&state.pool, &run_id, &ticket.ticket_hash)
        .map_err(internal)?
        .ok_or((
            StatusCode::CONFLICT,
            "This local browser resume has not been approved.".to_string(),
        ))?;
    jobs::update_application(
        &state.pool,
        &action.account_id,
        &action.application_id,
        "running",
        None,
    )
    .map_err(domain_error)?
    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    update_worker_browser_session(
        &state,
        &action.account_id,
        &action.application_id,
        "running",
    )?;
    if action.first_consumption {
        jobs::save_run_event(
            &state.pool,
            &action.account_id,
            &run_id,
            "local_resume_approval_consumed",
            json!({
                "application_id": action.application_id,
                "intervention_id": action.intervention_id,
                "action": action.action,
            }),
        )
        .map_err(internal)?;
    }
    Ok(Json(action))
}

async fn save_local_run_result(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunResultRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    let ticket =
        authorize_local_run_operation(&state, &run_id, &req.capability, &req.ticket, "result")?;
    if ticket.expires_at_ms <= jobs::now_ms() {
        return Err((
            StatusCode::GONE,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ));
    }
    let status = req
        .receipt
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let (bound_application, bound_session) = local_result_binding(&state, &ticket, &run_id)?;
    match ticket.status.as_str() {
        "failed" if status == "failed" && bound_application.state == "failed" => {
            return Ok(Json(bound_application));
        }
        "side_effect_unknown"
            if status == "side_effect_unknown"
                && bound_application.state == "side_effect_unknown" =>
        {
            return Ok(Json(bound_application));
        }
        "complete" if status != "submitted" => {
            return Err((
                StatusCode::CONFLICT,
                "This local run already has a different terminal result.".to_string(),
            ));
        }
        "failed" | "side_effect_unknown" => {
            return Err((
                StatusCode::CONFLICT,
                "This local run already has a different terminal result.".to_string(),
            ));
        }
        _ => {}
    }
    let application = match status.as_str() {
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
            jobs::update_local_run_ticket_status(
                &state.pool,
                &run_id,
                &ticket.ticket_hash,
                "needs_input",
            )
            .map_err(internal)?;
            application
        }
        "submitted" => {
            let application =
                jobs::get_application(&state.pool, &ticket.account_id, &ticket.application_id)
                    .map_err(internal)?
                    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
            let posting = jobs::get_posting(&state.pool, &ticket.account_id, &application.job_id)
                .map_err(internal)?
                .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
            if matches!(ats_kind(&posting.canonical_url), "greenhouse" | "lever")
                && !jobs::local_submission_approval_consumed(
                    &state.pool,
                    &ticket.account_id,
                    &ticket.application_id,
                    &run_id,
                )
                .map_err(internal)?
            {
                return Err((
                    StatusCode::CONFLICT,
                    "Final submission has not been approved for this local run.".to_string(),
                ));
            }
            let bundle = req.receipt_bundle.ok_or((
                StatusCode::BAD_REQUEST,
                "The local browser did not return its submission receipt.".to_string(),
            ))?;
            let application = persist_submission_receipt(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                bundle,
                req.evidence_objects,
                "local",
                Some(&ticket.ticket_hash),
            )
            .await?;
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
            jobs::update_local_run_ticket_status(
                &state.pool,
                &run_id,
                &ticket.ticket_hash,
                "failed",
            )
            .map_err(internal)?;
            application
        }
        "side_effect_unknown" => {
            let mut reconciliation_receipt = req.receipt;
            if let Some(receipt) = reconciliation_receipt.as_object_mut() {
                receipt.remove("screenshotPath");
            }
            let mut session = bound_session;
            session.status = "needs_input".to_string();
            session.current_step = "Submission outcome needs reconciliation".to_string();
            if let Some(takeover_url) = reconciliation_receipt
                .pointer("/intervention/takeoverUrl")
                .and_then(Value::as_str)
                .filter(|url| url.starts_with("https://") || url.starts_with("bluey-jobs://"))
            {
                session.takeover_url = Some(takeover_url.to_string());
            }
            jobs::finalize_local_side_effect_unknown(
                &state.pool,
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                &ticket.ticket_hash,
                reconciliation_receipt,
                &session,
            )
            .map_err(submission_domain_error)?
        }
        _ => return bad_request("Bluey Browser returned an invalid application result."),
    };
    Ok(Json(application))
}

fn local_result_binding(
    state: &AppState,
    ticket: &jobs::LocalRunTicket,
    run_id: &str,
) -> Result<(JobApplication, BrowserSession), ApiError> {
    if ticket.id != run_id {
        return Err((
            StatusCode::CONFLICT,
            "Local run ticket does not match this browser run.".to_string(),
        ));
    }
    let application =
        jobs::get_application(&state.pool, &ticket.account_id, &ticket.application_id)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    if application.run_id.as_deref() != Some(run_id) {
        return Err((
            StatusCode::CONFLICT,
            "Local run ticket does not match this application.".to_string(),
        ));
    }
    let session = jobs::list_browser_sessions(&state.pool, &ticket.account_id)
        .map_err(internal)?
        .into_iter()
        .find(|session| session.id == run_id)
        .filter(|session| {
            session.runner == "local"
                && session.application_id.as_deref() == Some(ticket.application_id.as_str())
        })
        .ok_or((
            StatusCode::CONFLICT,
            "Local run ticket does not match an active local browser session.".to_string(),
        ))?;
    Ok((application, session))
}

fn authorize_local_run_operation(
    state: &AppState,
    run_id: &str,
    capability: &str,
    _legacy_ticket: &str,
    operation: &str,
) -> Result<jobs::LocalRunTicket, ApiError> {
    #[cfg(debug_assertions)]
    if capability.is_empty() && !_legacy_ticket.is_empty() {
        let hash = local_run_ticket_hash(_legacy_ticket)?;
        return jobs::get_local_run_ticket_by_hash(&state.pool, run_id, &hash)
            .map_err(internal)?
            .ok_or((
                StatusCode::NOT_FOUND,
                "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
            ));
    }

    let claims =
        super::jobs_local_capability::verify(capability, run_id, operation, jobs::now_ms())
            .map_err(|_| {
                (
                    StatusCode::NOT_FOUND,
                    "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
                )
            })?;
    let ticket = jobs::get_local_run_ticket(&state.pool, &claims.account_id, run_id)
        .map_err(internal)?
        .filter(|ticket| {
            ticket.application_id == claims.application_id
                && ticket.expires_at_ms == claims.expires_at_ms
                && ticket
                    .payload
                    .get("browserProfileId")
                    .and_then(Value::as_str)
                    == Some(claims.browser_profile_id.as_str())
        })
        .ok_or((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ))?;
    Ok(ticket)
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
struct WorkerExecutionLeaseClaimRequest {
    account_id: String,
    application_id: String,
    run_id: String,
    browser_profile_id: String,
    owner_id: String,
}

#[derive(Debug, Deserialize)]
struct WorkerExecutionLeaseAccessRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
}

#[derive(Debug, Deserialize)]
struct WorkerIrreversibleExecutionRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
    action: String,
}

#[derive(Debug, Deserialize)]
struct WorkerFinishExecutionRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
    outcome: String,
}

async fn worker_claim_execution_lease(
    State(state): State<AppState>,
    Json(req): Json<WorkerExecutionLeaseClaimRequest>,
) -> Result<Json<jobs::ExecutionLeaseGrant>, ApiError> {
    if req.run_id.trim().is_empty() {
        return bad_request("Invalid execution lease request.");
    }
    jobs::claim_execution_lease(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &req.run_id,
        &req.browser_profile_id,
        &req.owner_id,
    )
    .map(Json)
    .map_err(execution_lease_error)
}

async fn worker_heartbeat_execution_lease(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerExecutionLeaseAccessRequest>,
) -> Result<Json<jobs::ExecutionLeaseRecord>, ApiError> {
    jobs::heartbeat_execution_lease(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.lease_token,
        req.fence,
    )
    .map(Json)
    .map_err(execution_lease_error)
}

async fn worker_start_irreversible_submission(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerIrreversibleExecutionRequest>,
) -> Result<Json<jobs::ExecutionLeaseRecord>, ApiError> {
    if req.action != "submit" {
        return bad_request("Invalid irreversible execution action.");
    }
    jobs::start_irreversible_submission(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.lease_token,
        req.fence,
    )
    .map(Json)
    .map_err(execution_lease_error)
}

async fn worker_finish_execution_lease(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerFinishExecutionRequest>,
) -> Result<StatusCode, ApiError> {
    jobs::finish_execution_lease(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.lease_token,
        req.fence,
        &req.outcome,
    )
    .map(|()| StatusCode::NO_CONTENT)
    .map_err(execution_lease_error)
}

#[derive(Debug, Deserialize)]
struct WorkerDiscoveryCompleteRequest {
    lease_token: String,
    replay_key: String,
    scheduled_for_ms: i64,
    jobs: Vec<jobs::DiscoveredJobInput>,
    #[serde(default)]
    complete_snapshot: bool,
}

#[derive(Debug, Deserialize)]
struct WorkerDiscoveryFailRequest {
    lease_token: String,
    replay_key: String,
    scheduled_for_ms: i64,
    error_code: String,
}

async fn worker_discovery_lease(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let worker_id = headers
        .get("x-bluey-jobs-worker-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("jobs-discovery-worker");
    match jobs::lease_due_discovery_source(&state.pool, worker_id)
        .map_err(discovery_domain_error)?
    {
        Some(lease) => Ok(Json(lease).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

async fn worker_discovery_complete(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<WorkerDiscoveryCompleteRequest>,
) -> Result<Json<jobs::DiscoveryRunResult>, ApiError> {
    jobs::complete_discovery_run(
        &state.pool,
        &source_id,
        &req.lease_token,
        &req.replay_key,
        req.scheduled_for_ms,
        &req.jobs,
        req.complete_snapshot,
    )
    .map(Json)
    .map_err(discovery_domain_error)
}

async fn worker_discovery_fail(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<WorkerDiscoveryFailRequest>,
) -> Result<Json<jobs::DiscoveryRunResult>, ApiError> {
    jobs::fail_discovery_run(
        &state.pool,
        &source_id,
        &req.lease_token,
        &req.replay_key,
        req.scheduled_for_ms,
        &req.error_code,
    )
    .map(Json)
    .map_err(discovery_domain_error)
}

#[derive(Debug, Deserialize)]
struct WorkerGlobalDiscoverySourceSyncRequest {
    sources: Vec<jobs::GlobalDiscoverySourceInput>,
}

async fn worker_global_discovery_source_sync(
    State(state): State<AppState>,
    Json(req): Json<WorkerGlobalDiscoverySourceSyncRequest>,
) -> Result<Json<Vec<jobs::GlobalDiscoverySource>>, ApiError> {
    jobs::sync_global_discovery_sources(&state.pool, &req.sources)
        .map(Json)
        .map_err(discovery_domain_error)
}

async fn worker_global_discovery_lease(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let worker_id = headers
        .get("x-bluey-jobs-worker-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("jobs-global-discovery-worker");
    match jobs::lease_due_global_discovery_source(&state.pool, worker_id)
        .map_err(discovery_domain_error)?
    {
        Some(lease) => Ok(Json(lease).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

async fn worker_global_discovery_batch(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<jobs::GlobalIngestionBatchInput>,
) -> Result<Json<jobs::GlobalIngestionBatchResult>, ApiError> {
    jobs::ingest_global_discovery_batch(&state.pool, &source_id, &req)
        .map(Json)
        .map_err(discovery_domain_error)
}

async fn worker_global_discovery_complete(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<jobs::GlobalIngestionCompleteInput>,
) -> Result<Json<jobs::GlobalIngestionRunResult>, ApiError> {
    jobs::complete_global_discovery_ingestion(&state.pool, &source_id, &req)
        .map(Json)
        .map_err(discovery_domain_error)
}

async fn worker_global_discovery_fail(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<jobs::GlobalIngestionFailureInput>,
) -> Result<Json<jobs::GlobalIngestionRunResult>, ApiError> {
    jobs::fail_global_discovery_ingestion(&state.pool, &source_id, &req)
        .map(Json)
        .map_err(discovery_domain_error)
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
    let mut state_name = req.state.as_str();
    let mut reconciled_state = None;
    if req.state == "failed" {
        let current = jobs::get_application(&state.pool, &req.account_id, &application_id)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
        if let Some(run_id) = current.run_id.as_deref() {
            match jobs::execution_lease_phase_for_application(
                &state.pool,
                &req.account_id,
                &application_id,
                run_id,
            )
            .map_err(internal)?
            .as_deref()
            {
                Some("prepared") => {
                    return Err((
                        StatusCode::CONFLICT,
                        "Finish the active execution lease before reporting failure.".to_string(),
                    ));
                }
                Some("click_started" | "side_effect_unknown") => {
                    reconciled_state = Some("side_effect_unknown");
                }
                Some("submitted") => {
                    return Err((
                        StatusCode::CONFLICT,
                        "This execution has already been reconciled.".to_string(),
                    ));
                }
                _ => {}
            }
        }
    }
    if let Some(value) = reconciled_state {
        state_name = value;
    }
    let application = jobs::update_application(
        &state.pool,
        &req.account_id,
        &application_id,
        state_name,
        None,
    )
    .map_err(domain_error)?
    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    let attempt_status = match state_name {
        "running" | "needs_input" => Some("running"),
        "side_effect_unknown" => Some("side_effect_unknown"),
        "failed" => Some("released"),
        _ => None,
    };
    if let Some(attempt_status) = attempt_status {
        jobs::update_attempt_reservation_status(
            &state.pool,
            &req.account_id,
            &application_id,
            attempt_status,
        )
        .map_err(internal)?;
    }
    update_worker_browser_session(&state, &req.account_id, &application_id, state_name)?;
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
        metadata: json!({
            "receipt": receipt,
            (TRUSTED_WORKER_RECEIPT_KEY): true,
        }),
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
    #[serde(default)]
    evidence_objects: Vec<ReceiptEvidenceObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptEvidenceObject {
    original_key: String,
    kind: String,
    media_type: String,
    sha256: String,
    bytes_base64: String,
}

async fn worker_receipt(
    State(state): State<AppState>,
    Path(application_id): Path<String>,
    Json(req): Json<WorkerReceiptRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    persist_submission_receipt(
        &state,
        &req.account_id,
        &application_id,
        req.receipt,
        req.evidence_objects,
        "cloud",
        None,
    )
    .await
    .map(Json)
}

async fn persist_submission_receipt(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    mut receipt: Value,
    evidence_objects: Vec<ReceiptEvidenceObject>,
    expected_runner: &str,
    local_ticket_hash: Option<&str>,
) -> Result<JobApplication, ApiError> {
    if receipt.get(SUBMISSION_FINGERPRINT_KEY).is_some() {
        return bad_request("Submission receipt contains a reserved server field.");
    }
    let request_fingerprint = submission_request_fingerprint(&receipt, &evidence_objects)?;
    let application = jobs::get_application(&state.pool, account_id, application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    let receipt_id = required_receipt_string(&receipt, "receiptId")?;
    if application.state == "submitted" {
        if application
            .receipt
            .get(SUBMISSION_FINGERPRINT_KEY)
            .and_then(Value::as_str)
            == Some(request_fingerprint.as_str())
        {
            return Ok(application);
        }
        return Err((
            StatusCode::CONFLICT,
            "This application already has a different final receipt.".to_string(),
        ));
    }
    if receipt.get("schemaVersion").and_then(Value::as_i64) != Some(1) {
        return bad_request("Unsupported submission receipt version.");
    }
    if receipt.get("accountId").and_then(Value::as_str) != Some(account_id) {
        return bad_request("Receipt does not match this account.");
    }
    if receipt.get("applicationId").and_then(Value::as_str) != Some(application_id) {
        return bad_request("Receipt does not match this application.");
    }
    if receipt.get("runner").and_then(Value::as_str) != Some(expected_runner) {
        return bad_request("Receipt does not match this browser runner.");
    }
    let run_id = required_receipt_string(&receipt, "runId")?;
    if application.run_id.as_deref() != Some(run_id.as_str()) {
        return bad_request("Receipt does not match this browser run.");
    }
    let result = receipt.get("result").and_then(Value::as_object).ok_or((
        StatusCode::BAD_REQUEST,
        "Submission result is missing.".to_string(),
    ))?;
    if result.get("status").and_then(Value::as_str) != Some("submitted") {
        return bad_request("Only a confirmed submission can create a final receipt.");
    }
    let confirmation_text = result
        .get("confirmationText")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let confirmation_url = result
        .get("confirmationUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    if confirmation_text.is_none() && confirmation_url.is_none() {
        return bad_request(
            "A submitted receipt needs real confirmation text or a confirmation URL.",
        );
    }
    let confirmation = confirmation_text
        .clone()
        .or_else(|| confirmation_url.clone())
        .expect("confirmation checked above");
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
    let verified_claim_ids = confirmed_resume_claim_ids(state, account_id, &resume)?;
    validate_receipt_verified_claim_ids(&receipt, &verified_claim_ids)?;
    if expected_runner == "cloud" {
        match jobs::execution_lease_phase_for_application(
            &state.pool,
            account_id,
            application_id,
            &run_id,
        )
        .map_err(internal)?
        .as_deref()
        {
            Some("submitted") => {}
            Some(_) | None => {
                return Err((
                    StatusCode::CONFLICT,
                    "A matching cloud execution lease is not terminal submitted.".to_string(),
                ));
            }
        }
    }
    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Application evidence storage is not configured.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    let prepared_objects =
        preflight_receipt_evidence(&receipt, evidence_objects, storage.max_object_bytes())?;
    let preflight_verified = prepared_objects
        .iter()
        .map(|object| (object.original_key.clone(), object.sha256.clone()))
        .collect::<BTreeMap<_, _>>();
    validate_receipt_bundle(
        account_id,
        &application,
        &posting,
        &resume,
        &receipt,
        &preflight_verified,
    )?;
    let terminal_session =
        submission_terminal_session(state, account_id, application_id, &run_id, expected_runner)?;
    receipt
        .as_object_mut()
        .expect("validated receipt fields require an object")
        .insert(
            SUBMISSION_FINGERPRINT_KEY.to_string(),
            Value::String(request_fingerprint.clone()),
        );
    let uploaded = upload_receipt_evidence(
        &storage,
        account_id,
        application_id,
        &mut receipt,
        prepared_objects,
    )
    .await?;
    if let Err(error) = validate_receipt_bundle(
        account_id,
        &application,
        &posting,
        &resume,
        &receipt,
        &uploaded.verified,
    ) {
        cleanup_uploaded_objects(&storage, &uploaded.created_keys).await;
        return Err(error);
    }
    let provider = required_receipt_string(&receipt, "adapter")?;
    let evidence = match submission_evidence_records(
        application_id,
        &receipt_id,
        &provider,
        &posting,
        &resume,
        &receipt,
        &uploaded.verified,
        confirmation,
        confirmation_url,
        confirmation_text,
        submitted_at,
    ) {
        Ok(evidence) => evidence,
        Err(error) => {
            cleanup_uploaded_objects(&storage, &uploaded.created_keys).await;
            return Err(error);
        }
    };
    let finalized = jobs::finalize_submission(
        &state.pool,
        account_id,
        application_id,
        &run_id,
        expected_runner,
        receipt,
        &request_fingerprint,
        &evidence,
        &terminal_session,
        local_ticket_hash,
    );
    match finalized {
        Ok(jobs::SubmissionFinalizeResult::Committed(application)) => Ok(application),
        Ok(jobs::SubmissionFinalizeResult::Replayed(application)) => {
            cleanup_uploaded_objects(&storage, &uploaded.created_keys).await;
            Ok(application)
        }
        Err(error) => {
            cleanup_uploaded_objects(&storage, &uploaded.created_keys).await;
            Err(submission_domain_error(error))
        }
    }
}

fn submission_request_fingerprint(
    receipt: &Value,
    evidence_objects: &[ReceiptEvidenceObject],
) -> Result<String, ApiError> {
    let encoded = serde_json::to_vec(&json!({
        "receipt": receipt,
        "evidence_objects": evidence_objects,
    }))
    .map_err(|error| internal(error.into()))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn confirmed_resume_claim_ids(
    state: &AppState,
    account_id: &str,
    resume: &ResumeVersion,
) -> Result<Vec<String>, ApiError> {
    let confirmed = jobs::list_facts(&state.pool, account_id)
        .map_err(internal)?
        .into_iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .map(|fact| fact.id)
        .collect::<BTreeSet<_>>();
    let mut claim_ids = resume
        .claim_ids
        .iter()
        .filter(|claim_id| confirmed.contains(*claim_id))
        .cloned()
        .collect::<Vec<_>>();
    claim_ids.sort();
    claim_ids.dedup();
    Ok(claim_ids)
}

fn validate_receipt_verified_claim_ids(
    receipt: &Value,
    expected_claim_ids: &[String],
) -> Result<(), ApiError> {
    let claims = receipt
        .pointer("/packet/verifiedClaimIds")
        .and_then(Value::as_array)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt is missing its verified career claims.".to_string(),
        ))?;
    let mut actual = claims
        .iter()
        .map(|claim| {
            claim.as_str().map(str::to_string).ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt verified career claims are invalid.".to_string(),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let original_len = actual.len();
    actual.sort();
    actual.dedup();
    if actual.len() != original_len || actual != expected_claim_ids {
        return bad_request("Receipt verified career claims do not match the approved resume.");
    }
    Ok(())
}

fn required_receipt_string(receipt: &Value, field: &str) -> Result<String, ApiError> {
    receipt
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!("Receipt is missing {field}."),
        ))
}

#[derive(Debug)]
struct PreparedReceiptEvidence {
    original_key: String,
    kind: String,
    media_type: &'static str,
    sha256: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct UploadedReceiptEvidence {
    verified: BTreeMap<String, String>,
    created_keys: Vec<String>,
}

fn preflight_receipt_evidence(
    receipt: &Value,
    evidence_objects: Vec<ReceiptEvidenceObject>,
    max_object_bytes: usize,
) -> Result<Vec<PreparedReceiptEvidence>, ApiError> {
    let documents = receipt.get("documents").and_then(Value::as_array).ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt is missing uploaded application documents.".to_string(),
    ))?;
    if documents.is_empty() || documents.len() > MAX_RECEIPT_DOCUMENTS {
        return bad_request("Receipt has an invalid number of application documents.");
    }
    let screenshots = receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt is missing a confirmation screenshot.".to_string(),
        ))?;
    if screenshots.is_empty() || screenshots.len() > MAX_RECEIPT_SCREENSHOTS {
        return bad_request("Receipt has an invalid number of confirmation screenshots.");
    }

    let mut references = BTreeMap::<String, (String, Option<String>)>::new();
    let mut resume_count = 0usize;
    let mut cover_letter_count = 0usize;
    for document in documents {
        let document = document.as_object().ok_or((
            StatusCode::BAD_REQUEST,
            "Application document metadata is invalid.".to_string(),
        ))?;
        let kind = document
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(kind, "resume" | "cover_letter" | "attachment") {
            return bad_request("Application document kind is invalid.");
        }
        resume_count += usize::from(kind == "resume");
        cover_letter_count += usize::from(kind == "cover_letter");
        let key = document
            .get("storageKey")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty() && key.len() <= 4_096)
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Application document reference is invalid.".to_string(),
            ))?;
        let sha256 = document
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|sha256| valid_sha256(sha256))
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Application document checksum is invalid.".to_string(),
            ))?
            .to_ascii_lowercase();
        if let Some(media_type) = document.get("mediaType").and_then(Value::as_str) {
            if !media_type.eq_ignore_ascii_case("application/pdf") {
                return bad_request("Application documents must be PDF files.");
            }
        }
        if references
            .insert(key.to_string(), (kind.to_string(), Some(sha256)))
            .is_some()
        {
            return bad_request("Receipt contains a duplicate evidence reference.");
        }
    }
    if resume_count != 1 || cover_letter_count > 1 {
        return bad_request("Receipt has duplicate or missing application document kinds.");
    }
    for screenshot in screenshots {
        let key = screenshot
            .as_str()
            .filter(|key| !key.trim().is_empty() && key.len() <= 4_096)
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Confirmation screenshot reference is invalid.".to_string(),
            ))?;
        if references
            .insert(key.to_string(), ("screenshot".to_string(), None))
            .is_some()
        {
            return bad_request("Receipt contains a duplicate evidence reference.");
        }
    }
    if references.len() > MAX_RECEIPT_EVIDENCE_OBJECTS
        || evidence_objects.len() > MAX_RECEIPT_EVIDENCE_OBJECTS
    {
        return bad_request("Receipt contains too many evidence objects.");
    }

    let mut seen_objects = BTreeSet::new();
    let mut aggregate_bytes = 0usize;
    let mut prepared = Vec::with_capacity(evidence_objects.len());
    for object in evidence_objects {
        if object.original_key.trim().is_empty()
            || object.original_key.len() > 4_096
            || !matches!(
                object.kind.as_str(),
                "resume" | "cover_letter" | "attachment" | "screenshot"
            )
            || !valid_sha256(&object.sha256)
        {
            return bad_request("Application evidence metadata is invalid.");
        }
        if !seen_objects.insert(object.original_key.clone()) {
            return bad_request("Application evidence contains a duplicate object.");
        }
        let Some((referenced_kind, referenced_sha256)) = references.get(&object.original_key)
        else {
            return bad_request("Application evidence contains an unreferenced object.");
        };
        if referenced_kind != &object.kind {
            return bad_request("Application evidence kind does not match its receipt reference.");
        }
        let media_type = expected_evidence_media_type(&object.kind);
        if !object.media_type.trim().eq_ignore_ascii_case(media_type) {
            return bad_request("Application evidence media type is invalid.");
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(object.bytes_base64.trim())
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Application evidence is not valid base64.".to_string(),
                )
            })?;
        if bytes.is_empty() || bytes.len() > max_object_bytes {
            return bad_request("Application evidence is empty or too large.");
        }
        aggregate_bytes = aggregate_bytes.checked_add(bytes.len()).ok_or((
            StatusCode::BAD_REQUEST,
            "Application evidence is too large.".to_string(),
        ))?;
        if aggregate_bytes > MAX_RECEIPT_EVIDENCE_BYTES {
            return bad_request("Application evidence is too large.");
        }
        let actual_sha256 = sha256_hex(&bytes);
        if !actual_sha256.eq_ignore_ascii_case(&object.sha256)
            || referenced_sha256
                .as_deref()
                .is_some_and(|expected| !actual_sha256.eq_ignore_ascii_case(expected))
        {
            return bad_request("Application evidence checksum does not match its bytes.");
        }
        if object.kind == "screenshot" {
            if !valid_png(&bytes) {
                return bad_request("Confirmation screenshots must be valid PNG images.");
            }
        } else if !valid_pdf(&bytes) {
            return bad_request("Application documents must be valid PDF files.");
        }
        prepared.push(PreparedReceiptEvidence {
            original_key: object.original_key,
            kind: object.kind,
            media_type,
            sha256: actual_sha256,
            bytes,
        });
    }
    if seen_objects.len() != references.len()
        || references.keys().any(|key| !seen_objects.contains(key))
    {
        return bad_request("Receipt references evidence that was not supplied.");
    }
    Ok(prepared)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn expected_evidence_media_type(kind: &str) -> &'static str {
    if kind == "screenshot" {
        "image/png"
    } else {
        "application/pdf"
    }
}

fn valid_pdf(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"%PDF-") {
        return false;
    }
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(0);
    bytes[..end].ends_with(b"%%EOF")
}

fn valid_png(bytes: &[u8]) -> bool {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    bytes.len() >= 33
        && &bytes[..8] == PNG_SIGNATURE
        && u32::from_be_bytes(bytes[8..12].try_into().expect("four-byte PNG chunk length")) == 13
        && &bytes[12..16] == b"IHDR"
        && u32::from_be_bytes(bytes[16..20].try_into().expect("four-byte PNG width")) > 0
        && u32::from_be_bytes(bytes[20..24].try_into().expect("four-byte PNG height")) > 0
}

async fn upload_receipt_evidence(
    storage: &ObjectStorage,
    account_id: &str,
    application_id: &str,
    receipt: &mut Value,
    evidence_objects: Vec<PreparedReceiptEvidence>,
) -> Result<UploadedReceiptEvidence, ApiError> {
    let mut verified = BTreeMap::new();
    let mut created_keys = Vec::new();
    let request_id = uuid::Uuid::new_v4();
    for object in evidence_objects {
        let artifact_id = format!(
            "jobs/{application_id}/receipts/{request_id}/{}-{}",
            safe_file_part(&object.kind),
            &object.sha256[..20]
        );
        let storage_key = storage.artifact_key(account_id, &artifact_id);
        if let Err(error) = storage
            .put(
                &storage_key,
                bytes::Bytes::from(object.bytes),
                object.media_type,
            )
            .await
        {
            cleanup_uploaded_objects(storage, &created_keys).await;
            return Err(evidence_storage_error(error));
        }
        created_keys.push(storage_key.clone());
        let stored = match storage.get(&storage_key).await {
            Ok(stored) => stored,
            Err(error) => {
                cleanup_uploaded_objects(storage, &created_keys).await;
                return Err(evidence_storage_error(error));
            }
        };
        if sha256_hex(&stored.bytes) != object.sha256 {
            cleanup_uploaded_objects(storage, &created_keys).await;
            return Err((
                StatusCode::BAD_GATEWAY,
                "Stored application evidence failed checksum verification.".to_string(),
            ));
        }
        replace_receipt_storage_key(receipt, &object.original_key, &storage_key);
        verified.insert(storage_key, object.sha256);
    }
    if let Some(result) = receipt.get_mut("result").and_then(Value::as_object_mut) {
        result.remove("screenshotPath");
    }
    Ok(UploadedReceiptEvidence {
        verified,
        created_keys,
    })
}

async fn cleanup_uploaded_objects(storage: &ObjectStorage, created_keys: &[String]) {
    for key in created_keys.iter().rev() {
        let _ = storage.delete(key).await;
    }
}

fn evidence_storage_error(_error: anyhow::Error) -> ApiError {
    tracing::error!("Bluey Jobs evidence storage request failed");
    (
        StatusCode::BAD_GATEWAY,
        "Bluey Jobs could not store submission evidence. Please try again.".to_string(),
    )
}

fn replace_receipt_storage_key(receipt: &mut Value, original_key: &str, storage_key: &str) {
    if let Some(documents) = receipt.get_mut("documents").and_then(Value::as_array_mut) {
        for document in documents {
            let Some(document) = document.as_object_mut() else {
                continue;
            };
            if document.get("storageKey").and_then(Value::as_str) == Some(original_key) {
                document.insert(
                    "storageKey".to_string(),
                    Value::String(storage_key.to_string()),
                );
                if let Some(kind) = document.get("kind").and_then(Value::as_str) {
                    document.insert(
                        "mediaType".to_string(),
                        Value::String(expected_evidence_media_type(kind).to_string()),
                    );
                }
            }
        }
    }
    if let Some(screenshots) = receipt
        .get_mut("screenshotKeys")
        .and_then(Value::as_array_mut)
    {
        for screenshot in screenshots {
            if screenshot.as_str() == Some(original_key) {
                *screenshot = Value::String(storage_key.to_string());
            }
        }
    }
}

fn submission_terminal_session(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
) -> Result<BrowserSession, ApiError> {
    let mut session = jobs::list_browser_sessions(&state.pool, account_id)
        .map_err(internal)?
        .into_iter()
        .find(|session| {
            session.id == run_id
                && session.runner == runner
                && session.application_id.as_deref() == Some(application_id)
        })
        .ok_or((
            StatusCode::CONFLICT,
            "The receipt does not match an active browser session.".to_string(),
        ))?;
    session.status = "complete".to_string();
    session.current_step = "Application submitted".to_string();
    session.takeover_url = None;
    Ok(session)
}

#[allow(clippy::too_many_arguments)]
fn submission_evidence_records(
    application_id: &str,
    receipt_id: &str,
    provider: &str,
    posting: &JobPosting,
    resume: &ResumeVersion,
    receipt: &Value,
    verified_objects: &BTreeMap<String, String>,
    confirmation: String,
    confirmation_url: Option<String>,
    confirmation_text: Option<String>,
    submitted_at: Option<Value>,
) -> Result<Vec<ApplicationEvidence>, ApiError> {
    let documents = receipt.get("documents").and_then(Value::as_array).ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt is missing uploaded application documents.".to_string(),
    ))?;
    let mut evidence = Vec::with_capacity(documents.len() + 1);
    for (index, document) in documents.iter().enumerate() {
        let kind = document
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let storage_key = document
            .get("storageKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let sha256 = verified_objects.get(storage_key).cloned().ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt document was not uploaded and verified.".to_string(),
        ))?;
        let (label, file_name) = match kind {
            "resume" => (
                "Resume submitted".to_string(),
                format!(
                    "{}-{}-resume.pdf",
                    safe_file_part(&posting.company),
                    safe_file_part(&posting.title)
                ),
            ),
            "cover_letter" => (
                "Cover letter submitted".to_string(),
                format!(
                    "{}-{}-cover-letter.pdf",
                    safe_file_part(&posting.company),
                    safe_file_part(&posting.title)
                ),
            ),
            "attachment" => (
                "Attachment submitted".to_string(),
                format!("application-attachment-{}.pdf", index + 1),
            ),
            _ => return bad_request("Application document kind is invalid."),
        };
        evidence.push(ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: kind.to_string(),
            label,
            provider: provider.to_string(),
            file_name,
            media_type: "application/pdf".to_string(),
            storage_key: storage_key.to_string(),
            sha256,
            resume_version_id: (kind == "resume").then(|| resume.id.clone()),
            occurred_at_ms: 0,
            metadata: json!({
                "attached_to_submission": true,
                "receipt_id": receipt_id,
                "application_identity_id": receipt.get("applicationIdentityId"),
                "structured_resume_checksum": (kind == "resume").then_some(&resume.checksum),
            }),
            created_at_ms: 0,
        });
    }
    let screenshot_key = receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .and_then(|screenshots| screenshots.first())
        .and_then(Value::as_str)
        .unwrap_or_default();
    let screenshot_sha256 = verified_objects.get(screenshot_key).cloned().ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt confirmation screenshot was not uploaded and verified.".to_string(),
    ))?;
    evidence.push(ApplicationEvidence {
        id: String::new(),
        application_id: application_id.to_string(),
        kind: "submission_confirmation".to_string(),
        label: confirmation.clone(),
        provider: provider.to_string(),
        file_name: "submission-confirmation.png".to_string(),
        media_type: "image/png".to_string(),
        storage_key: screenshot_key.to_string(),
        sha256: screenshot_sha256,
        resume_version_id: Some(resume.id.clone()),
        occurred_at_ms: 0,
        metadata: json!({
            "confirmation": confirmation,
            "confirmation_url": confirmation_url,
            "confirmation_text": confirmation_text,
            "submitted_at": submitted_at,
            "screenshot_keys": receipt.get("screenshotKeys"),
            "evidence_strength": "browser_confirmed",
            "receipt_id": receipt_id,
        }),
        created_at_ms: 0,
    });
    Ok(evidence)
}

fn validate_receipt_bundle(
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    resume: &ResumeVersion,
    receipt: &Value,
    verified_objects: &BTreeMap<String, String>,
) -> Result<(), ApiError> {
    let (approved_packet, approved_job, approved_checksum) =
        approved_execution_snapshot(application)?;
    validate_approved_execution_matches(
        application,
        posting,
        resume,
        application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        application
            .receipt
            .pointer("/application_identity/email")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &approved_packet,
        &approved_job,
    )?;
    let identity_id = required_receipt_string(receipt, "applicationIdentityId")?;
    let expected_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expected_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if expected_identity_id.is_empty() || identity_id != expected_identity_id {
        return bad_request(
            "Receipt application email identity does not match the approved packet.",
        );
    }
    let browser_profile_id_value = required_receipt_string(receipt, "browserProfileId")?;
    if browser_profile_id_value != browser_profile_id(account_id, expected_identity_id) {
        return bad_request(
            "Receipt browser profile does not match the approved application email.",
        );
    }
    required_receipt_string(receipt, "adapter")?;
    required_receipt_string(receipt, "adapterVersion")?;
    let packet = receipt.get("packet").and_then(Value::as_object).ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt is missing its exact application packet.".to_string(),
    ))?;
    if packet.get("jobId").and_then(Value::as_str) != Some(application.job_id.as_str())
        || packet.get("resumeVersionId").and_then(Value::as_str) != Some(resume.id.as_str())
        || packet.get("applicationEmail").and_then(Value::as_str) != Some(expected_email)
    {
        return bad_request(
            "Receipt packet does not match the approved job, resume, and application email.",
        );
    }
    if packet.get("approvedPacketChecksum").and_then(Value::as_str)
        != Some(approved_checksum.as_str())
        || packet.get("answers") != approved_packet.get("answers")
    {
        return bad_request(
            "Receipt answers do not match the exact application packet that was approved.",
        );
    }
    if receipt.pointer("/job/canonicalUrl").and_then(Value::as_str)
        != Some(posting.canonical_url.as_str())
    {
        return bad_request("Receipt job does not match the approved posting.");
    }
    let documents = receipt.get("documents").and_then(Value::as_array).ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt is missing uploaded application documents.".to_string(),
    ))?;
    let resume_document = documents
        .iter()
        .find(|document| document.get("kind").and_then(Value::as_str) == Some("resume"))
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt is missing the submitted resume.".to_string(),
        ))?;
    if resume_document.get("versionId").and_then(Value::as_str) != Some(resume.id.as_str()) {
        return bad_request("Receipt resume does not match the approved resume version.");
    }
    for document in documents {
        let storage_key = document
            .get("storageKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let sha256 = document
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if verified_objects.get(storage_key).map(String::as_str) != Some(sha256) {
            return bad_request(
                "Receipt references an application document that was not uploaded and verified.",
            );
        }
    }
    let screenshots = receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt is missing a confirmation screenshot.".to_string(),
        ))?;
    if screenshots.is_empty()
        || screenshots.iter().any(|screenshot| {
            screenshot
                .as_str()
                .is_none_or(|key| !verified_objects.contains_key(key))
        })
    {
        return bad_request("Receipt confirmation screenshot was not uploaded and verified.");
    }
    Ok(())
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

pub(crate) fn validate_profile(profile: &CareerProfile) -> Result<(), ApiError> {
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
    if !(-840..=840).contains(&preferences.time_zone_offset_minutes) {
        return bad_request("Choose a valid account time zone.");
    }
    if preferences
        .employment_types
        .iter()
        .any(|value| normalize_candidate_employment_type(value).is_none())
    {
        return bad_request("Choose a supported employment type.");
    }
    if preferences
        .engagement_types
        .iter()
        .any(|value| normalize_candidate_engagement_type(value).is_none())
    {
        return bad_request("Choose a supported engagement type.");
    }
    Ok(())
}

fn validate_track(track: &CareerTrack) -> Result<(), ApiError> {
    if track.name.trim().is_empty() || track.role.trim().is_empty() {
        return bad_request("Give this Career Track a name and target role.");
    }
    if track
        .policy
        .employment_types
        .iter()
        .any(|value| normalize_candidate_employment_type(value).is_none())
    {
        return bad_request("Choose a supported Career Track employment type.");
    }
    if track
        .policy
        .engagement_types
        .iter()
        .any(|value| normalize_candidate_engagement_type(value).is_none())
    {
        return bad_request("Choose a supported Career Track engagement type.");
    }
    Ok(())
}

fn enforce_track_limit(
    track: &CareerTrack,
    current: &[CareerTrack],
    entitlement: &JobsEntitlement,
) -> Result<(), ApiError> {
    let is_new = track.id.is_empty() || !current.iter().any(|item| item.id == track.id);
    let active_tracks = current.iter().filter(|item| item.active).count() as i64;
    if is_new && track.active && active_tracks >= entitlement.track_limit {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "Your {} plan includes {} Career Track Agent(s).",
                entitlement.plan, entitlement.track_limit
            ),
        ));
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
    } else if host == "jobs.lever.co" || host == "jobs.eu.lever.co" {
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

pub(super) fn bad_request<T>(message: &str) -> Result<T, ApiError> {
    Err((StatusCode::BAD_REQUEST, message.to_string()))
}

fn validation_or_internal(error: anyhow::Error, validation_message: &str) -> ApiError {
    if error.to_string().contains("invalid application") {
        (StatusCode::BAD_REQUEST, validation_message.to_string())
    } else {
        internal(error)
    }
}

pub(super) fn domain_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    let status = if message.contains("another Bluey Jobs account")
        || message.contains("dedicated confirmation flow")
        || message.contains("days old")
        || message.contains("no longer accepting")
        || message.contains("still open before applying")
        || message.contains("before marking this application submitted")
        || message.contains("daily application limit")
        || message.contains("active application attempt")
        || message.contains("application attempt")
        || message.contains("communication action cannot")
        || message.contains("communication action idempotency key was reused")
    {
        StatusCode::CONFLICT
    } else if message.contains("monthly packet limit")
        || message.contains("insufficient Bluey balance")
    {
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
        || message.contains("candidate feedback")
        || message.contains("candidate event")
        || message.contains("match feedback")
        || message.contains("application issue")
        || message.contains("application outcome")
        || message.contains("does not belong to this job")
        || message.contains("communication action")
        || message.contains("calendar action")
    {
        StatusCode::BAD_REQUEST
    } else {
        return internal(error);
    };
    (status, message)
}

fn submission_domain_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("different final receipt")
        || message.contains("execution lease is not terminal submitted")
        || message.contains("invalid application state transition")
        || message.contains("local run ticket is not active")
    {
        (StatusCode::CONFLICT, message)
    } else if message.contains("application not found") {
        (StatusCode::NOT_FOUND, message)
    } else {
        internal(error)
    }
}

fn discovery_domain_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    let status = if message.contains("not found") {
        StatusCode::NOT_FOUND
    } else if message.contains("stale") || message.contains("scheduled run") {
        StatusCode::CONFLICT
    } else if message.contains("discovery")
        || message.contains("Greenhouse")
        || message.contains("Lever")
    {
        StatusCode::BAD_REQUEST
    } else {
        return internal(error);
    };
    (status, message)
}

fn execution_lease_error(error: jobs::ExecutionLeaseError) -> ApiError {
    match error {
        jobs::ExecutionLeaseError::InvalidRequest => (
            StatusCode::BAD_REQUEST,
            "Invalid execution lease request.".to_string(),
        ),
        jobs::ExecutionLeaseError::NotFound => (
            StatusCode::NOT_FOUND,
            "Execution lease target not found.".to_string(),
        ),
        jobs::ExecutionLeaseError::Conflict => (
            StatusCode::CONFLICT,
            "Execution lease is not available.".to_string(),
        ),
        jobs::ExecutionLeaseError::Storage(error) => internal(error),
    }
}

pub(super) fn internal(error: anyhow::Error) -> ApiError {
    tracing::error!(error = %error, "Bluey Jobs request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Bluey Jobs could not finish that request. Please try again.".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::jobs::CareerTrackPolicy;

    fn test_entitlement(track_limit: i64) -> JobsEntitlement {
        JobsEntitlement {
            plan: "free".to_string(),
            track_limit,
            monthly_packet_limit: 5,
            used_packets: 0,
            period_start_ms: 0,
            period_end_ms: 0,
            local_browser: false,
            cloud_browser: false,
            overage_cents: 50,
            monthly_price_cents: 0,
            application_identity_limit: 2,
            connected_inbox_limit: 1,
            additional_inbox_cents: 500,
        }
    }

    #[test]
    fn unavailable_runner_distribution_masks_plan_entitlements() {
        let mut entitlement = test_entitlement(3);
        entitlement.local_browser = true;
        entitlement.cloud_browser = true;

        apply_jobs_distribution_gates(&mut entitlement, false, false);

        assert!(!entitlement.local_browser);
        assert!(!entitlement.cloud_browser);
    }

    #[test]
    fn available_runner_distribution_preserves_plan_entitlements() {
        let mut entitlement = test_entitlement(5);
        entitlement.local_browser = true;
        entitlement.cloud_browser = true;

        apply_jobs_distribution_gates(&mut entitlement, true, true);

        assert!(entitlement.local_browser);
        assert!(entitlement.cloud_browser);
    }

    #[test]
    fn free_plan_explains_that_auto_submit_needs_runner_access() {
        let entitlement = test_entitlement(1);
        let availability = build_runner_availability(&entitlement, false, false);

        assert_eq!(availability.local.status, "upgrade_required");
        assert_eq!(availability.cloud.status, "upgrade_required");
        assert!(!availability.auto_submit_available);
        assert!(availability
            .auto_submit_reason
            .contains("requires a Jobs plan"));
    }

    #[test]
    fn included_but_undistributed_runner_is_truthfully_invited_beta() {
        let mut entitlement = test_entitlement(3);
        entitlement.plan = "pro".to_string();
        entitlement.local_browser = true;
        let availability = build_runner_availability(&entitlement, false, false);

        assert_eq!(availability.local.status, "invited_beta");
        assert!(availability.local.plan_included);
        assert!(!availability.local.available);
        assert!(!availability.auto_submit_available);
        assert!(availability.auto_submit_reason.contains("invited beta"));
    }

    #[test]
    fn distributed_runner_enables_auto_submit_availability() {
        let mut entitlement = test_entitlement(5);
        entitlement.plan = "cloud".to_string();
        entitlement.local_browser = true;
        entitlement.cloud_browser = true;
        let availability = build_runner_availability(&entitlement, true, true);

        assert!(availability.local.available);
        assert!(availability.cloud.available);
        assert!(availability.auto_submit_available);
    }

    fn test_eligibility(capability: &str, can_auto_submit: bool) -> JobEligibilityDecision {
        JobEligibilityDecision {
            capability: capability.to_string(),
            can_auto_submit,
            can_queue_local: can_auto_submit,
            can_queue_cloud: can_auto_submit,
            ..JobEligibilityDecision::default()
        }
    }

    #[test]
    fn auto_submit_error_names_ats_and_runner_boundaries() {
        let mut entitlement = test_entitlement(3);
        entitlement.plan = "pro".to_string();
        entitlement.local_browser = true;
        let unavailable = build_runner_availability(&entitlement, false, false);

        let beta = auto_submit_request_error(&test_eligibility("beta_review", false), &unavailable)
            .expect("beta must stay in review");
        assert_eq!(beta.0, StatusCode::CONFLICT);
        assert!(beta.1.contains("in beta"));

        let handoff = auto_submit_request_error(&test_eligibility("handoff", false), &unavailable)
            .expect("handoff must stay user controlled");
        assert!(handoff.1.contains("user-controlled handoff"));

        let unknown =
            auto_submit_request_error(&test_eligibility("unknown_review", false), &unavailable)
                .expect("unknown ATS must stay in review");
        assert!(unknown.1.contains("has not been certified"));

        let runner = auto_submit_request_error(&test_eligibility("certified", true), &unavailable)
            .expect("undistributed runner must block auto submit");
        assert_eq!(runner.0, StatusCode::SERVICE_UNAVAILABLE);
        assert!(runner.1.contains("invited beta"));
    }

    fn test_track(id: &str) -> CareerTrack {
        CareerTrack {
            id: id.to_string(),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["Austin, TX".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: None,
            policy: CareerTrackPolicy::default(),
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    fn discovery_enrollment_test_pool() -> crate::db::DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-discovery-enrollment-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-one', 'one@example.com', 'hash', 0);
             INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-two', 'two@example.com', 'hash', 0);",
        )
        .unwrap();
        drop(conn);
        pool
    }

    fn imported_discovery_posting(source: &str, canonical_url: &str, track_id: &str) -> JobPosting {
        let mut posting = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: source.to_string(),
            external_id: "job-123".to_string(),
            company: "Acme".to_string(),
            title: "Platform Engineer".to_string(),
            location: "Austin, TX".to_string(),
            workplace: "Hybrid".to_string(),
            canonical_url: canonical_url.to_string(),
            description: "Build reliable services.".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: track_id.to_string(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: None,
            last_verified_at_ms: Some(1),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: jobs::JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        posting.canonical_key = jobs::canonical_job_key(&posting);
        let application_domain = reqwest::Url::parse(canonical_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string));
        posting.discovery_evidence = jobs::JobDiscoveryEvidence::verified_original_source(
            posting.canonical_key.clone(),
            format!("{source}:acme"),
            application_domain,
            jobs::now_ms(),
            "a".repeat(64),
        );
        posting
    }

    fn store_discovery_posting(
        pool: &crate::db::DbPool,
        account_id: &str,
        posting: &JobPosting,
    ) -> JobPosting {
        jobs::upsert_posting(
            pool,
            account_id,
            posting,
            &CareerProfile::default(),
            &JobPreferences::default(),
        )
        .unwrap()
    }

    #[test]
    fn client_supplied_track_id_cannot_bypass_plan_limit() {
        let current = vec![test_track("existing-track")];
        let error =
            enforce_track_limit(&test_track("new-client-id"), &current, &test_entitlement(1))
                .unwrap_err();
        assert_eq!(error.0, StatusCode::PAYMENT_REQUIRED);
    }

    #[test]
    fn retrying_the_same_onboarding_track_is_idempotent() {
        let current = vec![test_track("stable-onboarding-track")];
        enforce_track_limit(
            &test_track("stable-onboarding-track"),
            &current,
            &test_entitlement(1),
        )
        .unwrap();
    }

    #[test]
    fn verified_import_enrollment_is_idempotent_bound_and_retries_after_failure() {
        let pool = discovery_enrollment_test_pool();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-one")).unwrap();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-two")).unwrap();

        let first = imported_discovery_posting(
            "ashby_import",
            "https://jobs.ashbyhq.com/acme/job-123",
            "track-one",
        );
        enroll_verified_import_discovery_source(&pool, "acct-one", &first).unwrap();
        enroll_verified_import_discovery_source(&pool, "acct-one", &first).unwrap();
        let account_one_sources = jobs::list_discovery_sources(&pool, "acct-one").unwrap();
        assert_eq!(account_one_sources.len(), 1);
        assert_eq!(account_one_sources[0].provider, "ashby");
        assert_eq!(account_one_sources[0].source_key, "acme");
        assert_eq!(account_one_sources[0].track_id, "track-one");

        let second_track = imported_discovery_posting(
            "ashby_import",
            "https://jobs.ashbyhq.com/acme/job-123",
            "track-two",
        );
        let conflict =
            enroll_verified_import_discovery_source(&pool, "acct-one", &second_track).unwrap_err();
        assert!(conflict
            .to_string()
            .contains("already bound to another Career Track"));
        assert_eq!(
            jobs::list_discovery_sources(&pool, "acct-one")
                .unwrap()
                .len(),
            1
        );

        assert!(!try_enroll_verified_import_discovery_source(
            &pool, "acct-two", &first
        ));
        assert!(jobs::list_discovery_sources(&pool, "acct-two")
            .unwrap()
            .is_empty());

        jobs::upsert_track(&pool, "acct-two", &test_track("track-two-account-two")).unwrap();
        let second_account = imported_discovery_posting(
            "ashby_import",
            "https://jobs.ashbyhq.com/acme/job-123",
            "track-two-account-two",
        );
        assert!(try_enroll_verified_import_discovery_source(
            &pool,
            "acct-two",
            &second_account
        ));
        assert!(try_enroll_verified_import_discovery_source(
            &pool,
            "acct-two",
            &second_account
        ));
        let account_two_sources = jobs::list_discovery_sources(&pool, "acct-two").unwrap();
        assert_eq!(account_two_sources.len(), 1);
        assert_eq!(account_two_sources[0].account_id, "acct-two");
        assert_eq!(account_two_sources[0].track_id, "track-two-account-two");

        let manual = imported_discovery_posting(
            "pasted_link",
            "https://jobs.ashbyhq.com/acme/job-123",
            "track-one",
        );
        enroll_verified_import_discovery_source(&pool, "acct-one", &manual).unwrap();
        assert_eq!(
            jobs::list_discovery_sources(&pool, "acct-one")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn workspace_backfills_a_legacy_verified_import_once() {
        let pool = discovery_enrollment_test_pool();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-one")).unwrap();
        store_discovery_posting(
            &pool,
            "acct-one",
            &imported_discovery_posting(
                "workday_import",
                "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Ireland-Dublin/Senior-Software-Engineer_JR-0107796",
                "track-one",
            ),
        );
        let postings = jobs::list_postings(&pool, "acct-one").unwrap();

        assert_eq!(
            backfill_verified_import_discovery_sources(
                &pool,
                "acct-one",
                &postings,
                &CareerProfile::default(),
                &JobPreferences::default(),
            ),
            1
        );
        assert_eq!(
            backfill_verified_import_discovery_sources(
                &pool,
                "acct-one",
                &postings,
                &CareerProfile::default(),
                &JobPreferences::default(),
            ),
            0
        );
        let sources = jobs::list_discovery_sources(&pool, "acct-one").unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].provider, "workday");
        assert_eq!(sources[0].source_key, "workday~wd5~Workday");
        assert_eq!(sources[0].track_id, "track-one");
    }

    #[test]
    fn workspace_backfill_repairs_a_legacy_source_without_membership() {
        let pool = discovery_enrollment_test_pool();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-one")).unwrap();
        let stored = store_discovery_posting(
            &pool,
            "acct-one",
            &imported_discovery_posting(
                "lever_import",
                "https://jobs.eu.lever.co/acme/job-123",
                "track-one",
            ),
        );
        jobs::upsert_discovery_source(
            &pool,
            "acct-one",
            &jobs::DiscoverySourceInput {
                track_id: "track-one".to_string(),
                provider: "lever".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: 0,
            },
        )
        .unwrap();

        assert_eq!(
            jobs::verified_import_discovery_membership_job_id(
                &pool, "acct-one", "lever", "acme", "job-123",
            )
            .unwrap(),
            None,
        );
        assert_eq!(
            backfill_verified_import_discovery_sources(
                &pool,
                "acct-one",
                std::slice::from_ref(&stored),
                &CareerProfile::default(),
                &JobPreferences::default(),
            ),
            0,
        );
        assert_eq!(
            jobs::verified_import_discovery_membership_job_id(
                &pool, "acct-one", "lever", "acme", "job-123",
            )
            .unwrap(),
            Some(stored.id),
        );
        assert_eq!(jobs::list_postings(&pool, "acct-one").unwrap().len(), 1);
    }

    #[test]
    fn workspace_backfill_filters_before_its_bounded_scan() {
        let pool = discovery_enrollment_test_pool();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-one")).unwrap();
        let mut postings = (0..MAX_WORKSPACE_DISCOVERY_BACKFILL_POSTINGS)
            .map(|index| {
                imported_discovery_posting(
                    "pasted_link",
                    &format!("https://example.invalid/manual-{index}"),
                    "track-one",
                )
            })
            .collect::<Vec<_>>();
        postings.push(imported_discovery_posting(
            "lever_import",
            "https://jobs.lever.co/acme/job-123",
            "track-one",
        ));
        assert_eq!(
            backfill_verified_import_discovery_sources(
                &pool,
                "acct-one",
                &postings,
                &CareerProfile::default(),
                &JobPreferences::default(),
            ),
            1
        );
    }

    #[test]
    fn workspace_backfill_skips_manual_restricted_unverified_and_invalid_imports() {
        let pool = discovery_enrollment_test_pool();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-one")).unwrap();
        let postings = [
            imported_discovery_posting(
                "pasted_link",
                "https://jobs.ashbyhq.com/acme/manual-job",
                "track-one",
            ),
            imported_discovery_posting(
                "linkedin_import",
                "https://www.linkedin.com/jobs/view/123",
                "track-one",
            ),
            imported_discovery_posting(
                "greenhouse_import",
                "https://jobs.ashbyhq.com/acme/forged-source",
                "track-one",
            ),
            imported_discovery_posting(
                "ashby_import",
                "https://jobs.ashbyhq.com/acme/unverified-job",
                "track-one",
            ),
            imported_discovery_posting(
                "ashby_import",
                "https://jobs.ashbyhq.com/acme/missing-track",
                "missing-track",
            ),
        ];
        for (index, posting) in postings.into_iter().enumerate() {
            let mut posting = posting;
            if index == 3 {
                posting.last_verified_at_ms = None;
                posting.availability_status = "unknown".to_string();
            }
            store_discovery_posting(&pool, "acct-one", &posting);
        }

        let stored = jobs::list_postings(&pool, "acct-one").unwrap();
        assert_eq!(
            backfill_verified_import_discovery_sources(
                &pool,
                "acct-one",
                &stored,
                &CareerProfile::default(),
                &JobPreferences::default(),
            ),
            0
        );
        assert!(jobs::list_discovery_sources(&pool, "acct-one")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn workspace_backfill_is_quota_aware_and_track_validation_precedes_writes() {
        let pool = discovery_enrollment_test_pool();
        jobs::upsert_track(&pool, "acct-one", &test_track("track-one")).unwrap();
        for index in 0..jobs::DISCOVERY_MAX_SOURCES_PER_TRACK {
            jobs::upsert_discovery_source(
                &pool,
                "acct-one",
                &jobs::DiscoverySourceInput {
                    track_id: "track-one".to_string(),
                    provider: "ashby".to_string(),
                    source_key: format!("existing-board-{index}"),
                    company: "Acme".to_string(),
                    run_interval_ms: 0,
                },
            )
            .unwrap();
        }
        let legacy = imported_discovery_posting(
            "ashby_import",
            "https://jobs.ashbyhq.com/new-board/job-123",
            "track-one",
        );
        assert_eq!(
            backfill_verified_import_discovery_sources(
                &pool,
                "acct-one",
                &[legacy],
                &CareerProfile::default(),
                &JobPreferences::default(),
            ),
            0
        );
        assert_eq!(
            jobs::list_discovery_sources(&pool, "acct-one")
                .unwrap()
                .len(),
            jobs::DISCOVERY_MAX_SOURCES_PER_TRACK
        );
        let error = validate_match_track(&pool, "acct-one", "forged-track").unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(jobs::list_postings(&pool, "acct-one").unwrap().is_empty());
    }

    fn strict_receipt_fixture() -> (
        JobApplication,
        JobPosting,
        ResumeVersion,
        Value,
        BTreeMap<String, String>,
    ) {
        let account_id = "acct-test";
        let identity_id = "identity-test";
        let resume_key = "accounts/acct-test/jobs/app-test/resume.pdf";
        let screenshot_key = "accounts/acct-test/jobs/app-test/confirmation.png";
        let resume_sha = "a".repeat(64);
        let screenshot_sha = "b".repeat(64);
        let mut application = JobApplication {
            id: "app-test".to_string(),
            job_id: "job-test".to_string(),
            resume_version_id: Some("resume-test".to_string()),
            state: "running".to_string(),
            submission_mode: "review_first".to_string(),
            match_score: 90,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt: json!({
                "application_identity": {
                    "id": identity_id,
                    "email": "apply@example.com"
                }
            }),
            run_id: Some("run-test".to_string()),
            created_at_ms: 0,
            updated_at_ms: 0,
            submitted_at_ms: None,
        };
        let posting = JobPosting {
            id: "job-test".to_string(),
            canonical_key: "job-key".to_string(),
            source: "greenhouse".to_string(),
            external_id: "123".to_string(),
            company: "Acme".to_string(),
            title: "Engineer".to_string(),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            description: "Build reliable systems".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-test".to_string(),
            match_score: 90,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(1),
            last_verified_at_ms: Some(1),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: jobs::JobDiscoveryEvidence::verified_original_source(
                "job-key".to_string(),
                "greenhouse:acme".to_string(),
                Some("boards.greenhouse.io".to_string()),
                jobs::now_ms(),
                "a".repeat(64),
            ),
            eligibility: None,
        };
        let resume = ResumeVersion {
            id: "resume-test".to_string(),
            job_id: "job-test".to_string(),
            version_no: 3,
            mode: "factual".to_string(),
            content: json!({}),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "structured-resume-checksum".to_string(),
            created_at_ms: 0,
        };
        let approved_packet = json!({
            "applicationId": "app-test",
            "jobId": "job-test",
            "resumeVersionId": "resume-test",
            "resumeContent": resume.content,
            "coverLetterContent": "",
            "answers": { "email": "apply@example.com" },
            "verifiedClaimIds": [],
            "applicationIdentityId": identity_id,
            "applicationEmail": "apply@example.com",
            "browserProfileId": browser_profile_id(account_id, identity_id)
        });
        let approved_job = json!({
            "externalId": posting.external_id,
            "canonicalUrl": posting.canonical_url,
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
            "workplace": posting.workplace,
            "description": posting.description,
            "source": posting.source,
            "compensation": posting.compensation
        });
        let approved_checksum =
            approved_execution_checksum(&approved_packet, &approved_job).unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 1,
            "approved_at_ms": 1,
            "checksum": approved_checksum,
            "packet": approved_packet,
            "job": approved_job
        });
        let receipt = json!({
            "applicationIdentityId": identity_id,
            "browserProfileId": browser_profile_id(account_id, identity_id),
            "adapter": "greenhouse",
            "adapterVersion": "1.0.0",
            "packet": {
                "jobId": "job-test",
                "resumeVersionId": "resume-test",
                "approvedPacketChecksum": approved_checksum,
                "applicationEmail": "apply@example.com",
                "answers": { "email": "apply@example.com" },
                "verifiedClaimIds": []
            },
            "job": { "canonicalUrl": posting.canonical_url },
            "documents": [{
                "kind": "resume",
                "versionId": "resume-test",
                "storageKey": resume_key,
                "sha256": resume_sha
            }],
            "screenshotKeys": [screenshot_key]
        });
        let verified_objects = BTreeMap::from([
            (resume_key.to_string(), resume_sha),
            (screenshot_key.to_string(), screenshot_sha),
        ]);
        (application, posting, resume, receipt, verified_objects)
    }

    #[test]
    fn legacy_review_approval_remains_valid_but_legacy_auto_submit_fails_closed() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();

        approved_execution_snapshot(&application).unwrap();

        application.submission_mode = "auto_submit".to_string();
        let error = approved_execution_snapshot(&application).unwrap_err();
        assert_eq!(error.0, StatusCode::CONFLICT);
        assert!(error.1.contains("enable Auto-submit"));
    }

    #[test]
    fn auto_submit_admission_is_bound_into_the_approved_packet_checksum() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        application.submission_mode = "auto_submit".to_string();
        let packet = application.receipt["approved_execution"]["packet"].clone();
        let job = application.receipt["approved_execution"]["job"].clone();
        let admission = json!({
            "kind": "track_auto_submit",
            "authorization_id": "auto-auth-one",
            "career_track_id": "track-test",
            "revision_no": 2,
            "authority_fingerprint": "f".repeat(64)
        });
        let checksum = approved_execution_checksum_v2(&packet, &job, &admission).unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 2,
            "approved_at_ms": 1,
            "checksum": checksum,
            "admission": admission,
            "packet": packet,
            "job": job
        });

        approved_execution_snapshot(&application).unwrap();

        application.receipt["approved_execution"]["admission"]["revision_no"] = json!(3);
        let error = approved_execution_snapshot(&application).unwrap_err();
        assert_eq!(error.0, StatusCode::CONFLICT);
        assert!(error.1.contains("changed after review"));
    }

    fn png_fixture() -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend_from_slice(&1u32.to_be_bytes());
        png.extend_from_slice(&1u32.to_be_bytes());
        png.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&[0, 0, 0, 0]);
        png
    }

    fn receipt_preflight_fixture() -> (Value, Vec<ReceiptEvidenceObject>) {
        let (_, _, _, mut receipt, _) = strict_receipt_fixture();
        let pdf = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\nstartxref\n0\n%%EOF\n".to_vec();
        let png = png_fixture();
        let pdf_sha = sha256_hex(&pdf);
        let png_sha = sha256_hex(&png);
        receipt["documents"][0]["sha256"] = json!(pdf_sha);
        let resume_key = receipt["documents"][0]["storageKey"]
            .as_str()
            .unwrap()
            .to_string();
        let screenshot_key = receipt["screenshotKeys"][0].as_str().unwrap().to_string();
        let objects = vec![
            ReceiptEvidenceObject {
                original_key: resume_key,
                kind: "resume".to_string(),
                media_type: "application/pdf".to_string(),
                sha256: pdf_sha,
                bytes_base64: base64::engine::general_purpose::STANDARD.encode(pdf),
            },
            ReceiptEvidenceObject {
                original_key: screenshot_key,
                kind: "screenshot".to_string(),
                media_type: "image/png".to_string(),
                sha256: png_sha,
                bytes_base64: base64::engine::general_purpose::STANDARD.encode(png),
            },
        ];
        (receipt, objects)
    }

    #[test]
    fn public_fact_input_rejects_caller_owned_authority_fields() {
        let forged = serde_json::from_value::<SaveCareerFactRequest>(json!({
            "id": "fact-1",
            "category": "employment",
            "label": "Forged import",
            "value": "Claim",
            "source": "resume_import",
            "verification_status": "confirmed",
            "confirmed_by": "bluey",
            "confirmed_at_ms": 1,
            "schema_version": 99,
            "created_at_ms": 1,
            "updated_at_ms": 1
        }));
        assert!(forged.is_err());
    }

    #[test]
    fn receipt_preflight_rejects_image_documents_mislabeled_media_and_extras() {
        let (receipt, objects) = receipt_preflight_fixture();
        preflight_receipt_evidence(&receipt, objects.clone(), 1024 * 1024).unwrap();

        let png = png_fixture();
        let png_sha = sha256_hex(&png);
        let mut image_receipt = receipt.clone();
        image_receipt["documents"][0]["sha256"] = json!(png_sha);
        let mut image_objects = objects.clone();
        image_objects[0].sha256 = png_sha;
        image_objects[0].bytes_base64 = base64::engine::general_purpose::STANDARD.encode(&png);
        let image_error =
            preflight_receipt_evidence(&image_receipt, image_objects, 1024 * 1024).unwrap_err();
        assert!(image_error.1.contains("valid PDF"));

        let mut mislabeled = objects.clone();
        mislabeled[1].media_type = "application/pdf".to_string();
        let media_error =
            preflight_receipt_evidence(&receipt, mislabeled, 1024 * 1024).unwrap_err();
        assert!(media_error.1.contains("media type"));

        let mut with_extra = objects;
        let pdf = b"%PDF-1.4\n%%EOF\n".to_vec();
        with_extra.push(ReceiptEvidenceObject {
            original_key: "unreferenced-extra.pdf".to_string(),
            kind: "attachment".to_string(),
            media_type: "application/pdf".to_string(),
            sha256: sha256_hex(&pdf),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(pdf),
        });
        let extra_error =
            preflight_receipt_evidence(&receipt, with_extra, 1024 * 1024).unwrap_err();
        assert!(extra_error.1.contains("unreferenced"));
    }

    #[test]
    fn receipt_verified_claims_must_exactly_match_confirmed_resume_claims() {
        let (_, _, _, mut receipt, _) = strict_receipt_fixture();
        receipt["packet"]["verifiedClaimIds"] = json!(["fact-confirmed"]);
        validate_receipt_verified_claim_ids(&receipt, &["fact-confirmed".to_string()]).unwrap();

        receipt["packet"]["verifiedClaimIds"] = json!(["fact-confirmed", "fact-unconfirmed"]);
        let forged = validate_receipt_verified_claim_ids(&receipt, &["fact-confirmed".to_string()])
            .unwrap_err();
        assert!(forged.1.contains("do not match"));
    }

    #[test]
    fn public_job_input_cannot_supply_authority_fields() {
        let input: UserJobInput = serde_json::from_value(json!({
            "canonical_url": "https://boards.greenhouse.io/acme/jobs/123",
            "pasted_description": "Build reliable services",
            "company": "Acme",
            "title": "Software Engineer",
            "location": "New York, NY",
            "track_id": "track-sde",
            "source": "greenhouse_certified",
            "match_score": 100,
            "availability_status": "active",
            "last_verified_at_ms": 42,
            "matched_reasons": ["Caller says perfect"],
            "missing_requirements": []
        }))
        .unwrap();

        let posting = posting_from_user_input(input, None);
        assert_eq!(posting.source, "pasted_link");
        assert_eq!(posting.match_score, 0);
        assert_eq!(posting.availability_status, "unknown");
        assert_eq!(posting.last_verified_at_ms, None);
        assert!(posting.matched_reasons.is_empty());
        assert!(posting.missing_requirements.is_empty());
    }

    #[test]
    fn imported_job_facts_replace_manual_authority_fields() {
        let input: UserJobInput = serde_json::from_value(json!({
            "canonical_url": "https://jobs.lever.co/acme/job-123",
            "company": "Forged employer",
            "title": "Forged role",
            "location": "Forged location",
            "track_id": "track-sde"
        }))
        .unwrap();
        let posting = posting_from_user_input(
            input,
            Some(jobs_import::ImportedJob {
                source: "lever_import".to_string(),
                external_id: "job-123".to_string(),
                company: "Acme".to_string(),
                title: "Platform Engineer".to_string(),
                location: "Sunnyvale, CA".to_string(),
                workplace: "On-site".to_string(),
                canonical_url: "https://jobs.lever.co/acme/job-123".to_string(),
                description: "Build Java services on AWS.".to_string(),
                compensation: "USD 150000-220000 year".to_string(),
                employment_type: "full_time".to_string(),
                posted_at_ms: Some(1_744_222_396_719),
                verified_at_ms: 1_744_222_396_719,
                evidence_hash: "a".repeat(64),
            }),
        );

        assert_eq!(posting.source, "lever_import");
        assert_eq!(posting.company, "Acme");
        assert_eq!(posting.title, "Platform Engineer");
        assert_eq!(posting.location, "Sunnyvale, CA");
        assert_eq!(posting.posted_at_ms, Some(1_744_222_396_719));
        assert_eq!(posting.match_score, 0);
        assert_eq!(posting.availability_status, "active");
        assert!(posting.last_verified_at_ms.is_some());
        assert_eq!(
            posting.discovery_evidence.canonical_job_id.as_deref(),
            Some(posting.canonical_key.as_str())
        );
        assert_eq!(
            posting.discovery_evidence.original_source_status,
            "verified_open"
        );
        assert_eq!(
            posting.discovery_evidence.employer_verification_status,
            "ats_tenant_verified"
        );
        assert_eq!(
            posting.discovery_evidence.scam_risk_status,
            "source_screened"
        );
    }

    #[test]
    fn receipt_bundle_requires_exact_identity_packet_and_verified_evidence() {
        let (application, posting, resume, receipt, verified_objects) = strict_receipt_fixture();
        validate_receipt_bundle(
            "acct-test",
            &application,
            &posting,
            &resume,
            &receipt,
            &verified_objects,
        )
        .unwrap();

        let mut wrong_identity = receipt.clone();
        wrong_identity["applicationIdentityId"] = json!("identity-other");
        assert_eq!(
            validate_receipt_bundle(
                "acct-test",
                &application,
                &posting,
                &resume,
                &wrong_identity,
                &verified_objects,
            )
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );

        let mut wrong_resume = receipt.clone();
        wrong_resume["packet"]["resumeVersionId"] = json!("resume-other");
        assert_eq!(
            validate_receipt_bundle(
                "acct-test",
                &application,
                &posting,
                &resume,
                &wrong_resume,
                &verified_objects,
            )
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );

        let mut changed_answers = receipt.clone();
        changed_answers["packet"]["answers"] = json!({ "email": "other@example.com" });
        let changed_answers_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &posting,
            &resume,
            &changed_answers,
            &verified_objects,
        )
        .unwrap_err();
        assert_eq!(changed_answers_error.0, StatusCode::BAD_REQUEST);
        assert!(changed_answers_error.1.contains("exact application packet"));

        let mut wrong_approval = receipt.clone();
        wrong_approval["packet"]["approvedPacketChecksum"] = json!("d".repeat(64));
        let wrong_approval_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &posting,
            &resume,
            &wrong_approval,
            &verified_objects,
        )
        .unwrap_err();
        assert_eq!(wrong_approval_error.0, StatusCode::BAD_REQUEST);
        assert!(wrong_approval_error.1.contains("exact application packet"));

        let unverified = BTreeMap::new();
        let error = validate_receipt_bundle(
            "acct-test",
            &application,
            &posting,
            &resume,
            &receipt,
            &unverified,
        )
        .unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1.contains("not uploaded and verified"));
    }
}

//! Authenticated Bluey Jobs API.
//!
//! The web product shares Bluey identity and balance, while Jobs records,
//! automation state, and packet metering remain isolated under `/api/jobs`.

use axum::{
    body::Body,
    extract::{rejection::JsonRejection, DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
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
    api::{jobs_import, jobs_resume_generation, jobs_worker_auth::JobsWorkerIdentity, AppState},
    auth::AuthedAccount,
    db::{
        jobs::{
            self, normalize_candidate_employment_type, normalize_candidate_engagement_type,
            AnswerMemory, ApplicationEvidence, ApplicationIdentity, BrowserSession, CandidateEvent,
            CareerFact, CareerProfile, CareerTrack, Intervention, JobApplication,
            JobEligibilityDecision, JobPosting, JobPreferences, JobsEntitlement, JobsIntegration,
            JobsWorkspace, PacketCommitResult, ResumeVersion, RunEvent, RunnerAvailability,
            RunnerChannelAvailability,
        },
        object_uploads::{
            self, ApplicationObjectBinding, NewObjectUpload, NewSubmissionEvidenceCapacity,
            ObjectKind, StorageScope, UploadControlError,
        },
    },
    jobs_ats_target::{parse_provider_application_target, ProviderApplicationTargetPurpose},
    object_storage::{sha256_hex, ObjectStorage},
};

pub(super) type ApiError = (StatusCode, String);

#[derive(Debug, Serialize)]
pub struct WorkflowCommandAdmissionResponse {
    pub schema_version: i64,
    pub command_id: String,
    pub request_id: String,
    pub workflow_id: String,
    pub operation: String,
    pub state: String,
    pub replayed: bool,
}

impl WorkflowCommandAdmissionResponse {
    fn from_admission(admission: &jobs::JobsWorkflowCommandAdmission) -> Self {
        Self {
            schema_version: admission.command.protocol_version,
            command_id: admission.command.id.clone(),
            request_id: admission.command.request_id.clone(),
            workflow_id: admission.command.workflow_id.clone(),
            operation: match admission.command.command_kind {
                jobs::JobsWorkflowCommandKind::Start => "start",
                jobs::JobsWorkflowCommandKind::Resume => "resume",
            }
            .to_string(),
            state: workflow_command_state_name(admission.command.state).to_string(),
            replayed: admission.replayed,
        }
    }
}

fn workflow_command_state_name(state: jobs::JobsWorkflowCommandState) -> &'static str {
    match state {
        jobs::JobsWorkflowCommandState::Pending => "pending",
        jobs::JobsWorkflowCommandState::Claimed => "claimed",
        jobs::JobsWorkflowCommandState::Delivering => "delivering",
        jobs::JobsWorkflowCommandState::DeliveryUnknown => "delivery_unknown",
        jobs::JobsWorkflowCommandState::Accepted => "accepted",
        jobs::JobsWorkflowCommandState::IdentityConflict => "identity_conflict",
        jobs::JobsWorkflowCommandState::Rejected => "rejected",
        jobs::JobsWorkflowCommandState::Cancelled => "cancelled",
    }
}

// Receipts include the exact resume and confirmation screenshot as base64 so
// the server can verify and persist evidence before accepting "submitted".
const RECEIPT_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const DISCOVERY_SNAPSHOT_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const BROWSER_PROFILE_SNAPSHOT_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const EXECUTION_LEASE_CLAIM_BODY_LIMIT_BYTES: usize = 64 * 1024;
const WORKFLOW_COMMAND_BODY_LIMIT_BYTES: usize = 16 * 1024;
const WORKFLOW_INTERVENTION_PREPARE_BODY_LIMIT_BYTES: usize = 256 * 1024;
const TRUSTED_WORKER_RECEIPT_KEY: &str = "_bluey_worker_receipt_v1";
const SUBMISSION_FINGERPRINT_KEY: &str = "_bluey_server_submission_fingerprint_v1";
const MAX_RECEIPT_DOCUMENTS: usize = 8;
const MAX_RECEIPT_SCREENSHOTS: usize = 4;
const MAX_RECEIPT_EVIDENCE_OBJECTS: usize = 12;
const MAX_RECEIPT_EVIDENCE_BYTES: usize = 40 * 1024 * 1024;
const MAX_RECEIPT_BUNDLE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SUBMISSION_EVIDENCE_OBJECTS: i64 = (MAX_RECEIPT_EVIDENCE_OBJECTS + 1) as i64;
const GREENHOUSE_SUBMISSION_ADAPTER_VERSION: &str = "2026.07.1-beta.1";
const LEVER_SUBMISSION_ADAPTER_VERSION: &str = "2026.07.0-beta.1";
// Final employer-submission evidence is part of the account's immutable
// application history. It is retained for the account lifetime and erased by
// the durable account-deletion workflow; the generic artifact TTL must not
// silently sever a Submitted application's receipt pointer.
const SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRY_MS: i64 = i64::MAX;
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
            "/api/jobs/applications/:application_id/reconcile-submission",
            post(reconcile_submission),
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
            "/api/jobs/applications/:application_id/evidence/:evidence_id/download",
            get(download_application_evidence),
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
            "/api/jobs/mailbox-connections/:connection_id/communication-authorization/start",
            post(super::jobs_mailbox_oauth::start_communication_oauth),
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
            post(worker_claim_execution_lease).route_layer(DefaultBodyLimit::max(
                EXECUTION_LEASE_CLAIM_BODY_LIMIT_BYTES,
            )),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/heartbeat",
            post(worker_heartbeat_execution_lease),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/authorize-managed-effect",
            post(worker_authorize_managed_execution_effect),
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
            "/api/jobs/internal/execution-leases/:run_id/reconcile-checkpoint",
            post(worker_reconcile_execution_checkpoint),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/profile/restore",
            post(worker_restore_browser_profile_snapshot),
        )
        .route(
            "/api/jobs/internal/execution-leases/:run_id/profile/store",
            post(worker_store_browser_profile_snapshot).route_layer(DefaultBodyLimit::max(
                BROWSER_PROFILE_SNAPSHOT_BODY_LIMIT_BYTES,
            )),
        )
        .route(
            "/api/jobs/internal/workflow-commands/:request_id/materialize",
            post(worker_materialize_workflow_command)
                .route_layer(DefaultBodyLimit::max(WORKFLOW_COMMAND_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/jobs/internal/workflow-commands/:request_id/intervention/prepare",
            post(worker_prepare_workflow_intervention).route_layer(DefaultBodyLimit::max(
                WORKFLOW_INTERVENTION_PREPARE_BODY_LIMIT_BYTES,
            )),
        )
        .route(
            "/api/jobs/internal/workflow-commands/:request_id/intervention/:intervention_id/publish",
            post(worker_publish_workflow_intervention)
                .route_layer(DefaultBodyLimit::max(WORKFLOW_COMMAND_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/jobs/internal/workflow-commands/:request_id/finalize",
            post(worker_finalize_workflow_execution)
                .route_layer(DefaultBodyLimit::max(WORKFLOW_COMMAND_BODY_LIMIT_BYTES)),
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

fn runner_volume_fleet_is_distribution_ready(status: &jobs::RunnerVolumeFleetStatus) -> bool {
    status.cutover_state == "ready"
        && status.legacy_inventory_state == "ready"
        && status.unresolved_legacy_volume_count == 0
        && status.legacy_inventory_reconciliation_id.is_some()
        && status.legacy_inventory_authority_id.is_some()
        && status.legacy_inventory_authority_sha256.is_some()
        && status.legacy_inventory_root_count == Some(0)
        && status.legacy_inventory_root_set_sha256.as_deref()
            == Some(jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256)
        && status.cutover_enrollment_generation == Some(status.enrollment_generation)
        && status.cutover_purge_generation == Some(status.purge_generation)
        && status.cutover_tombstone_generation == Some(status.tombstone_generation)
        && status.cutover_destruction_generation == Some(status.destruction_generation)
        && status.cutover_legacy_reconciliation_generation
            == Some(status.legacy_reconciliation_generation)
        && status.storage_attestation_count == status.non_destroyed_volume_count
        && status.cutover_storage_attestation_generation
            == Some(status.storage_attestation_generation)
        && status.cutover_storage_attestation_count == Some(status.storage_attestation_count)
        && status.cutover_storage_attestation_set_sha256.as_deref()
            == Some(status.storage_attestation_set_sha256.as_str())
        && status.cutover_legacy_inventory_generation == Some(status.legacy_inventory_generation)
        && status.cutover_legacy_inventory_reconciliation_id
            == status.legacy_inventory_reconciliation_id
        && status.cutover_legacy_inventory_authority_id == status.legacy_inventory_authority_id
        && status.cutover_legacy_inventory_authority_sha256
            == status.legacy_inventory_authority_sha256
        && status.cutover_legacy_inventory_root_count == status.legacy_inventory_root_count
        && status.cutover_legacy_inventory_root_set_sha256
            == status.legacy_inventory_root_set_sha256
        && status.cutover_non_destroyed_volume_count == Some(status.non_destroyed_volume_count)
        && status.cutover_destruction_count == Some(status.destruction_count)
        && status.cutover_unresolved_legacy_volume_count == Some(0)
        && status.attested_reconciled_volume_count == status.non_destroyed_volume_count
        && status.cutover_evidence_ref.is_some()
        && status.cutover_evidence_sha256.is_some()
        && status.cutover_authorized_by.is_some()
        && status.cutover_at_ms.is_some()
}

fn runner_volume_fleet_distribution_ready(pool: &crate::db::DbPool) -> bool {
    jobs::runner_volume_fleet_status(pool)
        .as_ref()
        .is_ok_and(runner_volume_fleet_is_distribution_ready)
}

fn distribution_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn jobs_local_browser_distribution_enabled(pool: &crate::db::DbPool) -> bool {
    distribution_flag_enabled("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED")
        && runner_volume_fleet_distribution_ready(pool)
}

fn jobs_cloud_browser_distribution_enabled(pool: &crate::db::DbPool, account_id: &str) -> bool {
    if !distribution_flag_enabled("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED")
        || !crate::jobs_workflow_dispatch::workflow_command_dispatch_configured_for_admission()
        || !runner_volume_fleet_distribution_ready(pool)
    {
        return false;
    }
    let Some(scope) = crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission() else {
        return false;
    };
    jobs::resolve_managed_cloud_readiness(
        pool,
        &jobs::ManagedCloudReadinessQuery {
            scope,
            account_id: Some(account_id.to_string()),
        },
    )
    .is_ok_and(|readiness| readiness.status.customer_admission)
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
            release: None,
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
            release: None,
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
        release: None,
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
    let auto_submit_reason = runner_auto_submit_reason(&local, &cloud);
    RunnerAvailability {
        local,
        cloud,
        auto_submit_available,
        auto_submit_reason,
    }
}

fn runner_auto_submit_reason(
    local: &RunnerChannelAvailability,
    cloud: &RunnerChannelAvailability,
) -> String {
    if local.available && cloud.available {
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
    }
}

fn account_runner_availability(
    pool: &crate::db::DbPool,
    account_id: &str,
    entitlement: &JobsEntitlement,
) -> Result<RunnerAvailability, ApiError> {
    let local_distribution_enabled = jobs_local_browser_distribution_enabled(pool);
    let cloud_distribution_enabled = jobs_cloud_browser_distribution_enabled(pool, account_id);
    let mut availability = build_runner_availability(
        entitlement,
        local_distribution_enabled,
        cloud_distribution_enabled,
    );
    if entitlement.local_browser {
        let release = if !local_distribution_enabled {
            jobs::LocalBrowserReleaseAvailability::Disabled {
                reason: "Bluey Browser distribution is disabled for this release.".to_string(),
            }
        } else if let Ok(server_release_id) = browser_server_release_id() {
            jobs::local_browser_release_availability_for_distribution(
                pool,
                account_id,
                &server_release_id,
            )
            .map_err(internal)?
        } else {
            jobs::LocalBrowserReleaseAvailability::Unavailable {
                reason: "Bluey Browser release verification is unavailable.".to_string(),
            }
        };
        if !release.is_available() {
            availability.local.available = false;
            availability.local.status = "invited_beta".to_string();
            availability.local.reason = match &release {
                jobs::LocalBrowserReleaseAvailability::Available { reason, .. }
                | jobs::LocalBrowserReleaseAvailability::Disabled { reason }
                | jobs::LocalBrowserReleaseAvailability::Unassigned { reason }
                | jobs::LocalBrowserReleaseAvailability::Unavailable { reason } => reason.clone(),
            };
            availability.local.next_action =
                "Use Review first; no local run or installer is authorized.".to_string();
        }
        availability.local.release = Some(release);
        availability.auto_submit_available =
            availability.local.available || availability.cloud.available;
        availability.auto_submit_reason =
            runner_auto_submit_reason(&availability.local, &availability.cloud);
    }
    Ok(availability)
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
    workspace.runner_availability =
        account_runner_availability(&state.pool, &account.id, &workspace.entitlement)?;
    apply_jobs_distribution_gates(
        &mut workspace.entitlement,
        workspace.runner_availability.local.available,
        workspace.runner_availability.cloud.available,
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
        let runners = account_runner_availability(&state.pool, &account.id, &entitlement)?;
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

#[derive(Debug, Deserialize)]
pub struct ReconcileSubmissionRequest {
    pub outcome: String,
    #[serde(default)]
    pub confirmed: bool,
}

pub async fn reconcile_submission(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
    Json(req): Json<ReconcileSubmissionRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    if !req.confirmed || req.outcome != "not_submitted" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Confirm that the employer did not receive this application.".to_string(),
        ));
    }
    jobs::reconcile_submission_not_submitted(&state.pool, &account.id, &application_id)
        .map_err(submission_domain_error)?
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

pub async fn download_application_evidence(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path((application_id, evidence_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let application = jobs::get_application(&state.pool, &account.id, &application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    let evidence = jobs::list_application_evidence(&state.pool, &account.id, Some(&application_id))
        .map_err(internal)?;
    let selected = evidence
        .iter()
        .find(|item| item.id == evidence_id && item.application_id == application_id)
        .ok_or((
            StatusCode::NOT_FOUND,
            "Application evidence not found.".to_string(),
        ))?;
    let expected_media_type = evidence_download_media_type(&selected.kind).ok_or((
        StatusCode::NOT_FOUND,
        "Application evidence not found.".to_string(),
    ))?;
    let expected_size = validate_evidence_download_binding(
        &account.id,
        &application,
        selected,
        &evidence,
        expected_media_type,
    )?;
    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Application evidence storage is not configured.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    let evidence_limit = if selected.kind == "application_receipt" {
        MAX_RECEIPT_BUNDLE_BYTES
    } else {
        MAX_RECEIPT_EVIDENCE_BYTES
    };
    if selected.storage_key.len() > 4_096
        || !storage.key_belongs_to_account(&selected.storage_key, &account.id)
        || expected_size > storage.max_object_bytes()
        || expected_size > evidence_limit
    {
        return Err(evidence_download_integrity_error(
            &application_id,
            &evidence_id,
            "invalid object scope or recorded size",
        ));
    }
    let stored = storage.get(&selected.storage_key).await.map_err(|error| {
        tracing::warn!(
            error = %error,
            application_id_hash = %sha256_hex(application_id.as_bytes()),
            evidence_id_hash = %sha256_hex(evidence_id.as_bytes()),
            "application evidence object could not be read"
        );
        (
            StatusCode::BAD_GATEWAY,
            "Bluey could not load this application evidence right now.".to_string(),
        )
    })?;
    let stored_media_type = stored
        .content_type
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default();
    if stored.bytes.len() != expected_size
        || sha256_hex(&stored.bytes) != selected.sha256
        || !stored_media_type.eq_ignore_ascii_case(expected_media_type)
        || !valid_downloaded_evidence_structure(&account.id, &application, selected, &stored.bytes)
    {
        return Err(evidence_download_integrity_error(
            &application_id,
            &evidence_id,
            "object read-back did not match immutable evidence",
        ));
    }

    let file_name = evidence_download_file_name(selected);
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, expected_media_type)
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{file_name}\""),
        )
        .header(axum::http::header::CACHE_CONTROL, "private, no-store")
        .header(axum::http::header::PRAGMA, "no-cache")
        .header("x-content-type-options", "nosniff")
        .header(
            axum::http::header::CONTENT_LENGTH,
            expected_size.to_string(),
        )
        .body(Body::from(stored.bytes))
        .map_err(|error| internal(error.into()))
}

fn evidence_download_media_type(kind: &str) -> Option<&'static str> {
    match kind {
        "resume" | "cover_letter" | "attachment" => Some("application/pdf"),
        "application_receipt" => Some("application/json"),
        "submission_confirmation" => Some("image/png"),
        _ => None,
    }
}

fn validate_evidence_download_binding(
    account_id: &str,
    application: &JobApplication,
    selected: &ApplicationEvidence,
    evidence: &[ApplicationEvidence],
    expected_media_type: &str,
) -> Result<usize, ApiError> {
    let failure = || {
        evidence_download_integrity_error(
            &application.id,
            &selected.id,
            "evidence record did not match submitted application authority",
        )
    };
    if application.state != "submitted"
        || application.submitted_at_ms.is_none_or(|value| value <= 0)
        || selected.application_id != application.id
        || selected.media_type != expected_media_type
        || selected.storage_key.trim().is_empty()
        || selected.sha256 != selected.sha256.to_ascii_lowercase()
        || !valid_sha256(&selected.sha256)
    {
        return Err(failure());
    }
    let resume_version_id = application
        .resume_version_id
        .as_deref()
        .ok_or_else(failure)?;
    let expected_resume_version_id = match selected.kind.as_str() {
        "resume" | "application_receipt" | "submission_confirmation" => Some(resume_version_id),
        "cover_letter" | "attachment" => None,
        _ => return Err(failure()),
    };
    if selected.resume_version_id.as_deref() != expected_resume_version_id {
        return Err(failure());
    }
    let expected_size = selected
        .metadata
        .get("size_bytes")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(failure)?;

    let mut receipt_records = evidence
        .iter()
        .filter(|item| item.kind == "application_receipt");
    let receipt_record = receipt_records.next().ok_or_else(failure)?;
    if receipt_records.next().is_some() {
        return Err(failure());
    }
    let confirmation_records = evidence
        .iter()
        .filter(|item| item.kind == "submission_confirmation")
        .collect::<Vec<_>>();
    if !(1..=MAX_RECEIPT_SCREENSHOTS).contains(&confirmation_records.len()) {
        return Err(failure());
    }
    let receipt_id = receipt_record
        .metadata
        .get("receipt_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(failure)?;
    if application.receipt.get("receiptId").and_then(Value::as_str) != Some(receipt_id)
        || application.receipt.get("accountId").and_then(Value::as_str) != Some(account_id)
        || application
            .receipt
            .get("applicationId")
            .and_then(Value::as_str)
            != Some(application.id.as_str())
        || receipt_record.application_id != application.id
        || receipt_record.resume_version_id.as_deref() != Some(resume_version_id)
        || receipt_record.provider.trim().is_empty()
        || selected.provider != receipt_record.provider
        || confirmation_records.iter().any(|confirmation| {
            confirmation.application_id != application.id
                || confirmation.resume_version_id.as_deref() != Some(resume_version_id)
                || confirmation.provider != receipt_record.provider
                || confirmation
                    .metadata
                    .get("receipt_id")
                    .and_then(Value::as_str)
                    != Some(receipt_id)
        })
    {
        return Err(failure());
    }
    validate_receipt_evidence_record(application, receipt_record, receipt_id)
        .map_err(|_| failure())?;
    validate_confirmation_evidence_records(application, &confirmation_records, receipt_id)
        .map_err(|_| failure())?;
    if matches!(
        selected.kind.as_str(),
        "resume" | "cover_letter" | "attachment"
    ) {
        validate_document_evidence_record(application, selected, receipt_id, resume_version_id)
            .map_err(|_| failure())?;
    }
    Ok(expected_size)
}

fn validate_document_evidence_record(
    application: &JobApplication,
    evidence: &ApplicationEvidence,
    receipt_id: &str,
    resume_version_id: &str,
) -> Result<(), ()> {
    let size_bytes = evidence
        .metadata
        .get("size_bytes")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or(())?;
    if !matches!(
        evidence.kind.as_str(),
        "resume" | "cover_letter" | "attachment"
    ) || evidence.media_type != "application/pdf"
        || !evidence.file_name.to_ascii_lowercase().ends_with(".pdf")
        || evidence
            .metadata
            .get("attached_to_submission")
            .and_then(Value::as_bool)
            != Some(true)
        || evidence.metadata.get("receipt_id").and_then(Value::as_str) != Some(receipt_id)
    {
        return Err(());
    }
    if evidence.kind == "resume" {
        if evidence.resume_version_id.as_deref() != Some(resume_version_id) {
            return Err(());
        }
    } else if evidence.resume_version_id.is_some() {
        return Err(());
    }

    let mut documents = application
        .receipt
        .get("documents")
        .and_then(Value::as_array)
        .ok_or(())?
        .iter()
        .filter(|document| {
            document.get("storageKey").and_then(Value::as_str)
                == Some(evidence.storage_key.as_str())
        });
    let document = documents.next().ok_or(())?;
    if documents.next().is_some()
        || document.get("kind").and_then(Value::as_str) != Some(evidence.kind.as_str())
        || document.get("sha256").and_then(Value::as_str) != Some(evidence.sha256.as_str())
        || document.get("mediaType").and_then(Value::as_str) != Some("application/pdf")
        || (evidence.kind == "resume"
            && document.get("versionId").and_then(Value::as_str) != Some(resume_version_id))
    {
        return Err(());
    }

    let mut manifest_items = application
        .receipt
        .get("evidenceObjects")
        .and_then(Value::as_array)
        .ok_or(())?
        .iter()
        .filter(|item| {
            item.get("storageKey").and_then(Value::as_str) == Some(evidence.storage_key.as_str())
        });
    let manifest = manifest_items.next().ok_or(())?;
    if manifest_items.next().is_some()
        || manifest.get("kind").and_then(Value::as_str) != Some(evidence.kind.as_str())
        || manifest.get("sha256").and_then(Value::as_str) != Some(evidence.sha256.as_str())
        || manifest.get("mediaType").and_then(Value::as_str) != Some("application/pdf")
        || manifest.get("sizeBytes").and_then(Value::as_i64) != Some(size_bytes)
    {
        return Err(());
    }
    Ok(())
}

fn validate_receipt_evidence_record(
    application: &JobApplication,
    evidence: &ApplicationEvidence,
    receipt_id: &str,
) -> Result<(), ()> {
    let size_bytes = evidence
        .metadata
        .get("size_bytes")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or(())?;
    let receipt_object = application
        .receipt
        .get("receiptObject")
        .and_then(Value::as_object)
        .ok_or(())?;
    if evidence.media_type != "application/json"
        || !evidence.file_name.to_ascii_lowercase().ends_with(".json")
        || evidence.storage_key.trim().is_empty()
        || evidence.sha256 != evidence.sha256.to_ascii_lowercase()
        || !valid_sha256(&evidence.sha256)
        || evidence.metadata.get("immutable").and_then(Value::as_bool) != Some(true)
        || evidence
            .metadata
            .get("schema_version")
            .and_then(Value::as_i64)
            != Some(1)
        || evidence.metadata.get("receipt_id").and_then(Value::as_str) != Some(receipt_id)
        || receipt_object.get("storageKey").and_then(Value::as_str)
            != Some(evidence.storage_key.as_str())
        || receipt_object.get("sha256").and_then(Value::as_str) != Some(evidence.sha256.as_str())
        || receipt_object.get("mediaType").and_then(Value::as_str) != Some("application/json")
        || receipt_object.get("schemaVersion").and_then(Value::as_i64) != Some(1)
        || receipt_object.get("sizeBytes").and_then(Value::as_i64) != Some(size_bytes)
    {
        return Err(());
    }
    Ok(())
}

fn validate_confirmation_evidence_records(
    application: &JobApplication,
    evidence: &[&ApplicationEvidence],
    receipt_id: &str,
) -> Result<(), ()> {
    let screenshot_keys = application
        .receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .filter(|keys| !keys.is_empty() && keys.len() <= MAX_RECEIPT_SCREENSHOTS)
        .ok_or(())?;
    if screenshot_keys.len() != evidence.len() {
        return Err(());
    }
    let screenshot_keys = screenshot_keys
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()
        .ok_or(())?;
    if screenshot_keys.iter().any(|key| key.trim().is_empty())
        || screenshot_keys.iter().collect::<BTreeSet<_>>().len() != screenshot_keys.len()
    {
        return Err(());
    }
    let manifest = application
        .receipt
        .get("evidenceObjects")
        .and_then(Value::as_array)
        .ok_or(())?;
    let screenshot_manifest = manifest
        .iter()
        .filter(|item| item.get("kind").and_then(Value::as_str) == Some("screenshot"))
        .collect::<Vec<_>>();
    if screenshot_manifest.len() != screenshot_keys.len()
        || screenshot_keys.iter().any(|key| {
            screenshot_manifest
                .iter()
                .filter(|item| item.get("storageKey").and_then(Value::as_str) == Some(*key))
                .count()
                != 1
        })
    {
        return Err(());
    }

    let mut seen_indexes = BTreeSet::new();
    let mut seen_storage_keys = BTreeSet::new();
    let mut seen_file_names = BTreeSet::new();
    for record in evidence {
        let size_bytes = record
            .metadata
            .get("size_bytes")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .ok_or(())?;
        let metadata_keys = record
            .metadata
            .get("screenshot_keys")
            .and_then(Value::as_array)
            .ok_or(())?;
        if metadata_keys.len() != screenshot_keys.len()
            || metadata_keys
                .iter()
                .zip(&screenshot_keys)
                .any(|(actual, expected)| actual.as_str() != Some(*expected))
        {
            return Err(());
        }
        let index = match (
            record.metadata.get("screenshot_index"),
            record.metadata.get("screenshot_count"),
        ) {
            (None, None)
                if screenshot_keys.len() == 1 && record.metadata.get("immutable").is_none() =>
            {
                0
            }
            (Some(index), Some(count))
                if count.as_u64() == Some(screenshot_keys.len() as u64)
                    && index.as_u64().is_some_and(|index| {
                        (1..=screenshot_keys.len() as u64).contains(&index)
                    })
                    && record.metadata.get("immutable").and_then(Value::as_bool) == Some(true) =>
            {
                usize::try_from(index.as_u64().ok_or(())? - 1).map_err(|_| ())?
            }
            _ => return Err(()),
        };
        let mut manifest_items = screenshot_manifest.iter().filter(|item| {
            item.get("storageKey").and_then(Value::as_str) == Some(record.storage_key.as_str())
        });
        let manifest_item = manifest_items.next().ok_or(())?;
        if manifest_items.next().is_some()
            || record.media_type != "image/png"
            || !record.file_name.to_ascii_lowercase().ends_with(".png")
            || record.storage_key != screenshot_keys[index]
            || record.sha256 != record.sha256.to_ascii_lowercase()
            || !valid_sha256(&record.sha256)
            || record.metadata.get("receipt_id").and_then(Value::as_str) != Some(receipt_id)
            || record
                .metadata
                .get("evidence_strength")
                .and_then(Value::as_str)
                != Some("browser_confirmed")
            || record
                .metadata
                .get("confirmation")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            || manifest_item.get("sha256").and_then(Value::as_str) != Some(record.sha256.as_str())
            || manifest_item.get("mediaType").and_then(Value::as_str) != Some("image/png")
            || manifest_item.get("sizeBytes").and_then(Value::as_i64) != Some(size_bytes)
            || !seen_indexes.insert(index)
            || !seen_storage_keys.insert(record.storage_key.as_str())
            || !seen_file_names.insert(record.file_name.as_str())
        {
            return Err(());
        }
    }
    if seen_indexes.len() != screenshot_keys.len()
        || seen_storage_keys.len() != screenshot_keys.len()
    {
        return Err(());
    }
    Ok(())
}

fn valid_downloaded_evidence_structure(
    account_id: &str,
    application: &JobApplication,
    evidence: &ApplicationEvidence,
    bytes: &[u8],
) -> bool {
    match evidence.kind.as_str() {
        "application_receipt" => {
            let Ok(bundle) = serde_json::from_slice::<Value>(bytes) else {
                return false;
            };
            let receipt_id = evidence.metadata.get("receipt_id").and_then(Value::as_str);
            let fingerprint = application
                .receipt
                .get(SUBMISSION_FINGERPRINT_KEY)
                .and_then(Value::as_str);
            let mut expected_receipt = application.receipt.clone();
            let Some(expected_receipt) = expected_receipt.as_object_mut() else {
                return false;
            };
            expected_receipt.remove("receiptObject");
            let expected_job = expected_receipt.get("job");
            bundle.get("schemaVersion").and_then(Value::as_i64) == Some(1)
                && bundle.get("bundleId").and_then(Value::as_str) == fingerprint
                && fingerprint
                    .is_some_and(|value| value == value.to_ascii_lowercase() && valid_sha256(value))
                && bundle.get("accountId").and_then(Value::as_str) == Some(account_id)
                && bundle.get("applicationId").and_then(Value::as_str)
                    == Some(application.id.as_str())
                && bundle.get("receiptId").and_then(Value::as_str) == receipt_id
                && expected_job.is_some()
                && bundle.get("job") == expected_job
                && bundle
                    .pointer("/receipt/packet/jobId")
                    .and_then(Value::as_str)
                    == Some(application.job_id.as_str())
                && bundle.pointer("/resume/id").and_then(Value::as_str)
                    == application.resume_version_id.as_deref()
                && bundle.get("receipt") == Some(&Value::Object(expected_receipt.clone()))
        }
        "resume" | "cover_letter" | "attachment" => valid_pdf(bytes),
        "submission_confirmation" => valid_png(bytes),
        _ => false,
    }
}

fn evidence_download_file_name(evidence: &ApplicationEvidence) -> String {
    let (fallback, extension) = match evidence.kind.as_str() {
        "resume" => ("bluey-submitted-resume", "pdf"),
        "cover_letter" => ("bluey-submitted-cover-letter", "pdf"),
        "attachment" => ("bluey-submitted-attachment", "pdf"),
        "application_receipt" => ("bluey-application-receipt", "json"),
        "submission_confirmation" => ("bluey-submission-confirmation", "png"),
        _ => ("bluey-application-evidence", "bin"),
    };
    let leaf = evidence.file_name.replace('\\', "/");
    let leaf = leaf.rsplit('/').next().unwrap_or_default().trim();
    let stem = leaf.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(leaf);
    let stem = safe_file_part(stem);
    format!(
        "{}.{}",
        if stem.is_empty() { fallback } else { &stem },
        extension
    )
}

fn evidence_download_integrity_error(
    application_id: &str,
    evidence_id: &str,
    reason: &'static str,
) -> ApiError {
    tracing::warn!(
        application_id_hash = %sha256_hex(application_id.as_bytes()),
        evidence_id_hash = %sha256_hex(evidence_id.as_bytes()),
        reason,
        "application evidence download failed closed"
    );
    (
        StatusCode::CONFLICT,
        "Stored application evidence did not pass integrity verification.".to_string(),
    )
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
    pub workflow_command: Option<WorkflowCommandAdmissionResponse>,
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
    let runners = account_runner_availability(&state.pool, &account.id, &entitlement)?;
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
    let existing_run_id = if matches!(
        application.state.as_str(),
        "queued" | "running" | "needs_input"
    ) {
        application.run_id.clone()
    } else {
        None
    };
    let existing_session = if let Some(run_id) = existing_run_id.as_deref() {
        let cloud_session_id = format!("cloud-{}", application.id);
        jobs::list_browser_sessions(&state.pool, &account.id)
            .map_err(internal)?
            .into_iter()
            .find(|session| session.id == run_id || session.id == cloud_session_id)
    } else {
        None
    };
    if existing_session
        .as_ref()
        .is_some_and(|session| session.runner != req.runner)
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
    let run_id = existing_run_id.unwrap_or_else(|| {
        application_run_id(
            &account.id,
            &application.id,
            &resume.id,
            application.updated_at_ms,
        )
    });
    let private_workflow_input = approved_workflow_input(
        &account.id,
        &application,
        &posting,
        &resume,
        &req.runner,
        &run_id,
    )?;
    if req.runner == "cloud" {
        let workflow_id = workflow_id_for_run(&run_id);
        let browser_session_id = format!("cloud-{}", application.id);
        let browser_session = existing_session
            .as_ref()
            .filter(|session| session.id == browser_session_id)
            .cloned()
            .unwrap_or_else(|| BrowserSession {
                id: browser_session_id.clone(),
                runner: "cloud".to_string(),
                status: "queued".to_string(),
                current_company: posting.company.clone(),
                current_step: "Waiting for a browser".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            });
        let admission = jobs::stage_cloud_workflow_start(
            &state.pool,
            &jobs::StageCloudWorkflowStart {
                account_id: account.id.clone(),
                application_id: application.id.clone(),
                run_id: run_id.clone(),
                workflow_id: workflow_id.clone(),
                idempotency_key: run_id.clone(),
                workflow_input: private_workflow_input,
                browser_session,
                now_ms: jobs::now_ms(),
            },
        )
        .map_err(domain_error)?;
        application = jobs::get_application(&state.pool, &account.id, &application.id)
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
        let browser_session = jobs::list_browser_sessions(&state.pool, &account.id)
            .map_err(internal)?
            .into_iter()
            .find(|session| session.id == browser_session_id)
            .ok_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                "The queued cloud browser session is unavailable.".to_string(),
            ))?;
        return Ok(Json(QueueApplicationRunResponse {
            application,
            browser_session,
            workflow_id,
            run_id,
            workflow_command: Some(WorkflowCommandAdmissionResponse::from_admission(&admission)),
            launch_url: None,
        }));
    }

    jobs::reserve_application_attempt(&state.pool, &account.id, &application.id, "local")
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
    let mut browser_session = existing_session.unwrap_or_else(|| BrowserSession {
        id: run_id.clone(),
        runner: "local".to_string(),
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
    let local_workflow_input = json!({
        "accountId": private_workflow_input["accountId"].clone(),
        "applicationId": private_workflow_input["applicationId"].clone(),
        "jobId": private_workflow_input["jobId"].clone(),
        "canonicalJobKey": private_workflow_input["canonicalJobKey"].clone(),
        "packetId": private_workflow_input["packetId"].clone(),
        "applicationIdentityId": private_workflow_input["applicationIdentityId"].clone(),
        "browserProfileId": private_workflow_input["browserProfileId"].clone(),
        "packet": private_workflow_input["packet"].clone(),
        "job": private_workflow_input["job"].clone(),
        "runner": "local",
        "url": private_workflow_input["url"].clone(),
        "idempotencyKey": run_id,
        "runId": run_id,
        "browserSessionId": format!("local-{}", application.id),
    });
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
            local_workflow_input,
            jobs::now_ms() + 24 * 60 * 60 * 1_000,
        )
        .map_err(internal)?
    };
    Ok(Json(QueueApplicationRunResponse {
        application,
        browser_session,
        workflow_id: String::new(),
        run_id: run_id.clone(),
        workflow_command: None,
        launch_url: Some(format!(
            "bluey-jobs://run/{run_id}?ticket={}",
            ticket.ticket_secret
        )),
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

fn workflow_id_for_run(run_id: &str) -> String {
    format!("bluey-jobs-v2-{run_id}")
}

fn approved_workflow_input(
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    resume: &ResumeVersion,
    runner: &str,
    run_id: &str,
) -> Result<Value, ApiError> {
    if !matches!(runner, "local" | "cloud") {
        return bad_request("Choose the local or cloud runner.");
    }
    let (identity_id, identity_email) = approved_application_identity(application)?;
    let (mut packet, job, approved_packet_checksum) = approved_execution_snapshot(application)?;
    validate_approved_execution_matches(
        application,
        posting,
        resume,
        &identity_id,
        &identity_email,
        &packet,
        &job,
    )?;
    attach_approved_execution_transport(application, &mut packet, approved_packet_checksum)?;
    Ok(json!({
        "accountId": account_id,
        "applicationId": application.id,
        "jobId": posting.id,
        "canonicalJobKey": posting.canonical_key,
        "packetId": resume.id,
        "applicationIdentityId": identity_id,
        "browserProfileId": browser_profile_id(account_id, &identity_id),
        "packet": packet,
        "job": job,
        "runner": runner,
        "url": posting.canonical_url,
        "idempotencyKey": run_id,
    }))
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
        if application.submission_mode == "auto_submit"
            && !jobs::application_has_frozen_ats_certification(application)
        {
            return Err((
                StatusCode::CONFLICT,
                "This Auto-submit packet predates exact ATS certification. Prepare and approve it again."
                    .to_string(),
            ));
        }
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
    let approved_at_ms = jobs::now_ms();
    let (schema_version, admission) = if application.submission_mode == "auto_submit" {
        let authorization = jobs::require_valid_auto_submit_authorization(
            &state.pool,
            account_id,
            account_email,
            &posting.track_id,
        )
        .map_err(domain_error)?;
        let resolution = jobs::resolve_ats_certification_for_posting(
            &state.pool,
            account_id,
            posting,
            None,
            approved_at_ms,
        )
        .map_err(|error| match error {
            jobs::AtsCertificationAuthorityError::Storage(error) => internal(error),
            _ => (
                StatusCode::CONFLICT,
                "This exact ATS target does not have current server-owned certification. Review the packet or try again after certification is restored."
                    .to_string(),
            ),
        })?;
        let binding = resolution
            .active_binding
            .as_ref()
            .filter(|_| resolution.status.status == "active")
            .ok_or_else(|| {
                (
                    StatusCode::CONFLICT,
                    "This exact ATS target does not have current server-owned certification. Review the packet or try again after certification is restored."
                        .to_string(),
                )
            })?;
        let ats_certification = jobs::ats_frozen_certification_admission_projection(binding)
            .map_err(|_| {
                (
                    StatusCode::CONFLICT,
                    "This exact ATS certification cannot be frozen into the approved packet. Review the packet or prepare it again."
                        .to_string(),
                )
            })?;
        (
            3,
            json!({
                "kind": "track_auto_submit",
                "authorization_id": authorization.id,
                "career_track_id": authorization.career_track_id,
                "revision_no": authorization.revision_no,
                "authority_fingerprint": authorization.authority_fingerprint,
                "ats_certification": ats_certification,
            }),
        )
    } else {
        (
            2,
            json!({
                "kind": "review_approval",
            }),
        )
    };
    let checksum =
        approved_execution_checksum_with_admission(schema_version, &packet, &job, &admission)?;
    let approved_execution = json!({
        "schema_version": schema_version,
        "approved_at_ms": approved_at_ms,
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
    if !matches!(schema_version, 1..=3) {
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
        validate_approved_execution_admission(application, schema_version, admission)?;
        approved_execution_checksum_with_admission(schema_version, &packet, &job, admission)?
    };
    if checksum.len() != 64 || expected_checksum != checksum {
        return Err((
            StatusCode::CONFLICT,
            "The approved application packet changed after review. Prepare it again.".to_string(),
        ));
    }
    Ok((packet, job, checksum))
}

fn attach_approved_execution_transport(
    application: &JobApplication,
    packet: &mut Value,
    checksum: String,
) -> Result<(), ApiError> {
    let approved = application
        .receipt
        .get("approved_execution")
        .and_then(Value::as_object)
        .ok_or((
            StatusCode::CONFLICT,
            "The approved application packet is incomplete. Prepare it again.".to_string(),
        ))?;
    let schema_version = approved
        .get("schema_version")
        .and_then(Value::as_i64)
        .filter(|value| matches!(*value, 1..=3))
        .ok_or((
            StatusCode::CONFLICT,
            "This approved packet uses an unsupported version. Prepare it again.".to_string(),
        ))?;
    let fields = packet.as_object_mut().ok_or((
        StatusCode::CONFLICT,
        "The approved application packet is incomplete. Prepare it again.".to_string(),
    ))?;
    fields.insert(
        "approvedPacketChecksum".to_string(),
        Value::String(checksum),
    );
    fields.insert(
        "approvedExecutionSchemaVersion".to_string(),
        Value::Number(schema_version.into()),
    );
    if matches!(schema_version, 2 | 3) {
        fields.insert(
            "approvedExecutionAdmission".to_string(),
            approved
                .get("admission")
                .filter(|value| value.is_object())
                .cloned()
                .ok_or((
                    StatusCode::CONFLICT,
                    "The application approval proof is incomplete. Prepare it again.".to_string(),
                ))?,
        );
    }
    Ok(())
}

fn approved_execution_checksum(packet: &Value, job: &Value) -> Result<String, ApiError> {
    let canonical = canonical_json_value(&json!({
        "schema_version": 1,
        "packet": packet,
        "job": job,
    }))?;
    let bytes = serde_json::to_vec(&canonical).map_err(|error| internal(error.into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
fn approved_execution_checksum_v2(
    packet: &Value,
    job: &Value,
    admission: &Value,
) -> Result<String, ApiError> {
    approved_execution_checksum_with_admission(2, packet, job, admission)
}

fn approved_execution_checksum_with_admission(
    schema_version: i64,
    packet: &Value,
    job: &Value,
    admission: &Value,
) -> Result<String, ApiError> {
    if !matches!(schema_version, 2 | 3) {
        return Err((
            StatusCode::CONFLICT,
            "This approved packet uses an unsupported version. Prepare it again.".to_string(),
        ));
    }
    let canonical = canonical_json_value(&json!({
        "schema_version": schema_version,
        "admission": admission,
        "packet": packet,
        "job": job,
    }))?;
    let bytes = serde_json::to_vec(&canonical).map_err(|error| internal(error.into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_approved_execution_admission(
    application: &JobApplication,
    schema_version: i64,
    admission: &Value,
) -> Result<(), ApiError> {
    let Some(admission) = admission.as_object() else {
        return Err((
            StatusCode::CONFLICT,
            "The application approval proof is incomplete. Prepare it again.".to_string(),
        ));
    };
    let kind = admission
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if application.submission_mode == "auto_submit" {
        let complete = kind == "track_auto_submit"
            && approved_execution_object_has_keys(
                admission,
                if schema_version == 3 {
                    &[
                        "ats_certification",
                        "authority_fingerprint",
                        "authorization_id",
                        "career_track_id",
                        "kind",
                        "revision_no",
                    ]
                } else {
                    &[
                        "authority_fingerprint",
                        "authorization_id",
                        "career_track_id",
                        "kind",
                        "revision_no",
                    ]
                },
            )
            && admission
                .get("authorization_id")
                .and_then(Value::as_str)
                .is_some_and(valid_approved_execution_identifier)
            && admission
                .get("career_track_id")
                .and_then(Value::as_str)
                .is_some_and(valid_approved_execution_identifier)
            && admission
                .get("revision_no")
                .and_then(Value::as_i64)
                .is_some_and(|value| value > 0)
            && admission
                .get("authority_fingerprint")
                .and_then(Value::as_str)
                .is_some_and(valid_approved_execution_sha256)
            && (schema_version != 3
                || admission
                    .get("ats_certification")
                    .is_some_and(valid_ats_certification_admission));
        if !complete {
            return Err((
                StatusCode::CONFLICT,
                "The Auto-submit authorization proof is incomplete. Review the Career Track again."
                    .to_string(),
            ));
        }
    } else if kind != "review_approval" || !approved_execution_object_has_keys(admission, &["kind"])
    {
        return Err((
            StatusCode::CONFLICT,
            "Approve this exact application packet before starting a browser runner.".to_string(),
        ));
    }
    Ok(())
}

fn valid_ats_certification_admission(value: &Value) -> bool {
    let Some(certification) = value.as_object() else {
        return false;
    };
    if !approved_execution_object_has_keys(
        certification,
        &[
            "activation_generation",
            "activation_sha256",
            "adapter_bundle_sha256",
            "adapter_version",
            "expires_at_ms",
            "layout_contract_version",
            "layout_set_sha256",
            "manifest_sha256",
            "provider",
            "runner_target_sha256s",
            "schema_version",
            "surface_sha256",
            "target_key_sha256",
            "variant_key",
        ],
    ) || certification.get("schema_version").and_then(Value::as_i64) != Some(1)
    {
        return false;
    }
    let provider = certification
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let adapter_version = certification
        .get("adapter_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        (provider, adapter_version),
        ("greenhouse", "2026.07.1-beta.1") | ("lever", "2026.07.0-beta.1")
    ) || certification
        .get("variant_key")
        .and_then(Value::as_str)
        .is_none_or(|value| !valid_approved_execution_identifier(value))
        || certification
            .get("layout_contract_version")
            .and_then(Value::as_i64)
            .is_none_or(|value| value <= 0)
        || certification
            .get("activation_generation")
            .and_then(Value::as_i64)
            .is_none_or(|value| value <= 0)
        || certification
            .get("expires_at_ms")
            .and_then(Value::as_i64)
            .is_none_or(|value| value <= 0)
    {
        return false;
    }
    for key in [
        "activation_sha256",
        "adapter_bundle_sha256",
        "layout_set_sha256",
        "manifest_sha256",
        "surface_sha256",
        "target_key_sha256",
    ] {
        if !certification
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(valid_approved_execution_sha256)
        {
            return false;
        }
    }
    let Some(targets) = certification
        .get("runner_target_sha256s")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if targets.is_empty() || targets.len() > 2 {
        return false;
    }
    let mut previous: Option<&str> = None;
    for target in targets {
        let Some(target) = target.as_str().filter(|value| {
            valid_approved_execution_sha256(value)
                && previous.is_none_or(|previous| previous < *value)
        }) else {
            return false;
        };
        previous = Some(target);
    }
    true
}

fn approved_execution_object_has_keys(
    value: &serde_json::Map<String, Value>,
    expected: &[&str],
) -> bool {
    value.len() == expected.len() && expected.iter().all(|key| value.contains_key(*key))
}

fn valid_approved_execution_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value.trim() == value
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn valid_approved_execution_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_json_value(value: &Value) -> Result<Value, ApiError> {
    const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    match value {
        Value::Array(values) => values
            .iter()
            .map(canonical_json_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => {
            let sorted = values
                .iter()
                .map(|(key, value)| Ok((key.clone(), canonical_json_value(value)?)))
                .collect::<Result<BTreeMap<_, _>, ApiError>>()?;
            Ok(Value::Object(sorted.into_iter().collect()))
        }
        Value::Number(number) => {
            let interoperable = number
                .as_i64()
                .is_some_and(|value| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value))
                || number
                    .as_u64()
                    .is_some_and(|value| value <= MAX_SAFE_INTEGER as u64);
            if !interoperable {
                return Err((
                    StatusCode::CONFLICT,
                    "The approved application packet contains a non-interoperable number. Prepare it again."
                        .to_string(),
                ));
            }
            Ok(value.clone())
        }
        other => Ok(other.clone()),
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

fn validate_frozen_approved_execution_matches(
    application: &JobApplication,
    resume: &ResumeVersion,
    identity_id: &str,
    identity_email: &str,
    packet: &Value,
    job: &Value,
) -> Result<(), ApiError> {
    let matches = packet.get("applicationId").and_then(Value::as_str)
        == Some(application.id.as_str())
        && packet.get("jobId").and_then(Value::as_str) == Some(application.job_id.as_str())
        && packet.get("resumeVersionId").and_then(Value::as_str) == Some(resume.id.as_str())
        && packet.get("applicationIdentityId").and_then(Value::as_str) == Some(identity_id)
        && packet.get("applicationEmail").and_then(Value::as_str) == Some(identity_email)
        && job
            .get("canonicalUrl")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
    if !matches {
        return Err((
            StatusCode::CONFLICT,
            "The frozen approved packet does not match this application, resume, or application email."
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
    let runners = account_runner_availability(&state.pool, &account.id, &entitlement)?;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_command: Option<WorkflowCommandAdmissionResponse>,
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
    let remembered_answer = None;
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
        if !matches!(original_status.as_str(), "open" | "approved") {
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
        let scope = if req.scope.trim().is_empty() {
            "account".to_string()
        } else {
            req.scope.trim().to_ascii_lowercase()
        };
        let scope_id = req
            .scope_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if req.remember {
            if !matches!(scope.as_str(), "account" | "track" | "company") {
                return bad_request("Choose where this answer should be reused.");
            }
            if scope != "account" && scope_id.is_none() {
                return bad_request("Choose where this answer should be reused.");
            }
            if scope == "track"
                && !jobs::list_tracks(&state.pool, &account.id)
                    .map_err(internal)?
                    .iter()
                    .any(|track| Some(track.id.as_str()) == scope_id.as_deref())
            {
                return bad_request("Career track not found.");
            }
        }

        let revision = jobs::resolve_intervention_answer_for_review(
            &state.pool,
            &account.id,
            &intervention_id,
            answer,
        )
        .map_err(domain_error)?;
        let remembered_answer = if req.remember {
            let memory = AnswerMemory {
                id: String::new(),
                key: String::new(),
                question: revision.question.clone(),
                value: answer.to_string(),
                scope,
                scope_id,
                confirmed: true,
                source: "intervention".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                last_used_at_ms: None,
                use_count: 0,
            };
            match jobs::save_answer_memory(&state.pool, &account.id, &memory) {
                Ok(memory) => Some(memory),
                Err(error) => {
                    tracing::error!(
                        application_id = %revision.application.id,
                        intervention_id = %revision.intervention.id,
                        error = %error,
                        "Bluey Jobs saved an intervention answer but could not update answer memory"
                    );
                    None
                }
            }
        } else {
            None
        };
        return Ok(Json(InterventionResolutionResult {
            intervention: revision.intervention,
            answer_memory: remembered_answer,
            application: Some(revision.application),
            local_resume: None,
            workflow_command: None,
        }));
    }
    let resumes_runner = matches!(action.as_str(), "approve_email_otp" | "approve_submission")
        && updated.resume_after_resolution
        && matches!(updated.status.as_str(), "approved" | "resolved");
    if resumes_runner {
        if let Some(application_id) = updated.application_id.as_deref() {
            let application = jobs::get_application(&state.pool, &account.id, application_id)
                .map_err(internal)?
                .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
            let run_id = application.run_id.as_deref().ok_or((
                StatusCode::CONFLICT,
                "This application has no active browser run.".to_string(),
            ))?;
            let sessions =
                jobs::list_browser_sessions(&state.pool, &account.id).map_err(internal)?;
            let local_session = action == "approve_submission"
                && sessions
                    .iter()
                    .any(|session| session.id == run_id && session.runner == "local");
            if !local_session {
                let posting = jobs::get_posting(&state.pool, &account.id, &application.job_id)
                    .map_err(internal)?
                    .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
                let resume_id = application.resume_version_id.as_deref().ok_or((
                    StatusCode::CONFLICT,
                    "The approved application resume is unavailable.".to_string(),
                ))?;
                let resume = jobs::get_resume_version(&state.pool, &account.id, resume_id)
                    .map_err(internal)?
                    .ok_or((
                        StatusCode::CONFLICT,
                        "The approved application resume is unavailable.".to_string(),
                    ))?;
                let workflow_id = workflow_id_for_run(run_id);
                let browser_session_id = format!("cloud-{}", application.id);
                if !sessions.iter().any(|session| {
                    session.id == browser_session_id
                        && session.runner == "cloud"
                        && session.application_id.as_deref() == Some(application.id.as_str())
                }) {
                    return Err((
                        StatusCode::CONFLICT,
                        "The cloud browser run is not waiting for this approval.".to_string(),
                    ));
                }
                let field = updated
                    .metadata
                    .pointer("/receipt/intervention/field")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let workflow_input = approved_workflow_input(
                    &account.id,
                    &application,
                    &posting,
                    &resume,
                    "cloud",
                    run_id,
                )?;
                let admission = jobs::stage_cloud_workflow_resume(
                    &state.pool,
                    &jobs::StageCloudWorkflowResume {
                        account_id: account.id.clone(),
                        application_id: application.id.clone(),
                        run_id: run_id.to_string(),
                        workflow_id,
                        intervention_id: updated.id.clone(),
                        idempotency_key: updated.id.clone(),
                        workflow_input,
                        browser_session_id,
                        resolution: json!({
                            "action": action,
                            "field": field,
                            "answer": "",
                        }),
                        now_ms: jobs::now_ms(),
                    },
                )
                .map_err(domain_error)?;
                let saved = jobs::list_interventions(&state.pool, &account.id)
                    .map_err(internal)?
                    .into_iter()
                    .find(|item| item.id == intervention_id)
                    .ok_or((StatusCode::NOT_FOUND, "Intervention not found.".to_string()))?;
                let application = jobs::get_application(&state.pool, &account.id, application_id)
                    .map_err(internal)?
                    .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
                return Ok(Json(InterventionResolutionResult {
                    intervention: saved,
                    answer_memory: remembered_answer,
                    application: Some(application),
                    local_resume: None,
                    workflow_command: Some(WorkflowCommandAdmissionResponse::from_admission(
                        &admission,
                    )),
                }));
            }
        }
    }
    let mut saved = if action == "approve_submission" {
        Some(jobs::save_intervention(&state.pool, &account.id, &updated).map_err(internal)?)
    } else {
        None
    };
    let resumed_application = if resumes_runner {
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
                return Err((
                    StatusCode::CONFLICT,
                    "The browser run is not waiting for this approval.".to_string(),
                ));
            }
        }
    }
    Ok(Json(InterventionResolutionResult {
        intervention: saved,
        answer_memory: remembered_answer,
        application: resumed_application,
        local_resume,
        workflow_command: None,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalRunClaimRequest {
    ticket: String,
    claim_nonce: String,
    build_proof: jobs::BrowserBuildProof,
}

#[derive(Debug, Deserialize)]
struct LocalRunAccessRequest {
    #[serde(default)]
    capability: String,
    #[serde(default)]
    ticket: String,
}

#[derive(Debug, Deserialize)]
struct LocalRunSubmitAccessRequest {
    #[serde(default)]
    capability: String,
    #[serde(default)]
    ticket: String,
    final_submit_proof: jobs::FinalSubmitProof,
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
) -> Result<Response, ApiError> {
    if !jobs_local_browser_distribution_enabled(&state.pool) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser local runs are currently paused.".to_string(),
        ));
    }
    if req.claim_nonce.len() != 64
        || req.claim_nonce != req.claim_nonce.to_ascii_lowercase()
        || !req.claim_nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
        || req.claim_nonce != local_browser_claim_nonce(&run_id, &req.ticket, &req.build_proof)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid Browser claim nonce.".to_string(),
        ));
    }
    let descriptor =
        jobs::parse_browser_build_proof_for_claim(&req.build_proof).map_err(|error| {
            tracing::warn!(error = %error, "rejected invalid Bluey Browser build proof");
            (
                StatusCode::UPGRADE_REQUIRED,
                "Install an authorized Bluey Browser release and try again.".to_string(),
            )
        })?;
    let hash = local_run_ticket_hash(&req.ticket)?;
    let server_release_id = browser_server_release_id()?;
    let disposition = jobs::claim_local_run_with_browser_release_for_distribution(
        &state.pool,
        &run_id,
        &hash,
        &req.claim_nonce,
        &descriptor,
        &server_release_id,
        |ticket, binding| {
            let browser_profile_id = ticket
                .payload
                .get("browserProfileId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("local Browser claim has no browser profile"))?;
            let release = local_capability_release(binding);
            let result_capability = super::jobs_local_capability::issue(
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                browser_profile_id,
                "result",
                ticket.expires_at_ms,
                &release,
            )?;
            let resume_capability = super::jobs_local_capability::issue(
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                browser_profile_id,
                "resume",
                ticket.expires_at_ms,
                &release,
            )?;
            let submit_capability = super::jobs_local_capability::issue(
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                browser_profile_id,
                "submit",
                ticket.expires_at_ms,
                &release,
            )?;
            let mut payload = ticket
                .payload
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("local Browser claim payload is invalid"))?;
            payload.insert(
                "_blueyCapabilities".to_string(),
                json!({
                    "result": result_capability,
                    "resume": resume_capability,
                    "submit": submit_capability,
                    "expiresAtMs": ticket.expires_at_ms,
                }),
            );
            payload.insert("_blueyRelease".to_string(), serde_json::to_value(release)?);
            Ok(Value::Object(payload))
        },
    )
    .map_err(internal)?;
    match disposition {
        jobs::BrowserLocalRunClaimDisposition::Success(success) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(success.response_json))
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Bluey Browser could not start securely. Try again.".to_string(),
                )
            }),
        jobs::BrowserLocalRunClaimDisposition::ReleaseUnavailable => Err((
            StatusCode::UPGRADE_REQUIRED,
            "Install the active Bluey Browser release for this account and try again.".to_string(),
        )),
        jobs::BrowserLocalRunClaimDisposition::DistributionUnavailable => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser local runs are currently paused.".to_string(),
        )),
        jobs::BrowserLocalRunClaimDisposition::ConflictingReplay => Err((
            StatusCode::CONFLICT,
            "This Browser launch was already claimed by a different request.".to_string(),
        )),
        jobs::BrowserLocalRunClaimDisposition::Rejected => Err((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        )),
    }
}

fn local_browser_claim_nonce(
    run_id: &str,
    ticket: &str,
    proof: &jobs::BrowserBuildProof,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-browser-claim-v1\0");
    digest.update(run_id.as_bytes());
    digest.update(b"\0");
    digest.update(ticket.as_bytes());
    digest.update(b"\0");
    digest.update(proof.descriptor.as_bytes());
    digest.update(b"\0");
    digest.update(proof.signature.as_bytes());
    hex::encode(digest.finalize())
}

fn browser_server_release_id() -> Result<String, ApiError> {
    let value = std::env::var("BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser release verification is unavailable.".to_string(),
        )
    })?;
    if value.len() < 3
        || value.len() > 128
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser release verification is unavailable.".to_string(),
        ));
    }
    Ok(value)
}

fn local_capability_release(
    binding: &jobs::BrowserReleaseClaimBinding,
) -> super::jobs_local_capability::LocalRunReleaseClaims {
    super::jobs_local_capability::LocalRunReleaseClaims {
        descriptor_sha256: binding.build_descriptor_sha256.clone(),
        manifest_sha256: binding.manifest_sha256.clone(),
        activation_sha256: binding.activation_sha256.clone(),
        artifact_id: binding.artifact_id.clone(),
        artifact_sha256: binding.artifact_sha256.clone(),
        automation_bundle_sha256: binding.automation_bundle_sha256.clone(),
        chromium_executable_sha256: binding.chromium_executable_sha256.clone(),
        release_id: binding.release_id.clone(),
        build_id: binding.build_id.clone(),
        app_version: binding.app_version.clone(),
        protocol_version: binding.protocol_version,
        platform: binding.platform.clone(),
        architecture: binding.architecture.clone(),
        channel: binding.channel.clone(),
        trust_generation: binding.trust_generation,
        activation_generation: binding.activation_generation,
        channel_sequence: binding.channel_sequence,
    }
}

async fn authorize_local_run_submit(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunSubmitAccessRequest>,
) -> Result<Json<Value>, ApiError> {
    let authorization = authorize_local_run_operation(
        &state,
        &run_id,
        &req.capability,
        &req.ticket,
        "submit",
        None,
    )?;
    let exact_click_started_replay = authorization.capability_version
        == Some(super::jobs_local_capability::TOKEN_VERSION)
        && authorization.ticket.status == "click_started";
    let ticket = authorization.ticket;
    if !local_submit_distribution_allowed(
        exact_click_started_replay,
        jobs_local_browser_distribution_enabled(&state.pool),
    ) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Browser local runs are currently paused.".to_string(),
        ));
    }
    let (application, _) = local_result_binding(&state, &ticket, &run_id)?;
    let posting = jobs::get_posting(&state.pool, &ticket.account_id, &application.job_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Job not found.".to_string()))?;
    if matches!(ats_kind(&posting.canonical_url), "greenhouse" | "lever")
        && !jobs::application_has_frozen_ats_certification(&application)
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
    let capacity = local_submission_evidence_capacity(
        &state,
        &ticket.account_id,
        &ticket.application_id,
        &run_id,
        exact_click_started_replay,
    )?;
    let now_ms = capacity.now_ms;
    let server_release_id = browser_server_release_id()?;
    let authorization = jobs::local_run_submit_authorization_for_distribution(
        &state.pool,
        &run_id,
        &ticket.ticket_hash,
        &server_release_id,
        &req.final_submit_proof,
        &capacity,
    )
    .map_err(|error| {
        if error.downcast_ref::<UploadControlError>().is_some() {
            evidence_upload_control_error(error)
        } else {
            internal(error)
        }
    })?;
    let Some(authorization) = authorization else {
        return Err((
            StatusCode::CONFLICT,
            "This application is no longer authorized to submit. Return to Bluey Jobs to review it."
                .to_string(),
        ));
    };
    if let Some(authority) = authorization.ats_certified_receipt_authority {
        let authorized_at_ms = authority.binding_consumed_at_ms;
        return Ok(Json(json!({
            "authorized": true,
            "authorizedAtMs": authorized_at_ms,
            "atsCertifiedReceiptAuthority": authority,
        })));
    }
    Ok(Json(json!({
        "authorized": true,
        "authorizedAtMs": now_ms,
    })))
}

fn local_submission_evidence_capacity(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    exact_click_started_replay: bool,
) -> Result<NewSubmissionEvidenceCapacity, ApiError> {
    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Application evidence storage is not configured.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    let now_ms = jobs::now_ms();
    if exact_click_started_replay {
        let durable = object_uploads::get_submission_evidence_capacity(
            &state.pool,
            account_id,
            application_id,
            run_id,
        )
        .map_err(internal)?;
        return local_durable_recovery_evidence_capacity(
            durable,
            account_id,
            application_id,
            run_id,
            now_ms,
            storage.upload_limits(),
        )
        .ok_or((
            StatusCode::CONFLICT,
            "This application is no longer authorized to submit. Return to Bluey Jobs to review it."
                .to_string(),
        ));
    }
    let bundle_capacity_bytes = storage.max_object_bytes().min(MAX_RECEIPT_BUNDLE_BYTES) as i64;
    Ok(NewSubmissionEvidenceCapacity {
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        runner: "local".to_string(),
        reserved_bytes: MAX_RECEIPT_EVIDENCE_BYTES as i64 + bundle_capacity_bytes,
        reserved_objects: MAX_SUBMISSION_EVIDENCE_OBJECTS,
        expires_at_ms: now_ms.saturating_add(jobs::SUBMISSION_RECONCILIATION_GRACE_MS),
        now_ms,
        limits: storage.upload_limits(),
    })
}

fn local_durable_recovery_evidence_capacity(
    durable: Option<object_uploads::SubmissionEvidenceCapacity>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    now_ms: i64,
    limits: crate::object_storage::UploadLimits,
) -> Option<NewSubmissionEvidenceCapacity> {
    let durable = durable?;
    if durable.account_id != account_id
        || durable.application_id != application_id
        || durable.run_id != run_id
        || durable.runner != "local"
        || durable.state != "active"
        || durable.expires_at_ms <= now_ms
        || durable.reserved_bytes <= 0
        || durable.reserved_objects <= 0
    {
        return None;
    }
    Some(NewSubmissionEvidenceCapacity {
        account_id: durable.account_id,
        application_id: durable.application_id,
        run_id: durable.run_id,
        runner: durable.runner,
        reserved_bytes: durable.reserved_bytes,
        reserved_objects: durable.reserved_objects,
        expires_at_ms: durable.expires_at_ms,
        now_ms,
        limits,
    })
}

async fn consume_local_run_resume(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<LocalRunAccessRequest>,
) -> Result<Json<jobs::LocalRunResumeAction>, ApiError> {
    let ticket = authorize_local_run_operation(
        &state,
        &run_id,
        &req.capability,
        &req.ticket,
        "resume",
        None,
    )?
    .ticket;
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
    Json(mut req): Json<LocalRunResultRequest>,
) -> Result<Json<JobApplication>, ApiError> {
    let reported_status = req
        .receipt
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if reported_status == "submitted" {
        if let Some(application) =
            replay_submitted_local_run_result(&state, &run_id, &mut req).await?
        {
            return Ok(Json(application));
        }
    }
    let authorization = authorize_local_run_operation(
        &state,
        &run_id,
        &req.capability,
        &req.ticket,
        "result",
        Some(reported_status.as_str()),
    )?;
    let capability_version = authorization.capability_version;
    let ticket = authorization.ticket;
    let mut status = reported_status;
    if ticket.status == "click_started" && matches!(status.as_str(), "failed" | "needs_input") {
        status = "side_effect_unknown".to_string();
        if let Some(receipt) = req.receipt.as_object_mut() {
            receipt.insert("status".to_string(), Value::String(status.clone()));
            receipt.insert(
                "issues".to_string(),
                json!([{
                    "field": "submission",
                    "message": "The local browser crossed Submit but could not prove the employer outcome."
                }]),
            );
        }
    }
    if ticket.expires_at_ms <= jobs::now_ms()
        && !((matches!(status.as_str(), "submitted" | "side_effect_unknown"))
            && matches!(
                ticket.status.as_str(),
                "click_started" | "side_effect_unknown" | "complete"
            )
            && ticket
                .expires_at_ms
                .saturating_add(super::jobs_local_capability::RECONCILIATION_GRACE_MS)
                > jobs::now_ms())
    {
        return Err((
            StatusCode::GONE,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ));
    }
    let (bound_application, bound_session) = local_result_binding(&state, &ticket, &run_id)?;
    match ticket.status.as_str() {
        "failed" if status == "failed" && bound_application.state == "failed" => {
            object_uploads::release_submission_evidence_capacity(
                &state.pool,
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                jobs::now_ms(),
            )
            .map_err(internal)?;
            return Ok(Json(bound_application));
        }
        "side_effect_unknown"
            if status == "side_effect_unknown"
                && bound_application.state == "side_effect_unknown" => {}
        "complete" if status == "side_effect_unknown" && bound_application.state == "submitted" => {
            return Ok(Json(bound_application));
        }
        "complete" if status != "submitted" => {
            return Err((
                StatusCode::CONFLICT,
                "This local run already has a different terminal result.".to_string(),
            ));
        }
        "side_effect_unknown" if status == "submitted" => {}
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
                && !jobs::application_has_frozen_ats_certification(&application)
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
            let result_capability = req.capability;
            let application = persist_submission_receipt(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                bundle,
                req.evidence_objects,
                "local",
                None,
                Some(&ticket.ticket_hash),
                Some(&result_capability),
                capability_version,
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
            object_uploads::release_submission_evidence_capacity(
                &state.pool,
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                jobs::now_ms(),
            )
            .map_err(internal)?;
            application
        }
        "side_effect_unknown" => {
            let capacity = local_submission_evidence_capacity(
                &state,
                &ticket.account_id,
                &ticket.application_id,
                &run_id,
                local_result_uses_durable_submission_evidence_capacity(&ticket.status),
            )?;
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
                &capacity,
                reconciliation_receipt,
                &session,
            )
            .map_err(|error| {
                if error.downcast_ref::<UploadControlError>().is_some() {
                    evidence_upload_control_error(error)
                } else {
                    submission_domain_error(error)
                }
            })?
        }
        _ => return bad_request("Bluey Browser returned an invalid application result."),
    };
    Ok(Json(application))
}

async fn replay_submitted_local_run_result(
    state: &AppState,
    run_id: &str,
    req: &mut LocalRunResultRequest,
) -> Result<Option<JobApplication>, ApiError> {
    let Some(receipt) = req.receipt_bundle.as_ref() else {
        return Ok(None);
    };
    let Some(account_id) = receipt_replay_identifier(receipt.get("accountId")) else {
        return Ok(None);
    };
    let Some(application_id) = receipt_replay_identifier(receipt.get("applicationId")) else {
        return Ok(None);
    };
    if receipt.get("runner").and_then(Value::as_str) != Some("local")
        || receipt.get("runId").and_then(Value::as_str) != Some(run_id)
    {
        return Ok(None);
    }
    let account_id = account_id.to_string();
    let application_id = application_id.to_string();
    let Some(application) =
        jobs::get_application(&state.pool, &account_id, &application_id).map_err(internal)?
    else {
        return Ok(None);
    };
    if !jobs::submitted_local_receipt_replay_authorized(&application, run_id, &req.capability) {
        return Ok(None);
    }

    let result_capability = req.capability.clone();
    let receipt = req
        .receipt_bundle
        .take()
        .expect("submitted replay candidate has a receipt bundle");
    let evidence_objects = std::mem::take(&mut req.evidence_objects);
    persist_submission_receipt(
        state,
        &account_id,
        &application_id,
        receipt,
        evidence_objects,
        "local",
        None,
        None,
        Some(&result_capability),
        None,
    )
    .await
    .map(Some)
}

fn receipt_replay_identifier(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|value| {
        !value.is_empty()
            && value.len() <= 240
            && value.trim() == *value
            && value.bytes().all(|byte| !byte.is_ascii_control())
    })
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

struct AuthorizedLocalRunOperation {
    ticket: jobs::LocalRunTicket,
    capability_version: Option<u8>,
}

fn authorize_local_run_operation(
    state: &AppState,
    run_id: &str,
    capability: &str,
    _legacy_ticket: &str,
    operation: &str,
    reconciliation_status: Option<&str>,
) -> Result<AuthorizedLocalRunOperation, ApiError> {
    let now = jobs::now_ms();
    let allow_late_reconciliation = operation == "result"
        && matches!(
            reconciliation_status,
            Some("submitted" | "side_effect_unknown")
        );
    #[cfg(debug_assertions)]
    if capability.is_empty() && !_legacy_ticket.is_empty() {
        let hash = local_run_ticket_hash(_legacy_ticket)?;
        let ticket = jobs::get_local_run_ticket_by_hash(&state.pool, run_id, &hash)
            .map_err(internal)?
            .ok_or((
                StatusCode::NOT_FOUND,
                "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
            ))?;
        if !legacy_local_run_capability_status_allowed(&ticket.status, operation) {
            return Err((
                StatusCode::NOT_FOUND,
                "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
            ));
        }
        if !local_run_operation_expiry_allowed(
            operation,
            reconciliation_status,
            None,
            &ticket.status,
            ticket.expires_at_ms,
            now,
        ) {
            return Err((
                StatusCode::NOT_FOUND,
                "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
            ));
        }
        return Ok(AuthorizedLocalRunOperation {
            ticket,
            capability_version: None,
        });
    }

    let claims = if allow_late_reconciliation || operation == "submit" {
        super::jobs_local_capability::verify_for_reconciliation(capability, run_id, operation, now)
    } else {
        super::jobs_local_capability::verify(capability, run_id, operation, now)
    }
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
    if claims.version == 1 && !legacy_local_run_capability_status_allowed(&ticket.status, operation)
    {
        return Err((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ));
    }
    if !local_run_operation_expiry_allowed(
        operation,
        reconciliation_status,
        Some(claims.version),
        &ticket.status,
        ticket.expires_at_ms,
        now,
    ) {
        return Err((
            StatusCode::NOT_FOUND,
            "This Bluey Browser launch has expired. Start it again from Jobs.".to_string(),
        ));
    }
    Ok(AuthorizedLocalRunOperation {
        ticket,
        capability_version: Some(claims.version),
    })
}

fn local_run_operation_expiry_allowed(
    operation: &str,
    reconciliation_status: Option<&str>,
    capability_version: Option<u8>,
    ticket_status: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> bool {
    if expires_at_ms > now_ms {
        return true;
    }
    if expires_at_ms.saturating_add(super::jobs_local_capability::RECONCILIATION_GRACE_MS) <= now_ms
    {
        return false;
    }
    let result_reconciliation = operation == "result"
        && matches!(
            reconciliation_status,
            Some("submitted" | "side_effect_unknown")
        )
        && (matches!(ticket_status, "side_effect_unknown" | "complete")
            || (capability_version.is_some() && ticket_status == "click_started"));
    let submit_replay = operation == "submit"
        && capability_version == Some(super::jobs_local_capability::TOKEN_VERSION)
        && ticket_status == "click_started";
    result_reconciliation || submit_replay
}

fn local_submit_distribution_allowed(
    exact_click_started_replay: bool,
    distribution_enabled: bool,
) -> bool {
    distribution_enabled || exact_click_started_replay
}

fn local_result_uses_durable_submission_evidence_capacity(ticket_status: &str) -> bool {
    matches!(ticket_status, "click_started" | "side_effect_unknown")
}

fn legacy_local_run_capability_status_allowed(ticket_status: &str, operation: &str) -> bool {
    match operation {
        "result" => matches!(
            ticket_status,
            "claimed"
                | "needs_input"
                | "click_started"
                | "side_effect_unknown"
                | "complete"
                | "failed"
        ),
        "resume" => matches!(ticket_status, "needs_input" | "claimed"),
        _ => false,
    }
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
#[serde(deny_unknown_fields)]
struct WorkerExecutionLeaseClaimRequest {
    account_id: String,
    application_id: String,
    run_id: String,
    browser_profile_id: String,
    owner_id: String,
    volume_id: String,
    enrollment_epoch: i64,
    process_instance_id: String,
    runtime_grant_id: String,
    runtime_sha256: String,
    #[serde(default)]
    workflow_request_id: Option<String>,
    #[serde(default)]
    managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    #[serde(default)]
    managed_cloud_release_sha256: Option<String>,
    #[serde(default)]
    managed_cloud_runtime_instance_id: Option<String>,
    #[serde(default)]
    managed_cloud_runtime_instance_epoch: Option<i64>,
    volume_proof: jobs::RunnerVolumeAuthorityProof,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerExecutionLeaseAccessRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerManagedExecutionEffectRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
    #[serde(default)]
    workflow_request_id: Option<String>,
    #[serde(default)]
    managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    #[serde(default)]
    managed_cloud_release_sha256: Option<String>,
    #[serde(default)]
    managed_cloud_runtime_instance_id: Option<String>,
    #[serde(default)]
    managed_cloud_runtime_instance_epoch: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerIrreversibleExecutionRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
    action: String,
    final_submit_proof: jobs::FinalSubmitProof,
    #[serde(default)]
    workflow_request_id: Option<String>,
    #[serde(default)]
    managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    #[serde(default)]
    managed_cloud_release_sha256: Option<String>,
    #[serde(default)]
    managed_cloud_runtime_instance_id: Option<String>,
    #[serde(default)]
    managed_cloud_runtime_instance_epoch: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerFinishExecutionRequest {
    account_id: String,
    application_id: String,
    lease_token: String,
    fence: i64,
    outcome: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerCheckpointReconciliationRequest {
    account_id: String,
    application_id: String,
    owner_id: String,
    #[serde(default)]
    lease_token: Option<String>,
    fence: i64,
    checkpoint_version: i64,
    checkpoint_phase: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerBrowserProfileSnapshotAccessRequest {
    account_id: String,
    application_id: String,
    browser_profile_id: String,
    lease_token: String,
    fence: i64,
}

#[derive(Debug, Deserialize)]
struct WorkerBrowserProfileSnapshotStoreRequest {
    account_id: String,
    application_id: String,
    browser_profile_id: String,
    lease_token: String,
    fence: i64,
    expected_generation: i64,
    envelope_version: i64,
    sha256: String,
    size_bytes: i64,
    encrypted_snapshot_base64: String,
}

#[derive(Debug, Serialize)]
struct WorkerBrowserProfileSnapshotStoreResponse {
    browser_profile_id: String,
    generation: i64,
    sha256: String,
    size_bytes: i64,
    envelope_version: i64,
}

#[derive(Debug, Serialize)]
struct WorkerBrowserProfileSnapshotRestoreResponse {
    browser_profile_id: String,
    generation: i64,
    sha256: String,
    size_bytes: i64,
    envelope_version: i64,
    encrypted_snapshot_base64: String,
}

fn authenticated_execution_lease_owner<'a>(
    worker: &'a JobsWorkerIdentity,
    requested_owner_id: &'a str,
) -> Result<&'a str, ApiError> {
    #[cfg(debug_assertions)]
    if worker.scope == "debug" {
        return Ok(requested_owner_id);
    }

    if requested_owner_id != worker.worker_id {
        return bad_request("Execution lease owner does not match the authenticated worker.");
    }
    Ok(worker.worker_id.as_str())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum WorkflowCommandMaterializeRequest {
    Start {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        #[serde(default)]
        managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    },
    Resume {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        intervention_id: String,
        #[serde(default)]
        managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum WorkflowCommandMaterializeResponse {
    Start {
        schema_version: i64,
        request_id: String,
        workflow_id: String,
        payload_digest: String,
        workflow_input: Value,
        browser_session_id: String,
        result_request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    },
    Resume {
        schema_version: i64,
        request_id: String,
        workflow_id: String,
        payload_digest: String,
        intervention_id: String,
        workflow_input: Value,
        browser_session_id: String,
        result_request_id: String,
        resolution: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        managed_cloud_release: Option<jobs::ManagedCloudReleaseMemoAuthority>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum WorkflowCommandInterventionPrepareRequest {
    Start {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        receipt: Value,
    },
    Resume {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        intervention_id: String,
        receipt: Value,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum WorkflowCommandAuthorityRequest {
    Start {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
    },
    Resume {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        intervention_id: String,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkflowCommandTerminalState {
    Failed,
    SideEffectUnknown,
}

impl WorkflowCommandTerminalState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkflowCommandTerminalReason {
    RunnerFailed,
    RunnerAmbiguous,
    InterventionTimeout,
    InterventionLimit,
}

impl WorkflowCommandTerminalReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::RunnerFailed => "runner_failed",
            Self::RunnerAmbiguous => "runner_ambiguous",
            Self::InterventionTimeout => "intervention_timeout",
            Self::InterventionLimit => "intervention_limit",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum WorkflowCommandFinalizeRequest {
    Start {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        terminal_state: WorkflowCommandTerminalState,
        reason_code: WorkflowCommandTerminalReason,
        #[serde(default, deserialize_with = "deserialize_present_string")]
        open_intervention_id: Option<String>,
    },
    Resume {
        schema_version: i64,
        workflow_id: String,
        payload_digest: String,
        intervention_id: String,
        terminal_state: WorkflowCommandTerminalState,
        reason_code: WorkflowCommandTerminalReason,
        #[serde(default, deserialize_with = "deserialize_present_string")]
        open_intervention_id: Option<String>,
    },
}

fn deserialize_present_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}

#[derive(Debug, Serialize)]
struct WorkflowCommandInterventionMutationResponse {
    schema_version: i64,
    request_id: String,
    workflow_id: String,
    payload_digest: String,
    operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_intervention_id: Option<String>,
    intervention_id: String,
    replayed: bool,
}

#[derive(Debug, Serialize)]
struct WorkflowCommandFinalizationResponse {
    schema_version: i64,
    request_id: String,
    workflow_id: String,
    payload_digest: String,
    operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_intervention_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_intervention_id: Option<String>,
    terminal_state: &'static str,
    reason_code: &'static str,
    replayed: bool,
}

async fn worker_prepare_workflow_intervention(
    State(state): State<AppState>,
    Extension(_worker): Extension<JobsWorkerIdentity>,
    Path(request_id): Path<String>,
    request: Result<Json<WorkflowCommandInterventionPrepareRequest>, JsonRejection>,
) -> Response {
    if !workflow_command_opaque_identifier(&request_id, 128) {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) => return workflow_command_json_rejection_response(rejection),
    };
    let (schema_version, workflow_id, payload_digest, command_kind, intervention_id, receipt) =
        match request {
            WorkflowCommandInterventionPrepareRequest::Start {
                schema_version,
                workflow_id,
                payload_digest,
                receipt,
            } => (
                schema_version,
                workflow_id,
                payload_digest,
                jobs::JobsWorkflowCommandKind::Start,
                None,
                receipt,
            ),
            WorkflowCommandInterventionPrepareRequest::Resume {
                schema_version,
                workflow_id,
                payload_digest,
                intervention_id,
                receipt,
            } => (
                schema_version,
                workflow_id,
                payload_digest,
                jobs::JobsWorkflowCommandKind::Resume,
                Some(intervention_id),
                receipt,
            ),
        };
    if !valid_workflow_command_authority(
        schema_version,
        &workflow_id,
        &payload_digest,
        intervention_id.as_deref(),
    ) {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let prepared = match jobs::prepare_jobs_workflow_intervention(
        &state.pool,
        &jobs::PrepareJobsWorkflowIntervention {
            request_id: request_id.clone(),
            payload_hmac_sha256: payload_digest.clone(),
            workflow_id: workflow_id.clone(),
            command_kind,
            intervention_id: intervention_id.clone(),
            receipt,
            now_ms: jobs::now_ms(),
        },
    ) {
        Ok(prepared) => prepared,
        Err(error) => return workflow_command_database_error_response(error),
    };
    if prepared.request_id != request_id || prepared.payload_hmac_sha256 != payload_digest {
        return workflow_command_error_response(
            StatusCode::CONFLICT,
            "identity_conflict",
            "identity_conflict",
        );
    }
    workflow_command_serialized_response(
        StatusCode::OK,
        WorkflowCommandInterventionMutationResponse {
            schema_version: 2,
            request_id,
            workflow_id,
            payload_digest,
            operation: workflow_command_operation(command_kind),
            command_intervention_id: intervention_id,
            intervention_id: prepared.intervention_id,
            replayed: prepared.replayed,
        },
    )
}

async fn worker_publish_workflow_intervention(
    State(state): State<AppState>,
    Extension(_worker): Extension<JobsWorkerIdentity>,
    Path((request_id, prepared_intervention_id)): Path<(String, String)>,
    request: Result<Json<WorkflowCommandAuthorityRequest>, JsonRejection>,
) -> Response {
    if !workflow_command_opaque_identifier(&request_id, 128)
        || !workflow_command_opaque_identifier(&prepared_intervention_id, 128)
    {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) => return workflow_command_json_rejection_response(rejection),
    };
    let (schema_version, workflow_id, payload_digest, command_kind, intervention_id) =
        workflow_command_authority_parts(request);
    if !valid_workflow_command_authority(
        schema_version,
        &workflow_id,
        &payload_digest,
        intervention_id.as_deref(),
    ) {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let published = match jobs::publish_jobs_workflow_intervention(
        &state.pool,
        &jobs::PublishJobsWorkflowIntervention {
            request_id: request_id.clone(),
            payload_hmac_sha256: payload_digest.clone(),
            workflow_id: workflow_id.clone(),
            command_kind,
            command_intervention_id: intervention_id.clone(),
            intervention_id: prepared_intervention_id.clone(),
            now_ms: jobs::now_ms(),
        },
    ) {
        Ok(published) => published,
        Err(error) => return workflow_command_database_error_response(error),
    };
    if published.intervention.id != prepared_intervention_id {
        return workflow_command_error_response(
            StatusCode::CONFLICT,
            "identity_conflict",
            "identity_conflict",
        );
    }
    workflow_command_serialized_response(
        StatusCode::OK,
        WorkflowCommandInterventionMutationResponse {
            schema_version: 2,
            request_id,
            workflow_id,
            payload_digest,
            operation: workflow_command_operation(command_kind),
            command_intervention_id: intervention_id,
            intervention_id: prepared_intervention_id,
            replayed: published.replayed,
        },
    )
}

async fn worker_finalize_workflow_execution(
    State(state): State<AppState>,
    Extension(_worker): Extension<JobsWorkerIdentity>,
    Path(request_id): Path<String>,
    request: Result<Json<WorkflowCommandFinalizeRequest>, JsonRejection>,
) -> Response {
    if !workflow_command_opaque_identifier(&request_id, 128) {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) => return workflow_command_json_rejection_response(rejection),
    };
    let parts = workflow_command_finalization_parts(request);
    if !valid_workflow_command_authority(
        parts.schema_version,
        &parts.workflow_id,
        &parts.payload_digest,
        parts.intervention_id.as_deref(),
    ) || parts
        .open_intervention_id
        .as_deref()
        .is_some_and(|value| !workflow_command_opaque_identifier(value, 128))
    {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let Some(outcome) = workflow_command_terminal_outcome(
        parts.terminal_state,
        parts.reason_code,
        parts.open_intervention_id.as_deref(),
    ) else {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    };
    let finalized = match jobs::finalize_jobs_workflow_execution(
        &state.pool,
        &jobs::FinalizeJobsWorkflowExecution {
            request_id: request_id.clone(),
            payload_hmac_sha256: parts.payload_digest.clone(),
            workflow_id: parts.workflow_id.clone(),
            command_kind: parts.command_kind,
            intervention_id: parts.intervention_id.clone(),
            outcome,
            open_intervention_id: parts.open_intervention_id.clone(),
            now_ms: jobs::now_ms(),
        },
    ) {
        Ok(finalized) => finalized,
        Err(error) => return workflow_command_database_error_response(error),
    };
    if finalized.request_id != request_id || finalized.outcome != outcome {
        return workflow_command_error_response(
            StatusCode::CONFLICT,
            "identity_conflict",
            "identity_conflict",
        );
    }
    workflow_command_serialized_response(
        StatusCode::OK,
        WorkflowCommandFinalizationResponse {
            schema_version: 2,
            request_id,
            workflow_id: parts.workflow_id,
            payload_digest: parts.payload_digest,
            operation: workflow_command_operation(parts.command_kind),
            command_intervention_id: parts.intervention_id,
            open_intervention_id: parts.open_intervention_id,
            terminal_state: parts.terminal_state.as_str(),
            reason_code: parts.reason_code.as_str(),
            replayed: finalized.replayed,
        },
    )
}

fn workflow_command_authority_parts(
    request: WorkflowCommandAuthorityRequest,
) -> (
    i64,
    String,
    String,
    jobs::JobsWorkflowCommandKind,
    Option<String>,
) {
    match request {
        WorkflowCommandAuthorityRequest::Start {
            schema_version,
            workflow_id,
            payload_digest,
        } => (
            schema_version,
            workflow_id,
            payload_digest,
            jobs::JobsWorkflowCommandKind::Start,
            None,
        ),
        WorkflowCommandAuthorityRequest::Resume {
            schema_version,
            workflow_id,
            payload_digest,
            intervention_id,
        } => (
            schema_version,
            workflow_id,
            payload_digest,
            jobs::JobsWorkflowCommandKind::Resume,
            Some(intervention_id),
        ),
    }
}

struct WorkflowCommandFinalizationParts {
    schema_version: i64,
    workflow_id: String,
    payload_digest: String,
    command_kind: jobs::JobsWorkflowCommandKind,
    intervention_id: Option<String>,
    terminal_state: WorkflowCommandTerminalState,
    reason_code: WorkflowCommandTerminalReason,
    open_intervention_id: Option<String>,
}

fn workflow_command_finalization_parts(
    request: WorkflowCommandFinalizeRequest,
) -> WorkflowCommandFinalizationParts {
    match request {
        WorkflowCommandFinalizeRequest::Start {
            schema_version,
            workflow_id,
            payload_digest,
            terminal_state,
            reason_code,
            open_intervention_id,
        } => WorkflowCommandFinalizationParts {
            schema_version,
            workflow_id,
            payload_digest,
            command_kind: jobs::JobsWorkflowCommandKind::Start,
            intervention_id: None,
            terminal_state,
            reason_code,
            open_intervention_id,
        },
        WorkflowCommandFinalizeRequest::Resume {
            schema_version,
            workflow_id,
            payload_digest,
            intervention_id,
            terminal_state,
            reason_code,
            open_intervention_id,
        } => WorkflowCommandFinalizationParts {
            schema_version,
            workflow_id,
            payload_digest,
            command_kind: jobs::JobsWorkflowCommandKind::Resume,
            intervention_id: Some(intervention_id),
            terminal_state,
            reason_code,
            open_intervention_id,
        },
    }
}

fn workflow_command_terminal_outcome(
    state: WorkflowCommandTerminalState,
    reason: WorkflowCommandTerminalReason,
    open_intervention_id: Option<&str>,
) -> Option<jobs::JobsWorkflowTerminalOutcome> {
    use jobs::{JobsWorkflowTerminalOutcome as Outcome, JobsWorkflowTerminalReason as Reason};

    match (state, reason, open_intervention_id) {
        (
            WorkflowCommandTerminalState::Failed,
            WorkflowCommandTerminalReason::RunnerFailed,
            None,
        ) => Some(Outcome::Failed(Reason::RunnerFailed)),
        (
            WorkflowCommandTerminalState::SideEffectUnknown,
            WorkflowCommandTerminalReason::RunnerAmbiguous,
            None,
        ) => Some(Outcome::SideEffectUnknown(Reason::RunnerAmbiguous)),
        (
            WorkflowCommandTerminalState::Failed,
            WorkflowCommandTerminalReason::InterventionTimeout,
            Some(_),
        ) => Some(Outcome::Failed(Reason::InterventionTimeout)),
        (
            WorkflowCommandTerminalState::Failed,
            WorkflowCommandTerminalReason::InterventionLimit,
            None,
        ) => Some(Outcome::Failed(Reason::InterventionLimit)),
        _ => None,
    }
}

fn workflow_command_operation(kind: jobs::JobsWorkflowCommandKind) -> &'static str {
    match kind {
        jobs::JobsWorkflowCommandKind::Start => "start",
        jobs::JobsWorkflowCommandKind::Resume => "resume",
    }
}

fn valid_workflow_command_authority(
    schema_version: i64,
    workflow_id: &str,
    payload_digest: &str,
    intervention_id: Option<&str>,
) -> bool {
    schema_version == 2
        && workflow_command_opaque_identifier(workflow_id, 192)
        && workflow_command_digest(payload_digest)
        && intervention_id.is_none_or(|value| workflow_command_opaque_identifier(value, 128))
}

async fn worker_materialize_workflow_command(
    State(state): State<AppState>,
    Extension(_worker): Extension<JobsWorkerIdentity>,
    Path(request_id): Path<String>,
    request: Result<Json<WorkflowCommandMaterializeRequest>, JsonRejection>,
) -> Response {
    if !workflow_command_opaque_identifier(&request_id, 128) {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let Json(req) = match request {
        Ok(request) => request,
        Err(rejection) => return workflow_command_json_rejection_response(rejection),
    };
    if !valid_workflow_command_materialize_request(&req) {
        return workflow_command_error_response(
            StatusCode::BAD_REQUEST,
            "rejected",
            "invalid_request",
        );
    }
    let command = match jobs::get_materializable_jobs_workflow_command_by_request_id(
        &state.pool,
        &request_id,
    ) {
        Ok(Some(command)) => command,
        Ok(None) => {
            return workflow_command_error_response(StatusCode::NOT_FOUND, "rejected", "not_found")
        }
        Err(error) => return workflow_command_database_error_response(error),
    };
    let exact_authority = match &req {
        WorkflowCommandMaterializeRequest::Start {
            schema_version,
            workflow_id,
            payload_digest,
            ..
        } => {
            *schema_version == 2
                && command.protocol_version == 2
                && command.command_kind == jobs::JobsWorkflowCommandKind::Start
                && workflow_id == &command.workflow_id
                && payload_digest == &command.payload_hmac_sha256
                && command.intervention_id.is_none()
        }
        WorkflowCommandMaterializeRequest::Resume {
            schema_version,
            workflow_id,
            payload_digest,
            intervention_id,
            ..
        } => {
            *schema_version == 2
                && command.protocol_version == 2
                && command.command_kind == jobs::JobsWorkflowCommandKind::Resume
                && workflow_id == &command.workflow_id
                && payload_digest == &command.payload_hmac_sha256
                && command.intervention_id.as_ref() == Some(intervention_id)
        }
    };
    if !exact_authority {
        return workflow_command_error_response(
            StatusCode::CONFLICT,
            "identity_conflict",
            "identity_conflict",
        );
    }

    let expected_managed_cloud_release =
        match jobs::get_managed_cloud_workflow_release_memo(&state.pool, &command) {
            Ok(authority) => authority,
            Err(jobs::ManagedCloudRegistryError::Storage(error)) => {
                return workflow_command_database_error_response(error)
            }
            Err(jobs::ManagedCloudRegistryError::NotFound) => {
                return workflow_command_error_response(
                    StatusCode::NOT_FOUND,
                    "rejected",
                    "not_found",
                )
            }
            Err(_) => {
                return workflow_command_error_response(
                    StatusCode::CONFLICT,
                    "identity_conflict",
                    "identity_conflict",
                )
            }
        };
    let requested_managed_cloud_release = match &req {
        WorkflowCommandMaterializeRequest::Start {
            managed_cloud_release,
            ..
        }
        | WorkflowCommandMaterializeRequest::Resume {
            managed_cloud_release,
            ..
        } => managed_cloud_release,
    };
    if requested_managed_cloud_release != &expected_managed_cloud_release {
        return workflow_command_error_response(
            StatusCode::CONFLICT,
            "identity_conflict",
            "identity_conflict",
        );
    }

    let response = match (req, command.envelope.payload) {
        (
            WorkflowCommandMaterializeRequest::Start { .. },
            jobs::JobsWorkflowCommandPayload::Start(material),
        ) => WorkflowCommandMaterializeResponse::Start {
            schema_version: 2,
            request_id: command.request_id,
            workflow_id: command.workflow_id,
            payload_digest: command.payload_hmac_sha256,
            workflow_input: material.workflow_input,
            browser_session_id: material.browser_session_id,
            result_request_id: material.result_request_id,
            managed_cloud_release: expected_managed_cloud_release,
        },
        (
            WorkflowCommandMaterializeRequest::Resume {
                intervention_id, ..
            },
            jobs::JobsWorkflowCommandPayload::Resume(material),
        ) => WorkflowCommandMaterializeResponse::Resume {
            schema_version: 2,
            request_id: command.request_id,
            workflow_id: command.workflow_id,
            payload_digest: command.payload_hmac_sha256,
            intervention_id,
            workflow_input: material.workflow_input,
            browser_session_id: material.browser_session_id,
            result_request_id: material.result_request_id,
            resolution: material.resolution,
            managed_cloud_release: expected_managed_cloud_release,
        },
        _ => {
            return workflow_command_error_response(
                StatusCode::CONFLICT,
                "identity_conflict",
                "identity_conflict",
            )
        }
    };
    match serde_json::to_value(response) {
        Ok(value) => workflow_command_json_response(StatusCode::OK, value),
        Err(_) => workflow_command_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "rejected",
            "internal_error",
        ),
    }
}

fn valid_workflow_command_materialize_request(req: &WorkflowCommandMaterializeRequest) -> bool {
    let (schema_version, workflow_id, payload_digest, intervention_id) = match req {
        WorkflowCommandMaterializeRequest::Start {
            schema_version,
            workflow_id,
            payload_digest,
            ..
        } => (*schema_version, workflow_id, payload_digest, None),
        WorkflowCommandMaterializeRequest::Resume {
            schema_version,
            workflow_id,
            payload_digest,
            intervention_id,
            ..
        } => (
            *schema_version,
            workflow_id,
            payload_digest,
            Some(intervention_id),
        ),
    };
    schema_version == 2
        && workflow_command_opaque_identifier(workflow_id, 192)
        && workflow_command_digest(payload_digest)
        && intervention_id.is_none_or(|value| workflow_command_opaque_identifier(value, 128))
}

fn workflow_command_opaque_identifier(value: &str, max_bytes: usize) -> bool {
    (20..=max_bytes).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn workflow_command_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn workflow_command_database_error_response(error: anyhow::Error) -> Response {
    if error
        .downcast_ref::<UploadControlError>()
        .is_some_and(|error| {
            matches!(
                error,
                UploadControlError::AccountDeleting | UploadControlError::SessionNotOwned
            )
        })
    {
        return workflow_command_error_response(StatusCode::NOT_FOUND, "rejected", "not_found");
    }
    if let Some(error) = error.downcast_ref::<jobs::JobsWorkflowCommandError>() {
        return match error {
            jobs::JobsWorkflowCommandError::InvalidRequest => workflow_command_error_response(
                StatusCode::BAD_REQUEST,
                "rejected",
                "invalid_request",
            ),
            jobs::JobsWorkflowCommandError::IdentityConflict => workflow_command_error_response(
                StatusCode::CONFLICT,
                "identity_conflict",
                "identity_conflict",
            ),
            jobs::JobsWorkflowCommandError::NotFound
            | jobs::JobsWorkflowCommandError::InvalidState
            | jobs::JobsWorkflowCommandError::RequestNotStarted
            | jobs::JobsWorkflowCommandError::CleanupFenced => {
                workflow_command_error_response(StatusCode::NOT_FOUND, "rejected", "not_found")
            }
            jobs::JobsWorkflowCommandError::StaleLease
            | jobs::JobsWorkflowCommandError::LeaseExpired => workflow_command_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "rejected",
                "internal_error",
            ),
        };
    }
    tracing::warn!(
        reason_code = "workflow_command_internal_database_error",
        "Jobs workflow command internal request could not complete"
    );
    workflow_command_error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "rejected",
        "internal_error",
    )
}

fn workflow_command_serialized_response<T: Serialize>(status: StatusCode, value: T) -> Response {
    match serde_json::to_value(value) {
        Ok(value) => workflow_command_json_response(status, value),
        Err(_) => workflow_command_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "rejected",
            "internal_error",
        ),
    }
}

fn workflow_command_error_response(
    status: StatusCode,
    outcome: &'static str,
    reason: &'static str,
) -> Response {
    workflow_command_json_response(
        status,
        json!({
            "schema_version": 2,
            "outcome": outcome,
            "reason": reason,
        }),
    )
}

fn workflow_command_json_rejection_response(rejection: JsonRejection) -> Response {
    let status = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    };
    workflow_command_error_response(status, "rejected", "invalid_request")
}

fn workflow_command_json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

async fn worker_claim_execution_lease(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Json(req): Json<WorkerExecutionLeaseClaimRequest>,
) -> Result<Response, ApiError> {
    if req.run_id.trim().is_empty() {
        return bad_request("Invalid execution lease request.");
    }
    let owner_id = authenticated_execution_lease_owner(&worker, &req.owner_id)?;
    if req.volume_proof.volume_id != req.volume_id
        || req.volume_proof.enrollment_epoch != req.enrollment_epoch
        || req.volume_proof.process_instance_id != req.process_instance_id
    {
        return bad_request("Runner-volume lease proof does not match the claim.");
    }
    let managed_cloud = managed_execution_lease_authority_input(
        req.workflow_request_id.as_deref(),
        req.managed_cloud_release.as_ref(),
        req.managed_cloud_release_sha256.as_deref(),
        req.managed_cloud_runtime_instance_id.as_deref(),
        req.managed_cloud_runtime_instance_epoch,
    )?;
    if managed_cloud.is_some() && worker.scope == "debug" {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
    let server_now_ms = jobs::now_ms();
    let payload_sha256 =
        super::jobs_runner_volumes::runner_volume_execution_lease_claim_payload_sha256(
            &super::jobs_runner_volumes::RunnerVolumeExecutionLeaseClaimPayload {
                worker_id: &worker.worker_id,
                account_id: &req.account_id,
                application_id: &req.application_id,
                run_id: &req.run_id,
                browser_profile_id: &req.browser_profile_id,
                owner_id,
                volume_id: &req.volume_id,
                enrollment_epoch: req.enrollment_epoch,
                process_instance_id: &req.process_instance_id,
                runtime_grant_id: &req.runtime_grant_id,
                runtime_sha256: &req.runtime_sha256,
                managed_cloud: managed_cloud.as_ref().map(|managed_cloud| {
                    super::jobs_runner_volumes::RunnerVolumeManagedExecutionLeaseClaimPayload {
                        workflow_request_id: &managed_cloud.workflow_request_id,
                        managed_cloud_release_sha256: &managed_cloud.managed_cloud_release_sha256,
                        runtime_instance_id: &managed_cloud.managed_cloud_runtime_instance_id,
                        runtime_instance_epoch: managed_cloud.managed_cloud_runtime_instance_epoch,
                    }
                }),
            },
        );
    let authority = jobs::verify_runner_volume_authority_proof(
        &state.pool,
        &req.volume_proof,
        "execution_lease_claim",
        &payload_sha256,
        server_now_ms,
        super::jobs_runner_volumes::RUNNER_VOLUME_AUTHORITY_MAX_CLOCK_SKEW_MS,
    )
    .map_err(super::jobs_runner_volumes::runner_volume_api_error)?;
    if authority.volume().worker_id != worker.worker_id {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Runner-volume authority is invalid.".to_string(),
        ));
    }
    let binding = jobs::BindRunnerVolumeResidencyRequest {
        account_id: req.account_id.clone(),
        run_id: req.run_id.clone(),
        worker_id: worker.worker_id.clone(),
        volume_id: req.volume_id.clone(),
        enrollment_epoch: req.enrollment_epoch,
        process_instance_id: req.process_instance_id.clone(),
        now_ms: server_now_ms,
    };
    jobs::claim_managed_execution_lease_for_runner_volume_authorized(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &req.run_id,
        &req.browser_profile_id,
        owner_id,
        &binding,
        &req.runtime_grant_id,
        &req.runtime_sha256,
        &authority,
        managed_cloud.as_ref(),
        &worker.worker_id,
        &binding.worker_id,
    )
    .map(worker_execution_lease_json_response)
    .map_err(execution_lease_error)
}

fn worker_execution_lease_json_response(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn managed_execution_lease_authority_input(
    workflow_request_id: Option<&str>,
    release: Option<&jobs::ManagedCloudReleaseMemoAuthority>,
    release_sha256: Option<&str>,
    runtime_instance_id: Option<&str>,
    runtime_instance_epoch: Option<i64>,
) -> Result<Option<jobs::ManagedCloudExecutionLeaseClaimInput>, ApiError> {
    let authority = match (
        workflow_request_id,
        release,
        release_sha256,
        runtime_instance_id,
        runtime_instance_epoch,
    ) {
        (None, None, None, None, None) => None,
        (
            Some(workflow_request_id),
            Some(release),
            Some(release_sha256),
            Some(runtime_instance_id),
            Some(runtime_instance_epoch),
        ) => {
            let exact_sha256 = jobs::managed_cloud_release_memo_sha256(release).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Invalid managed-cloud execution authority.".to_string(),
                )
            })?;
            if exact_sha256 != release_sha256
                || !valid_managed_workflow_request_id(workflow_request_id)
                || runtime_instance_epoch <= 0
                || !(20..=128).contains(&runtime_instance_id.len())
                || !runtime_instance_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid managed-cloud execution authority.".to_string(),
                ));
            }
            Some(jobs::ManagedCloudExecutionLeaseClaimInput {
                workflow_request_id: workflow_request_id.to_string(),
                managed_cloud_release: release.clone(),
                managed_cloud_release_sha256: release_sha256.to_string(),
                managed_cloud_runtime_instance_id: runtime_instance_id.to_string(),
                managed_cloud_runtime_instance_epoch: runtime_instance_epoch,
            })
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Managed-cloud execution authority must be complete.".to_string(),
            ));
        }
    };
    Ok(authority)
}

fn valid_managed_workflow_request_id(value: &str) -> bool {
    let Some(suffix) = value.strip_prefix("wfreq-v2-") else {
        return false;
    };
    uuid::Uuid::parse_str(suffix).is_ok_and(|request_id| {
        request_id.get_version_num() == 5
            && request_id.get_variant() == uuid::Variant::RFC4122
            && request_id.hyphenated().to_string() == suffix
    })
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

async fn worker_authorize_managed_execution_effect(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerManagedExecutionEffectRequest>,
) -> Result<Response, ApiError> {
    if worker.scope == "debug" {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
    let managed_cloud = managed_execution_lease_authority_input(
        req.workflow_request_id.as_deref(),
        req.managed_cloud_release.as_ref(),
        req.managed_cloud_release_sha256.as_deref(),
        req.managed_cloud_runtime_instance_id.as_deref(),
        req.managed_cloud_runtime_instance_epoch,
    )?
    .ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Managed-cloud execution authority is required.".to_string(),
        )
    })?;
    jobs::authorize_managed_execution_effect(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.lease_token,
        req.fence,
        Some(&managed_cloud),
        &worker.worker_id,
    )
    .map(worker_execution_lease_json_response)
    .map_err(execution_lease_error)
}

async fn worker_start_irreversible_submission(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerIrreversibleExecutionRequest>,
) -> Result<Response, ApiError> {
    if req.action != "submit" {
        return bad_request("Invalid irreversible execution action.");
    }
    let managed_cloud = managed_execution_lease_authority_input(
        req.workflow_request_id.as_deref(),
        req.managed_cloud_release.as_ref(),
        req.managed_cloud_release_sha256.as_deref(),
        req.managed_cloud_runtime_instance_id.as_deref(),
        req.managed_cloud_runtime_instance_epoch,
    )?;
    if managed_cloud.is_some() && worker.scope == "debug" {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Application evidence storage is not configured.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    let now_ms = jobs::now_ms();
    let bundle_capacity_bytes = storage.max_object_bytes().min(MAX_RECEIPT_BUNDLE_BYTES) as i64;
    let capacity = NewSubmissionEvidenceCapacity {
        account_id: req.account_id.clone(),
        application_id: req.application_id.clone(),
        run_id: run_id.clone(),
        runner: "cloud".to_string(),
        reserved_bytes: MAX_RECEIPT_EVIDENCE_BYTES as i64 + bundle_capacity_bytes,
        reserved_objects: MAX_SUBMISSION_EVIDENCE_OBJECTS,
        expires_at_ms: now_ms.saturating_add(jobs::SUBMISSION_RECONCILIATION_GRACE_MS),
        now_ms,
        limits: storage.upload_limits(),
    };
    jobs::start_irreversible_submission_authorized(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.lease_token,
        req.fence,
        &req.final_submit_proof,
        &capacity,
        managed_cloud.as_ref(),
        &worker.worker_id,
    )
    .map(worker_execution_lease_json_response)
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

async fn worker_reconcile_execution_checkpoint(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerCheckpointReconciliationRequest>,
) -> Result<Json<jobs::ExecutionLeaseRecord>, ApiError> {
    jobs::reconcile_execution_lease_checkpoint(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.owner_id,
        req.lease_token.as_deref(),
        req.fence,
        req.checkpoint_version,
        &req.checkpoint_phase,
    )
    .map(Json)
    .map_err(execution_lease_error)
}

async fn worker_restore_browser_profile_snapshot(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerBrowserProfileSnapshotAccessRequest>,
) -> Result<Response, ApiError> {
    let _object_lifecycle_guard =
        crate::db::account_data::acquire_account_object_writer(&state.pool, &req.account_id)
            .await
            .map_err(internal)?;
    let snapshot = jobs::get_browser_profile_snapshot_for_lease(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.browser_profile_id,
        &req.lease_token,
        req.fence,
    )
    .map_err(execution_lease_error)?;
    let Some(snapshot) = snapshot else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let storage = browser_profile_object_storage(&state)?;
    if !storage.key_belongs_to_account(&snapshot.object_key, &req.account_id) {
        return Err((
            StatusCode::CONFLICT,
            "The saved browser profile is unavailable.".to_string(),
        ));
    }
    let stored = storage
        .get(&snapshot.object_key)
        .await
        .map_err(browser_profile_storage_error)?;
    if let Err(error) = verify_browser_profile_snapshot_bytes(
        &stored.bytes,
        &snapshot.sha256,
        snapshot.size_bytes,
        storage.max_object_bytes(),
    ) {
        tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(&req.account_id),
            browser_profile_id = %req.browser_profile_id,
            generation = snapshot.generation,
            error = %error.1,
            "Bluey Jobs stored browser profile snapshot failed integrity verification"
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "The saved browser profile is unavailable.".to_string(),
        ));
    }
    Ok(Json(WorkerBrowserProfileSnapshotRestoreResponse {
        browser_profile_id: snapshot.browser_profile_id,
        generation: snapshot.generation,
        sha256: snapshot.sha256,
        size_bytes: snapshot.size_bytes,
        envelope_version: snapshot.envelope_version,
        encrypted_snapshot_base64: base64::engine::general_purpose::STANDARD.encode(stored.bytes),
    })
    .into_response())
}

async fn worker_store_browser_profile_snapshot(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<WorkerBrowserProfileSnapshotStoreRequest>,
) -> Result<Json<WorkerBrowserProfileSnapshotStoreResponse>, ApiError> {
    let storage = browser_profile_object_storage(&state)?;
    if req.envelope_version <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid browser profile envelope version.".to_string(),
        ));
    }
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(req.encrypted_snapshot_base64.as_bytes())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid encrypted browser profile snapshot.".to_string(),
            )
        })?;
    verify_browser_profile_snapshot_bytes(
        &encrypted,
        &req.sha256,
        req.size_bytes,
        storage.max_object_bytes(),
    )?;
    let _object_writer =
        crate::db::account_data::acquire_account_object_writer(&state.pool, &req.account_id)
            .await
            .map_err(internal)?;
    let next_generation = req.expected_generation.checked_add(1).ok_or((
        StatusCode::BAD_REQUEST,
        "Invalid browser profile generation.".to_string(),
    ))?;
    let object_key = storage.browser_profile_snapshot_key(
        &req.account_id,
        &req.browser_profile_id,
        next_generation,
        &req.sha256,
    );
    let current = jobs::authorize_browser_profile_snapshot_store(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.browser_profile_id,
        &req.lease_token,
        req.fence,
        req.expected_generation,
        &object_key,
        &req.sha256,
        req.size_bytes,
        req.envelope_version,
    )
    .map_err(execution_lease_error)?;
    let exact_replay = current.as_ref().is_some_and(|current| {
        current.generation == next_generation
            && current.object_key == object_key
            && current.sha256 == req.sha256
            && current.size_bytes == req.size_bytes
            && current.envelope_version == req.envelope_version
            && current.writer_run_id == run_id
            && current.writer_fence == req.fence
    });
    if !exact_replay
        && current.as_ref().map_or(0, |snapshot| snapshot.generation) != req.expected_generation
    {
        return Err((
            StatusCode::CONFLICT,
            "A newer browser profile snapshot is already available.".to_string(),
        ));
    }
    let reservation = object_uploads::reserve_account_object_upload(
        &state.pool,
        &NewObjectUpload {
            account_id: req.account_id.clone(),
            object_kind: ObjectKind::Artifact,
            logical_id: format!(
                "jobs-browser-profile:{}:{next_generation}:{}:{}",
                req.browser_profile_id, req.envelope_version, req.sha256
            ),
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: object_key.clone(),
            size_bytes: req.size_bytes,
            sha256: req.sha256.clone(),
            content_type: jobs::BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE.to_string(),
            expires_at_ms: i64::MAX,
            metadata_json: json!({
                "artifact_class": "jobs_browser_profile_snapshot",
                "jobs_browser_profile_id": req.browser_profile_id,
                "jobs_application_id": req.application_id,
                "jobs_run_id": run_id,
                "generation": next_generation,
                "expected_generation": req.expected_generation,
                "writer_fence": req.fence,
                "envelope_version": req.envelope_version,
                "retention_policy": "account_lifetime_until_deletion",
            }),
            now_ms: jobs::now_ms(),
            limits: storage.upload_limits(),
        },
    )
    .map_err(browser_profile_upload_control_error)?;
    let encrypted = bytes::Bytes::from(encrypted);
    if reservation.needs_put {
        object_uploads::begin_upload_put(&state.pool, &reservation.upload.id, jobs::now_ms())
            .map_err(browser_profile_upload_control_error)?;
        if let Err(error) = storage
            .put(
                &object_key,
                encrypted.clone(),
                jobs::BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE,
            )
            .await
        {
            let _ = object_uploads::record_put_failure(
                &state.pool,
                &reservation.upload.id,
                &error.to_string(),
                jobs::now_ms(),
            );
            return Err(browser_profile_storage_error(error));
        }
    }
    let read_back = storage.get(&object_key).await.map_err(|error| {
        let _ = object_uploads::record_put_failure(
            &state.pool,
            &reservation.upload.id,
            &error.to_string(),
            jobs::now_ms(),
        );
        browser_profile_storage_error(error)
    })?;
    let valid_content_type = read_back
        .content_type
        .split(';')
        .next()
        .is_some_and(|value| {
            value.eq_ignore_ascii_case(jobs::BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE)
        });
    let valid_payload = verify_browser_profile_snapshot_bytes(
        &read_back.bytes,
        &req.sha256,
        req.size_bytes,
        storage.max_object_bytes(),
    )
    .is_ok();
    if !valid_content_type || read_back.bytes != encrypted || !valid_payload {
        let _ = object_uploads::record_put_failure(
            &state.pool,
            &reservation.upload.id,
            "browser profile read-back verification failed",
            jobs::now_ms(),
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            "The saved browser profile failed integrity verification.".to_string(),
        ));
    }
    if reservation.needs_put {
        object_uploads::release_verified_upload_put(
            &state.pool,
            &reservation.upload.id,
            jobs::now_ms(),
        )
        .map_err(browser_profile_upload_control_error)?;
    }
    let committed = jobs::publish_browser_profile_snapshot_for_lease(
        &state.pool,
        &req.account_id,
        &req.application_id,
        &run_id,
        &req.browser_profile_id,
        &req.lease_token,
        req.fence,
        req.expected_generation,
        &object_key,
        &req.sha256,
        req.size_bytes,
        req.envelope_version,
        &reservation.upload.id,
    );
    let committed = committed.map_err(|error| {
        // The verified upload stays pending unless the pointer transaction
        // commits it ready. This avoids a check-then-delete race with an
        // uncertain commit; exact retries reconcile pending bytes, while the
        // stale-upload worker removes a definite loser.
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&req.account_id),
            browser_profile_id = %req.browser_profile_id,
            generation = next_generation,
            object_key = %object_key,
            "Bluey Jobs browser profile metadata commit lost its lease or generation race"
        );
        execution_lease_error(error)
    })?;
    Ok(Json(browser_profile_snapshot_store_response(&committed)))
}

fn browser_profile_snapshot_store_response(
    snapshot: &jobs::BrowserProfileSnapshotRecord,
) -> WorkerBrowserProfileSnapshotStoreResponse {
    WorkerBrowserProfileSnapshotStoreResponse {
        browser_profile_id: snapshot.browser_profile_id.clone(),
        generation: snapshot.generation,
        sha256: snapshot.sha256.clone(),
        size_bytes: snapshot.size_bytes,
        envelope_version: snapshot.envelope_version,
    }
}

fn browser_profile_object_storage(state: &AppState) -> Result<ObjectStorage, ApiError> {
    state
        .config
        .object_storage
        .clone()
        .map(ObjectStorage::new)
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "Browser profile recovery storage is unavailable.".to_string(),
        ))
}

fn verify_browser_profile_snapshot_bytes(
    bytes: &[u8],
    expected_sha256: &str,
    expected_size_bytes: i64,
    maximum_bytes: usize,
) -> Result<(), ApiError> {
    let actual_size = i64::try_from(bytes.len()).map_err(|_| {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Encrypted browser profile snapshot is too large.".to_string(),
        )
    })?;
    if bytes.is_empty()
        || bytes.len() > maximum_bytes
        || expected_size_bytes <= 0
        || actual_size != expected_size_bytes
        || expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || sha256_hex(bytes) != expected_sha256
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Encrypted browser profile snapshot did not pass integrity checks.".to_string(),
        ));
    }
    Ok(())
}

fn browser_profile_storage_error(error: anyhow::Error) -> ApiError {
    tracing::error!(error = %error, "Bluey Jobs browser profile storage failed");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "Browser profile recovery storage is unavailable.".to_string(),
    )
}

fn browser_profile_upload_control_error(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<UploadControlError>() {
        Some(UploadControlError::ObjectTooLarge) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Encrypted browser profile snapshot is too large.".to_string(),
        ),
        Some(
            UploadControlError::AccountBytesQuotaExceeded
            | UploadControlError::AccountObjectQuotaExceeded,
        ) => (
            StatusCode::INSUFFICIENT_STORAGE,
            "Browser profile recovery storage quota is unavailable.".to_string(),
        ),
        Some(UploadControlError::DailyQuotaExceeded) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Browser profile recovery upload capacity is temporarily unavailable.".to_string(),
        ),
        Some(UploadControlError::AccountDeleting) => (
            StatusCode::CONFLICT,
            "Account deletion has fenced new browser profile snapshots.".to_string(),
        ),
        Some(
            UploadControlError::IdempotencyConflict
            | UploadControlError::UploadInProgress
            | UploadControlError::UploadGone,
        ) => (
            StatusCode::CONFLICT,
            "This browser profile generation is already being stored differently.".to_string(),
        ),
        _ => internal(error),
    }
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
    lease_token: String,
    fence: i64,
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
    Extension(_worker): Extension<JobsWorkerIdentity>,
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
        Some(CloudReceiptAccess {
            lease_token: req.lease_token,
            fence: req.fence,
        }),
        None,
        None,
        None,
    )
    .await
    .map(Json)
}

struct CloudReceiptAccess {
    lease_token: String,
    fence: i64,
}

#[allow(clippy::too_many_arguments)]
async fn persist_submission_receipt(
    state: &AppState,
    account_id: &str,
    application_id: &str,
    mut receipt: Value,
    evidence_objects: Vec<ReceiptEvidenceObject>,
    expected_runner: &str,
    cloud_access: Option<CloudReceiptAccess>,
    local_ticket_hash: Option<&str>,
    local_result_capability: Option<&str>,
    local_capability_version: Option<u8>,
) -> Result<JobApplication, ApiError> {
    if receipt.get(SUBMISSION_FINGERPRINT_KEY).is_some()
        || receipt.get(jobs::SERVER_SUBMISSION_AUTHORITY_KEY).is_some()
        || receipt.get("receiptObject").is_some()
        || receipt.get("evidenceObjects").is_some()
    {
        return bad_request("Submission receipt contains a reserved server field.");
    }
    let application = jobs::get_application(&state.pool, account_id, application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    let receipt_id = required_receipt_string(&receipt, "receiptId")?;
    let certified_execution = jobs::application_has_frozen_ats_certification(&application);
    let expected_receipt_schema = if certified_execution { 2 } else { 1 };
    if receipt.get("schemaVersion").and_then(Value::as_i64) != Some(expected_receipt_schema) {
        return bad_request("Unsupported submission receipt version.");
    }
    if !certified_execution && receipt.get("atsCertifiedReceiptAuthority").is_some() {
        return bad_request("Review submission receipts cannot carry certified authority.");
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
    if certified_execution {
        validate_receipt_ats_certification_authority(
            &state.pool,
            &application,
            &receipt,
            account_id,
            application_id,
            &run_id,
            expected_runner,
        )?;
    }
    let result = receipt.get("result").and_then(Value::as_object).ok_or((
        StatusCode::BAD_REQUEST,
        "Submission result is missing.".to_string(),
    ))?;
    if result.get("status").and_then(Value::as_str) != Some("submitted") {
        return bad_request("Only a confirmed submission can create a final receipt.");
    }
    let submit_http_status = successful_submit_http_status(result)?;
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
    if application.state == "submitted" {
        let stored_execution_authority = application
            .receipt
            .pointer(&format!(
                "/{}/executionAuthority",
                jobs::SERVER_SUBMISSION_AUTHORITY_KEY
            ))
            .ok_or((
                StatusCode::CONFLICT,
                "This application already has a different final receipt.".to_string(),
            ))?;
        let authority_matches = match (
            expected_runner,
            cloud_access.as_ref(),
            local_ticket_hash,
            local_result_capability,
        ) {
            ("cloud", Some(access), None, None) => jobs::submitted_cloud_receipt_replay_authorized(
                &application,
                &run_id,
                &access.lease_token,
                access.fence,
            ),
            ("local", None, _, Some(result_capability)) => {
                jobs::submitted_local_receipt_replay_authorized(
                    &application,
                    &run_id,
                    result_capability,
                )
            }
            _ => false,
        };
        if !authority_matches {
            return Err((
                StatusCode::CONFLICT,
                "This application already has a different final receipt.".to_string(),
            ));
        }
        let request_fingerprint = submission_request_fingerprint(
            &receipt,
            &evidence_objects,
            stored_execution_authority,
        )?;
        return match submission_finalize_error_disposition(Some(&application), &request_fingerprint)
        {
            SubmissionFinalizeErrorDisposition::Replay => mark_submitted_cloud_workflow_terminal(
                &state.pool,
                application,
                account_id,
                application_id,
                &run_id,
                expected_runner,
            ),
            SubmissionFinalizeErrorDisposition::Conflict
            | SubmissionFinalizeErrorDisposition::NotCommitted => Err((
                StatusCode::CONFLICT,
                "This application already has a different final receipt.".to_string(),
            )),
        };
    }

    let execution_authority = match (
        expected_runner,
        cloud_access,
        local_ticket_hash,
        local_result_capability,
        local_capability_version,
    ) {
        ("cloud", Some(access), None, None, None) => {
            let authority = jobs::execution_receipt_authority(
                &state.pool,
                account_id,
                application_id,
                &run_id,
                &access.lease_token,
                access.fence,
            )
            .map_err(submission_receipt_authority_error)?;
            json!({
                "kind": "cloud_execution_lease",
                "ownerId": authority.owner_id,
                "leaseTokenSha256": authority.lease_token_sha256,
                "fence": authority.fence,
                "phase": authority.phase,
            })
        }
        ("local", None, Some(ticket_hash), Some(result_capability), Some(capability_version)) => {
            let result_capability_sha256 =
                jobs::local_result_replay_credential_sha256(result_capability).ok_or((
                    StatusCode::BAD_REQUEST,
                    "Submission receipt runner authority is invalid.".to_string(),
                ))?;
            let mut authority = json!({
                "kind": "local_run_ticket",
                "ticketHash": ticket_hash,
                "runId": run_id,
                "resultCapabilitySha256": result_capability_sha256,
            });
            match capability_version {
                super::jobs_local_capability::LEGACY_TOKEN_VERSION => {}
                super::jobs_local_capability::TOKEN_VERSION => {
                    let release = jobs::get_local_run_browser_release_binding(
                        &state.pool,
                        account_id,
                        &run_id,
                    )
                    .map_err(internal)?
                    .ok_or((
                        StatusCode::CONFLICT,
                        "This local Browser run has no immutable release binding.".to_string(),
                    ))?;
                    authority["browserRelease"] = jobs::browser_release_receipt_authority(&release);
                }
                _ => return bad_request("Submission receipt runner authority is invalid."),
            }
            authority
        }
        _ => {
            return bad_request("Submission receipt runner authority is invalid.");
        }
    };
    if expected_runner == "cloud"
        && !matches!(
            execution_authority.get("phase").and_then(Value::as_str),
            Some("submitted" | "side_effect_unknown")
        )
    {
        return Err((
            StatusCode::CONFLICT,
            "A matching cloud execution lease cannot accept this receipt.".to_string(),
        ));
    }
    let request_fingerprint =
        submission_request_fingerprint(&receipt, &evidence_objects, &execution_authority)?;
    let verified_claim_ids = frozen_submission_claim_ids(&application)?;
    validate_receipt_verified_claim_ids(&receipt, &verified_claim_ids)?;
    validate_provider_submission_proof(&application, &receipt)?;
    validate_receipt_final_submit_proof(&application, &receipt)?;
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
    let (_, approved_job, _) = approved_execution_snapshot(&application)?;
    let storage_config = state.config.object_storage.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Application evidence storage is not configured.".to_string(),
    ))?;
    let storage = ObjectStorage::new(storage_config);
    let _object_writer =
        crate::db::account_data::acquire_account_object_writer(&state.pool, account_id)
            .await
            .map_err(internal)?;
    let prepared_objects =
        preflight_receipt_evidence(&receipt, evidence_objects, storage.max_object_bytes())?;
    let preflight_verified = prepared_objects
        .iter()
        .map(|object| (object.original_key.clone(), object.sha256.clone()))
        .collect::<BTreeMap<_, _>>();
    validate_receipt_bundle(
        account_id,
        &application,
        &resume,
        &receipt,
        &preflight_verified,
        false,
    )?;
    let terminal_session =
        submission_terminal_session(state, account_id, application_id, &run_id, expected_runner)?;
    let submission_authority = submission_authority_snapshot(&application, execution_authority)?;
    let receipt_object = receipt
        .as_object_mut()
        .expect("validated receipt fields require an object");
    receipt_object.insert(
        SUBMISSION_FINGERPRINT_KEY.to_string(),
        Value::String(request_fingerprint.clone()),
    );
    receipt_object.insert(
        jobs::SERVER_SUBMISSION_AUTHORITY_KEY.to_string(),
        submission_authority,
    );
    let mut uploaded = match upload_receipt_evidence(
        &state.pool,
        &storage,
        account_id,
        application_id,
        &run_id,
        expected_runner,
        &request_fingerprint,
        &mut receipt,
        prepared_objects,
    )
    .await
    {
        Ok(uploaded) => uploaded,
        Err(error) => {
            return reconcile_submission_precommit_error(
                &state.pool,
                account_id,
                application_id,
                &request_fingerprint,
                error,
            )
        }
    };
    if let Err(error) = upload_immutable_receipt_bundle(
        &state.pool,
        &storage,
        account_id,
        application_id,
        &run_id,
        expected_runner,
        &request_fingerprint,
        &receipt_id,
        &approved_job,
        &resume,
        &mut receipt,
        &mut uploaded,
    )
    .await
    {
        return reconcile_submission_precommit_error(
            &state.pool,
            account_id,
            application_id,
            &request_fingerprint,
            error,
        );
    }
    validate_receipt_bundle(
        account_id,
        &application,
        &resume,
        &receipt,
        &uploaded.verified,
        true,
    )?;
    let provider = required_receipt_string(&receipt, "adapter")?;
    let evidence = match submission_evidence_records(
        application_id,
        &receipt_id,
        &provider,
        &approved_job,
        &resume,
        &receipt,
        &uploaded.verified,
        submit_http_status,
        confirmation,
        confirmation_url,
        confirmation_text,
        submitted_at,
    ) {
        Ok(evidence) => evidence,
        Err(error) => return Err(error),
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
        &uploaded.uploads,
        &terminal_session,
        local_ticket_hash,
    );
    let finalized_application = match finalized {
        Ok(jobs::SubmissionFinalizeResult::Committed(application))
        | Ok(jobs::SubmissionFinalizeResult::Replayed(application)) => Ok(application),
        Err(error) => {
            // A PostgreSQL COMMIT can take effect even when the client receives
            // a transport error. Never delete evidence on an uncertain result:
            // first reconcile against the primary application authority. If
            // that read also fails, pending objects remain in the durable
            // ledger for the stale-upload cleanup worker, while committed
            // ready objects remain attached to Submitted.
            match jobs::get_application(&state.pool, account_id, application_id) {
                Ok(application) => match submission_finalize_error_disposition(
                    application.as_ref(),
                    &request_fingerprint,
                ) {
                    SubmissionFinalizeErrorDisposition::Replay => {
                        Ok(application.expect("replay disposition requires an application"))
                    }
                    SubmissionFinalizeErrorDisposition::Conflict => Err((
                        StatusCode::CONFLICT,
                        "This application already has a different final receipt.".to_string(),
                    )),
                    SubmissionFinalizeErrorDisposition::NotCommitted => {
                        Err(submission_domain_error(error))
                    }
                },
                Err(reconcile_error) => {
                    tracing::error!(
                        error = %reconcile_error,
                        application_id_hash = %sha256_hex(application_id.as_bytes()),
                        "submission commit result is uncertain; durable evidence was retained"
                    );
                    Err(submission_domain_error(error))
                }
            }
        }
    }?;
    mark_submitted_cloud_workflow_terminal(
        &state.pool,
        finalized_application,
        account_id,
        application_id,
        &run_id,
        expected_runner,
    )
}

fn mark_submitted_cloud_workflow_terminal(
    pool: &crate::db::DbPool,
    application: JobApplication,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    expected_runner: &str,
) -> Result<JobApplication, ApiError> {
    if expected_runner != "cloud" {
        return Ok(application);
    }
    jobs::mark_jobs_workflow_execution_submitted(
        pool,
        account_id,
        application_id,
        run_id,
        &workflow_id_for_run(run_id),
        jobs::now_ms(),
    )
    .map_err(|_| {
        tracing::warn!(
            reason_code = "workflow_submission_terminal_evidence_failed",
            "Submitted Jobs workflow terminal evidence could not be persisted"
        );
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey Jobs is still finalizing this submitted application. Please retry.".to_string(),
        )
    })?;
    Ok(application)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubmissionFinalizeErrorDisposition {
    Replay,
    Conflict,
    NotCommitted,
}

fn submission_finalize_error_disposition(
    application: Option<&JobApplication>,
    request_fingerprint: &str,
) -> SubmissionFinalizeErrorDisposition {
    let Some(application) = application else {
        return SubmissionFinalizeErrorDisposition::NotCommitted;
    };
    if application.state != "submitted" {
        return SubmissionFinalizeErrorDisposition::NotCommitted;
    }
    if application
        .receipt
        .get(SUBMISSION_FINGERPRINT_KEY)
        .and_then(Value::as_str)
        != Some(request_fingerprint)
    {
        return SubmissionFinalizeErrorDisposition::Conflict;
    }
    SubmissionFinalizeErrorDisposition::Replay
}

fn reconcile_submission_precommit_error(
    pool: &crate::db::DbPool,
    account_id: &str,
    application_id: &str,
    request_fingerprint: &str,
    original_error: ApiError,
) -> Result<JobApplication, ApiError> {
    match jobs::get_application(pool, account_id, application_id) {
        Ok(application) => {
            match submission_finalize_error_disposition(application.as_ref(), request_fingerprint) {
                SubmissionFinalizeErrorDisposition::Replay => {
                    Ok(application.expect("replay disposition requires an application"))
                }
                SubmissionFinalizeErrorDisposition::Conflict => Err((
                    StatusCode::CONFLICT,
                    "This application already has a different final receipt.".to_string(),
                )),
                SubmissionFinalizeErrorDisposition::NotCommitted => Err(original_error),
            }
        }
        Err(reconcile_error) => {
            tracing::error!(
                error = %reconcile_error,
                application_id_hash = %sha256_hex(application_id.as_bytes()),
                "submission upload error could not be reconciled; durable evidence was retained"
            );
            Err(original_error)
        }
    }
}

fn submission_request_fingerprint(
    receipt: &Value,
    evidence_objects: &[ReceiptEvidenceObject],
    execution_authority: &Value,
) -> Result<String, ApiError> {
    let encoded = serde_json::to_vec(&json!({
        "receipt": receipt,
        "evidence_objects": evidence_objects,
        "execution_authority": execution_authority,
    }))
    .map_err(|error| internal(error.into()))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn submission_authority_snapshot(
    application: &JobApplication,
    execution_authority: Value,
) -> Result<Value, ApiError> {
    // Re-run the checksum/admission validator before copying any authority into
    // the immutable receipt. The complete pre-submission receipt preserves the
    // exact approved packet, Auto-submit admission, identity/evidence binding,
    // metering state, and intervention-driven packet revision history.
    approved_execution_snapshot(application)?;
    Ok(json!({
        "schemaVersion": 1,
        "preSubmissionReceipt": application.receipt,
        "executionAuthority": execution_authority,
    }))
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

fn frozen_submission_claim_ids(application: &JobApplication) -> Result<Vec<String>, ApiError> {
    let claims = if application.state == "submitted" {
        application.receipt.pointer("/packet/verifiedClaimIds")
    } else {
        application
            .receipt
            .pointer("/approved_execution/packet/verifiedClaimIds")
    }
    .and_then(Value::as_array)
    .ok_or((
        StatusCode::CONFLICT,
        "The approved submission claims are missing.".to_string(),
    ))?;
    let mut claim_ids = claims
        .iter()
        .map(|claim| {
            claim.as_str().map(str::to_string).ok_or((
                StatusCode::CONFLICT,
                "The approved submission claims are invalid.".to_string(),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let original_len = claim_ids.len();
    claim_ids.sort();
    claim_ids.dedup();
    if claim_ids.len() != original_len {
        return Err((
            StatusCode::CONFLICT,
            "The approved submission claims are invalid.".to_string(),
        ));
    }
    Ok(claim_ids)
}

fn validate_provider_submission_proof(
    application: &JobApplication,
    receipt: &Value,
) -> Result<(), ApiError> {
    let frozen_url = if application.state == "submitted" {
        application.receipt.pointer("/job/canonicalUrl")
    } else {
        application
            .receipt
            .pointer("/approved_execution/job/canonicalUrl")
    }
    .and_then(Value::as_str)
    .ok_or((
        StatusCode::CONFLICT,
        "The approved application provider is missing.".to_string(),
    ))?;
    let (adapter, adapter_version, event_type) = match ats_kind(frozen_url) {
        "greenhouse" => (
            "greenhouse",
            GREENHOUSE_SUBMISSION_ADAPTER_VERSION,
            "greenhouse_state_transition",
        ),
        "lever" => (
            "lever",
            LEVER_SUBMISSION_ADAPTER_VERSION,
            "lever_state_changed",
        ),
        _ => {
            return Err((
                StatusCode::CONFLICT,
                "This application provider is not authorized for final submission.".to_string(),
            ));
        }
    };
    let final_submit_proof = jobs::stored_final_submit_proof(application).map_err(|error| {
        tracing::warn!(
            application_id_hash = %sha256_hex(application.id.as_bytes()),
            error = %error,
            "submitted receipt is missing its exact provider target proof"
        );
        (
            StatusCode::CONFLICT,
            "The pre-click provider target proof is missing or invalid.".to_string(),
        )
    })?;
    let approved_job_key = jobs::final_submit_provider_job_key(
        adapter,
        &final_submit_proof.job.approved_canonical_url,
    )
    .map_err(|_| {
        (
            StatusCode::CONFLICT,
            "The pre-click provider target proof is missing or invalid.".to_string(),
        )
    })?;
    let target_job_key =
        jobs::final_submit_provider_job_key(adapter, &final_submit_proof.target.action_url)
            .map_err(|_| {
                (
                    StatusCode::CONFLICT,
                    "The pre-click provider target proof is missing or invalid.".to_string(),
                )
            })?;
    let expected_proof_schema = if jobs::application_has_frozen_ats_certification(application) {
        4
    } else {
        3
    };
    if final_submit_proof.schema_version != expected_proof_schema
        || final_submit_proof.adapter != adapter
        || final_submit_proof.job.approved_canonical_url != frozen_url
        || target_job_key != approved_job_key
        || final_submit_proof.target.provider_job_key != approved_job_key
    {
        return Err((
            StatusCode::CONFLICT,
            "The pre-click provider target proof is missing or invalid.".to_string(),
        ));
    }
    if receipt.get("adapter").and_then(Value::as_str) != Some(adapter)
        || receipt.get("adapterVersion").and_then(Value::as_str) != Some(adapter_version)
    {
        return bad_request("Receipt provider implementation does not match the approved adapter.");
    }
    let result = receipt.get("result").and_then(Value::as_object).ok_or((
        StatusCode::BAD_REQUEST,
        "Submission result is missing.".to_string(),
    ))?;
    if result
        .get("issues")
        .and_then(Value::as_array)
        .is_none_or(|issues| !issues.is_empty())
    {
        return bad_request("A submitted receipt cannot contain unresolved browser issues.");
    }
    successful_submit_http_status(result)?;
    let confirmation_text = result
        .get("confirmationText")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "A submitted receipt needs explicit employer confirmation text.".to_string(),
        ))?;
    if !has_explicit_submission_confirmation(confirmation_text) {
        return bad_request("Employer confirmation text does not prove a successful submission.");
    }
    let confirmation_url = result
        .get("confirmationUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "A submitted receipt needs an employer confirmation URL.".to_string(),
        ))?;
    let final_url = receipt
        .get("finalUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "A submitted receipt needs the final employer URL.".to_string(),
        ))?;
    let confirmation_target = parse_provider_application_target(
        confirmation_url,
        ProviderApplicationTargetPurpose::Confirmation,
    )
    .filter(|target| target.provider == adapter)
    .ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt confirmation is not bound to the approved provider job.".to_string(),
    ))?;
    if final_url != confirmation_url || confirmation_target.provider_job_key != approved_job_key {
        return bad_request("Receipt confirmation is not bound to the approved provider page.");
    }
    let generated_at = receipt
        .get("generatedAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt generation time is invalid.".to_string(),
        ))?;
    let submitted_at = result
        .get("submittedAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt submission time is invalid.".to_string(),
        ))?;
    if submitted_at > generated_at + chrono::Duration::minutes(5)
        || submitted_at < generated_at - chrono::Duration::minutes(30)
    {
        return bad_request("Receipt confirmation timing is not bound to this browser run.");
    }
    let provider_event = receipt
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|event| {
            if event.get("type").and_then(Value::as_str) != Some(event_type) {
                return false;
            }
            let detail = event.get("detail").and_then(Value::as_object);
            match adapter {
                "greenhouse" => {
                    detail
                        .and_then(|value| value.get("state"))
                        .and_then(Value::as_str)
                        == Some("receipt")
                        && detail
                            .and_then(|value| value.get("status"))
                            .and_then(Value::as_str)
                            == Some("submitted")
                        && detail
                            .and_then(|value| value.get("capability"))
                            .and_then(Value::as_str)
                            == Some("beta_review")
                }
                "lever" => {
                    detail
                        .and_then(|value| value.get("state"))
                        .and_then(Value::as_str)
                        == Some("receipt")
                        && detail
                            .and_then(|value| value.get("outcome"))
                            .and_then(Value::as_str)
                            == Some("submitted")
                        && detail
                            .and_then(|value| value.get("page_kind"))
                            .and_then(Value::as_str)
                            == Some("confirmation")
                        && detail
                            .and_then(|value| value.get("mode"))
                            .and_then(Value::as_str)
                            == Some("review_only")
                }
                _ => false,
            }
        })
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt is missing the provider final-state transition.".to_string(),
        ))?;
    let provider_event_at = provider_event
        .get("occurredAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt provider final-state time is invalid.".to_string(),
        ))?;
    if provider_event_at < submitted_at - chrono::Duration::minutes(1)
        || provider_event_at > generated_at + chrono::Duration::minutes(5)
    {
        return bad_request("Receipt provider proof is not bound to the submission time.");
    }
    Ok(())
}

fn successful_submit_http_status(result: &serde_json::Map<String, Value>) -> Result<i64, ApiError> {
    result
        .get("submitHttpStatus")
        .and_then(Value::as_i64)
        .filter(|status| {
            (200..=299).contains(status) || matches!(*status, 301 | 302 | 303 | 307 | 308)
        })
        .ok_or((
            StatusCode::BAD_REQUEST,
            "A submitted receipt needs a successful exact POST HTTP status.".to_string(),
        ))
}

fn validate_receipt_final_submit_proof(
    application: &JobApplication,
    receipt: &Value,
) -> Result<(), ApiError> {
    let proof = jobs::stored_final_submit_proof(application).map_err(|error| {
        tracing::warn!(
            application_id_hash = %sha256_hex(application.id.as_bytes()),
            error = %error,
            "submitted receipt is missing its pre-click document proof"
        );
        (
            StatusCode::CONFLICT,
            "The pre-click submission proof is missing or invalid.".to_string(),
        )
    })?;
    if receipt.get("adapter").and_then(Value::as_str) != Some(proof.adapter.as_str())
        || receipt.get("adapterVersion").and_then(Value::as_str)
            != Some(proof.adapter_version.as_str())
    {
        return bad_request("Receipt provider does not match its pre-click proof.");
    }
    let documents = receipt.get("documents").and_then(Value::as_array).ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt documents are missing.".to_string(),
    ))?;
    if documents.len() != proof.documents.len() {
        return bad_request("Receipt documents do not match the pre-click proof.");
    }
    let mut actual = BTreeMap::new();
    for document in documents {
        let kind = document
            .get("kind")
            .and_then(Value::as_str)
            .filter(|kind| matches!(*kind, "resume" | "cover_letter"))
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt contains a document that was not bound before Submit.".to_string(),
            ))?;
        let version_id = document
            .get("versionId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let sha256 = document
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|value| valid_sha256(value))
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt document checksum is invalid.".to_string(),
            ))?;
        if actual
            .insert(kind.to_string(), (version_id, sha256.to_ascii_lowercase()))
            .is_some()
        {
            return bad_request("Receipt contains duplicate submitted documents.");
        }
    }
    for expected in &proof.documents {
        let Some((version_id, sha256)) = actual.get(&expected.kind) else {
            return bad_request("Receipt documents do not match the pre-click proof.");
        };
        if version_id != &expected.version_id || sha256 != &expected.sha256 {
            return bad_request("Receipt documents do not match the pre-click proof.");
        }
    }
    Ok(())
}

fn validate_receipt_ats_certification_authority(
    pool: &crate::db::DbPool,
    application: &JobApplication,
    receipt: &Value,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    expected_runner: &str,
) -> Result<jobs::AtsCertifiedReceiptAuthority, ApiError> {
    let authority = receipt
        .get("atsCertifiedReceiptAuthority")
        .cloned()
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Certified submission receipt authority is missing.".to_string(),
        ))?;
    let authority = serde_json::from_value::<jobs::AtsCertifiedReceiptAuthority>(authority)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Certified submission receipt authority is invalid.".to_string(),
            )
        })?;
    let proof = jobs::stored_final_submit_proof(application).map_err(|_| {
        (
            StatusCode::CONFLICT,
            "The certified pre-click authority is missing or invalid.".to_string(),
        )
    })?;
    let certification = proof.certification.as_ref().ok_or((
        StatusCode::CONFLICT,
        "The certified pre-click authority is missing or invalid.".to_string(),
    ))?;
    let observed_surface = proof.observed_surface.as_ref().ok_or((
        StatusCode::CONFLICT,
        "The certified layout authority is missing or invalid.".to_string(),
    ))?;
    let generated_at_ms = receipt
        .get("generatedAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Certified submission receipt time is invalid.".to_string(),
        ))?;
    let ids_are_valid = [
        authority.application_attempt_id.as_str(),
        authority.phase_b_request_id.as_str(),
    ]
    .into_iter()
    .all(valid_approved_execution_identifier);
    let digests_are_valid = [
        authority.binding_sha256.as_str(),
        authority.canary_reservation_sha256.as_str(),
        authority.layout_observation_sha256.as_str(),
        authority.metering_reservation_sha256.as_str(),
        authority.observed_surface_sha256.as_str(),
    ]
    .into_iter()
    .all(valid_approved_execution_sha256);
    let exact = authority.schema_version == 1
        && authority.account_id == account_id
        && authority.application_id == application_id
        && authority.run_id == run_id
        && authority.provider == proof.adapter
        && authority.adapter == proof.adapter
        && authority.adapter_version == proof.adapter_version
        && authority.manifest_sha256 == certification.manifest_sha256
        && authority.activation_sha256 == certification.activation_sha256
        && authority.activation_generation == certification.activation_generation
        && authority.target_key_sha256 == certification.target_key_sha256
        && authority.layout_set_sha256 == certification.layout_set_sha256
        && authority.observed_surface_sha256 == observed_surface.surface_sha256
        && authority.adapter_bundle_sha256 == certification.adapter_bundle_sha256
        && authority.runner_kind == expected_runner
        && certification
            .runner_target_sha256s
            .contains(&authority.runner_target_sha256)
        && authority.binding_fence > 0
        && authority.binding_consumed_at_ms > 0
        && authority.binding_consumed_at_ms <= generated_at_ms
        && authority.binding_consumed_at_ms <= certification.expires_at_ms
        && matches!(authority.rollout_channel.as_str(), "canary" | "general")
        && ids_are_valid
        && digests_are_valid;
    if !exact {
        return bad_request(
            "Certified submission receipt authority does not match the frozen execution.",
        );
    }
    jobs::lookup_ats_certification_terminal_receipt_authority(
        pool,
        account_id,
        application_id,
        run_id,
        &authority,
    )
    .map_err(|error| match error {
        jobs::AtsCertificationAuthorityError::Storage(error) => internal(error),
        _ => (
            StatusCode::CONFLICT,
            "Certified submission receipt authority is not the exact terminal server record."
                .to_string(),
        ),
    })
}

fn has_explicit_submission_confirmation(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    let words = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let contains_phrase = |phrase: &[&str]| {
        !phrase.is_empty() && words.windows(phrase.len()).any(|window| window == phrase)
    };
    let followed_by_application = |phrase: &[&str], determiners: &[&str]| {
        words
            .windows(phrase.len())
            .enumerate()
            .any(|(index, window)| {
                if window != phrase {
                    return false;
                }
                let next = index + phrase.len();
                words.get(next) == Some(&"application")
                    || (words
                        .get(next)
                        .is_some_and(|word| determiners.contains(word))
                        && words.get(next + 1) == Some(&"application"))
            })
    };
    let followed_by_optional_yet_successful_outcome = |phrase: &[&str]| {
        words
            .windows(phrase.len())
            .enumerate()
            .any(|(index, window)| {
                if window != phrase {
                    return false;
                }
                let mut next = index + phrase.len();
                if words.get(next) == Some(&"yet") {
                    next += 1;
                }
                if words.get(next) == Some(&"successfully") {
                    next += 1;
                }
                words
                    .get(next)
                    .is_some_and(|outcome| matches!(*outcome, "submitted" | "received"))
            })
    };

    let negative_application_outcome = contains_phrase(&["already", "applied"])
        || followed_by_application(&["already", "submitted"], &["an", "the", "your"])
        || contains_phrase(&["application", "already", "submitted"])
        || contains_phrase(&["application", "was", "already", "submitted"])
        || contains_phrase(&["application", "has", "been", "already", "submitted"])
        || contains_phrase(&["application", "had", "been", "already", "submitted"])
        || contains_phrase(&["application", "has", "already", "been", "submitted"])
        || contains_phrase(&["application", "had", "already", "been", "submitted"])
        || [
            &["unable", "to", "submit"][..],
            &["failed", "to", "submit"],
            &["could", "not", "submit"],
            &["couldn", "t", "submit"],
            &["cannot", "submit"],
            &["can", "t", "submit"],
        ]
        .iter()
        .any(|phrase| followed_by_application(phrase, &["the", "your"]))
        || [
            &["could", "not", "be", "submitted"][..],
            &["couldn", "t", "be", "submitted"],
            &["cannot", "be", "submitted"],
            &["can", "t", "be", "submitted"],
            &["unable", "to", "be", "submitted"],
        ]
        .iter()
        .any(|phrase| contains_phrase(phrase))
        || [
            &["have", "not", "submitted"][..],
            &["have", "not", "yet", "submitted"],
            &["haven", "t", "submitted"],
            &["haven", "t", "yet", "submitted"],
            &["has", "not", "submitted"],
            &["has", "not", "yet", "submitted"],
            &["hasn", "t", "submitted"],
            &["hasn", "t", "yet", "submitted"],
            &["had", "not", "submitted"],
            &["had", "not", "yet", "submitted"],
            &["hadn", "t", "submitted"],
            &["hadn", "t", "yet", "submitted"],
            &["did", "not", "submit"],
            &["did", "not", "yet", "submit"],
            &["didn", "t", "submit"],
            &["didn", "t", "yet", "submit"],
        ]
        .iter()
        .any(|phrase| followed_by_application(phrase, &["an", "the", "your"]))
        || [
            &["was", "not"][..],
            &["wasn", "t"],
            &["was", "not", "yet"],
            &["wasn", "t", "yet"],
            &["has", "not", "been"],
            &["hasn", "t", "been"],
            &["has", "not", "yet", "been"],
            &["hasn", "t", "yet", "been"],
            &["had", "not", "been"],
            &["hadn", "t", "been"],
            &["had", "not", "yet", "been"],
            &["hadn", "t", "yet", "been"],
            &["is", "not"],
            &["isn", "t"],
            &["is", "not", "yet"],
            &["isn", "t", "yet"],
        ]
        .iter()
        .any(|phrase| followed_by_optional_yet_successful_outcome(phrase))
        || contains_phrase(&["not", "submitted"])
        || contains_phrase(&["not", "yet", "submitted"])
        || contains_phrase(&["not", "successfully", "submitted"])
        || contains_phrase(&["not", "yet", "successfully", "submitted"])
        || [
            &["did", "not", "receive"][..],
            &["didn", "t", "receive"],
            &["have", "not", "received"],
            &["haven", "t", "received"],
        ]
        .iter()
        .any(|phrase| followed_by_application(phrase, &["the", "your"]))
        || contains_phrase(&["application", "was", "not", "received"])
        || contains_phrase(&["application", "wasn", "t", "received"])
        || contains_phrase(&["application", "has", "not", "been", "received"])
        || contains_phrase(&["application", "hasn", "t", "been", "received"])
        || contains_phrase(&["application", "is", "not", "received"])
        || contains_phrase(&["application", "isn", "t", "received"])
        || contains_phrase(&["submission", "failed"])
        || contains_phrase(&["submission", "was", "unsuccessful"])
        || contains_phrase(&["submission", "was", "not", "successful"]);
    if negative_application_outcome {
        return false;
    }
    contains_phrase(&["thank", "you", "for", "applying"])
        || contains_phrase(&["thanks", "for", "applying"])
        || contains_phrase(&["received", "your", "application"])
        || (words.contains(&"application")
            && (words.contains(&"submitted") || words.contains(&"received")))
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
    uploads: Vec<ApplicationObjectBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredEvidenceObject {
    kind: String,
    storage_key: String,
    sha256: String,
    media_type: String,
    size_bytes: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredReceiptObject {
    storage_key: String,
    sha256: String,
    media_type: &'static str,
    size_bytes: usize,
    schema_version: i64,
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
    bytes.starts_with(b"%PDF-")
        && lopdf::Document::load_mem(bytes)
            .ok()
            .is_some_and(|document| !document.get_pages().is_empty())
}

fn valid_png(bytes: &[u8]) -> bool {
    const MAX_DECODED_SCREENSHOT_BYTES: usize = 64 * 1024 * 1024;
    let decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(bytes),
        png::Limits {
            bytes: MAX_DECODED_SCREENSHOT_BYTES,
        },
    );
    let Ok(mut reader) = decoder.read_info() else {
        return false;
    };
    if reader.info().width == 0 || reader.info().height == 0 {
        return false;
    }
    let Some(buffer_size) = reader.output_buffer_size() else {
        return false;
    };
    if buffer_size == 0 || buffer_size > MAX_DECODED_SCREENSHOT_BYTES {
        return false;
    }
    let mut decoded = vec![0; buffer_size];
    reader.next_frame(&mut decoded).is_ok()
}

#[allow(clippy::too_many_arguments)]
async fn upload_receipt_evidence(
    pool: &crate::db::DbPool,
    storage: &ObjectStorage,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    request_fingerprint: &str,
    receipt: &mut Value,
    evidence_objects: Vec<PreparedReceiptEvidence>,
) -> Result<UploadedReceiptEvidence, ApiError> {
    let mut verified = BTreeMap::new();
    let mut uploads = Vec::new();
    let mut manifest = Vec::new();
    // The fingerprint-stable rows may be shared by an exact concurrent replay.
    // A losing request must never delete them. Failed sets remain pending for
    // exact retry and are reclaimed only after their protected capacity ends.
    for (index, object) in evidence_objects.into_iter().enumerate() {
        let artifact_id = format!(
            "jobs/{application_id}/receipts/{request_fingerprint}/{index}-{}-{}",
            safe_file_part(&object.kind),
            &object.sha256[..20]
        );
        let storage_key = storage.artifact_key(account_id, &artifact_id);
        let (binding, needs_put) = match reserve_submission_object(
            pool,
            storage,
            account_id,
            application_id,
            run_id,
            runner,
            &artifact_id,
            &storage_key,
            object.bytes.len(),
            &object.sha256,
            object.media_type,
            &format!("Submission {}", object.kind.replace('_', " ")),
            &object.kind,
        ) {
            Ok(reservation) => reservation,
            Err(error) => return Err(error),
        };
        let durable_key = binding.object_key.clone();
        uploads.push(binding.clone());
        let bytes = bytes::Bytes::from(object.bytes);
        put_and_verify_submission_object(
            pool,
            storage,
            &binding,
            bytes,
            object.media_type,
            needs_put,
        )
        .await?;
        replace_receipt_storage_key(receipt, &object.original_key, &durable_key);
        verified.insert(durable_key.clone(), object.sha256.clone());
        manifest.push(StoredEvidenceObject {
            kind: object.kind,
            storage_key: durable_key,
            sha256: object.sha256,
            media_type: object.media_type.to_string(),
            size_bytes: binding.size_bytes,
        });
    }
    manifest.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
    receipt
        .as_object_mut()
        .expect("validated receipt fields require an object")
        .insert(
            "evidenceObjects".to_string(),
            serde_json::to_value(manifest).map_err(|error| internal(error.into()))?,
        );
    if let Some(result) = receipt.get_mut("result").and_then(Value::as_object_mut) {
        result.remove("screenshotPath");
    }
    Ok(UploadedReceiptEvidence { verified, uploads })
}

#[allow(clippy::too_many_arguments)]
async fn upload_immutable_receipt_bundle(
    pool: &crate::db::DbPool,
    storage: &ObjectStorage,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    request_fingerprint: &str,
    receipt_id: &str,
    approved_job: &Value,
    resume: &ResumeVersion,
    receipt: &mut Value,
    uploaded: &mut UploadedReceiptEvidence,
) -> Result<(), ApiError> {
    let receipt_schema_version = receipt
        .get("schemaVersion")
        .and_then(Value::as_i64)
        .filter(|version| matches!(*version, 1 | 2))
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Application receipt schema is invalid.".to_string(),
        ))?;
    let bundle_id = request_fingerprint.to_string();
    let encoded = serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "bundleId": bundle_id,
        "accountId": account_id,
        "applicationId": application_id,
        "receiptId": receipt_id,
        "job": approved_job,
        "resume": resume,
        "receipt": receipt,
    }))
    .map_err(|error| internal(error.into()))?;
    if encoded.len() > MAX_RECEIPT_BUNDLE_BYTES || encoded.len() > storage.max_object_bytes() {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Application receipt bundle is too large.".to_string(),
        ));
    }
    let sha256 = sha256_hex(&encoded);
    let storage_key = storage.jobs_submission_bundle_key(
        account_id,
        application_id,
        receipt_id,
        &bundle_id,
        &sha256,
    );
    let logical_id = format!(
        "jobs-submission-bundle:{}",
        sha256_hex(format!("{application_id}\0{receipt_id}\0{bundle_id}"))
    );
    let (binding, needs_put) = reserve_submission_object(
        pool,
        storage,
        account_id,
        application_id,
        run_id,
        runner,
        &logical_id,
        &storage_key,
        encoded.len(),
        &sha256,
        "application/json",
        "Application receipt bundle",
        "application_receipt",
    )?;
    let durable_key = binding.object_key.clone();
    uploaded.uploads.push(binding.clone());
    put_and_verify_submission_object(
        pool,
        storage,
        &binding,
        bytes::Bytes::from(encoded.clone()),
        "application/json",
        needs_put,
    )
    .await?;
    uploaded
        .verified
        .insert(durable_key.clone(), sha256.clone());
    receipt
        .as_object_mut()
        .expect("validated receipt fields require an object")
        .insert(
            "receiptObject".to_string(),
            serde_json::to_value(StoredReceiptObject {
                storage_key: durable_key,
                sha256,
                media_type: "application/json",
                size_bytes: encoded.len(),
                schema_version: receipt_schema_version,
            })
            .map_err(|error| internal(error.into()))?,
        );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn reserve_submission_object(
    pool: &crate::db::DbPool,
    storage: &ObjectStorage,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    logical_id: &str,
    storage_key: &str,
    size_bytes: usize,
    sha256: &str,
    content_type: &str,
    title: &str,
    evidence_kind: &str,
) -> Result<(ApplicationObjectBinding, bool), ApiError> {
    let size_bytes = i64::try_from(size_bytes).map_err(|_| {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Application evidence is too large.".to_string(),
        )
    })?;
    let reservation = object_uploads::reserve_application_object_upload(
        pool,
        application_id,
        &NewObjectUpload {
            account_id: account_id.to_string(),
            object_kind: ObjectKind::Artifact,
            logical_id: logical_id.to_string(),
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: storage_key.to_string(),
            size_bytes,
            sha256: sha256.to_string(),
            content_type: content_type.to_string(),
            expires_at_ms: SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRY_MS,
            metadata_json: json!({
                "artifact_class": "jobs_submission_evidence",
                "jobs_application_id": application_id,
                "jobs_run_id": run_id,
                "jobs_runner": runner,
                "evidence_kind": evidence_kind,
                "title": title,
                "retention_policy": "account_lifetime_until_deletion",
            }),
            now_ms: jobs::now_ms(),
            limits: storage.upload_limits(),
        },
    )
    .map_err(evidence_upload_control_error)?;
    Ok((
        ApplicationObjectBinding {
            upload_id: reservation.upload.id,
            object_key: reservation.upload.object_key,
            size_bytes: reservation.upload.size_bytes,
            sha256: reservation.upload.sha256,
            content_type: reservation.upload.content_type,
        },
        reservation.needs_put,
    ))
}

async fn put_and_verify_submission_object(
    pool: &crate::db::DbPool,
    storage: &ObjectStorage,
    binding: &ApplicationObjectBinding,
    bytes: bytes::Bytes,
    content_type: &str,
    needs_put: bool,
) -> Result<(), ApiError> {
    let size_bytes = i64::try_from(bytes.len()).map_err(|_| {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Application evidence is too large.".to_string(),
        )
    })?;
    if binding.size_bytes != size_bytes || binding.content_type != content_type {
        return bad_request("Application evidence does not match its durable reservation.");
    }
    if needs_put {
        object_uploads::begin_upload_put(pool, &binding.upload_id, jobs::now_ms())
            .map_err(evidence_upload_control_error)?;
        if let Err(error) = storage
            .put(&binding.object_key, bytes.clone(), content_type)
            .await
        {
            let _ = object_uploads::record_put_failure(
                pool,
                &binding.upload_id,
                &error.to_string(),
                jobs::now_ms(),
            );
            return Err(evidence_storage_error(error));
        }
    }
    let stored = match storage.get(&binding.object_key).await {
        Ok(stored) => stored,
        Err(error) => {
            let _ = object_uploads::record_put_failure(
                pool,
                &binding.upload_id,
                &error.to_string(),
                jobs::now_ms(),
            );
            return Err(evidence_storage_error(error));
        }
    };
    if stored.bytes != bytes
        || sha256_hex(&stored.bytes) != binding.sha256
        || !stored
            .content_type
            .split(';')
            .next()
            .is_some_and(|stored_type| stored_type.eq_ignore_ascii_case(content_type))
    {
        let _ = object_uploads::record_put_failure(
            pool,
            &binding.upload_id,
            "object read-back verification failed",
            jobs::now_ms(),
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            "Stored application evidence failed verification.".to_string(),
        ));
    }
    if needs_put {
        object_uploads::release_verified_upload_put(pool, &binding.upload_id, jobs::now_ms())
            .map_err(evidence_upload_control_error)?;
    }
    Ok(())
}

fn evidence_upload_control_error(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<UploadControlError>() {
        Some(UploadControlError::ObjectTooLarge) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Application evidence is too large.".to_string(),
        ),
        Some(
            UploadControlError::AccountBytesQuotaExceeded
            | UploadControlError::AccountObjectQuotaExceeded,
        ) => (
            StatusCode::INSUFFICIENT_STORAGE,
            "Application evidence storage quota is unavailable.".to_string(),
        ),
        Some(UploadControlError::DailyQuotaExceeded) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Application evidence upload capacity is temporarily unavailable.".to_string(),
        ),
        Some(UploadControlError::AccountDeleting) => (
            StatusCode::CONFLICT,
            "Account deletion has already fenced new application evidence.".to_string(),
        ),
        Some(UploadControlError::SubmissionEvidenceCapacityUnavailable) => (
            StatusCode::CONFLICT,
            "The protected submission evidence reservation is no longer active.".to_string(),
        ),
        Some(UploadControlError::SubmissionEvidenceCapacityExceeded) => (
            StatusCode::INSUFFICIENT_STORAGE,
            "Application evidence exceeded its protected reservation.".to_string(),
        ),
        _ => evidence_storage_error(error),
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
    approved_job: &Value,
    resume: &ResumeVersion,
    receipt: &Value,
    verified_objects: &BTreeMap<String, String>,
    submit_http_status: i64,
    confirmation: String,
    confirmation_url: Option<String>,
    confirmation_text: Option<String>,
    submitted_at: Option<Value>,
) -> Result<Vec<ApplicationEvidence>, ApiError> {
    let approved_company = approved_job
        .get("company")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or((
            StatusCode::CONFLICT,
            "The approved job company is missing.".to_string(),
        ))?;
    let approved_title = approved_job
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or((
            StatusCode::CONFLICT,
            "The approved job title is missing.".to_string(),
        ))?;
    let evidence_manifest = validate_stored_evidence_manifest(receipt, verified_objects)?;
    let documents = receipt.get("documents").and_then(Value::as_array).ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt is missing uploaded application documents.".to_string(),
    ))?;
    let screenshot_keys = receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .filter(|screenshots| {
            !screenshots.is_empty() && screenshots.len() <= MAX_RECEIPT_SCREENSHOTS
        })
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt has an invalid number of confirmation screenshots.".to_string(),
        ))?;
    let screenshot_count = screenshot_keys.len();
    let mut evidence = Vec::with_capacity(documents.len() + screenshot_count + 1);
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
        let size_bytes = evidence_manifest
            .get(storage_key)
            .map(|object| object.size_bytes)
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt document is missing from its evidence manifest.".to_string(),
            ))?;
        let (label, file_name) = match kind {
            "resume" => (
                "Resume submitted".to_string(),
                format!(
                    "{}-{}-resume.pdf",
                    safe_file_part(approved_company),
                    safe_file_part(approved_title)
                ),
            ),
            "cover_letter" => (
                "Cover letter submitted".to_string(),
                format!(
                    "{}-{}-cover-letter.pdf",
                    safe_file_part(approved_company),
                    safe_file_part(approved_title)
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
                "size_bytes": size_bytes,
            }),
            created_at_ms: 0,
        });
    }
    let receipt_object = receipt
        .get("receiptObject")
        .and_then(Value::as_object)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt is missing its immutable receipt object.".to_string(),
        ))?;
    let receipt_storage_key = receipt_object
        .get("storageKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let receipt_sha256 = receipt_object
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let verified_receipt_sha256 = verified_objects
        .get(receipt_storage_key)
        .filter(|verified| verified.as_str() == receipt_sha256)
        .cloned()
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt bundle was not uploaded and verified.".to_string(),
        ))?;
    evidence.push(ApplicationEvidence {
        id: String::new(),
        application_id: application_id.to_string(),
        kind: "application_receipt".to_string(),
        label: "Application receipt bundle".to_string(),
        provider: provider.to_string(),
        file_name: format!("{}.json", safe_file_part(receipt_id)),
        media_type: "application/json".to_string(),
        storage_key: receipt_storage_key.to_string(),
        sha256: verified_receipt_sha256,
        resume_version_id: Some(resume.id.clone()),
        occurred_at_ms: 0,
        metadata: json!({
            "immutable": true,
            "receipt_id": receipt_id,
            "schema_version": receipt_object.get("schemaVersion"),
            "size_bytes": receipt_object.get("sizeBytes"),
            "runner": receipt.get("runner"),
            "run_id": receipt.get("runId"),
            "submit_http_status": submit_http_status,
        }),
        created_at_ms: 0,
    });
    for (index, screenshot) in screenshot_keys.iter().enumerate() {
        let screenshot_key = screenshot.as_str().unwrap_or_default();
        let screenshot_sha256 = verified_objects.get(screenshot_key).cloned().ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt confirmation screenshot was not uploaded and verified.".to_string(),
        ))?;
        let screenshot_size_bytes = evidence_manifest
            .get(screenshot_key)
            .map(|object| object.size_bytes)
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt confirmation is missing from its evidence manifest.".to_string(),
            ))?;
        evidence.push(ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: "submission_confirmation".to_string(),
            label: confirmation.clone(),
            provider: provider.to_string(),
            file_name: format!(
                "submission-confirmation-{}-of-{screenshot_count}.png",
                index + 1
            ),
            media_type: "image/png".to_string(),
            storage_key: screenshot_key.to_string(),
            sha256: screenshot_sha256,
            resume_version_id: Some(resume.id.clone()),
            occurred_at_ms: 0,
            metadata: json!({
                "immutable": true,
                "confirmation": confirmation,
                "confirmation_url": confirmation_url,
                "confirmation_text": confirmation_text,
                "submit_http_status": submit_http_status,
                "submitted_at": submitted_at,
                "screenshot_keys": receipt.get("screenshotKeys"),
                "screenshot_index": index + 1,
                "screenshot_count": screenshot_count,
                "evidence_strength": "browser_confirmed",
                "receipt_id": receipt_id,
                "size_bytes": screenshot_size_bytes,
            }),
            created_at_ms: 0,
        });
    }
    Ok(evidence)
}

fn validate_stored_evidence_manifest(
    receipt: &Value,
    verified_objects: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, StoredEvidenceObject>, ApiError> {
    let manifest = receipt.get("evidenceObjects").cloned().ok_or((
        StatusCode::BAD_REQUEST,
        "Receipt is missing its immutable evidence manifest.".to_string(),
    ))?;
    let manifest = serde_json::from_value::<Vec<StoredEvidenceObject>>(manifest).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Receipt evidence manifest is invalid.".to_string(),
        )
    })?;
    if manifest.is_empty() || manifest.len() > MAX_RECEIPT_EVIDENCE_OBJECTS {
        return bad_request("Receipt evidence manifest is invalid.");
    }

    let mut expected = BTreeMap::<String, (String, Option<String>, &'static str)>::new();
    for document in receipt
        .get("documents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = document
            .get("storageKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = document
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let sha256 = document
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if key.is_empty()
            || !matches!(kind, "resume" | "cover_letter" | "attachment")
            || !valid_sha256(sha256)
            || expected
                .insert(
                    key.to_string(),
                    (
                        kind.to_string(),
                        Some(sha256.to_ascii_lowercase()),
                        "application/pdf",
                    ),
                )
                .is_some()
        {
            return bad_request("Receipt evidence manifest does not match its documents.");
        }
    }
    for screenshot in receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = screenshot.as_str().unwrap_or_default();
        if key.is_empty()
            || expected
                .insert(
                    key.to_string(),
                    ("screenshot".to_string(), None, "image/png"),
                )
                .is_some()
        {
            return bad_request("Receipt evidence manifest does not match its screenshots.");
        }
    }
    if manifest.len() != expected.len() {
        return bad_request("Receipt evidence manifest is incomplete.");
    }

    let mut objects = BTreeMap::new();
    let mut previous_key: Option<&str> = None;
    for object in &manifest {
        let Some((expected_kind, expected_sha256, expected_media_type)) =
            expected.get(&object.storage_key)
        else {
            return bad_request("Receipt evidence manifest contains an unreferenced object.");
        };
        if previous_key.is_some_and(|previous| previous >= object.storage_key.as_str())
            || object.kind != *expected_kind
            || object.media_type != *expected_media_type
            || object.size_bytes <= 0
            || !valid_sha256(&object.sha256)
            || expected_sha256
                .as_deref()
                .is_some_and(|expected| !expected.eq_ignore_ascii_case(&object.sha256))
            || verified_objects
                .get(&object.storage_key)
                .is_none_or(|verified| !verified.eq_ignore_ascii_case(&object.sha256))
        {
            return bad_request("Receipt evidence manifest failed exact verification.");
        }
        previous_key = Some(&object.storage_key);
        objects.insert(object.storage_key.clone(), object.clone());
    }
    Ok(objects)
}

fn validate_receipt_bundle(
    account_id: &str,
    application: &JobApplication,
    resume: &ResumeVersion,
    receipt: &Value,
    verified_objects: &BTreeMap<String, String>,
    require_receipt_object: bool,
) -> Result<(), ApiError> {
    let receipt_schema_version = receipt
        .get("schemaVersion")
        .and_then(Value::as_i64)
        .filter(|version| matches!(*version, 1 | 2))
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Application receipt schema is invalid.".to_string(),
        ))?;
    let (approved_packet, approved_job, approved_checksum) =
        approved_execution_snapshot(application)?;
    validate_frozen_approved_execution_matches(
        application,
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
    if require_receipt_object {
        let authority = receipt
            .get(jobs::SERVER_SUBMISSION_AUTHORITY_KEY)
            .and_then(Value::as_object)
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt is missing its server submission authority.".to_string(),
            ))?;
        if authority.get("schemaVersion").and_then(Value::as_i64) != Some(1)
            || authority.get("preSubmissionReceipt") != Some(&application.receipt)
        {
            return bad_request("Receipt submission authority does not match the approved packet.");
        }
        validate_stored_evidence_manifest(receipt, verified_objects)?;
    }
    let receipt_object = receipt.get("receiptObject");
    if require_receipt_object && receipt_object.is_none() {
        return bad_request("Receipt is missing its immutable receipt object.");
    }
    if let Some(receipt_object) = receipt_object {
        let receipt_object = receipt_object.as_object().ok_or((
            StatusCode::BAD_REQUEST,
            "Receipt bundle reference is invalid.".to_string(),
        ))?;
        let storage_key = receipt_object
            .get("storageKey")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt bundle reference is invalid.".to_string(),
            ))?;
        let sha256 = receipt_object
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|value| valid_sha256(value))
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Receipt bundle checksum is invalid.".to_string(),
            ))?;
        if receipt_object.get("mediaType").and_then(Value::as_str) != Some("application/json")
            || receipt_object.get("schemaVersion").and_then(Value::as_i64)
                != Some(receipt_schema_version)
            || receipt_object
                .get("sizeBytes")
                .and_then(Value::as_u64)
                .is_none_or(|size| size == 0)
            || verified_objects.get(storage_key).map(String::as_str) != Some(sha256)
        {
            return bad_request("Receipt bundle was not uploaded and verified.");
        }
    }
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
        || packet.get("verifiedClaimIds") != approved_packet.get("verifiedClaimIds")
    {
        return bad_request(
            "Receipt answers do not match the exact application packet that was approved.",
        );
    }
    let receipt_admission = packet.get("approvedExecutionAdmission");
    let receipt_admission_schema = packet
        .get("approvedExecutionSchemaVersion")
        .and_then(Value::as_i64);
    if receipt_schema_version == 2 {
        let frozen_admission = application
            .receipt
            .pointer("/approved_execution/admission")
            .ok_or((
                StatusCode::CONFLICT,
                "The certified frozen admission is missing.".to_string(),
            ))?;
        if !jobs::application_has_frozen_ats_certification(application)
            || receipt_admission_schema != Some(3)
            || receipt_admission != Some(frozen_admission)
        {
            return bad_request(
                "Receipt certification admission does not match the frozen execution.",
            );
        }
    } else if receipt_admission_schema.is_some() || receipt_admission.is_some() {
        return bad_request("Review submission receipt contains certified admission fields.");
    }
    if receipt.get("job") != Some(&approved_job) {
        return bad_request("Receipt job snapshot does not match the exact approved posting.");
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
        "side_effect_unknown" => "needs_input",
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
            "needs_input" if status == "side_effect_unknown" => {
                "Submission outcome needs reconciliation"
            }
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
    if let Some(target) =
        parse_provider_application_target(raw_url, ProviderApplicationTargetPurpose::Submit)
    {
        return target.provider;
    }
    let host = reqwest::Url::parse(raw_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    if host.ends_with(".myworkdayjobs.com") {
        "workday"
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
        || message.contains("already been resolved")
        || message.contains("no longer waiting for an answer")
        || message.contains("awaiting reconciliation")
        || message.contains("execution authority changed")
        || message.contains("application packet revision history is invalid")
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
        || message.contains("execution lease cannot accept this receipt")
        || message.contains("execution lease cannot accept receipt")
        || message.contains("execution lease changed")
        || message.contains("invalid application state transition")
        || message.contains("local run ticket is not active")
        || message.contains("awaiting reconciliation")
        || message.contains("execution authority changed")
        || message.contains("attempt reservation changed")
        || message.contains("browser session changed")
        || message.contains("submission outcome changed")
        || message.contains("does not match this application")
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
        jobs::ExecutionLeaseError::Storage(error) => {
            if error.downcast_ref::<UploadControlError>().is_some() {
                evidence_upload_control_error(error)
            } else {
                internal(error)
            }
        }
    }
}

fn submission_receipt_authority_error(error: jobs::ExecutionLeaseError) -> ApiError {
    match error {
        jobs::ExecutionLeaseError::NotFound | jobs::ExecutionLeaseError::Conflict => (
            StatusCode::CONFLICT,
            "A matching cloud execution lease cannot accept this receipt.".to_string(),
        ),
        other => execution_lease_error(other),
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

    #[test]
    fn execution_lease_success_responses_are_private_and_non_sniffable() {
        let response = worker_execution_lease_json_response(json!({"lease_token": "secret"}));
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("private, no-store"))
        );
        assert_eq!(
            response.headers().get("x-content-type-options"),
            Some(&HeaderValue::from_static("nosniff"))
        );
    }

    #[test]
    fn managed_execution_lease_requires_the_canonical_workflow_request_id() {
        assert!(valid_managed_workflow_request_id(
            "wfreq-v2-01234567-89ab-5cde-8f01-23456789abcd"
        ));
        for invalid in [
            "wfreq-v2-01234567-89ab-4cde-8f01-23456789abcd",
            "wfreq-v2-01234567-89AB-5CDE-8F01-23456789ABCD",
            "wfreq-v2-01234567-89ab-5cde-7f01-23456789abcd",
            "wfreq-v2-01234567-89ab-5cde-8f01-23456789abcd\nextra",
        ] {
            assert!(!valid_managed_workflow_request_id(invalid));
        }
    }

    #[test]
    fn workflow_finalization_accepts_only_closed_state_reason_prompt_pairs() {
        use jobs::{JobsWorkflowTerminalOutcome as Outcome, JobsWorkflowTerminalReason as Reason};

        assert_eq!(
            workflow_command_terminal_outcome(
                WorkflowCommandTerminalState::Failed,
                WorkflowCommandTerminalReason::RunnerFailed,
                None,
            ),
            Some(Outcome::Failed(Reason::RunnerFailed))
        );
        assert_eq!(
            workflow_command_terminal_outcome(
                WorkflowCommandTerminalState::SideEffectUnknown,
                WorkflowCommandTerminalReason::RunnerAmbiguous,
                None,
            ),
            Some(Outcome::SideEffectUnknown(Reason::RunnerAmbiguous))
        );
        assert_eq!(
            workflow_command_terminal_outcome(
                WorkflowCommandTerminalState::Failed,
                WorkflowCommandTerminalReason::InterventionTimeout,
                Some("wfint-v2-1234567890"),
            ),
            Some(Outcome::Failed(Reason::InterventionTimeout))
        );
        assert_eq!(
            workflow_command_terminal_outcome(
                WorkflowCommandTerminalState::Failed,
                WorkflowCommandTerminalReason::InterventionLimit,
                None,
            ),
            Some(Outcome::Failed(Reason::InterventionLimit))
        );
        assert!(workflow_command_terminal_outcome(
            WorkflowCommandTerminalState::Failed,
            WorkflowCommandTerminalReason::InterventionLimit,
            Some("wfint-v2-1234567890"),
        )
        .is_none());
        assert!(workflow_command_terminal_outcome(
            WorkflowCommandTerminalState::Failed,
            WorkflowCommandTerminalReason::InterventionTimeout,
            None,
        )
        .is_none());
    }

    #[test]
    fn workflow_finalization_rejects_explicit_null_optional_authority() {
        let request = json!({
            "schema_version": 2,
            "workflow_id": "bluey-jobs-v2-1234567890",
            "payload_digest": "a".repeat(64),
            "operation": "start",
            "terminal_state": "failed",
            "reason_code": "runner_failed",
            "open_intervention_id": null,
        });
        assert!(
            serde_json::from_value::<WorkflowCommandFinalizeRequest>(request).is_err(),
            "optional authority must be omitted rather than supplied as null"
        );
    }

    #[test]
    fn workflow_command_identifiers_require_twenty_closed_characters() {
        assert!(!workflow_command_opaque_identifier(
            "wfreq-v2-123456789",
            128
        ));
        assert!(workflow_command_opaque_identifier(
            "wfreq-v2-12345678901",
            128
        ));
        assert!(!workflow_command_opaque_identifier(
            "wfreq-v2-12345678901.",
            128
        ));
    }

    #[test]
    fn expired_v2_submit_capability_is_limited_to_click_started_replay() {
        let expires_at_ms = 10_000;
        let now_ms = expires_at_ms + 1;
        assert!(local_run_operation_expiry_allowed(
            "submit",
            None,
            Some(super::super::jobs_local_capability::TOKEN_VERSION),
            "click_started",
            expires_at_ms,
            now_ms,
        ));
        assert!(!local_run_operation_expiry_allowed(
            "submit",
            None,
            Some(super::super::jobs_local_capability::LEGACY_TOKEN_VERSION),
            "click_started",
            expires_at_ms,
            now_ms,
        ));
        assert!(!local_run_operation_expiry_allowed(
            "submit",
            None,
            Some(super::super::jobs_local_capability::TOKEN_VERSION),
            "click_started",
            expires_at_ms,
            expires_at_ms
                .saturating_add(super::super::jobs_local_capability::RECONCILIATION_GRACE_MS),
        ));
    }

    #[test]
    fn expired_claimed_submit_capability_remains_denied() {
        assert!(!local_run_operation_expiry_allowed(
            "submit",
            None,
            Some(super::super::jobs_local_capability::TOKEN_VERSION),
            "claimed",
            10_000,
            10_001,
        ));
    }

    #[test]
    fn legacy_result_reconciliation_expiry_contract_is_preserved() {
        let expires_at_ms = 10_000;
        let within_grace_ms = expires_at_ms + 1;
        let legacy_v1 = Some(super::super::jobs_local_capability::LEGACY_TOKEN_VERSION);

        assert!(!local_run_operation_expiry_allowed(
            "result",
            Some("submitted"),
            None,
            "click_started",
            expires_at_ms,
            within_grace_ms,
        ));
        assert!(local_run_operation_expiry_allowed(
            "result",
            Some("submitted"),
            legacy_v1,
            "click_started",
            expires_at_ms,
            within_grace_ms,
        ));
        for terminal_status in ["side_effect_unknown", "complete"] {
            assert!(local_run_operation_expiry_allowed(
                "result",
                Some("side_effect_unknown"),
                None,
                terminal_status,
                expires_at_ms,
                within_grace_ms,
            ));
            assert!(local_run_operation_expiry_allowed(
                "result",
                Some("submitted"),
                legacy_v1,
                terminal_status,
                expires_at_ms,
                within_grace_ms,
            ));
        }
        assert!(!local_run_operation_expiry_allowed(
            "result",
            Some("failed"),
            legacy_v1,
            "click_started",
            expires_at_ms,
            within_grace_ms,
        ));
        assert!(!local_run_operation_expiry_allowed(
            "result",
            Some("submitted"),
            legacy_v1,
            "claimed",
            expires_at_ms,
            within_grace_ms,
        ));
        assert!(!local_run_operation_expiry_allowed(
            "result",
            Some("submitted"),
            legacy_v1,
            "click_started",
            expires_at_ms,
            expires_at_ms
                .saturating_add(super::super::jobs_local_capability::RECONCILIATION_GRACE_MS),
        ));
    }

    #[test]
    fn disabled_distribution_denies_claimed_submit_but_allows_click_started_replay() {
        assert!(!local_submit_distribution_allowed(false, false));
        assert!(local_submit_distribution_allowed(true, false));
        assert!(local_submit_distribution_allowed(false, true));
    }

    fn durable_local_submission_capacity() -> object_uploads::SubmissionEvidenceCapacity {
        object_uploads::SubmissionEvidenceCapacity {
            account_id: "account-1".to_string(),
            application_id: "application-1".to_string(),
            run_id: "run-1".to_string(),
            runner: "local".to_string(),
            reserved_bytes: 384,
            reserved_objects: 3,
            consumed_bytes: 0,
            consumed_objects: 0,
            state: "active".to_string(),
            expires_at_ms: 20_000,
            created_at_ms: 9_000,
            updated_at_ms: 9_000,
            completed_at_ms: None,
        }
    }

    fn reduced_upload_limits() -> crate::object_storage::UploadLimits {
        crate::object_storage::UploadLimits {
            max_object_bytes: 64,
            max_account_bytes: 128,
            max_daily_bytes: 128,
            max_account_objects: 2,
        }
    }

    #[test]
    fn click_started_replay_preserves_durable_capacity_across_config_drift() {
        let capacity = local_durable_recovery_evidence_capacity(
            Some(durable_local_submission_capacity()),
            "account-1",
            "application-1",
            "run-1",
            10_000,
            reduced_upload_limits(),
        )
        .expect("active exact capacity");

        assert_eq!(capacity.reserved_bytes, 384);
        assert_eq!(capacity.reserved_objects, 3);
        assert_eq!(capacity.expires_at_ms, 20_000);
        assert_eq!(capacity.now_ms, 10_000);
        assert_eq!(capacity.limits, reduced_upload_limits());
    }

    #[test]
    fn result_reconciliation_uses_durable_capacity_only_after_the_click_boundary() {
        for durable_status in ["click_started", "side_effect_unknown"] {
            assert!(local_result_uses_durable_submission_evidence_capacity(
                durable_status
            ));
            let capacity = local_durable_recovery_evidence_capacity(
                Some(durable_local_submission_capacity()),
                "account-1",
                "application-1",
                "run-1",
                10_000,
                reduced_upload_limits(),
            )
            .expect("recovery keeps the durable capacity identity despite current config drift");
            assert_eq!(capacity.reserved_bytes, 384);
            assert_eq!(capacity.reserved_objects, 3);
        }
        for new_reservation_status in ["claimed", "needs_input"] {
            assert!(!local_result_uses_durable_submission_evidence_capacity(
                new_reservation_status
            ));
        }
    }

    #[test]
    fn click_started_replay_rejects_missing_inactive_expired_or_wrong_run_capacity() {
        let limits = reduced_upload_limits();
        let rebuild = |durable| {
            local_durable_recovery_evidence_capacity(
                durable,
                "account-1",
                "application-1",
                "run-1",
                10_000,
                limits,
            )
        };
        assert!(rebuild(None).is_none());

        let mut inactive = durable_local_submission_capacity();
        inactive.state = "released".to_string();
        assert!(rebuild(Some(inactive)).is_none());

        let mut expired = durable_local_submission_capacity();
        expired.expires_at_ms = 10_000;
        assert!(rebuild(Some(expired)).is_none());

        let mut wrong_runner = durable_local_submission_capacity();
        wrong_runner.runner = "cloud".to_string();
        assert!(rebuild(Some(wrong_runner)).is_none());

        let mut wrong_run = durable_local_submission_capacity();
        wrong_run.run_id = "run-2".to_string();
        assert!(rebuild(Some(wrong_run)).is_none());
    }

    #[test]
    fn local_submit_distribution_gate_defers_only_durable_replay_to_database_authority() {
        let source = include_str!("jobs.rs");
        let submit = source
            .split("async fn authorize_local_run_submit")
            .nth(1)
            .expect("local submit route")
            .split("fn local_submission_evidence_capacity")
            .next()
            .expect("bounded local submit route");
        let ticket = submit
            .find("authorize_local_run_operation")
            .expect("ticket capability authority");
        let durable_replay = submit
            .find("local_submit_distribution_allowed")
            .expect("durable replay distribution exception");
        let distribution = submit
            .find("jobs_local_browser_distribution_enabled")
            .expect("local distribution flag");
        let database = submit
            .find("local_run_submit_authorization_for_distribution")
            .expect("atomic database submit authority");
        assert!(
            ticket < durable_replay && durable_replay < distribution && distribution < database
        );
    }

    #[test]
    fn ats_kind_matches_shared_exact_target_vectors() {
        let vectors: Value = serde_json::from_str(include_str!(
            "../../../jobs/automation/tests/fixtures/ats-target-vectors.json"
        ))
        .unwrap();
        for vector in vectors.as_array().unwrap() {
            let name = vector["name"].as_str().unwrap();
            let url = vector["url"].as_str().unwrap();
            let expected = vector["expectedDetection"].as_str().unwrap();
            assert_eq!(ats_kind(url), expected, "{name}");
        }
    }

    #[test]
    fn ats_kind_rejects_workday_lookalikes() {
        assert_eq!(
            ats_kind("https://myworkdayjobs.com/acme/job/123"),
            "semantic"
        );
        assert_eq!(
            ats_kind("https://myworkdayjobs.com.attacker.example/acme/job/123"),
            "semantic"
        );
    }

    #[test]
    fn execution_lease_owner_is_bound_to_authenticated_worker() {
        let worker = JobsWorkerIdentity {
            worker_id: "signed-execution-worker".to_string(),
            scope: "execution".to_string(),
        };
        assert_eq!(
            authenticated_execution_lease_owner(&worker, "signed-execution-worker").unwrap(),
            "signed-execution-worker"
        );
        let error = authenticated_execution_lease_owner(&worker, "forged-owner").unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);

        #[cfg(debug_assertions)]
        {
            let debug_worker = JobsWorkerIdentity {
                worker_id: "debug-legacy-worker".to_string(),
                scope: "debug".to_string(),
            };
            assert_eq!(
                authenticated_execution_lease_owner(&debug_worker, "legacy-body-owner").unwrap(),
                "legacy-body-owner"
            );
        }
    }

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

    fn ready_runner_volume_fleet() -> jobs::RunnerVolumeFleetStatus {
        jobs::RunnerVolumeFleetStatus {
            enrollment_generation: 4,
            purge_generation: 7,
            tombstone_generation: 6,
            destruction_generation: 2,
            legacy_reconciliation_generation: 3,
            storage_attestation_generation: 8,
            storage_attestation_count: 2,
            storage_attestation_set_sha256: "c".repeat(64),
            legacy_inventory_state: "ready".to_string(),
            legacy_inventory_generation: 5,
            legacy_inventory_reconciliation_id: Some("legacy-reconciliation".to_string()),
            legacy_inventory_authority_id: Some("legacy-authority".to_string()),
            legacy_inventory_authority_sha256: Some("b".repeat(64)),
            legacy_inventory_root_count: Some(0),
            legacy_inventory_root_set_sha256: Some(
                jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
            ),
            cutover_state: "ready".to_string(),
            unresolved_legacy_volume_count: 0,
            cutover_enrollment_generation: Some(4),
            cutover_purge_generation: Some(7),
            cutover_tombstone_generation: Some(6),
            cutover_destruction_generation: Some(2),
            cutover_legacy_reconciliation_generation: Some(3),
            cutover_storage_attestation_generation: Some(8),
            cutover_storage_attestation_count: Some(2),
            cutover_storage_attestation_set_sha256: Some("c".repeat(64)),
            cutover_legacy_inventory_generation: Some(5),
            cutover_legacy_inventory_reconciliation_id: Some("legacy-reconciliation".to_string()),
            cutover_legacy_inventory_authority_id: Some("legacy-authority".to_string()),
            cutover_legacy_inventory_authority_sha256: Some("b".repeat(64)),
            cutover_legacy_inventory_root_count: Some(0),
            cutover_legacy_inventory_root_set_sha256: Some(
                jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
            ),
            cutover_non_destroyed_volume_count: Some(2),
            cutover_destruction_count: Some(1),
            cutover_unresolved_legacy_volume_count: Some(0),
            cutover_evidence_ref: Some("fleet-evidence".to_string()),
            cutover_evidence_sha256: Some("a".repeat(64)),
            cutover_authorized_by: Some("operator".to_string()),
            cutover_at_ms: Some(100),
            non_destroyed_volume_count: 2,
            destruction_count: 1,
            attested_reconciled_volume_count: 2,
            updated_at_ms: 100,
        }
    }

    #[test]
    fn distribution_requires_exact_live_runner_volume_cutover_snapshot() {
        let ready = ready_runner_volume_fleet();
        assert!(runner_volume_fleet_is_distribution_ready(&ready));

        let mut generation_drift = ready.clone();
        generation_drift.purge_generation += 1;
        assert!(!runner_volume_fleet_is_distribution_ready(
            &generation_drift
        ));

        let mut legacy_unresolved = ready.clone();
        legacy_unresolved.unresolved_legacy_volume_count = 1;
        assert!(!runner_volume_fleet_is_distribution_ready(
            &legacy_unresolved
        ));

        let mut inactive_volume = ready.clone();
        inactive_volume.attested_reconciled_volume_count = 1;
        assert!(!runner_volume_fleet_is_distribution_ready(&inactive_volume));

        let mut legacy_inventory_drift = ready.clone();
        legacy_inventory_drift.legacy_inventory_generation += 1;
        assert!(!runner_volume_fleet_is_distribution_ready(
            &legacy_inventory_drift
        ));

        let mut nonempty_legacy_inventory = ready.clone();
        nonempty_legacy_inventory.legacy_inventory_root_count = Some(1);
        assert!(!runner_volume_fleet_is_distribution_ready(
            &nonempty_legacy_inventory
        ));

        let mut missing_evidence = ready;
        missing_evidence.cutover_evidence_sha256 = None;
        assert!(!runner_volume_fleet_is_distribution_ready(
            &missing_evidence
        ));
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

    #[test]
    fn runner_availability_serializes_signed_release_origin_in_snake_case() {
        let mut entitlement = test_entitlement(5);
        entitlement.plan = "pro".to_string();
        entitlement.local_browser = true;
        let mut availability = build_runner_availability(&entitlement, true, false);
        availability.local.release = Some(jobs::LocalBrowserReleaseAvailability::Available {
            reason: "The active beta release is available for this account.".to_string(),
            channel: "beta".to_string(),
            release_id: "browser-release-603-1".to_string(),
            artifact_origin: "https://bluey.sh".to_string(),
            manifest_sha256: "a".repeat(64),
            release_sequence: 1,
            build_id: "browser-603.1".to_string(),
            app_version: "0.1.5".to_string(),
            protocol_version: 1,
            released_at_ms: 1,
            artifacts: Vec::new(),
        });

        let serialized = serde_json::to_value(availability).unwrap();
        assert_eq!(
            serialized.pointer("/local/release/release_id"),
            Some(&json!("browser-release-603-1"))
        );
        assert_eq!(
            serialized.pointer("/local/release/artifact_origin"),
            Some(&json!("https://bluey.sh"))
        );
        assert!(serialized.pointer("/local/release/releaseId").is_none());
        assert!(serialized
            .pointer("/local/release/artifactOrigin")
            .is_none());
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
        let receipt_key = "accounts/acct-test/jobs/app-test/receipt.json";
        let resume_sha = "a".repeat(64);
        let screenshot_sha = "b".repeat(64);
        let receipt_sha = "c".repeat(64);
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
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY] = json!({
            "schemaVersion": 3,
            "adapter": "greenhouse",
            "adapterVersion": GREENHOUSE_SUBMISSION_ADAPTER_VERSION,
            "control": "greenhouse_submit_application",
            "job": {
                "approvedCanonicalUrl": approved_job["canonicalUrl"],
                "pageUrl": approved_job["canonicalUrl"],
            },
            "target": {
                "actionUrl": approved_job["canonicalUrl"],
                "method": "post",
                "enctype": "multipart/form-data",
                "formTarget": "_self",
                "providerJobKey": "greenhouse:acme:123",
                "formIdentity": r#"[0,"application-form","","","","",""]"#,
            },
            "files": [{
                "fieldName": "resume",
                "name": format!("resume-{resume_sha}.pdf"),
                "byteLength": 50,
                "sha256": resume_sha,
            }],
            "fields": [{
                "fieldName": "candidate_name",
                "valueByteLength": 0,
                "valueSha256":
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            }],
            "partOrder": [{
                "kind": "field",
                "index": 0,
            }, {
                "kind": "file",
                "index": 0,
            }],
            "documents": [{
                "kind": "resume",
                "versionId": "resume-test",
                "sha256": resume_sha,
            }]
        });
        let mut receipt = json!({
            "schemaVersion": 1,
            "applicationIdentityId": identity_id,
            "browserProfileId": browser_profile_id(account_id, identity_id),
            "adapter": "greenhouse",
            "adapterVersion": GREENHOUSE_SUBMISSION_ADAPTER_VERSION,
            "packet": {
                "jobId": "job-test",
                "resumeVersionId": "resume-test",
                "approvedPacketChecksum": approved_checksum,
                "applicationEmail": "apply@example.com",
                "answers": { "email": "apply@example.com" },
                "verifiedClaimIds": []
            },
            "job": approved_job.clone(),
            "documents": [{
                "kind": "resume",
                "versionId": "resume-test",
                "storageKey": resume_key,
                "sha256": resume_sha
            }],
            "screenshotKeys": [screenshot_key],
            "evidenceObjects": [{
                "kind": "screenshot",
                "storageKey": screenshot_key,
                "sha256": screenshot_sha,
                "mediaType": "image/png",
                "sizeBytes": 33
            }, {
                "kind": "resume",
                "storageKey": resume_key,
                "sha256": resume_sha,
                "mediaType": "application/pdf",
                "sizeBytes": 50
            }],
            "receiptObject": {
                "storageKey": receipt_key,
                "sha256": receipt_sha,
                "mediaType": "application/json",
                "sizeBytes": 512,
                "schemaVersion": 1
            }
        });
        receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY] = json!({
            "schemaVersion": 1,
            "preSubmissionReceipt": application.receipt.clone(),
            "executionAuthority": {
                "kind": "cloud_execution_lease",
                "ownerId": "runner-test",
                "leaseTokenSha256": "d".repeat(64),
                "fence": 1,
                "phase": "submitted",
            }
        });
        let verified_objects = BTreeMap::from([
            (resume_key.to_string(), resume_sha),
            (screenshot_key.to_string(), screenshot_sha),
            (receipt_key.to_string(), receipt_sha),
        ]);
        (application, posting, resume, receipt, verified_objects)
    }

    fn two_screenshot_evidence_fixture() -> (JobApplication, Vec<ApplicationEvidence>) {
        let (mut application, _, resume, mut receipt, mut verified_objects) =
            strict_receipt_fixture();
        let first_screenshot_key = receipt["screenshotKeys"][0].as_str().unwrap().to_string();
        let second_screenshot_key =
            "accounts/acct-test/jobs/app-test/confirmation2.png".to_string();
        let second_screenshot_sha = "d".repeat(64);
        receipt["screenshotKeys"] = json!([first_screenshot_key, second_screenshot_key]);
        receipt["evidenceObjects"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "kind": "screenshot",
                "storageKey": second_screenshot_key,
                "sha256": second_screenshot_sha,
                "mediaType": "image/png",
                "sizeBytes": 44,
            }));
        receipt["evidenceObjects"]
            .as_array_mut()
            .unwrap()
            .sort_by(|left, right| {
                left["storageKey"]
                    .as_str()
                    .unwrap()
                    .cmp(right["storageKey"].as_str().unwrap())
            });
        verified_objects.insert(second_screenshot_key, second_screenshot_sha);
        receipt["receiptId"] = json!("receipt-multi-screenshot");
        receipt["accountId"] = json!("acct-test");
        receipt["applicationId"] = json!(application.id);
        receipt[SUBMISSION_FINGERPRINT_KEY] = json!("e".repeat(64));
        let approved_job = receipt["job"].clone();
        let evidence = submission_evidence_records(
            &application.id,
            "receipt-multi-screenshot",
            "greenhouse",
            &approved_job,
            &resume,
            &receipt,
            &verified_objects,
            302,
            "Application received".to_string(),
            Some("https://boards.greenhouse.io/acme/jobs/123/confirmation".to_string()),
            Some("Thanks for applying".to_string()),
            Some(json!("2026-08-05T12:00:00Z")),
        )
        .unwrap();
        application.receipt = receipt;
        application.state = "submitted".to_string();
        application.submitted_at_ms = Some(1);
        (application, evidence)
    }

    #[test]
    fn submission_evidence_records_materialize_every_confirmation_screenshot() {
        let (_, evidence) = two_screenshot_evidence_fixture();
        let confirmations = evidence
            .iter()
            .filter(|item| item.kind == "submission_confirmation")
            .collect::<Vec<_>>();

        assert_eq!(confirmations.len(), 2);
        assert_eq!(
            confirmations
                .iter()
                .map(|item| item.file_name.as_str())
                .collect::<Vec<_>>(),
            [
                "submission-confirmation-1-of-2.png",
                "submission-confirmation-2-of-2.png"
            ]
        );
        for (index, confirmation) in confirmations.iter().enumerate() {
            assert_eq!(confirmation.metadata["immutable"], json!(true));
            assert_eq!(confirmation.metadata["screenshot_index"], json!(index + 1));
            assert_eq!(confirmation.metadata["screenshot_count"], json!(2));
            assert_eq!(confirmation.metadata["submit_http_status"], json!(302));
            assert_eq!(
                confirmation.storage_key,
                confirmation.metadata["screenshot_keys"][index]
                    .as_str()
                    .unwrap()
            );
        }
    }

    #[test]
    fn evidence_download_requires_the_exact_complete_screenshot_set() {
        let (application, evidence) = two_screenshot_evidence_fixture();
        let confirmations = evidence
            .iter()
            .filter(|item| item.kind == "submission_confirmation")
            .collect::<Vec<_>>();
        assert_eq!(confirmations.len(), 2);
        for confirmation in &confirmations {
            assert_eq!(
                validate_evidence_download_binding(
                    "acct-test",
                    &application,
                    confirmation,
                    &evidence,
                    "image/png",
                )
                .unwrap(),
                confirmation.metadata["size_bytes"].as_i64().unwrap() as usize
            );
        }

        let missing = evidence
            .iter()
            .filter(|item| {
                item.id != confirmations[1].id || item.storage_key != confirmations[1].storage_key
            })
            .cloned()
            .collect::<Vec<_>>();
        let selected = missing
            .iter()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap();
        assert!(validate_evidence_download_binding(
            "acct-test",
            &application,
            selected,
            &missing,
            "image/png",
        )
        .is_err());

        let mut duplicate = evidence.clone();
        let last = duplicate
            .iter_mut()
            .rev()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap();
        last.metadata["screenshot_index"] = json!(1);
        let selected = duplicate
            .iter()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap();
        assert!(validate_evidence_download_binding(
            "acct-test",
            &application,
            selected,
            &duplicate,
            "image/png",
        )
        .is_err());

        let mut mismatched = evidence.clone();
        let last = mismatched
            .iter_mut()
            .rev()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap();
        last.metadata["screenshot_keys"][1] = json!("unbound-screenshot.png");
        let selected = mismatched
            .iter()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap();
        assert!(validate_evidence_download_binding(
            "acct-test",
            &application,
            selected,
            &mismatched,
            "image/png",
        )
        .is_err());
    }

    #[test]
    fn evidence_download_accepts_legacy_single_screenshot_records() {
        let (mut application, mut evidence) = two_screenshot_evidence_fixture();
        let second_key = application.receipt["screenshotKeys"][1]
            .as_str()
            .unwrap()
            .to_string();
        application.receipt["screenshotKeys"]
            .as_array_mut()
            .unwrap()
            .pop();
        application.receipt["evidenceObjects"]
            .as_array_mut()
            .unwrap()
            .retain(|item| item["storageKey"].as_str() != Some(second_key.as_str()));
        let mut kept_confirmation = false;
        evidence.retain(|item| {
            if item.kind != "submission_confirmation" {
                return true;
            }
            if kept_confirmation {
                return false;
            }
            kept_confirmation = true;
            true
        });
        let confirmation = evidence
            .iter_mut()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap();
        confirmation.file_name = "submission-confirmation.png".to_string();
        confirmation.metadata["screenshot_keys"] = application.receipt["screenshotKeys"].clone();
        confirmation
            .metadata
            .as_object_mut()
            .unwrap()
            .remove("screenshot_index");
        confirmation
            .metadata
            .as_object_mut()
            .unwrap()
            .remove("screenshot_count");
        confirmation
            .metadata
            .as_object_mut()
            .unwrap()
            .remove("immutable");
        let confirmation_id = confirmation.id.clone();
        let confirmation = evidence
            .iter()
            .find(|item| item.id == confirmation_id && item.kind == "submission_confirmation")
            .unwrap();

        assert!(validate_evidence_download_binding(
            "acct-test",
            &application,
            confirmation,
            &evidence,
            "image/png",
        )
        .is_ok());

        let confirmation_index = evidence
            .iter()
            .position(|item| item.kind == "submission_confirmation")
            .unwrap();
        evidence[confirmation_index].metadata["screenshot_index"] = json!("1");
        assert!(validate_evidence_download_binding(
            "acct-test",
            &application,
            &evidence[confirmation_index],
            &evidence,
            "image/png",
        )
        .is_err());
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
    fn approved_execution_checksum_matches_shared_rust_typescript_vectors() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../jobs/automation/tests/fixtures/approved-execution-vectors.json"
        ))
        .unwrap();
        for vector in fixture["vectors"].as_array().unwrap() {
            let name = vector["name"].as_str().unwrap();
            let schema_version = vector["schemaVersion"].as_i64().unwrap();
            let checksum = if schema_version == 1 {
                approved_execution_checksum(&vector["packet"], &vector["job"]).unwrap()
            } else {
                approved_execution_checksum_with_admission(
                    schema_version,
                    &vector["packet"],
                    &vector["job"],
                    &vector["admission"],
                )
                .unwrap()
            };
            assert_eq!(checksum, vector["checksum"].as_str().unwrap(), "{name}");
        }
    }

    #[test]
    fn approved_execution_checksum_rejects_non_interoperable_numbers() {
        for value in [json!(1.5), json!(-0.0), json!(9_007_199_254_740_992_u64)] {
            let error =
                approved_execution_checksum(&json!({ "value": value }), &json!({})).unwrap_err();
            assert_eq!(error.0, StatusCode::CONFLICT);
            assert!(error.1.contains("non-interoperable number"));
        }
    }

    #[test]
    fn runner_packet_carries_explicit_approval_schema_and_admission() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        let mut legacy_packet = application.receipt["approved_execution"]["packet"].clone();
        let legacy_checksum = application.receipt["approved_execution"]["checksum"]
            .as_str()
            .unwrap()
            .to_string();
        attach_approved_execution_transport(&application, &mut legacy_packet, legacy_checksum)
            .unwrap();
        assert_eq!(legacy_packet["approvedExecutionSchemaVersion"], json!(1));
        assert!(legacy_packet.get("approvedExecutionAdmission").is_none());

        let packet = application.receipt["approved_execution"]["packet"].clone();
        let job = application.receipt["approved_execution"]["job"].clone();
        let admission = json!({ "kind": "review_approval" });
        let checksum = approved_execution_checksum_v2(&packet, &job, &admission).unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 2,
            "approved_at_ms": 1,
            "checksum": checksum,
            "admission": admission,
            "packet": packet,
            "job": job,
        });
        approved_execution_snapshot(&application).unwrap();
        let mut runtime_packet = application.receipt["approved_execution"]["packet"].clone();
        attach_approved_execution_transport(&application, &mut runtime_packet, checksum).unwrap();
        assert_eq!(runtime_packet["approvedExecutionSchemaVersion"], json!(2));
        assert_eq!(
            runtime_packet["approvedExecutionAdmission"],
            json!({ "kind": "review_approval" })
        );
    }

    #[test]
    fn certified_auto_packet_requires_and_transports_exact_schema_three_admission() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        application.submission_mode = "auto_submit".to_string();
        let packet = application.receipt["approved_execution"]["packet"].clone();
        let job = application.receipt["approved_execution"]["job"].clone();
        let admission = json!({
            "kind": "track_auto_submit",
            "authorization_id": "auto-auth-604",
            "career_track_id": "track-test",
            "revision_no": 4,
            "authority_fingerprint": "1".repeat(64),
            "ats_certification": {
                "schema_version": 1,
                "provider": "greenhouse",
                "adapter_version": "2026.07.1-beta.1",
                "manifest_sha256": "2".repeat(64),
                "activation_sha256": "3".repeat(64),
                "activation_generation": 5,
                "target_key_sha256": "6".repeat(64),
                "layout_set_sha256": "7".repeat(64),
                "variant_key": "greenhouse_public",
                "layout_contract_version": 1,
                "surface_sha256": "4".repeat(64),
                "adapter_bundle_sha256": "8".repeat(64),
                "runner_target_sha256s": ["9".repeat(64)],
                "expires_at_ms": 9_007_199_254_740_000_i64,
            }
        });
        let checksum =
            approved_execution_checksum_with_admission(3, &packet, &job, &admission).unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 3,
            "approved_at_ms": 1,
            "checksum": checksum,
            "admission": admission,
            "packet": packet,
            "job": job,
        });

        approved_execution_snapshot(&application).unwrap();
        let mut runtime_packet = application.receipt["approved_execution"]["packet"].clone();
        attach_approved_execution_transport(&application, &mut runtime_packet, checksum).unwrap();
        assert_eq!(runtime_packet["approvedExecutionSchemaVersion"], json!(3));
        assert_eq!(
            runtime_packet
                .pointer("/approvedExecutionAdmission/ats_certification/manifest_sha256")
                .and_then(Value::as_str)
                .unwrap(),
            "2".repeat(64)
        );

        application.receipt["approved_execution"]["admission"]["ats_certification"]
            ["manifest_sha256"] = json!("a".repeat(64));
        let error = approved_execution_snapshot(&application).unwrap_err();
        assert_eq!(error.0, StatusCode::CONFLICT);
        assert!(error.1.contains("changed after review"));
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

    #[test]
    fn submission_authority_snapshot_preserves_auto_submit_and_packet_history() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        application.submission_mode = "auto_submit".to_string();
        application.cover_letter = "Dear Acme,\n\nI build reliable systems.".to_string();
        application.receipt["approved_execution"]["packet"]["coverLetterContent"] =
            json!(application.cover_letter);
        application.receipt["packet_revisions"] = json!([{
            "revision_no": 2,
            "reason": "intervention_answer",
            "intervention_id": "intervention-one",
            "answer_fingerprint": "e".repeat(64)
        }]);
        let packet = application.receipt["approved_execution"]["packet"].clone();
        let job = application.receipt["approved_execution"]["job"].clone();
        let admission = json!({
            "kind": "track_auto_submit",
            "authorization_id": "auto-auth-one",
            "career_track_id": "track-test",
            "revision_no": 7,
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

        let authority = submission_authority_snapshot(
            &application,
            json!({
                "kind": "cloud_execution_lease",
                "ownerId": "runner-one",
                "leaseTokenSha256": "c".repeat(64),
                "fence": 9,
                "phase": "submitted",
            }),
        )
        .unwrap();
        let preserved = authority.get("preSubmissionReceipt").unwrap();
        assert_eq!(preserved, &application.receipt);
        assert_eq!(
            preserved
                .pointer("/approved_execution/admission/authorization_id")
                .and_then(Value::as_str),
            Some("auto-auth-one")
        );
        assert_eq!(
            preserved
                .pointer("/approved_execution/admission/revision_no")
                .and_then(Value::as_i64),
            Some(7)
        );
        assert_eq!(
            preserved
                .pointer("/approved_execution/packet/coverLetterContent")
                .and_then(Value::as_str),
            Some("Dear Acme,\n\nI build reliable systems.")
        );
        assert_eq!(
            preserved
                .pointer("/packet_revisions/0/intervention_id")
                .and_then(Value::as_str),
            Some("intervention-one")
        );
        assert_eq!(
            authority
                .pointer("/executionAuthority/ownerId")
                .and_then(Value::as_str),
            Some("runner-one")
        );
        assert_eq!(
            authority
                .pointer("/executionAuthority/fence")
                .and_then(Value::as_i64),
            Some(9)
        );
    }

    #[test]
    fn finalize_error_reconciliation_never_deletes_an_uncertain_committed_receipt() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        let fingerprint = "f".repeat(64);
        application.state = "submitted".to_string();
        application.receipt[SUBMISSION_FINGERPRINT_KEY] = json!(fingerprint);

        assert_eq!(
            submission_finalize_error_disposition(Some(&application), &fingerprint),
            SubmissionFinalizeErrorDisposition::Replay
        );

        application.receipt[SUBMISSION_FINGERPRINT_KEY] = json!("e".repeat(64));
        assert_eq!(
            submission_finalize_error_disposition(Some(&application), &fingerprint),
            SubmissionFinalizeErrorDisposition::Conflict
        );

        application.state = "running".to_string();
        assert_eq!(
            submission_finalize_error_disposition(Some(&application), &fingerprint),
            SubmissionFinalizeErrorDisposition::NotCommitted
        );
        assert_eq!(
            submission_finalize_error_disposition(None, &fingerprint),
            SubmissionFinalizeErrorDisposition::NotCommitted
        );
    }

    #[test]
    fn precommit_upload_error_reconciliation_uses_authoritative_submitted_fingerprint() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-submission-reconciliation-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        let fingerprint = "f".repeat(64);
        application.state = "submitted".to_string();
        application.receipt[SUBMISSION_FINGERPRINT_KEY] = json!(&fingerprint);
        let payload = serde_json::to_string(&application).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ('acct-test', 'submission-reconciliation@example.com', 'hash', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, company, title,
                created_at_ms, updated_at_ms
             ) VALUES (
                'job-test', 'acct-test', 'job-test', '{}', 'greenhouse', 'Acme',
                'Engineer', 1, 1
             )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_applications (
                id, account_id, job_id, state, application_json, created_at_ms, updated_at_ms
             ) VALUES ('app-test', 'acct-test', 'job-test', 'submitted', ?1, 1, 1)",
            rusqlite::params![payload],
        )
        .unwrap();
        drop(conn);

        let replay = reconcile_submission_precommit_error(
            &pool,
            "acct-test",
            "app-test",
            &fingerprint,
            (
                StatusCode::BAD_GATEWAY,
                "original upload failure".to_string(),
            ),
        )
        .unwrap();
        assert_eq!(replay.state, "submitted");

        application.receipt[SUBMISSION_FINGERPRINT_KEY] = json!("e".repeat(64));
        let payload = serde_json::to_string(&application).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications SET application_json = ?1 WHERE id = 'app-test'",
                rusqlite::params![payload],
            )
            .unwrap();
        let conflict = reconcile_submission_precommit_error(
            &pool,
            "acct-test",
            "app-test",
            &fingerprint,
            (
                StatusCode::BAD_GATEWAY,
                "original upload failure".to_string(),
            ),
        )
        .unwrap_err();
        assert_eq!(conflict.0, StatusCode::CONFLICT);

        application.state = "running".to_string();
        let payload = serde_json::to_string(&application).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications
                    SET state = 'running', application_json = ?1
                  WHERE id = 'app-test'",
                rusqlite::params![payload],
            )
            .unwrap();
        let original = reconcile_submission_precommit_error(
            &pool,
            "acct-test",
            "app-test",
            &fingerprint,
            (
                StatusCode::BAD_GATEWAY,
                "original upload failure".to_string(),
            ),
        )
        .unwrap_err();
        assert_eq!(
            original,
            (
                StatusCode::BAD_GATEWAY,
                "original upload failure".to_string()
            )
        );
    }

    #[test]
    fn submitted_cloud_receipt_replay_uses_only_frozen_token_and_fence_authority() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        let lease_token = "opaque-cloud-lease-token";
        application.state = "submitted".to_string();
        application.receipt = json!({
            "runner": "cloud",
            "runId": "run-test",
            (jobs::SERVER_SUBMISSION_AUTHORITY_KEY): {
                "executionAuthority": {
                    "kind": "cloud_execution_lease",
                    "ownerId": "runner-test",
                    "leaseTokenSha256": hex::encode(Sha256::digest(lease_token.as_bytes())),
                    "fence": 7,
                    "phase": "submitted",
                }
            }
        });

        assert!(jobs::submitted_cloud_receipt_replay_authorized(
            &application,
            "run-test",
            lease_token,
            7,
        ));
        assert!(!jobs::submitted_cloud_receipt_replay_authorized(
            &application,
            "run-test",
            "wrong-token",
            7,
        ));
        assert!(!jobs::submitted_cloud_receipt_replay_authorized(
            &application,
            "run-test",
            lease_token,
            8,
        ));

        application.receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]["executionAuthority"]["phase"] =
            json!("released");
        assert!(!jobs::submitted_cloud_receipt_replay_authorized(
            &application,
            "run-test",
            lease_token,
            7,
        ));
    }

    #[test]
    fn submitted_local_receipt_replay_uses_frozen_nonce_bearing_result_capability() {
        let (mut application, _, _, _, _) = strict_receipt_fixture();
        let result_capability = format!("claims-with-a-random-nonce.{}", "a".repeat(64));
        let replay_hash = jobs::local_result_replay_credential_sha256(&result_capability).unwrap();
        application.state = "submitted".to_string();
        application.receipt = json!({
            "runner": "local",
            "runId": "run-test",
            (jobs::SERVER_SUBMISSION_AUTHORITY_KEY): {
                "executionAuthority": {
                    "kind": "local_run_ticket",
                    "ticketHash": "b".repeat(64),
                    "runId": "run-test",
                    "resultCapabilitySha256": replay_hash,
                    "browserRelease": {
                        "schemaVersion": 2,
                        "bindingSha256": "1".repeat(64),
                        "assignmentSha256": "2".repeat(64),
                        "assignmentGeneration": 1,
                        "channel": "beta",
                        "channelHeadRevision": 1,
                        "channelTransitionSha256": "3".repeat(64),
                        "activationSha256": "4".repeat(64),
                        "activationGeneration": 1,
                        "trustGeneration": 1,
                        "trustPolicySha256": "5".repeat(64),
                        "channelSequence": 1,
                        "manifestSignatureSetSha256": "6".repeat(64),
                        "activationAuthorizationSignatureSetSha256": "7".repeat(64),
                        "manifestSha256": "8".repeat(64),
                        "releaseSequence": 1,
                        "artifactId": "artifact-test",
                        "artifactSha256": "9".repeat(64),
                        "releaseId": "release-test",
                        "buildId": "browser-1.0",
                        "appVersion": "1.0.0",
                        "protocolVersion": 1,
                        "platform": "darwin",
                        "architecture": "arm64",
                        "packageKind": "darwin-dmg",
                        "descriptorSha256": "a".repeat(64),
                    },
                }
            }
        });

        assert!(jobs::submitted_local_receipt_replay_authorized(
            &application,
            "run-test",
            &result_capability,
        ));
        assert!(!jobs::submitted_local_receipt_replay_authorized(
            &application,
            "run-test",
            &format!("different-nonce.{}", "a".repeat(64)),
        ));
        assert!(!jobs::submitted_local_receipt_replay_authorized(
            &application,
            "different-run",
            &result_capability,
        ));

        let execution = application.receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]
            ["executionAuthority"]
            .as_object_mut()
            .unwrap();
        execution.remove("browserRelease");
        assert_eq!(execution.len(), 4);
        assert!(jobs::submitted_local_receipt_replay_authorized(
            &application,
            "run-test",
            &result_capability,
        ));
        application.receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]["executionAuthority"]
            ["unexpected"] = json!(true);
        assert!(!jobs::submitted_local_receipt_replay_authorized(
            &application,
            "run-test",
            &result_capability,
        ));
        application.receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]["executionAuthority"]
            .as_object_mut()
            .unwrap()
            .remove("unexpected");

        application.receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]["executionAuthority"]
            .as_object_mut()
            .unwrap()
            .remove("resultCapabilitySha256");
        assert!(!jobs::submitted_local_receipt_replay_authorized(
            &application,
            "run-test",
            &result_capability,
        ));
    }

    fn pdf_fixture() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for object in [
            b"<< /Type /Catalog /Pages 2 0 R >>".as_slice(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".as_slice(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>".as_slice(),
        ] {
            offsets.push(pdf.len());
            let object_number = offsets.len();
            pdf.extend_from_slice(format!("{object_number} 0 obj\n").as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        pdf
    }

    fn png_fixture() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0, 0, 0]).unwrap();
        }
        bytes
    }

    fn receipt_preflight_fixture() -> (Value, Vec<ReceiptEvidenceObject>) {
        let (_, _, _, mut receipt, _) = strict_receipt_fixture();
        receipt.as_object_mut().unwrap().remove("receiptObject");
        receipt
            .as_object_mut()
            .unwrap()
            .remove(jobs::SERVER_SUBMISSION_AUTHORITY_KEY);
        receipt.as_object_mut().unwrap().remove("evidenceObjects");
        let pdf = pdf_fixture();
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
    fn receipt_evidence_decoders_reject_truncated_or_corrupted_files() {
        let pdf = pdf_fixture();
        assert!(valid_pdf(&pdf));
        assert!(!valid_pdf(&pdf[..pdf.len() - 12]));
        let mut corrupt_pdf = pdf.clone();
        let xref = corrupt_pdf
            .windows(4)
            .position(|window| window == b"xref")
            .expect("fixture xref");
        corrupt_pdf[xref] = b'z';
        assert!(!valid_pdf(&corrupt_pdf));

        let png = png_fixture();
        assert!(valid_png(&png));
        assert!(!valid_png(&png[..png.len() - 8]));
        let mut corrupt_png = png;
        let pixel = corrupt_png.len() / 2;
        corrupt_png[pixel] ^= 0x01;
        assert!(!valid_png(&corrupt_png));
    }

    #[test]
    fn receipt_documents_must_match_the_pre_click_hash_proof_exactly() {
        let (application, _, _, receipt, _) = strict_receipt_fixture();
        validate_receipt_final_submit_proof(&application, &receipt).unwrap();

        let mut wrong_hash = receipt.clone();
        wrong_hash["documents"][0]["sha256"] = json!("f".repeat(64));
        assert!(
            validate_receipt_final_submit_proof(&application, &wrong_hash)
                .unwrap_err()
                .1
                .contains("pre-click proof")
        );

        let mut wrong_version = receipt.clone();
        wrong_version["documents"][0]["versionId"] = json!("resume-other");
        assert!(validate_receipt_final_submit_proof(&application, &wrong_version).is_err());

        let mut extra = receipt;
        extra["documents"].as_array_mut().unwrap().push(json!({
            "kind": "attachment",
            "storageKey": "accounts/acct-test/jobs/app-test/attachment.pdf",
            "sha256": "e".repeat(64),
        }));
        assert!(validate_receipt_final_submit_proof(&application, &extra).is_err());
    }

    fn provider_receipt_fixture() -> (JobApplication, Value) {
        let (application, _, _, mut receipt, _) = strict_receipt_fixture();
        receipt["generatedAt"] = json!("2026-08-04T12:00:30Z");
        receipt["finalUrl"] = json!("https://boards.greenhouse.io/acme/jobs/123/confirmation");
        receipt["result"] = json!({
            "status": "submitted",
            "submitHttpStatus": 302,
            "confirmationText": "Thank you for applying. Your application was received.",
            "confirmationUrl": "https://boards.greenhouse.io/acme/jobs/123/confirmation",
            "submittedAt": "2026-08-04T12:00:00Z",
            "issues": [],
        });
        receipt["events"] = json!([{
            "id": "provider-final-state",
            "occurredAt": "2026-08-04T12:00:01Z",
            "type": "greenhouse_state_transition",
            "detail": {
                "state": "receipt",
                "status": "submitted",
                "capability": "beta_review",
            }
        }]);
        (application, receipt)
    }

    #[test]
    fn explicit_submission_confirmation_requires_supported_positive_language() {
        for confirmation in [
            "Thank you for applying.",
            "THANK\nYOU\tFOR APPLYING!",
            "Thanks for applying.",
            "We received your application.",
            "Your application has been submitted.",
            "Your application was received.",
            "Thank you for applying. You haven't submitted a cover letter because it was optional.",
            "Thanks for applying. We haven't yet reviewed your application.",
        ] {
            assert!(
                has_explicit_submission_confirmation(confirmation),
                "rejected supported confirmation: {confirmation:?}"
            );
        }

        for unproven in [
            "",
            "Application pending.",
            "Submission is still processing.",
            "Please review the application before continuing.",
        ] {
            assert!(
                !has_explicit_submission_confirmation(unproven),
                "accepted unproven confirmation: {unproven:?}"
            );
        }
    }

    #[test]
    fn explicit_submission_confirmation_rejects_every_negative_outcome_family() {
        for negative in [
            "You already applied",
            "You already submitted an application",
            "Application already submitted",
            "The application was already submitted",
            "Your application has been already submitted",
            "The application had been already submitted",
            "Your application has already been submitted",
            "The application had already been submitted",
            "Unable to submit your application",
            "Failed to submit the application",
            "Could not submit your application",
            "Couldn't submit your application",
            "Cannot submit the application",
            "Can't submit your application",
            "Your application could not be submitted",
            "Your application couldn't be submitted",
            "Your application cannot be submitted",
            "Your application can't be submitted",
            "Your application was unable to be submitted",
            "Your application was not submitted",
            "Your application wasn't successfully submitted",
            "Your application has not been submitted",
            "Your application hasn't been successfully submitted",
            "Your application has not yet been submitted",
            "Your application hasn't yet been submitted",
            "Your application hasn’t yet been submitted",
            "The application had not yet been submitted",
            "The application hadn't yet been submitted",
            "Your application is not submitted",
            "Your application isn't successfully submitted",
            "Your application was not yet submitted",
            "Your application wasn't yet submitted",
            "Your application wasn’t yet submitted",
            "You have not submitted your application",
            "You have not yet submitted your application",
            "You haven't submitted your application",
            "You haven't yet submitted your application",
            "You haven’t yet submitted your application",
            "The system has not yet submitted your application",
            "We had not yet submitted an application",
            "You did not submit the application",
            "You didn't submit your application",
            "You didn't yet submit your application",
            "You didn’t yet submit your application",
            "Status: NOT\nSUBMITTED",
            "Status: NOT YET SUBMITTED",
            "We did not receive your application",
            "We didn't receive the application",
            "We have not received your application",
            "We haven't received application",
            "The application was not received",
            "The application wasn't received",
            "Your application has not been received",
            "Your application hasn't been received",
            "The application is not received",
            "The application isn't received",
            "Submission failed",
            "Application submission was unsuccessful",
            "Submission was not successful",
            "The application was not successfully submitted",
        ] {
            let mixed_body =
                format!("Thank you for applying. {negative}. We received your application.");
            assert!(
                !has_explicit_submission_confirmation(&mixed_body),
                "accepted mixed positive and negative confirmation: {negative:?}"
            );
        }
    }

    #[test]
    fn provider_submission_proof_rejects_mixed_negative_confirmation_text() {
        let (application, receipt) = provider_receipt_fixture();

        for confirmation_text in [
            "Thank you for applying, but your application was not submitted.",
            "We received your application. However, it could not be submitted.",
            "Thanks for applying, but the application was not received.",
            "Thanks for applying. Application submission failed.",
            "Thank you for applying. You have not submitted your application.",
            "Thanks for applying. You haven't yet submitted your application.",
            "We received your application, but it hasn’t yet been submitted.",
        ] {
            let mut invalid = receipt.clone();
            invalid["result"]["confirmationText"] = json!(confirmation_text);
            let error = validate_provider_submission_proof(&application, &invalid).unwrap_err();
            assert_eq!(error.0, StatusCode::BAD_REQUEST);
            assert!(error.1.contains("does not prove a successful submission"));
        }
    }

    #[test]
    fn provider_submission_proof_requires_exact_adapter_state_url_and_time() {
        let (application, receipt) = provider_receipt_fixture();
        validate_provider_submission_proof(&application, &receipt).unwrap();

        for invalid in [
            {
                let mut value = receipt.clone();
                value["adapterVersion"] = json!("2026.07.0-beta.1");
                value
            },
            {
                let mut value = receipt.clone();
                value["events"][0]["detail"]["status"] = json!("pending");
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]["confirmationUrl"] =
                    json!("https://jobs.lever.co/acme/confirmation");
                value
            },
            {
                let mut value = receipt.clone();
                value["finalUrl"] =
                    json!("https://boards.greenhouse.io/acme/jobs/other/confirmation");
                value["result"]["confirmationUrl"] =
                    json!("https://boards.greenhouse.io/acme/jobs/other/confirmation");
                value
            },
            {
                let mut value = receipt.clone();
                value["finalUrl"] = json!("https://boards.greenhouse.io/acme/confirmation");
                value["result"]["confirmationUrl"] =
                    json!("https://boards.greenhouse.io/acme/confirmation");
                value
            },
            {
                let mut value = receipt.clone();
                value["finalUrl"] = json!("https://boards.greenhouse.io/acme/jobs/123");
                value["result"]["confirmationUrl"] =
                    json!("https://boards.greenhouse.io/acme/jobs/123");
                value
            },
            {
                let mut value = receipt.clone();
                value["events"][0]["occurredAt"] = json!("2026-08-04T13:00:00Z");
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]["confirmationText"] = json!("Application already submitted");
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]
                    .as_object_mut()
                    .unwrap()
                    .remove("submitHttpStatus");
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]["submitHttpStatus"] = json!(199);
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]["submitHttpStatus"] = json!(304);
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]["submitHttpStatus"] = json!(400);
                value
            },
            {
                let mut value = receipt.clone();
                value["result"]["submitHttpStatus"] = json!("302");
                value
            },
        ] {
            assert!(validate_provider_submission_proof(&application, &invalid).is_err());
        }
    }

    #[test]
    fn successful_submit_http_status_accepts_only_2xx_and_intended_redirects() {
        let mut result = serde_json::Map::new();

        for status in 200..=399 {
            result.insert("submitHttpStatus".to_string(), json!(status));
            let expected = status <= 299 || matches!(status, 301 | 302 | 303 | 307 | 308);
            if expected {
                assert_eq!(successful_submit_http_status(&result).unwrap(), status);
            } else {
                assert!(successful_submit_http_status(&result).is_err());
            }
        }

        for status in [199, 400] {
            result.insert("submitHttpStatus".to_string(), json!(status));
            assert!(successful_submit_http_status(&result).is_err());
        }
    }

    #[test]
    fn lever_provider_submission_proof_requires_its_exact_final_state() {
        let (mut application, mut receipt) = provider_receipt_fixture();
        application.receipt["approved_execution"]["job"]["canonicalUrl"] =
            json!("https://jobs.lever.co/acme/job-123");
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY]["adapter"] = json!("lever");
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY]["adapterVersion"] =
            json!(LEVER_SUBMISSION_ADAPTER_VERSION);
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY]["control"] =
            json!("lever_application_submit");
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY]["job"] = json!({
            "approvedCanonicalUrl": "https://jobs.lever.co/acme/job-123",
            "pageUrl": "https://jobs.lever.co/acme/job-123",
        });
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY]["target"] = json!({
            "actionUrl": "https://jobs.lever.co/acme/job-123/apply",
            "method": "post",
            "enctype": "multipart/form-data",
            "formTarget": "_self",
            "providerJobKey": "lever:jobs.lever.co:acme:job-123",
            "formIdentity": r#"[0,"application-form","","","","",""]"#,
        });
        receipt["adapter"] = json!("lever");
        receipt["adapterVersion"] = json!(LEVER_SUBMISSION_ADAPTER_VERSION);
        receipt["finalUrl"] = json!("https://jobs.lever.co/acme/job-123/confirmation");
        receipt["result"]["confirmationUrl"] =
            json!("https://jobs.lever.co/acme/job-123/confirmation");
        receipt["events"][0] = json!({
            "id": "provider-final-state",
            "occurredAt": "2026-08-04T12:00:01Z",
            "type": "lever_state_changed",
            "detail": {
                "state": "receipt",
                "outcome": "submitted",
                "page_kind": "confirmation",
                "mode": "review_only",
            }
        });
        validate_provider_submission_proof(&application, &receipt).unwrap();

        receipt["events"][0]["detail"]["mode"] = json!("auto_submit");
        assert!(validate_provider_submission_proof(&application, &receipt).is_err());
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
        let (application, _posting, resume, receipt, verified_objects) = strict_receipt_fixture();
        validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &receipt,
            &verified_objects,
            true,
        )
        .unwrap();

        let mut wrong_identity = receipt.clone();
        wrong_identity["applicationIdentityId"] = json!("identity-other");
        assert_eq!(
            validate_receipt_bundle(
                "acct-test",
                &application,
                &resume,
                &wrong_identity,
                &verified_objects,
                true,
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
                &resume,
                &wrong_resume,
                &verified_objects,
                true,
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
            &resume,
            &changed_answers,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(changed_answers_error.0, StatusCode::BAD_REQUEST);
        assert!(changed_answers_error.1.contains("exact application packet"));

        let mut wrong_approval = receipt.clone();
        wrong_approval["packet"]["approvedPacketChecksum"] = json!("d".repeat(64));
        let wrong_approval_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &wrong_approval,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(wrong_approval_error.0, StatusCode::BAD_REQUEST);
        assert!(wrong_approval_error.1.contains("exact application packet"));

        let mut wrong_claims = receipt.clone();
        wrong_claims["packet"]["verifiedClaimIds"] = json!(["claim-not-approved"]);
        let wrong_claims_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &wrong_claims,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(wrong_claims_error.0, StatusCode::BAD_REQUEST);
        assert!(wrong_claims_error.1.contains("exact application packet"));

        let mut wrong_job = receipt.clone();
        wrong_job["job"]["company"] = json!("Other employer");
        let wrong_job_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &wrong_job,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(wrong_job_error.0, StatusCode::BAD_REQUEST);
        assert!(wrong_job_error.1.contains("exact approved posting"));

        let mut wrong_screenshot_hash = receipt.clone();
        wrong_screenshot_hash["evidenceObjects"][0]["sha256"] = json!("d".repeat(64));
        let manifest_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &wrong_screenshot_hash,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(manifest_error.0, StatusCode::BAD_REQUEST);
        assert!(manifest_error.1.contains("manifest"));

        let unverified = BTreeMap::new();
        let error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &receipt,
            &unverified,
            true,
        )
        .unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1.contains("manifest"));

        let mut missing_receipt_object = receipt.clone();
        missing_receipt_object
            .as_object_mut()
            .unwrap()
            .remove("receiptObject");
        let missing_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &missing_receipt_object,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(missing_error.0, StatusCode::BAD_REQUEST);
        assert!(missing_error.1.contains("immutable receipt object"));

        let mut tampered_receipt_object = receipt.clone();
        tampered_receipt_object["receiptObject"]["sha256"] = json!("d".repeat(64));
        let tampered_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &tampered_receipt_object,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(tampered_error.0, StatusCode::BAD_REQUEST);
        assert!(tampered_error.1.contains("not uploaded and verified"));

        let mut tampered_authority = receipt.clone();
        tampered_authority[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"]
            ["packet_revisions"] = json!([]);
        let authority_error = validate_receipt_bundle(
            "acct-test",
            &application,
            &resume,
            &tampered_authority,
            &verified_objects,
            true,
        )
        .unwrap_err();
        assert_eq!(authority_error.0, StatusCode::BAD_REQUEST);
        assert!(authority_error.1.contains("submission authority"));
    }

    struct CertifiedReceiptValidationFixture {
        pool: crate::db::DbPool,
        application: JobApplication,
        resume: ResumeVersion,
        receipt: Value,
        verified_objects: BTreeMap<String, String>,
    }

    fn certified_receipt_validation_fixture() -> CertifiedReceiptValidationFixture {
        const SURFACE_SHA256: &str =
            "a88a8916beb511abb11af4acd27242723f8387ea8bebf639dde83a5629f92a68";
        let (mut application, _, resume, mut receipt, verified_objects) = strict_receipt_fixture();
        let account_id = "acct-test";
        let application_id = application.id.clone();
        let run_id = application.run_id.clone().unwrap();
        let target_key = "greenhouse:acme:123";
        let target_key_sha256 = sha256_hex(target_key.as_bytes());
        let manifest_sha256 = "2".repeat(64);
        let activation_sha256 = "3".repeat(64);
        let layout_observation_sha256 = "4".repeat(64);
        let layout_set_sha256 = "7".repeat(64);
        let adapter_bundle_sha256 = "8".repeat(64);
        let runner_target_sha256 = "9".repeat(64);
        let metering_reservation_sha256 = "c".repeat(64);
        let now = chrono::Utc::now();
        let now_ms = now.timestamp_millis();
        let consumed_at_ms = now_ms - 1_000;
        let expires_at_ms = now_ms + 60 * 60 * 1_000;
        let generated_at = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        application.submission_mode = "auto_submit".to_string();
        let approved_packet = application.receipt["approved_execution"]["packet"].clone();
        let approved_job = application.receipt["approved_execution"]["job"].clone();
        let certification_admission =
            serde_json::to_value(jobs::AtsFrozenCertificationAdmissionProjection {
                schema_version: 1,
                provider: "greenhouse".to_string(),
                adapter_version: GREENHOUSE_SUBMISSION_ADAPTER_VERSION.to_string(),
                manifest_sha256: manifest_sha256.clone(),
                activation_sha256: activation_sha256.clone(),
                activation_generation: 5,
                target_key_sha256: target_key_sha256.clone(),
                layout_set_sha256: layout_set_sha256.clone(),
                variant_key: "greenhouse_public".to_string(),
                layout_contract_version: 1,
                surface_sha256: SURFACE_SHA256.to_string(),
                adapter_bundle_sha256: adapter_bundle_sha256.clone(),
                runner_target_sha256s: vec![runner_target_sha256.clone()],
                expires_at_ms,
            })
            .unwrap();
        let admission = json!({
            "kind": "track_auto_submit",
            "authorization_id": "auto-auth-604",
            "career_track_id": "track-test",
            "revision_no": 4,
            "authority_fingerprint": "1".repeat(64),
            "ats_certification": certification_admission,
        });
        let approved_checksum = approved_execution_checksum_with_admission(
            3,
            &approved_packet,
            &approved_job,
            &admission,
        )
        .unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 3,
            "approved_at_ms": now_ms - 5_000,
            "checksum": approved_checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        let final_submit_proof = jobs::FinalSubmitProof {
            schema_version: 4,
            adapter: "greenhouse".to_string(),
            adapter_version: GREENHOUSE_SUBMISSION_ADAPTER_VERSION.to_string(),
            control: "greenhouse_submit_application".to_string(),
            job: jobs::FinalSubmitJobProof {
                approved_canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
                page_url: "https://job-boards.greenhouse.io/embed/job_app?for=acme&token=123"
                    .to_string(),
            },
            target: jobs::FinalSubmitTargetProof {
                action_url: "https://job-boards.greenhouse.io/embed/job_app?for=acme&token=123"
                    .to_string(),
                method: "post".to_string(),
                enctype: "multipart/form-data".to_string(),
                form_target: "_self".to_string(),
                provider_job_key: target_key.to_string(),
                form_identity: r#"[0,"application_form","","application-form","","123"]"#
                    .to_string(),
            },
            files: vec![jobs::FinalSubmitFileProof {
                field_name: "resume".to_string(),
                name: format!("resume-{}.pdf", "a".repeat(64)),
                byte_length: 50,
                sha256: "a".repeat(64),
            }],
            fields: vec![jobs::FinalSubmitFieldProof {
                field_name: "job_id".to_string(),
                value_byte_length: 3,
                value_sha256: "d".repeat(64),
            }],
            part_order: vec![
                jobs::FinalSubmitPartOrderProof {
                    kind: "field".to_string(),
                    index: 0,
                },
                jobs::FinalSubmitPartOrderProof {
                    kind: "file".to_string(),
                    index: 0,
                },
            ],
            documents: vec![jobs::FinalSubmitDocumentProof {
                kind: "resume".to_string(),
                version_id: Some("resume-test".to_string()),
                sha256: "a".repeat(64),
            }],
            certification: Some(jobs::AtsFinalSubmitCertificationProof {
                schema_version: 1,
                provider: "greenhouse".to_string(),
                adapter_version: GREENHOUSE_SUBMISSION_ADAPTER_VERSION.to_string(),
                manifest_sha256: manifest_sha256.clone(),
                activation_sha256: activation_sha256.clone(),
                activation_generation: 5,
                target_key_sha256: target_key_sha256.clone(),
                layout_set_sha256: layout_set_sha256.clone(),
                adapter_bundle_sha256: adapter_bundle_sha256.clone(),
                runner_target_sha256s: vec![runner_target_sha256.clone()],
                expires_at_ms,
            }),
            observed_surface: Some(jobs::AtsFinalSubmitObservedSurfaceProof {
                schema_version: 1,
                variant_key: "greenhouse_public".to_string(),
                layout_contract_version: 1,
                surface_sha256: SURFACE_SHA256.to_string(),
            }),
        };
        application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY] =
            serde_json::to_value(final_submit_proof).unwrap();

        let binding_id = "binding-api-receipt-604";
        let application_attempt_id = "attempt-604";
        let nonce_sha256 = "6".repeat(64);
        let runtime = jobs::AtsCertificationRuntimeTarget {
            runtime_kind: "cloud".to_string(),
            runtime_id: "runner-test".to_string(),
            runtime_sha256: runner_target_sha256.clone(),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: "d".repeat(64),
            browser_release_manifest_sha256: None,
            browser_artifact_sha256: None,
            browser_build_descriptor_sha256: None,
            runner_build_id: Some("runner-build-604".to_string()),
            runner_image_sha256: Some("e".repeat(64)),
            playwright_version: "1.55.0".to_string(),
            chromium_revision: "1234567".to_string(),
            chromium_executable_sha256: "f".repeat(64),
        };
        let target_evidence = jobs::AtsCertificationFreshTargetEvidence {
            canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            discovery_provider: "greenhouse".to_string(),
            discovery_target_key: target_key.to_string(),
            discovery_observed_at_ms: now_ms - 10_000,
            original_source_provider: "greenhouse".to_string(),
            original_source_target_key: target_key.to_string(),
            original_source_observed_at_ms: now_ms - 10_000,
        };
        let certification_binding = jobs::AtsCertificationBinding {
            binding_version: 1,
            trust_policy_sha256: "0".repeat(64),
            provider: "greenhouse".to_string(),
            target_key: target_key.to_string(),
            allowed_provider_hosts: vec![
                "boards.greenhouse.io".to_string(),
                "job-boards.greenhouse.io".to_string(),
            ],
            variant_key: "greenhouse_public".to_string(),
            surface_sha256: SURFACE_SHA256.to_string(),
            scope_sha256: "1".repeat(64),
            manifest_sha256: manifest_sha256.clone(),
            certification_id: "certification-604".to_string(),
            manifest_generation: 4,
            activation_sha256: activation_sha256.clone(),
            activation_id: "activation-604".to_string(),
            activation_generation: 5,
            channel: "canary".to_string(),
            channel_sequence: 5,
            channel_head_revision: 5,
            channel_transition_sha256: "b".repeat(64),
            capability: "unattended_submit".to_string(),
            account_allowlist_sha256: Some("a".repeat(64)),
            canary_max_submissions: 10,
            canary_account_cap: 2,
            canary_concurrency_cap: 1,
            canary_daily_side_effect_cap: 2,
            adapter_version: GREENHOUSE_SUBMISSION_ADAPTER_VERSION.to_string(),
            final_submit_control_id: "greenhouse_submit_application".to_string(),
            adapter_bundle_sha256: adapter_bundle_sha256.clone(),
            source_commit: "c".repeat(40),
            layout_contract_version: 1,
            layout_contract_sha256: "5".repeat(64),
            layout_set_sha256: layout_set_sha256.clone(),
            layout_observation_sha256s: vec![layout_observation_sha256.clone()],
            evidence_sha256s: vec!["a".repeat(64)],
            runtime_targets: vec![runtime.clone()],
            selected_runtime: Some(runtime),
            not_before_ms: now_ms - 60_000,
            expires_at_ms,
            last_verified_at_ms: now_ms - 5_000,
        };
        let binding_authority = jobs::AtsApplicationCertificationBindingAuthority {
            schema_version: 1,
            binding_id: binding_id.to_string(),
            account_id: account_id.to_string(),
            application_id: application_id.clone(),
            run_id: run_id.clone(),
            application_attempt_id: application_attempt_id.to_string(),
            browser_session_id: run_id.clone(),
            browser_profile_id: receipt["browserProfileId"].as_str().unwrap().to_string(),
            packet_checksum_sha256: approved_checksum.clone(),
            auto_authorization_id: "auto-auth-604".to_string(),
            auto_authorization_revision: 4,
            auto_authorization_fingerprint_sha256: "1".repeat(64),
            target_evidence,
            nonce_sha256: nonce_sha256.clone(),
            certification: certification_binding,
            requested_expires_at_ms: expires_at_ms,
            created_at_ms: now_ms - 2_000,
            expires_at_ms,
        };
        let mut canonical_binding = serde_json::to_vec(&binding_authority).unwrap();
        canonical_binding.push(b'\n');
        let binding_sha256 = hex::encode(Sha256::digest(&canonical_binding));
        let frozen_certification_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&canonical_binding);

        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-certified-receipt-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let connection = pool.get().unwrap();
        // Seed the exact terminal output of Phase B without rebuilding the
        // separately tested signed import lifecycle. The receipt validator
        // still decodes and hashes the canonical frozen binding from storage.
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_application_ats_certification_bindings (
                   binding_id, binding_sha256, account_id, application_id, run_id, attempt_id,
                   browser_session_id, browser_profile_id, packet_checksum_sha256,
                   auto_authorization_id, auto_authorization_revision,
                   auto_authorization_fingerprint_sha256, provider, target_key,
                   manifest_sha256, activation_sha256, layout_set_sha256,
                   adapter_bundle_sha256, runner_target_sha256, platform, architecture,
                   automation_bundle_sha256, browser_release_manifest_sha256,
                   browser_artifact_sha256, browser_build_descriptor_sha256, runner_build_id,
                   runner_image_sha256, browser_runtime_sha256, chromium_executable_sha256,
                   nonce_sha256, frozen_certification_base64url, layout_observation_sha256,
                   observed_surface_sha256, phase_b_request_id, phase_b_request_sha256,
                   canary_reservation_sha256, metering_reservation_sha256, expires_at_ms,
                   phase, fence, created_at_ms, consumed_at_ms, invalidation_kind,
                   invalidated_at_ms
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                   ?16, ?17, ?18, ?19, ?20, ?21, ?22, NULL, NULL, NULL, ?23, ?24, ?25,
                   ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, 'consumed', 1,
                   ?36, ?37, NULL, NULL
                 )",
                rusqlite::params![
                    binding_id,
                    binding_sha256,
                    account_id,
                    application_id,
                    run_id,
                    application_attempt_id,
                    run_id,
                    binding_authority.browser_profile_id,
                    approved_checksum,
                    "auto-auth-604",
                    4,
                    "1".repeat(64),
                    "greenhouse",
                    target_key,
                    manifest_sha256,
                    activation_sha256,
                    layout_set_sha256,
                    adapter_bundle_sha256,
                    runner_target_sha256,
                    "linux",
                    "x86_64",
                    "d".repeat(64),
                    "runner-build-604",
                    "e".repeat(64),
                    runner_target_sha256,
                    "f".repeat(64),
                    nonce_sha256,
                    frozen_certification_base64url,
                    layout_observation_sha256,
                    SURFACE_SHA256,
                    "phase-b-request-604",
                    "a".repeat(64),
                    "b".repeat(64),
                    metering_reservation_sha256,
                    expires_at_ms,
                    now_ms - 2_000,
                    consumed_at_ms,
                ],
            )
            .unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        drop(connection);

        let recovered = jobs::recover_ats_application_certification(
            &pool,
            &jobs::AtsCertificationRecoveryRequest {
                binding_id: binding_id.to_string(),
                account_id: account_id.to_string(),
                application_id: application_id.clone(),
                run_id: run_id.clone(),
                application_attempt_id: application_attempt_id.to_string(),
                nonce_sha256,
            },
        )
        .unwrap();
        receipt["schemaVersion"] = json!(2);
        receipt["receiptId"] = json!("receipt-certified-604");
        receipt["accountId"] = json!(account_id);
        receipt["applicationId"] = json!(application_id);
        receipt["runId"] = json!(run_id);
        receipt["runner"] = json!("cloud");
        receipt["generatedAt"] = json!(generated_at);
        receipt["finalUrl"] = json!("https://boards.greenhouse.io/acme/jobs/123/confirmation");
        receipt["result"] = json!({
            "status": "submitted",
            "issues": [],
            "submitHttpStatus": 302,
            "confirmationText": "Thank you for applying. Your application was received.",
            "confirmationUrl": "https://boards.greenhouse.io/acme/jobs/123/confirmation",
            "submittedAt": generated_at,
        });
        receipt["events"] = json!([{
            "type": "greenhouse_state_transition",
            "occurredAt": generated_at,
            "detail": {
                "state": "receipt",
                "status": "submitted",
                "capability": "beta_review",
            },
        }]);
        receipt["atsCertifiedReceiptAuthority"] =
            serde_json::to_value(&recovered.ats_certified_receipt_authority).unwrap();
        receipt["packet"]["approvedPacketChecksum"] = json!(approved_checksum);
        receipt["packet"]["approvedExecutionSchemaVersion"] = json!(3);
        receipt["packet"]["approvedExecutionAdmission"] = admission;
        receipt["receiptObject"]["schemaVersion"] = json!(2);
        receipt[jobs::SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"] =
            application.receipt.clone();

        CertifiedReceiptValidationFixture {
            pool,
            application,
            resume,
            receipt,
            verified_objects,
        }
    }

    fn validate_complete_certified_receipt(
        fixture: &CertifiedReceiptValidationFixture,
        receipt: &Value,
    ) -> Result<jobs::AtsCertifiedReceiptAuthority, ApiError> {
        let authority = validate_receipt_ats_certification_authority(
            &fixture.pool,
            &fixture.application,
            receipt,
            "acct-test",
            "app-test",
            "run-test",
            "cloud",
        )?;
        let claim_ids = frozen_submission_claim_ids(&fixture.application)?;
        validate_receipt_verified_claim_ids(receipt, &claim_ids)?;
        validate_provider_submission_proof(&fixture.application, receipt)?;
        validate_receipt_final_submit_proof(&fixture.application, receipt)?;
        validate_receipt_bundle(
            "acct-test",
            &fixture.application,
            &fixture.resume,
            receipt,
            &fixture.verified_objects,
            true,
        )?;
        Ok(authority)
    }

    #[test]
    fn schema_four_certified_receipt_requires_exact_terminal_and_evidence_authority() {
        let fixture = certified_receipt_validation_fixture();
        let proof = jobs::stored_final_submit_proof(&fixture.application).unwrap();
        assert_eq!(proof.schema_version, 4);
        assert_eq!(fixture.receipt["schemaVersion"], json!(2));
        let exact = validate_complete_certified_receipt(&fixture, &fixture.receipt).unwrap();
        assert_eq!(
            serde_json::to_value(exact).unwrap(),
            fixture.receipt["atsCertifiedReceiptAuthority"]
        );

        let mut wrong_document = fixture.receipt.clone();
        wrong_document["documents"][0]["sha256"] = json!("f".repeat(64));
        let document_error =
            validate_complete_certified_receipt(&fixture, &wrong_document).unwrap_err();
        assert_eq!(document_error.0, StatusCode::BAD_REQUEST);
        assert!(document_error.1.contains("pre-click proof"));

        let mut wrong_screenshot = fixture.receipt.clone();
        wrong_screenshot["screenshotKeys"][0] =
            json!("accounts/acct-test/jobs/app-test/different-confirmation.png");
        let screenshot_error =
            validate_complete_certified_receipt(&fixture, &wrong_screenshot).unwrap_err();
        assert_eq!(screenshot_error.0, StatusCode::BAD_REQUEST);
        assert!(screenshot_error.1.contains("manifest"));

        let mut wrong_confirmation = fixture.receipt.clone();
        wrong_confirmation["result"]["confirmationUrl"] =
            json!("https://boards.greenhouse.io/acme/jobs/other/confirmation");
        let confirmation_error =
            validate_complete_certified_receipt(&fixture, &wrong_confirmation).unwrap_err();
        assert_eq!(confirmation_error.0, StatusCode::BAD_REQUEST);
        assert!(confirmation_error.1.contains("approved provider"));

        for (field, value) in [
            ("applicationAttemptId", json!("attempt-other")),
            ("meteringReservationSha256", json!("e".repeat(64))),
        ] {
            let mut mismatch = fixture.receipt.clone();
            mismatch["atsCertifiedReceiptAuthority"][field] = value;
            let terminal_error =
                validate_complete_certified_receipt(&fixture, &mismatch).unwrap_err();
            assert_eq!(terminal_error.0, StatusCode::CONFLICT, "{field}");
            assert!(terminal_error.1.contains("exact terminal server record"));
        }
    }
}

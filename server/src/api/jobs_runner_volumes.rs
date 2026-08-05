//! Authenticated HTTP boundary for managed Bluey Jobs runner volumes.
//!
//! Fleet HMAC authentication protects transport. Durable Ed25519 volume proofs
//! independently bind every post-enrollment operation to the current key epoch.

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::{
    api::{jobs_worker_auth::JobsWorkerIdentity, AppState},
    auth::AuthedAccount,
    db::jobs::{
        self, EnrollRunnerVolumeRequest, NewRunnerVolumeAdmissionGrant,
        PollRunnerVolumePurgeCommandsRequest, RecordRunnerLegacyInventoryAuthorityRequest,
        RecordRunnerVolumeDestructionRequest, RecordRunnerVolumeFleetCutoverRequest,
        RecordRunnerVolumeResidencyRequest, ResolveRunnerPurgeLegacyRequest, RunnerPurgeAck,
        RunnerPurgeCommand, RunnerPurgeCommandKeyRing, RunnerPurgeRequestStatus, RunnerPurgeSigner,
        RunnerVolumeAuthorityProof, RunnerVolumeFleetStatus, RunnerVolumeInstanceLease,
        RunnerVolumePurgeError, RunnerVolumeRecord, RunnerVolumeResidencyRecord,
        RunnerVolumeWriteDisposition,
    },
};

type ApiError = (StatusCode, String);

const RUNNER_VOLUME_SIGNING_KEY_ID_ENV: &str = "BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID";
const RUNNER_VOLUME_SIGNING_KEY_ENV: &str = "BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY";
const RUNNER_VOLUME_MINIMUM_BUILD_ENV: &str = "BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID";
const RUNNER_VOLUME_VERIFYING_KEYS_ENV: &str = "BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON";
const RUNNER_VOLUME_HTTP_PAYLOAD_DOMAIN: &str = "bluey-jobs-runner-volume-http-payload-v1";
const RUNNER_VOLUME_EXECUTION_LEASE_CLAIM_PATH: &str = "/api/jobs/internal/execution-leases/claim";
pub(crate) const RUNNER_VOLUME_AUTHORITY_MAX_CLOCK_SKEW_MS: i64 = 90_000;
const RUNNER_VOLUME_ENROLLMENT_MAX_AGE_MS: i64 = 300_000;
const RUNNER_VOLUME_INSTANCE_LEASE_TTL_MS: i64 = 120_000;
const RUNNER_VOLUME_ADMISSION_GRANT_TTL_MS: i64 = 600_000;
const MAX_RUNNER_VOLUME_POLL_COMMANDS: usize = 100;
const RUNNER_VOLUME_BODY_LIMIT_BYTES: usize = 64 * 1024;
const MAX_RUNNER_VOLUME_VERIFYING_KEYS: usize = 16;

#[derive(Clone)]
pub(crate) struct RunnerVolumePurgePolicy {
    pub(crate) signer: RunnerPurgeSigner,
    pub(crate) key_ring: RunnerPurgeCommandKeyRing,
    pub(crate) minimum_runner_build_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfiguredRunnerVolumeVerifyingKey {
    key_id: String,
    public_key_base64url: String,
}

/// Load the server-owned runner purge policy without defaults or secret logs.
///
/// Account deletion uses this same helper so command fan-out and runner polling
/// cannot silently diverge on signing authority or the exact accepted build.
pub(crate) fn runner_volume_purge_policy() -> Result<RunnerVolumePurgePolicy, StatusCode> {
    let key_id = required_policy_value(RUNNER_VOLUME_SIGNING_KEY_ID_ENV)?;
    let encoded_seed = required_policy_value(RUNNER_VOLUME_SIGNING_KEY_ENV)?;
    let minimum_runner_build_id = required_policy_value(RUNNER_VOLUME_MINIMUM_BUILD_ENV)?;
    let verifying_keys_json = required_policy_value(RUNNER_VOLUME_VERIFYING_KEYS_ENV)?;
    parse_runner_volume_purge_policy(
        &key_id,
        &encoded_seed,
        &minimum_runner_build_id,
        &verifying_keys_json,
    )
    .map_err(|()| {
        tracing::error!(
            "runner-volume purge policy is invalid; managed volume operations fail closed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })
}

/// Fail startup when account deletion could create a durable fence but could
/// not sign the mandatory managed runner-volume purge commands that follow.
pub fn validate_runtime_config() -> anyhow::Result<()> {
    runner_volume_purge_policy()
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("invalid managed runner-volume purge signing policy"))
}

fn required_policy_value(name: &str) -> Result<String, StatusCode> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty() && value.trim() == value)
        .ok_or_else(|| {
            tracing::error!(
                variable = name,
                "runner-volume purge policy is unavailable; managed volume operations fail closed"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })
}

fn parse_runner_volume_purge_policy(
    key_id: &str,
    encoded_seed: &str,
    minimum_runner_build_id: &str,
    verifying_keys_json: &str,
) -> Result<RunnerVolumePurgePolicy, ()> {
    if !valid_policy_identifier(key_id)
        || !jobs::runner_build_id_is_canonical(minimum_runner_build_id)
    {
        return Err(());
    }
    let seed = URL_SAFE_NO_PAD
        .decode(encoded_seed.as_bytes())
        .map_err(|_| ())?;
    if seed.len() != 32 || URL_SAFE_NO_PAD.encode(&seed) != encoded_seed {
        return Err(());
    }
    let seed: [u8; 32] = seed.try_into().map_err(|_| ())?;
    let signer = RunnerPurgeSigner::from_seed(key_id, seed).map_err(|_| ())?;
    let configured_keys: Vec<ConfiguredRunnerVolumeVerifyingKey> =
        serde_json::from_str(verifying_keys_json).map_err(|_| ())?;
    if configured_keys.is_empty() || configured_keys.len() > MAX_RUNNER_VOLUME_VERIFYING_KEYS {
        return Err(());
    }
    let mut server_command_keys = BTreeMap::new();
    for configured in configured_keys {
        if !valid_policy_identifier(&configured.key_id) {
            return Err(());
        }
        let public_key = URL_SAFE_NO_PAD
            .decode(configured.public_key_base64url.as_bytes())
            .map_err(|_| ())?;
        if public_key.len() != 32
            || URL_SAFE_NO_PAD.encode(&public_key) != configured.public_key_base64url
            || server_command_keys
                .insert(configured.key_id, configured.public_key_base64url)
                .is_some()
        {
            return Err(());
        }
    }
    let key_ring = RunnerPurgeCommandKeyRing::new(&signer, server_command_keys).map_err(|_| ())?;
    Ok(RunnerVolumePurgePolicy {
        signer,
        key_ring,
        minimum_runner_build_id: minimum_runner_build_id.to_string(),
    })
}

fn valid_policy_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'+' | b'-')
        })
}

pub fn worker_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/runner-volumes/enroll",
            post(enroll_runner_volume),
        )
        .route(
            "/api/jobs/internal/runner-volumes/:volume_id/instances/claim",
            post(claim_runner_volume_instance),
        )
        .route(
            "/api/jobs/internal/runner-volumes/:volume_id/instances/heartbeat",
            post(heartbeat_runner_volume_instance),
        )
        .route(
            "/api/jobs/internal/runner-volumes/:volume_id/storage-attestations",
            post(record_runner_volume_storage_attestation),
        )
        .route(
            "/api/jobs/internal/runner-volumes/:volume_id/residencies",
            post(record_runner_volume_residency),
        )
        .route(
            "/api/jobs/internal/runner-volumes/:volume_id/commands/poll",
            post(poll_runner_volume_commands),
        )
        .route(
            "/api/jobs/internal/runner-volumes/:volume_id/commands/:command_id/ack",
            post(acknowledge_runner_volume_command),
        )
        .layer(DefaultBodyLimit::max(RUNNER_VOLUME_BODY_LIMIT_BYTES))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/runner-volumes/admission-grants",
            post(create_runner_volume_admission_grant),
        )
        .route(
            "/admin/jobs/runner-volumes/:volume_id/epochs/:enrollment_epoch/destructions",
            post(record_runner_volume_destruction),
        )
        .route(
            "/admin/jobs/runner-volumes/purge-requests/:request_id/legacy-resolution",
            post(resolve_runner_volume_legacy),
        )
        .route(
            "/admin/jobs/runner-volumes/fleet/status",
            get(runner_volume_fleet_status),
        )
        .route(
            "/admin/jobs/runner-volumes/fleet/legacy-inventory-authority",
            post(record_runner_legacy_inventory_authority),
        )
        .route(
            "/admin/jobs/runner-volumes/fleet/cutover",
            post(record_runner_volume_fleet_cutover),
        )
        .layer(DefaultBodyLimit::max(RUNNER_VOLUME_BODY_LIMIT_BYTES))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnrollRunnerVolumeHttpRequest {
    grant_token: String,
    proof: jobs::RunnerVolumeEnrollmentProof,
}

async fn enroll_runner_volume(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Json(request): Json<EnrollRunnerVolumeHttpRequest>,
) -> Result<Json<RunnerVolumeRecord>, ApiError> {
    require_runner_volume_worker(&worker)?;
    // Refuse enrollment when the server could not subsequently issue purge
    // commands for the newly persistent authority.
    let _policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    if request.proof.worker_id != worker.worker_id {
        return Err(unauthorized());
    }
    let now_ms = server_now_ms();
    if !runner_volume_enrollment_proof_is_fresh(&request.proof, now_ms) {
        let stored = jobs::lookup_runner_volume(&state.pool, &request.proof.volume_id)
            .map_err(runner_volume_api_error)?;
        if !runner_volume_enrollment_request_allowed(&request.proof, stored.as_ref(), now_ms) {
            return Err(unauthorized());
        }
    }
    jobs::enroll_runner_volume(
        &state.pool,
        &EnrollRunnerVolumeRequest {
            grant_token: request.grant_token,
            proof: request.proof,
            enrolled_at_ms: now_ms,
        },
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

fn runner_volume_enrollment_proof_is_fresh(
    proof: &jobs::RunnerVolumeEnrollmentProof,
    now_ms: i64,
) -> bool {
    proof.requested_at_ms <= now_ms
        && now_ms.saturating_sub(proof.requested_at_ms) <= RUNNER_VOLUME_ENROLLMENT_MAX_AGE_MS
}

fn runner_volume_enrollment_request_allowed(
    proof: &jobs::RunnerVolumeEnrollmentProof,
    stored: Option<&RunnerVolumeRecord>,
    now_ms: i64,
) -> bool {
    runner_volume_enrollment_proof_is_fresh(proof, now_ms)
        || stored.is_some_and(|volume| exact_runner_volume_enrollment_replay(volume, proof))
}

/// Match only the durable immutable identity. The DB verifies the signature and
/// repeats this comparison under its enrollment lock before returning replay.
fn exact_runner_volume_enrollment_replay(
    stored: &RunnerVolumeRecord,
    proof: &jobs::RunnerVolumeEnrollmentProof,
) -> bool {
    stored.volume_id == proof.volume_id
        && stored.worker_id == proof.worker_id
        && stored.provider == proof.provider
        && stored.provider_resource_id == proof.provider_resource_id
        && stored.resource_fingerprint == proof.resource_fingerprint
        && stored.current_epoch == proof.enrollment_epoch
        && stored.admission_grant_id == proof.admission_grant_id
        && stored.legacy_artifact_count == proof.legacy_artifact_count
        && stored.public_key_base64url == proof.public_key_base64url
        && stored.key_fingerprint == proof.key_fingerprint
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunnerVolumeAuthorityHttpRequest {
    proof: RunnerVolumeAuthorityProof,
}

async fn claim_runner_volume_instance(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(volume_id): Path<String>,
    Json(request): Json<RunnerVolumeAuthorityHttpRequest>,
) -> Result<Json<RunnerVolumeInstanceLease>, ApiError> {
    let _policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/instances/claim");
    let now_ms = server_now_ms();
    let authority = verify_runner_volume_authority(
        &state,
        &worker,
        &volume_id,
        &request.proof,
        RunnerVolumeAuthorityHttpBinding {
            expected_operation: "instance_claim",
            path: &path,
            payload_fields: &[],
            now_ms,
        },
    )?;
    let lease_expires_at_ms = now_ms
        .checked_add(RUNNER_VOLUME_INSTANCE_LEASE_TTL_MS)
        .ok_or_else(internal_error)?;
    jobs::claim_runner_volume_instance_authorized(
        &state.pool,
        &volume_id,
        request.proof.enrollment_epoch,
        &request.proof.process_instance_id,
        now_ms,
        lease_expires_at_ms,
        &authority,
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

async fn heartbeat_runner_volume_instance(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(volume_id): Path<String>,
    Json(request): Json<RunnerVolumeAuthorityHttpRequest>,
) -> Result<Json<RunnerVolumeInstanceLease>, ApiError> {
    let _policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/instances/heartbeat");
    let now_ms = server_now_ms();
    let authority = verify_runner_volume_authority(
        &state,
        &worker,
        &volume_id,
        &request.proof,
        RunnerVolumeAuthorityHttpBinding {
            expected_operation: "instance_heartbeat",
            path: &path,
            payload_fields: &[],
            now_ms,
        },
    )?;
    let lease_expires_at_ms = now_ms
        .checked_add(RUNNER_VOLUME_INSTANCE_LEASE_TTL_MS)
        .ok_or_else(internal_error)?;
    jobs::heartbeat_runner_volume_instance_authorized(
        &state.pool,
        &volume_id,
        request.proof.enrollment_epoch,
        &request.proof.process_instance_id,
        now_ms,
        lease_expires_at_ms,
        &authority,
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordRunnerVolumeStorageAttestationHttpRequest {
    proof: RunnerVolumeAuthorityProof,
    attestation: jobs::RunnerVolumeStorageAttestation,
}

async fn record_runner_volume_storage_attestation(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(volume_id): Path<String>,
    Json(request): Json<RecordRunnerVolumeStorageAttestationHttpRequest>,
) -> Result<Json<jobs::RunnerVolumeStorageAttestationOutcome>, ApiError> {
    let policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    if request.attestation.volume_id != volume_id
        || request.attestation.enrollment_epoch != request.proof.enrollment_epoch
        || request.attestation.process_instance_id != request.proof.process_instance_id
    {
        return Err(bad_request(
            "Runner-volume storage attestation does not match its authority path.",
        ));
    }
    let attestation_sha256 = request
        .attestation
        .attestation_sha256()
        .map_err(runner_volume_api_error)?;
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/storage-attestations");
    let now_ms = server_now_ms();
    let authority = verify_runner_volume_authority(
        &state,
        &worker,
        &volume_id,
        &request.proof,
        RunnerVolumeAuthorityHttpBinding {
            expected_operation: "storage_attestation",
            path: &path,
            payload_fields: &[("attestation_sha256", attestation_sha256.as_str())],
            now_ms,
        },
    )?;
    jobs::record_runner_volume_storage_attestation(
        &state.pool,
        &request.attestation,
        &policy.minimum_runner_build_id,
        now_ms,
        &authority,
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordRunnerVolumeResidencyHttpRequest {
    proof: RunnerVolumeAuthorityProof,
    purge_subject: String,
}

async fn record_runner_volume_residency(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(volume_id): Path<String>,
    Json(request): Json<RecordRunnerVolumeResidencyHttpRequest>,
) -> Result<Json<RunnerVolumeResidencyRecord>, ApiError> {
    let _policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/residencies");
    let now_ms = server_now_ms();
    let authority = verify_runner_volume_authority(
        &state,
        &worker,
        &volume_id,
        &request.proof,
        RunnerVolumeAuthorityHttpBinding {
            expected_operation: "residency_bind",
            path: &path,
            payload_fields: &[("purge_subject", request.purge_subject.as_str())],
            now_ms,
        },
    )?;
    jobs::record_runner_volume_residency(
        &state.pool,
        &RecordRunnerVolumeResidencyRequest {
            purge_subject: request.purge_subject,
            volume_id,
            enrollment_epoch: request.proof.enrollment_epoch,
            process_instance_id: request.proof.process_instance_id,
            now_ms,
        },
        &authority,
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PollRunnerVolumeCommandsHttpRequest {
    after_command_id: Option<String>,
    proof: RunnerVolumeAuthorityProof,
    limit: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PollRunnerVolumeCommandsHttpResponse {
    commands: Vec<RunnerPurgeCommand>,
    next_command_cursor: Option<String>,
    ready: bool,
    enrollment_generation: i64,
    required_tombstone_generation: i64,
    reconciled_tombstone_generation: i64,
    predecessor_attestation_generation: i64,
    predecessor_attestation_sha256: String,
    storage_attestation_required: bool,
    server_command_keys: BTreeMap<String, String>,
}

async fn poll_runner_volume_commands(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(volume_id): Path<String>,
    Json(request): Json<PollRunnerVolumeCommandsHttpRequest>,
) -> Result<Json<PollRunnerVolumeCommandsHttpResponse>, ApiError> {
    if request.limit == 0 || request.limit > MAX_RUNNER_VOLUME_POLL_COMMANDS {
        return Err(bad_request("Invalid runner-volume command poll limit."));
    }
    let policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/commands/poll");
    let after_command_id = request.after_command_id.as_deref().unwrap_or("");
    let cursor_poll = request.after_command_id.is_some();
    let limit = request.limit.to_string();
    let now_ms = server_now_ms();
    let authority = verify_runner_volume_authority(
        &state,
        &worker,
        &volume_id,
        &request.proof,
        RunnerVolumeAuthorityHttpBinding {
            expected_operation: "purge_poll",
            path: &path,
            payload_fields: &[
                ("after_command_id", after_command_id),
                ("limit", limit.as_str()),
            ],
            now_ms,
        },
    )?;
    let commands = jobs::poll_runner_volume_purge_commands_authorized(
        &state.pool,
        &policy.signer,
        &policy.key_ring,
        &PollRunnerVolumePurgeCommandsRequest {
            volume_id: volume_id.clone(),
            enrollment_epoch: request.proof.enrollment_epoch,
            process_instance_id: request.proof.process_instance_id.clone(),
            minimum_runner_build_id: policy.minimum_runner_build_id,
            now_ms,
            limit: request.limit,
            after_command_id: request.after_command_id,
        },
        &authority,
    )
    .map_err(runner_volume_api_error)?;

    let current = jobs::lookup_runner_volume(&state.pool, &volume_id)
        .map_err(runner_volume_api_error)?
        .ok_or_else(|| not_found("Runner volume not found."))?;
    let predecessor = jobs::runner_volume_storage_attestation_predecessor(
        &state.pool,
        &volume_id,
        request.proof.enrollment_epoch,
        &request.proof.process_instance_id,
        now_ms,
    )
    .map_err(runner_volume_api_error)?;
    let ready = if poll_may_check_readiness(cursor_poll, commands.commands.len()) {
        match jobs::require_active_reconciled_runner_volume(
            &state.pool,
            &volume_id,
            request.proof.enrollment_epoch,
            &request.proof.process_instance_id,
            now_ms,
        ) {
            Ok(_) => true,
            Err(
                RunnerVolumePurgeError::NotReady
                | RunnerVolumePurgeError::Conflict
                | RunnerVolumePurgeError::Unauthorized,
            ) => false,
            Err(error) => return Err(runner_volume_api_error(error)),
        }
    } else {
        false
    };
    Ok(Json(PollRunnerVolumeCommandsHttpResponse {
        commands: commands.commands,
        next_command_cursor: commands.next_after_command_id,
        ready,
        enrollment_generation: current.enrollment_generation,
        required_tombstone_generation: current.required_tombstone_generation,
        reconciled_tombstone_generation: current.reconciled_tombstone_generation,
        predecessor_attestation_generation: predecessor.predecessor_attestation_generation,
        predecessor_attestation_sha256: predecessor.predecessor_attestation_sha256,
        storage_attestation_required: predecessor.storage_attestation_required,
        server_command_keys: policy.key_ring.public_keys_by_id().clone(),
    }))
}

fn poll_may_check_readiness(cursor_poll: bool, command_count: usize) -> bool {
    !cursor_poll && command_count == 0
}

async fn acknowledge_runner_volume_command(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path((volume_id, command_id)): Path<(String, String)>,
    Json(ack): Json<RunnerPurgeAck>,
) -> Result<Json<RunnerPurgeAckHttpResponse>, ApiError> {
    require_runner_volume_worker(&worker)?;
    let policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    if ack.target_volume_id != volume_id || ack.command_id != command_id {
        return Err(bad_request(
            "Runner-volume command path does not match the acknowledgement.",
        ));
    }
    if !jobs::runner_build_id_is_canonical(&ack.runner_build_id) {
        return Err(bad_request("Runner build identifier is malformed."));
    }
    let volume = jobs::lookup_runner_volume(&state.pool, &volume_id)
        .map_err(runner_volume_api_error)?
        .ok_or_else(|| not_found("Runner volume not found."))?;
    // A committed acknowledgement may be replayed after the volume advances
    // to a successor key epoch. The database verifies the immutable historical
    // key and permits only an exact stored replay; new historical-epoch writes
    // still fail its locked live-state checks.
    if volume.worker_id != worker.worker_id {
        return Err(unauthorized());
    }
    jobs::acknowledge_runner_volume_purge(&state.pool, &policy.key_ring, &ack, server_now_ms())
        .map(RunnerPurgeAckHttpResponse::from)
        .map(Json)
        .map_err(runner_volume_api_error)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunnerPurgeAckHttpResponse {
    disposition: RunnerVolumeWriteDisposition,
    status: RunnerPurgeAckSafeStatus,
    reconciled_tombstone_generation: i64,
}

/// Purge-control responses intentionally exclude direct or correlating account
/// material. The runner needs only progress state and aggregate counters.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunnerPurgeAckSafeStatus {
    purge_generation: i64,
    state: String,
    legacy_unresolved_count: i64,
    required_target_count: i64,
    resolved_target_count: i64,
}

impl From<jobs::RunnerPurgeAckOutcome> for RunnerPurgeAckHttpResponse {
    fn from(outcome: jobs::RunnerPurgeAckOutcome) -> Self {
        Self {
            disposition: outcome.disposition,
            status: RunnerPurgeAckSafeStatus {
                purge_generation: outcome.status.purge_generation,
                state: outcome.status.state,
                legacy_unresolved_count: outcome.status.legacy_unresolved_count,
                required_target_count: outcome.status.required_target_count,
                resolved_target_count: outcome.status.resolved_target_count,
            },
            reconciled_tombstone_generation: outcome.reconciled_tombstone_generation,
        }
    }
}

struct RunnerVolumeAuthorityHttpBinding<'a> {
    expected_operation: &'a str,
    path: &'a str,
    payload_fields: &'a [(&'a str, &'a str)],
    now_ms: i64,
}

fn verify_runner_volume_authority(
    state: &AppState,
    worker: &JobsWorkerIdentity,
    path_volume_id: &str,
    proof: &RunnerVolumeAuthorityProof,
    binding: RunnerVolumeAuthorityHttpBinding<'_>,
) -> Result<jobs::VerifiedRunnerVolumeAuthority, ApiError> {
    require_runner_volume_worker(worker)?;
    if proof.volume_id != path_volume_id {
        return Err(bad_request(
            "Runner-volume path does not match the signed volume identity.",
        ));
    }
    let payload_sha256 =
        runner_volume_http_payload_sha256(binding.path, &worker.worker_id, binding.payload_fields);
    let authority = jobs::verify_runner_volume_authority_proof(
        &state.pool,
        proof,
        binding.expected_operation,
        &payload_sha256,
        binding.now_ms,
        RUNNER_VOLUME_AUTHORITY_MAX_CLOCK_SKEW_MS,
    )
    .map_err(runner_volume_api_error)?;
    if authority.volume().worker_id != worker.worker_id {
        return Err(unauthorized());
    }
    Ok(authority)
}

pub(crate) fn runner_volume_http_payload_sha256(
    path: &str,
    worker_id: &str,
    payload_fields: &[(&str, &str)],
) -> String {
    canonical_http_payload_sha256(
        RUNNER_VOLUME_HTTP_PAYLOAD_DOMAIN,
        path,
        worker_id,
        payload_fields,
    )
}

pub(crate) struct RunnerVolumeExecutionLeaseClaimPayload<'a> {
    pub(crate) worker_id: &'a str,
    pub(crate) account_id: &'a str,
    pub(crate) application_id: &'a str,
    pub(crate) run_id: &'a str,
    pub(crate) browser_profile_id: &'a str,
    pub(crate) owner_id: &'a str,
    pub(crate) volume_id: &'a str,
    pub(crate) enrollment_epoch: i64,
    pub(crate) process_instance_id: &'a str,
}

pub(crate) fn runner_volume_execution_lease_claim_payload_sha256(
    payload: &RunnerVolumeExecutionLeaseClaimPayload<'_>,
) -> String {
    let enrollment_epoch = payload.enrollment_epoch.to_string();
    runner_volume_http_payload_sha256(
        RUNNER_VOLUME_EXECUTION_LEASE_CLAIM_PATH,
        payload.worker_id,
        &[
            ("account_id", payload.account_id),
            ("application_id", payload.application_id),
            ("run_id", payload.run_id),
            ("browser_profile_id", payload.browser_profile_id),
            ("owner_id", payload.owner_id),
            ("volume_id", payload.volume_id),
            ("enrollment_epoch", enrollment_epoch.as_str()),
            ("process_instance_id", payload.process_instance_id),
        ],
    )
}

fn canonical_http_payload_sha256(
    domain: &str,
    path: &str,
    worker_id: &str,
    payload_fields: &[(&str, &str)],
) -> String {
    let mut canonical = format!("{domain}\nmethod=POST\npath={path}\nworker_id={worker_id}\n");
    for (name, value) in payload_fields {
        canonical.push_str(name);
        canonical.push('=');
        canonical.push_str(value);
        canonical.push('\n');
    }
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn require_runner_volume_worker(worker: &JobsWorkerIdentity) -> Result<(), ApiError> {
    if worker.scope != "runner-volume" {
        return Err(unauthorized());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateRunnerVolumeAdmissionGrantHttpRequest {
    expected_worker_id: String,
    provider: String,
    provider_resource_id: String,
    resource_fingerprint: String,
    authorization_ref: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRunnerVolumeAdmissionGrantHttpResponse {
    grant_id: String,
    grant_token: String,
    expected_worker_id: String,
    provider: String,
    provider_resource_id: String,
    resource_fingerprint: String,
    authorization_ref: String,
    issued_fleet_generation: i64,
    expires_at_ms: i64,
    created_at_ms: i64,
}

async fn create_runner_volume_admission_grant(
    State(state): State<AppState>,
    Extension(AuthedAccount(admin)): Extension<AuthedAccount>,
    Json(request): Json<CreateRunnerVolumeAdmissionGrantHttpRequest>,
) -> Result<Json<CreateRunnerVolumeAdmissionGrantHttpResponse>, ApiError> {
    let _policy = runner_volume_purge_policy().map_err(policy_api_error)?;
    let created_at_ms = server_now_ms();
    let expires_at_ms = created_at_ms
        .checked_add(RUNNER_VOLUME_ADMISSION_GRANT_TTL_MS)
        .ok_or_else(internal_error)?;
    let grant_id = format!("runner-volume-grant-{}", uuid::Uuid::new_v4());
    let mut token = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut token);
    let grant_token = URL_SAFE_NO_PAD.encode(token);
    let stored = jobs::create_runner_volume_admission_grant(
        &state.pool,
        &NewRunnerVolumeAdmissionGrant {
            grant_id: grant_id.clone(),
            token: grant_token.clone(),
            expected_worker_id: request.expected_worker_id.clone(),
            provider: request.provider.clone(),
            provider_resource_id: request.provider_resource_id.clone(),
            resource_fingerprint: request.resource_fingerprint.clone(),
            authorization_ref: request.authorization_ref.clone(),
            created_by: admin.id,
            expires_at_ms,
            created_at_ms,
        },
    )
    .map_err(runner_volume_api_error)?;
    Ok(Json(CreateRunnerVolumeAdmissionGrantHttpResponse {
        grant_id,
        grant_token,
        expected_worker_id: request.expected_worker_id,
        provider: request.provider,
        provider_resource_id: request.provider_resource_id,
        resource_fingerprint: request.resource_fingerprint,
        authorization_ref: request.authorization_ref,
        issued_fleet_generation: stored.issued_fleet_generation,
        expires_at_ms,
        created_at_ms,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordRunnerVolumeDestructionHttpRequest {
    expected_enrollment_generation: i64,
    expected_volume_key_fingerprint: String,
    expected_provider: String,
    expected_provider_resource_id: String,
    expected_resource_fingerprint: String,
    evidence_type: String,
    snapshot_inventory_sha256: String,
    evidence_sha256: String,
    authorization_ref: String,
    occurred_at_ms: i64,
    details: Value,
}

async fn record_runner_volume_destruction(
    State(state): State<AppState>,
    Extension(AuthedAccount(admin)): Extension<AuthedAccount>,
    Path((volume_id, enrollment_epoch)): Path<(String, i64)>,
    Json(request): Json<RecordRunnerVolumeDestructionHttpRequest>,
) -> Result<Json<jobs::RunnerVolumeDestructionRecord>, ApiError> {
    let volume = jobs::lookup_runner_volume(&state.pool, &volume_id)
        .map_err(runner_volume_api_error)?
        .ok_or_else(|| not_found("Runner volume not found."))?;
    if volume.current_epoch != enrollment_epoch {
        return Err(conflict(
            "Runner-volume enrollment epoch is no longer current.",
        ));
    }
    let fleet = jobs::runner_volume_fleet_status(&state.pool).map_err(runner_volume_api_error)?;
    if request.expected_enrollment_generation != fleet.enrollment_generation
        || request.expected_volume_key_fingerprint != volume.key_fingerprint
        || request.expected_provider != volume.provider
        || request.expected_provider_resource_id != volume.provider_resource_id
        || request.expected_resource_fingerprint != volume.resource_fingerprint
    {
        return Err(conflict(
            "Runner-volume destruction evidence snapshot changed.",
        ));
    }
    let recorded_at_ms = server_now_ms();
    if request.occurred_at_ms < 0 || request.occurred_at_ms > recorded_at_ms {
        return Err(bad_request(
            "Runner-volume destruction occurrence time is invalid.",
        ));
    }
    jobs::record_runner_volume_destruction(
        &state.pool,
        &RecordRunnerVolumeDestructionRequest {
            destruction_id: format!("runner-volume-destruction-{}", uuid::Uuid::new_v4()),
            expected_enrollment_generation: request.expected_enrollment_generation,
            volume_id,
            volume_epoch: enrollment_epoch,
            volume_key_fingerprint: request.expected_volume_key_fingerprint,
            provider: request.expected_provider,
            provider_resource_id: request.expected_provider_resource_id,
            resource_fingerprint: request.expected_resource_fingerprint,
            evidence_type: request.evidence_type,
            snapshot_inventory_sha256: request.snapshot_inventory_sha256,
            evidence_sha256: request.evidence_sha256,
            authorization_ref: request.authorization_ref,
            authorized_by: admin.id,
            occurred_at_ms: request.occurred_at_ms,
            recorded_at_ms,
            details: request.details,
        },
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveRunnerVolumeLegacyHttpRequest {
    expected_legacy_unresolved_count: i64,
    resolution_ref: String,
    resolution_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRunnerVolumeLegacyHttpResponse {
    disposition: RunnerVolumeWriteDisposition,
    status: RunnerPurgeRequestStatus,
}

async fn resolve_runner_volume_legacy(
    State(state): State<AppState>,
    Extension(AuthedAccount(admin)): Extension<AuthedAccount>,
    Path(request_id): Path<String>,
    Json(request): Json<ResolveRunnerVolumeLegacyHttpRequest>,
) -> Result<Json<ResolveRunnerVolumeLegacyHttpResponse>, ApiError> {
    let (disposition, status) = jobs::resolve_runner_volume_purge_legacy(
        &state.pool,
        &ResolveRunnerPurgeLegacyRequest {
            request_id,
            expected_legacy_unresolved_count: request.expected_legacy_unresolved_count,
            resolution_ref: request.resolution_ref,
            resolution_sha256: request.resolution_sha256,
            resolved_by: admin.id,
            resolved_at_ms: server_now_ms(),
        },
    )
    .map_err(runner_volume_api_error)?;
    Ok(Json(ResolveRunnerVolumeLegacyHttpResponse {
        disposition,
        status,
    }))
}

async fn runner_volume_fleet_status(
    State(state): State<AppState>,
) -> Result<Json<RunnerVolumeFleetStatus>, ApiError> {
    jobs::runner_volume_fleet_status(&state.pool)
        .map(Json)
        .map_err(runner_volume_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordRunnerLegacyInventoryAuthorityHttpRequest {
    reconciliation_id: String,
    authority_state: String,
    expected_predecessor_generation: i64,
    expected_predecessor_authority_id: Option<String>,
    expected_predecessor_authority_sha256: Option<String>,
    root_count: i64,
    root_set_sha256: String,
    scope_ref: String,
    evidence_ref: String,
    evidence_sha256: String,
}

async fn record_runner_legacy_inventory_authority(
    State(state): State<AppState>,
    Extension(AuthedAccount(admin)): Extension<AuthedAccount>,
    Json(request): Json<RecordRunnerLegacyInventoryAuthorityHttpRequest>,
) -> Result<Json<jobs::RunnerLegacyInventoryAuthorityOutcome>, ApiError> {
    jobs::record_runner_legacy_inventory_authority(
        &state.pool,
        &RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: request.reconciliation_id,
            authority_state: request.authority_state,
            expected_predecessor_generation: request.expected_predecessor_generation,
            expected_predecessor_authority_id: request.expected_predecessor_authority_id,
            expected_predecessor_authority_sha256: request.expected_predecessor_authority_sha256,
            root_count: request.root_count,
            root_set_sha256: request.root_set_sha256,
            scope_ref: request.scope_ref,
            evidence_ref: request.evidence_ref,
            evidence_sha256: request.evidence_sha256,
            authorized_by: admin.id,
            recorded_at_ms: server_now_ms(),
        },
    )
    .map(Json)
    .map_err(runner_volume_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordRunnerVolumeFleetCutoverHttpRequest {
    cutover_state: String,
    expected_enrollment_generation: i64,
    expected_purge_generation: i64,
    expected_tombstone_generation: i64,
    expected_destruction_generation: i64,
    expected_legacy_reconciliation_generation: i64,
    expected_storage_attestation_generation: i64,
    expected_storage_attestation_count: i64,
    expected_storage_attestation_set_sha256: String,
    expected_legacy_inventory_generation: i64,
    expected_legacy_inventory_reconciliation_id: String,
    expected_legacy_inventory_authority_id: String,
    expected_legacy_inventory_authority_sha256: String,
    expected_legacy_inventory_root_count: i64,
    expected_legacy_inventory_root_set_sha256: String,
    expected_non_destroyed_volume_count: i64,
    expected_destruction_count: i64,
    expected_unresolved_legacy_volume_count: i64,
    evidence_ref: String,
    evidence_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordRunnerVolumeFleetCutoverHttpResponse {
    disposition: RunnerVolumeWriteDisposition,
    status: RunnerVolumeFleetStatus,
}

async fn record_runner_volume_fleet_cutover(
    State(state): State<AppState>,
    Extension(AuthedAccount(admin)): Extension<AuthedAccount>,
    Json(request): Json<RecordRunnerVolumeFleetCutoverHttpRequest>,
) -> Result<Json<RecordRunnerVolumeFleetCutoverHttpResponse>, ApiError> {
    let before = jobs::runner_volume_fleet_status(&state.pool).map_err(runner_volume_api_error)?;
    if request.expected_enrollment_generation != before.enrollment_generation
        || request.expected_purge_generation != before.purge_generation
        || request.expected_tombstone_generation != before.tombstone_generation
        || request.expected_destruction_generation != before.destruction_generation
        || request.expected_legacy_reconciliation_generation
            != before.legacy_reconciliation_generation
        || request.expected_storage_attestation_generation != before.storage_attestation_generation
        || request.expected_storage_attestation_count != before.storage_attestation_count
        || request.expected_storage_attestation_set_sha256 != before.storage_attestation_set_sha256
        || request.expected_legacy_inventory_generation != before.legacy_inventory_generation
        || before.legacy_inventory_state != "ready"
        || before.legacy_inventory_reconciliation_id.as_deref()
            != Some(request.expected_legacy_inventory_reconciliation_id.as_str())
        || before.legacy_inventory_authority_id.as_deref()
            != Some(request.expected_legacy_inventory_authority_id.as_str())
        || before.legacy_inventory_authority_sha256.as_deref()
            != Some(request.expected_legacy_inventory_authority_sha256.as_str())
        || before.legacy_inventory_root_count != Some(request.expected_legacy_inventory_root_count)
        || before.legacy_inventory_root_set_sha256.as_deref()
            != Some(request.expected_legacy_inventory_root_set_sha256.as_str())
        || request.expected_non_destroyed_volume_count != before.non_destroyed_volume_count
        || request.expected_destruction_count != before.destruction_count
        || request.expected_unresolved_legacy_volume_count != before.unresolved_legacy_volume_count
    {
        return Err(conflict("Runner-volume fleet evidence snapshot changed."));
    }
    let now_ms = server_now_ms();
    let reuses_cutover_snapshot = before.cutover_enrollment_generation
        == Some(request.expected_enrollment_generation)
        && before.cutover_purge_generation == Some(request.expected_purge_generation)
        && before.cutover_tombstone_generation == Some(request.expected_tombstone_generation)
        && before.cutover_destruction_generation == Some(request.expected_destruction_generation)
        && before.cutover_legacy_reconciliation_generation
            == Some(request.expected_legacy_reconciliation_generation)
        && before.cutover_storage_attestation_generation
            == Some(request.expected_storage_attestation_generation)
        && before.cutover_storage_attestation_count
            == Some(request.expected_storage_attestation_count)
        && before.cutover_storage_attestation_set_sha256.as_deref()
            == Some(request.expected_storage_attestation_set_sha256.as_str())
        && before.cutover_legacy_inventory_generation
            == Some(request.expected_legacy_inventory_generation)
        && before.cutover_legacy_inventory_reconciliation_id.as_deref()
            == Some(request.expected_legacy_inventory_reconciliation_id.as_str())
        && before.cutover_legacy_inventory_authority_id.as_deref()
            == Some(request.expected_legacy_inventory_authority_id.as_str())
        && before.cutover_legacy_inventory_authority_sha256.as_deref()
            == Some(request.expected_legacy_inventory_authority_sha256.as_str())
        && before.cutover_legacy_inventory_root_count
            == Some(request.expected_legacy_inventory_root_count)
        && before.cutover_legacy_inventory_root_set_sha256.as_deref()
            == Some(request.expected_legacy_inventory_root_set_sha256.as_str())
        && before.cutover_non_destroyed_volume_count
            == Some(request.expected_non_destroyed_volume_count)
        && before.cutover_destruction_count == Some(request.expected_destruction_count)
        && before.cutover_unresolved_legacy_volume_count
            == Some(request.expected_unresolved_legacy_volume_count)
        && before.cutover_evidence_ref.as_deref() == Some(request.evidence_ref.as_str())
        && before.cutover_evidence_sha256.as_deref() == Some(request.evidence_sha256.as_str())
        && before.cutover_authorized_by.as_deref() == Some(admin.id.as_str());
    let cutover_at_ms = if reuses_cutover_snapshot {
        before.cutover_at_ms.unwrap_or(now_ms)
    } else {
        now_ms
    };
    let disposition = jobs::record_runner_volume_fleet_cutover(
        &state.pool,
        &RecordRunnerVolumeFleetCutoverRequest {
            cutover_state: request.cutover_state,
            expected_enrollment_generation: request.expected_enrollment_generation,
            expected_purge_generation: request.expected_purge_generation,
            expected_tombstone_generation: request.expected_tombstone_generation,
            expected_destruction_generation: request.expected_destruction_generation,
            expected_legacy_reconciliation_generation: request
                .expected_legacy_reconciliation_generation,
            expected_storage_attestation_generation: request
                .expected_storage_attestation_generation,
            expected_storage_attestation_count: request.expected_storage_attestation_count,
            expected_storage_attestation_set_sha256: request
                .expected_storage_attestation_set_sha256,
            expected_legacy_inventory_generation: request.expected_legacy_inventory_generation,
            expected_legacy_inventory_reconciliation_id: request
                .expected_legacy_inventory_reconciliation_id,
            expected_legacy_inventory_authority_id: request.expected_legacy_inventory_authority_id,
            expected_legacy_inventory_authority_sha256: request
                .expected_legacy_inventory_authority_sha256,
            expected_legacy_inventory_root_count: request.expected_legacy_inventory_root_count,
            expected_legacy_inventory_root_set_sha256: request
                .expected_legacy_inventory_root_set_sha256,
            expected_non_destroyed_volume_count: request.expected_non_destroyed_volume_count,
            expected_destruction_count: request.expected_destruction_count,
            expected_unresolved_legacy_volume_count: request
                .expected_unresolved_legacy_volume_count,
            evidence_ref: request.evidence_ref,
            evidence_sha256: request.evidence_sha256,
            authorized_by: admin.id,
            cutover_at_ms,
            now_ms,
        },
    )
    .map_err(runner_volume_api_error)?;
    let status = jobs::runner_volume_fleet_status(&state.pool).map_err(runner_volume_api_error)?;
    Ok(Json(RecordRunnerVolumeFleetCutoverHttpResponse {
        disposition,
        status,
    }))
}

fn server_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn runner_volume_api_error(error: RunnerVolumePurgeError) -> (StatusCode, String) {
    match error {
        RunnerVolumePurgeError::InvalidRequest => bad_request("Invalid runner-volume request."),
        RunnerVolumePurgeError::NotFound => not_found("Runner-volume authority not found."),
        RunnerVolumePurgeError::Conflict => conflict("Runner-volume authority conflicts."),
        RunnerVolumePurgeError::Unauthorized => unauthorized(),
        RunnerVolumePurgeError::NotReady => conflict("Runner volume is not ready."),
        RunnerVolumePurgeError::Storage(error) => {
            tracing::error!(error = %error, "runner-volume storage operation failed");
            internal_error()
        }
    }
}

fn policy_api_error(status: StatusCode) -> ApiError {
    (
        status,
        "Runner-volume purge policy is unavailable.".to_string(),
    )
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, message.to_string())
}

fn not_found(message: &str) -> ApiError {
    (StatusCode::NOT_FOUND, message.to_string())
}

fn conflict(message: &str) -> ApiError {
    (StatusCode::CONFLICT, message.to_string())
}

fn unauthorized() -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        "Runner-volume authority is invalid.".to_string(),
    )
}

fn internal_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Runner-volume operation failed.".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_material_requires_canonical_seed_and_identifiers() {
        let seed = URL_SAFE_NO_PAD.encode([7_u8; 32]);
        let signer = RunnerPurgeSigner::from_seed("server-key-602", [7_u8; 32]).unwrap();
        let verifying_keys = serde_json::json!([{
            "keyId": "server-key-602",
            "publicKeyBase64url": signer.public_key_base64url()
        }])
        .to_string();
        let policy = parse_runner_volume_purge_policy(
            "server-key-602",
            &seed,
            "runner-602.0",
            &verifying_keys,
        )
        .expect("valid policy");
        assert_eq!(policy.signer.key_id(), "server-key-602");
        assert_eq!(policy.minimum_runner_build_id, "runner-602.0");
        assert_eq!(policy.key_ring.public_keys_by_id().len(), 1);

        assert!(
            parse_runner_volume_purge_policy(" bad", &seed, "runner-602.0", &verifying_keys,)
                .is_err()
        );
        assert!(parse_runner_volume_purge_policy(
            "server-key-602",
            "AA",
            "runner-602.0",
            &verifying_keys,
        )
        .is_err());
        assert!(parse_runner_volume_purge_policy(
            "server-key-602",
            &seed,
            "runner build",
            &verifying_keys,
        )
        .is_err());
        assert!(parse_runner_volume_purge_policy(
            "server-key-602",
            &seed,
            "runner-602.01",
            &verifying_keys,
        )
        .is_err());
        assert!(
            parse_runner_volume_purge_policy("server-key-602", &seed, "runner-602.0", "[]",)
                .is_err()
        );
    }

    #[test]
    fn payload_hash_binds_path_worker_and_operation_fields() {
        let path = "/api/jobs/internal/runner-volumes/volume-id/commands/poll";
        let digest = runner_volume_http_payload_sha256(
            path,
            "worker-602",
            &[("after_command_id", ""), ("limit", "25")],
        );
        let canonical = concat!(
            "bluey-jobs-runner-volume-http-payload-v1\n",
            "method=POST\n",
            "path=/api/jobs/internal/runner-volumes/volume-id/commands/poll\n",
            "worker_id=worker-602\n",
            "after_command_id=\n",
            "limit=25\n"
        );
        assert_eq!(digest, hex::encode(Sha256::digest(canonical.as_bytes())));
        assert_ne!(
            digest,
            runner_volume_http_payload_sha256(
                path,
                "other-worker",
                &[("after_command_id", ""), ("limit", "25")],
            )
        );
        assert_ne!(
            digest,
            runner_volume_http_payload_sha256(
                path,
                "worker-602",
                &[("after_command_id", ""), ("limit", "26")],
            )
        );
        assert_ne!(
            digest,
            runner_volume_http_payload_sha256(
                path,
                "worker-602",
                &[("after_command_id", "command-1"), ("limit", "25")],
            )
        );
    }

    #[test]
    fn only_an_empty_null_cursor_poll_can_report_readiness() {
        assert!(poll_may_check_readiness(false, 0));
        assert!(!poll_may_check_readiness(false, 1));
        assert!(!poll_may_check_readiness(true, 0));
        assert!(!poll_may_check_readiness(true, 1));
    }

    #[test]
    fn stale_enrollment_is_allowed_only_for_exact_durable_identity_replay() {
        let proof = jobs::RunnerVolumeEnrollmentProof {
            admission_grant_id: "grant-602".to_string(),
            volume_id: "volume-602".to_string(),
            worker_id: "worker-602".to_string(),
            provider: "managed-provider".to_string(),
            provider_resource_id: "provider-volume-602".to_string(),
            resource_fingerprint: "1".repeat(64),
            enrollment_epoch: 1,
            public_key_base64url: "public-key-602".to_string(),
            key_fingerprint: "2".repeat(64),
            legacy_artifact_count: 3,
            requested_at_ms: 100,
            signature: "signature-602".to_string(),
        };
        let stored = RunnerVolumeRecord {
            volume_id: proof.volume_id.clone(),
            worker_id: proof.worker_id.clone(),
            provider: proof.provider.clone(),
            provider_resource_id: proof.provider_resource_id.clone(),
            resource_fingerprint: proof.resource_fingerprint.clone(),
            current_epoch: proof.enrollment_epoch,
            enrollment_generation: 9,
            required_tombstone_generation: 7,
            reconciled_tombstone_generation: 7,
            status: "active".to_string(),
            active_instance_id: None,
            instance_lease_expires_at_ms: None,
            legacy_artifact_count: proof.legacy_artifact_count,
            admission_grant_id: proof.admission_grant_id.clone(),
            enrolled_at_ms: 100,
            last_seen_at_ms: 100,
            updated_at_ms: 100,
            public_key_base64url: proof.public_key_base64url.clone(),
            key_fingerprint: proof.key_fingerprint.clone(),
            disposition: RunnerVolumeWriteDisposition::Replay,
        };
        let more_than_five_minutes_later =
            proof.requested_at_ms + RUNNER_VOLUME_ENROLLMENT_MAX_AGE_MS + 1;
        assert!(!runner_volume_enrollment_proof_is_fresh(
            &proof,
            more_than_five_minutes_later
        ));
        assert!(exact_runner_volume_enrollment_replay(&stored, &proof));
        assert!(runner_volume_enrollment_request_allowed(
            &proof,
            Some(&stored),
            more_than_five_minutes_later
        ));

        let mut changed = proof.clone();
        changed.worker_id = "other-worker".to_string();
        assert!(!exact_runner_volume_enrollment_replay(&stored, &changed));
        assert!(!runner_volume_enrollment_request_allowed(
            &changed,
            Some(&stored),
            more_than_five_minutes_later
        ));

        assert!(!runner_volume_enrollment_request_allowed(
            &proof,
            None,
            more_than_five_minutes_later
        ));
    }

    #[test]
    fn execution_lease_claim_payload_has_one_frozen_canonical_implementation() {
        let digest = runner_volume_execution_lease_claim_payload_sha256(
            &RunnerVolumeExecutionLeaseClaimPayload {
                worker_id: "worker-602",
                account_id: "account-602",
                application_id: "application-602",
                run_id: "run-602",
                browser_profile_id: "profile-602",
                owner_id: "worker-602",
                volume_id: "volume-602",
                enrollment_epoch: 7,
                process_instance_id: "process-602",
            },
        );
        let canonical = concat!(
            "bluey-jobs-runner-volume-http-payload-v1\n",
            "method=POST\n",
            "path=/api/jobs/internal/execution-leases/claim\n",
            "worker_id=worker-602\n",
            "account_id=account-602\n",
            "application_id=application-602\n",
            "run_id=run-602\n",
            "browser_profile_id=profile-602\n",
            "owner_id=worker-602\n",
            "volume_id=volume-602\n",
            "enrollment_epoch=7\n",
            "process_instance_id=process-602\n"
        );
        assert_eq!(digest, hex::encode(Sha256::digest(canonical.as_bytes())));
    }

    #[test]
    fn strict_http_requests_reject_unknown_or_server_owned_fields() {
        let enrollment_proof = serde_json::json!({
            "admissionGrantId": "grant-602",
            "volumeId": "A".repeat(43),
            "workerId": "worker-602",
            "provider": "managed-provider",
            "providerResourceId": "provider-volume-602",
            "resourceFingerprint": "1".repeat(64),
            "enrollmentEpoch": 1,
            "publicKeyBase64url": "B".repeat(43),
            "keyFingerprint": "2".repeat(64),
            "legacyArtifactCount": 3,
            "requestedAtMs": 602,
            "signature": "C".repeat(86)
        });
        let enrollment = serde_json::json!({
            "grantToken": "D".repeat(43),
            "proof": enrollment_proof
        });
        assert!(
            serde_json::from_value::<EnrollRunnerVolumeHttpRequest>(enrollment.clone()).is_ok()
        );
        let mut unsigned_legacy_count = enrollment;
        unsigned_legacy_count
            .as_object_mut()
            .unwrap()
            .insert("legacyArtifactCount".to_string(), serde_json::json!(3));
        assert!(
            serde_json::from_value::<EnrollRunnerVolumeHttpRequest>(unsigned_legacy_count).is_err()
        );

        let proof = serde_json::json!({
            "version": 1,
            "audience": "bluey-jobs-runner-volume-authority",
            "operation": "purge_poll",
            "requestId": "request-602",
            "volumeId": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "enrollmentEpoch": 1,
            "processInstanceId": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "issuedAtMs": 1,
            "payloadSha256": "0".repeat(64),
            "signature": "C".repeat(86)
        });
        let valid = serde_json::json!({"proof": proof, "limit": 5});
        assert!(serde_json::from_value::<PollRunnerVolumeCommandsHttpRequest>(valid).is_ok());

        let invalid = serde_json::json!({
            "proof": proof,
            "limit": 5,
            "nowMs": 123,
            "minimumRunnerBuildId": "forged"
        });
        assert!(serde_json::from_value::<PollRunnerVolumeCommandsHttpRequest>(invalid).is_err());
    }

    #[test]
    fn admin_http_requests_require_complete_evidence_snapshots() {
        let destruction = serde_json::json!({
            "expectedEnrollmentGeneration": 9,
            "expectedVolumeKeyFingerprint": "1".repeat(64),
            "expectedProvider": "managed-provider",
            "expectedProviderResourceId": "provider-volume-602",
            "expectedResourceFingerprint": "2".repeat(64),
            "evidenceType": "provider_volume_destroyed",
            "snapshotInventorySha256": "3".repeat(64),
            "evidenceSha256": "4".repeat(64),
            "authorizationRef": "ticket-602",
            "occurredAtMs": 602,
            "details": {"providerReceipt": "receipt-602"}
        });
        assert!(
            serde_json::from_value::<RecordRunnerVolumeDestructionHttpRequest>(destruction.clone())
                .is_ok()
        );
        let mut incomplete_destruction = destruction.clone();
        incomplete_destruction
            .as_object_mut()
            .unwrap()
            .remove("expectedProviderResourceId");
        assert!(
            serde_json::from_value::<RecordRunnerVolumeDestructionHttpRequest>(
                incomplete_destruction
            )
            .is_err()
        );
        let mut server_owned_destruction = destruction;
        server_owned_destruction
            .as_object_mut()
            .unwrap()
            .insert("recordedAtMs".to_string(), serde_json::json!(603));
        assert!(
            serde_json::from_value::<RecordRunnerVolumeDestructionHttpRequest>(
                server_owned_destruction
            )
            .is_err()
        );

        let legacy = serde_json::json!({
            "expectedLegacyUnresolvedCount": 2,
            "resolutionRef": "ticket-legacy-602",
            "resolutionSha256": "5".repeat(64)
        });
        assert!(serde_json::from_value::<ResolveRunnerVolumeLegacyHttpRequest>(legacy).is_ok());

        let inventory = serde_json::json!({
            "reconciliationId": "inventory-602",
            "authorityState": "reconciling",
            "expectedPredecessorGeneration": 0,
            "expectedPredecessorAuthorityId": null,
            "expectedPredecessorAuthoritySha256": null,
            "rootCount": 0,
            "rootSetSha256": jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256,
            "scopeRef": "all-managed-runner-storage-roots",
            "evidenceRef": "inventory-evidence-602",
            "evidenceSha256": "7".repeat(64)
        });
        assert!(
            serde_json::from_value::<RecordRunnerLegacyInventoryAuthorityHttpRequest>(
                inventory.clone()
            )
            .is_ok()
        );
        let mut client_timed_inventory = inventory;
        client_timed_inventory
            .as_object_mut()
            .unwrap()
            .insert("recordedAtMs".to_string(), serde_json::json!(603));
        assert!(
            serde_json::from_value::<RecordRunnerLegacyInventoryAuthorityHttpRequest>(
                client_timed_inventory
            )
            .is_err()
        );

        let cutover = serde_json::json!({
            "cutoverState": "reconciling",
            "expectedEnrollmentGeneration": 9,
            "expectedPurgeGeneration": 8,
            "expectedTombstoneGeneration": 7,
            "expectedDestructionGeneration": 6,
            "expectedLegacyReconciliationGeneration": 5,
            "expectedStorageAttestationGeneration": 10,
            "expectedStorageAttestationCount": 4,
            "expectedStorageAttestationSetSha256": "8".repeat(64),
            "expectedLegacyInventoryGeneration": 4,
            "expectedLegacyInventoryReconciliationId": "inventory-602",
            "expectedLegacyInventoryAuthorityId": "legacy-inventory-authority-602",
            "expectedLegacyInventoryAuthoritySha256": "7".repeat(64),
            "expectedLegacyInventoryRootCount": 0,
            "expectedLegacyInventoryRootSetSha256": jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256,
            "expectedNonDestroyedVolumeCount": 4,
            "expectedDestructionCount": 3,
            "expectedUnresolvedLegacyVolumeCount": 2,
            "evidenceRef": "cutover-602",
            "evidenceSha256": "6".repeat(64)
        });
        assert!(
            serde_json::from_value::<RecordRunnerVolumeFleetCutoverHttpRequest>(cutover.clone())
                .is_ok()
        );
        let mut incomplete_cutover = cutover;
        incomplete_cutover
            .as_object_mut()
            .unwrap()
            .remove("expectedDestructionGeneration");
        assert!(
            serde_json::from_value::<RecordRunnerVolumeFleetCutoverHttpRequest>(incomplete_cutover)
                .is_err()
        );
    }

    #[test]
    fn poll_response_uses_the_frozen_key_ring_object_shape() {
        let response = PollRunnerVolumeCommandsHttpResponse {
            commands: Vec::new(),
            next_command_cursor: None,
            ready: true,
            enrollment_generation: 11,
            required_tombstone_generation: 7,
            reconciled_tombstone_generation: 7,
            predecessor_attestation_generation: 3,
            predecessor_attestation_sha256: "d".repeat(64),
            storage_attestation_required: false,
            server_command_keys: BTreeMap::from([(
                "server-key-602".to_string(),
                URL_SAFE_NO_PAD.encode([9_u8; 32]),
            )]),
        };
        let serialized = serde_json::to_value(response).unwrap();
        assert_eq!(
            serialized,
            serde_json::json!({
                "commands": [],
                "nextCommandCursor": null,
                "ready": true,
                "enrollmentGeneration": 11,
                "requiredTombstoneGeneration": 7,
                "reconciledTombstoneGeneration": 7,
                "predecessorAttestationGeneration": 3,
                "predecessorAttestationSha256": "d".repeat(64),
                "storageAttestationRequired": false,
                "serverCommandKeys": {
                    "server-key-602": URL_SAFE_NO_PAD.encode([9_u8; 32])
                }
            })
        );
    }

    #[test]
    fn ack_http_defers_historical_epoch_and_frozen_build_authority_to_database() {
        let source = include_str!("jobs_runner_volumes.rs");
        let handler = source
            .split_once("async fn acknowledge_runner_volume_command(")
            .expect("ACK HTTP handler")
            .1
            .split_once("\n#[derive(Debug, Serialize)]")
            .expect("end of ACK HTTP handler")
            .0;
        assert!(handler.contains("runner_build_id_is_canonical"));
        assert!(!handler.contains("runner_build_satisfies"));
        assert!(!handler.contains("volume.current_epoch != ack.enrollment_epoch"));
        assert!(handler.contains("acknowledge_runner_volume_purge"));
    }

    #[test]
    fn ack_response_excludes_account_correlating_control_material() {
        let response = RunnerPurgeAckHttpResponse::from(jobs::RunnerPurgeAckOutcome {
            disposition: RunnerVolumeWriteDisposition::Applied,
            status: RunnerPurgeRequestStatus {
                request_id: "request-602".to_string(),
                account_id: Some("SENTINEL-DIRECT-ACCOUNT-ID".to_string()),
                purge_subject: "SENTINEL-RAW-PURGE-SUBJECT".to_string(),
                purge_generation: 11,
                legacy_inventory_generation: 7,
                legacy_inventory_reconciliation_id: "inventory-602".to_string(),
                legacy_inventory_authority_id: "legacy-inventory-authority-602".to_string(),
                legacy_inventory_authority_sha256: "6".repeat(64),
                state: "pending".to_string(),
                legacy_unresolved_count: 2,
                required_target_count: 4,
                resolved_target_count: 3,
                target_set_sha256: "7".repeat(64),
                created_at_ms: 100,
                updated_at_ms: 200,
                completed_at_ms: None,
            },
            reconciled_tombstone_generation: 9,
        });
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("SENTINEL-DIRECT-ACCOUNT-ID"));
        assert!(!serialized.contains("SENTINEL-RAW-PURGE-SUBJECT"));
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({
                "disposition": "applied",
                "status": {
                    "purgeGeneration": 11,
                    "state": "pending",
                    "legacyUnresolvedCount": 2,
                    "requiredTargetCount": 4,
                    "resolvedTargetCount": 3
                },
                "reconciledTombstoneGeneration": 9
            })
        );
    }

    #[test]
    fn routers_expose_only_the_frozen_phase_602_paths() {
        let _worker = worker_router();
        let _admin = admin_router();
    }
}

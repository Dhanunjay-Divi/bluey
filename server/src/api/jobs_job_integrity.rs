//! Authenticated administration APIs for the signed Jobs integrity authority.

use axum::{
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::post,
    Extension, Json, Router,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::{
        jobs::{
            self, JobIntegrityAttestationPackageV1, JobIntegrityAuthorityError,
            JobIntegrityImportResult, JobIntegrityResolution, JobIntegrityRevocationPackageV1,
            JobIntegrityTrustPolicyPackageV1,
        },
        ops_audit,
    },
};

type ApiError = (StatusCode, String);

// One attestation package can contain three independently bounded base64url
// documents. Keep the HTTP bound explicit and close to their canonical 64 KiB
// limits without inheriting the much larger general router limit.
const JOB_INTEGRITY_BODY_LIMIT_BYTES: usize = 384 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobIntegrityStatusRequest {
    account_id: String,
    job_id: String,
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/job-integrity/trust-policies",
            post(import_trust_policy),
        )
        .route(
            "/admin/jobs/job-integrity/attestations",
            post(import_attestation),
        )
        .route(
            "/admin/jobs/job-integrity/revocations",
            post(import_revocation),
        )
        .route("/admin/jobs/job-integrity/status", post(resolve_status))
        .layer(DefaultBodyLimit::max(JOB_INTEGRITY_BODY_LIMIT_BYTES))
}

async fn import_trust_policy(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(package): Json<JobIntegrityTrustPolicyPackageV1>,
) -> Result<Json<JobIntegrityImportResult>, ApiError> {
    let result = jobs::import_job_integrity_trust_policy(&state.pool, &package, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "trust_policy", &result)?;
    Ok(Json(result))
}

async fn import_attestation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(package): Json<JobIntegrityAttestationPackageV1>,
) -> Result<Json<JobIntegrityImportResult>, ApiError> {
    let result = jobs::import_job_integrity_attestation(&state.pool, &package, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "attestation", &result)?;
    Ok(Json(result))
}

async fn import_revocation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(package): Json<JobIntegrityRevocationPackageV1>,
) -> Result<Json<JobIntegrityImportResult>, ApiError> {
    let result = jobs::import_job_integrity_revocation(&state.pool, &package, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "revocation", &result)?;
    Ok(Json(result))
}

async fn resolve_status(
    State(state): State<AppState>,
    Json(request): Json<JobIntegrityStatusRequest>,
) -> Result<Json<JobIntegrityResolution>, ApiError> {
    if request.account_id.trim().is_empty()
        || request.account_id.len() > 240
        || request.job_id.trim().is_empty()
        || request.job_id.len() > 240
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "The Jobs integrity status request is invalid.".to_string(),
        ));
    }
    let projection = jobs::resolve_composed_job_integrity_projection_for_posting(
        &state.pool,
        &request.account_id,
        &request.job_id,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "Jobs composed integrity status resolution failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The Jobs integrity authority operation failed.".to_string(),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "The Jobs posting was not found.".to_string(),
        )
    })?;
    Ok(Json(projection.job_integrity))
}

fn record_import_audit(
    state: &AppState,
    actor_id: &str,
    operation: &str,
    result: &JobIntegrityImportResult,
) -> Result<(), ApiError> {
    ops_audit::record_event(
        &state.pool,
        ops_audit::OpsAuditEventInput {
            account_id_hash: None,
            actor_account_id_hash: Some(audit_actor_hash(actor_id)),
            event_type: format!("jobs_job_integrity_{operation}"),
            status: "success".to_string(),
            metadata_json: import_audit_metadata(result),
        },
    )
    .map_err(|error| {
        tracing::error!(error = %error, operation, "Jobs integrity audit event failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The Jobs integrity authority operation failed.".to_string(),
        )
    })
}

fn import_audit_metadata(result: &JobIntegrityImportResult) -> serde_json::Value {
    serde_json::json!({
        "objectSha256": result.object_sha256,
        "generation": result.generation,
        "headRevision": result.head_revision,
        "headTransitionSha256": result.head_transition_sha256,
        "replayed": result.replayed,
    })
}

fn audit_actor_hash(actor_id: &str) -> String {
    hex::encode(Sha256::digest(actor_id.as_bytes()))
}

fn authority_api_error(error: JobIntegrityAuthorityError) -> ApiError {
    match error {
        JobIntegrityAuthorityError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The Jobs integrity authority operation failed.".to_string(),
        ),
        JobIntegrityAuthorityError::NotFound | JobIntegrityAuthorityError::NotInitialized => (
            StatusCode::NOT_FOUND,
            "The Jobs integrity authority was not found.".to_string(),
        ),
        JobIntegrityAuthorityError::IdentityConflict
        | JobIntegrityAuthorityError::CompareAndSwapConflict
        | JobIntegrityAuthorityError::SequenceRegression
        | JobIntegrityAuthorityError::Expired
        | JobIntegrityAuthorityError::Revoked
        | JobIntegrityAuthorityError::SourceMismatch => (
            StatusCode::CONFLICT,
            "The Jobs integrity authority conflicts with current state.".to_string(),
        ),
        _ => (
            StatusCode::BAD_REQUEST,
            "The Jobs integrity authority request is invalid.".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_audit_is_redacted_to_authority_identities() {
        let result = JobIntegrityImportResult {
            object_sha256: "ab".repeat(32),
            generation: 2,
            replayed: false,
            head_revision: Some(3),
            head_transition_sha256: Some("cd".repeat(32)),
        };

        let metadata = import_audit_metadata(&result);

        assert_eq!(metadata["objectSha256"], result.object_sha256);
        assert_eq!(metadata["generation"], 2);
        assert_eq!(metadata["headRevision"], 3);
        assert_eq!(metadata["headTransitionSha256"], "cd".repeat(32));
        assert_eq!(metadata["replayed"], false);
        assert!(metadata.get("canonicalEmployerDomain").is_none());
        assert!(metadata.get("subjectSha256").is_none());
        assert!(metadata.get("canonicalApplicationUrl").is_none());
    }

    #[test]
    fn authority_errors_are_stable_and_do_not_disclose_storage_details() {
        assert_eq!(
            authority_api_error(JobIntegrityAuthorityError::InvalidSignature).0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            authority_api_error(JobIntegrityAuthorityError::NotInitialized).0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            authority_api_error(JobIntegrityAuthorityError::IdentityConflict).0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            authority_api_error(JobIntegrityAuthorityError::Storage(anyhow::anyhow!(
                "secret database detail"
            )))
            .1,
            "The Jobs integrity authority operation failed."
        );
    }

    #[test]
    fn status_request_accepts_only_server_owned_posting_identity() {
        let value = serde_json::json!({
            "accountId": "account-1",
            "jobId": "job-1",
        });
        assert!(serde_json::from_value::<JobIntegrityStatusRequest>(value.clone()).is_ok());

        let mut caller_authority = value;
        caller_authority["sourceExpiresAtMs"] = serde_json::json!(1_900_000_000_000_i64);
        assert!(serde_json::from_value::<JobIntegrityStatusRequest>(caller_authority).is_err());
    }
}

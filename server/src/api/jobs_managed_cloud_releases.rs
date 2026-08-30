//! HTTP boundary for the managed Bluey Jobs cloud release authority.
//!
//! Administrator routes import deployment authority. Private runtime routes
//! bind the fleet HMAC identity to the one-time grant and fenced instance.

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{jobs_worker_auth::JobsWorkerIdentity, AppState},
    auth::AuthedAccount,
    db::jobs::{
        self, ApplyManagedCloudActivationRequest, ApplyManagedCloudRollbackRequest,
        ClaimManagedCloudRuntimeGrant, ManagedCloudActivationImportRequest,
        ManagedCloudAuthorityEnvelope, ManagedCloudCohortImportRequest, ManagedCloudReadinessQuery,
        ManagedCloudRegistryError, ManagedCloudReleaseImportRequest,
        ManagedCloudRevocationImportRequest, ManagedCloudRollbackImportRequest,
        ManagedCloudRuntimeHeartbeatInput, ManagedCloudScope, NewManagedCloudRuntimeGrant,
        RevokeManagedCloudRuntimeGrant,
    },
};

type ApiError = (StatusCode, String);

const MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES: usize = 128 * 1024;
const MANAGED_CLOUD_ENVELOPE_BODY_LIMIT_BYTES: usize = 384 * 1024;
const MANAGED_CLOUD_COHORT_BODY_LIMIT_BYTES: usize = 512 * 1024;
const MANAGED_CLOUD_ACTIVATION_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const MANAGED_CLOUD_RELEASE_BODY_LIMIT_BYTES: usize = 18 * 1024 * 1024;
const MANAGED_CLOUD_RUNTIME_SCOPE: &str = "managed-cloud-runtime";

pub fn worker_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/managed-cloud/runtime-grants/:grant_id/claim",
            post(claim_runtime_grant),
        )
        .route(
            "/api/jobs/internal/managed-cloud/runtime-instances/:runtime_instance_id/heartbeats",
            post(record_runtime_heartbeat),
        )
        .layer(DefaultBodyLimit::max(
            MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES,
        ))
}

pub fn admin_router() -> Router<AppState> {
    let trust_authority = Router::new()
        .route(
            "/admin/jobs/managed-cloud/trust-policies",
            post(import_trust_policy),
        )
        .layer(DefaultBodyLimit::max(
            MANAGED_CLOUD_ENVELOPE_BODY_LIMIT_BYTES,
        ));
    let cohort_import = Router::new()
        .route("/admin/jobs/managed-cloud/cohorts", post(import_cohort))
        .layer(DefaultBodyLimit::max(MANAGED_CLOUD_COHORT_BODY_LIMIT_BYTES));
    let activation_import = Router::new()
        .route(
            "/admin/jobs/managed-cloud/activations",
            post(import_activation),
        )
        .layer(DefaultBodyLimit::max(
            MANAGED_CLOUD_ACTIVATION_BODY_LIMIT_BYTES,
        ));
    let bounded_envelopes = Router::new()
        .route("/admin/jobs/managed-cloud/rollbacks", post(import_rollback))
        .route(
            "/admin/jobs/managed-cloud/revocations",
            post(import_revocation),
        )
        .layer(DefaultBodyLimit::max(
            MANAGED_CLOUD_ENVELOPE_BODY_LIMIT_BYTES,
        ));
    let bounded_mutations = Router::new()
        .route("/admin/jobs/managed-cloud/status", get(release_status))
        .route(
            "/admin/jobs/managed-cloud/readiness",
            post(resolve_readiness),
        )
        .route(
            "/admin/jobs/managed-cloud/runtime-grants",
            post(issue_runtime_grant),
        )
        .route(
            "/admin/jobs/managed-cloud/runtime-grants/:grant_id/revocations",
            post(revoke_runtime_grant),
        )
        .route(
            "/admin/jobs/managed-cloud/heads/activation",
            post(apply_activation),
        )
        .route(
            "/admin/jobs/managed-cloud/heads/rollback",
            post(apply_rollback),
        )
        .layer(DefaultBodyLimit::max(
            MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES,
        ));
    let release_import = Router::new()
        .route("/admin/jobs/managed-cloud/releases", post(import_release))
        .layer(DefaultBodyLimit::max(
            MANAGED_CLOUD_RELEASE_BODY_LIMIT_BYTES,
        ));
    trust_authority
        .merge(cohort_import)
        .merge(activation_import)
        .merge(bounded_envelopes)
        .merge(bounded_mutations)
        .merge(release_import)
}

async fn import_trust_policy(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<ManagedCloudAuthorityEnvelope>,
) -> Result<Response, ApiError> {
    jobs::import_managed_cloud_trust_policy(&state.pool, &envelope, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn import_release(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ManagedCloudReleaseImportRequest>,
) -> Result<Response, ApiError> {
    jobs::import_managed_cloud_release(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn import_cohort(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ManagedCloudCohortImportRequest>,
) -> Result<Response, ApiError> {
    jobs::import_managed_cloud_cohort(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn import_activation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ManagedCloudActivationImportRequest>,
) -> Result<Response, ApiError> {
    jobs::import_managed_cloud_activation(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn import_rollback(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ManagedCloudRollbackImportRequest>,
) -> Result<Response, ApiError> {
    jobs::import_managed_cloud_rollback(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn import_revocation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ManagedCloudRevocationImportRequest>,
) -> Result<Response, ApiError> {
    jobs::import_managed_cloud_revocation(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn apply_activation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ApplyManagedCloudActivationRequest>,
) -> Result<Response, ApiError> {
    jobs::apply_managed_cloud_activation(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn apply_rollback(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ApplyManagedCloudRollbackRequest>,
) -> Result<Response, ApiError> {
    jobs::apply_managed_cloud_rollback(&state.pool, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IssueRuntimeGrantRequest {
    issuance_ref: String,
    scope: ManagedCloudScope,
    activation_sha256: String,
    manifest_sha256: String,
    component_id: String,
    role: String,
    expected_worker_id: String,
    authorization_ref: String,
    ttl_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevokeRuntimeGrantRequest {
    reason_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudStatusQuery {
    environment: String,
    region: String,
    channel: String,
}

async fn release_status(
    State(state): State<AppState>,
    Extension(_admin): Extension<AuthedAccount>,
    Query(query): Query<ManagedCloudStatusQuery>,
) -> Result<Response, ApiError> {
    jobs::get_managed_cloud_release_status(
        &state.pool,
        &ManagedCloudScope {
            environment: query.environment,
            region: query.region,
            channel: query.channel,
        },
    )
    .map(authoritative_json)
    .map_err(registry_api_error)
}

async fn resolve_readiness(
    State(state): State<AppState>,
    Extension(_admin): Extension<AuthedAccount>,
    Json(query): Json<ManagedCloudReadinessQuery>,
) -> Result<Response, ApiError> {
    jobs::resolve_managed_cloud_readiness(&state.pool, &query)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn issue_runtime_grant(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<IssueRuntimeGrantRequest>,
) -> Result<Response, ApiError> {
    let input = NewManagedCloudRuntimeGrant {
        issuance_ref: request.issuance_ref,
        scope: request.scope,
        activation_sha256: request.activation_sha256,
        manifest_sha256: request.manifest_sha256,
        component_id: request.component_id,
        role: request.role,
        expected_worker_id: request.expected_worker_id,
        authorization_ref: request.authorization_ref,
        created_by: admin.0.id.clone(),
        ttl_ms: request.ttl_ms,
    };
    jobs::issue_managed_cloud_runtime_grant(&state.pool, &input)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn revoke_runtime_grant(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Path(grant_id): Path<String>,
    Json(request): Json<RevokeRuntimeGrantRequest>,
) -> Result<Response, ApiError> {
    jobs::revoke_managed_cloud_runtime_grant(
        &state.pool,
        &RevokeManagedCloudRuntimeGrant {
            grant_id,
            reason_ref: request.reason_ref,
            revoked_by: admin.0.id.clone(),
        },
    )
    .map(authoritative_json)
    .map_err(registry_api_error)
}

async fn claim_runtime_grant(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(grant_id): Path<String>,
    Json(request): Json<ClaimManagedCloudRuntimeGrant>,
) -> Result<Response, ApiError> {
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.grant_id != grant_id {
        return Err(invalid_request());
    }
    jobs::claim_managed_cloud_runtime_grant(&state.pool, &request)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn record_runtime_heartbeat(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    Json(request): Json<ManagedCloudRuntimeHeartbeatInput>,
) -> Result<Response, ApiError> {
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    jobs::record_managed_cloud_runtime_heartbeat(&state.pool, &request)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

fn require_runtime_worker(worker: &JobsWorkerIdentity, expected: &str) -> Result<(), ApiError> {
    if worker.scope != MANAGED_CLOUD_RUNTIME_SCOPE || worker.worker_id != expected {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
    Ok(())
}

fn authoritative_json<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn invalid_request() -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        "The managed-cloud authority request is invalid.".to_string(),
    )
}

fn registry_api_error(error: ManagedCloudRegistryError) -> ApiError {
    match error {
        ManagedCloudRegistryError::InvalidEnvelope
        | ManagedCloudRegistryError::InvalidAuthority
        | ManagedCloudRegistryError::InvalidRequest => invalid_request(),
        ManagedCloudRegistryError::NotFound => (
            StatusCode::NOT_FOUND,
            "The managed-cloud authority was not found.".to_string(),
        ),
        ManagedCloudRegistryError::Unavailable
        | ManagedCloudRegistryError::CohortIneligible
        | ManagedCloudRegistryError::GrantExpired
        | ManagedCloudRegistryError::RecoveryNotAccepted
        | ManagedCloudRegistryError::Revoked => (
            StatusCode::SERVICE_UNAVAILABLE,
            "The managed-cloud authority is unavailable.".to_string(),
        ),
        ManagedCloudRegistryError::IdentityConflict
        | ManagedCloudRegistryError::CompareAndSwapConflict
        | ManagedCloudRegistryError::SequenceRegression
        | ManagedCloudRegistryError::DowngradeRequiresRollback
        | ManagedCloudRegistryError::GrantConsumed
        | ManagedCloudRegistryError::HeartbeatSequenceConflict => (
            StatusCode::CONFLICT,
            "The managed-cloud authority conflicts with current state.".to_string(),
        ),
        ManagedCloudRegistryError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The managed-cloud authority operation failed.".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use base64::Engine;
    use tower::ServiceExt;

    #[test]
    fn runtime_worker_identity_must_match_authenticated_extension() {
        let worker = JobsWorkerIdentity {
            worker_id: "managed-runtime-worker-1234".to_string(),
            scope: MANAGED_CLOUD_RUNTIME_SCOPE.to_string(),
        };
        assert!(require_runtime_worker(&worker, "managed-runtime-worker-1234").is_ok());
        assert!(require_runtime_worker(&worker, "managed-runtime-worker-5678").is_err());
    }

    #[test]
    fn authoritative_runtime_response_is_private_and_typed() {
        let response = authoritative_json(serde_json::json!({ "ok": true }));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
    }

    #[test]
    fn authority_route_caps_admit_their_maximum_valid_encoded_shapes() {
        let canonical = canonical_attachment(MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES);
        let envelope = serde_json::json!({
            "canonicalBase64url": canonical.clone(),
            "signatureSetBase64url": canonical,
        });
        let trust_bytes = serde_json::to_vec(&envelope).unwrap();
        assert!(trust_bytes.len() <= MANAGED_CLOUD_ENVELOPE_BODY_LIMIT_BYTES);

        let account_ids = (0..512)
            .map(|index| {
                let prefix = format!("managed-cloud-account-{index:04}-");
                format!("{prefix}{}", "a".repeat(128 - prefix.len()))
            })
            .collect::<Vec<_>>();
        let cohort_bytes = serde_json::to_vec(&serde_json::json!({
            "envelope": envelope.clone(),
            "accountIds": account_ids,
        }))
        .unwrap();
        assert!(cohort_bytes.len() <= MANAGED_CLOUD_COHORT_BODY_LIMIT_BYTES);

        let evidence = canonical_attachment(MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES);
        let activation_bytes = serde_json::to_vec(&serde_json::json!({
            "envelope": envelope,
            "evidence": {
                "canaryBase64url": evidence.clone(),
                "cleanupBase64url": evidence.clone(),
                "failureConverterBase64url": evidence.clone(),
                "portalReadbackBase64url": evidence.clone(),
                "runnerFleetBase64url": evidence.clone(),
                "storageBase64url": evidence.clone(),
                "taskQueueBase64url": evidence.clone(),
                "temporalNamespaceBase64url": evidence,
            },
        }))
        .unwrap();
        assert!(activation_bytes.len() <= MANAGED_CLOUD_ACTIVATION_BODY_LIMIT_BYTES);
    }

    #[tokio::test]
    async fn isolated_body_limits_accept_exact_bytes_and_reject_one_more() {
        for limit in [
            MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES,
            MANAGED_CLOUD_ENVELOPE_BODY_LIMIT_BYTES,
            MANAGED_CLOUD_COHORT_BODY_LIMIT_BYTES,
            MANAGED_CLOUD_ACTIVATION_BODY_LIMIT_BYTES,
            MANAGED_CLOUD_RELEASE_BODY_LIMIT_BYTES,
        ] {
            assert_eq!(
                bounded_json_status(limit, limit).await,
                StatusCode::NO_CONTENT
            );
            assert_eq!(
                bounded_json_status(limit, limit + 1).await,
                StatusCode::PAYLOAD_TOO_LARGE
            );
        }
    }

    fn canonical_attachment(decoded_bytes: usize) -> String {
        let padding = "a".repeat(decoded_bytes - 15);
        let canonical = format!("{{\"padding\":\"{padding}\"}}\n");
        assert_eq!(canonical.len(), decoded_bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical)
    }

    async fn bounded_json_status(limit: usize, bytes: usize) -> StatusCode {
        let router = Router::new()
            .route(
                "/",
                post(|Json(_): Json<serde_json::Value>| async { StatusCode::NO_CONTENT }),
            )
            .layer(DefaultBodyLimit::max(limit));
        let padding = "a".repeat(bytes - 14);
        let body = format!("{{\"padding\":\"{padding}\"}}");
        assert_eq!(body.len(), bytes);
        router
            .oneshot(
                Request::post("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }
}

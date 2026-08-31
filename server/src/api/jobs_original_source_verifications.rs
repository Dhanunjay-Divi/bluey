//! Private, receipt-producing original-source verification worker boundary.

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::post,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{jobs_worker_auth::JobsWorkerIdentity, AppState},
    db::jobs::{
        self, OriginalSourceVerificationCompletionRequest, OriginalSourceVerificationError,
        OriginalSourceVerificationFailureRequest, OriginalSourceVerificationHeartbeatRequest,
        OriginalSourceVerifierBinding,
    },
};

type ApiError = (StatusCode, String);

const ORIGINAL_SOURCE_VERIFICATION_BODY_LIMIT_BYTES: usize = 256 * 1024;
const ORIGINAL_SOURCE_VERIFICATION_SCOPE: &str = "original-source-verification";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseRequest {
    binding: OriginalSourceVerifierBinding,
}

pub fn worker_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/original-source-verifications/lease",
            post(lease),
        )
        .route(
            "/api/jobs/internal/original-source-verifications/:assignment_id/heartbeat",
            post(heartbeat),
        )
        .route(
            "/api/jobs/internal/original-source-verifications/:assignment_id/complete",
            post(complete),
        )
        .route(
            "/api/jobs/internal/original-source-verifications/:assignment_id/fail",
            post(fail),
        )
        .layer(DefaultBodyLimit::max(
            ORIGINAL_SOURCE_VERIFICATION_BODY_LIMIT_BYTES,
        ))
}

async fn lease(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Json(request): Json<LeaseRequest>,
) -> Result<axum::response::Response, ApiError> {
    require_verifier_scope(&worker)?;
    require_verifier_binding(&state, &worker, &request.binding)?;
    match jobs::lease_original_source_verification(&state.pool, &request.binding)
        .map_err(original_source_verification_api_error)?
    {
        Some(value) => Ok(authoritative_json(value)),
        None => Ok(authoritative_empty()),
    }
}

async fn heartbeat(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(assignment_id): Path<String>,
    Json(request): Json<OriginalSourceVerificationHeartbeatRequest>,
) -> Result<Response, ApiError> {
    require_verifier_scope(&worker)?;
    require_assignment_path(&assignment_id, &request.assignment_id)?;
    require_verifier_binding(&state, &worker, &request.binding)?;
    jobs::heartbeat_original_source_verification(&state.pool, &request)
        .map(authoritative_json)
        .map_err(original_source_verification_api_error)
}

async fn complete(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(assignment_id): Path<String>,
    Json(request): Json<OriginalSourceVerificationCompletionRequest>,
) -> Result<Response, ApiError> {
    require_verifier_scope(&worker)?;
    require_assignment_path(&assignment_id, &request.assignment_id)?;
    // Terminal persistence owns the replay/current-authority distinction. A
    // byte-identical response-loss replay must remain recoverable after later
    // runtime revocation, while a new publication still rechecks authority in
    // the same database transaction that appends its receipt.
    require_verifier_identity(&worker, &request.binding)?;
    jobs::complete_original_source_verification(&state.pool, &request)
        .map(authoritative_json)
        .map_err(original_source_verification_api_error)
}

async fn fail(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(assignment_id): Path<String>,
    Json(request): Json<OriginalSourceVerificationFailureRequest>,
) -> Result<Response, ApiError> {
    require_verifier_scope(&worker)?;
    require_assignment_path(&assignment_id, &request.assignment_id)?;
    require_verifier_identity(&worker, &request.binding)?;
    jobs::fail_original_source_verification(&state.pool, &request)
        .map(authoritative_json)
        .map_err(original_source_verification_api_error)
}

fn authoritative_json<T: Serialize>(value: T) -> Response {
    authoritative_response(Json(value).into_response())
}

fn authoritative_empty() -> Response {
    authoritative_response(StatusCode::NO_CONTENT.into_response())
}

fn authoritative_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn require_verifier_scope(worker: &JobsWorkerIdentity) -> Result<(), ApiError> {
    if worker.scope == ORIGINAL_SOURCE_VERIFICATION_SCOPE {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()))
    }
}

fn require_assignment_path(path: &str, body: &str) -> Result<(), ApiError> {
    if path == body {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Original-source verification assignment does not match the route.".to_string(),
        ))
    }
}

fn require_verifier_binding(
    state: &AppState,
    worker: &JobsWorkerIdentity,
    binding: &OriginalSourceVerifierBinding,
) -> Result<(), ApiError> {
    require_verifier_identity(worker, binding)?;
    jobs::require_original_source_verifier_runtime_active(
        &state.pool,
        &binding.worker_id,
        &binding.runtime_instance_id,
        &binding.runtime_session_token,
        binding.runtime_instance_epoch,
        &binding.runtime_authority_sha256,
    )
    .map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Original-source verifier runtime authority is unavailable.".to_string(),
        )
    })?;
    Ok(())
}

fn require_verifier_identity(
    worker: &JobsWorkerIdentity,
    binding: &OriginalSourceVerifierBinding,
) -> Result<(), ApiError> {
    if binding.worker_id == worker.worker_id {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()))
    }
}

fn original_source_verification_api_error(error: OriginalSourceVerificationError) -> ApiError {
    match error {
        OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Original-source verifier runtime authority is unavailable.".to_string(),
        ),
        OriginalSourceVerificationError::InvalidInput(_) => (
            StatusCode::BAD_REQUEST,
            "Invalid original-source verification request.".to_string(),
        ),
        OriginalSourceVerificationError::AssignmentNotFound => (
            StatusCode::NOT_FOUND,
            "Original-source verification assignment was not found.".to_string(),
        ),
        OriginalSourceVerificationError::LeaseLost
        | OriginalSourceVerificationError::LeaseExpired
        | OriginalSourceVerificationError::ConflictingReplay
        | OriginalSourceVerificationError::ConcurrentHeadAdvance => (
            StatusCode::CONFLICT,
            "Original-source verification authority changed; discard this result.".to_string(),
        ),
        OriginalSourceVerificationError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Original-source verification storage is unavailable.".to_string(),
        ),
    }
}

use axum::response::IntoResponse as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_route_and_body_must_match_exactly() {
        assert!(require_assignment_path(
            "source-verification-assignment-123",
            "source-verification-assignment-123"
        )
        .is_ok());
        assert_eq!(
            require_assignment_path(
                "source-verification-assignment-123",
                "source-verification-assignment-124"
            )
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn worker_scope_is_never_inferred_by_the_handler() {
        let exact = JobsWorkerIdentity {
            worker_id: "source-verifier-worker".to_string(),
            scope: ORIGINAL_SOURCE_VERIFICATION_SCOPE.to_string(),
        };
        assert!(require_verifier_scope(&exact).is_ok());
        let discovery = JobsWorkerIdentity {
            worker_id: "source-verifier-worker".to_string(),
            scope: "discovery".to_string(),
        };
        assert_eq!(
            require_verifier_scope(&discovery).unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn verifier_responses_are_non_cacheable_and_typed() {
        let response = authoritative_json(serde_json::json!({ "ok": true }));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );

        let empty = authoritative_empty();
        assert_eq!(empty.status(), StatusCode::NO_CONTENT);
        assert_eq!(empty.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(empty.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }
}

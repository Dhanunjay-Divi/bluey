//! HTTP boundary for the managed Bluey Jobs cloud release authority.
//!
//! Administrator routes import deployment authority. Private runtime routes
//! bind the fleet HMAC identity to the one-time grant and fenced instance.

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::{
    api::{jobs_worker_auth::JobsWorkerIdentity, AppState},
    auth::AuthedAccount,
    db::jobs::{
        self, ApplyManagedCloudActivationRequest, ApplyManagedCloudRollbackRequest,
        ClaimManagedCloudRuntimeGrant, ManagedCloudActivationImportRequest,
        ManagedCloudAuthorityEnvelope, ManagedCloudCohortImportRequest,
        ManagedCloudOriginalSourceVerifierStartupRequest, ManagedCloudReadinessQuery,
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
        .route(
            "/api/jobs/internal/managed-cloud/runtime-instances/:runtime_instance_id/original-source-verifier-startup",
            post(record_original_source_verifier_startup),
        )
        .route(
            "/api/jobs/internal/managed-cloud/runtime-instances/:runtime_instance_id/source-verifier-boot-registration",
            post(register_source_verifier_boot_endpoint),
        )
        .route(
            "/api/jobs/internal/managed-cloud/runtime-instances/:runtime_instance_id/source-verifier-boot-startup",
            post(record_source_verifier_boot_startup_endpoint),
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

async fn record_original_source_verifier_startup(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    Json(request): Json<ManagedCloudOriginalSourceVerifierStartupRequest>,
) -> Result<Response, ApiError> {
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    jobs::record_managed_cloud_original_source_verifier_startup(&state.pool, &request)
        .map(|result| authoritative_raw_json(result.canonical_response))
        .map_err(registry_api_error)
}

async fn register_source_verifier_boot_endpoint(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    Json(request): Json<jobs::SourceVerifierBootRegistrationRequest>,
) -> Result<Response, ApiError> {
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    let registration = jobs::register_source_verifier_boot(&state.pool, &request)
        .map_err(source_verifier_boot_api_error)?;
    let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(registration.canonical_response_base64url.as_bytes())
        .map_err(|_| invalid_request())?;
    Ok(authoritative_raw_json(canonical))
}

async fn record_source_verifier_boot_startup_endpoint(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    Json(request): Json<jobs::SourceVerifierBootStartupRequest>,
) -> Result<Response, ApiError> {
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    // The authority transaction owns the rollback-only probe and the ready CAS.
    // No client-supplied probe result can reach this boundary.
    let startup = jobs::record_source_verifier_boot_startup(&state.pool, &request)
        .map_err(source_verifier_boot_api_error)?;
    let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(startup.canonical_response_base64url.as_bytes())
        .map_err(|_| invalid_request())?;
    Ok(authoritative_raw_json(canonical))
}

fn source_verifier_boot_api_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("conflicts") {
        return (StatusCode::CONFLICT, message);
    }
    (
        StatusCode::BAD_REQUEST,
        "The source-verifier boot registration request is invalid.".to_string(),
    )
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

fn authoritative_raw_json(canonical: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(canonical));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
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
        ManagedCloudRegistryError::Storage(error) => {
            let boot_transition_conflict = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<postgres::Error>())
                .and_then(postgres::Error::as_db_error)
                .is_some_and(|database_error| {
                    database_error.code() == &postgres::error::SqlState::RAISE_EXCEPTION
                        && database_error.message()
                            == "source-verifier boot heartbeat transition is unauthorized"
                });
            if boot_transition_conflict {
                (
                    StatusCode::CONFLICT,
                    "The managed-cloud authority conflicts with current state.".to_string(),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "The managed-cloud authority operation failed.".to_string(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use base64::Engine;
    use tower::ServiceExt;

    #[test]
    fn postgres_boot_routes_with_authenticated_extension_persist_and_replay_when_configured() {
        let database_url = match std::env::var("BLUEY_TEST_POSTGRES_URL") {
            Ok(url) => url,
            Err(_) if std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref() == Ok("1") => {
                panic!("PostgreSQL boot HTTP regression requires BLUEY_TEST_POSTGRES_URL");
            }
            Err(_) => {
                eprintln!(
                    "SKIP PostgreSQL boot HTTP regression: BLUEY_TEST_POSTGRES_URL is unavailable"
                );
                return;
            }
        };
        let _guard = jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
            .lock()
            .expect("serialize PostgreSQL boot HTTP fixture");
        let pool =
            crate::db::open_postgres_pool(&database_url).expect("open actual HTTP PostgreSQL pool");
        crate::db::run_migrations(&pool).expect("migrate actual HTTP PostgreSQL pool");
        let fixture = jobs::managed_cloud_release_authority_tests::
            install_original_source_verifier_startup_request(&pool);
        let state = postgres_runtime_test_state(pool.clone(), database_url);
        // This boundary test deliberately injects an already-authenticated worker identity.
        // It exercises real Axum routing/JSON/handlers/PostgreSQL, not fleet HMAC or sockets.
        let app = worker_router()
            .with_state(state)
            .layer(Extension(JobsWorkerIdentity {
                worker_id: fixture.worker_id.clone(),
                scope: MANAGED_CLOUD_RUNTIME_SCOPE.to_string(),
            }));
        // Keep the shared synchronous fixture lock outside the async test future.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build bounded Axum test runtime");
        runtime.block_on(async {
            let suffix = uuid::Uuid::new_v4().simple().to_string();
            let base = format!("/api/jobs/internal/managed-cloud/runtime-instances/{}", fixture.runtime_instance_id);
            let registration_path = format!("{base}/source-verifier-boot-registration");
            let heartbeat_path = format!("{base}/heartbeats");
            let startup_path = format!("{base}/source-verifier-boot-startup");
            let registration = jobs::SourceVerifierBootRegistrationRequest {
                session_token: fixture.session_token.clone(),
                registration_request_id: format!("phase629-http-registration-{suffix}"),
                boot_id: format!("phase629-http-boot-{suffix}"),
                runtime_instance_id: fixture.runtime_instance_id.clone(),
                instance_epoch: fixture.instance_epoch, worker_id: fixture.worker_id.clone(),
                runtime_identity_sha256: fixture.runtime_identity_sha256.clone(),
                activation_sha256: fixture.activation_sha256.clone(),
                manifest_sha256: fixture.manifest_sha256.clone(),
                component_id: fixture.component_id.clone(), role: fixture.role.clone(),
            };
            let (status, headers, registration_bytes) = post_runtime_json(
                &app, &registration_path, serde_json::to_value(&registration).unwrap(),
            ).await;
            assert_eq!(status, StatusCode::OK);
            assert_runtime_headers(&headers);
            let registered: serde_json::Value = serde_json::from_slice(&registration_bytes).unwrap();
            let generation: i64 = registered["bootGeneration"].as_str().unwrap().parse().unwrap();
            assert!(generation > 1, "HTTP registration replaces the fixture's bootstrap boot");
            let mut heartbeat = serde_json::json!({
                "runtimeInstanceId": fixture.runtime_instance_id, "workerId": fixture.worker_id,
                "sessionToken": fixture.session_token, "heartbeatSequence": 2,
                "observedHeadRevision": fixture.observed_head_revision,
                "observedTransitionSha256": fixture.observed_transition_sha256,
                "activationSha256": fixture.activation_sha256, "manifestSha256": fixture.manifest_sha256,
                "componentId": fixture.component_id, "role": fixture.role, "artifactSha256": fixture.artifact_sha256,
                "migrationSetSha256": fixture.migration_set_sha256, "configSchemaSha256": fixture.config_schema_sha256,
                "protocolSetSha256": fixture.protocol_set_sha256, "taskQueueSha256": fixture.task_queue_sha256,
                "failureConverterSha256": fixture.failure_converter_sha256,
                "dependencyEvidenceSha256": fixture.dependency_evidence_sha256,
                "healthState": "ready", "reasonCode": null,
                "bootId": registration.boot_id, "bootGeneration": generation,
            });
            let before_forged_ready = postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id);
            let direct_input: ManagedCloudRuntimeHeartbeatInput =
                serde_json::from_value(heartbeat.clone()).expect("decode exact forged HTTP heartbeat");
            let error = jobs::record_managed_cloud_runtime_heartbeat(&pool, &direct_input)
                .expect_err("real PostgreSQL trigger must reject forged bound readiness");
            let ManagedCloudRegistryError::Storage(storage_error) = error else {
                panic!("expected PostgreSQL trigger denial, got {error:?}");
            };
            let database_error = storage_error.chain()
                .find_map(|cause| cause.downcast_ref::<postgres::Error>())
                .and_then(postgres::Error::as_db_error)
                .expect("forged readiness must expose the actual PostgreSQL trigger error");
            assert_eq!(database_error.code(), &postgres::error::SqlState::RAISE_EXCEPTION);
            assert_eq!(database_error.message(), "source-verifier boot heartbeat transition is unauthorized");
            assert_eq!(postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id), before_forged_ready,
                "direct trigger denial must roll back tentative boot binding and heartbeat changes");
            assert_eq!(post_runtime_json(&app, &heartbeat_path, heartbeat.clone()).await.0, StatusCode::CONFLICT);
            let mut unbound = heartbeat.clone();
            unbound["bootId"] = serde_json::Value::Null;
            unbound["bootGeneration"] = serde_json::Value::Null;
            assert_eq!(post_runtime_json(&app, &heartbeat_path, unbound).await.0, StatusCode::CONFLICT);
            assert_eq!(postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id), before_forged_ready);

            heartbeat["healthState"] = serde_json::json!("degraded");
            heartbeat["reasonCode"] = serde_json::json!("startup");
            let (status, headers, _) = post_runtime_json(&app, &heartbeat_path, heartbeat).await;
            assert_eq!(status, StatusCode::OK);
            assert_runtime_headers(&headers);
            let startup = jobs::SourceVerifierBootStartupRequest {
                session_token: fixture.session_token.clone(),
                startup_request_id: format!("phase629-http-startup-{suffix}"),
                boot_id: registration.boot_id.clone(), runtime_instance_id: fixture.runtime_instance_id.clone(),
                instance_epoch: fixture.instance_epoch, boot_generation: generation,
                worker_id: fixture.worker_id.clone(), degraded_heartbeat_sequence: 2,
            };
            let startup_json = serde_json::to_value(&startup).unwrap();
            let before_forged_result = postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id);
            let mut forged_result = startup_json.clone();
            forged_result["probeResult"] = serde_json::json!("leaseable");
            assert_eq!(post_runtime_json(&app, &startup_path, forged_result).await.0, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id), before_forged_result);
            let (status, headers, startup_bytes) = post_runtime_json(&app, &startup_path, startup_json.clone()).await;
            assert_eq!(status, StatusCode::OK);
            assert_runtime_headers(&headers);
            let response: serde_json::Value = serde_json::from_slice(&startup_bytes).unwrap();
            assert_eq!(response["readyHeartbeatSequence"], "3");
            assert_eq!(response["probeResult"], "idle");
            crate::db::run_blocking_db(|| {
                let mut connection = pool.get_pg().unwrap();
                let row = connection.query_one(
                    "SELECT heartbeat.heartbeat_sequence,heartbeat.health_state,
                            head.current_boot_generation,head.ready_boot_generation,
                            startup.canonical_response_base64url,
                            (SELECT COUNT(*) FROM jobs_managed_cloud_source_verifier_boot_startup_authorities
                              WHERE runtime_instance_id=$1),
                            (SELECT COUNT(*) FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit
                              WHERE runtime_instance_id=$1)
                       FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats heartbeat
                       JOIN jobs_managed_cloud_source_verifier_boot_generation_heads head
                         ON head.runtime_instance_id=heartbeat.runtime_instance_id
                        AND head.instance_epoch=heartbeat.instance_epoch
                       JOIN jobs_managed_cloud_source_verifier_boot_startup_authorities startup
                         ON startup.runtime_instance_id=head.runtime_instance_id AND startup.boot_id=$2
                      WHERE heartbeat.runtime_instance_id=$1",
                    &[&fixture.runtime_instance_id, &registration.boot_id],
                ).expect("read HTTP-published PostgreSQL startup authority");
                assert_eq!((row.get::<_, i64>(0), row.get::<_, String>(1).as_str()), (3, "ready"));
                assert_eq!((row.get::<_, i64>(2), row.get::<_, i64>(3)), (generation, generation));
                assert_eq!(row.get::<_, i64>(5), 1);
                assert_eq!(row.get::<_, i64>(6), 3);
                let stored: String = row.get(4);
                assert_eq!(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(stored).unwrap(), startup_bytes);
            });
            let before_replay = postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id);
            let (status, headers, replay_bytes) = post_runtime_json(&app, &startup_path, startup_json).await;
            assert_eq!(status, StatusCode::OK);
            assert_runtime_headers(&headers);
            assert_eq!(replay_bytes, startup_bytes);
            assert_eq!(postgres_runtime_snapshot(&pool, &fixture.runtime_instance_id), before_replay);
        });
    }

    fn postgres_runtime_test_state(pool: crate::db::DbPool, database_url: String) -> AppState {
        use crate::config::{Config, ServerDbBackend, TrialAbuseConfig, UpstreamKeys};
        AppState {
            pool,
            config: std::sync::Arc::new(Config {
                port: 0,
                db_path: std::path::PathBuf::new(),
                db_backend: ServerDbBackend::Postgres,
                database_url: Some(database_url),
                jwt_secret: "phase629-http-test-only-secret".to_string(),
                public_url: "http://localhost".to_string(),
                stripe_secret_key: None,
                stripe_webhook_secret: None,
                upstream: UpstreamKeys::default(),
                upstream_spend_guard: None,
                smtp: None,
                admin_emails: Vec::new(),
                trial_abuse: TrialAbuseConfig::default(),
                turnstile_site_key: None,
                turnstile_secret_key: None,
                require_turnstile: false,
                object_storage: None,
                log_storage: None,
            }),
            rate_limiters: crate::rate_limit::RateLimiters::default(),
            provider_health: crate::provider_health::ProviderHealth::default(),
        }
    }

    async fn post_runtime_json(
        app: &Router,
        path: &str,
        body: serde_json::Value,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let response = app
            .clone()
            .oneshot(
                Request::post(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes =
            axum::body::to_bytes(response.into_body(), MANAGED_CLOUD_RUNTIME_BODY_LIMIT_BYTES)
                .await
                .expect("read bounded HTTP response")
                .to_vec();
        (status, headers, bytes)
    }

    fn assert_runtime_headers(headers: &axum::http::HeaderMap) {
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }

    fn postgres_runtime_snapshot(pool: &crate::db::DbPool, runtime_id: &str) -> Vec<String> {
        crate::db::run_blocking_db(|| {
            let mut connection = pool.get_pg().unwrap();
            [
                "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
                "jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit",
                "jobs_managed_cloud_source_verifier_boot_registrations",
                "jobs_managed_cloud_source_verifier_boot_generation_heads",
                "jobs_managed_cloud_source_verifier_boot_startup_authorities",
                "jobs_managed_cloud_source_verifier_heartbeat_boot_bindings",
            ]
            .iter()
            .map(|table| {
                connection
                    .query_one(
                        &format!(
                "SELECT COALESCE(jsonb_agg(to_jsonb(item) ORDER BY to_jsonb(item)::text),'[]')::text
                   FROM {table} item WHERE runtime_instance_id=$1",
            ),
                        &[&runtime_id],
                    )
                    .expect("snapshot exact HTTP runtime authority rows")
                    .get(0)
            })
            .collect()
        })
    }

    #[test]
    fn unrelated_storage_errors_keep_internal_server_error_without_message_matching() {
        for message in [
            "source-verifier boot heartbeat transition is unauthorized",
            "unrelated database connection failure",
        ] {
            let response =
                registry_api_error(ManagedCloudRegistryError::Storage(anyhow::anyhow!(message)));
            assert_eq!(response.0, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(response.1, "The managed-cloud authority operation failed.");
        }
    }

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

    #[tokio::test]
    async fn authoritative_startup_replay_preserves_raw_canonical_bytes_and_headers() {
        let canonical = br#"{"audience":"bluey-jobs-managed-cloud-original-source-verifier-startup-v1","version":1}
"#
        .to_vec();
        let response = authoritative_raw_json(canonical.clone());
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        let body = axum::body::to_bytes(response.into_body(), canonical.len())
            .await
            .expect("read canonical startup response");
        assert_eq!(body.as_ref(), canonical);
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

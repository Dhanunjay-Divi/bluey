//! Authenticated administrator API for the Bluey Jobs Browser release registry.

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{
        self, ApplyBrowserReleaseActivationRequest, AssignBrowserReleaseChannelRequest,
        BrowserReleaseAuthorityEnvelope, BrowserReleaseChannelStatus, BrowserReleaseImportResult,
        BrowserReleaseManifestImportRequest, BrowserReleaseRegistryError,
    },
};

type ApiError = (StatusCode, String);

const BROWSER_RELEASE_ADMIN_BODY_LIMIT_BYTES: usize = 256 * 1024;

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/browser-releases/trust-policies",
            post(import_trust_policy),
        )
        .route(
            "/admin/jobs/browser-releases/manifests",
            post(import_manifest),
        )
        .route(
            "/admin/jobs/browser-releases/activations",
            post(import_activation),
        )
        .route(
            "/admin/jobs/browser-releases/activations/apply",
            post(apply_activation),
        )
        .route(
            "/admin/jobs/browser-releases/rollbacks",
            post(apply_rollback),
        )
        .route(
            "/admin/jobs/browser-releases/revocations",
            post(append_revocation),
        )
        .route(
            "/admin/jobs/browser-releases/accounts/:account_id/channel",
            post(assign_account_channel),
        )
        .route(
            "/admin/jobs/browser-releases/channels/:channel/status",
            get(channel_status),
        )
        .layer(DefaultBodyLimit::max(
            BROWSER_RELEASE_ADMIN_BODY_LIMIT_BYTES,
        ))
}

async fn import_trust_policy(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<BrowserReleaseAuthorityEnvelope>,
) -> Result<Json<BrowserReleaseImportResult>, ApiError> {
    jobs::import_browser_release_trust_policy(&state.pool, &envelope, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn import_manifest(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<BrowserReleaseManifestImportRequest>,
) -> Result<Json<BrowserReleaseImportResult>, ApiError> {
    jobs::import_browser_release_manifest(&state.pool, &request, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn import_activation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<BrowserReleaseAuthorityEnvelope>,
) -> Result<Json<BrowserReleaseImportResult>, ApiError> {
    jobs::import_browser_release_activation(&state.pool, &envelope, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn apply_activation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ApplyBrowserReleaseActivationRequest>,
) -> Result<Json<BrowserReleaseChannelStatus>, ApiError> {
    jobs::apply_browser_release_activation(&state.pool, &request, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn apply_rollback(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<BrowserReleaseAuthorityEnvelope>,
) -> Result<Json<BrowserReleaseChannelStatus>, ApiError> {
    jobs::apply_browser_release_rollback(&state.pool, &envelope, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn append_revocation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<BrowserReleaseAuthorityEnvelope>,
) -> Result<Json<BrowserReleaseImportResult>, ApiError> {
    jobs::append_browser_release_revocation(&state.pool, &envelope, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn assign_account_channel(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Path(account_id): Path<String>,
    Json(request): Json<AssignBrowserReleaseChannelRequest>,
) -> Result<Json<jobs::BrowserReleaseAccountChannelAssignment>, ApiError> {
    jobs::assign_browser_release_account_channel(&state.pool, &account_id, &request, &admin.0.id)
        .map(Json)
        .map_err(registry_api_error)
}

async fn channel_status(
    State(state): State<AppState>,
    Path(channel): Path<String>,
) -> Result<Json<BrowserReleaseChannelStatus>, ApiError> {
    jobs::browser_release_channel_status(&state.pool, &channel)
        .map(Json)
        .map_err(registry_api_error)
}

fn registry_api_error(error: BrowserReleaseRegistryError) -> ApiError {
    match error {
        BrowserReleaseRegistryError::InvalidEnvelope
        | BrowserReleaseRegistryError::InvalidAuthority
        | BrowserReleaseRegistryError::InvalidRequest => (
            StatusCode::BAD_REQUEST,
            "The Browser release authority request is invalid.".to_string(),
        ),
        BrowserReleaseRegistryError::NotFound => (
            StatusCode::NOT_FOUND,
            "The Browser release authority was not found.".to_string(),
        ),
        BrowserReleaseRegistryError::IdentityConflict
        | BrowserReleaseRegistryError::CompareAndSwapConflict
        | BrowserReleaseRegistryError::SequenceRegression
        | BrowserReleaseRegistryError::DowngradeRequiresRollback
        | BrowserReleaseRegistryError::Revoked => (
            StatusCode::CONFLICT,
            "The Browser release authority conflicts with current state.".to_string(),
        ),
        BrowserReleaseRegistryError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The Browser release authority operation failed.".to_string(),
        ),
    }
}

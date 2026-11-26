//! Durable public-beta authority for customer-facing Bluey Jobs routes.
//!
//! The public status projection is deliberately smaller than the internal
//! cohort record. Customer responses never expose account identifiers,
//! capacity, counts, position, or the compare-and-swap revision.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs_beta_access::{
        self, PublicBetaAccessDecision, PublicBetaAccessReason, PublicBetaCohort,
        PublicBetaCohortState, PublicBetaCohortUpdate, PublicBetaError, PublicBetaOverride,
    },
};

const SCHEMA_VERSION: u8 = 1;
const JOBS_BETA_DISABLED_MESSAGE: &str = "Bluey Jobs beta is not enabled.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PublicAccess {
    Admitted,
    NotAdmitted,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PublicReason {
    Admitted,
    VerificationRequired,
    NotOpen,
    WindowClosed,
    CapacityReached,
    Denied,
    Suspended,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicAccessResponse {
    schema_version: u8,
    access: PublicAccess,
    reason: PublicReason,
}

impl PublicAccessResponse {
    const fn new(access: PublicAccess, reason: PublicReason) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            access,
            reason,
        }
    }

    const fn verification_required() -> Self {
        Self::new(
            PublicAccess::NotAdmitted,
            PublicReason::VerificationRequired,
        )
    }

    const fn unavailable() -> Self {
        Self::new(PublicAccess::NotAdmitted, PublicReason::Unavailable)
    }
}

pub fn access_router() -> Router<AppState> {
    Router::new().route("/api/jobs/beta-access", get(beta_access))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/public-beta",
            get(admin_get_cohort).put(admin_update_cohort),
        )
        .route(
            "/admin/jobs/public-beta/accounts/:account_id/grant",
            post(admin_grant_account),
        )
        .route(
            "/admin/jobs/public-beta/accounts/:account_id/override",
            put(admin_set_override),
        )
}

pub async fn require_jobs_master(request: Request<Body>, next: Next) -> Response {
    if !jobs_beta_enabled() {
        return jobs_disabled_response();
    }
    next.run(request).await
}

pub async fn require_customer_access(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !jobs_beta_enabled() {
        return jobs_disabled_response();
    }

    let Some(account) = request.extensions().get::<AuthedAccount>() else {
        // Authentication must wrap this middleware in every composition. A
        // missing extension is a fail-closed composition error.
        return private_no_store(StatusCode::UNAUTHORIZED.into_response());
    };
    if !account_is_eligible(&account.0) {
        return public_access_response(
            StatusCode::FORBIDDEN,
            PublicAccessResponse::verification_required(),
        );
    }

    match jobs_beta_access::evaluate_or_enroll_public_beta(&state.pool, &account.0.id) {
        Ok(decision) if decision.reason == PublicBetaAccessReason::Admitted => {
            next.run(request).await
        }
        Ok(decision) => public_access_response(StatusCode::FORBIDDEN, public_projection(decision)),
        Err(error) => {
            log_access_error(&account.0.id, &error);
            let (status, projection) = status_access_error_projection(&error);
            let status = if status == StatusCode::OK {
                StatusCode::FORBIDDEN
            } else {
                status
            };
            public_access_response(status, projection)
        }
    }
}

async fn beta_access(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Response {
    if !jobs_beta_enabled() {
        return jobs_disabled_response();
    }
    if !account_is_eligible(&account) {
        return public_access_response(
            StatusCode::OK,
            PublicAccessResponse::verification_required(),
        );
    }

    match jobs_beta_access::evaluate_or_enroll_public_beta(&state.pool, &account.id) {
        Ok(decision) => public_access_response(StatusCode::OK, public_projection(decision)),
        Err(error) => {
            log_access_error(&account.id, &error);
            let (status, projection) = status_access_error_projection(&error);
            public_access_response(status, projection)
        }
    }
}

fn account_is_eligible(account: &crate::db::accounts::Account) -> bool {
    !account.is_temporary && account.email_verified_at.is_some()
}

fn public_projection(decision: PublicBetaAccessDecision) -> PublicAccessResponse {
    match decision.reason {
        PublicBetaAccessReason::Admitted => {
            PublicAccessResponse::new(PublicAccess::Admitted, PublicReason::Admitted)
        }
        PublicBetaAccessReason::Denied => {
            PublicAccessResponse::new(PublicAccess::NotAdmitted, PublicReason::Denied)
        }
        PublicBetaAccessReason::Suspended => {
            PublicAccessResponse::new(PublicAccess::Suspended, PublicReason::Suspended)
        }
        PublicBetaAccessReason::NotOpen => {
            PublicAccessResponse::new(PublicAccess::NotAdmitted, PublicReason::NotOpen)
        }
        PublicBetaAccessReason::WindowClosed => {
            PublicAccessResponse::new(PublicAccess::NotAdmitted, PublicReason::WindowClosed)
        }
        PublicBetaAccessReason::CapacityReached => {
            PublicAccessResponse::new(PublicAccess::NotAdmitted, PublicReason::CapacityReached)
        }
    }
}

fn status_access_error_projection(error: &PublicBetaError) -> (StatusCode, PublicAccessResponse) {
    match error {
        PublicBetaError::CapacityReached => (
            StatusCode::OK,
            PublicAccessResponse::new(PublicAccess::NotAdmitted, PublicReason::CapacityReached),
        ),
        PublicBetaError::AccountIneligible => (
            StatusCode::OK,
            PublicAccessResponse::verification_required(),
        ),
        PublicBetaError::Conflict { .. }
        | PublicBetaError::Invalid(_)
        | PublicBetaError::AccountNotFound
        | PublicBetaError::AccountDeletionPending
        | PublicBetaError::AdminActorRequired
        | PublicBetaError::Database(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            PublicAccessResponse::unavailable(),
        ),
    }
}

fn jobs_beta_enabled() -> bool {
    jobs_beta_access::public_beta_master_enabled()
}

fn jobs_disabled_response() -> Response {
    private_no_store((StatusCode::NOT_FOUND, JOBS_BETA_DISABLED_MESSAGE).into_response())
}

fn public_access_response(status: StatusCode, body: PublicAccessResponse) -> Response {
    private_no_store((status, Json(body)).into_response())
}

fn private_no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Authorization"));
    response
}

fn log_access_error(account_id: &str, error: &PublicBetaError) {
    tracing::warn!(
        account_id_hash = %cue_core::account_id_hash_prefix(account_id),
        error_class = public_beta_error_class(error),
        "public beta access authority failed closed"
    );
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AdminCohortState {
    Draft,
    Open,
    ClosedToNew,
    Suspended,
}

impl From<AdminCohortState> for PublicBetaCohortState {
    fn from(value: AdminCohortState) -> Self {
        match value {
            AdminCohortState::Draft => Self::Draft,
            AdminCohortState::Open => Self::Open,
            AdminCohortState::ClosedToNew => Self::ClosedToNew,
            AdminCohortState::Suspended => Self::Suspended,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdminUpdateCohortRequest {
    expected_revision: i64,
    state: AdminCohortState,
    opens_at_ms: Option<i64>,
    closes_at_ms: Option<i64>,
    hard_cap: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdminGrantRequest {
    expected_revision: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdminOverrideRequest {
    expected_revision: i64,
    denied: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminCohortResponse {
    schema_version: u8,
    state: &'static str,
    opens_at_ms: Option<i64>,
    closes_at_ms: Option<i64>,
    hard_cap: i64,
    assigned_count: i64,
    revision: i64,
}

impl From<PublicBetaCohort> for AdminCohortResponse {
    fn from(value: PublicBetaCohort) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            state: cohort_state_name(value.state),
            opens_at_ms: value.opens_at_ms,
            closes_at_ms: value.closes_at_ms,
            hard_cap: value.hard_cap,
            assigned_count: value.assigned_count,
            revision: value.revision,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminOverrideResponse {
    schema_version: u8,
    denied: bool,
    revision: i64,
}

impl From<PublicBetaOverride> for AdminOverrideResponse {
    fn from(value: PublicBetaOverride) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            denied: value.denied,
            revision: value.revision,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminErrorResponse {
    schema_version: u8,
    error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_revision: Option<i64>,
}

async fn admin_get_cohort(State(state): State<AppState>) -> Response {
    match jobs_beta_access::get_public_beta_cohort(&state.pool) {
        Ok(cohort) => private_no_store(
            (StatusCode::OK, Json(AdminCohortResponse::from(cohort))).into_response(),
        ),
        Err(error) => admin_error_response(&error),
    }
}

async fn admin_update_cohort(
    State(state): State<AppState>,
    Extension(AuthedAccount(actor)): Extension<AuthedAccount>,
    Json(request): Json<AdminUpdateCohortRequest>,
) -> Response {
    let update = PublicBetaCohortUpdate {
        expected_revision: request.expected_revision,
        state: request.state.into(),
        opens_at_ms: request.opens_at_ms,
        closes_at_ms: request.closes_at_ms,
        hard_cap: request.hard_cap,
    };
    match jobs_beta_access::update_public_beta_cohort_audited(&state.pool, update, &actor.id) {
        Ok(cohort) => private_no_store(
            (StatusCode::OK, Json(AdminCohortResponse::from(cohort))).into_response(),
        ),
        Err(error) => admin_error_response(&error),
    }
}

async fn admin_grant_account(
    State(state): State<AppState>,
    Extension(AuthedAccount(actor)): Extension<AuthedAccount>,
    Path(account_id): Path<String>,
    Json(request): Json<AdminGrantRequest>,
) -> Response {
    match jobs_beta_access::grant_public_beta_access_audited(
        &state.pool,
        &account_id,
        request.expected_revision,
        &actor.id,
    ) {
        Ok(decision) => public_access_response(StatusCode::OK, public_projection(decision)),
        Err(error) => admin_error_response(&error),
    }
}

async fn admin_set_override(
    State(state): State<AppState>,
    Extension(AuthedAccount(actor)): Extension<AuthedAccount>,
    Path(account_id): Path<String>,
    Json(request): Json<AdminOverrideRequest>,
) -> Response {
    match jobs_beta_access::set_public_beta_override_audited(
        &state.pool,
        &account_id,
        request.expected_revision,
        request.denied,
        &actor.id,
    ) {
        Ok(override_record) => private_no_store(
            (
                StatusCode::OK,
                Json(AdminOverrideResponse::from(override_record)),
            )
                .into_response(),
        ),
        Err(error) => admin_error_response(&error),
    }
}

fn admin_error_response(error: &PublicBetaError) -> Response {
    let (status, error_name, current_revision) = match error {
        PublicBetaError::Conflict { current_revision } => (
            StatusCode::CONFLICT,
            "revision_conflict",
            Some(*current_revision),
        ),
        PublicBetaError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request", None),
        PublicBetaError::CapacityReached => (StatusCode::CONFLICT, "capacity_reached", None),
        PublicBetaError::AccountNotFound => (StatusCode::NOT_FOUND, "account_not_found", None),
        PublicBetaError::AccountIneligible => (StatusCode::CONFLICT, "account_ineligible", None),
        PublicBetaError::AccountDeletionPending => {
            (StatusCode::CONFLICT, "account_deletion_pending", None)
        }
        PublicBetaError::AdminActorRequired => {
            (StatusCode::FORBIDDEN, "admin_actor_required", None)
        }
        PublicBetaError::Database(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", None),
    };
    private_no_store(
        (
            status,
            Json(AdminErrorResponse {
                schema_version: SCHEMA_VERSION,
                error: error_name,
                current_revision,
            }),
        )
            .into_response(),
    )
}

fn cohort_state_name(state: PublicBetaCohortState) -> &'static str {
    match state {
        PublicBetaCohortState::Draft => "draft",
        PublicBetaCohortState::Open => "open",
        PublicBetaCohortState::ClosedToNew => "closed_to_new",
        PublicBetaCohortState::Suspended => "suspended",
    }
}

fn public_beta_error_class(error: &PublicBetaError) -> &'static str {
    match error {
        PublicBetaError::Conflict { .. } => "revision_conflict",
        PublicBetaError::Invalid(_) => "invalid_request",
        PublicBetaError::CapacityReached => "capacity_reached",
        PublicBetaError::AccountNotFound => "account_not_found",
        PublicBetaError::AccountIneligible => "account_ineligible",
        PublicBetaError::AccountDeletionPending => "account_deletion_pending",
        PublicBetaError::AdminActorRequired => "admin_actor_required",
        PublicBetaError::Database(_) => "database",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_private_no_store(response: &Response) {
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store, max-age=0"
        );
        assert_eq!(response.headers()[header::PRAGMA], "no-cache");
        assert_eq!(response.headers()[header::VARY], "Authorization");
    }

    #[test]
    fn public_projection_is_closed_and_contains_no_capacity_or_identity() {
        let response =
            PublicAccessResponse::new(PublicAccess::NotAdmitted, PublicReason::CapacityReached);
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(
            value,
            json!({
                "schemaVersion": 1,
                "access": "not_admitted",
                "reason": "capacity_reached"
            })
        );
    }

    #[test]
    fn every_durable_access_reason_has_an_exact_public_projection() {
        let cases = [
            (PublicBetaAccessReason::Admitted, "admitted", "admitted"),
            (PublicBetaAccessReason::Denied, "not_admitted", "denied"),
            (PublicBetaAccessReason::Suspended, "suspended", "suspended"),
            (PublicBetaAccessReason::NotOpen, "not_admitted", "not_open"),
            (
                PublicBetaAccessReason::WindowClosed,
                "not_admitted",
                "window_closed",
            ),
            (
                PublicBetaAccessReason::CapacityReached,
                "not_admitted",
                "capacity_reached",
            ),
        ];
        for (reason, expected_access, expected_reason) in cases {
            let value =
                serde_json::to_value(public_projection(PublicBetaAccessDecision { reason }))
                    .unwrap();
            assert_eq!(value["access"], expected_access);
            assert_eq!(value["reason"], expected_reason);
            assert_eq!(value.as_object().unwrap().len(), 3);
        }
    }

    #[test]
    fn admin_requests_reject_unknown_fields() {
        let update = json!({
            "expectedRevision": 1,
            "state": "open",
            "opensAtMs": 1,
            "closesAtMs": 2,
            "hardCap": 25,
            "accountId": "must-not-be-accepted"
        });
        assert!(serde_json::from_value::<AdminUpdateCohortRequest>(update).is_err());

        let grant = json!({
            "expectedRevision": 1,
            "bypassCapacity": true
        });
        assert!(serde_json::from_value::<AdminGrantRequest>(grant).is_err());

        let override_request = json!({
            "expectedRevision": 0,
            "denied": true,
            "reason": "free-form-text-is-not-part-of-the-contract"
        });
        assert!(serde_json::from_value::<AdminOverrideRequest>(override_request).is_err());
    }

    #[test]
    fn every_beta_projection_is_private_and_uncacheable() {
        let master_off = jobs_disabled_response();
        assert_eq!(master_off.status(), StatusCode::NOT_FOUND);
        assert_private_no_store(&master_off);

        let status = public_access_response(
            StatusCode::OK,
            PublicAccessResponse::new(PublicAccess::Admitted, PublicReason::Admitted),
        );
        assert_private_no_store(&status);

        let unavailable = admin_error_response(&PublicBetaError::AdminActorRequired);
        assert_eq!(unavailable.status(), StatusCode::FORBIDDEN);
        assert_private_no_store(&unavailable);

        let missing_auth = private_no_store(StatusCode::UNAUTHORIZED.into_response());
        assert_private_no_store(&missing_auth);
    }
}

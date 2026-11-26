//! Protected Bluey Jobs operational safety administration and readiness API.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use axum::{
    extract::{rejection::JsonRejection, DefaultBodyLimit, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{from_fn, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use super::{
    metrics::{OPERATIONAL_CAPABILITIES, OPERATIONAL_SCOPE_KINDS},
    AppState,
};
use crate::{
    auth::AuthedAccount,
    db::{
        jobs::{
            self, AppendOperationalHoldEventByRefRequest, AppendOperationalHoldEventRequest,
            OperationalCapability, OperationalHoldError, OperationalHoldPublicState,
        },
        metrics::{JobsReadinessSnapshot, OperationalHoldMetric},
    },
};

const OPERATIONAL_HOLD_BODY_LIMIT_BYTES: usize = 64 * 1024;
const OPERATIONAL_HOLD_DEFAULT_LIMIT: usize = 50;
const OPERATIONAL_HOLD_MAX_API_LIMIT: usize = 100;
const OPERATIONAL_HOLD_MAX_CURSOR_BYTES: usize = 2_048;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OperationalHoldMutationRequest {
    Raw(AppendOperationalHoldEventRequest),
    ByRef(AppendOperationalHoldEventByRefRequest),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationalHoldListQuery {
    active_only: Option<bool>,
    limit: Option<usize>,
    cursor: Option<String>,
}

impl OperationalHoldListQuery {
    fn validated(self) -> Result<(bool, usize, Option<String>)> {
        let limit = self.limit.unwrap_or(OPERATIONAL_HOLD_DEFAULT_LIMIT);
        if !(1..=OPERATIONAL_HOLD_MAX_API_LIMIT).contains(&limit)
            || self.cursor.as_deref().is_some_and(|cursor| {
                cursor.is_empty() || cursor.len() > OPERATIONAL_HOLD_MAX_CURSOR_BYTES
            })
        {
            bail!("invalid operational hold list limit")
        }
        Ok((self.active_only.unwrap_or(true), limit, self.cursor))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationalHoldListResponse {
    active_only: bool,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
    holds: Vec<OperationalHoldPublicState>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct JobsCapabilityReadiness {
    capability: OperationalCapability,
    ready: bool,
    blocker_count: i64,
    operational_hold_count: i64,
    native_blocker_count: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct JobsReadinessResponse {
    schema_version: i64,
    ready: bool,
    capabilities: Vec<JobsCapabilityReadiness>,
    paused_discovery_source_count: i64,
    open_ats_circuit_count: i64,
    evaluated_at_ms: i64,
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/operational-holds/events",
            post(append_operational_hold),
        )
        .route("/admin/jobs/operational-holds", get(list_operational_holds))
        .route("/admin/jobs/readiness", get(jobs_readiness))
        .layer(DefaultBodyLimit::max(OPERATIONAL_HOLD_BODY_LIMIT_BYTES))
        .layer(from_fn(private_no_store))
}

async fn append_operational_hold(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    payload: std::result::Result<Json<OperationalHoldMutationRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match payload {
        Ok(request) => request,
        Err(rejection) => {
            return private_error(
                rejection.status(),
                "The operational hold request is invalid.",
            )
        }
    };
    let result = match request {
        OperationalHoldMutationRequest::Raw(request) => {
            jobs::append_operational_hold_event(&state.pool, &request, &admin.0.id)
        }
        OperationalHoldMutationRequest::ByRef(request) => {
            jobs::append_operational_hold_event_by_ref(&state.pool, &request, &admin.0.id)
        }
    };
    match result {
        Ok(result) => {
            let status = if result.replayed {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            private_json(status, result)
        }
        Err(error) => operational_hold_error_response(error),
    }
}

async fn list_operational_holds(
    State(state): State<AppState>,
    query: std::result::Result<
        Query<OperationalHoldListQuery>,
        axum::extract::rejection::QueryRejection,
    >,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => {
            return private_error(
                StatusCode::BAD_REQUEST,
                "The operational hold query is invalid.",
            )
        }
    };
    let (active_only, limit, cursor) = match query.validated() {
        Ok(validated) => validated,
        Err(_) => {
            return private_error(
                StatusCode::BAD_REQUEST,
                "The operational hold query is invalid.",
            )
        }
    };
    match jobs::list_operational_hold_states(&state.pool, active_only, limit, cursor.as_deref()) {
        Ok(page) => {
            let holds = match page
                .states
                .into_iter()
                .map(|state| state.redacted())
                .collect::<std::result::Result<Vec<_>, _>>()
            {
                Ok(holds) => holds,
                Err(error) => return operational_hold_error_response(error),
            };
            private_json(
                StatusCode::OK,
                OperationalHoldListResponse {
                    active_only,
                    limit,
                    next_cursor: page.next_cursor,
                    holds,
                },
            )
        }
        Err(error) => operational_hold_error_response(error),
    }
}

async fn jobs_readiness(State(state): State<AppState>) -> Response {
    let snapshot = match crate::db::metrics::jobs_readiness_snapshot(&state.pool) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            tracing::error!("Jobs readiness snapshot unavailable");
            return private_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Jobs readiness is unavailable.",
            );
        }
    };
    match build_readiness(snapshot, chrono::Utc::now().timestamp_millis()) {
        Ok(readiness) => private_json(StatusCode::OK, readiness),
        Err(_) => {
            tracing::error!("Jobs readiness contained an invalid closed dimension");
            private_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Jobs readiness is unavailable.",
            )
        }
    }
}

fn build_readiness(
    snapshot: JobsReadinessSnapshot,
    evaluated_at_ms: i64,
) -> Result<JobsReadinessResponse> {
    if evaluated_at_ms < 0
        || snapshot.paused_discovery_sources < 0
        || snapshot.open_ats_circuits < 0
    {
        bail!("invalid Jobs readiness count")
    }
    let active_counts = validated_active_counts(&snapshot.operational_holds)?;
    let all_hold_count = capability_hold_count(&active_counts, "all")?;
    let mut capabilities = Vec::with_capacity(OperationalCapability::CONCRETE.len());
    for capability in OperationalCapability::CONCRETE {
        let operational_hold_count = all_hold_count
            .checked_add(capability_hold_count(&active_counts, capability.as_str())?)
            .ok_or_else(|| anyhow::anyhow!("operational hold count overflow"))?;
        let native_blocker_count = match capability {
            OperationalCapability::Discovery => snapshot.paused_discovery_sources,
            OperationalCapability::FinalSubmit => snapshot.open_ats_circuits,
            _ => 0,
        };
        let blocker_count = operational_hold_count
            .checked_add(native_blocker_count)
            .ok_or_else(|| anyhow::anyhow!("Jobs readiness count overflow"))?;
        capabilities.push(JobsCapabilityReadiness {
            capability,
            ready: blocker_count == 0,
            blocker_count,
            operational_hold_count,
            native_blocker_count,
        });
    }
    Ok(JobsReadinessResponse {
        schema_version: 1,
        ready: capabilities.iter().all(|capability| capability.ready),
        capabilities,
        paused_discovery_source_count: snapshot.paused_discovery_sources,
        open_ats_circuit_count: snapshot.open_ats_circuits,
        evaluated_at_ms,
    })
}

fn validated_active_counts(
    metrics: &[OperationalHoldMetric],
) -> Result<BTreeMap<(&str, &str), i64>> {
    let mut active_counts = BTreeMap::new();
    for metric in metrics {
        if !OPERATIONAL_CAPABILITIES.contains(&metric.capability.as_str())
            || !OPERATIONAL_SCOPE_KINDS.contains(&metric.scope_kind.as_str())
            || metric.active_count < 0
        {
            bail!("invalid operational hold readiness dimension")
        }
        if active_counts
            .insert(
                (metric.capability.as_str(), metric.scope_kind.as_str()),
                metric.active_count,
            )
            .is_some()
        {
            bail!("duplicate operational hold readiness dimension")
        }
    }
    Ok(active_counts)
}

fn capability_hold_count(
    active_counts: &BTreeMap<(&str, &str), i64>,
    capability: &str,
) -> Result<i64> {
    OPERATIONAL_SCOPE_KINDS
        .iter()
        .try_fold(0_i64, |count, scope_kind| {
            count
                .checked_add(
                    active_counts
                        .get(&(capability, *scope_kind))
                        .copied()
                        .unwrap_or(0),
                )
                .ok_or_else(|| anyhow::anyhow!("operational hold count overflow"))
        })
}

fn operational_hold_error_response(error: OperationalHoldError) -> Response {
    match error {
        OperationalHoldError::InvalidRequest => private_error(
            StatusCode::BAD_REQUEST,
            "The operational hold request is invalid.",
        ),
        OperationalHoldError::NotFound
        | OperationalHoldError::Conflict
        | OperationalHoldError::IdentityConflict
        | OperationalHoldError::Held(_) => private_error(
            StatusCode::CONFLICT,
            "The operational hold request conflicts with current state.",
        ),
        OperationalHoldError::Storage(_) => {
            tracing::error!("operational hold storage operation unavailable");
            private_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "The operational hold operation is unavailable.",
            )
        }
    }
}

fn private_json<T: Serialize>(status: StatusCode, value: T) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(value),
    )
        .into_response()
}

fn private_error(status: StatusCode, message: &'static str) -> Response {
    private_json(status, serde_json::json!({ "error": message }))
}

async fn private_no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    set_private_headers(&mut response);
    response
}

pub(crate) async fn private_admin_no_store(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let private_admin_response = path.starts_with("/admin/") || path == "/api/jobs/beta-access";
    let mut response = next.run(request).await;
    if private_admin_response {
        set_private_headers(&mut response);
    }
    response
}

fn set_private_headers(response: &mut Response) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Authorization"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::jobs::{
        OperationalHoldReasonCode, OperationalHoldScopeKind, OperationalHoldState,
        OperationalHoldTransition,
    };
    use crate::{
        config::{Config, ServerDbBackend, TrialAbuseConfig, UpstreamKeys},
        db::accounts::Account,
    };
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    struct TempDbPath {
        path: std::path::PathBuf,
    }

    impl TempDbPath {
        fn new(label: &str) -> Self {
            Self {
                path: std::env::temp_dir()
                    .join(format!("bluey-{label}-{}.db", uuid::Uuid::new_v4())),
            }
        }
    }

    impl Drop for TempDbPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(format!("{}-wal", self.path.display()));
            let _ = std::fs::remove_file(format!("{}-shm", self.path.display()));
        }
    }

    fn test_config(db_path: std::path::PathBuf) -> Config {
        Config {
            port: 0,
            db_path,
            db_backend: ServerDbBackend::Sqlite,
            database_url: None,
            jwt_secret: "jobs-operations-test-secret".to_string(),
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
        }
    }

    fn admin_account() -> Account {
        Account {
            id: "admin-jobs-operations".to_string(),
            email: "admin@example.com".to_string(),
            email_verified_at: None,
            balance_cents: 0,
            trial_seconds_remaining: 0,
            is_temporary: false,
            temporary_expires_at: None,
            auto_topup_enabled: false,
            auto_topup_threshold_cents: 0,
            auto_topup_amount_cents: 0,
            is_admin: true,
            stripe_customer_id: None,
            stripe_payment_method_id: None,
            square_customer_id: None,
            square_card_id: None,
            square_card_brand: None,
            square_card_last4: None,
            billing_restricted: false,
            billing_restriction_reason: None,
            billing_restricted_at: None,
        }
    }

    fn test_state(pool: crate::db::DbPool, config: Config) -> AppState {
        AppState {
            pool,
            config: std::sync::Arc::new(config),
            rate_limiters: crate::rate_limit::RateLimiters::default(),
            provider_health: crate::provider_health::ProviderHealth::default(),
        }
    }

    #[test]
    fn list_query_is_strict_and_bounded() {
        let query = OperationalHoldListQuery {
            active_only: Some(false),
            limit: Some(100),
            cursor: None,
        };
        assert_eq!(query.validated().unwrap(), (false, 100, None));
        assert!(OperationalHoldListQuery {
            active_only: None,
            limit: Some(0),
            cursor: None,
        }
        .validated()
        .is_err());
        assert!(OperationalHoldListQuery {
            active_only: None,
            limit: Some(101),
            cursor: None,
        }
        .validated()
        .is_err());
        assert!(OperationalHoldListQuery {
            active_only: None,
            limit: None,
            cursor: Some("x".repeat(OPERATIONAL_HOLD_MAX_CURSOR_BYTES + 1)),
        }
        .validated()
        .is_err());
        assert!(
            serde_json::from_value::<OperationalHoldListQuery>(serde_json::json!({
                "scopeId": "private"
            }))
            .is_err()
        );
    }

    #[test]
    fn operational_hold_json_is_private_and_non_storable() {
        let response = private_json(StatusCode::OK, serde_json::json!({ "ready": true }));
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(response.headers()[header::PRAGMA], "no-cache");
    }

    #[test]
    fn public_hold_state_redacts_scope_reason_actor_and_event_identity() {
        let private_scope = "private-account-id";
        let state = OperationalHoldState {
            capability: OperationalCapability::Generation,
            scope_kind: OperationalHoldScopeKind::Account,
            scope_id: private_scope.to_string(),
            head_revision: 3,
            current_event_id: "private-event-id".to_string(),
            event_sha256: "ab".repeat(32),
            state: OperationalHoldTransition::Held,
            reason_code: OperationalHoldReasonCode::Incident,
            reason_ref: Some("private-reason-ref".to_string()),
            recorded_by: "private-actor-id".to_string(),
            recorded_at_ms: 123,
        };
        let public_state = state.redacted().unwrap();
        let encoded = serde_json::to_string(&public_state).unwrap();
        assert!(encoded.contains("scopeRef"));
        assert!(encoded.contains("currentEventRef"));
        assert_eq!(public_state.scope_ref.len(), "scope-".len() + 64);
        assert_eq!(public_state.current_event_ref.len(), "event-".len() + 64);
        assert!(encoded.contains("eventSha256"));
        for forbidden in [
            private_scope,
            "private-event-id",
            "private-reason-ref",
            "private-actor-id",
            "scopeId",
            "reasonRef",
            "recordedBy",
            "currentEventId",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn readiness_composes_global_specific_and_native_blockers() {
        let readiness = build_readiness(
            JobsReadinessSnapshot {
                operational_holds: vec![
                    OperationalHoldMetric {
                        capability: "all".to_string(),
                        scope_kind: "global".to_string(),
                        active_count: 1,
                    },
                    OperationalHoldMetric {
                        capability: "generation".to_string(),
                        scope_kind: "account".to_string(),
                        active_count: 2,
                    },
                ],
                paused_discovery_sources: 3,
                open_ats_circuits: 4,
            },
            100,
        )
        .unwrap();

        assert!(!readiness.ready);
        assert_eq!(readiness.capabilities.len(), 8);
        let original_source_verification = readiness
            .capabilities
            .iter()
            .find(|value| value.capability == OperationalCapability::OriginalSourceVerification)
            .unwrap();
        assert_eq!(original_source_verification.operational_hold_count, 1);
        assert_eq!(original_source_verification.native_blocker_count, 0);
        assert_eq!(original_source_verification.blocker_count, 1);
        let generation = readiness
            .capabilities
            .iter()
            .find(|value| value.capability == OperationalCapability::Generation)
            .unwrap();
        assert_eq!(generation.operational_hold_count, 3);
        assert_eq!(generation.native_blocker_count, 0);
        assert_eq!(generation.blocker_count, 3);
        let discovery = readiness
            .capabilities
            .iter()
            .find(|value| value.capability == OperationalCapability::Discovery)
            .unwrap();
        assert_eq!(discovery.operational_hold_count, 1);
        assert_eq!(discovery.native_blocker_count, 3);
        assert_eq!(discovery.blocker_count, 4);
        let final_submit = readiness
            .capabilities
            .iter()
            .find(|value| value.capability == OperationalCapability::FinalSubmit)
            .unwrap();
        assert_eq!(final_submit.operational_hold_count, 1);
        assert_eq!(final_submit.native_blocker_count, 4);
        assert_eq!(final_submit.blocker_count, 5);
    }

    #[test]
    fn readiness_fails_closed_on_private_or_unknown_dimensions() {
        let result = build_readiness(
            JobsReadinessSnapshot {
                operational_holds: vec![OperationalHoldMetric {
                    capability: "private-account-id".to_string(),
                    scope_kind: "account".to_string(),
                    active_count: 1,
                }],
                ..JobsReadinessSnapshot::default()
            },
            100,
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn admin_routes_mutate_list_and_report_readiness_without_private_ids() {
        let temp_db = TempDbPath::new("jobs-operations");
        let db_path = temp_db.path.clone();
        let pool = crate::db::open_pool(&db_path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('private-account-id', 'private-account@example.test', 'hash', 0)",
                [],
            )
            .unwrap();
        let app = admin_router()
            .with_state(test_state(pool, test_config(db_path)))
            .layer(Extension(AuthedAccount(admin_account())));

        let append = app
            .clone()
            .oneshot(
                Request::post("/admin/jobs/operational-holds/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "eventId": "event-api-1",
                            "capability": "generation",
                            "scopeKind": "account",
                            "scopeId": "private-account-id",
                            "transition": "held",
                            "reasonCode": "incident",
                            "reasonRef": "private-reason-ref",
                            "expectedHeadRevision": 0,
                            "expectedCurrentEventId": null
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(append.status(), StatusCode::CREATED);
        assert_eq!(
            append.headers()[header::CACHE_CONTROL],
            "private, no-store, max-age=0"
        );
        assert_eq!(append.headers()[header::PRAGMA], "no-cache");
        assert_eq!(append.headers()[header::VARY], "Authorization");
        let append_body = axum::body::to_bytes(append.into_body(), 64 * 1024)
            .await
            .unwrap();
        let append_body = String::from_utf8(append_body.to_vec()).unwrap();
        assert!(append_body.contains("scopeRef"));
        for forbidden in [
            "private-account-id",
            "private-reason-ref",
            "admin-jobs-operations",
            "event-api-1",
        ] {
            assert!(!append_body.contains(forbidden));
        }

        let list = app
            .clone()
            .oneshot(
                Request::get("/admin/jobs/operational-holds?activeOnly=true&limit=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let list_body = axum::body::to_bytes(list.into_body(), 64 * 1024)
            .await
            .unwrap();
        let list: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
        let list_body = list.to_string();
        assert!(list_body.contains("scopeRef"));
        assert!(list_body.contains("currentEventRef"));
        assert!(!list_body.contains("private-account-id"));
        assert!(!list_body.contains("private-reason-ref"));
        let held = &list["holds"][0];
        let scope_ref = held["scopeRef"].as_str().unwrap().to_string();
        let current_event_ref = held["currentEventRef"].as_str().unwrap().to_string();

        let readiness = app
            .clone()
            .oneshot(
                Request::get("/admin/jobs/readiness")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(readiness.status(), StatusCode::OK);
        let readiness_body = axum::body::to_bytes(readiness.into_body(), 64 * 1024)
            .await
            .unwrap();
        let readiness: serde_json::Value = serde_json::from_slice(&readiness_body).unwrap();
        assert_eq!(readiness["ready"], false);
        assert_eq!(
            readiness["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["capability"] == "generation")
                .unwrap()["blockerCount"],
            1
        );
        let encoded = readiness.to_string();
        assert!(!encoded.contains("private-account-id"));
        assert!(!encoded.contains("private-reason-ref"));

        let release_payload = serde_json::json!({
            "eventId": "event-api-2",
            "capability": "generation",
            "scopeKind": "account",
            "scopeRef": scope_ref,
            "transition": "released",
            "reasonCode": "manual_release",
            "reasonRef": "INC-606",
            "expectedHeadRevision": 1,
            "expectedCurrentEventRef": current_event_ref
        })
        .to_string();
        let release = app
            .clone()
            .oneshot(
                Request::post("/admin/jobs/operational-holds/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(release_payload.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(release.status(), StatusCode::CREATED);
        let release_body = axum::body::to_bytes(release.into_body(), 64 * 1024)
            .await
            .unwrap();
        let release: serde_json::Value = serde_json::from_slice(&release_body).unwrap();
        assert_eq!(release["replayed"], false);
        assert_eq!(release["state"], "released");
        assert!(!release.to_string().contains("private-account-id"));

        let replay = app
            .clone()
            .oneshot(
                Request::post("/admin/jobs/operational-holds/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(release_payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let replay_body = axum::body::to_bytes(replay.into_body(), 64 * 1024)
            .await
            .unwrap();
        let replay: serde_json::Value = serde_json::from_slice(&replay_body).unwrap();
        assert_eq!(replay["replayed"], true);

        let released_readiness = app
            .oneshot(
                Request::get("/admin/jobs/readiness")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(released_readiness.status(), StatusCode::OK);
        let released_readiness = axum::body::to_bytes(released_readiness.into_body(), 64 * 1024)
            .await
            .unwrap();
        let released_readiness: serde_json::Value =
            serde_json::from_slice(&released_readiness).unwrap();
        assert_eq!(released_readiness["ready"], true);
    }

    #[tokio::test]
    async fn full_and_standalone_routers_protect_operations_and_metrics_routes() {
        let temp_db = TempDbPath::new("jobs-router-coverage");
        let db_path = temp_db.path.clone();
        let pool = crate::db::open_pool(&db_path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let config = test_config(db_path);
        let full = crate::api::build_router(pool.clone(), config.clone());
        let standalone = crate::api::build_jobs_router(pool, config);

        for router in [full, standalone] {
            for path in [
                "/admin/jobs/operational-holds",
                "/admin/jobs/readiness",
                "/admin/metrics",
            ] {
                let response = router
                    .clone()
                    .oneshot(Request::get(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
                assert_eq!(
                    response.headers().get(header::CACHE_CONTROL),
                    Some(&HeaderValue::from_static("private, no-store, max-age=0",)),
                    "{path}"
                );
                assert_eq!(
                    response.headers().get(header::PRAGMA),
                    Some(&HeaderValue::from_static("no-cache")),
                    "{path}"
                );
            }
            let response = router
                .clone()
                .oneshot(
                    Request::post("/admin/jobs/operational-holds/events")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL),
                Some(&HeaderValue::from_static("private, no-store, max-age=0",))
            );
            assert_eq!(
                response.headers().get(header::PRAGMA),
                Some(&HeaderValue::from_static("no-cache"))
            );
        }
    }

    #[tokio::test]
    async fn full_and_standalone_routers_preserve_json_rejection_statuses() {
        let temp_db = TempDbPath::new("jobs-router-json-rejections");
        let db_path = temp_db.path.clone();
        let pool = crate::db::open_pool(&db_path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let config = test_config(db_path);
        let admin =
            Account::create_with_admin(&pool, "json-admin@example.com", "hash", true).unwrap();
        let user = Account::create(&pool, "json-user@example.com", "hash").unwrap();
        let token = crate::auth::jwt::issue(
            &config.jwt_secret,
            &admin.id,
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let user_token = crate::auth::jwt::issue(
            &config.jwt_secret,
            &user.id,
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let full = crate::api::build_router(pool.clone(), config.clone());
        let standalone = crate::api::build_jobs_router(pool.clone(), config);
        let routers = [full, standalone];

        for router in &routers {
            let forbidden = router
                .clone()
                .oneshot(
                    Request::get("/admin/jobs/operational-holds")
                        .header(header::AUTHORIZATION, format!("Bearer {user_token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
            assert_eq!(
                forbidden.headers()[header::CACHE_CONTROL],
                "private, no-store, max-age=0"
            );

            let allowed = router
                .clone()
                .oneshot(
                    Request::get("/admin/jobs/operational-holds")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(allowed.status(), StatusCode::OK);

            let unsupported_media_type = router
                .clone()
                .oneshot(
                    Request::post("/admin/jobs/operational-holds/events")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                unsupported_media_type.status(),
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            assert_eq!(
                unsupported_media_type.headers()[header::CACHE_CONTROL],
                "private, no-store, max-age=0"
            );
            assert_eq!(unsupported_media_type.headers()[header::PRAGMA], "no-cache");

            let oversized_json = format!(
                "{{\"padding\":\"{}\"}}",
                "x".repeat(OPERATIONAL_HOLD_BODY_LIMIT_BYTES)
            );
            let payload_too_large = router
                .clone()
                .oneshot(
                    Request::post("/admin/jobs/operational-holds/events")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(oversized_json))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(payload_too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(
                payload_too_large.headers()[header::CACHE_CONTROL],
                "private, no-store, max-age=0"
            );
            assert_eq!(payload_too_large.headers()[header::PRAGMA], "no-cache");

            let unknown_field = router
                .clone()
                .oneshot(
                    Request::post("/admin/jobs/operational-holds/events")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "eventId": "event-unknown-field",
                                "capability": "generation",
                                "scopeKind": "global",
                                "scopeId": "*",
                                "transition": "held",
                                "reasonCode": "incident",
                                "expectedHeadRevision": 0,
                                "expectedCurrentEventId": null,
                                "unexpected": true
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(unknown_field.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                unknown_field.headers()[header::CACHE_CONTROL],
                "private, no-store, max-age=0"
            );
        }

        crate::db::account_data::begin_account_deletion(
            &pool,
            &admin.id,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap()
        .unwrap();
        for router in &routers {
            let fenced = router
                .clone()
                .oneshot(
                    Request::post("/admin/jobs/operational-holds/events")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "eventId": "event-deletion-fenced",
                                "capability": "generation",
                                "scopeKind": "global",
                                "scopeId": "*",
                                "transition": "held",
                                "reasonCode": "incident",
                                "expectedHeadRevision": 0,
                                "expectedCurrentEventId": null
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(fenced.status(), StatusCode::CONFLICT);
            assert_eq!(
                fenced.headers()[header::CACHE_CONTROL],
                "private, no-store, max-age=0"
            );
        }

        let event_count = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_operational_hold_events",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(event_count, 0);
    }
}

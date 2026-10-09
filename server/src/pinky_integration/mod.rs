//! Additive, default-off Pinky integration. Standalone and Jobs auth are unchanged.

pub mod delegation;
mod store;

#[cfg(test)]
mod http_tests;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, OriginalUri, State},
    http::{HeaderMap, StatusCode},
    middleware::from_fn,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;

use crate::db::DbPool;
use delegation::DelegationConfig;

/// Explicit opt-in; absent/zero preserves the existing standalone router.
/// Integration credentials must be independent of standalone JWT signing.
pub fn config_from_env(standalone_secret: &str) -> anyhow::Result<Option<DelegationConfig>> {
    config_from_lookup(standalone_secret, |name| std::env::var(name))
}

fn config_from_lookup(
    standalone_secret: &str,
    lookup: impl Fn(&str) -> Result<String, std::env::VarError>,
) -> anyhow::Result<Option<DelegationConfig>> {
    match lookup("BLUEY_PINKY_INTEGRATION_ENABLED").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => return Ok(None),
        Ok("1") => {}
        _ => anyhow::bail!("invalid Pinky integration flag"),
    }
    let read = |name| {
        lookup(name).map_err(|_| anyhow::anyhow!("incomplete Pinky integration configuration"))
    };
    let secret = read("BLUEY_PINKY_DELEGATION_SECRET")?;
    if secret == standalone_secret {
        anyhow::bail!("Pinky delegation requires independent signing material");
    }
    Ok(Some(DelegationConfig::new(
        secret,
        read("BLUEY_PINKY_ISSUER")?,
        read("BLUEY_PINKY_AUDIENCE")?,
        read("BLUEY_PINKY_ENVIRONMENT")?,
    )?))
}

#[derive(Clone)]
struct IntegrationState {
    pool: DbPool,
    delegation: Arc<DelegationConfig>,
    start_limit: crate::rate_limit::Limiter,
    stop_limit: crate::rate_limit::Limiter,
}

/// Called by main and the merged-router regression tests. None neither creates
/// integration tables nor installs delegated routes.
pub fn build_application(
    pool: DbPool,
    config: crate::config::Config,
    delegation: Option<DelegationConfig>,
) -> anyhow::Result<Router> {
    let app = crate::api::build_router(pool.clone(), config);
    match delegation {
        Some(delegation) => Ok(app.merge(routes(pool, delegation)?)),
        None => Ok(app),
    }
}

pub fn routes(pool: DbPool, delegation: DelegationConfig) -> anyhow::Result<Router> {
    store::initialize(&pool)?;
    let state = IntegrationState {
        pool,
        delegation: Arc::new(delegation),
        // One fixed key per route class: memory stays bounded even for bad
        // credentials. Separate Stop budget avoids starvation by Start traffic.
        // These are single-process preprod limits, not a distributed quota.
        start_limit: crate::rate_limit::Limiter::new(120, 30),
        stop_limit: crate::rate_limit::Limiter::new(120, 30),
    };
    Ok(Router::new()
        .route("/integrations/pinky/sessions", post(open_session))
        .route("/integrations/pinky/sessions/close", post(close_session))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(from_fn(
            crate::api::middleware::request_id::request_id_middleware,
        ))
        .with_state(state))
}

async fn admission(state: &IntegrationState, stop: bool) -> Result<(), ApiError> {
    let limit = if stop {
        &state.stop_limit
    } else {
        &state.start_limit
    };
    if let Err(seconds) = limit.check("integration-edge").await {
        let mut response = error(StatusCode::TOO_MANY_REQUESTS, "integration_rate_limited");
        response.retry_after = Some(seconds);
        return Err(response);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRequest {
    session_id: String,
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    retry_after: Option<u64>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response =
            (self.status, Json(serde_json::json!({"error":self.code}))).into_response();
        if let Some(seconds) = self.retry_after {
            if let Ok(value) = seconds.to_string().parse() {
                response
                    .headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, value);
            }
        }
        response
    }
}

fn error(status: StatusCode, code: &'static str) -> ApiError {
    ApiError {
        status,
        code,
        retry_after: None,
    }
}

fn owned_request(
    state: &IntegrationState,
    headers: &HeaderMap,
    uri: &axum::http::Uri,
    body: &[u8],
    now: i64,
) -> Result<(String, SessionRequest), ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "delegation_required"))?;
    let claims = delegation::verify(
        token,
        &state.delegation,
        "POST",
        &uri.to_string(),
        body,
        now,
    )
    .map_err(|_| error(StatusCode::UNAUTHORIZED, "invalid_delegation"))?;
    let request: SessionRequest = serde_json::from_slice(body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_session_request"))?;
    let id = uuid::Uuid::parse_str(&request.session_id)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_session_request"))?;
    if id.hyphenated().to_string() != request.session_id || id.is_nil() {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_session_request"));
    }
    Ok((
        store::subject_key(&claims.iss, &claims.aud, &claims.env, &claims.sub),
        request,
    ))
}

async fn open_session(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<store::AiSession>, ApiError> {
    let now = Utc::now().timestamp();
    let (key, request) = owned_request(&state, &headers, &uri, &body, now)?;
    admission(&state, false).await?;
    let result = tokio::task::spawn_blocking(move || {
        store::open(&state.pool, &key, &request.session_id, now)
    })
    .await
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"))?
    .map_err(store_error)?;
    Ok(Json(result))
}

fn store_error(err: anyhow::Error) -> ApiError {
    match err.downcast_ref::<store::StoreError>() {
        Some(store::StoreError::Denied) => error(StatusCode::FORBIDDEN, "session_unavailable"),
        Some(store::StoreError::Capacity) => {
            error(StatusCode::TOO_MANY_REQUESTS, "session_capacity")
        }
        None => error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"),
    }
}

async fn close_session(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<store::AiSession>, ApiError> {
    let now = Utc::now().timestamp();
    let (key, request) = owned_request(&state, &headers, &uri, &body, now)?;
    admission(&state, true).await?;
    let result = tokio::task::spawn_blocking(move || {
        store::close(&state.pool, &key, &request.session_id, now)
    })
    .await
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"))?
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"))?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "session_unavailable"))?;
    Ok(Json(result))
}

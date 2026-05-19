//! HTTP API. Builds the axum router with all routes wired in.

use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::{auth, config::Config, db::DbPool};

pub mod account;
pub mod admin;
pub mod auth_routes;
pub mod billing;
pub mod router;
pub mod usage;

/// Shared app state passed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub config: Arc<Config>,
}

pub fn build_router(pool: DbPool, config: Config) -> Router {
    let state = AppState {
        pool,
        config: Arc::new(config),
    };

    // ---- Public (no auth) ---------------------------------------------------
    let public = Router::new()
        .route("/admin/health", get(admin::health))
        .route("/auth/signup", axum::routing::post(auth_routes::signup))
        .route("/auth/login", axum::routing::post(auth_routes::login))
        .route("/auth/refresh", axum::routing::post(auth_routes::refresh))
        .route(
            "/auth/device/start",
            axum::routing::post(auth_routes::device_start),
        )
        .route(
            "/auth/device/poll",
            axum::routing::post(auth_routes::device_poll),
        )
        .route("/billing/webhook", axum::routing::post(billing::webhook));

    // ---- Authenticated (Bearer JWT) -----------------------------------------
    let protected = Router::new()
        .route("/account/me", get(account::me))
        .route("/account/usage", get(account::usage))
        .route("/router/complete", axum::routing::post(router::complete))
        .route("/router/embed", axum::routing::post(router::embed))
        .route(
            "/router/transcribe",
            axum::routing::post(router::transcribe),
        )
        .route("/usage/event", axum::routing::post(usage::ingest))
        .route("/billing/checkout", axum::routing::post(billing::checkout))
        .route(
            "/auth/device/approve",
            axum::routing::post(auth_routes::device_approve),
        )
        .route("/admin/customers", get(admin::customers))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

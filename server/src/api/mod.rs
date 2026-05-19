//! HTTP API. Builds the axum router with all routes wired in.

use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::{config::Config, db::DbPool};

pub mod account;
pub mod admin;
pub mod auth;
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

    Router::new()
        // Public health
        .route("/admin/health", get(admin::health))
        // Auth (public)
        .route("/auth/signup", axum::routing::post(auth::signup))
        .route("/auth/login", axum::routing::post(auth::login))
        .route("/auth/refresh", axum::routing::post(auth::refresh))
        .route("/auth/device/start", axum::routing::post(auth::device_start))
        .route("/auth/device/poll", axum::routing::post(auth::device_poll))
        .route("/auth/device/approve", axum::routing::post(auth::device_approve))
        // Account (auth required)
        .route("/account/me", get(account::me))
        .route("/account/usage", get(account::usage))
        // Router (auth required, the monetization handle)
        .route("/router/complete", axum::routing::post(router::complete))
        .route("/router/embed", axum::routing::post(router::embed))
        .route("/router/transcribe", axum::routing::post(router::transcribe))
        // Usage ingestion (auth required, daemon emits events)
        .route("/usage/event", axum::routing::post(usage::ingest))
        // Billing (mixed auth)
        .route("/billing/checkout", axum::routing::post(billing::checkout))
        .route("/billing/webhook", axum::routing::post(billing::webhook))
        // Admin
        .route("/admin/customers", get(admin::customers))
        // Cross-cutting
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

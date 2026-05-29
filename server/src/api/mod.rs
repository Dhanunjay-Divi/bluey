//! HTTP API. Builds the axum router with all routes wired in.

use axum::{middleware::from_fn, routing::get, Router};
use std::sync::Arc;

use crate::{auth, config::Config, db::DbPool};

pub mod account;
pub mod admin;
pub mod auth_routes;
pub mod billing;
pub mod metrics;
pub mod middleware;
pub mod pricing;
pub mod router;
pub mod stt;
pub mod sync;
pub mod usage;

/// Shared app state passed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub config: Arc<Config>,
    /// Codex Stage 11: per-IP rate limiters for sensitive endpoints.
    pub rate_limiters: crate::rate_limit::RateLimiters,
    /// Provider/model/key cooldown ledger used to route around upstream 429s.
    pub provider_health: crate::provider_health::ProviderHealth,
}

pub fn build_router(pool: DbPool, config: Config) -> Router {
    let state = AppState {
        pool,
        config: Arc::new(config),
        rate_limiters: crate::rate_limit::RateLimiters::default(),
        provider_health: crate::provider_health::ProviderHealth::default(),
    };

    // ---- Public (no auth) ---------------------------------------------------
    let public = Router::new()
        // Public unauthenticated health endpoints. /health is the standard
        // monitoring/uptime path; /admin/health is the legacy alias kept
        // for backward compat. Both return identical JSON.
        .route("/health", get(admin::health))
        .route("/admin/health", get(admin::health))
        .route(
            "/auth/signup",
            axum::routing::post(auth_routes::signup).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_signup,
                ),
            ),
        )
        .route(
            "/auth/login",
            axum::routing::post(auth_routes::login).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_login,
                ),
            ),
        )
        .route(
            "/auth/refresh",
            axum::routing::post(auth_routes::refresh).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_refresh,
                ),
            ),
        )
        .route(
            "/auth/device/start",
            axum::routing::post(auth_routes::device_start),
        )
        .route(
            "/auth/device/poll",
            axum::routing::post(auth_routes::device_poll).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_device_poll,
                ),
            ),
        )
        .route("/billing/webhook", axum::routing::post(billing::webhook))
        .route(
            "/billing/square/webhook",
            axum::routing::post(billing::square_webhook),
        )
        .route("/pricing/tiers", get(pricing::get_tiers))
        .route(
            "/auth/verify-email/confirm",
            axum::routing::post(auth_routes::verify_email_confirm),
        )
        .route(
            "/auth/password-reset/start",
            axum::routing::post(auth_routes::password_reset_start),
        )
        .route(
            "/auth/password-reset/confirm",
            axum::routing::post(auth_routes::password_reset_confirm),
        )
        .route(
            "/auth/link/exchange",
            axum::routing::post(auth_routes::link_exchange),
        );

    // ---- Admin-only (require_auth + require_admin) -------------------------
    let admin_only = Router::new()
        .route("/admin/customers", get(admin::customers))
        .route("/admin/echo-peer", get(admin::echo_peer_key))
        .route("/admin/metrics", get(metrics::get_metrics))
        .route_layer(axum::middleware::from_fn(auth::require_admin));

    // ---- Authenticated (Bearer JWT) -----------------------------------------
    let protected = Router::new()
        .route("/account/me", get(account::me))
        .route("/account/usage", get(account::usage))
        .route(
            "/router/complete",
            axum::routing::post(router::complete).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_complete,
                ),
            ),
        )
        .route(
            "/router/complete/stream",
            axum::routing::post(router::complete_stream).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_complete,
                ),
            ),
        )
        .route(
            "/router/embed",
            axum::routing::post(router::embed).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::rate_limit::limit_router_embed,
            )),
        )
        .route(
            "/router/transcribe",
            axum::routing::post(router::transcribe).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_transcribe,
                ),
            ),
        )
        .route("/stt/session", axum::routing::post(stt::create_session))
        .route("/stt/relay", axum::routing::get(stt::relay))
        .route("/sync/batch", axum::routing::post(sync::batch))
        .route("/sync/sessions", get(sync::list_sessions))
        .route("/sync/sessions/:session_id", get(sync::get_session))
        .route("/rag/query", axum::routing::post(sync::rag_query))
        .route("/usage/event", axum::routing::post(usage::ingest))
        .route("/billing/checkout", axum::routing::post(billing::checkout))
        .route("/billing/portal", axum::routing::post(billing::portal))
        .route("/account/export", get(account::export_data))
        .route(
            "/account/delete",
            axum::routing::post(account::delete_account),
        )
        .route(
            "/auth/device/approve",
            axum::routing::post(auth_routes::device_approve),
        )
        .route(
            "/auth/verify-email/start",
            axum::routing::post(auth_routes::verify_email_start),
        )
        .route(
            "/auth/link/mint",
            axum::routing::post(auth_routes::link_mint),
        )
        .merge(admin_only)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(from_fn(middleware::request_id::request_id_middleware))
        .with_state(state)
}

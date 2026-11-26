//! HTTP API. Builds the axum router with all routes wired in.

use axum::{
    extract::DefaultBodyLimit,
    middleware::from_fn,
    routing::{delete, get},
    Router,
};
use std::sync::Arc;

use crate::{auth, config::Config, db::DbPool};

const ROUTER_COMPLETE_BODY_LIMIT_BYTES: usize = 20 * 1024 * 1024;
const ROUTER_TRANSCRIBE_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

pub mod account;
pub mod admin;
pub mod auth_routes;
pub mod billing;
pub mod jobs;
pub mod jobs_ats_certifications;
pub mod jobs_beta_access;
pub mod jobs_browser_releases;
mod jobs_communication_actions;
mod jobs_import;
pub mod jobs_interview_prep;
pub mod jobs_job_integrity;
pub mod jobs_local_capability;
mod jobs_mailbox;
pub mod jobs_mailbox_oauth;
pub mod jobs_managed_cloud_releases;
pub mod jobs_operations;
pub mod jobs_original_source_verifications;
pub(crate) mod jobs_resume_assets;
pub(crate) mod jobs_resume_generation;
pub mod jobs_runner_volumes;
mod jobs_source_directory;
mod jobs_source_directory_catalog;
pub mod jobs_worker_auth;
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
    build_router_with_jobs_context(
        pool,
        config,
        Arc::new(crate::db::jobs::managed_cloud_provisioning_runtime::ProvisioningProcessContext::production()),
    )
}

/// Startup-owned context; HTTP requests cannot select its purpose.
pub fn build_router_from_environment(pool: DbPool, config: Config) -> anyhow::Result<Router> {
    let context = crate::db::jobs::managed_cloud_provisioning_runtime::ProvisioningProcessContext::from_environment()
        .map_err(|_| anyhow::anyhow!("invalid Jobs provisioning process context"))?;
    Ok(build_router_with_jobs_context(
        pool,
        config,
        Arc::new(context),
    ))
}

fn build_router_with_jobs_context(
    pool: DbPool,
    config: Config,
    provisioning_context: Arc<
        crate::db::jobs::managed_cloud_provisioning_runtime::ProvisioningProcessContext,
    >,
) -> Router {
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
            "/auth/signup/start",
            axum::routing::post(auth_routes::signup_start).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_signup,
                ),
            ),
        )
        .route(
            "/auth/signup/confirm",
            axum::routing::post(auth_routes::signup_confirm).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_signup,
                ),
            ),
        )
        .route(
            "/auth/trial/start",
            axum::routing::post(auth_routes::trial_start).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_signup,
                ),
            ),
        )
        .route("/auth/captcha/config", get(auth_routes::captcha_config))
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
            axum::routing::post(auth_routes::device_start).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
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
            axum::routing::post(auth_routes::verify_email_confirm).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .route(
            "/auth/password-reset/start",
            axum::routing::post(auth_routes::password_reset_start).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .route(
            "/auth/password-reset/confirm",
            axum::routing::post(auth_routes::password_reset_confirm).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .route(
            "/auth/link/exchange",
            axum::routing::post(auth_routes::link_exchange).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .merge(
            jobs::worker_router()
                .merge(jobs_ats_certifications::worker_router())
                .merge(jobs_original_source_verifications::worker_router())
                .merge(jobs_managed_cloud_releases::worker_router())
                .merge(jobs_managed_cloud_releases::provisioning_worker_router(
                    provisioning_context.clone(),
                ))
                .merge(jobs_runner_volumes::worker_router())
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    jobs_worker_auth::require_jobs_worker,
                )),
        )
        .merge(
            jobs::local_runner_router().route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::rate_limit::limit_jobs_local_runner,
            )),
        )
        .merge(jobs_mailbox_oauth::public_router().route_layer(
            axum::middleware::from_fn_with_state(state.clone(), crate::rate_limit::limit_auth_otp),
        ));

    // ---- Admin-only (require_auth + require_admin) -------------------------
    let admin_only = Router::new()
        .route("/admin/customers", get(admin::customers))
        .route("/admin/echo-peer", get(admin::echo_peer_key))
        .route("/admin/metrics", get(metrics::get_metrics))
        .route("/admin/trial-abuse", get(admin::trial_abuse))
        .route("/admin/billing-risk", get(admin::billing_risk))
        .route("/admin/provider-routing", get(admin::provider_routing))
        .route("/admin/storage/health", get(admin::storage_health))
        .route("/admin/ops/events", get(admin::ops_events))
        .route(
            "/admin/support/accounts/:account_id",
            get(admin::support_account),
        )
        .merge(jobs::admin_router())
        .merge(jobs_ats_certifications::admin_router())
        .merge(jobs_job_integrity::admin_router())
        .merge(jobs_browser_releases::admin_router())
        .merge(jobs_managed_cloud_releases::admin_router())
        .merge(jobs_managed_cloud_releases::provisioning_admin_router(
            provisioning_context,
        ))
        .merge(jobs_operations::admin_router())
        .merge(jobs_runner_volumes::admin_router())
        .route_layer(axum::middleware::from_fn(auth::require_admin));

    // ---- Authenticated (Bearer JWT) -----------------------------------------
    let protected = Router::new()
        .route("/account/me", get(account::me))
        .route(
            "/account/devices",
            get(account::devices).delete(account::revoke_all_devices),
        )
        .route(
            "/account/devices/register",
            axum::routing::post(account::register_device),
        )
        .route(
            "/account/devices/status",
            axum::routing::post(account::device_status),
        )
        .route(
            "/account/devices/:device_id",
            delete(account::revoke_device),
        )
        .route(
            "/account/billing",
            axum::routing::patch(account::update_billing_settings),
        )
        .route("/account/usage", get(account::usage))
        .merge(
            jobs::router(state.clone()).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::rate_limit::limit_jobs_api,
            )),
        )
        .route(
            "/router/complete",
            axum::routing::post(router::complete)
                .route_layer(DefaultBodyLimit::max(ROUTER_COMPLETE_BODY_LIMIT_BYTES))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_complete,
                )),
        )
        .route(
            "/router/complete/stream",
            axum::routing::post(router::complete_stream)
                .route_layer(DefaultBodyLimit::max(ROUTER_COMPLETE_BODY_LIMIT_BYTES))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_complete,
                )),
        )
        .route(
            "/router/embed",
            axum::routing::post(router::embed).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::rate_limit::limit_router_embed,
            )),
        )
        .route(
            "/router/embed/batch",
            axum::routing::post(router::embed_batch).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_embed,
                ),
            ),
        )
        .route(
            "/router/transcribe",
            axum::routing::post(router::transcribe)
                .route_layer(DefaultBodyLimit::max(ROUTER_TRANSCRIBE_BODY_LIMIT_BYTES))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_router_transcribe,
                )),
        )
        .route("/stt/session", axum::routing::post(stt::create_session))
        .route(
            "/stt/session/cancel",
            axum::routing::post(stt::cancel_session),
        )
        .route("/stt/relay", axum::routing::get(stt::relay))
        .route("/sync/batch", axum::routing::post(sync::batch))
        .route(
            "/sync/artifacts/:artifact_id/object",
            axum::routing::post(sync::upload_artifact_object)
                .get(sync::download_artifact_object)
                .route_layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/sync/session-audit/:session_id/:bundle_id",
            axum::routing::post(sync::upload_session_audit_bundle)
                .route_layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/sync/sessions", get(sync::list_sessions))
        .route(
            "/sync/sessions/:session_id",
            get(sync::get_session).delete(sync::delete_session),
        )
        .route("/rag/query", axum::routing::post(sync::rag_query))
        .route("/usage/event", axum::routing::post(usage::ingest))
        .route("/billing/checkout", axum::routing::post(billing::checkout))
        .route("/billing/portal", axum::routing::post(billing::portal))
        .route(
            "/billing/square/pay",
            axum::routing::post(billing::square_pay),
        )
        .route(
            "/auth/password/change",
            axum::routing::post(auth_routes::password_change),
        )
        .route("/auth/logout", axum::routing::post(auth_routes::logout))
        .route(
            "/billing/square/card",
            axum::routing::post(billing::save_square_card),
        )
        .route("/account/export", get(account::export_data))
        .route(
            "/account/delete",
            axum::routing::post(account::delete_account),
        )
        .route(
            "/auth/device/approve",
            axum::routing::post(auth_routes::device_approve).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .route(
            "/auth/verify-email/start",
            axum::routing::post(auth_routes::verify_email_start).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .route(
            "/auth/link/mint",
            axum::routing::post(auth_routes::link_mint),
        )
        .route(
            "/auth/trial/convert/start",
            axum::routing::post(auth_routes::trial_convert_start).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .route(
            "/auth/trial/convert/confirm",
            axum::routing::post(auth_routes::trial_convert_confirm).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    crate::rate_limit::limit_auth_otp,
                ),
            ),
        )
        .merge(admin_only)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            account::reject_mutation_after_deletion_fence,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(from_fn(jobs_operations::private_admin_no_store))
        .layer(from_fn(middleware::request_id::request_id_middleware))
        .with_state(state)
}

/// Independently deployable Bluey Jobs API surface.
///
/// It validates the same Bluey bearer tokens and uses the same account and
/// balance database, but does not expose meeting, audio, router, or sync APIs.
pub fn build_jobs_router(pool: DbPool, config: Config) -> Router {
    build_jobs_router_with_context(
        pool,
        config,
        Arc::new(crate::db::jobs::managed_cloud_provisioning_runtime::ProvisioningProcessContext::production()),
    )
}

/// Standalone Jobs startup validates the same immutable context as the full API.
pub fn build_jobs_router_from_environment(pool: DbPool, config: Config) -> anyhow::Result<Router> {
    let context = crate::db::jobs::managed_cloud_provisioning_runtime::ProvisioningProcessContext::from_environment()
        .map_err(|_| anyhow::anyhow!("invalid Jobs provisioning process context"))?;
    Ok(build_jobs_router_with_context(
        pool,
        config,
        Arc::new(context),
    ))
}

fn build_jobs_router_with_context(
    pool: DbPool,
    config: Config,
    provisioning_context: Arc<
        crate::db::jobs::managed_cloud_provisioning_runtime::ProvisioningProcessContext,
    >,
) -> Router {
    let state = AppState {
        pool,
        config: Arc::new(config),
        rate_limiters: crate::rate_limit::RateLimiters::default(),
        provider_health: crate::provider_health::ProviderHealth::default(),
    };

    let protected = Router::new()
        .merge(
            jobs::router(state.clone()).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::rate_limit::limit_jobs_api,
            )),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            account::reject_mutation_after_deletion_fence,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let jobs_admin = Router::new()
        .route("/admin/metrics", get(metrics::get_metrics))
        .merge(jobs::admin_router())
        .merge(jobs_ats_certifications::admin_router())
        .merge(jobs_job_integrity::admin_router())
        .merge(jobs_browser_releases::admin_router())
        .merge(jobs_managed_cloud_releases::admin_router())
        .merge(jobs_managed_cloud_releases::provisioning_admin_router(
            provisioning_context.clone(),
        ))
        .merge(jobs_operations::admin_router())
        .merge(jobs_runner_volumes::admin_router())
        .route_layer(axum::middleware::from_fn(auth::require_admin))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            account::reject_mutation_after_deletion_fence,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/health", get(admin::health))
        .merge(jobs_mailbox_oauth::public_router().route_layer(
            axum::middleware::from_fn_with_state(state.clone(), crate::rate_limit::limit_auth_otp),
        ))
        .merge(
            jobs::worker_router()
                .merge(jobs_ats_certifications::worker_router())
                .merge(jobs_original_source_verifications::worker_router())
                .merge(jobs_managed_cloud_releases::worker_router())
                .merge(jobs_managed_cloud_releases::provisioning_worker_router(
                    provisioning_context,
                ))
                .merge(jobs_runner_volumes::worker_router())
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    jobs_worker_auth::require_jobs_worker,
                )),
        )
        .merge(
            jobs::local_runner_router().route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::rate_limit::limit_jobs_local_runner,
            )),
        )
        .merge(jobs_admin)
        .merge(protected)
        .layer(from_fn(jobs_operations::private_admin_no_store))
        .layer(from_fn(middleware::request_id::request_id_middleware))
        .with_state(state)
}

#[cfg(test)]
mod provisioning_router_tests {
    use super::*;
    use crate::config::{ServerDbBackend, TrialAbuseConfig, UpstreamKeys};
    use crate::db::jobs::managed_cloud_provisioning_snapshot as snapshot;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    const KEY: &str = "phase648-router-synthetic-signing-key-only";
    const ORIGIN: &str = "https://jobs-api.internal";
    const WORKER: &str = "phase648-unit-control-worker";

    struct Environment(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl Environment {
        fn set(values: &[(&'static str, &str)]) -> Self {
            let mut prior = Vec::new();
            for (name, value) in values {
                prior.push((*name, std::env::var_os(name)));
                std::env::set_var(name, value);
            }
            Self(prior)
        }
    }

    impl Drop for Environment {
        fn drop(&mut self) {
            for (name, prior) in &self.0 {
                match prior {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    pub(super) fn fixture() -> (DbPool, Config) {
        // Canonical launcher owns the complete temporary root and cleanup.
        let db_path = std::env::temp_dir().join(format!(
            "phase648-router-unit-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&db_path).unwrap();
        let config = Config {
            port: 0,
            db_path,
            db_backend: ServerDbBackend::Sqlite,
            database_url: None,
            jwt_secret: "phase648-router-unit-jwt-secret-at-least32".into(),
            public_url: ORIGIN.into(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            smtp: None,
            upstream: UpstreamKeys::default(),
            upstream_spend_guard: None,
            admin_emails: Vec::new(),
            trial_abuse: TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: None,
            log_storage: None,
        };
        (pool, config)
    }

    fn signed_request(path: &str, body: Vec<u8>) -> Request<Body> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let digest = hex::encode(Sha256::digest(&body));
        let canonical = [
            "bluey-jobs-worker-v2",
            &timestamp,
            &nonce,
            WORKER,
            "bluey-jobs-api",
            "managed-cloud-runtime",
            ORIGIN,
            "POST",
            path,
            &digest,
        ]
        .join("\n");
        let mut mac = Hmac::<Sha256>::new_from_slice(KEY.as_bytes()).unwrap();
        mac.update(canonical.as_bytes());
        Request::post(path)
            .header("x-bluey-jobs-worker-id", WORKER)
            .header("x-bluey-jobs-worker-timestamp", timestamp)
            .header("x-bluey-jobs-worker-nonce", nonce)
            .header("x-bluey-jobs-worker-audience", "bluey-jobs-api")
            .header("x-bluey-jobs-worker-scope", "managed-cloud-runtime")
            .header("x-bluey-jobs-worker-origin", ORIGIN)
            .header("x-bluey-jobs-worker-content-sha256", digest)
            .header(
                "x-bluey-jobs-worker-signature",
                hex::encode(mac.finalize().into_bytes()),
            )
            .body(Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn jobs_interview_prep_cache_namespace_rejects_public_router_requests() {
        let (pool, config) = fixture();
        crate::db::run_migrations(&pool).unwrap();
        let account = crate::db::accounts::Account::create(
            &pool,
            "private-prep-test@example.invalid",
            "unused-test-hash",
        )
        .unwrap();
        let token = crate::auth::jwt::issue(
            &config.jwt_secret,
            &account.id,
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let app = build_router(pool, config);
        let key = format!(
            "{}{}",
            router::JOBS_INTERVIEW_PREP_REQUEST_PREFIX,
            "ab".repeat(32)
        );
        let completion = serde_json::json!({
            "request_id": key,
            "system": "Synthetic test",
            "user": "Synthetic draft",
            "lane": "balanced",
        });
        let transcribe_path = format!("/router/transcribe?request_id={key}");
        for (path, body) in [
            ("/router/complete", completion.clone()),
            ("/router/complete/stream", completion),
            (
                "/router/embed",
                serde_json::json!({"request_id": key, "input": "synthetic"}),
            ),
            (
                "/router/embed/batch",
                serde_json::json!({"request_id": key, "inputs": ["synthetic"]}),
            ),
            (transcribe_path.as_str(), serde_json::Value::Null),
        ] {
            let request = Request::post(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
            let bytes = axum::body::to_bytes(response.into_body(), 4_096)
                .await
                .unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["reason"], "reserved_request_id", "{path}");
        }
    }

    #[test]
    #[serial_test::serial]
    fn c3_both_startup_builders_reject_unverified_process_context() {
        let (pool, config) = fixture();
        for value in ["", "true", "nonproduction_synthetic", "production "] {
            let _environment =
                Environment::set(&[("BLUEY_JOBS_PROVISIONING_PROCESS_CONTEXT", value)]);
            assert!(build_router_from_environment(pool.clone(), config.clone()).is_err());
            assert!(build_jobs_router_from_environment(pool.clone(), config.clone()).is_err());
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn c3_full_and_standalone_routers_share_closed_authenticated_v2_controls() {
        let _environment = Environment::set(&[
            ("BLUEY_JOBS_PROVISIONING_PROCESS_CONTEXT", "production"),
            ("BLUEY_JOBS_WORKER_SIGNING_KEY", KEY),
            ("BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN", ORIGIN),
        ]);
        let (pool, config) = fixture();
        // No signed malformed request should reach storage; deliberately leave
        // this unit DB unmigrated so accidental business/storage access fails.
        for app in [
            build_router_from_environment(pool.clone(), config.clone()).unwrap(),
            build_jobs_router_from_environment(pool.clone(), config.clone()).unwrap(),
        ] {
            for path in [
                "/api/jobs/internal/managed-cloud/v2/runtime-grants/abcdefghijklmnopqrst/claim",
                "/api/jobs/internal/managed-cloud/v2/runtime-instances/abcdefghijklmnopqrst/heartbeats",
                "/api/jobs/internal/managed-cloud/v2/runtime-instances/abcdefghijklmnopqrst/source-verifier-boot-registration",
                "/api/jobs/internal/managed-cloud/v2/runtime-instances/abcdefghijklmnopqrst/source-verifier-boot-startup",
            ] {
                let unsigned = Request::post(path).body(Body::from("{}")).unwrap();
                assert_eq!(app.clone().oneshot(unsigned).await.unwrap().status(),
                    StatusCode::UNAUTHORIZED);
                let response = app.clone().oneshot(signed_request(path, b"{}".to_vec()))
                    .await.unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains("sessionToken"));
                let mut query = signed_request(path, b"{}".to_vec());
                *query.uri_mut() = format!("{path}?extra=1").parse().unwrap();
                assert_eq!(app.clone().oneshot(query).await.unwrap().status(),
                    StatusCode::UNAUTHORIZED);
                let oversized = signed_request(path, vec![b' '; 64 * 1024 + 1]);
                assert_eq!(app.clone().oneshot(oversized).await.unwrap().status(),
                    StatusCode::UNAUTHORIZED);
            }
            for path in [
                "/admin/jobs/managed-cloud/v2/runtime-grants",
                "/admin/jobs/managed-cloud/v2/queue-generations",
                "/admin/jobs/managed-cloud/v2/provisioning-heads",
            ] {
                let admin = Request::post(path).body(Body::from("{}")).unwrap();
                assert_eq!(
                    app.clone().oneshot(admin).await.unwrap().status(),
                    StatusCode::UNAUTHORIZED
                );
            }
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn phase656_both_routers_require_admin_for_provisioning_installation() {
        let _environment = Environment::set(&[
            ("BLUEY_JOBS_PROVISIONING_PROCESS_CONTEXT", "production"),
            ("BLUEY_JOBS_WORKER_SIGNING_KEY", KEY),
            ("BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN", ORIGIN),
        ]);
        let (pool, config) = fixture();
        crate::db::run_migrations(&pool).unwrap();
        for (id, is_admin) in [("phase656-customer", false), ("phase656-admin", true)] {
            pool.get().unwrap().execute(
                "INSERT INTO accounts(id,email,password_hash,is_admin) VALUES(?1,?2,'unused-test-hash',?3)",
                rusqlite::params![id, format!("{id}@example.invalid"), is_admin],
            ).unwrap();
        }
        let customer = crate::auth::jwt::issue(
            &config.jwt_secret,
            "phase656-customer",
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let admin = crate::auth::jwt::issue(
            &config.jwt_secret,
            "phase656-admin",
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        for app in [
            build_router_from_environment(pool.clone(), config.clone()).unwrap(),
            build_jobs_router_from_environment(pool.clone(), config.clone()).unwrap(),
        ] {
            for path in [
                "/admin/jobs/managed-cloud/v2/queue-generations",
                "/admin/jobs/managed-cloud/v2/provisioning-heads",
            ] {
                let request = Request::post(path)
                    .header("authorization", format!("Bearer {customer}"))
                    .body(Body::from("{}"))
                    .unwrap();
                assert_eq!(
                    app.clone().oneshot(request).await.unwrap().status(),
                    StatusCode::FORBIDDEN
                );
                let request = Request::post(path)
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::from(
                        r#"{"unexpected":"phase656-private-rejected-value"}"#,
                    ))
                    .unwrap();
                let response = app.clone().oneshot(request).await.unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                let body = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                assert!(!String::from_utf8_lossy(&body).contains("phase656-private-rejected-value"));
            }
        }
        for table in [
            "jobs_managed_cloud_queue_generations",
            "jobs_managed_cloud_activations",
        ] {
            let count: i64 = pool
                .get()
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(
                count, 0,
                "rejected HTTP requests must not install authority"
            );
        }
    }

    // Insert immediately after
    // `phase656_both_routers_require_admin_for_provisioning_installation` and
    // before the closing brace of `provisioning_router_tests` in api/mod.rs.

    const SNAPSHOT_SIGNING_REQUEST_PATH: &str =
        "/admin/jobs/managed-cloud/v2/provisioning-signing-requests";

    fn signing_request_json() -> String {
        [
            r#"{"version":1,"#,
            r#""scope":{"environment":"production","region":"us-east-1","channel":"general"},"#,
            r#""queueGenerationId":"queue-gen-1","issuanceRef":"issue-ref-1","#,
            r#""manifestSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","#,
            r#""cohortSha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","#,
            r#""activationId":"activation-1","activationGeneration":1,"issuedAtMs":1000,"#,
            r#""notBeforeMs":1000,"expiresAtMs":2000,"maximumInflight":1,"#,
            r#""maximumDailyAdmissions":1,"heartbeatTtlMs":5000,"#,
            r#""intendedStorageConfigSha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","#,
            r#""signatureSetId":"signature-set-1","requestedSignedAtMs":1000}"#,
        ]
        .concat()
    }

    fn signing_request(token: Option<&str>, body: impl Into<Body>) -> Request<Body> {
        let mut request =
            Request::post(SNAPSHOT_SIGNING_REQUEST_PATH).header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        request.body(body.into()).unwrap()
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn phase657_both_routers_fence_and_redact_provisioning_signing_requests() {
        let _environment = Environment::set(&[
            ("BLUEY_JOBS_PROVISIONING_PROCESS_CONTEXT", "production"),
            ("BLUEY_JOBS_WORKER_SIGNING_KEY", KEY),
            ("BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN", ORIGIN),
        ]);
        let (pool, config) = fixture();
        crate::db::run_migrations(&pool).unwrap();
        for (id, is_admin) in [
            ("phase657-snapshot-customer", false),
            ("phase657-snapshot-admin", true),
            ("phase657-snapshot-fenced-admin", true),
        ] {
            pool.get()
                .unwrap()
                .execute(
                    "INSERT INTO accounts(id,email,password_hash,is_admin) VALUES(?1,?2,'unused-test-hash',?3)",
                    rusqlite::params![id, format!("{id}@example.invalid"), is_admin],
                )
                .unwrap();
        }
        let customer = crate::auth::jwt::issue(
            &config.jwt_secret,
            "phase657-snapshot-customer",
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let admin = crate::auth::jwt::issue(
            &config.jwt_secret,
            "phase657-snapshot-admin",
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let fenced_admin = crate::auth::jwt::issue(
            &config.jwt_secret,
            "phase657-snapshot-fenced-admin",
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();

        for app in [
            build_router_from_environment(pool.clone(), config.clone()).unwrap(),
            build_jobs_router_from_environment(pool.clone(), config.clone()).unwrap(),
        ] {
            for token in [None, Some("not-a-jwt")] {
                let response = app
                    .clone()
                    .oneshot(signing_request(token, signing_request_json()))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }

            let response = app
                .clone()
                .oneshot(signing_request(Some(&customer), signing_request_json()))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);

            let response = app
                .clone()
                .oneshot(signing_request(Some(&admin), "{"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);

            let sentinel = "phase657-private-rejected-sentinel";
            let unknown = format!(
                "{},\"unexpected\":\"{sentinel}\"}}",
                signing_request_json().strip_suffix('}').unwrap()
            );
            let response = app
                .clone()
                .oneshot(signing_request(Some(&admin), unknown))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            assert!(!String::from_utf8_lossy(&body).contains(sentinel));

            let response = app
                .clone()
                .oneshot(signing_request(Some(&admin), vec![b' '; 64 * 1024 + 1]))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        }

        crate::db::account_data::begin_account_deletion(
            &pool,
            "phase657-snapshot-fenced-admin",
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap()
        .expect("fixture account must establish a deletion fence");

        for app in [
            build_router_from_environment(pool.clone(), config.clone()).unwrap(),
            build_jobs_router_from_environment(pool.clone(), config.clone()).unwrap(),
        ] {
            let response = app
                .oneshot(signing_request(Some(&fenced_admin), signing_request_json()))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CONFLICT);
        }
    }

    fn seeded_snapshot_request_json(request: &snapshot::ProvisioningSigningRequest) -> String {
        serde_json::json!({
            "version": request.version,
            "scope": {
                "environment": &request.scope.environment,
                "region": &request.scope.region,
                "channel": &request.scope.channel,
            },
            "queueGenerationId": &request.queue_generation_id,
            "issuanceRef": &request.issuance_ref,
            "manifestSha256": &request.manifest_sha256,
            "cohortSha256": &request.cohort_sha256,
            "activationId": &request.activation_id,
            "activationGeneration": request.activation_generation,
            "issuedAtMs": request.issued_at_ms,
            "notBeforeMs": request.not_before_ms,
            "expiresAtMs": request.expires_at_ms,
            "maximumInflight": request.maximum_inflight,
            "maximumDailyAdmissions": request.maximum_daily_admissions,
            "heartbeatTtlMs": request.heartbeat_ttl_ms,
            "intendedStorageConfigSha256": &request.intended_storage_config_sha256,
            "signatureSetId": &request.signature_set_id,
            "requestedSignedAtMs": request.requested_signed_at_ms,
        })
        .to_string()
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn phase657_both_routers_return_bound_unsigned_provisioning_snapshot() {
        let _environment = Environment::set(&[
            ("BLUEY_JOBS_PROVISIONING_PROCESS_CONTEXT", "production"),
            ("BLUEY_JOBS_WORKER_SIGNING_KEY", KEY),
            ("BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN", ORIGIN),
        ]);
        let (pool, config) = fixture();
        crate::db::run_migrations(&pool).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts(id,email,password_hash,is_admin) VALUES(?1,?2,'unused-test-hash',1)",
                rusqlite::params![
                    "phase657-snapshot-success-admin",
                    "phase657-snapshot-success-admin@example.invalid"
                ],
            )
            .unwrap();
        let seed = snapshot::sqlite_snapshot_tests::seed_for_http(pool.clone());
        let admin = crate::auth::jwt::issue(
            &config.jwt_secret,
            "phase657-snapshot-success-admin",
            crate::auth::jwt::TokenKind::Access,
        )
        .unwrap();
        let request_json = seeded_snapshot_request_json(&seed.request);

        for app in [
            build_router_from_environment(pool.clone(), config.clone()).unwrap(),
            build_jobs_router_from_environment(pool.clone(), config.clone()).unwrap(),
        ] {
            let response = app
                .oneshot(signing_request(Some(&admin), request_json.clone()))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let response: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 256 * 1024)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(response["version"], 1);
            assert_eq!(
                response["queue"]["queueGenerationId"],
                seed.request.queue_generation_id
            );
            assert_eq!(response["queue"]["issuanceRef"], seed.request.issuance_ref);
            assert_eq!(
                response["unsignedSignatureSet"]["targetSha256"],
                response["documentSha256"]
            );
            assert_eq!(response["queue"]["taskQueueSha256"], seed.task_queue_sha256);
            assert_eq!(response["expectedHead"]["revision"], 0);
            assert!(response["expectedHead"]["transitionSha256"].is_null());
            assert!(response["expectedHead"]["activationSha256"].is_null());
            assert_eq!(
                response["sources"]["storageConfigSha256"]["origin"],
                "admin_intent"
            );
            assert_eq!(
                response["sources"]["storageConfigSha256"]["value"],
                seed.request.intended_storage_config_sha256
            );
            assert_eq!(
                response["sources"]["failureConverterSha256"]["origin"],
                "validated_import_projection"
            );
            for source in ["manifest", "cohort", "trust", "queue", "head"] {
                assert_eq!(response["sources"][source], "database_snapshot");
            }
            let document_sha256 = response["documentSha256"].as_str().unwrap();
            let document_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(response["documentCanonicalBase64url"].as_str().unwrap())
                .unwrap();
            snapshot::sqlite_snapshot_tests::assert_http_document(
                &document_bytes,
                &seed.request,
                &seed.task_queue_sha256,
                document_sha256,
            );
        }
    }
}

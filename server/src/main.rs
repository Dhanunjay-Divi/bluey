//! bluey-server entry point.
//!
//! Reads config from env, initialises tracing + DB, builds the router,
//! starts the axum HTTP server. Graceful shutdown on SIGINT/SIGTERM.

use anyhow::Context;
use bluey_server::{
    api,
    config::{Config, ServerDbBackend},
    db, jobs_communication_dispatch, jobs_mailbox_sync, object_storage,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing_subscriber::EnvFilter;

fn validate_runtime_config() -> anyhow::Result<()> {
    db::jobs::validate_data_encryption_config().context("validate Jobs data encryption")?;
    api::jobs_local_capability::validate_runtime_config()
        .context("validate Bluey Browser capability configuration")?;
    api::jobs_runner_volumes::validate_runtime_config()
        .context("validate managed runner-volume purge signing configuration")?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,bluey_server=debug")),
        )
        .init();

    // Config
    let config = Config::from_env().context("load config")?;
    validate_runtime_config()?;
    tracing::info!(
        port = config.port,
        db_backend = ?config.db_backend,
        db_path = %config.db_path.display(),
        database_url_configured = config.database_url.is_some(),
        "bluey-server starting",
    );

    // DB
    let pool = match config.db_backend {
        ServerDbBackend::Sqlite => db::open_pool(&config.db_path).context("open sqlite db")?,
        ServerDbBackend::Postgres => {
            let database_url = config
                .database_url
                .as_deref()
                .context("BLUEY_DATABASE_URL is required when BLUEY_SERVER_DB_BACKEND=postgres")?;
            db::open_postgres_pool(database_url).context("open postgres db")?
        }
    };
    db::run_migrations(&pool).context("run migrations")?;
    let expired_usage_released =
        db::usage_reservations::reconcile_expired_usage_reservations(&pool)
            .context("reconcile expired managed usage reservations at startup")?;
    tracing::info!(
        expired_usage_released,
        "startup managed usage reservation reconciliation completed"
    );
    let usage_reservation_janitor =
        db::usage_reservations::spawn_expired_usage_reservation_janitor(pool.clone());
    let startup_spend_cleanup = db::jobs_provider_cost_holds::prune_expired_spend_truth(&pool)
        .context("prune expired upstream spend truth at startup")?;
    tracing::info!(
        provider_holds_deleted = startup_spend_cleanup.provider_holds_deleted,
        cutover_baseline_rows_deleted = startup_spend_cleanup.cutover_baseline_rows_deleted,
        "startup upstream spend truth cleanup completed"
    );
    let spend_truth_janitor = db::jobs_provider_cost_holds::spawn_spend_truth_janitor(pool.clone());
    let mailbox_sync_worker = jobs_mailbox_sync::spawn_mailbox_sync_worker(pool.clone());
    let communication_workers =
        jobs_communication_dispatch::spawn_communication_workers(pool.clone());

    let cleanup_worker = object_storage::spawn_cleanup_worker(
        pool.clone(),
        config.object_storage.clone(),
        config
            .log_storage
            .clone()
            .or_else(|| config.object_storage.clone()),
    );

    // Router
    let app = api::build_router(pool.clone(), config.clone());

    // Bind
    let host = std::env::var("BLUEY_API_HOST")
        .ok()
        .map(|value| value.parse::<IpAddr>())
        .transpose()
        .context("BLUEY_API_HOST must be an IP address")?
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::new(host, config.port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    tracing::info!(%addr, "listening");

    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("axum serve");
    if let Some(worker) = cleanup_worker {
        worker.abort();
    }
    if let Some(worker) = mailbox_sync_worker {
        worker.abort();
    }
    communication_workers.abort();
    spend_truth_janitor.abort();
    usage_reservation_janitor.abort();
    serve_result?;

    tracing::info!("bluey-server exited cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c    => tracing::info!("ctrl-c received"),
        _ = terminate => tracing::info!("SIGTERM received"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn full_server_startup_rejects_an_invalid_jobs_data_key() {
        let previous = std::env::var_os("BLUEY_JOBS_DATA_KEY");
        std::env::set_var("BLUEY_JOBS_DATA_KEY", "malformed-key");
        let error = validate_runtime_config().unwrap_err().to_string();
        match previous {
            Some(value) => std::env::set_var("BLUEY_JOBS_DATA_KEY", value),
            None => std::env::remove_var("BLUEY_JOBS_DATA_KEY"),
        }
        assert!(error.contains("validate Jobs data encryption"));
    }
}

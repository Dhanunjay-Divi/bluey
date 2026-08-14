//! Independently deployable Bluey Jobs API.

use anyhow::Context;
use bluey_server::{
    api,
    config::{Config, ServerDbBackend},
    db, jobs_communication_dispatch, jobs_global_archive, jobs_mailbox_sync, jobs_workflow_cleanup,
    jobs_workflow_dispatch,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,bluey_server=info")),
        )
        .init();

    let config = Config::from_env().context("load Bluey configuration")?;
    let archive_storage_config = config.object_storage.clone();
    db::jobs::validate_data_encryption_config().context("validate Jobs data encryption")?;
    api::jobs_local_capability::validate_runtime_config()
        .context("validate Bluey Browser capability configuration")?;
    api::jobs_runner_volumes::validate_runtime_config()
        .context("validate managed runner-volume purge signing configuration")?;
    let port = std::env::var("BLUEY_JOBS_API_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8081);
    let host = std::env::var("BLUEY_JOBS_API_HOST")
        .ok()
        .map(|value| value.parse::<IpAddr>())
        .transpose()
        .context("BLUEY_JOBS_API_HOST must be an IP address")?
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let pool = match config.db_backend {
        ServerDbBackend::Sqlite => db::open_pool(&config.db_path).context("open sqlite db")?,
        ServerDbBackend::Postgres => {
            let database_url = config
                .database_url
                .as_deref()
                .context("BLUEY_DATABASE_URL is required for Postgres")?;
            db::open_postgres_pool(database_url).context("open postgres db")?
        }
    };
    db::run_migrations(&pool).context("run Jobs migrations")?;
    let expired_usage_released =
        db::usage_reservations::reconcile_expired_usage_reservations(&pool)
            .context("reconcile expired managed usage reservations at startup")?;
    tracing::info!(
        expired_usage_released,
        "startup managed usage reservation reconciliation completed"
    );
    let startup_spend_cleanup = db::jobs_provider_cost_holds::prune_expired_spend_truth(&pool)
        .context("prune expired upstream spend truth at startup")?;
    tracing::info!(
        provider_holds_deleted = startup_spend_cleanup.provider_holds_deleted,
        cutover_baseline_rows_deleted = startup_spend_cleanup.cutover_baseline_rows_deleted,
        "startup upstream spend truth cleanup completed"
    );
    let workflow_cleanup_dispatcher =
        jobs_workflow_cleanup::spawn_jobs_workflow_cleanup_dispatcher(pool.clone())
            .context("start Jobs workflow cleanup dispatcher")?;
    let usage_reservation_janitor =
        db::usage_reservations::spawn_expired_usage_reservation_janitor(pool.clone());
    let spend_truth_janitor = db::jobs_provider_cost_holds::spawn_spend_truth_janitor(pool.clone());
    let mailbox_sync_worker = jobs_mailbox_sync::spawn_mailbox_sync_worker(pool.clone());
    let communication_workers =
        jobs_communication_dispatch::spawn_communication_workers(pool.clone());
    let workflow_command_dispatcher =
        jobs_workflow_dispatch::spawn_jobs_workflow_command_dispatcher(pool.clone())
            .context("start Jobs workflow command dispatcher")?;
    let global_archive_worker = jobs_global_archive::spawn_global_candidate_archive_worker(
        pool.clone(),
        archive_storage_config,
    )
    .context("start global job candidate archive worker")?;

    let app = api::build_jobs_router(pool, config);
    let addr = SocketAddr::new(host, port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "bluey-jobs-api listening");

    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serve Bluey Jobs API");
    if let Some(worker) = mailbox_sync_worker {
        worker.abort();
    }
    communication_workers.abort();
    if let Some(worker) = workflow_command_dispatcher {
        worker.abort();
    }
    if let Some(worker) = workflow_cleanup_dispatcher {
        worker.abort();
    }
    if let Some(worker) = global_archive_worker {
        worker.abort();
    }
    spend_truth_janitor.abort();
    usage_reservation_janitor.abort();
    serve_result?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install terminate signal")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received"),
        _ = terminate => tracing::info!("terminate received"),
    }
}

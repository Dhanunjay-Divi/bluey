//! Independently deployable Bluey Jobs API.

use anyhow::Context;
use bluey_server::{
    api,
    config::{Config, ServerDbBackend},
    db,
};
use std::net::SocketAddr;
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
    db::jobs::validate_data_encryption_config().context("validate Jobs data encryption")?;
    let port = std::env::var("BLUEY_JOBS_API_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8081);
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

    let app = api::build_jobs_router(pool, config);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "bluey-jobs-api listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serve Bluey Jobs API")?;
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

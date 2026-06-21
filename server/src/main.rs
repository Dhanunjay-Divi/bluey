//! bluey-server entry point.
//!
//! Reads config from env, initialises tracing + DB, builds the router,
//! starts the axum HTTP server. Graceful shutdown on SIGINT/SIGTERM.

use anyhow::Context;
use bluey_server::{api, config::{Config, ServerDbBackend}, db};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

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
    tracing::info!(
        port = config.port,
        db_backend = ?config.db_backend,
        db_path = %config.db_path.display(),
        "bluey-server starting",
    );

    // DB
    if config.db_backend == ServerDbBackend::Postgres {
        anyhow::bail!(
            "BLUEY_SERVER_DB_BACKEND=postgres is provisioned but this bluey-server build still uses the SQLite runtime adapter; deploy the Postgres adapter build before cutover"
        );
    }
    let pool = db::open_pool(&config.db_path).context("open db")?;
    db::run_migrations(&pool).context("run migrations")?;

    // Router
    let app = api::build_router(pool.clone(), config.clone());

    // Bind
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    tracing::info!(%addr, "listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("axum serve")?;

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

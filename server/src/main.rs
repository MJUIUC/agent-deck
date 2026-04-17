#![recursion_limit = "256"]

mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;

use anyhow::Result;
use std::net::SocketAddr;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting agent-deck server");

    // Load config
    let mut config = config::Config::from_env()?;

    // ── Create data directory tree ────────────────────────────────────────────
    // Ensure all required directories exist before doing anything else.
    tokio::fs::create_dir_all(&config.process_dir).await?;
    tokio::fs::create_dir_all(config.process_dir.join(".database")).await?;
    tokio::fs::create_dir_all(&config.mcp_dir).await?;
    tokio::fs::create_dir_all(&config.personas_dir).await?;
    tokio::fs::create_dir_all(&config.workspaces_dir).await?;
    tokio::fs::create_dir_all(&config.skills_dir).await?;

    info!("data_dir: {}", config.data_dir.display());

    // ── Legacy migration hints ────────────────────────────────────────────────
    // Check for databases from earlier layout variants. Checks are ordered from
    // oldest to newest so that the most recent pre-migration path takes effect
    // when multiple legacy files happen to exist.
    {
        let new_db_path = config.process_dir.join(".database").join("agent-deck.db");

        // 1. Dev-era path: ./data/agent-deck.db
        let dev_legacy_path = std::path::Path::new("./data/agent-deck.db");
        if dev_legacy_path.exists() && !new_db_path.exists() {
            warn!(
                "mcp: legacy database found at {} but new path {} does not exist yet. \
                 Using legacy path for this run. Copy or move the file to migrate: \
                 cp {} {}",
                dev_legacy_path.display(),
                new_db_path.display(),
                dev_legacy_path.display(),
                new_db_path.display(),
            );
            config.database_url = format!("sqlite:{}", dev_legacy_path.display());
        }

        // 2. Pre-.process layout: data_dir/.database/agent-deck.db
        let pre_process_legacy_path = config.data_dir.join(".database").join("agent-deck.db");
        if pre_process_legacy_path.exists() && !new_db_path.exists() {
            warn!(
                "mcp: legacy database found at {} but new path {} does not exist yet. \
                 Using legacy path for this run. Copy or move the file to migrate: \
                 cp {} {}",
                pre_process_legacy_path.display(),
                new_db_path.display(),
                pre_process_legacy_path.display(),
                new_db_path.display(),
            );
            config.database_url = format!("sqlite:{}", pre_process_legacy_path.display());
        }
    }

    // Initialize database
    let pool = db::init(&config.database_url).await?;

    // Cleanup orphaned routine_executions from a previous crash.
    // Any execution that was marked 'running' at startup never completed —
    // mark them as failed so the UI doesn't show them as stuck.
    sqlx::query(
        "UPDATE routine_executions SET status = 'failed', error = 'server restarted during execution'
         WHERE status = 'running'",
    )
    .execute(&pool)
    .await?;
    info!("Orphaned routine_executions cleaned up");

    // Build the application router.  Also returns the McpConnectionManager
    // handle so we can shut down all MCP child processes cleanly on exit.
    let (app, mcp) = routes::build_router(pool.clone(), config.clone()).await?;

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    info!("Listening on port {}", config.port);

    // Graceful shutdown: wait for SIGTERM or SIGINT, then disconnect all MCP
    // servers (killing local child processes) before the runtime exits.
    let shutdown_signal = async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
            let mut sigint =
                signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
            tokio::select! {
                _ = sigterm.recv() => info!("Received SIGTERM, shutting down"),
                _ = sigint.recv()  => info!("Received SIGINT, shutting down"),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl-C handler");
            info!("Received Ctrl-C, shutting down");
        }

        // Disconnect all MCP servers: kills child processes, closes HTTP clients.
        mcp.shutdown_all().await;
        info!("MCP connections closed");
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal)
    .await?;

    info!("Server shut down cleanly");
    Ok(())
}

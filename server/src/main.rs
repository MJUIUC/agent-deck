#![recursion_limit = "256"]

mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;

use anyhow::Result;
use std::net::SocketAddr;
use tracing::info;

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
    let config = config::Config::from_env()?;

    // Initialize database
    let pool = db::init(&config.database_url).await?;

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

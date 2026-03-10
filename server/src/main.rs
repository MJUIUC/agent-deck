#![recursion_limit = "256"]

mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;

use anyhow::Result;
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

    // Build and run the application
    let app = routes::build_router(pool.clone(), config.clone()).await?;

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    info!("Listening on port {}", config.port);

    axum::serve(listener, app).await?;

    Ok(())
}

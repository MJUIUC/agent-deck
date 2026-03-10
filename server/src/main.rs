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

/// Ensure a user row exists — auto-creates one on first run so the single-user
/// local app works without a separate setup wizard.
async fn ensure_setup(pool: &sqlx::SqlitePool) -> Result<()> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;

    if count == 0 {
        info!("First run — completing setup automatically");
        let user = models::user::User::new("Local User");
        sqlx::query("INSERT INTO users (id, display_name, created_at) VALUES (?, ?, ?)")
            .bind(&user.id)
            .bind(&user.display_name)
            .bind(&user.created_at)
            .execute(pool)
            .await?;

        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        sqlx::query(
            "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(models::app_config::keys::SETUP_COMPLETE)
        .bind("true")
        .bind(&now)
        .execute(pool)
        .await?;

        info!("Setup complete — local user created");
    }

    Ok(())
}

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

    // Auto-complete setup on first run
    ensure_setup(&pool).await?;

    // Build and run the application
    let app = routes::build_router(pool.clone(), config.clone()).await?;

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    info!("Listening on port {}", config.port);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

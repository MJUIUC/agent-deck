use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tracing::info;

pub async fn init(database_url: &str) -> Result<SqlitePool> {
    // Ensure the data directory exists
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        let path = std::path::Path::new(path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Enable WAL mode and foreign keys
    sqlx::query("PRAGMA journal_mode=WAL;")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON;")
        .execute(&pool)
        .await?;

    // Run migrations
    info!("Running database migrations...");
    sqlx::migrate!("src/db/migrations").run(&pool).await?;
    info!("Migrations complete");

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_initializes_in_memory() {
        let pool = init("sqlite::memory:").await.expect("DB should initialize");
        // Verify we can query the DB
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .expect("users table should exist");
        assert_eq!(result.0, 0);
    }
}

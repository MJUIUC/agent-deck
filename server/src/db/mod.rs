use anyhow::Result;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::str::FromStr;
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

    // Strip the "sqlite:" prefix for SqliteConnectOptions, which requires
    // a plain file path (or ":memory:"). Then set create_if_missing so a
    // brand-new database file is created on first run.
    let db_path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);

    // Detect whether the DB file already exists so we can log clearly.
    // ":memory:" is always "new" and skips the file check.
    let is_new_db = if db_path == ":memory:" {
        false
    } else {
        let exists = std::path::Path::new(db_path).exists();
        if !exists {
            info!(
                path = db_path,
                "Database file not found — creating a new database. \
                 If you expected an existing database, check that DATABASE_URL \
                 in your .env points to the correct path."
            );
        } else {
            info!(path = db_path, "Opening existing database");
        }
        !exists
    };

    let connect_options = SqliteConnectOptions::from_str(db_path)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    // Enable WAL mode and foreign keys
    sqlx::query("PRAGMA journal_mode=WAL;")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON;")
        .execute(&pool)
        .await?;

    // Run migrations
    if is_new_db {
        info!("Running migrations on new database...");
    } else {
        info!("Running database migrations (checking for pending)...");
    }
    sqlx::migrate!("src/db/migrations").run(&pool).await?;
    if is_new_db {
        info!("New database initialized successfully");
    } else {
        info!("Migrations complete");
    }

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

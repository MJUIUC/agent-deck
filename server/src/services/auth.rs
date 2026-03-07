use anyhow::Result;
use rand::Rng;
use sqlx::SqlitePool;
use tracing::info;

use crate::models::app_config::{keys, AppConfig};

/// Generate a cryptographically random 64-character hex token.
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    hex::encode(bytes)
}

/// Generate a machine-derived encryption secret.
/// Uses a random 32-byte seed stored persistently in app_config.
/// On first call it creates the seed; subsequent calls read the stored value.
pub async fn get_or_create_machine_secret(pool: &SqlitePool) -> Result<String> {
    const KEY: &str = "machine_secret";

    let existing: Option<AppConfig> =
        sqlx::query_as("SELECT key, value, updated_at FROM app_config WHERE key = ?")
            .bind(KEY)
            .fetch_optional(pool)
            .await?;

    if let Some(config) = existing {
        return Ok(config.value);
    }

    // Generate a new random 32-byte secret
    let bytes: [u8; 32] = rand::thread_rng().gen();
    let secret = hex::encode(bytes);

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query("INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(KEY)
        .bind(&secret)
        .bind(&now)
        .execute(pool)
        .await?;

    Ok(secret)
}

/// Get the auth token from app_config, or generate and store one on first run.
/// The token is printed to the terminal on startup so the user can access remote clients.
pub async fn get_or_create_auth_token(pool: &SqlitePool) -> Result<String> {
    let existing: Option<AppConfig> =
        sqlx::query_as("SELECT key, value, updated_at FROM app_config WHERE key = ?")
            .bind(keys::AUTH_TOKEN)
            .fetch_optional(pool)
            .await?;

    if let Some(config) = existing {
        return Ok(config.value);
    }

    // First run — generate and persist a new token
    let token = generate_token();
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query("INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(keys::AUTH_TOKEN)
        .bind(&token)
        .bind(&now)
        .execute(pool)
        .await?;

    info!("Generated new auth token (first run)");
    Ok(token)
}

/// Validate a token string against the stored auth token.
pub async fn validate_token(pool: &SqlitePool, candidate: &str) -> Result<bool> {
    let stored: Option<AppConfig> =
        sqlx::query_as("SELECT key, value, updated_at FROM app_config WHERE key = ?")
            .bind(keys::AUTH_TOKEN)
            .fetch_optional(pool)
            .await?;

    Ok(stored.map(|c| c.value == candidate).unwrap_or(false))
}

/// Rotate the auth token — generates a new one and updates it in the DB.
pub async fn rotate_auth_token(pool: &SqlitePool) -> Result<String> {
    let new_token = generate_token();
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(keys::AUTH_TOKEN)
    .bind(&new_token)
    .bind(&now)
    .execute(pool)
    .await?;

    info!("Auth token rotated");
    Ok(new_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory DB");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    #[test]
    fn test_generate_token_is_64_chars() {
        let token = generate_token();
        assert_eq!(token.len(), 64, "token should be 64 hex chars");
    }

    #[test]
    fn test_generate_token_is_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2, "two generated tokens should differ");
    }

    #[test]
    fn test_generate_token_is_hex() {
        let token = generate_token();
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "token should be lowercase hex"
        );
    }

    #[tokio::test]
    async fn test_get_or_create_auth_token_idempotent() {
        let pool = setup_test_db().await;

        let t1 = get_or_create_auth_token(&pool).await.expect("first call");
        let t2 = get_or_create_auth_token(&pool).await.expect("second call");

        assert_eq!(t1, t2, "repeated calls should return the same token");
    }

    #[tokio::test]
    async fn test_validate_token_accepts_correct_token() {
        let pool = setup_test_db().await;
        let token = get_or_create_auth_token(&pool).await.expect("create token");
        let valid = validate_token(&pool, &token).await.expect("validate");
        assert!(valid, "correct token should be accepted");
    }

    #[tokio::test]
    async fn test_validate_token_rejects_wrong_token() {
        let pool = setup_test_db().await;
        let _token = get_or_create_auth_token(&pool).await.expect("create token");
        let valid = validate_token(&pool, "wrong-token-value")
            .await
            .expect("validate");
        assert!(!valid, "wrong token should be rejected");
    }

    #[tokio::test]
    async fn test_rotate_auth_token_changes_token() {
        let pool = setup_test_db().await;
        let original = get_or_create_auth_token(&pool).await.expect("original");
        let rotated = rotate_auth_token(&pool).await.expect("rotate");

        assert_ne!(original, rotated, "rotated token should differ");

        // Old token should now be invalid
        let old_valid = validate_token(&pool, &original)
            .await
            .expect("validate old");
        assert!(!old_valid, "old token should be rejected after rotation");

        // New token should be valid
        let new_valid = validate_token(&pool, &rotated).await.expect("validate new");
        assert!(new_valid, "new token should be accepted");
    }

    #[tokio::test]
    async fn test_machine_secret_idempotent() {
        let pool = setup_test_db().await;
        let s1 = get_or_create_machine_secret(&pool).await.expect("first");
        let s2 = get_or_create_machine_secret(&pool).await.expect("second");
        assert_eq!(s1, s2, "machine secret should be stable across calls");
    }
}

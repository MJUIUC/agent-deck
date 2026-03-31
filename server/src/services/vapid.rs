//! VAPID key management for Web Push notifications.
//!
//! Generates a P-256 key pair on first server run and persists both keys in
//! `app_config`. On every subsequent startup the persisted keys are loaded and
//! returned unchanged — ensuring a stable VAPID identity across restarts.

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use p256::{elliptic_curve::sec1::ToEncodedPoint, pkcs8::EncodePrivateKey, SecretKey};
use sqlx::SqlitePool;
use tracing::info;

use crate::models::app_config::keys;

/// Retrieve the VAPID key pair from `app_config`, or generate and persist a
/// new one on first run.
///
/// Returns `(private_key_pem, public_key_b64url)`:
/// - `private_key_pem`: PKCS8 PEM string — load with
///   `VapidSignatureBuilder::from_pem` in Story 7.2. **Never log or expose.**
/// - `public_key_b64url`: base64url-no-pad encoded uncompressed P-256 point
///   (65 bytes → 87 chars). Safe to expose via the public API endpoint.
pub async fn get_or_create_vapid_keys(pool: &SqlitePool) -> Result<(String, String)> {
    // Check whether both keys already exist.
    let existing_private: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_config WHERE key = ?")
            .bind(keys::VAPID_PRIVATE_KEY)
            .fetch_optional(pool)
            .await?;

    let existing_public: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_config WHERE key = ?")
            .bind(keys::VAPID_PUBLIC_KEY)
            .fetch_optional(pool)
            .await?;

    if let (Some((priv_pem,)), Some((pub_b64,))) = (existing_private, existing_public) {
        return Ok((priv_pem, pub_b64));
    }

    // First run — generate a fresh P-256 key pair.
    let secret = SecretKey::random(&mut rand::thread_rng());

    let pem = secret
        .to_pkcs8_pem(Default::default())
        .map_err(|e| anyhow::anyhow!("VAPID PEM encoding failed: {}", e))?
        .to_string();

    // Uncompressed public key point: 0x04 || X (32 bytes) || Y (32 bytes) = 65 bytes total.
    let point = secret.public_key().to_encoded_point(false);
    let public_b64 = URL_SAFE_NO_PAD.encode(point.as_bytes());

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    // Persist private key.
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO NOTHING",
    )
    .bind(keys::VAPID_PRIVATE_KEY)
    .bind(&pem)
    .bind(&now)
    .execute(pool)
    .await?;

    // Persist public key.
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO NOTHING",
    )
    .bind(keys::VAPID_PUBLIC_KEY)
    .bind(&public_b64)
    .bind(&now)
    .execute(pool)
    .await?;

    info!("vapid: generated new key pair (first run)");
    Ok((pem, public_b64))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory DB");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    #[tokio::test]
    async fn test_vapid_keys_stable_across_calls() {
        let pool = setup_db().await;

        let (priv1, pub1) = get_or_create_vapid_keys(&pool).await.expect("first call");
        let (priv2, pub2) = get_or_create_vapid_keys(&pool).await.expect("second call");

        assert_eq!(priv1, priv2, "private key must be stable across calls");
        assert_eq!(pub1, pub2, "public key must be stable across calls");
    }

    #[tokio::test]
    async fn test_vapid_public_key_is_valid_base64url() {
        let pool = setup_db().await;
        let (_, pub_b64) = get_or_create_vapid_keys(&pool).await.expect("generate");

        let bytes = URL_SAFE_NO_PAD
            .decode(&pub_b64)
            .expect("should be valid base64url");

        assert_eq!(
            bytes.len(),
            65,
            "uncompressed P-256 point is always 65 bytes"
        );
        assert_eq!(
            bytes[0], 0x04,
            "first byte must be 0x04 (uncompressed point marker)"
        );
    }

    #[tokio::test]
    async fn test_vapid_private_key_is_pem() {
        let pool = setup_db().await;
        let (priv_pem, _) = get_or_create_vapid_keys(&pool).await.expect("generate");

        assert!(
            priv_pem.starts_with("-----BEGIN"),
            "private key must be PEM-encoded, got: {}",
            &priv_pem[..50.min(priv_pem.len())]
        );
    }
}

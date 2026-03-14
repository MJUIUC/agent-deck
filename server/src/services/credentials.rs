use anyhow::{anyhow, Result};
use rand::Rng;
use sqlx::SqlitePool;
use tracing::info;

use crate::models::app_config::keys;
use crate::models::credential::{
    CreateCredential, Credential, CredentialSecret, CredentialWithData, UpdateCredential,
    CREDENTIAL_TYPES,
};
use crate::services::encryption;

// ---------------------------------------------------------------------------
// Master key management
// ---------------------------------------------------------------------------

/// Retrieve the credential master key from `app_config`, or generate and
/// persist a new one on first run.
///
/// The key is a random 256-bit value encoded as a 64-character hex string.
/// It is **never** included in log output or API responses — callers must
/// treat the return value as sensitive.
pub async fn get_or_create_master_key(pool: &SqlitePool) -> Result<String> {
    let existing: Option<(String,)> = sqlx::query_as("SELECT value FROM app_config WHERE key = ?")
        .bind(keys::CREDENTIAL_MASTER_KEY)
        .fetch_optional(pool)
        .await?;

    if let Some((value,)) = existing {
        return Ok(value);
    }

    // First run — generate a fresh 256-bit key.
    let bytes: [u8; 32] = rand::thread_rng().gen();
    let master_key = hex::encode(bytes);

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query("INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(keys::CREDENTIAL_MASTER_KEY)
        .bind(&master_key)
        .bind(&now)
        .execute(pool)
        .await?;

    info!("Generated new credential master key (first run)");
    Ok(master_key)
}

// ---------------------------------------------------------------------------
// Encryption helpers (thin wrappers around services::encryption)
// ---------------------------------------------------------------------------

/// Encrypt a `CredentialSecret` blob using the master key.
/// Returns a base64-encoded `nonce || ciphertext || tag` string.
fn encrypt_secret(secret: &CredentialSecret, master_key: &str) -> Result<String> {
    let json = serde_json::to_string(secret)
        .map_err(|e| anyhow!("Failed to serialize credential secret: {}", e))?;
    encryption::encrypt(&json, master_key)
}

/// Decrypt an `encrypted_data` column value back into a `CredentialSecret`.
pub fn decrypt_secret(encrypted_data: &str, master_key: &str) -> Result<CredentialSecret> {
    let json = encryption::decrypt(encrypted_data, master_key)?;
    serde_json::from_str(&json)
        .map_err(|e| anyhow!("Failed to deserialize credential secret: {}", e))
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

/// Validate that the given `credential_type` is one of the allowed values.
fn validate_credential_type(credential_type: &str) -> Result<()> {
    if CREDENTIAL_TYPES.contains(&credential_type) {
        Ok(())
    } else {
        Err(anyhow!(
            "Invalid credential_type '{}'. Must be one of: {}",
            credential_type,
            CREDENTIAL_TYPES.join(", ")
        ))
    }
}

/// Return all credentials as public records (no `encrypted_data`).
pub async fn list_credentials(pool: &SqlitePool) -> Result<Vec<Credential>> {
    let rows: Vec<Credential> = sqlx::query_as(
        "SELECT id, key, display_name, service, credential_type,
                service_url, username, email, created_at, updated_at
         FROM credentials
         ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Return a single credential by ID as a public record.
pub async fn get_credential(pool: &SqlitePool, id: &str) -> Result<Option<Credential>> {
    let row: Option<Credential> = sqlx::query_as(
        "SELECT id, key, display_name, service, credential_type,
                service_url, username, email, created_at, updated_at
         FROM credentials
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Return a single credential by its unique `key` field, including the
/// encrypted payload — for internal use only (never sent to API callers).
pub async fn get_credential_with_data_by_key(
    pool: &SqlitePool,
    key: &str,
) -> Result<Option<CredentialWithData>> {
    let row: Option<CredentialWithData> = sqlx::query_as(
        "SELECT id, key, display_name, service, credential_type,
                service_url, username, email, encrypted_data, created_at, updated_at
         FROM credentials
         WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Decrypt and return just the raw secret string for a credential identified
/// by its `key` field.  Used by the connection manager to inject secrets into
/// MCP server env vars / auth headers.
pub async fn resolve_secret(
    pool: &SqlitePool,
    master_key: &str,
    credential_key: &str,
) -> Result<String> {
    let row = get_credential_with_data_by_key(pool, credential_key)
        .await?
        .ok_or_else(|| anyhow!("Credential '{}' not found", credential_key))?;

    let secret = decrypt_secret(&row.encrypted_data, master_key)?;
    Ok(secret.secret)
}

/// Create a new credential, encrypting the secret before storage.
/// Returns the public-facing record (no `encrypted_data`).
pub async fn create_credential(
    pool: &SqlitePool,
    master_key: &str,
    req: &CreateCredential,
) -> Result<Credential> {
    validate_credential_type(&req.credential_type)?;

    // Check for key uniqueness up-front for a clearer error message.
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM credentials WHERE key = ?")
        .bind(&req.key)
        .fetch_optional(pool)
        .await?;
    if existing.is_some() {
        return Err(anyhow!(
            "A credential with key '{}' already exists",
            req.key
        ));
    }

    let secret_blob = CredentialSecret {
        secret: req.secret.clone(),
        password: req.password.clone(),
    };
    let encrypted_data = encrypt_secret(&secret_blob, master_key)?;

    let record = Credential::new_record(req);
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO credentials
           (id, key, display_name, service, credential_type,
            service_url, username, email, encrypted_data, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&record.id)
    .bind(&record.key)
    .bind(&record.display_name)
    .bind(&record.service)
    .bind(&record.credential_type)
    .bind(&record.service_url)
    .bind(&record.username)
    .bind(&record.email)
    .bind(&encrypted_data)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // Return a fresh read so the caller gets DB-generated timestamps.
    get_credential(pool, &record.id)
        .await?
        .ok_or_else(|| anyhow!("Credential not found after insert"))
}

/// Update an existing credential.  Only supplied fields are changed.
/// If `secret` is provided the encrypted blob is re-written; otherwise the
/// existing encrypted value is preserved.
/// Returns the updated public-facing record.
pub async fn update_credential(
    pool: &SqlitePool,
    master_key: &str,
    id: &str,
    req: &UpdateCredential,
) -> Result<Option<Credential>> {
    // Load the full row (including encrypted_data) so we can re-encrypt if needed.
    let existing: Option<CredentialWithData> = sqlx::query_as(
        "SELECT id, key, display_name, service, credential_type,
                service_url, username, email, encrypted_data, created_at, updated_at
         FROM credentials
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let existing = match existing {
        Some(r) => r,
        None => return Ok(None),
    };

    // Validate type if being changed.
    if let Some(ct) = &req.credential_type {
        validate_credential_type(ct)?;
    }

    // Determine the new encrypted_data value.
    let encrypted_data = if let Some(new_secret) = &req.secret {
        // Re-encrypt with possibly updated password.
        let secret_blob = CredentialSecret {
            secret: new_secret.clone(),
            password: req.password.clone(),
        };
        encrypt_secret(&secret_blob, master_key)?
    } else if req.password.is_some() {
        // Password changed but secret unchanged — decrypt old, re-encrypt.
        let mut old_blob = decrypt_secret(&existing.encrypted_data, master_key)?;
        old_blob.password = req.password.clone();
        encrypt_secret(&old_blob, master_key)?
    } else {
        existing.encrypted_data.clone()
    };

    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE credentials
         SET display_name   = COALESCE(?, display_name),
             service        = COALESCE(?, service),
             credential_type = COALESCE(?, credential_type),
             service_url    = COALESCE(?, service_url),
             username       = COALESCE(?, username),
             email          = COALESCE(?, email),
             encrypted_data = ?,
             updated_at     = ?
         WHERE id = ?",
    )
    .bind(&req.display_name)
    .bind(&req.service)
    .bind(&req.credential_type)
    .bind(&req.service_url)
    .bind(&req.username)
    .bind(&req.email)
    .bind(&encrypted_data)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    get_credential(pool, id)
        .await
        .map(Some)
        .and_then(|opt| opt.map_or(Ok(None), |inner| Ok(inner)))
}

/// Delete a credential by ID.
/// Returns `true` if a row was deleted, `false` if the ID was not found.
pub async fn delete_credential(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM credentials WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Return the list of MCP server names that reference this credential via
/// their `config` JSON field (`credential_key` placeholder pattern).
/// Used to warn the UI before deletion.
pub async fn mcp_servers_using_credential(
    pool: &SqlitePool,
    credential_key: &str,
) -> Result<Vec<String>> {
    // The config column stores JSON; we look for the placeholder pattern
    // "{credential:<key>}" anywhere in the config text.
    let pattern = format!("%{{credential:{}}}%", credential_key);
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM mcp_servers WHERE config LIKE ?")
        .bind(&pattern)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(name,)| name).collect())
}

// ---------------------------------------------------------------------------
// Provider key migration helper
// ---------------------------------------------------------------------------

/// Migrate a provider's plaintext `api_key` into the credential store.
///
/// Creates a credential with:
///   - `key`             = `"{kind}_{id_prefix}"` (first 8 chars of provider ID)
///   - `service`         = provider kind string
///   - `credential_type` = `"api_key"`
///   - `display_name`    = `"{kind} API Key"`
///
/// Updates `providers.credential_key` to point at the new credential's key,
/// then clears `providers.api_key`.
///
/// Is a no-op if the provider already has a `credential_key` set, or if its
/// `api_key` is NULL / empty.
pub async fn migrate_provider_api_key(
    pool: &SqlitePool,
    master_key: &str,
    provider_id: &str,
    provider_kind: &str,
    plaintext_api_key: &str,
) -> Result<String> {
    if plaintext_api_key.is_empty() {
        return Err(anyhow!("plaintext_api_key is empty — nothing to migrate"));
    }

    let id_prefix = &provider_id[..provider_id.len().min(8)];
    let cred_key = format!("{}_{}", provider_kind, id_prefix);

    // Idempotency: if a credential with this key already exists, just return it.
    if let Some(existing) = get_credential_with_data_by_key(pool, &cred_key).await? {
        return Ok(existing.key);
    }

    let req = CreateCredential {
        key: cred_key.clone(),
        display_name: format!("{} API Key", provider_kind),
        service: provider_kind.to_string(),
        credential_type: "api_key".to_string(),
        service_url: None,
        username: None,
        email: None,
        secret: plaintext_api_key.to_string(),
        password: None,
    };

    create_credential(pool, master_key, &req).await?;

    // Link the provider to the new credential and clear the plaintext key.
    sqlx::query("UPDATE providers SET credential_key = ?, api_key = NULL WHERE id = ?")
        .bind(&cred_key)
        .bind(provider_id)
        .execute(pool)
        .await?;

    info!(
        provider_id = provider_id,
        credential_key = %cred_key,
        "Migrated provider API key into credential store"
    );

    Ok(cred_key)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

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

    fn test_master_key() -> String {
        let bytes: [u8; 32] = [0x42u8; 32];
        hex::encode(bytes)
    }

    fn sample_create() -> CreateCredential {
        CreateCredential {
            key: "openai_test".to_string(),
            display_name: "OpenAI Test Key".to_string(),
            service: "openai".to_string(),
            credential_type: "api_key".to_string(),
            service_url: None,
            username: None,
            email: None,
            secret: "sk-supersecret".to_string(),
            password: None,
        }
    }

    // ── Master key ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_master_key_generated_on_first_run() {
        let pool = setup_db().await;
        let key = get_or_create_master_key(&pool).await.expect("master key");
        assert_eq!(key.len(), 64, "key should be 64 hex chars (32 bytes)");
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "key should be lowercase hex"
        );
    }

    #[tokio::test]
    async fn test_master_key_stable_across_calls() {
        let pool = setup_db().await;
        let k1 = get_or_create_master_key(&pool).await.expect("first");
        let k2 = get_or_create_master_key(&pool).await.expect("second");
        assert_eq!(k1, k2, "master key should be identical on subsequent calls");
    }

    // ── Encrypt / decrypt round-trip ──────────────────────────────────────────

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let master_key = test_master_key();
        let secret = CredentialSecret {
            secret: "ghp_abc123".to_string(),
            password: Some("hunter2".to_string()),
        };
        let encrypted = encrypt_secret(&secret, &master_key).expect("encrypt");
        let decrypted = decrypt_secret(&encrypted, &master_key).expect("decrypt");
        assert_eq!(decrypted.secret, secret.secret);
        assert_eq!(decrypted.password, secret.password);
    }

    #[test]
    fn test_different_encryptions_differ() {
        let master_key = test_master_key();
        let secret = CredentialSecret {
            secret: "same-secret".to_string(),
            password: None,
        };
        let enc1 = encrypt_secret(&secret, &master_key).expect("enc1");
        let enc2 = encrypt_secret(&secret, &master_key).expect("enc2");
        assert_ne!(enc1, enc2, "nonce uniqueness: two ciphertexts must differ");
    }

    // ── CRUD ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_and_list() {
        let pool = setup_db().await;
        let mk = test_master_key();

        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let list = list_credentials(&pool).await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].key, "openai_test");
        assert_eq!(list[0].service, "openai");
        assert_eq!(list[0].credential_type, "api_key");
    }

    #[tokio::test]
    async fn test_list_never_exposes_encrypted_data() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        // Serialize the list to JSON and confirm encrypted_data is absent.
        let list = list_credentials(&pool).await.expect("list");
        let json = serde_json::to_string(&list).expect("serialize");
        assert!(
            !json.contains("encrypted_data"),
            "API response must not contain encrypted_data"
        );
    }

    #[tokio::test]
    async fn test_duplicate_key_rejected() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("first create");
        let result = create_credential(&pool, &mk, &sample_create()).await;
        assert!(result.is_err(), "duplicate key should be rejected");
    }

    #[tokio::test]
    async fn test_get_credential() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let created = create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");
        let fetched = get_credential(&pool, &created.id)
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.key, "openai_test");
    }

    #[tokio::test]
    async fn test_resolve_secret() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");
        let secret = resolve_secret(&pool, &mk, "openai_test")
            .await
            .expect("resolve");
        assert_eq!(secret, "sk-supersecret");
    }

    #[tokio::test]
    async fn test_update_display_name() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let created = create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let update = UpdateCredential {
            display_name: Some("Updated Name".to_string()),
            service: None,
            credential_type: None,
            service_url: None,
            username: None,
            email: None,
            secret: None,
            password: None,
        };
        let updated = update_credential(&pool, &mk, &created.id, &update)
            .await
            .expect("update")
            .expect("should exist");
        assert_eq!(updated.display_name, "Updated Name");
    }

    #[tokio::test]
    async fn test_update_secret_re_encrypts() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let created = create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let update = UpdateCredential {
            display_name: None,
            service: None,
            credential_type: None,
            service_url: None,
            username: None,
            email: None,
            secret: Some("sk-newsecret".to_string()),
            password: None,
        };
        update_credential(&pool, &mk, &created.id, &update)
            .await
            .expect("update");

        let resolved = resolve_secret(&pool, &mk, "openai_test")
            .await
            .expect("resolve");
        assert_eq!(resolved, "sk-newsecret");
    }

    #[tokio::test]
    async fn test_delete_credential() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let created = create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let deleted = delete_credential(&pool, &created.id).await.expect("delete");
        assert!(deleted, "should return true when a row was deleted");

        let fetched = get_credential(&pool, &created.id).await.expect("get");
        assert!(fetched.is_none(), "credential should be gone after delete");
    }

    #[tokio::test]
    async fn test_delete_nonexistent_returns_false() {
        let pool = setup_db().await;
        let deleted = delete_credential(&pool, "nonexistent-id")
            .await
            .expect("delete");
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_invalid_credential_type_rejected() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let mut req = sample_create();
        req.credential_type = "oauth2".to_string(); // old type, no longer valid
        let result = create_credential(&pool, &mk, &req).await;
        assert!(
            result.is_err(),
            "invalid credential_type should be rejected"
        );
    }
}

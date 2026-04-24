use anyhow::{anyhow, Result};
use rand::Rng;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

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

/// Decrypt and return a specific field (`"secret"` or `"password"`) of a
/// credential identified by its `key` field.
///
/// Used by the connection manager to inject individual fields of multi-part
/// credentials (e.g. `key_secret_pair`) into MCP server env vars using the
/// `{credential:<key>:<field>}` placeholder syntax.
pub async fn resolve_field(
    pool: &SqlitePool,
    master_key: &str,
    credential_key: &str,
    field: &str,
) -> Result<String> {
    debug!(
        credential_key = %credential_key,
        field = %field,
        "resolve_field: looking up credential"
    );

    let row = match get_credential_with_data_by_key(pool, credential_key).await? {
        Some(r) => r,
        None => {
            warn!(
                credential_key = %credential_key,
                "resolve_field: no credential found with this key"
            );
            return Err(anyhow!("Credential '{}' not found", credential_key));
        }
    };

    let blob = decrypt_secret(&row.encrypted_data, master_key).map_err(|e| {
        warn!(
            credential_key = %credential_key,
            field = %field,
            error = %e,
            "resolve_field: decryption failed"
        );
        e
    })?;

    match field {
        "secret" => blob.secret.ok_or_else(|| {
            warn!(
                credential_key = %credential_key,
                "resolve_field: credential has no 'secret' field"
            );
            anyhow!("Credential '{}' has no 'secret' field", credential_key)
        }),
        "password" => blob.password.ok_or_else(|| {
            warn!(
                credential_key = %credential_key,
                "resolve_field: credential has no 'password' field"
            );
            anyhow!("Credential '{}' has no 'password' field", credential_key)
        }),
        other => Err(anyhow!(
            "Unknown credential field '{}'; valid fields are 'secret' and 'password'",
            other
        )),
    }
}

/// Decrypt and return just the raw secret string for a credential identified
/// by its `key` field.  Used by the connection manager to inject secrets into
/// MCP server env vars / auth headers.
pub async fn resolve_secret(
    pool: &SqlitePool,
    master_key: &str,
    credential_key: &str,
) -> Result<String> {
    debug!(credential_key = %credential_key, "resolve_secret: looking up credential");

    let row = match get_credential_with_data_by_key(pool, credential_key).await? {
        Some(r) => {
            debug!(
                credential_key = %credential_key,
                found_key = %r.key,
                credential_type = %r.credential_type,
                encrypted_data_len = r.encrypted_data.len(),
                "resolve_secret: credential row found"
            );
            r
        }
        None => {
            warn!(credential_key = %credential_key, "resolve_secret: no credential found with this key");
            return Err(anyhow!("Credential '{}' not found", credential_key));
        }
    };

    debug!(credential_key = %credential_key, "resolve_secret: attempting decryption");
    let secret = match decrypt_secret(&row.encrypted_data, master_key) {
        Ok(s) => {
            debug!(credential_key = %credential_key, "resolve_secret: decryption succeeded");
            s
        }
        Err(e) => {
            warn!(
                credential_key = %credential_key,
                master_key_len = master_key.len(),
                encrypted_data_len = row.encrypted_data.len(),
                error = %e,
                "resolve_secret: decryption failed"
            );
            return Err(e);
        }
    };

    match secret.secret {
        Some(s) => Ok(s),
        None => {
            warn!(credential_key = %credential_key, "resolve_secret: credential has no primary secret (secret field is null)");
            Err(anyhow!(
                "Credential '{}' has no primary secret",
                credential_key
            ))
        }
    }
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
        secret: req.secret.clone(), // Option<String> — None is valid
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
    // Re-encrypt whenever either secret field is explicitly supplied in the request.
    let encrypted_data = if req.secret.is_some() || req.password.is_some() {
        // Decrypt the existing blob so we can merge: supplied fields win,
        // unmentioned fields carry over from the stored value.
        let old_blob = decrypt_secret(&existing.encrypted_data, master_key)?;
        let secret_blob = CredentialSecret {
            secret: if req.secret.is_some() {
                req.secret.clone() // caller explicitly set (or cleared) the secret
            } else {
                old_blob.secret // unchanged — carry forward
            },
            password: if req.password.is_some() {
                req.password.clone() // caller explicitly set (or cleared) the password
            } else {
                old_blob.password // unchanged — carry forward
            },
        };
        encrypt_secret(&secret_blob, master_key)?
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
    // The config column stores JSON; we look for both placeholder patterns:
    //   "{credential:<key>}"         (plain — returns secret)
    //   "{credential:<key>:<field>}" (field-selector — returns secret or password)
    // We use two OR'd LIKE clauses so either form is matched.
    let pattern_plain = format!("%{{credential:{}}}%", credential_key);
    let pattern_field = format!("%{{credential:{}:%}}%", credential_key);
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM mcp_servers WHERE config LIKE ? OR config LIKE ?")
            .bind(&pattern_plain)
            .bind(&pattern_field)
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
        service: Some(provider_kind.to_string()),
        credential_type: "api_key".to_string(),
        service_url: None,
        username: None,
        email: None,
        secret: Some(plaintext_api_key.to_string()),
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
// Tool call credential injection
// ---------------------------------------------------------------------------

/// Scan a JSON value tree for `{credential:…}` placeholder strings, resolve
/// each unique placeholder against the credential store, and return a new
/// value tree with all placeholders replaced by their decrypted values.
///
/// Placeholders may appear as standalone string values **or** embedded inside
/// a larger string (e.g. `"Bearer {credential:my_token}"`).  Each unique
/// placeholder is resolved exactly once regardless of how many times it appears.
///
/// Returns the original value unchanged when no placeholders are present.
/// Returns an error if any referenced credential cannot be resolved — the
/// caller should surface this as a tool error rather than panicking.
pub async fn inject_credentials(
    val: serde_json::Value,
    pool: &SqlitePool,
    master_key: &str,
) -> Result<serde_json::Value> {
    let mut placeholders: HashSet<String> = HashSet::new();
    collect_credential_placeholders(&val, &mut placeholders);

    if placeholders.is_empty() {
        return Ok(val);
    }

    // Resolve each unique placeholder exactly once.
    let mut resolved: HashMap<String, String> = HashMap::new();
    for placeholder in &placeholders {
        // Strip `{credential:` prefix and `}` suffix to get the inner spec.
        let inner = &placeholder["{credential:".len()..placeholder.len() - 1];
        let secret = match inner.split_once(':') {
            Some((key, field)) => resolve_field(pool, master_key, key, field).await?,
            None => resolve_secret(pool, master_key, inner).await?,
        };
        resolved.insert(placeholder.clone(), secret);
    }

    Ok(apply_credential_resolutions(val, &resolved))
}

/// Recursively collect every `{credential:…}` placeholder string found inside
/// any string value in the JSON tree.
fn collect_credential_placeholders(val: &serde_json::Value, out: &mut HashSet<String>) {
    match val {
        serde_json::Value::String(s) => {
            let mut search = s.as_str();
            while let Some(start) = search.find("{credential:") {
                match search[start..].find('}') {
                    Some(end_rel) => {
                        out.insert(search[start..start + end_rel + 1].to_string());
                        search = &search[start + end_rel + 1..];
                    }
                    None => break, // malformed — no closing brace
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map {
                collect_credential_placeholders(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_credential_placeholders(v, out);
            }
        }
        _ => {}
    }
}

/// Apply a pre-resolved placeholder → secret map to every string value in a
/// JSON tree.  Non-string values (numbers, booleans, null) are passed through
/// unchanged.
fn apply_credential_resolutions(
    val: serde_json::Value,
    resolved: &HashMap<String, String>,
) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => {
            let mut out = s;
            for (placeholder, secret) in resolved {
                out = out.replace(placeholder.as_str(), secret.as_str());
            }
            serde_json::Value::String(out)
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, apply_credential_resolutions(v, resolved)))
                .collect(),
        ),
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.into_iter()
                .map(|v| apply_credential_resolutions(v, resolved))
                .collect(),
        ),
        other => other,
    }
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
            service: Some("openai".to_string()),
            credential_type: "api_key".to_string(),
            service_url: None,
            username: None,
            email: None,
            secret: Some("sk-supersecret".to_string()),
            password: None,
        }
    }

    fn sample_create_password_only() -> CreateCredential {
        CreateCredential {
            key: "db_password_test".to_string(),
            display_name: "DB Password".to_string(),
            service: Some("postgres".to_string()),
            credential_type: "key_secret_pair".to_string(),
            service_url: Some("postgres://localhost/mydb".to_string()),
            username: Some("admin".to_string()),
            email: None,
            secret: None,
            password: Some("s3cr3t-db-pass".to_string()),
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
            secret: Some("ghp_abc123".to_string()),
            password: Some("hunter2".to_string()),
        };
        let encrypted = encrypt_secret(&secret, &master_key).expect("encrypt");
        let decrypted = decrypt_secret(&encrypted, &master_key).expect("decrypt");
        assert_eq!(decrypted.secret, secret.secret);
        assert_eq!(decrypted.password, secret.password);
    }

    #[test]
    fn test_encrypt_decrypt_password_only() {
        let master_key = test_master_key();
        let secret = CredentialSecret {
            secret: None,
            password: Some("only-a-password".to_string()),
        };
        let encrypted = encrypt_secret(&secret, &master_key).expect("encrypt");
        let decrypted = decrypt_secret(&encrypted, &master_key).expect("decrypt");
        assert_eq!(decrypted.secret, None);
        assert_eq!(decrypted.password, Some("only-a-password".to_string()));
    }

    #[test]
    fn test_different_encryptions_differ() {
        let master_key = test_master_key();
        let secret = CredentialSecret {
            secret: Some("same-secret".to_string()),
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
    async fn test_resolve_secret_fails_when_no_primary_secret() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create_password_only())
            .await
            .expect("create");
        let result = resolve_secret(&pool, &mk, "db_password_test").await;
        assert!(
            result.is_err(),
            "resolve_secret should fail when credential has no primary secret"
        );
    }

    #[tokio::test]
    async fn test_create_password_only_credential() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let created = create_credential(&pool, &mk, &sample_create_password_only())
            .await
            .expect("create");
        assert_eq!(created.key, "db_password_test");
        // Decrypt and verify only password is set
        let row = get_credential_with_data_by_key(&pool, "db_password_test")
            .await
            .expect("lookup")
            .expect("should exist");
        let blob = decrypt_secret(&row.encrypted_data, &mk).expect("decrypt");
        assert_eq!(blob.secret, None);
        assert_eq!(blob.password, Some("s3cr3t-db-pass".to_string()));
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
    async fn test_update_adds_password_preserves_secret() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let created = create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        // Add a password without touching the secret
        let update = UpdateCredential {
            display_name: None,
            service: None,
            credential_type: None,
            service_url: None,
            username: None,
            email: None,
            secret: None,
            password: Some("extra-password".to_string()),
        };
        update_credential(&pool, &mk, &created.id, &update)
            .await
            .expect("update");

        let row = get_credential_with_data_by_key(&pool, "openai_test")
            .await
            .expect("lookup")
            .expect("should exist");
        let blob = decrypt_secret(&row.encrypted_data, &mk).expect("decrypt");
        // Original secret must be preserved
        assert_eq!(blob.secret, Some("sk-supersecret".to_string()));
        // New password must be stored
        assert_eq!(blob.password, Some("extra-password".to_string()));
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

    // ── resolve_field ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_resolve_field_secret() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let req = CreateCredential {
            key: "schwab".to_string(),
            display_name: "Schwab Credentials".to_string(),
            service: Some("schwab".to_string()),
            credential_type: "key_secret_pair".to_string(),
            service_url: None,
            username: None,
            email: None,
            secret: Some("my-client-id".to_string()),
            password: Some("my-client-secret".to_string()),
        };
        create_credential(&pool, &mk, &req).await.expect("create");

        let value = resolve_field(&pool, &mk, "schwab", "secret")
            .await
            .expect("resolve secret");
        assert_eq!(value, "my-client-id");
    }

    #[tokio::test]
    async fn test_resolve_field_password() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let req = CreateCredential {
            key: "schwab".to_string(),
            display_name: "Schwab Credentials".to_string(),
            service: Some("schwab".to_string()),
            credential_type: "key_secret_pair".to_string(),
            service_url: None,
            username: None,
            email: None,
            secret: Some("my-client-id".to_string()),
            password: Some("my-client-secret".to_string()),
        };
        create_credential(&pool, &mk, &req).await.expect("create");

        let value = resolve_field(&pool, &mk, "schwab", "password")
            .await
            .expect("resolve password");
        assert_eq!(value, "my-client-secret");
    }

    #[tokio::test]
    async fn test_resolve_field_missing_credential() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let result = resolve_field(&pool, &mk, "nonexistent", "secret").await;
        assert!(
            result.is_err(),
            "should fail when credential key does not exist"
        );
    }

    #[tokio::test]
    async fn test_resolve_field_missing_password_field() {
        // Credential with secret only — resolving 'password' should fail.
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");
        let result = resolve_field(&pool, &mk, "openai_test", "password").await;
        assert!(result.is_err(), "should fail when 'password' field is None");
    }

    #[tokio::test]
    async fn test_resolve_field_missing_secret_field() {
        // Credential with password only — resolving 'secret' should fail.
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create_password_only())
            .await
            .expect("create");
        let result = resolve_field(&pool, &mk, "db_password_test", "secret").await;
        assert!(result.is_err(), "should fail when 'secret' field is None");
    }

    #[tokio::test]
    async fn test_resolve_field_unknown_field_rejected() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");
        let result = resolve_field(&pool, &mk, "openai_test", "token").await;
        assert!(result.is_err(), "unknown field name should be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Unknown credential field"),
            "error message should mention unknown field: {}",
            msg
        );
    }

    // ── mcp_servers_using_credential ──────────────────────────────────────────

    async fn insert_test_user(pool: &SqlitePool) {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, display_name, created_at) VALUES ('u1', 'Test', ?)",
        )
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_mcp_servers_using_credential_plain_pattern() {
        let pool = setup_db().await;
        insert_test_user(&pool).await;
        let now = chrono::Utc::now().to_rfc3339();
        // MCP server using plain placeholder {credential:schwab}
        sqlx::query(
            "INSERT INTO mcp_servers (id, user_id, name, tag, server_type, config, status, enabled, created_at, updated_at)
             VALUES ('s1', 'u1', 'myserver', 'myserver', 'local', '{\"env\":{\"KEY\":\"{credential:schwab}\"}}', 'inactive', 1, ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let names = mcp_servers_using_credential(&pool, "schwab")
            .await
            .expect("query");
        assert_eq!(names, vec!["myserver"]);
    }

    #[tokio::test]
    async fn test_mcp_servers_using_credential_field_pattern() {
        let pool = setup_db().await;
        insert_test_user(&pool).await;
        let now = chrono::Utc::now().to_rfc3339();
        // MCP server using field-selector placeholders {credential:schwab:secret} and {credential:schwab:password}
        sqlx::query(
            "INSERT INTO mcp_servers (id, user_id, name, tag, server_type, config, status, enabled, created_at, updated_at)
             VALUES ('s2', 'u1', 'schwabmcp', 'schwabmcp', 'local', '{\"env\":{\"A\":\"{credential:schwab:secret}\",\"B\":\"{credential:schwab:password}\"}}', 'inactive', 1, ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let names = mcp_servers_using_credential(&pool, "schwab")
            .await
            .expect("query");
        assert_eq!(names, vec!["schwabmcp"]);
    }

    #[tokio::test]
    async fn test_mcp_servers_using_credential_unrelated_not_returned() {
        let pool = setup_db().await;
        insert_test_user(&pool).await;
        let now = chrono::Utc::now().to_rfc3339();
        // Server using a different credential key
        sqlx::query(
            "INSERT INTO mcp_servers (id, user_id, name, tag, server_type, config, status, enabled, created_at, updated_at)
             VALUES ('s3', 'u1', 'other', 'other', 'local', '{\"env\":{\"X\":\"{credential:github_pat}\"}}', 'inactive', 1, ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let names = mcp_servers_using_credential(&pool, "schwab")
            .await
            .expect("query");
        assert!(names.is_empty(), "should not return unrelated servers");
    }

    // ── inject_credentials ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_inject_credentials_no_placeholders_is_noop() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let args = serde_json::json!({
            "url": "https://api.example.com",
            "method": "GET"
        });
        let result = inject_credentials(args.clone(), &pool, &mk)
            .await
            .expect("inject");
        assert_eq!(result, args);
    }

    #[tokio::test]
    async fn test_inject_credentials_standalone_placeholder() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let args = serde_json::json!({ "api_key": "{credential:openai_test}" });
        let result = inject_credentials(args, &pool, &mk).await.expect("inject");
        assert_eq!(result["api_key"], "sk-supersecret");
    }

    #[tokio::test]
    async fn test_inject_credentials_embedded_in_string() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let args = serde_json::json!({
            "authorization": "Bearer {credential:openai_test}"
        });
        let result = inject_credentials(args, &pool, &mk).await.expect("inject");
        assert_eq!(result["authorization"], "Bearer sk-supersecret");
    }

    #[tokio::test]
    async fn test_inject_credentials_field_selector() {
        let pool = setup_db().await;
        let mk = test_master_key();
        let req = CreateCredential {
            key: "schwab".to_string(),
            display_name: "Schwab".to_string(),
            service: Some("schwab".to_string()),
            credential_type: "key_secret_pair".to_string(),
            service_url: None,
            username: None,
            email: None,
            secret: Some("app-key".to_string()),
            password: Some("app-secret".to_string()),
        };
        create_credential(&pool, &mk, &req).await.expect("create");

        let args = serde_json::json!({
            "key": "{credential:schwab:secret}",
            "secret": "{credential:schwab:password}"
        });
        let result = inject_credentials(args, &pool, &mk).await.expect("inject");
        assert_eq!(result["key"], "app-key");
        assert_eq!(result["secret"], "app-secret");
    }

    #[tokio::test]
    async fn test_inject_credentials_nested_json() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let args = serde_json::json!({
            "headers": {
                "Authorization": "Bearer {credential:openai_test}",
                "Content-Type": "application/json"
            },
            "body": {
                "model": "gpt-4o"
            }
        });
        let result = inject_credentials(args, &pool, &mk).await.expect("inject");
        assert_eq!(result["headers"]["Authorization"], "Bearer sk-supersecret");
        assert_eq!(result["headers"]["Content-Type"], "application/json");
        assert_eq!(result["body"]["model"], "gpt-4o");
    }

    #[tokio::test]
    async fn test_inject_credentials_array_values() {
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let args = serde_json::json!({
            "tokens": ["{credential:openai_test}", "literal-value"]
        });
        let result = inject_credentials(args, &pool, &mk).await.expect("inject");
        assert_eq!(result["tokens"][0], "sk-supersecret");
        assert_eq!(result["tokens"][1], "literal-value");
    }

    #[tokio::test]
    async fn test_inject_credentials_missing_credential_returns_error() {
        let pool = setup_db().await;
        let mk = test_master_key();

        let args = serde_json::json!({ "key": "{credential:does_not_exist}" });
        let result = inject_credentials(args, &pool, &mk).await;
        assert!(
            result.is_err(),
            "should return error when credential key does not exist"
        );
    }

    #[tokio::test]
    async fn test_inject_credentials_unique_placeholder_resolved_once() {
        // Same placeholder appears in multiple fields — credential should be
        // resolved once and reused (verified indirectly by checking both values).
        let pool = setup_db().await;
        let mk = test_master_key();
        create_credential(&pool, &mk, &sample_create())
            .await
            .expect("create");

        let args = serde_json::json!({
            "field_a": "{credential:openai_test}",
            "field_b": "{credential:openai_test}"
        });
        let result = inject_credentials(args, &pool, &mk).await.expect("inject");
        assert_eq!(result["field_a"], "sk-supersecret");
        assert_eq!(result["field_b"], "sk-supersecret");
    }
}

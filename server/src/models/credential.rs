use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Credential types supported by the store.
pub const CREDENTIAL_TYPES: &[&str] = &["api_key", "pat", "bearer_token", "key_secret_pair"];

/// Public-facing credential record returned by the API.
/// `encrypted_data` is intentionally absent — it must never appear in responses.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Credential {
    pub id: String,
    pub key: String,
    pub display_name: String,
    pub service: String,
    pub credential_type: String,
    pub service_url: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Internal struct used when the encrypted payload is needed (e.g. decryption).
/// Never serialized to API responses.
#[derive(Debug, sqlx::FromRow)]
pub struct CredentialWithData {
    pub id: String,
    pub key: String,
    pub display_name: String,
    pub service: String,
    pub credential_type: String,
    pub service_url: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub encrypted_data: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Payload for creating a new credential.
/// The `secret` (and optional `password`) fields are raw — the service layer
/// encrypts them before writing to the database.
#[derive(Debug, Deserialize)]
pub struct CreateCredential {
    /// Unique machine-readable key used to reference this credential
    /// from other records (e.g. MCP server config, provider `credential_key`).
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub service: Option<String>,
    /// Must be one of: "api_key", "pat", "bearer_token", "key_secret_pair".
    pub credential_type: String,
    pub service_url: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    /// The primary secret value (API key, token, etc.). Optional — not all
    /// credential types require a primary secret (e.g. a key_secret_pair
    /// where only the password/secret-key matters).
    pub secret: Option<String>,
    /// Optional secondary secret — used for `key_secret_pair` type or any
    /// credential that requires a password alongside a key.
    pub password: Option<String>,
}

/// Payload for updating an existing credential.
/// All fields are optional; only supplied fields are updated.
#[derive(Debug, Deserialize)]
pub struct UpdateCredential {
    pub display_name: Option<String>,
    pub service: Option<String>, // remains Option — omitting it leaves the existing value unchanged
    pub credential_type: Option<String>,
    pub service_url: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    /// If provided, the secret is re-encrypted and stored.
    pub secret: Option<String>,
    /// If provided alongside `secret`, stored in the encrypted blob.
    pub password: Option<String>,
}

/// The JSON blob that gets AES-256-GCM encrypted and stored in `encrypted_data`.
/// Both fields are optional — callers should populate whichever are relevant
/// for the credential type. At least one should be non-None in practice.
#[derive(Debug, Serialize, Deserialize)]
pub struct CredentialSecret {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl Credential {
    /// Construct a new `Credential` from a `CreateCredential` request.
    /// The caller is responsible for supplying the `encrypted_data` string
    /// (result of encrypting a `CredentialSecret` JSON blob).
    pub fn new_record(req: &CreateCredential) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            key: req.key.clone(),
            display_name: req.display_name.clone(),
            service: req.service.clone().unwrap_or_default(),
            credential_type: req.credential_type.clone(),
            service_url: req.service_url.clone(),
            username: req.username.clone(),
            email: req.email.clone(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

impl CredentialWithData {
    /// Convert to the public-facing `Credential` (strips `encrypted_data`).
    pub fn into_public(self) -> Credential {
        Credential {
            id: self.id,
            key: self.key,
            display_name: self.display_name,
            service: self.service,
            credential_type: self.credential_type,
            service_url: self.service_url,
            username: self.username,
            email: self.email,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Public-facing credential record. Never includes `encrypted_data`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Credential {
    pub id: String,
    pub key: String,
    pub display_name: String,
    pub provider: String,
    pub credential_type: String, // "oauth2" | "api_key" | "custom"
    pub owner_type: String,      // "user" | "persona"
    pub persona_id: Option<String>,
    // NOTE: encrypted_data is intentionally NOT included here.
    // It must never appear in API responses.
    pub scopes: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Used internally when reading from DB — never serialized to API responses.
#[derive(Debug, sqlx::FromRow)]
pub struct CredentialWithData {
    pub id: String,
    pub key: String,
    pub encrypted_data: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCredential {
    pub key: String,
    pub display_name: String,
    pub provider: String,
    pub credential_type: String,
    pub owner_type: String,
    pub persona_id: Option<String>,
    pub secret: String, // raw secret — will be encrypted before storage
    pub scopes: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCredential {
    pub display_name: Option<String>,
    pub secret: Option<String>, // raw secret — will be encrypted before storage
    pub scopes: Option<String>,
    pub expires_at: Option<String>,
}

impl Credential {
    pub fn new(req: &CreateCredential, encrypted_data: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            key: req.key.clone(),
            display_name: req.display_name.clone(),
            provider: req.provider.clone(),
            credential_type: req.credential_type.clone(),
            owner_type: req.owner_type.clone(),
            persona_id: req.persona_id.clone(),
            scopes: req.scopes.clone(),
            expires_at: req.expires_at.clone(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

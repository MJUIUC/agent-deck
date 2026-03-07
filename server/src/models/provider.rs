use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Provider {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    /// Encrypted at rest. Never returned in plaintext via API.
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

/// What gets returned to the client — api_key is masked.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    /// Returns `"••••••••"` if a key is stored, `null` if not.
    pub api_key: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

impl From<Provider> for ProviderResponse {
    fn from(p: Provider) -> Self {
        Self {
            id: p.id,
            user_id: p.user_id,
            name: p.name,
            kind: p.kind,
            base_url: p.base_url,
            api_key: p.api_key.map(|_| "••••••••".to_string()),
            enabled: p.enabled,
            created_at: p.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateProvider {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProvider {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

impl Provider {
    pub fn new(user_id: impl Into<String>, req: CreateProvider) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            name: req.name,
            kind: req.kind,
            base_url: req.base_url,
            api_key: req.api_key,
            enabled: true,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

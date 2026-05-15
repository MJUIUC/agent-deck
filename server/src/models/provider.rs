use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
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
    pub vision: bool,
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
    pub vision: bool,
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
            vision: p.vision,
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
    pub vision: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProvider {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
    pub vision: Option<bool>,
}

impl Provider {
    /// The canonical column list for all SELECT queries on the providers table.
    /// Update this constant whenever a column is added or removed.
    const COLUMNS: &'static str =
        "id, user_id, name, kind, base_url, api_key, enabled, vision, created_at";

    /// Fetch a single provider by (id, user_id). Returns `None` if not found.
    pub async fn fetch(pool: &SqlitePool, id: &str, user_id: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as(&format!(
            "SELECT {} FROM providers WHERE id = ? AND user_id = ?",
            Self::COLUMNS
        ))
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    /// Fetch all providers for a user, ordered by creation time.
    pub async fn fetch_all(pool: &SqlitePool, user_id: &str) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as(&format!(
            "SELECT {} FROM providers WHERE user_id = ? ORDER BY created_at ASC",
            Self::COLUMNS
        ))
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub fn new(user_id: impl Into<String>, req: CreateProvider) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            name: req.name,
            kind: req.kind,
            base_url: req.base_url,
            api_key: req.api_key,
            enabled: true,
            vision: req.vision.unwrap_or(false),
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

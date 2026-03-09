use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct McpServer {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub server_type: String, // "local" | "remote"
    pub config: String,      // JSON string
    pub status: String,      // "inactive" | "connecting" | "connected" | "error"
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMcpServer {
    pub name: String,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub server_type: String,
    pub config: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServer {
    pub name: Option<String>,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub config: Option<Value>,
    pub enabled: Option<bool>,
}

impl McpServer {
    pub fn new(user_id: &str, req: CreateMcpServer) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            name: req.name,
            description: req.description,
            source_url: req.source_url,
            server_type: req.server_type,
            config: req.config.to_string(),
            status: "inactive".to_string(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

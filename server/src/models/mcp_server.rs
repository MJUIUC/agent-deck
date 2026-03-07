use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct McpServer {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub command: String,
    /// JSON array of string args, e.g. `["--port", "8080"]`
    pub args: String,
    /// JSON object of env vars, e.g. `{"API_KEY": "abc"}`
    pub env: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMcpServer {
    pub name: String,
    pub command: String,
    #[serde(default = "default_args")]
    pub args: serde_json::Value,
    #[serde(default = "default_env")]
    pub env: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServer {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<serde_json::Value>,
    pub env: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn default_args() -> serde_json::Value {
    serde_json::json!([])
}

fn default_env() -> serde_json::Value {
    serde_json::json!({})
}

impl McpServer {
    pub fn new(user_id: impl Into<String>, req: CreateMcpServer) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            name: req.name,
            command: req.command,
            args: req.args.to_string(),
            env: req.env.to_string(),
            enabled: true,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    /// Parse the stored JSON args string into a Vec<String>.
    pub fn parsed_args(&self) -> Vec<String> {
        serde_json::from_str::<Vec<String>>(&self.args).unwrap_or_default()
    }

    /// Parse the stored JSON env string into a HashMap<String, String>.
    pub fn parsed_env(&self) -> std::collections::HashMap<String, String> {
        serde_json::from_str(&self.env).unwrap_or_default()
    }
}

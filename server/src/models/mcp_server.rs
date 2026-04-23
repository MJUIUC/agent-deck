use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct McpServer {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub tag: String, // short identifier used to namespace tools, e.g. "github"
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub server_type: String, // "local" | "remote"
    pub config: String,      // JSON string
    pub status: String,      // "inactive" | "connecting" | "connected" | "error"
    pub enabled: bool,
    /// Per-server timeout for individual tool calls, in seconds.
    /// NULL means no timeout is enforced.
    pub tool_call_timeout_secs: Option<i64>,
    /// JSON array of tool names that are disabled for this server.
    /// e.g. `'["create_issue","delete_repo"]'`
    pub disabled_tools: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMcpServer {
    pub name: String,
    /// Short alphanumeric identifier used to namespace tool names.
    /// Defaults to `name` when not provided.
    pub tag: Option<String>,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub server_type: String,
    pub config: Value,
    /// Optional timeout in seconds for tool calls from this server.
    pub tool_call_timeout_secs: Option<i64>,
    /// Optional list of tool names to disable on this server.
    pub disabled_tools: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServer {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub config: Option<Value>,
    pub enabled: Option<bool>,
    /// `None` → omit (no change); `Some(null)` → clear timeout; `Some(n)` → set to n seconds.
    pub tool_call_timeout_secs: Option<Value>,
    /// `None` → omit (no change); `Some(vec)` → replace disabled tool list.
    pub disabled_tools: Option<Vec<String>>,
}

impl McpServer {
    pub fn new(user_id: &str, req: CreateMcpServer) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        // Default tag to name when the caller omits it, stripping whitespace and
        // lower-casing so "My Server" becomes "my_server" as a safe fallback.
        let tag = req
            .tag
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| req.name.to_lowercase().replace(' ', "_"));
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            name: req.name,
            tag,
            description: req.description,
            source_url: req.source_url,
            server_type: req.server_type,
            config: req.config.to_string(),
            status: "inactive".to_string(),
            enabled: true,
            tool_call_timeout_secs: req.tool_call_timeout_secs,
            disabled_tools: serde_json::to_string(&req.disabled_tools.unwrap_or_default())
                .unwrap_or_else(|_| "[]".to_string()),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

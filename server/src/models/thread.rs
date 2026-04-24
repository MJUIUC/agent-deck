use serde::{Deserialize, Serialize};
use serde_json;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Thread {
    pub id: String,
    pub user_id: String,
    pub persona_id: String,
    pub title: String,
    pub active_model: Option<String>,
    pub active_provider: Option<String>,
    pub system_prompt_addendum: Option<String>,
    pub status: String,
    pub show_tool_activity: bool,
    pub show_system_events: bool,
    pub summary: Option<String>,
    pub summary_updated_at: Option<String>,
    pub summary_message_count: i64,
    pub auto_summarize: bool,
    pub auto_retitle: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateThread {
    pub persona_id: Option<String>,
    pub title: Option<String>,
    pub active_model: Option<String>,
    pub active_provider: Option<String>,
    pub system_prompt_addendum: Option<String>,
    pub show_tool_activity: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateThread {
    pub title: Option<String>,
    pub active_model: Option<String>,
    pub active_provider: Option<String>,
    pub system_prompt_addendum: Option<String>,
    pub show_tool_activity: Option<bool>,
    pub show_system_events: Option<bool>,
    pub auto_summarize: Option<bool>,
    pub auto_retitle: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadMcpServer {
    pub id: String,
    pub thread_id: String,
    pub mcp_server_id: String,
    pub enabled: bool,
    pub disabled_tools: String, // JSON array, e.g. '["tool1"]'
    pub tool_call_timeout_secs: Option<i64>, // NULL = inherit from mcp_server default
}

#[derive(Debug, Deserialize)]
pub struct AttachMcpServer {
    pub mcp_server_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateThreadMcpServer {
    /// None = no change. Some(vec) = replace the full disabled list for this thread.
    pub disabled_tools: Option<Vec<String>>,
    /// None = no change. Some(null JSON value) = clear. Some(n) = set to n seconds.
    pub tool_call_timeout_secs: Option<serde_json::Value>,
}

impl Thread {
    pub fn new(
        user_id: impl Into<String>,
        persona_id: impl Into<String>,
        req: CreateThread,
    ) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            persona_id: persona_id.into(),
            title: req.title.unwrap_or_else(|| "New Chat".to_string()),
            active_model: req.active_model,
            active_provider: req.active_provider,
            system_prompt_addendum: req.system_prompt_addendum,
            status: "active".to_string(),
            show_tool_activity: req.show_tool_activity.unwrap_or(false),
            show_system_events: false,
            summary: None,
            summary_updated_at: None,
            summary_message_count: 0,
            auto_summarize: true,
            auto_retitle: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    pub fn is_archived(&self) -> bool {
        self.status == "archived"
    }
}

impl ThreadMcpServer {
    pub fn new(thread_id: impl Into<String>, mcp_server_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            mcp_server_id: mcp_server_id.into(),
            enabled: true,
            disabled_tools: "[]".to_string(),
            tool_call_timeout_secs: None,
        }
    }
}

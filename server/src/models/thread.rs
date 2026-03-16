use serde::{Deserialize, Serialize};
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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateThread {
    pub persona_id: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadMcpServer {
    pub id: String,
    pub thread_id: String,
    pub mcp_server_id: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct AttachMcpServer {
    pub mcp_server_id: String,
}

impl Thread {
    pub fn new(user_id: impl Into<String>, req: CreateThread) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            persona_id: req.persona_id,
            title: req.title.unwrap_or_else(|| "New Chat".to_string()),
            active_model: req.active_model,
            active_provider: req.active_provider,
            system_prompt_addendum: req.system_prompt_addendum,
            status: "active".to_string(),
            show_tool_activity: req.show_tool_activity.unwrap_or(false),
            show_system_events: false,
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
        }
    }
}

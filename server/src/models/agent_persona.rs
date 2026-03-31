use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentPersona {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub emoji: String,
    pub avatar_path: Option<String>,
    pub system_prompt: String,
    pub default_model: Option<String>,
    pub default_provider: Option<String>,
    /// True for the system-managed Default persona.
    /// Default personas cannot be edited or deleted via the API.
    pub is_default: bool,
    pub recall_conversation_cross_thread: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentPersona {
    pub name: String,
    pub emoji: String,
    pub system_prompt: String,
    pub default_model: Option<String>,
    pub default_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentPersona {
    pub name: Option<String>,
    pub emoji: Option<String>,
    pub system_prompt: Option<String>,
    pub default_model: Option<String>,
    pub default_provider: Option<String>,
    pub recall_conversation_cross_thread: Option<bool>,
}

impl AgentPersona {
    /// Create a new user-defined persona (`is_default = false`).
    pub fn new(user_id: impl Into<String>, req: CreateAgentPersona) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            name: req.name,
            emoji: req.emoji,
            avatar_path: None,
            system_prompt: req.system_prompt,
            default_model: req.default_model,
            default_provider: req.default_provider,
            is_default: false,
            recall_conversation_cross_thread: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Create the system-managed Default persona for a given user.
    /// This is called once at setup time and must not be called elsewhere.
    pub fn new_default(user_id: impl Into<String>) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            name: "Default".to_string(),
            emoji: "💬".to_string(),
            avatar_path: None,
            system_prompt: String::new(),
            default_model: None,
            default_provider: None,
            is_default: true,
            recall_conversation_cross_thread: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

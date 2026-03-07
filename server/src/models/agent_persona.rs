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
}

impl AgentPersona {
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
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

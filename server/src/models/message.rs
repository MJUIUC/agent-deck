use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub role: String,
    pub content: String,
    pub source: String,
    pub routine_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMessage {
    pub content: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_source")]
    pub source: String,
    pub routine_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub thread_id: String,
    pub role: String,
    pub content: String,
    pub source: String,
    pub routine_id: Option<String>,
    pub created_at: String,
}

fn default_role() -> String {
    "user".to_string()
}

fn default_source() -> String {
    "chat".to_string()
}

impl Message {
    pub fn new(thread_id: impl Into<String>, req: CreateMessage) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            role: req.role,
            content: req.content,
            source: req.source,
            routine_id: req.routine_id,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    pub fn new_user(thread_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            role: "user".to_string(),
            content: content.into(),
            source: "chat".to_string(),
            routine_id: None,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    pub fn new_assistant(thread_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            role: "assistant".to_string(),
            content: content.into(),
            source: "chat".to_string(),
            routine_id: None,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    pub fn new_routine(
        thread_id: impl Into<String>,
        content: impl Into<String>,
        routine_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            role: "assistant".to_string(),
            content: content.into(),
            source: "routine".to_string(),
            routine_id: Some(routine_id.into()),
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

impl From<Message> for MessageResponse {
    fn from(m: Message) -> Self {
        Self {
            id: m.id,
            thread_id: m.thread_id,
            role: m.role,
            content: m.content,
            source: m.source,
            routine_id: m.routine_id,
            created_at: m.created_at,
        }
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Memory {
    pub id: String,
    pub user_id: String,
    pub persona_id: String,
    pub thread_id: Option<String>,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemory {
    pub content: String,
    pub persona_id: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchQuery {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct MemoryListResponse {
    pub memories: Vec<MemoryEntry>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub thread_title: Option<String>,
    pub created_at: String,
}

fn default_limit() -> i64 {
    20
}

impl Memory {
    pub fn new(user_id: impl Into<String>, req: CreateMemory) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            persona_id: req.persona_id,
            thread_id: req.thread_id,
            content: req.content,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    pub fn new_for_thread(
        user_id: impl Into<String>,
        persona_id: impl Into<String>,
        thread_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            persona_id: persona_id.into(),
            thread_id: Some(thread_id.into()),
            content: content.into(),
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

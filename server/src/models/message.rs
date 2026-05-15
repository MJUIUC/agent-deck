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
    pub visibility: String, // "visible" | "hidden"
    pub execution_id: Option<String>,
    pub event_type: Option<String>,
    pub stopped: bool,
    pub attachments: Option<String>,
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
    #[serde(default = "default_visibility")]
    pub visibility: String,
    pub execution_id: Option<String>,
    pub attachments: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub thread_id: String,
    pub role: String,
    pub content: String,
    pub source: String,
    pub routine_id: Option<String>,
    pub visibility: String,
    pub execution_id: Option<String>,
    pub event_type: Option<String>,
    pub stopped: bool,
    /// Parsed from the stored JSON string so the client receives a proper array.
    pub attachments: Option<serde_json::Value>,
    pub created_at: String,
}

fn default_role() -> String {
    "user".to_string()
}

fn default_source() -> String {
    "chat".to_string()
}

fn default_visibility() -> String {
    "visible".to_string()
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
            visibility: req.visibility,
            execution_id: req.execution_id,
            event_type: None,
            stopped: false,
            attachments: None,
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
            visibility: "visible".to_string(),
            execution_id: None,
            event_type: None,
            stopped: false,
            attachments: None,
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
            visibility: "visible".to_string(),
            execution_id: None,
            event_type: None,
            stopped: false,
            attachments: None,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    pub fn new_assistant_stopped(thread_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            role: "assistant".to_string(),
            content: content.into(),
            source: "chat".to_string(),
            routine_id: None,
            visibility: "visible".to_string(),
            execution_id: None,
            event_type: None,
            stopped: true,
            attachments: None,
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
            visibility: "visible".to_string(),
            execution_id: None,
            event_type: None,
            stopped: false,
            attachments: None,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    pub fn new_chat_segment(thread_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            role: "assistant".to_string(),
            content: content.into(),
            source: "chat".to_string(),
            routine_id: None,
            visibility: "visible".to_string(),
            execution_id: None,
            event_type: Some("chat_segment".to_string()),
            stopped: false,
            attachments: None,
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
            visibility: m.visibility,
            execution_id: m.execution_id,
            event_type: m.event_type,
            stopped: m.stopped,
            // Parse the stored JSON string into a Value so the API response
            // contains a proper array rather than a raw JSON string.
            attachments: m.attachments.and_then(|s| serde_json::from_str(&s).ok()),
            created_at: m.created_at,
        }
    }
}

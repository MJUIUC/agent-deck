use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoutineExecution {
    pub id: String,
    pub routine_id: String,
    pub thread_id: String,
    pub fired_at: String,
    pub status: String, // "running" | "completed" | "failed"
    pub output_message_id: Option<String>,
    pub error: Option<String>,
    pub completed_at: Option<String>,
}

impl RoutineExecution {
    pub fn new(routine_id: &str, thread_id: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            routine_id: routine_id.to_string(),
            thread_id: thread_id.to_string(),
            fired_at: chrono::Utc::now().to_rfc3339(),
            status: "running".to_string(),
            output_message_id: None,
            error: None,
            completed_at: None,
        }
    }

    pub fn complete(mut self, output_message_id: &str) -> Self {
        self.status = "completed".to_string();
        self.output_message_id = Some(output_message_id.to_string());
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn fail(mut self, error: &str) -> Self {
        self.status = "failed".to_string();
        self.error = Some(error.to_string());
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    pub fn is_complete(&self) -> bool {
        self.status == "completed"
    }

    pub fn is_failed(&self) -> bool {
        self.status == "failed"
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSummary {
    pub id: String,
    pub thread_id: String,
    pub summary: String,
    pub from_message_seq: i64,
    pub to_message_seq: i64,
    pub from_date: String,
    pub to_date: String,
    pub created_at: String,
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Routine {
    pub id: String,
    pub thread_id: String,
    pub name: String,
    pub prompt: String,
    pub cron_expr: String,
    pub timezone: String,
    pub enabled: bool,
    pub run_count: i64,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoutine {
    pub name: String,
    pub prompt: String,
    pub cron_expr: String,
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoutine {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub cron_expr: Option<String>,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
}

impl Routine {
    pub fn new(thread_id: impl Into<String>, req: CreateRoutine) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            name: req.name,
            prompt: req.prompt,
            cron_expr: req.cron_expr,
            timezone: req.timezone.unwrap_or_else(|| "UTC".to_string()),
            enabled: true,
            run_count: 0,
            last_run_at: None,
            next_run_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

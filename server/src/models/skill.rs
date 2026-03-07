use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Skill {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub instructions: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSkill {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub instructions: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSkill {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub enabled: Option<bool>,
}

impl Skill {
    pub fn new(user_id: impl Into<String>, req: CreateSkill) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            name: req.name,
            display_name: req.display_name,
            description: req.description,
            instructions: req.instructions,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Model {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub vision: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateModel {
    pub model_id: String,
    pub display_name: String,
    pub vision: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModel {
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
    pub vision: Option<bool>,
}

impl Model {
    pub fn new(provider_id: impl Into<String>, req: CreateModel) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            provider_id: provider_id.into(),
            model_id: req.model_id,
            display_name: req.display_name,
            enabled: true,
            vision: req.vision.unwrap_or(false),
        }
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub display_name: String,
    pub pronouns: Option<String>,
    pub role: Option<String>,
    pub organization: Option<String>,
    pub location: Option<String>,
    pub timezone: Option<String>,
    pub about: Option<String>,
    pub profile_updated_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub display_name: String,
}

impl User {
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            display_name: display_name.into(),
            pronouns: None,
            role: None,
            organization: None,
            location: None,
            timezone: None,
            about: None,
            profile_updated_at: None,
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceToken {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub platform: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceToken {
    pub token: String,
    #[serde(default = "default_platform")]
    pub platform: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteDeviceToken {
    pub token: String,
}

fn default_platform() -> String {
    "android".to_string()
}

impl DeviceToken {
    pub fn new(user_id: impl Into<String>, req: RegisterDeviceToken) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            token: req.token,
            platform: req.platform,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

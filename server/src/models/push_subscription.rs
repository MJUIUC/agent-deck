use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A browser Web Push subscription row.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PushSubscription {
    pub id: String,
    pub user_id: String,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub user_agent: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Payload for POST /api/push/subscribe.
#[derive(Debug, Deserialize)]
pub struct CreatePushSubscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    /// Optional UA string sent by the client for debugging.
    #[serde(default)]
    pub user_agent: String,
}

/// Payload for DELETE /api/push/subscribe.
#[derive(Debug, Deserialize)]
pub struct DeletePushSubscription {
    pub endpoint: String,
}

impl PushSubscription {
    pub fn new(user_id: impl Into<String>, payload: CreatePushSubscription) -> Self {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            endpoint: payload.endpoint,
            p256dh: payload.p256dh,
            auth: payload.auth,
            user_agent: payload.user_agent,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

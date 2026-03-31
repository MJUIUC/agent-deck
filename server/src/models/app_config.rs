use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppConfig {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

impl AppConfig {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            updated_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

/// Well-known config keys used throughout the application.
pub mod keys {
    /// The auth token used to authenticate API requests from remote clients.
    pub const AUTH_TOKEN: &str = "auth_token";

    /// Whether the first-run setup wizard has been completed.
    pub const SETUP_COMPLETE: &str = "setup_complete";

    /// The FCM server key (if configured via UI rather than env).
    pub const FCM_SERVER_KEY: &str = "fcm_server_key";

    /// The AES-256 master key used to encrypt/decrypt credential secrets.
    /// Stored as a 64-character hex string (32 bytes). Generated once on first
    /// server run and never included in any log output or API response.
    pub const CREDENTIAL_MASTER_KEY: &str = "credential_master_key";
}

#[derive(Debug, Serialize)]
pub struct AppConfigPublic {
    /// Whether setup is complete.
    pub setup_complete: bool,
}

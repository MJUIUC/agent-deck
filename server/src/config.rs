use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub public_dir: String,
    pub fcm_service_account_json: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "7474".to_string())
            .parse::<u16>()
            .unwrap_or(7474);

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:./data/agent-deck.db".to_string());

        let public_dir = std::env::var("PUBLIC_DIR").unwrap_or_else(|_| "./public".to_string());

        let fcm_service_account_json = std::env::var("FCM_SERVICE_ACCOUNT_JSON").ok();

        Ok(Self {
            port,
            database_url,
            public_dir,
            fcm_service_account_json,
        })
    }
}

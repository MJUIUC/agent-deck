use std::path::PathBuf;

/// Application configuration, derived from environment variables and sensible defaults.
///
/// Directory layout (all paths are absolute):
///
/// ```
/// ~/.agent-deck/          ← AGENT_DECK_DATA_DIR or default
///   .process/
///     .database/
///       agent-deck.db     ← SQLite database
///     bin/
///     public/
///     server.log
///     agent-deck.pid
///     run.sh
///     agent-deck-cli.sh
///   mcp/                  ← one subdirectory per MCP server
///   personas/             ← one subdirectory per persona
///   skills/               ← one subdirectory per skill
///   workspaces/           ← one subdirectory per workspace
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    /// Absolute path to the hidden process directory (`data_dir/.process`).
    pub process_dir: PathBuf,
    /// Absolute path to the top-level data directory.
    pub data_dir: PathBuf,
    /// Absolute path to the MCP servers directory (`data_dir/mcp`).
    pub mcp_dir: PathBuf,
    /// Absolute path to the personas directory (`data_dir/personas`).
    pub personas_dir: PathBuf,
    /// Absolute path to the workspaces directory (`data_dir/workspaces`).
    pub workspaces_dir: PathBuf,
    /// Absolute path to the skills directory (`data_dir/skills`).
    pub skills_dir: PathBuf,
    /// SQLite connection URL: `sqlite:<process_dir>/.database/agent-deck.db`
    pub database_url: String,
    /// Directory to serve static frontend assets from.
    pub public_dir: String,
    /// Optional path to the FCM service-account JSON file.
    pub fcm_service_account_json: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "7474".to_string())
            .parse::<u16>()
            .unwrap_or(7474);

        // Resolve the data directory:
        //   1. AGENT_DECK_DATA_DIR env var (must be an absolute path)
        //   2. ~/.agent-deck via dirs::home_dir()
        let data_dir: PathBuf = if let Ok(env_dir) = std::env::var("AGENT_DECK_DATA_DIR") {
            let p = PathBuf::from(env_dir);
            if !p.is_absolute() {
                panic!(
                    "AGENT_DECK_DATA_DIR must be an absolute path, got: {}",
                    p.display()
                );
            }
            p
        } else {
            dirs::home_dir()
                .expect(
                    "Cannot determine home directory and AGENT_DECK_DATA_DIR is not set. \
                     Set AGENT_DECK_DATA_DIR to an absolute path to continue.",
                )
                .join(".agent-deck")
        };

        let process_dir = data_dir.join(".process");
        let mcp_dir = data_dir.join("mcp");
        let personas_dir = data_dir.join("personas");
        let workspaces_dir = data_dir.join("workspaces");
        let skills_dir = data_dir.join("skills");

        let db_path = process_dir.join(".database").join("agent-deck.db");
        let database_url = format!("sqlite:{}", db_path.display());

        let public_dir = std::env::var("PUBLIC_DIR")
            .unwrap_or_else(|_| process_dir.join("public").to_string_lossy().into_owned());

        let fcm_service_account_json = std::env::var("FCM_SERVICE_ACCOUNT_JSON").ok();

        Ok(Self {
            port,
            process_dir,
            data_dir,
            mcp_dir,
            personas_dir,
            workspaces_dir,
            skills_dir,
            database_url,
            public_dir,
            fcm_service_account_json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize all config tests that touch env vars so they don't race each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn config_uses_home_dir_when_no_env_var_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Remove the env var if it happens to be set in the test environment.
        std::env::remove_var("AGENT_DECK_DATA_DIR");

        let config = Config::from_env().expect("from_env should succeed");

        let home = dirs::home_dir().expect("home dir must exist in test environment");
        assert_eq!(config.data_dir, home.join(".agent-deck"));
    }

    #[test]
    fn config_respects_agent_deck_data_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("AGENT_DECK_DATA_DIR", "/tmp/test-agent-deck-config");
        let config = Config::from_env().expect("from_env should succeed");
        std::env::remove_var("AGENT_DECK_DATA_DIR");

        assert_eq!(
            config.data_dir,
            PathBuf::from("/tmp/test-agent-deck-config")
        );
    }

    #[test]
    fn config_derived_paths_are_correct() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("AGENT_DECK_DATA_DIR", "/tmp/test-agent-deck-paths");
        let config = Config::from_env().expect("from_env should succeed");
        std::env::remove_var("AGENT_DECK_DATA_DIR");

        let base = PathBuf::from("/tmp/test-agent-deck-paths");
        assert_eq!(config.process_dir, base.join(".process"));
        assert_eq!(config.mcp_dir, base.join("mcp"));
        assert_eq!(config.personas_dir, base.join("personas"));
        assert_eq!(config.workspaces_dir, base.join("workspaces"));
        assert_eq!(config.skills_dir, base.join("skills"));
        assert_eq!(
            config.database_url,
            format!(
                "sqlite:{}",
                base.join(".process")
                    .join(".database")
                    .join("agent-deck.db")
                    .display()
            )
        );
    }
}

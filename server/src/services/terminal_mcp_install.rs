use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use sqlx::SqlitePool;
use tracing::{error, info, warn};

const TERMINAL_MCP_REPO: &str = "https://github.com/iris-networks/terminal_mcp.git";
const TERMINAL_MCP_HASH: &str = "73f111d580bd5c64a8d8742e4be5bc4698794234";
const TERMINAL_MCP_TAG: &str = "terminalmcp";
const TERMINAL_MCP_NAME: &str = "Terminal";
const TERMINAL_MCP_CONFIG: &str =
    r#"{"executable":"./terminal_mcp/mcp-terminal-server","args":[],"env":{}}"#;

/// Ensures the terminal MCP server is registered in agent-deck.
/// - If no user exists yet (called before setup completes), returns Ok(()) immediately.
/// - If already registered in the DB, skips registration.
/// - Seeds config.json + DB row if missing.
/// - If the binary is absent, spawns an async build task.
///
/// This function is idempotent and safe to call on every startup.
pub async fn ensure_terminal_mcp(mcp_dir: &Path, pool: &SqlitePool) -> Result<()> {
    // If no user exists yet, setup hasn't completed — nothing to do.
    let user_row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(pool)
        .await?;

    let user_id = match user_row {
        Some((id,)) => id,
        None => return Ok(()),
    };

    // Check whether the server is already registered.
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM mcp_servers WHERE tag = ? LIMIT 1")
            .bind(TERMINAL_MCP_TAG)
            .fetch_optional(pool)
            .await?;

    let server_id = match existing {
        Some((id,)) => id,
        None => {
            let new_id = uuid::Uuid::new_v4().to_string();

            // Create the directory that will hold the binary.
            tokio::fs::create_dir_all(mcp_dir.join(TERMINAL_MCP_TAG).join("terminal_mcp"))
                .await
                .map_err(|e| anyhow!("terminal_mcp: failed to create install dir: {}", e))?;

            // Build the config.json payload with a nested config object.
            let inner_config: serde_json::Value = serde_json::from_str(TERMINAL_MCP_CONFIG)
                .map_err(|e| anyhow!("terminal_mcp: failed to parse inner config: {}", e))?;

            let payload = serde_json::json!({
                "id": new_id,
                "name": TERMINAL_MCP_NAME,
                "tag": TERMINAL_MCP_TAG,
                "server_type": "local",
                "config": inner_config,
            });

            let json = serde_json::to_string_pretty(&payload)
                .map_err(|e| anyhow!("terminal_mcp: failed to serialize config.json: {}", e))?;

            let config_path = mcp_dir.join(TERMINAL_MCP_TAG).join("config.json");
            tokio::fs::write(&config_path, json.as_bytes())
                .await
                .map_err(|e| {
                    anyhow!(
                        "terminal_mcp: failed to write config.json at {}: {}",
                        config_path.display(),
                        e
                    )
                })?;

            let now = chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();

            sqlx::query(
                "INSERT OR IGNORE INTO mcp_servers \
                 (id, user_id, name, tag, description, source_url, server_type, config, status, enabled, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&new_id)
            .bind(&user_id)
            .bind(TERMINAL_MCP_NAME)
            .bind(TERMINAL_MCP_TAG)
            .bind("Building terminal MCP binary\u{2026}")
            .bind("https://github.com/iris-networks/terminal_mcp")
            .bind("local")
            .bind(TERMINAL_MCP_CONFIG)
            .bind("inactive")
            .bind(1_i64)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;

            info!("terminal_mcp: registered in DB");
            new_id
        }
    };

    // Determine the binary path.
    let binary_path = mcp_dir
        .join(TERMINAL_MCP_TAG)
        .join("terminal_mcp")
        .join("mcp-terminal-server");

    if binary_path.exists() {
        // Binary is already present — clear any leftover build description.
        set_description(pool, &server_id, "").await;
        info!(
            "terminal_mcp: binary already present at {}",
            binary_path.display()
        );
        return Ok(());
    }

    // Binary is absent — kick off a background build.
    warn!(
        "terminal_mcp: binary not found at {} — starting background build",
        binary_path.display()
    );

    let pool_clone = pool.clone();
    tokio::spawn(async move {
        build_terminal_mcp(binary_path, server_id, pool_clone).await;
    });

    Ok(())
}

async fn build_terminal_mcp(binary_dest: PathBuf, server_id: String, pool: SqlitePool) {
    // Verify git is available.
    let git_available = tokio::process::Command::new("git")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !git_available {
        let msg = git_missing_message();
        set_description(&pool, &server_id, &msg).await;
        error!("terminal_mcp: git not found — {}", msg);
        return;
    }

    // Verify go is available.
    let go_available = tokio::process::Command::new("go")
        .arg("version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !go_available {
        let msg = go_missing_message();
        set_description(&pool, &server_id, &msg).await;
        error!("terminal_mcp: go not found — {}", msg);
        return;
    }

    // Derive the .build directory from the binary path:
    // binary_dest = mcp_dir/terminalmcp/terminal_mcp/mcp-terminal-server
    // parent      = mcp_dir/terminalmcp/terminal_mcp/
    // parent      = mcp_dir/terminalmcp/
    // .build      = mcp_dir/terminalmcp/.build
    let terminalmcp_dir = match binary_dest.parent().and_then(|p| p.parent()) {
        Some(dir) => dir.to_path_buf(),
        None => {
            let msg = "terminal_mcp: could not determine build directory from binary path";
            error!("{}", msg);
            set_description(&pool, &server_id, msg).await;
            return;
        }
    };

    let build_dir = terminalmcp_dir.join(".build");

    // Clone the repository if the build directory doesn't already exist.
    if !build_dir.exists() {
        let build_dir_str = match build_dir.to_str() {
            Some(s) => s.to_string(),
            None => {
                let msg = "terminal_mcp: build directory path contains invalid UTF-8";
                error!("{}", msg);
                set_description(&pool, &server_id, msg).await;
                return;
            }
        };

        info!(
            "terminal_mcp: cloning repository into {}",
            build_dir.display()
        );

        let output = match tokio::process::Command::new("git")
            .args(["clone", "--no-checkout", TERMINAL_MCP_REPO, &build_dir_str])
            .output()
            .await
        {
            Ok(o) => o,
            Err(e) => {
                let msg = format!("terminal_mcp: git clone failed to spawn: {}", e);
                error!("{}", msg);
                set_description(&pool, &server_id, &msg).await;
                return;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = format!("terminal_mcp: git clone failed: {}", stderr);
            error!("{}", msg);
            set_description(&pool, &server_id, &msg).await;
            return;
        }
    }

    // Checkout the pinned hash.
    let output = match tokio::process::Command::new("git")
        .args(["checkout", TERMINAL_MCP_HASH])
        .current_dir(&build_dir)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("terminal_mcp: git checkout failed to spawn: {}", e);
            error!("{}", msg);
            set_description(&pool, &server_id, &msg).await;
            return;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("terminal_mcp: git checkout failed: {}", stderr);
        error!("{}", msg);
        set_description(&pool, &server_id, &msg).await;
        return;
    }

    // Build the binary.
    let binary_dest_str = match binary_dest.to_str() {
        Some(s) => s.to_string(),
        None => {
            let msg = "terminal_mcp: binary destination path contains invalid UTF-8";
            error!("{}", msg);
            set_description(&pool, &server_id, msg).await;
            return;
        }
    };

    info!("terminal_mcp: building binary (this may take a moment)...");

    let output = match tokio::process::Command::new("go")
        .args(["build", "-o", &binary_dest_str, "."])
        .current_dir(&build_dir)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("terminal_mcp: go build failed to spawn: {}", e);
            error!("{}", msg);
            set_description(&pool, &server_id, &msg).await;
            return;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("terminal_mcp: go build failed: {}", stderr);
        error!("{}", msg);
        set_description(&pool, &server_id, &msg).await;
        return;
    }

    // Make the binary executable.
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) =
        tokio::fs::set_permissions(&binary_dest, std::fs::Permissions::from_mode(0o755)).await
    {
        warn!(
            "terminal_mcp: failed to set executable permissions on {}: {}",
            binary_dest.display(),
            e
        );
    }

    // Clear the build description now that the binary is ready.
    set_description(&pool, &server_id, "").await;
    info!(
        "terminal_mcp: binary built and ready at {}",
        binary_dest.display()
    );
}

fn git_missing_message() -> String {
    let url = "https://git-scm.com/downloads";
    match std::env::consts::OS {
        "macos" => format!(
            "git is required but was not found. Install with: brew install git  OR download from {}",
            url
        ),
        "linux" => format!(
            "git is required but was not found. Install with: sudo apt install git  OR download from {}",
            url
        ),
        _ => format!("git is required but was not found. Download from: {}", url),
    }
}

fn go_missing_message() -> String {
    let url = "https://go.dev/dl/";
    match std::env::consts::OS {
        "macos" => format!(
            "Go is required to build the terminal MCP server but was not found. Install with: brew install go  OR download from {}",
            url
        ),
        "linux" => format!(
            "Go is required to build the terminal MCP server but was not found. Install with: sudo apt install golang-go  OR download from {}",
            url
        ),
        _ => format!(
            "Go is required to build the terminal MCP server but was not found. Download from: {}",
            url
        ),
    }
}

async fn set_description(pool: &SqlitePool, server_id: &str, description: &str) {
    let desc_opt: Option<&str> = if description.is_empty() {
        None
    } else {
        Some(description)
    };

    if let Err(e) = sqlx::query("UPDATE mcp_servers SET description = ? WHERE id = ?")
        .bind(desc_opt)
        .bind(server_id)
        .execute(pool)
        .await
    {
        warn!("terminal_mcp: failed to update description: {}", e);
    }
}

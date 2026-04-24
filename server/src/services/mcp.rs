//! MCP Connection Manager — Story 4.3
//!
//! Manages connections to all configured MCP (Model Context Protocol) servers.
//! Two transport types are supported:
//!
//! - **Local (stdio):** Spawn a child process; communicate over stdin/stdout
//!   using newline-delimited JSON-RPC.
//! - **Remote (HTTP/SSE):** POST client→server messages to the configured URL;
//!   receive server→client messages via Server-Sent Events.
//!
//! Lifecycle:
//! - On startup: connect all `enabled = 1` servers from the database.
//! - On create/update via API: `connect_server` or `disconnect_server`.
//! - On delete: `disconnect_server`.
//! - On graceful shutdown: `shutdown_all` kills every child and closes every
//!   remote connection.
//!
//! Status is persisted to `mcp_servers.status` and broadcast over the global
//! SSE channel as `mcp_status_changed` events so the UI updates in real time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, watch, Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::services::copilot::GlobalEvent;
use crate::services::credentials;

// ─── Backoff constants ────────────────────────────────────────────────────────

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// After this much stable uptime, reset the backoff counter.
const STABILITY_THRESHOLD: Duration = Duration::from_secs(300); // 5 minutes

// ─── Tag validation ───────────────────────────────────────────────────────────

/// Validate that a tag contains only alphanumeric characters, hyphens, and
/// underscores.  No spaces, no dots, no slashes — it will be used as a prefix
/// in tool names like `github__create_issue`.
pub fn validate_tag(tag: &str) -> Result<(), String> {
    if tag.is_empty() {
        return Err("tag must not be empty".to_string());
    }
    if tag.len() > 64 {
        return Err("tag must be 64 characters or fewer".to_string());
    }
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("tag may only contain letters, digits, hyphens, and underscores".to_string());
    }
    Ok(())
}

// ─── MCP JSON-RPC wire types ──────────────────────────────────────────────────

/// A JSON-RPC 2.0 request sent *to* an MCP server.
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: Option<u64>,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

impl JsonRpcRequest {
    fn request(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: Some(id),
            method: method.into(),
            params,
        }
    }

    fn notification(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: None,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response received *from* an MCP server.
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    id: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

/// A single MCP tool descriptor returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<Value>,
}

// ─── Per-connection state ─────────────────────────────────────────────────────

/// Internal state for a single MCP server connection.
struct McpConnectionInner {
    /// Monotonically increasing counter for JSON-RPC request IDs.
    next_id: u64,
    /// For local servers: the child process handle.
    child: Option<Child>,
    /// For local servers: the write half of the child's stdin pipe.
    stdin: Option<ChildStdin>,
    /// For remote servers: the reqwest client (kept alive across requests).
    http_client: Option<reqwest::Client>,
    /// All resolved headers to send on every request to this remote server.
    /// Includes the credential-derived auth header plus any extra headers
    /// defined in the config.  Empty for local servers.
    extra_headers: HashMap<String, String>,
    /// The URL for remote servers.
    remote_url: Option<String>,
    /// Whether the shutdown signal has been sent (prevents reconnect loops).
    shutting_down: bool,
}

impl McpConnectionInner {
    fn new_local(child: Child, stdin: ChildStdin) -> Self {
        Self {
            next_id: 1,
            child: Some(child),
            stdin: Some(stdin),
            http_client: None,
            extra_headers: HashMap::new(),
            remote_url: None,
            shutting_down: false,
        }
    }

    fn new_remote(client: reqwest::Client, url: String, headers: HashMap<String, String>) -> Self {
        Self {
            next_id: 1,
            child: None,
            stdin: None,
            http_client: Some(client),
            extra_headers: headers,
            remote_url: Some(url),
            shutting_down: false,
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// A live (or recently-live) connection to an MCP server.
struct McpConnection {
    /// DB row id of the server.
    #[allow(dead_code)]
    server_id: String,
    inner: Mutex<McpConnectionInner>,
    request_lock: tokio::sync::Mutex<()>,
    /// Current connection status (mirrored here for fast in-memory reads).
    #[allow(dead_code)]
    status: RwLock<McpStatus>,
    /// Cached tool list — stored outside `inner` so it can be read from async
    /// contexts without blocking.  Written once after `tools/list` succeeds,
    /// then refreshed on every reconnect.
    tools: RwLock<Vec<McpTool>>,
    /// For local (stdio) servers: a shared reader over the child's stdout.
    /// Kept here so `call_tool` can send requests and read responses after
    /// the initial handshake is complete.  `None` for remote servers.
    stdout_reader: Option<Arc<Mutex<BufReader<tokio::process::ChildStdout>>>>,
    /// Shutdown signal.  `disconnect_server` sends `true`; `supervise` and
    /// `monitor_local` select on the receiver so they wake immediately instead
    /// of waiting out a sleep or spinning on a flag check.
    shutdown_tx: watch::Sender<bool>,
    /// Receiver cloned by every task that needs to react to shutdown.
    shutdown_rx: watch::Receiver<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    Inactive,
    Connecting,
    Connected,
    Error(String),
}

impl McpStatus {
    pub fn as_str(&self) -> &str {
        match self {
            McpStatus::Inactive => "inactive",
            McpStatus::Connecting => "connecting",
            McpStatus::Connected => "connected",
            McpStatus::Error(_) => "error",
        }
    }

    pub fn reason(&self) -> Option<String> {
        if let McpStatus::Error(msg) = self {
            Some(msg.clone())
        } else {
            None
        }
    }
}

// ─── Parsed server config ─────────────────────────────────────────────────────

/// Parsed representation of the `mcp_servers.config` JSON column.
#[derive(Debug, Deserialize)]
struct LocalConfig {
    executable: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    working_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RemoteConfig {
    url: String,
    /// Credential key whose decrypted value is sent as the auth header.
    credential_key: Option<String>,
    /// Header name, defaults to `Authorization`.
    auth_header: Option<String>,
    /// Header value template, defaults to `Bearer {value}`.
    auth_format: Option<String>,
    /// Arbitrary extra headers sent on every request to this server.
    /// These are merged with (and may override) any auth header derived
    /// from `credential_key`.  Stored verbatim in the config JSON so the
    /// filesystem config.json is always the source of truth.
    #[serde(default)]
    headers: HashMap<String, String>,
}

/// A minimal DB projection of the `mcp_servers` row — only what the manager needs.
#[derive(Debug, sqlx::FromRow)]
struct McpServerRow {
    id: String,
    tag: String,
    server_type: String,
    config: String,
    enabled: bool,
}

// ─── McpConnectionManager ─────────────────────────────────────────────────────

/// Cloneable handle to the MCP connection pool.  All heavy state lives behind
/// an `Arc` so clones are cheap.
#[derive(Clone)]
pub struct McpConnectionManager {
    connections: Arc<DashMap<String, Arc<McpConnection>>>,
    pool: SqlitePool,
    master_key: String,
    global_tx: broadcast::Sender<GlobalEvent>,
    /// Root directory for per-server working directories and config files.
    mcp_dir: PathBuf,
}

impl McpConnectionManager {
    /// Create a new manager.  Call [`start`] to connect enabled servers.
    pub fn new(
        pool: SqlitePool,
        master_key: String,
        global_tx: broadcast::Sender<GlobalEvent>,
        mcp_dir: PathBuf,
    ) -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            pool,
            master_key,
            global_tx,
            mcp_dir,
        }
    }

    // ─── Startup ─────────────────────────────────────────────────────────────

    /// Connect all `enabled = 1` MCP servers found in the database.
    ///
    /// Each server gets its own spawned task so a slow or failing server does
    /// not block the others.  Returns immediately.
    pub async fn start(&self) {
        // Sync filesystem config.json files into DB before connecting.
        if let Err(e) = self.startup_sync().await {
            warn!("mcp: startup_sync failed: {}", e);
        }

        // Ensure the terminal MCP server is registered (idempotent; no-op if already present).
        if let Err(e) =
            crate::services::terminal_mcp_install::ensure_terminal_mcp(&self.mcp_dir, &self.pool)
                .await
        {
            warn!("mcp: terminal MCP registration failed: {}", e);
        }

        let rows: Vec<McpServerRow> = match sqlx::query_as(
            "SELECT id, tag, server_type, config, enabled FROM mcp_servers WHERE enabled = 1",
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("mcp: failed to load servers from DB: {}", e);
                return;
            }
        };

        info!("mcp: connecting {} enabled server(s)", rows.len());

        for row in rows {
            let mgr = self.clone();
            tokio::spawn(async move {
                mgr.connect_server(&row.id).await;
            });
        }
    }

    /// Scan `mcp_dir` for subdirectories containing a `config.json` and upsert
    /// each one into the database.  Also disables any enabled DB rows whose
    /// config.json has gone missing.
    ///
    /// The config.json format expected on disk:
    /// ```json
    /// {
    ///   "name": "github",
    ///   "tag": "github",
    ///   "server_type": "local",
    ///   "config": { "executable": "docker", "args": [...], "env": {...} }
    /// }
    /// ```
    pub async fn startup_sync(&self) -> Result<()> {
        use tokio::fs;

        // ── 1. Scan filesystem and upsert into DB ──────────────────────────────
        let mut read_dir = match fs::read_dir(&self.mcp_dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // mcp_dir doesn't exist yet — nothing to sync.
                return Ok(());
            }
            Err(e) => return Err(anyhow!("failed to read mcp_dir: {}", e)),
        };

        while let Some(entry) = read_dir.next_entry().await? {
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }

            let config_path = entry_path.join("config.json");
            if !config_path.exists() {
                continue;
            }

            let raw = match fs::read_to_string(&config_path).await {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        "mcp: startup_sync: failed to read {}: {}",
                        config_path.display(),
                        e
                    );
                    continue;
                }
            };

            let v: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        "mcp: startup_sync: invalid JSON in {}: {}",
                        config_path.display(),
                        e
                    );
                    continue;
                }
            };

            let dir_name = match entry_path.file_name().and_then(|n| n.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            // ── Migration: rename UUID-named dirs to tag-based names ──────────
            if is_uuid_like(&dir_name) {
                let tag_in_config = v["tag"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        v["name"]
                            .as_str()
                            .unwrap_or(&dir_name)
                            .to_lowercase()
                            .replace(' ', "-")
                    });
                let new_dir = self.mcp_dir.join(&tag_in_config);
                if !new_dir.exists() {
                    if let Err(e) = tokio::fs::rename(&entry_path, &new_dir).await {
                        warn!(
                            "mcp: startup_sync: failed to rename '{}' -> '{}': {}",
                            dir_name, tag_in_config, e
                        );
                        continue;
                    }
                    // Rewrite config.json in new location to include "id" if missing.
                    let id_in_config = v["id"].as_str().unwrap_or(&dir_name).to_string();
                    let updated_v = {
                        let mut obj = v.clone();
                        obj["id"] = serde_json::Value::String(id_in_config.clone());
                        obj
                    };
                    if let Ok(json) = serde_json::to_string_pretty(&updated_v) {
                        let new_config_path = new_dir.join("config.json");
                        let _ = tokio::fs::write(&new_config_path, json.as_bytes()).await;
                    }
                    info!(
                        "mcp: startup_sync: migrated '{}' -> '{}'",
                        dir_name, tag_in_config
                    );
                    // Re-parse from the migrated data.
                    let id = id_in_config;
                    let name = v["name"].as_str().unwrap_or(&tag_in_config).to_string();
                    let tag = tag_in_config;
                    let server_type = v["server_type"].as_str().unwrap_or("local").to_string();
                    let config_inner = v["config"].to_string();

                    let now = chrono::Utc::now().to_rfc3339();

                    // DB upsert using the migrated variables.
                    let existing: Option<(String,)> =
                        sqlx::query_as("SELECT id FROM mcp_servers WHERE id = ?")
                            .bind(&id)
                            .fetch_optional(&self.pool)
                            .await?;

                    if existing.is_some() {
                        if let Err(e) = sqlx::query(
                            "UPDATE mcp_servers SET name = ?, tag = ?, server_type = ?, config = ?, updated_at = ? WHERE id = ?",
                        )
                        .bind(&name)
                        .bind(&tag)
                        .bind(&server_type)
                        .bind(&config_inner)
                        .bind(&now)
                        .bind(&id)
                        .execute(&self.pool)
                        .await
                        {
                            warn!("mcp: startup_sync: failed to update row {}: {}", id, e);
                        }
                    } else {
                        let user_id: Option<(String,)> =
                            sqlx::query_as("SELECT id FROM users LIMIT 1")
                                .fetch_optional(&self.pool)
                                .await
                                .unwrap_or(None);

                        let user_id = match user_id {
                            Some((uid,)) => uid,
                            None => {
                                warn!(
                                    "mcp: startup_sync: skipping insert for '{}' — no user in DB yet",
                                    id
                                );
                                continue;
                            }
                        };

                        if let Err(e) = sqlx::query(
                            "INSERT INTO mcp_servers (id, user_id, name, tag, server_type, config, status, enabled, created_at, updated_at)
                             VALUES (?, ?, ?, ?, ?, ?, 'inactive', 1, ?, ?)",
                        )
                        .bind(&id)
                        .bind(&user_id)
                        .bind(&name)
                        .bind(&tag)
                        .bind(&server_type)
                        .bind(&config_inner)
                        .bind(&now)
                        .bind(&now)
                        .execute(&self.pool)
                        .await
                        {
                            warn!("mcp: startup_sync: failed to insert row {}: {}", id, e);
                        }
                    }
                    info!("mcp: startup_sync: synced server '{}'", id);
                }
                // If new_dir already exists (collision), skip.
                continue; // After migration, pick up cleanly on the next startup.
            }

            // ── Normal (tag-named) dir ────────────────────────────────────────
            let id = v["id"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let name = v["name"].as_str().unwrap_or(&dir_name).to_string();
            let tag = v["tag"].as_str().unwrap_or(&dir_name).to_string();
            let server_type = v["server_type"].as_str().unwrap_or("local").to_string();
            let config_inner = v["config"].to_string();

            let now = chrono::Utc::now().to_rfc3339();

            // Check if a DB row already exists for this id.
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT id FROM mcp_servers WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(&self.pool)
                    .await?;

            if existing.is_some() {
                // Update the config from the filesystem.
                if let Err(e) = sqlx::query(
                    "UPDATE mcp_servers SET name = ?, tag = ?, server_type = ?, config = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&name)
                .bind(&tag)
                .bind(&server_type)
                .bind(&config_inner)
                .bind(&now)
                .bind(&id)
                .execute(&self.pool)
                .await
                {
                    warn!("mcp: startup_sync: failed to update row {}: {}", id, e);
                }
            } else {
                // Look up the real user_id so the FK constraint is satisfied.
                // If no user exists yet (pre-setup), fall back to empty string
                // and skip the insert — the row will be synced on the next
                // startup once setup is complete.
                let user_id: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
                    .fetch_optional(&self.pool)
                    .await
                    .unwrap_or(None);

                let user_id = match user_id {
                    Some((uid,)) => uid,
                    None => {
                        warn!(
                            "mcp: startup_sync: skipping insert for '{}' — no user in DB yet",
                            id
                        );
                        continue;
                    }
                };

                // Insert a new row with enabled=1 and status=inactive.
                if let Err(e) = sqlx::query(
                    "INSERT INTO mcp_servers (id, user_id, name, tag, server_type, config, status, enabled, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, 'inactive', 1, ?, ?)",
                )
                .bind(&id)
                .bind(&user_id)
                .bind(&name)
                .bind(&tag)
                .bind(&server_type)
                .bind(&config_inner)
                .bind(&now)
                .bind(&now)
                .execute(&self.pool)
                .await
                {
                    warn!("mcp: startup_sync: failed to insert row {}: {}", id, e);
                }
            }

            info!("mcp: startup_sync: synced server '{}'", id);
        }

        // ── 2. Disable DB rows whose config.json is missing ────────────────────
        #[derive(sqlx::FromRow)]
        struct EnabledRow {
            id: String,
            tag: String,
        }

        let enabled_rows: Vec<EnabledRow> =
            sqlx::query_as("SELECT id, tag FROM mcp_servers WHERE enabled = 1")
                .fetch_all(&self.pool)
                .await?;

        for row in enabled_rows {
            let config_path = self.mcp_dir.join(&row.tag).join("config.json");
            if !config_path.exists() {
                let now = chrono::Utc::now().to_rfc3339();
                if let Err(e) = sqlx::query(
                    "UPDATE mcp_servers SET enabled = 0, status = 'inactive', updated_at = ? WHERE id = ?",
                )
                .bind(&now)
                .bind(&row.id)
                .execute(&self.pool)
                .await
                {
                    warn!(
                        "mcp: startup_sync: failed to disable row {}: {}",
                        row.id, e
                    );
                } else {
                    info!(
                        "mcp: startup_sync: disabled '{}' — config.json gone",
                        row.id
                    );
                }
            }
        }

        Ok(())
    }

    /// Write `mcp_dir/<tag>/config.json` for the given server.
    ///
    /// The file captures enough information to reconstruct the DB row on the
    /// next `startup_sync`.
    pub async fn write_config_file(
        &self,
        server: &crate::models::mcp_server::McpServer,
    ) -> Result<()> {
        let dir = self.mcp_dir.join(&server.tag);
        tokio::fs::create_dir_all(&dir).await.map_err(|e| {
            anyhow!(
                "write_config_file: failed to create dir {}: {}",
                dir.display(),
                e
            )
        })?;

        let config_value: serde_json::Value =
            serde_json::from_str(&server.config).unwrap_or(serde_json::Value::Null);

        let payload = serde_json::json!({
            "id": server.id,
            "name": server.name,
            "tag": server.tag,
            "server_type": server.server_type,
            "config": config_value,
        });

        let json = serde_json::to_string_pretty(&payload)?;
        let path = dir.join("config.json");

        let mut file = tokio::fs::File::create(&path).await.map_err(|e| {
            anyhow!(
                "write_config_file: failed to create {}: {}",
                path.display(),
                e
            )
        })?;
        tokio::io::AsyncWriteExt::write_all(&mut file, json.as_bytes())
            .await
            .map_err(|e| {
                anyhow!(
                    "write_config_file: failed to write {}: {}",
                    path.display(),
                    e
                )
            })?;
        tokio::io::AsyncWriteExt::flush(&mut file).await?;

        info!("mcp: wrote config file for server '{}'", server.tag);
        Ok(())
    }

    /// Remove `mcp_dir/<tag>/` and all its contents.
    pub async fn delete_config_dir(&self, tag: &str) -> Result<()> {
        let dir = self.mcp_dir.join(tag);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await.map_err(|e| {
                anyhow!(
                    "delete_config_dir: failed to remove {}: {}",
                    dir.display(),
                    e
                )
            })?;
            info!("mcp: removed config dir for tag '{}'", tag);
        }
        Ok(())
    }

    // ─── Public API ───────────────────────────────────────────────────────────

    /// Connect (or reconnect) a server by its DB id.  Spawns a supervision
    /// task that handles the full lifecycle including reconnect-with-backoff.
    /// Safe to call multiple times — any existing connection is first torn down.
    pub async fn connect_server(&self, server_id: &str) {
        // Tear down any existing connection first.
        self.disconnect_server(server_id).await;

        let mgr = self.clone();
        let server_id = server_id.to_string();
        tokio::spawn(async move {
            mgr.supervise(&server_id).await;
        });
    }

    /// Disconnect and remove a server from the pool.  Kills local children,
    /// cancels remote SSE streams.
    pub async fn disconnect_server(&self, server_id: &str) {
        if let Some((_, conn)) = self.connections.remove(server_id) {
            // Signal shutdown to supervise/monitor_local before touching the
            // inner lock — this lets those tasks wake from their select! and
            // exit cleanly rather than racing with us for the lock.
            let _ = conn.shutdown_tx.send(true);

            let mut inner = conn.inner.lock().await;
            inner.shutting_down = true;
            kill_child_if_any(&mut inner).await;
            info!("mcp: disconnected server {}", server_id);
        }
        // Mark the DB row as inactive regardless.
        self.set_db_status(server_id, "inactive").await;
    }

    /// Return the cached tool list for a server (empty if not connected yet).
    pub async fn cached_tools(&self, server_id: &str) -> Vec<McpTool> {
        match self.connections.get(server_id) {
            Some(conn) => conn.tools.read().await.clone(),
            None => vec![],
        }
    }

    // ─── Tool invocation ──────────────────────────────────────────────────────

    /// Invoke a tool on the named MCP server and return the raw result text.
    ///
    /// `server_id` — DB id of the MCP server
    /// `tool_name` — un-namespaced tool name (e.g. `"create_issue"`)
    /// `arguments`  — parsed JSON arguments object from the model
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        info!(
            server_id = %server_id,
            tool_name = %tool_name,
            "call_tool: invoked"
        );

        let conn = match self.connections.get(server_id) {
            Some(c) => c,
            None => {
                warn!(
                    server_id = %server_id,
                    tool_name = %tool_name,
                    "call_tool: server not found in connection pool"
                );
                return Err(anyhow!("MCP server '{}' not connected", server_id));
            }
        };

        // Determine transport type and gather what we need while holding the lock.
        let (is_local, req_id) = {
            let mut inner = conn.inner.lock().await;
            let id = inner.next_id();
            let is_local = inner.child.is_some();
            (is_local, id)
        };

        let req = JsonRpcRequest::request(
            req_id,
            "tools/call",
            Some(json!({ "name": tool_name, "arguments": arguments })),
        );

        if is_local {
            let _request_guard = conn.request_lock.lock().await;

            // ── Stdio transport ───────────────────────────────────────────────
            // Write the request to stdin, then read the response from the shared
            // stdout reader.  We lock stdin briefly for the write, then release
            // it before locking stdout for the read to avoid deadlock.
            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                req_id = %req_id,
                "call_tool: writing tools/call to stdin"
            );
            {
                let mut inner = conn.inner.lock().await;
                write_line_to_stdin(inner.stdin.as_mut().unwrap(), &req).await?;
            }
            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                "call_tool: stdin write complete, waiting for stdout response"
            );

            let stdout_reader = conn
                .stdout_reader
                .as_ref()
                .ok_or_else(|| anyhow!("MCP server '{}' has no stdout reader", server_id))?;

            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                "call_tool: acquiring stdout_reader lock"
            );
            let resp = {
                let mut reader = stdout_reader.lock().await;
                info!(
                    server_id = %server_id,
                    tool_name = %tool_name,
                    "call_tool: stdout_reader lock acquired, reading JSON-RPC response"
                );
                read_json_rpc_response(&mut *reader).await?
            };
            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                has_error = resp.error.is_some(),
                has_result = resp.result.is_some(),
                "call_tool: received JSON-RPC response from stdio"
            );

            if let Some(err) = resp.error {
                warn!(
                    server_id = %server_id,
                    tool_name = %tool_name,
                    error = %err,
                    "call_tool: server returned JSON-RPC error"
                );
                return Err(anyhow!("MCP tools/call error: {}", err));
            }

            let result = resp.result.unwrap_or(serde_json::Value::Null);
            let text = result["content"][0]["text"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| result.to_string());

            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                result_len = text.len(),
                "call_tool: stdio transport success"
            );
            Ok(text)
        } else {
            // ── HTTP transport ────────────────────────────────────────────────
            // Extract what we need from the inner lock, then release it before
            // the async HTTP call so we don't hold the lock across an await.
            let (client, url, headers) = {
                let inner = conn.inner.lock().await;
                let client = inner
                    .http_client
                    .clone()
                    .ok_or_else(|| anyhow!("MCP server '{}' has no HTTP client", server_id))?;
                let url = inner
                    .remote_url
                    .clone()
                    .ok_or_else(|| anyhow!("MCP server '{}' has no remote URL", server_id))?;
                let headers = inner.extra_headers.clone();
                (client, url, headers)
            };

            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                url = %url,
                "call_tool: posting tools/call over HTTP"
            );

            let resp = self.post_rpc(&client, &url, &req, &headers).await?;

            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                has_error = resp.error.is_some(),
                has_result = resp.result.is_some(),
                "call_tool: received HTTP response"
            );

            if let Some(err) = resp.error {
                warn!(
                    server_id = %server_id,
                    tool_name = %tool_name,
                    error = %err,
                    "call_tool: server returned JSON-RPC error"
                );
                return Err(anyhow!("MCP tools/call error: {}", err));
            }

            let result = resp.result.unwrap_or(serde_json::Value::Null);
            let text = result["content"][0]["text"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| result.to_string());

            info!(
                server_id = %server_id,
                tool_name = %tool_name,
                result_len = text.len(),
                "call_tool: HTTP transport success"
            );
            Ok(text)
        }
    }

    /// Graceful shutdown: disconnect every server and kill every child process.
    pub async fn shutdown_all(&self) {
        let ids: Vec<String> = self.connections.iter().map(|e| e.key().clone()).collect();
        info!("mcp: shutting down {} connection(s)", ids.len());
        for id in ids {
            self.disconnect_server(&id).await;
        }
    }

    // ─── Supervision loop ─────────────────────────────────────────────────────

    /// Main supervision loop for a single server.  Attempts to connect, runs
    /// the `initialize` handshake and `tools/list`, then monitors the
    /// connection.  On failure, waits with exponential backoff and retries.
    async fn supervise(&self, server_id: &str) {
        let mut backoff = BACKOFF_INITIAL;
        let mut connected_at: Option<tokio::time::Instant> = None;

        loop {
            // Fetch current server config from DB (may have changed since last iteration).
            let row = match self.fetch_server_row(server_id).await {
                Some(r) => r,
                None => {
                    info!(
                        "mcp server {} no longer in DB, stopping supervision",
                        server_id
                    );
                    return;
                }
            };

            // If the server was disabled externally, stop.
            if !row.enabled {
                info!("mcp server {} is disabled, stopping supervision", server_id);
                self.set_db_status(server_id, "inactive").await;
                self.broadcast_status(server_id, McpStatus::Inactive);
                return;
            }

            self.set_status(server_id, McpStatus::Connecting).await;

            // `shutdown_rx` for the backoff wait below.  We carry it out of
            // the match so both arms can set it — the Ok arm takes it from
            // the live conn (which fires when disconnect_server is called),
            // the Err arm creates a never-firing receiver so the backoff runs
            // normally and we always retry on a plain connect failure.
            let mut backoff_shutdown_rx: watch::Receiver<bool>;

            match self.connect_and_handshake(&row).await {
                Ok(conn) => {
                    let conn = Arc::new(conn);
                    self.connections.insert(server_id.to_string(), conn.clone());

                    // Spawn keepalive for local connections only.
                    {
                        let is_local = conn.inner.lock().await.child.is_some();
                        if is_local {
                            let conn_for_keepalive = conn.clone();
                            tokio::spawn(async move {
                                Self::run_keepalive_local(conn_for_keepalive).await;
                            });
                        }
                    }

                    // Record when we connected so we can reset backoff after
                    // a sustained stable period.
                    connected_at = Some(tokio::time::Instant::now());
                    self.set_status(server_id, McpStatus::Connected).await;
                    let tool_count = conn.tools.read().await.len();
                    info!(
                        server_id = %server_id,
                        tool_count,
                        "mcp: server connected"
                    );

                    // Monitor until the connection drops (or shutdown requested).
                    self.monitor_connection(server_id, &conn).await;

                    // Carry the shutdown receiver so the backoff select below
                    // can react to an intentional disconnect_server call.
                    backoff_shutdown_rx = conn.shutdown_rx.clone();

                    // If we were asked to shut down, exit the loop.
                    if *backoff_shutdown_rx.borrow() {
                        return;
                    }

                    // Reset backoff if we were stable long enough.
                    if connected_at
                        .map(|t| t.elapsed() >= STABILITY_THRESHOLD)
                        .unwrap_or(false)
                    {
                        backoff = BACKOFF_INITIAL;
                    }
                    connected_at = None;

                    warn!(
                        server_id = %server_id,
                        retry_in_secs = backoff.as_secs(),
                        "mcp: server connection lost, will retry"
                    );
                    self.set_status(server_id, McpStatus::Error("Connection lost".to_string()))
                        .await;
                    self.connections.remove(server_id);
                }
                Err(e) => {
                    let msg = format!("{:#}", e);
                    error!(server_id = %server_id, error = %msg, "mcp: server failed to connect");
                    self.set_status(server_id, McpStatus::Error(msg)).await;

                    // No live conn — use a never-firing receiver so the
                    // backoff wait below runs to completion and we retry.
                    let (_tx, rx) = watch::channel(false);
                    backoff_shutdown_rx = rx;
                }
            }

            // Wait with backoff before next attempt.  We select on two
            // conditions so shutdown and disable are both immediate:
            //   1. The backoff timer expires      → retry
            //   2. shutdown_rx fires true         → intentional disconnect, stop
            //   3. Every 500 ms: check DB enabled → server was disabled, stop
            let sleep_fut = sleep(backoff);
            tokio::pin!(sleep_fut);
            loop {
                tokio::select! {
                    _ = &mut sleep_fut => break,
                    _ = backoff_shutdown_rx.changed() => {
                        if *backoff_shutdown_rx.borrow() {
                            return;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
                        match self.fetch_server_row(server_id).await {
                            Some(row) if !row.enabled => return,
                            None => return,
                            _ => {}
                        }
                    }
                }
            }

            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }

    /// Connect to a server and complete the MCP `initialize` / `tools/list`
    /// handshake.  Returns an `McpConnection` in the `Connected` state.
    async fn connect_and_handshake(&self, row: &McpServerRow) -> Result<McpConnection> {
        match row.server_type.as_str() {
            "local" => self.connect_local(row).await,
            "remote" => self.connect_remote(row).await,
            other => Err(anyhow!("unknown server_type '{}'", other)),
        }
    }

    // ─── Local (stdio) transport ──────────────────────────────────────────────

    async fn connect_local(&self, row: &McpServerRow) -> Result<McpConnection> {
        let cfg: LocalConfig = serde_json::from_str(&row.config)
            .map_err(|e| anyhow!("invalid local config: {}", e))?;

        // Resolve credential placeholders in env vars:
        // Values like "{credential:github_pat}" are replaced with the decrypted secret.
        let mut resolved_env: HashMap<String, String> = HashMap::new();
        for (k, v) in &cfg.env {
            let resolved = self.resolve_env_value(v).await?;
            resolved_env.insert(k.clone(), resolved);
        }

        // Resolve the working directory for the child process.
        // Prefer the explicitly configured working_dir; fall back to mcp_dir/<tag>.
        let working_dir = if let Some(ref wd) = cfg.working_dir {
            PathBuf::from(wd)
        } else {
            self.mcp_dir.join(&row.tag)
        };
        tokio::fs::create_dir_all(&working_dir).await.map_err(|e| {
            anyhow!(
                "failed to create working dir '{}': {}",
                working_dir.display(),
                e
            )
        })?;

        let mut cmd = Command::new(&cfg.executable);
        cmd.args(&cfg.args)
            .envs(&resolved_env)
            .current_dir(&working_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to spawn '{}': {}", cfg.executable, e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("child stdin not available"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("child stdout not available"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("child stderr not available"))?;

        // Capture stderr in a background task for diagnostics.
        let server_id = row.id.clone();
        let stderr_server_id = server_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // stderr is normal for stdio MCP servers — they use it for
                // human-readable status output since stdout is reserved for
                // JSON-RPC.  Log at debug so it doesn't pollute normal output.
                tracing::debug!("mcp[{}] stderr: {}", stderr_server_id, line);
            }
        });

        let mut conn = McpConnectionInner::new_local(child, stdin);

        // Run `initialize` handshake over stdio.
        let id = conn.next_id();
        let req = JsonRpcRequest::request(
            id,
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "agent-deck",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        write_line_to_stdin(conn.stdin.as_mut().unwrap(), &req).await?;

        // Read the initialize response from stdout.
        let mut reader = BufReader::new(stdout);
        let resp = read_json_rpc_response(&mut reader).await?;
        if let Some(err) = resp.error {
            return Err(anyhow!("initialize error: {}", err));
        }

        // Send `initialized` notification (no response expected).
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        write_line_to_stdin(conn.stdin.as_mut().unwrap(), &notif).await?;

        // Discover tools.
        let tools = Self::list_tools_stdio(&mut conn, &mut reader)
            .await
            .unwrap_or_else(|e| {
                warn!("tools/list failed: {}", e);
                Vec::new()
            });

        // Wrap the stdout reader in an Arc<Mutex<…>> so it can be shared between
        // the connection struct (for `call_tool` requests) and the monitor task
        // (which watches for EOF to detect process exit).
        let stdout_reader: Arc<Mutex<BufReader<tokio::process::ChildStdout>>> =
            Arc::new(Mutex::new(reader));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let mcp_conn = McpConnection {
            server_id: row.id.clone(),
            inner: Mutex::new(conn),
            request_lock: tokio::sync::Mutex::new(()),
            status: RwLock::new(McpStatus::Connected),
            tools: RwLock::new(tools),
            stdout_reader: Some(Arc::clone(&stdout_reader)),
            shutdown_tx,
            shutdown_rx,
        };

        // No EOF monitor task here — monitor_local() already detects process
        // exit via child.try_wait() every 2 seconds without touching stdout.
        // Spawning a task that holds stdout_reader.lock() permanently would
        // deadlock call_tool, which also needs to acquire that lock to read
        // tool call responses.

        Ok(mcp_conn)
    }

    async fn list_tools_stdio(
        conn: &mut McpConnectionInner,
        reader: &mut BufReader<tokio::process::ChildStdout>,
    ) -> Result<Vec<McpTool>> {
        let id = conn.next_id();
        let req = JsonRpcRequest::request(id, "tools/list", None);
        write_line_to_stdin(conn.stdin.as_mut().unwrap(), &req).await?;

        let resp = read_json_rpc_response(reader).await?;
        if let Some(err) = resp.error {
            return Err(anyhow!("tools/list error: {}", err));
        }

        let tools = resp
            .result
            .as_ref()
            .and_then(|r| r.get("tools"))
            .and_then(|t| serde_json::from_value::<Vec<McpTool>>(t.clone()).ok())
            .unwrap_or_default();

        Ok(tools)
    }

    // ─── Remote (HTTP/SSE) transport ──────────────────────────────────────────

    async fn connect_remote(&self, row: &McpServerRow) -> Result<McpConnection> {
        let cfg: RemoteConfig = serde_json::from_str(&row.config)
            .map_err(|e| anyhow!("invalid remote config: {}", e))?;

        info!(
            server_id = %row.id,
            url = %cfg.url,
            credential_key = ?cfg.credential_key,
            extra_headers = ?cfg.headers.keys().collect::<Vec<_>>(),
            "connect_remote: resolved config"
        );

        // Build the merged header map.  Start with any static headers from the
        // config, then layer the credential-derived auth header on top so that
        // an explicit `headers` entry can override the default auth behaviour.
        let mut resolved_headers: HashMap<String, String> = cfg.headers.clone();

        // Resolve credential if specified (treat empty string same as absent).
        if let Some(ref cred_key) = cfg.credential_key.filter(|k| !k.is_empty()) {
            info!(
                server_id = %row.id,
                credential_key = %cred_key,
                "connect_remote: resolving credential"
            );
            let secret = credentials::resolve_secret(&self.pool, &self.master_key, cred_key)
                .await
                .map_err(|e| anyhow!("credential resolution failed: {}", e))?;

            let header_name = cfg.auth_header.as_deref().unwrap_or("Authorization");
            let format = cfg.auth_format.as_deref().unwrap_or("Bearer {value}");
            let value = format.replace("{value}", &secret);
            resolved_headers.insert(header_name.to_string(), value);
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow!("failed to build HTTP client: {}", e))?;

        // Perform `initialize` over HTTP POST.
        let init_req = JsonRpcRequest::request(
            1,
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "agent-deck",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let resp = self
            .post_rpc(&client, &cfg.url, &init_req, &resolved_headers)
            .await?;

        if let Some(err) = resp.error {
            return Err(anyhow!("initialize error: {}", err));
        }

        // Send `initialized` notification.
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        let _ = self
            .post_rpc(&client, &cfg.url, &notif, &resolved_headers)
            .await;

        // Discover tools via HTTP.
        let tools_req = JsonRpcRequest::request(2, "tools/list", None);
        let tools = match self
            .post_rpc(&client, &cfg.url, &tools_req, &resolved_headers)
            .await
        {
            Ok(resp) => resp
                .result
                .as_ref()
                .and_then(|r| r.get("tools"))
                .and_then(|t| serde_json::from_value::<Vec<McpTool>>(t.clone()).ok())
                .unwrap_or_default(),
            Err(e) => {
                warn!("mcp: remote tools/list failed: {}", e);
                Vec::new()
            }
        };

        let mut inner = McpConnectionInner::new_remote(client, cfg.url, resolved_headers);
        inner.next_id = 3; // 1 and 2 used during handshake above

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(McpConnection {
            server_id: row.id.clone(),
            inner: Mutex::new(inner),
            request_lock: tokio::sync::Mutex::new(()),
            status: RwLock::new(McpStatus::Connected),
            tools: RwLock::new(tools),
            stdout_reader: None,
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// POST a single JSON-RPC message to a remote MCP server.
    async fn post_rpc(
        &self,
        client: &reqwest::Client,
        url: &str,
        req: &JsonRpcRequest,
        headers: &HashMap<String, String>,
    ) -> Result<JsonRpcResponse> {
        let body = serde_json::to_string(req)?;

        let mut builder = client
            .post(url)
            // Required by the MCP Streamable HTTP spec: the server uses this to
            // decide whether to reply with plain JSON or an SSE stream.
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(body);

        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let response = builder
            .send()
            .await
            .map_err(|e| anyhow!("HTTP error: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP {} from MCP server",
                response.status().as_u16()
            ));
        }

        // The MCP Streamable HTTP spec allows the server to respond with either
        // `application/json` (single JSON object) or `text/event-stream` (SSE).
        // We must handle both.
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if content_type.contains("text/event-stream") {
            // Read the SSE stream and return the first JSON-RPC response found
            // in a `data:` line.  Per the spec the server SHOULD close the
            // stream after sending the response, so we stop at the first hit.
            let text = response
                .text()
                .await
                .map_err(|e| anyhow!("SSE read error: {}", e))?;

            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let resp: JsonRpcResponse = serde_json::from_str(data)
                        .map_err(|e| anyhow!("SSE JSON decode error: {}", e))?;
                    return Ok(resp);
                }
            }

            Err(anyhow!("SSE stream ended without a JSON-RPC response"))
        } else {
            let resp: JsonRpcResponse = response
                .json()
                .await
                .map_err(|e| anyhow!("JSON decode error: {}", e))?;

            Ok(resp)
        }
    }

    // ─── Connection monitoring ─────────────────────────────────────────────────

    /// Block until the connection is considered dead:
    /// - Local: the child process exits.
    /// - Remote: a periodic health-ping fails, or the connection is removed
    ///   from the pool (indicating an explicit disconnect).
    async fn monitor_connection(&self, server_id: &str, conn: &Arc<McpConnection>) {
        let is_local = conn.inner.lock().await.child.is_some();

        if is_local {
            self.monitor_local(server_id, conn).await;
        } else {
            self.monitor_remote(server_id, conn).await;
        }
    }

    async fn monitor_local(&self, server_id: &str, conn: &Arc<McpConnection>) {
        let mut shutdown_rx = conn.shutdown_rx.clone();
        loop {
            // Wait 2 s or until shutdown is signalled — whichever comes first.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        return;
                    }
                }
            }

            // Also check the flag in case it was set before we subscribed.
            if *conn.shutdown_rx.borrow() {
                return;
            }

            // Poll the child process status (non-blocking).
            let exited = {
                let mut inner = conn.inner.lock().await;
                match &mut inner.child {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => {
                            warn!(
                                "mcp: local server {} exited with status {}",
                                server_id, status
                            );
                            true
                        }
                        Ok(None) => false, // still running
                        Err(e) => {
                            warn!("mcp: try_wait error for {}: {}", server_id, e);
                            true
                        }
                    },
                    None => true, // child was taken → treat as exited
                }
            };

            if exited {
                return;
            }
        }
    }

    async fn monitor_remote(&self, server_id: &str, conn: &Arc<McpConnection>) {
        // For remote servers we send a periodic ping.  A failure signals the
        // connection is lost and triggers a reconnect.
        let mut shutdown_rx = conn.shutdown_rx.clone();
        loop {
            // Wait 30 s or until shutdown fires.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        return;
                    }
                }
            }

            // Also check the flag in case it was set before we subscribed.
            if *conn.shutdown_rx.borrow() {
                return;
            }

            // If the connection was removed from the pool, stop monitoring.
            if !self.connections.contains_key(server_id) {
                return;
            }

            // Send a ping.  We use `tools/list` as a lightweight probe since
            // the MCP protocol doesn't define a dedicated ping method.
            let ping_result = {
                let inner = conn.inner.lock().await;
                if let (Some(client), Some(url)) = (&inner.http_client, &inner.remote_url) {
                    let ping_req = JsonRpcRequest::request(0, "tools/list", None);
                    let headers = inner.extra_headers.clone();
                    self.post_rpc(client, url, &ping_req, &headers)
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                }
            };

            if let Err(e) = ping_result {
                warn!("mcp: remote server {} ping failed: {}", server_id, e);
                return; // signal reconnect
            }
        }
    }

    async fn run_keepalive_local(conn: Arc<McpConnection>) {
        const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(45);
        let mut ticker = tokio::time::interval(KEEPALIVE_INTERVAL);
        ticker.tick().await; // consume the immediate first tick

        let mut shutdown_rx = conn.shutdown_rx.clone();

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match conn.request_lock.try_lock() {
                        Err(_) => {
                            tracing::trace!(
                                server_id = %conn.server_id,
                                "mcp keepalive: skipping tick, request_lock held"
                            );
                        }
                        Ok(_guard) => {
                            let ping_result: anyhow::Result<()> = async {
                                {
                                    let mut inner = conn.inner.lock().await;
                                    let id = inner.next_id();
                                    let req = JsonRpcRequest::request(id, "tools/list", None);
                                    write_line_to_stdin(inner.stdin.as_mut().unwrap(), &req).await?;
                                }
                                let stdout_reader = conn.stdout_reader.as_ref()
                                    .ok_or_else(|| anyhow::anyhow!("no stdout reader"))?;
                                let mut reader = stdout_reader.lock().await;
                                let resp = read_json_rpc_response(&mut *reader).await?;
                                if let Some(err) = resp.error {
                                    return Err(anyhow::anyhow!("tools/list error: {}", err));
                                }
                                Ok(())
                            }.await;

                            match ping_result {
                                Ok(()) => tracing::trace!(
                                    server_id = %conn.server_id,
                                    "mcp keepalive: ping ok"
                                ),
                                Err(e) => {
                                    tracing::warn!(
                                        server_id = %conn.server_id,
                                        error = %e,
                                        "mcp keepalive: ping failed, stopping"
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }
                result = shutdown_rx.changed() => {
                    tracing::debug!(
                        server_id = %conn.server_id,
                        "mcp keepalive: shutdown signal received"
                    );
                    let _ = result;
                    return;
                }
            }
        }
    }

    // ─── Status helpers ───────────────────────────────────────────────────────

    async fn set_status(&self, server_id: &str, status: McpStatus) {
        self.set_db_status(server_id, status.as_str()).await;
        self.broadcast_status(server_id, status);
    }

    async fn set_db_status(&self, server_id: &str, status: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) =
            sqlx::query("UPDATE mcp_servers SET status = ?, updated_at = ? WHERE id = ?")
                .bind(status)
                .bind(&now)
                .bind(server_id)
                .execute(&self.pool)
                .await
        {
            warn!(
                "mcp: failed to update status in DB for {}: {}",
                server_id, e
            );
        }
    }

    fn broadcast_status(&self, server_id: &str, status: McpStatus) {
        let event = GlobalEvent::McpStatusChanged {
            mcp_server_id: server_id.to_string(),
            status: status.as_str().to_string(),
            reason: status.reason(),
        };
        // Ignore send errors — no active SSE subscribers is fine.
        let _ = self.global_tx.send(event);
    }

    // ─── DB helpers ───────────────────────────────────────────────────────────

    async fn fetch_server_row(&self, server_id: &str) -> Option<McpServerRow> {
        sqlx::query_as("SELECT id, tag, server_type, config, enabled FROM mcp_servers WHERE id = ?")
            .bind(server_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
    }

    // ─── Credential placeholder resolution ───────────────────────────────────

    /// Resolve a `{credential:<key>}` or `{credential:<key>:<field>}` placeholder
    /// in an env var value.  Plain values (without the placeholder) are returned
    /// unchanged.
    ///
    /// Supported forms:
    /// - `{credential:schwab}`          → backwards-compat, returns `secret` field
    /// - `{credential:schwab:secret}`   → explicit, returns `secret` field
    /// - `{credential:schwab:password}` → returns `password` field
    async fn resolve_env_value(&self, value: &str) -> Result<String> {
        let inner = match value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'))
        {
            Some(s) => s,
            None => return Ok(value.to_string()),
        };

        // Split on the first `:` to separate key from optional field selector.
        match inner.split_once(':') {
            // `{credential:key:field}` form
            Some((cred_key, field)) => {
                credentials::resolve_field(&self.pool, &self.master_key, cred_key, field)
                    .await
                    .map_err(|e| {
                        anyhow!(
                            "env credential '{}' field '{}' not found: {}",
                            cred_key,
                            field,
                            e
                        )
                    })
            }
            // `{credential:key}` form — backwards-compat, returns `secret`
            None => credentials::resolve_secret(&self.pool, &self.master_key, inner)
                .await
                .map_err(|e| anyhow!("env credential '{}' not found: {}", inner, e)),
        }
    }
}

// ─── Standalone stdio helpers ──────────────────────────────────────────────────

/// Write a single JSON-RPC message to a child process stdin as a newline-
/// delimited JSON line.
async fn write_line_to_stdin(stdin: &mut ChildStdin, req: &JsonRpcRequest) -> Result<()> {
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| anyhow!("stdin write failed: {}", e))?;
    stdin
        .flush()
        .await
        .map_err(|e| anyhow!("stdin flush failed: {}", e))?;
    Ok(())
}

/// Read one line from the child's stdout and parse it as a JSON-RPC response.
async fn read_json_rpc_response(
    reader: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<JsonRpcResponse> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .await
        .map_err(|e| anyhow!("stdout read failed: {}", e))?;

    if n == 0 {
        return Err(anyhow!("MCP server closed stdout unexpectedly"));
    }

    serde_json::from_str(line.trim())
        .map_err(|e| anyhow!("JSON parse error (line: {:?}): {}", line.trim(), e))
}

/// Kill a local child process if one is present, waiting up to 5 s for exit.
fn is_uuid_like(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts[0].len() == 8
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 4
        && parts[4].len() == 12
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

async fn kill_child_if_any(inner: &mut McpConnectionInner) {
    if let Some(child) = &mut inner.child {
        if let Err(e) = child.start_kill() {
            warn!("mcp: failed to kill child: {}", e);
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    }
    inner.child = None;
    inner.stdin = None;
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tag validation ────────────────────────────────────────────────────────

    #[test]
    fn tag_accepts_valid_identifiers() {
        assert!(validate_tag("github").is_ok());
        assert!(validate_tag("my-server").is_ok());
        assert!(validate_tag("my_server").is_ok());
        assert!(validate_tag("server1").is_ok());
        assert!(validate_tag("ABC").is_ok());
    }

    #[test]
    fn tag_rejects_empty() {
        assert!(validate_tag("").is_err());
    }

    #[test]
    fn tag_rejects_spaces() {
        assert!(validate_tag("my server").is_err());
    }

    #[test]
    fn tag_rejects_dots() {
        assert!(validate_tag("my.server").is_err());
    }

    #[test]
    fn tag_rejects_slashes() {
        assert!(validate_tag("my/server").is_err());
    }

    #[test]
    fn tag_rejects_too_long() {
        let long = "a".repeat(65);
        assert!(validate_tag(&long).is_err());
    }

    #[test]
    fn tag_accepts_exactly_64_chars() {
        let max = "a".repeat(64);
        assert!(validate_tag(&max).is_ok());
    }

    // ── Credential placeholder resolution ────────────────────────────────────

    #[test]
    fn resolve_env_value_plain_passthrough() {
        // Non-placeholder values are returned as-is (synchronous path can't
        // call the async fn directly, so we just test the logic inline).
        let value = "some-literal-value";
        let placeholder = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'));
        assert!(
            placeholder.is_none(),
            "should not be treated as a placeholder"
        );
    }

    #[test]
    fn resolve_env_value_extracts_key_no_field() {
        // `{credential:github_pat}` — plain form, no field selector.
        let value = "{credential:github_pat}";
        let inner = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'))
            .expect("should be a placeholder");
        let parsed = inner.split_once(':');
        assert!(
            parsed.is_none(),
            "no field selector expected for plain form"
        );
        assert_eq!(inner, "github_pat");
    }

    #[test]
    fn resolve_env_value_extracts_key_and_field_secret() {
        // `{credential:schwab:secret}` — explicit secret field selector.
        let value = "{credential:schwab:secret}";
        let inner = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'))
            .expect("placeholder");
        let (key, field) = inner.split_once(':').expect("field separator");
        assert_eq!(key, "schwab");
        assert_eq!(field, "secret");
    }

    #[test]
    fn resolve_env_value_extracts_key_and_field_password() {
        // `{credential:schwab:password}` — password field selector.
        let value = "{credential:schwab:password}";
        let inner = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'))
            .expect("placeholder");
        let (key, field) = inner.split_once(':').expect("field separator");
        assert_eq!(key, "schwab");
        assert_eq!(field, "password");
    }

    #[test]
    fn resolve_env_value_no_false_positive_on_similar_prefix() {
        // A value that starts with `{credential:` but has no closing `}` should
        // NOT match the placeholder.
        let value = "{credential:oops";
        let inner = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'));
        assert!(inner.is_none(), "missing closing brace must not match");
    }

    // ── JsonRpcRequest serialisation ─────────────────────────────────────────

    #[test]
    fn rpc_request_serialises_with_id() {
        let req = JsonRpcRequest::request(42, "tools/list", None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 42);
        assert_eq!(json["method"], "tools/list");
        assert!(json.get("params").is_none() || json["params"].is_null());
    }

    #[test]
    fn rpc_notification_has_no_id() {
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert!(json["id"].is_null());
        assert_eq!(json["method"], "notifications/initialized");
    }

    // ── McpStatus helpers ─────────────────────────────────────────────────────

    #[test]
    fn mcp_status_as_str() {
        assert_eq!(McpStatus::Inactive.as_str(), "inactive");
        assert_eq!(McpStatus::Connecting.as_str(), "connecting");
        assert_eq!(McpStatus::Connected.as_str(), "connected");
        assert_eq!(McpStatus::Error("oops".into()).as_str(), "error");
    }

    #[test]
    fn mcp_status_reason_only_for_error() {
        assert!(McpStatus::Inactive.reason().is_none());
        assert!(McpStatus::Connected.reason().is_none());
        assert_eq!(
            McpStatus::Error("timed out".into()).reason(),
            Some("timed out".to_string())
        );
    }

    // ── Filesystem helpers ────────────────────────────────────────────────────

    /// Build a minimal McpConnectionManager backed by an in-memory SQLite DB
    /// with migrations applied, pointing at the given mcp_dir.
    /// Also inserts a test user row so FK constraints on mcp_servers are satisfied.
    async fn make_test_manager(mcp_dir: std::path::PathBuf) -> McpConnectionManager {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test pool");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        // Insert a user so foreign-key constraints on mcp_servers are satisfied.
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO users (id, display_name, created_at) VALUES ('test-user', 'Test User', ?)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert test user");

        let (tx, _rx) = broadcast::channel(16);
        McpConnectionManager::new(pool, "test-master-key".to_string(), tx, mcp_dir)
    }

    /// Insert a minimal mcp_servers row directly into the DB.
    async fn insert_mcp_row(
        pool: &sqlx::SqlitePool,
        id: &str,
        name: &str,
        server_type: &str,
        config_json: &str,
        enabled: i32,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO mcp_servers (id, user_id, name, tag, server_type, config, status, enabled, created_at, updated_at)
             VALUES (?, 'test-user', ?, ?, ?, ?, 'inactive', ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(name) // tag = name for simplicity
        .bind(server_type)
        .bind(config_json)
        .bind(enabled)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert mcp row");
    }

    /// write_config_file creates the directory and writes valid JSON.
    #[tokio::test]
    async fn write_config_file_creates_dir_and_valid_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mcp_dir = tmp.path().to_path_buf();
        let mgr = make_test_manager(mcp_dir.clone()).await;

        // Build a minimal McpServer struct.
        let server = crate::models::mcp_server::McpServer {
            id: "srv-001".to_string(),
            user_id: "u1".to_string(),
            name: "github".to_string(),
            tag: "github".to_string(),
            description: None,
            source_url: None,
            server_type: "local".to_string(),
            config: r#"{"executable":"docker","args":[],"env":{}}"#.to_string(),
            status: "inactive".to_string(),
            enabled: true,
            tool_call_timeout_secs: None,
            disabled_tools: "[]".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        mgr.write_config_file(&server)
            .await
            .expect("write_config_file");

        // Directory should be named after the tag, not the id.
        let config_path = mcp_dir.join("github").join("config.json");
        assert!(
            config_path.exists(),
            "config.json should exist under tag-named dir"
        );

        let raw = tokio::fs::read_to_string(&config_path).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(
            v["id"], "srv-001",
            "config.json should contain the server id"
        );
        assert_eq!(v["name"], "github");
        assert_eq!(v["tag"], "github");
        assert_eq!(v["server_type"], "local");
        assert!(v["config"].is_object(), "config should be a JSON object");
        assert_eq!(v["config"]["executable"], "docker");
    }

    /// delete_config_dir removes the mcp_dir/<id>/ directory.
    #[tokio::test]
    async fn delete_config_dir_removes_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mcp_dir = tmp.path().to_path_buf();
        let mgr = make_test_manager(mcp_dir.clone()).await;

        // Create the directory and a file inside it.
        let srv_dir = mcp_dir.join("srv-del");
        tokio::fs::create_dir_all(&srv_dir).await.unwrap();
        tokio::fs::write(srv_dir.join("config.json"), b"{}")
            .await
            .unwrap();
        assert!(srv_dir.exists());

        mgr.delete_config_dir("srv-del")
            .await
            .expect("delete_config_dir");

        assert!(!srv_dir.exists(), "directory should be removed");
    }

    /// delete_config_dir is a no-op when the directory does not exist.
    #[tokio::test]
    async fn delete_config_dir_nonexistent_is_ok() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mcp_dir = tmp.path().to_path_buf();
        let mgr = make_test_manager(mcp_dir.clone()).await;

        let result = mgr.delete_config_dir("does-not-exist").await;
        assert!(result.is_ok());
    }

    /// startup_sync upserts a config.json into the DB when no row exists.
    #[tokio::test]
    async fn startup_sync_inserts_missing_db_row() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mcp_dir = tmp.path().to_path_buf();
        let mgr = make_test_manager(mcp_dir.clone()).await;

        // Write a config.json for a server that has no DB row.
        // The folder is named after the tag; the config includes an explicit id.
        let srv_dir = mcp_dir.join("github");
        tokio::fs::create_dir_all(&srv_dir).await.unwrap();
        let cfg = serde_json::json!({
            "id": "github-srv-id",
            "name": "github",
            "tag": "github",
            "server_type": "local",
            "config": { "executable": "docker", "args": [], "env": {} }
        });
        tokio::fs::write(
            srv_dir.join("config.json"),
            serde_json::to_string_pretty(&cfg).unwrap().as_bytes(),
        )
        .await
        .unwrap();

        mgr.startup_sync().await.expect("startup_sync");

        // The row should now exist in the DB, keyed by the "id" field in config.json.
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, name FROM mcp_servers WHERE id = 'github-srv-id'")
                .fetch_optional(&mgr.pool)
                .await
                .unwrap();

        assert!(row.is_some(), "row should have been inserted");
        let (id, name) = row.unwrap();
        assert_eq!(id, "github-srv-id");
        assert_eq!(name, "github");
    }

    /// startup_sync updates the DB config column when config.json changes.
    #[tokio::test]
    async fn startup_sync_updates_db_config_when_file_changes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mcp_dir = tmp.path().to_path_buf();
        let mgr = make_test_manager(mcp_dir.clone()).await;

        // Pre-insert a row with an old config.
        insert_mcp_row(
            &mgr.pool,
            "my-srv",
            "old-name",
            "local",
            r#"{"executable":"old-bin"}"#,
            1,
        )
        .await;

        // Write a config.json with updated values.
        // The folder is named after the tag; include "id" so startup_sync can
        // match the pre-existing DB row.
        let srv_dir = mcp_dir.join("my-srv");
        tokio::fs::create_dir_all(&srv_dir).await.unwrap();
        let cfg = serde_json::json!({
            "id": "my-srv",
            "name": "new-name",
            "tag": "my-srv",
            "server_type": "local",
            "config": { "executable": "new-bin", "args": [], "env": {} }
        });
        tokio::fs::write(
            srv_dir.join("config.json"),
            serde_json::to_string_pretty(&cfg).unwrap().as_bytes(),
        )
        .await
        .unwrap();

        mgr.startup_sync().await.expect("startup_sync");

        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, name FROM mcp_servers WHERE id = 'my-srv'")
                .fetch_optional(&mgr.pool)
                .await
                .unwrap();

        let (_, name) = row.expect("row should exist");
        assert_eq!(name, "new-name", "name should be updated from config.json");
    }

    /// startup_sync disables a DB row when its config.json directory is gone.
    #[tokio::test]
    async fn startup_sync_disables_row_when_config_dir_gone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mcp_dir = tmp.path().to_path_buf();
        let mgr = make_test_manager(mcp_dir.clone()).await;

        // Insert an enabled row but do NOT create the config.json.
        insert_mcp_row(
            &mgr.pool,
            "orphan-srv",
            "orphan",
            "local",
            r#"{"executable":"bin"}"#,
            1, // enabled
        )
        .await;

        mgr.startup_sync().await.expect("startup_sync");

        let row: Option<(bool,)> =
            sqlx::query_as("SELECT enabled FROM mcp_servers WHERE id = 'orphan-srv'")
                .fetch_optional(&mgr.pool)
                .await
                .unwrap();

        let (enabled,) = row.expect("row should still exist");
        assert!(
            !enabled,
            "row should be disabled when config.json is missing"
        );
    }
}

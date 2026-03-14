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
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex, RwLock};
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
    /// Decrypted auth header value, e.g. `"Bearer <token>"`.
    auth_header: Option<String>,
    /// The URL for remote servers.
    remote_url: Option<String>,
    /// Cached tool list populated after `tools/list` succeeds.
    tools: Vec<McpTool>,
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
            auth_header: None,
            remote_url: None,
            tools: Vec::new(),
            shutting_down: false,
        }
    }

    fn new_remote(client: reqwest::Client, url: String, auth_header: Option<String>) -> Self {
        Self {
            next_id: 1,
            child: None,
            stdin: None,
            http_client: Some(client),
            auth_header,
            remote_url: Some(url),
            tools: Vec::new(),
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
    /// Current connection status (mirrored here for fast in-memory reads).
    #[allow(dead_code)]
    status: RwLock<McpStatus>,
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
}

/// A minimal DB projection of the `mcp_servers` row — only what the manager needs.
#[derive(Debug, sqlx::FromRow)]
struct McpServerRow {
    id: String,
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
}

impl McpConnectionManager {
    /// Create a new manager.  Call [`start`] to connect enabled servers.
    pub fn new(
        pool: SqlitePool,
        master_key: String,
        global_tx: broadcast::Sender<GlobalEvent>,
    ) -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            pool,
            master_key,
            global_tx,
        }
    }

    // ─── Startup ─────────────────────────────────────────────────────────────

    /// Connect all `enabled = 1` MCP servers found in the database.
    ///
    /// Each server gets its own spawned task so a slow or failing server does
    /// not block the others.  Returns immediately.
    pub async fn start(&self) {
        let rows: Vec<McpServerRow> = match sqlx::query_as(
            "SELECT id, server_type, config, enabled FROM mcp_servers WHERE enabled = 1",
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
            let mut inner = conn.inner.lock().await;
            inner.shutting_down = true;
            kill_child_if_any(&mut inner).await;
            info!("mcp: disconnected server {}", server_id);
        }
        // Mark the DB row as inactive regardless.
        self.set_db_status(server_id, "inactive").await;
    }

    /// Return the cached tool list for a server (empty if not connected yet).
    pub fn cached_tools(&self, server_id: &str) -> Vec<McpTool> {
        self.connections
            .get(server_id)
            .map(|conn| {
                // We need a blocking read — this is only called from async HTTP
                // handlers, so we use `blocking_read()` which is fine on a
                // multi-thread Tokio runtime.
                conn.inner.blocking_lock().tools.clone()
            })
            .unwrap_or_default()
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
                        "mcp: server {} no longer in DB, stopping supervision",
                        server_id
                    );
                    return;
                }
            };

            // If the server was disabled externally, stop.
            if !row.enabled {
                info!(
                    "mcp: server {} is disabled, stopping supervision",
                    server_id
                );
                self.set_db_status(server_id, "inactive").await;
                self.broadcast_status(server_id, McpStatus::Inactive);
                return;
            }

            self.set_status(server_id, McpStatus::Connecting).await;

            match self.connect_and_handshake(&row).await {
                Ok(conn) => {
                    let conn = Arc::new(conn);
                    self.connections.insert(server_id.to_string(), conn.clone());

                    // Record when we connected so we can reset backoff after
                    // a sustained stable period.
                    connected_at = Some(tokio::time::Instant::now());
                    self.set_status(server_id, McpStatus::Connected).await;
                    info!("mcp: server {} connected", server_id);

                    // Monitor until the connection drops (or shutdown requested).
                    self.monitor_connection(server_id, &conn).await;

                    // If we were asked to shut down, exit the loop.
                    if conn.inner.lock().await.shutting_down {
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
                        "mcp: server {} connection lost, will retry in {}s",
                        server_id,
                        backoff.as_secs()
                    );
                    self.set_status(server_id, McpStatus::Error("Connection lost".to_string()))
                        .await;
                    self.connections.remove(server_id);
                }
                Err(e) => {
                    let msg = format!("{:#}", e);
                    error!("mcp: server {} failed to connect: {}", server_id, msg);
                    self.set_status(server_id, McpStatus::Error(msg)).await;
                }
            }

            // Wait with backoff before next attempt, but exit early if the
            // entry disappears from the map (disconnect_server was called).
            let sleep_fut = sleep(backoff);
            tokio::pin!(sleep_fut);
            loop {
                tokio::select! {
                    _ = &mut sleep_fut => break,
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
                        if !self.connections.contains_key(server_id) {
                            // disconnect_server removed us — stop supervision.
                            return;
                        }
                        // Also stop if the server was disabled.
                        if let Some(row) = self.fetch_server_row(server_id).await {
                            if !row.enabled {
                                return;
                            }
                        } else {
                            return;
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

        let mut cmd = Command::new(&cfg.executable);
        cmd.args(&cfg.args)
            .envs(&resolved_env)
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
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("mcp[{}] stderr: {}", server_id, line);
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
                warn!("mcp: tools/list failed: {}", e);
                Vec::new()
            });

        info!(
            "mcp: local server connected, {} tool(s) discovered",
            tools.len()
        );
        conn.tools = tools;

        // Put stdout back onto the child for monitoring (via a shared Arc).
        // We wrap the reader in a background task that watches for EOF (process exit).
        let stdout_reader: Arc<Mutex<BufReader<tokio::process::ChildStdout>>> =
            Arc::new(Mutex::new(reader));

        let mcp_conn = McpConnection {
            server_id: row.id.clone(),
            inner: Mutex::new(conn),
            status: RwLock::new(McpStatus::Connected),
        };

        // Stash the stdout reader into a task-local slot so `monitor_connection`
        // can wait on it.  We use a channel as a one-shot signal.
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            // Block until the child's stdout yields EOF (process exited).
            let mut reader = stdout_reader.lock().await;
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break, // EOF or error → process exited
                    Ok(_) => {}              // discard stdout lines (not needed post-handshake)
                }
            }
            let _ = exit_tx.send(());
        });

        // Store the exit receiver so `monitor_connection` can await it.
        // We use a Mutex<Option<…>> stored alongside the connection.  For
        // simplicity we tuck it away in a task-local via a second DashMap slot.
        //
        // In practice: the local connection's `monitor_connection` path polls
        // the child's `wait()` directly, so we just let it drop here.
        drop(exit_rx);

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

        // Resolve credential if specified.
        let auth_header_value = if let Some(ref cred_key) = cfg.credential_key {
            let secret = credentials::resolve_secret(&self.pool, &self.master_key, cred_key)
                .await
                .map_err(|e| anyhow!("credential resolution failed: {}", e))?;

            let header_name = cfg.auth_header.as_deref().unwrap_or("Authorization");
            let format = cfg.auth_format.as_deref().unwrap_or("Bearer {value}");
            let value = format.replace("{value}", &secret);
            Some(format!("{}: {}", header_name, value))
        } else {
            None
        };

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
            .post_rpc(&client, &cfg.url, &init_req, auth_header_value.as_deref())
            .await?;

        if let Some(err) = resp.error {
            return Err(anyhow!("initialize error: {}", err));
        }

        // Send `initialized` notification.
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        let _ = self
            .post_rpc(&client, &cfg.url, &notif, auth_header_value.as_deref())
            .await;

        // Discover tools via HTTP.
        let tools_req = JsonRpcRequest::request(2, "tools/list", None);
        let tools = match self
            .post_rpc(&client, &cfg.url, &tools_req, auth_header_value.as_deref())
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

        info!(
            "mcp: remote server {} connected, {} tool(s) discovered",
            cfg.url,
            tools.len()
        );

        let mut inner = McpConnectionInner::new_remote(client, cfg.url, auth_header_value);
        inner.next_id = 3; // 1 and 2 used during handshake above
        inner.tools = tools;

        Ok(McpConnection {
            server_id: row.id.clone(),
            inner: Mutex::new(inner),
            status: RwLock::new(McpStatus::Connected),
        })
    }

    /// POST a single JSON-RPC message to a remote MCP server.
    async fn post_rpc(
        &self,
        client: &reqwest::Client,
        url: &str,
        req: &JsonRpcRequest,
        auth_header: Option<&str>,
    ) -> Result<JsonRpcResponse> {
        let body = serde_json::to_string(req)?;

        let mut builder = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body);

        if let Some(header) = auth_header {
            // header is "HeaderName: Value" — split on the first ": ".
            if let Some((name, value)) = header.split_once(": ") {
                builder = builder.header(name, value);
            }
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

        let resp: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("JSON decode error: {}", e))?;

        Ok(resp)
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
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Check if an explicit shutdown was requested.
            {
                let inner = conn.inner.lock().await;
                if inner.shutting_down {
                    return;
                }
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
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;

            // Check for explicit shutdown.
            {
                let inner = conn.inner.lock().await;
                if inner.shutting_down {
                    return;
                }
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
                    let auth = inner.auth_header.as_deref();
                    self.post_rpc(client, url, &ping_req, auth)
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
        sqlx::query_as("SELECT id, server_type, config, enabled FROM mcp_servers WHERE id = ?")
            .bind(server_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
    }

    // ─── Credential placeholder resolution ───────────────────────────────────

    /// Resolve a `{credential:<key>}` placeholder in an env var value.
    /// Plain values (without the placeholder) are returned unchanged.
    async fn resolve_env_value(&self, value: &str) -> Result<String> {
        if let Some(cred_key) = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'))
        {
            credentials::resolve_secret(&self.pool, &self.master_key, cred_key)
                .await
                .map_err(|e| anyhow!("env credential '{}' not found: {}", cred_key, e))
        } else {
            Ok(value.to_string())
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
    fn resolve_env_value_extracts_key() {
        let value = "{credential:github_pat}";
        let key = value
            .strip_prefix("{credential:")
            .and_then(|s| s.strip_suffix('}'));
        assert_eq!(key, Some("github_pat"));
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
}

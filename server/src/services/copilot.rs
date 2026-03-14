//! CopilotApiService — manages the copilot-api child process.
//!
//! Responsibilities:
//! - Build `dist/main.js` inside `vendor/copilot-api/` when it is missing, using
//!   two self-contained Node steps:
//!     1. `npm install`                      — installs deps into node_modules/
//!     2. `./node_modules/.bin/tsdown`       — bundles src/ → dist/main.js
//! - Spawn `node dist/main.js start` to run the sidecar process.
//! - Poll `http://localhost:4141/` until the process responds (health check)
//! - Monitor the child and restart it on unexpected exit with exponential backoff
//! - Expose `is_available()` and `base_url()` for other services
//! - Emit `provider_status` events on the global SSE broadcast channel
//! - Kill the child cleanly on server shutdown
//!
//! Requires Node.js 24+ and npm on PATH (or at well-known nvm/system locations).

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Status of the copilot-api side-car process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopilotStatus {
    /// Process has not been started yet.
    Stopped,
    /// Process was spawned; waiting for the health check to pass.
    Connecting,
    /// Health check passed — proxy is accepting requests.
    Connected,
    /// Process exited unexpectedly; service is waiting before restarting.
    Reconnecting,
    /// Process could not be started (e.g. `bun` not on PATH).
    Error(String),
}

impl CopilotStatus {
    pub fn as_str(&self) -> &str {
        match self {
            CopilotStatus::Stopped => "stopped",
            CopilotStatus::Connecting => "connecting",
            CopilotStatus::Connected => "connected",
            CopilotStatus::Reconnecting => "reconnecting",
            CopilotStatus::Error(_) => "error",
        }
    }
}

/// Global SSE events produced by the server.
///
/// This enum lives here because CopilotApiService is the first consumer;
/// it will be expanded in Story 2.3 when the SSE infrastructure is wired
/// up fully.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GlobalEvent {
    /// copilot-api connected or disconnected.
    ProviderStatus {
        provider_id: String,
        status: String,
        reason: Option<String>,
    },
    /// A thread's last message preview changed (emitted by agent run-loop).
    ThreadUpdated {
        thread_id: String,
        last_message: String,
        updated_at: String,
    },
    /// A routine fired.
    RoutineFired {
        thread_id: String,
        routine_id: String,
        routine_name: String,
    },
    /// Title was generated (or updated) for a thread after the first assistant
    /// reply. Emitted by `agent::run_inner` so the client sidebar updates
    /// without a page reload or a second HTTP call.
    TitleUpdated { thread_id: String, title: String },
}

// ─── Constants ────────────────────────────────────────────────────────────────

const COPILOT_PORT: u16 = 4141;
const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(2);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const MIN_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

// ─── Inner state (behind an Arc so handles can be cloned) ─────────────────────

struct Inner {
    status: RwLock<CopilotStatus>,
    child: Mutex<Option<Child>>,
}

// ─── Public handle ─────────────────────────────────────────────────────────────

/// Cloneable handle to the CopilotApiService.
///
/// All methods are cheap to call — the heavy state lives behind an `Arc`.
#[derive(Clone)]
pub struct CopilotApiService {
    inner: Arc<Inner>,
    global_tx: broadcast::Sender<GlobalEvent>,
}

impl CopilotApiService {
    /// Create a new service.  The process is *not* started yet — call [`start`].
    pub fn new(global_tx: broadcast::Sender<GlobalEvent>) -> Self {
        Self {
            inner: Arc::new(Inner {
                status: RwLock::new(CopilotStatus::Stopped),
                child: Mutex::new(None),
            }),
            global_tx,
        }
    }

    // ─── Status ───────────────────────────────────────────────────────────────

    /// Returns `true` when the proxy is healthy and accepting requests.
    pub async fn is_available(&self) -> bool {
        *self.inner.status.read().await == CopilotStatus::Connected
    }

    /// Base URL for the copilot-api OpenAI-compatible proxy.
    pub fn base_url(&self) -> &str {
        // Port is fixed per PLAN.md §3.4.
        "http://localhost:4141/v1"
    }

    /// Current status snapshot (cheap clone).
    pub async fn status(&self) -> CopilotStatus {
        self.inner.status.read().await.clone()
    }

    // ─── Lifecycle ────────────────────────────────────────────────────────────

    /// Spawn the background supervision task.
    ///
    /// Returns immediately — does *not* block until copilot-api is ready.
    /// Use [`is_available()`] to check readiness.
    pub fn start(self) {
        tokio::spawn(async move {
            self.supervise_loop().await;
        });
    }

    /// Graceful shutdown: send SIGTERM to the child process and wait briefly.
    pub async fn shutdown(&self) {
        let mut guard = self.inner.child.lock().await;
        if let Some(child) = guard.as_mut() {
            info!("copilot-api: sending SIGTERM to child process");
            if let Err(e) = child.start_kill() {
                warn!("copilot-api: failed to kill child: {}", e);
            }
            // Give it up to 5 seconds to exit gracefully
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        }
        *guard = None;
        self.set_status(CopilotStatus::Stopped, None).await;
    }

    /// Restart the sidecar so it re-reads the GitHub token from disk.
    ///
    /// Kills the current child (if any), waits for it to fully exit (so the
    /// port is released), then clears the slot so the supervision loop spawns
    /// a fresh process.  The caller does not need to call `start()` again.
    pub async fn restart(&self) {
        info!("copilot-api: restart requested (new GitHub token written to disk)");
        self.set_status(
            CopilotStatus::Reconnecting,
            Some("Restarting to load new token".to_string()),
        )
        .await;

        let mut guard = self.inner.child.lock().await;
        if let Some(child) = guard.as_mut() {
            if let Err(e) = child.start_kill() {
                warn!("copilot-api: failed to kill child during restart: {}", e);
            }
            // Wait for the process to fully exit so the port (4141) is released
            // before the supervise_loop spawns a new instance.
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        }
        // Clearing the slot signals wait_for_exit to return, which causes the
        // supervise_loop to fall through to the respawn path.
        *guard = None;
    }

    // ─── Internal ─────────────────────────────────────────────────────────────

    /// Main supervision loop.  Runs forever until the Tokio runtime shuts down.
    async fn supervise_loop(&self) {
        let mut backoff = MIN_BACKOFF;

        loop {
            match self.spawn_child().await {
                Ok(child) => {
                    *self.inner.child.lock().await = Some(child);
                    backoff = MIN_BACKOFF; // reset on successful spawn

                    self.set_status(CopilotStatus::Connecting, None).await;
                    info!("copilot-api: process spawned, waiting for health check");

                    // Wait for health check to pass (with timeout)
                    match self.wait_until_healthy().await {
                        Ok(()) => {
                            self.set_status(CopilotStatus::Connected, None).await;
                            info!("copilot-api: healthy on {}", self.base_url());

                            // Block until the child exits
                            self.wait_for_exit().await;

                            warn!("copilot-api: process exited unexpectedly");
                            self.set_status(
                                CopilotStatus::Reconnecting,
                                Some("Process exited unexpectedly".to_string()),
                            )
                            .await;
                        }
                        Err(e) => {
                            warn!("copilot-api: health check timed out: {}", e);
                            self.set_status(
                                CopilotStatus::Reconnecting,
                                Some(format!("Health check failed: {}", e)),
                            )
                            .await;
                            // Kill the process — it may still be running but unresponsive
                            let mut guard = self.inner.child.lock().await;
                            if let Some(child) = guard.as_mut() {
                                let _ = child.start_kill();
                            }
                            *guard = None;
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to spawn copilot-api: {}", e);
                    error!("{}", msg);
                    self.set_status(CopilotStatus::Error(msg), None).await;
                }
            }

            info!("copilot-api: restarting in {} seconds", backoff.as_secs());
            sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    /// Kill any stale process already listening on COPILOT_PORT.
    ///
    /// When the Rust server is hard-killed (SIGKILL, IDE restart, etc.) the
    /// supervised `bun` child becomes an orphan — its Tokio `Child` handle is
    /// dropped without calling `start_kill()`, so it keeps running and holds
    /// the port.  We do a best-effort `lsof`-based kill before spawning so the
    /// new process can bind successfully.
    async fn kill_stale_port_holder() {
        // `lsof -t -i :<port>` prints the PID(s) listening on the port, one per line.
        let output = Command::new("lsof")
            .args(["-t", "-i", &format!(":{}", COPILOT_PORT)])
            .output()
            .await;

        let pids = match output {
            Ok(o) if o.status.success() => o.stdout,
            _ => return, // lsof not available or no process found — nothing to do
        };

        let current_pid = std::process::id();

        for line in std::str::from_utf8(&pids).unwrap_or("").lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                // Never kill ourselves
                if pid == current_pid {
                    continue;
                }
                info!("copilot-api: killing stale port holder PID {}", pid);
                // Use `kill` via Command — no libc dependency needed.
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .output()
                    .await;
            }
        }

        // Brief pause to let the processes release the port before we bind.
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    /// Spawn the copilot-api process.

    /// Resolve the `vendor/copilot-api` directory, returning an error if it is
    /// missing (submodule not initialised).
    fn copilot_dir() -> Result<std::path::PathBuf> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent() // workspace root (agent-deck/)
            .ok_or_else(|| anyhow!("Cannot resolve workspace root"))?
            .join("vendor/copilot-api");

        if !dir.exists() {
            return Err(anyhow!(
                "vendor/copilot-api directory not found at {:?}. \
                 Run `git submodule update --init` to initialise the submodule.",
                dir
            ));
        }

        Ok(dir)
    }

    /// Resolve the `node` binary.
    ///
    /// Checks (in order):
    /// 1. `node` on the current `PATH` (covers most interactive shells and CI).
    /// 2. Common nvm default installation paths (`~/.nvm/versions/node/*/bin/node`),
    ///    picking the highest version directory available. This handles the common
    ///    case where nvm is installed but the shell profile has not been sourced
    ///    (e.g. launched from an IDE or a launchd service).
    /// 3. `/usr/local/bin/node` and `/usr/bin/node` as last-resort system paths.
    fn resolve_node_binary() -> Option<std::ffi::OsString> {
        // 1. PATH fast path.
        if Self::binary_on_path("node") {
            return Some("node".into());
        }

        // 2. nvm install directory — pick the lexicographically highest version.
        if let Some(home) = std::env::var_os("HOME") {
            let nvm_versions = std::path::Path::new(&home)
                .join(".nvm")
                .join("versions")
                .join("node");

            if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
                let mut versions: Vec<std::path::PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                versions.sort();

                // Highest version is last after sort (v24 > v22 > ...).
                if let Some(latest) = versions.last() {
                    let candidate = latest.join("bin").join("node");
                    if candidate.exists() {
                        return Some(candidate.into_os_string());
                    }
                }
            }
        }

        // 3. Well-known system paths.
        for path in &["/usr/local/bin/node", "/usr/bin/node"] {
            let p = std::path::Path::new(path);
            if p.exists() {
                return Some(p.as_os_str().to_os_string());
            }
        }

        None
    }

    /// Resolve the `npm` binary, using the same search strategy as `resolve_node_binary`.
    fn resolve_npm_binary() -> Option<std::ffi::OsString> {
        // 1. PATH fast path.
        if Self::binary_on_path("npm") {
            return Some("npm".into());
        }

        // 2. nvm — npm sits alongside node in the same bin/ directory.
        if let Some(home) = std::env::var_os("HOME") {
            let nvm_versions = std::path::Path::new(&home)
                .join(".nvm")
                .join("versions")
                .join("node");

            if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
                let mut versions: Vec<std::path::PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                versions.sort();

                if let Some(latest) = versions.last() {
                    let candidate = latest.join("bin").join("npm");
                    if candidate.exists() {
                        return Some(candidate.into_os_string());
                    }
                }
            }
        }

        // 3. Well-known system paths.
        for path in &["/usr/local/bin/npm", "/usr/bin/npm"] {
            let p = std::path::Path::new(path);
            if p.exists() {
                return Some(p.as_os_str().to_os_string());
            }
        }

        None
    }

    /// Returns `true` if `name` resolves to an executable on the current PATH.
    fn binary_on_path(name: &str) -> bool {
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                if dir.join(name).exists() {
                    return true;
                }
            }
        }
        false
    }

    /// Ensure `dist/main.js` exists inside `copilot_dir`, building it when missing.
    ///
    /// Build steps (all self-contained — no global tools beyond node/npm needed):
    ///   1. `npm install`                    — installs deps incl. tsdown
    ///   2. `./node_modules/.bin/tsdown`     — bundles src/ → dist/main.js
    async fn ensure_dist_built(copilot_dir: &std::path::Path) -> Result<()> {
        let dist_main = copilot_dir.join("dist").join("main.js");
        if dist_main.exists() {
            return Ok(());
        }

        info!("copilot-api: dist/main.js not found — building...");

        let npm = Self::resolve_npm_binary().ok_or_else(|| {
            anyhow!(
                "`npm` not found on PATH or in nvm. \
                 Install Node.js 24+ (https://nodejs.org) to use the GitHub Copilot provider."
            )
        })?;

        // Step 1: npm install
        info!("copilot-api: running npm install");
        let install_status = Command::new(&npm)
            .args(["install"])
            .current_dir(copilot_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| anyhow!("Failed to run npm install: {}", e))?;

        if !install_status.success() {
            return Err(anyhow!(
                "npm install failed (exit code {:?})",
                install_status.code()
            ));
        }

        // Step 2: ./node_modules/.bin/tsdown
        let tsdown = copilot_dir.join("node_modules").join(".bin").join("tsdown");
        if !tsdown.exists() {
            return Err(anyhow!(
                "tsdown not found at {:?} after npm install — something went wrong",
                tsdown
            ));
        }

        info!("copilot-api: running tsdown build");
        let build_status = Command::new(&tsdown)
            .current_dir(copilot_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| anyhow!("Failed to run tsdown: {}", e))?;

        if !build_status.success() {
            return Err(anyhow!(
                "tsdown build failed (exit code {:?})",
                build_status.code()
            ));
        }

        info!("copilot-api: build complete — dist/main.js is ready");
        Ok(())
    }

    async fn spawn_child(&self) -> Result<Child> {
        let copilot_dir = Self::copilot_dir()?;

        // Ensure dist/main.js exists, building it from source if necessary.
        Self::ensure_dist_built(&copilot_dir).await?;

        let node = Self::resolve_node_binary().ok_or_else(|| {
            anyhow!(
                "`node` not found on PATH or in nvm. \
                 Install Node.js 24+ (https://nodejs.org) to use the GitHub Copilot provider."
            )
        })?;

        // Kill any stale orphan that may be holding the port from a previous
        // server run that was hard-killed before it could clean up its child.
        Self::kill_stale_port_holder().await;

        let child = Command::new(&node)
            .args(["dist/main.js", "start", "--port", &COPILOT_PORT.to_string()])
            .current_dir(&copilot_dir)
            // Pipe stdout/stderr so we don't clutter the server's terminal by default.
            // Flip to `Stdio::inherit()` for debugging.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Send SIGKILL to the child automatically when the Child handle is
            // dropped.  This covers the case where the Tokio runtime tears down
            // the supervision task without going through our explicit shutdown
            // path (e.g. the server binary is SIGKILL'd by cargo-watch / an IDE).
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    anyhow!(
                        "`node` not found. \
                         Install Node.js 24+ (https://nodejs.org) to use the GitHub Copilot provider."
                    )
                } else {
                    anyhow!("Failed to spawn copilot-api: {}", e)
                }
            })?;

        Ok(child)
    }

    /// Poll `http://localhost:4141/` until it returns HTTP 200, or until
    /// `HEALTH_TIMEOUT` elapses.
    async fn wait_until_healthy(&self) -> Result<()> {
        let url = format!("http://localhost:{}/", COPILOT_PORT);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;

        let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(anyhow!(
                    "Timed out after {}s waiting for copilot-api to become healthy",
                    HEALTH_TIMEOUT.as_secs()
                ));
            }

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 200 => {
                    return Ok(());
                }
                Ok(resp) => {
                    // Non-200 but reachable — keep waiting
                    info!("copilot-api health check: HTTP {} (waiting)", resp.status());
                }
                Err(_) => {
                    // Connection refused or timeout — process still starting up
                }
            }

            sleep(HEALTH_POLL_INTERVAL).await;
        }
    }

    /// Block until the managed child process exits.
    async fn wait_for_exit(&self) {
        // We need exclusive access to poll, but we can't hold the lock for the
        // entire wait (shutdown() also needs it).  Instead we poll with a short
        // sleep in a loop.
        loop {
            {
                let mut guard = self.inner.child.lock().await;
                if let Some(child) = guard.as_mut() {
                    match child.try_wait() {
                        Ok(Some(_exit)) => {
                            // Child has exited; clear the slot.
                            *guard = None;
                            return;
                        }
                        Ok(None) => {} // still running
                        Err(e) => {
                            warn!("copilot-api: error polling child: {}", e);
                            *guard = None;
                            return;
                        }
                    }
                } else {
                    // Child slot was cleared externally (e.g. shutdown())
                    return;
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// Update the internal status and emit a `provider_status` global SSE event.
    async fn set_status(&self, status: CopilotStatus, reason: Option<String>) {
        let status_str = status.as_str().to_string();
        *self.inner.status.write().await = status;

        let event = GlobalEvent::ProviderStatus {
            provider_id: "copilot".to_string(),
            status: status_str,
            reason,
        };

        // It's fine if there are no receivers yet — broadcast::Sender::send
        // returns Err only when there are zero active receivers, which is
        // expected at startup.
        let _ = self.global_tx.send(event);
    }
}

// ─── Auth helpers ─────────────────────────────────────────────────────────────

/// Response from `GET /token` on the copilot-api proxy.
#[derive(Debug, Serialize, Deserialize)]
pub struct CopilotAuthStatus {
    /// Whether the proxy currently holds a valid GitHub token.
    pub authenticated: bool,
    /// Optional human-readable reason when not authenticated.
    pub reason: Option<String>,
}

/// Response returned when the user initiates the GitHub device auth flow.
#[derive(Debug, Serialize, Deserialize)]
pub struct CopilotAuthStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Check whether copilot-api has a valid GitHub token by hitting `GET /token`.
pub async fn check_auth_status(base_url: &str) -> Result<CopilotAuthStatus> {
    let url = format!("{}/token", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to reach copilot-api at {}: {}", url, e))?;

    if resp.status().is_success() {
        Ok(CopilotAuthStatus {
            authenticated: true,
            reason: None,
        })
    } else {
        let body = resp.text().await.unwrap_or_default();
        Ok(CopilotAuthStatus {
            authenticated: false,
            reason: Some(body),
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> (CopilotApiService, broadcast::Receiver<GlobalEvent>) {
        let (tx, rx) = broadcast::channel(16);
        (CopilotApiService::new(tx), rx)
    }

    #[tokio::test]
    async fn initial_status_is_stopped() {
        let (svc, _rx) = make_service();
        assert_eq!(svc.status().await, CopilotStatus::Stopped);
        assert!(!svc.is_available().await);
    }

    #[tokio::test]
    async fn base_url_is_correct() {
        let (svc, _rx) = make_service();
        assert_eq!(svc.base_url(), "http://localhost:4141/v1");
    }

    #[tokio::test]
    async fn set_status_emits_global_event() {
        let (svc, mut rx) = make_service();

        svc.set_status(CopilotStatus::Connected, None).await;

        let event = rx.try_recv().expect("should have received an event");
        match event {
            GlobalEvent::ProviderStatus {
                provider_id,
                status,
                reason,
            } => {
                assert_eq!(provider_id, "copilot");
                assert_eq!(status, "connected");
                assert!(reason.is_none());
            }
            _ => panic!("unexpected event type"),
        }
    }

    #[tokio::test]
    async fn set_status_connected_makes_available() {
        let (svc, _rx) = make_service();
        svc.set_status(CopilotStatus::Connected, None).await;
        assert!(svc.is_available().await);
    }

    #[tokio::test]
    async fn set_status_reconnecting_makes_unavailable() {
        let (svc, _rx) = make_service();
        // First go connected
        svc.set_status(CopilotStatus::Connected, None).await;
        assert!(svc.is_available().await);
        // Then simulate an unexpected exit
        svc.set_status(
            CopilotStatus::Reconnecting,
            Some("Process exited unexpectedly".to_string()),
        )
        .await;
        assert!(!svc.is_available().await);
    }

    #[tokio::test]
    async fn set_status_error_includes_reason() {
        let (svc, mut rx) = make_service();
        svc.set_status(
            CopilotStatus::Error("bun not found".to_string()),
            Some("bun not found".to_string()),
        )
        .await;

        let event = rx.try_recv().expect("event");
        match event {
            GlobalEvent::ProviderStatus { reason, .. } => {
                assert_eq!(reason, Some("bun not found".to_string()));
            }
            _ => panic!("wrong event"),
        }
    }

    #[tokio::test]
    async fn shutdown_when_no_child_is_noop() {
        let (svc, _rx) = make_service();
        // Should not panic
        svc.shutdown().await;
        assert_eq!(svc.status().await, CopilotStatus::Stopped);
    }

    #[tokio::test]
    async fn copilot_dir_path_resolves() {
        // Verify CARGO_MANIFEST_DIR is set and the path resolution doesn't panic
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        assert!(!manifest_dir.is_empty());
        let root = std::path::Path::new(manifest_dir)
            .parent()
            .expect("parent of server/");
        // workspace root should exist
        assert!(root.exists(), "workspace root should exist at {:?}", root);
    }
}

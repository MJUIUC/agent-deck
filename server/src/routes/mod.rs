use axum::{
    body::Body,
    extract::Request,
    http::header,
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};
use dashmap::DashMap;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::warn;

use crate::config::Config;
use crate::error::AppError;
use crate::services::auth as auth_service;
use crate::services::copilot::{CopilotApiService, GlobalEvent};
use crate::services::credentials as credentials_service;
use crate::services::mcp::McpConnectionManager;
use crate::services::scheduler::SchedulerCommand;
use crate::services::tools::{self as tools_service, AgentTool};

pub mod auth;
pub mod config;
pub mod credentials;
pub mod fs;
pub mod health;
pub mod memory;
pub mod messages;
pub mod models;
pub mod notify;
pub mod personas;
pub mod profile;
pub mod providers;
pub mod push;
pub mod routines;
pub mod setup;

pub mod sse;
pub mod threads;
pub mod tokens;

/// A unit of work — kept for compatibility with any remaining references,
/// but the channel-based agent dispatch has been replaced with per-thread
/// `RunState` and direct `tokio::spawn` in the `send` handler.
pub struct AgentJob {
    pub thread_id: String,
    pub content: String,
}

// ─── RunningTurn ──────────────────────────────────────────────────────────────

/// Holds the handle and cancellation channel for a single active agent turn.
pub struct RunningTurn {
    pub task: JoinHandle<()>,
    pub cancellation_tx: tokio::sync::watch::Sender<bool>,
    pub thread_id: String,
    pub turn_id: uuid::Uuid,
}

impl RunningTurn {
    /// Signal the task to cancel cooperatively and return the JoinHandle
    /// so the caller can await its completion.
    pub fn cancel(self) -> JoinHandle<()> {
        // Harmless if the receiver was already dropped (task already done).
        let _ = self.cancellation_tx.send(true);
        self.task
    }
}

// ─── Per-thread run state ─────────────────────────────────────────────────────

/// Per-thread run state. Controls concurrency and task lifecycle for agent
/// runs on a single thread.
///
/// - `semaphore`: capacity-1 semaphore that serialises concurrent runs.
///   The second run waits behind the first rather than being rejected.
/// - `depth`: count of tasks that have been spawned and not yet completed.
///   Used to reject sends that would exceed `MAX_DEPTH`.
/// - `running_turn`: the currently active `RunningTurn`, if any. `cancel_run`
///   takes this slot and calls `cancel()` to send the cooperative shutdown
///   signal, then awaits the handle.
pub struct RunState {
    pub semaphore: tokio::sync::Semaphore,
    pub depth: AtomicUsize,
    pub running_turn: tokio::sync::Mutex<Option<RunningTurn>>,
}

impl RunState {
    pub fn new() -> Self {
        Self {
            semaphore: tokio::sync::Semaphore::new(1),
            depth: AtomicUsize::new(0),
            running_turn: tokio::sync::Mutex::new(None),
        }
    }
}

/// Application state shared across all handlers via Axum's `State` extractor.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
    pub machine_secret: String,
    pub credential_master_key: String,
    pub auth_token: Arc<RwLock<String>>,
    /// Global SSE broadcast channel.  All connected global-stream clients
    /// subscribe via `global_tx.subscribe()`.
    pub global_tx: broadcast::Sender<GlobalEvent>,
    /// Per-thread SSE senders.  Keyed by thread ID; each value is a list of
    /// senders — one per currently connected client.
    pub thread_senders: Arc<
        Mutex<HashMap<String, Vec<tokio::sync::mpsc::Sender<crate::routes::sse::ThreadEvent>>>>,
    >,
    /// Per-thread run state map.  Created on first access for each thread.
    pub run_states: DashMap<String, Arc<RunState>>,
    /// Handle to the copilot-api side-car service.  `None` when the service
    /// could not be started (e.g. `node` not on PATH).
    pub copilot: Option<CopilotApiService>,
    /// MCP connection pool.  Manages all local and remote MCP server connections,
    /// tool caching, and status broadcasting.
    pub mcp: McpConnectionManager,
    /// Statically-registered built-in tools available to every agent run.
    /// MCP tools are discovered dynamically and routed separately.
    pub built_in_tools: Arc<Vec<Arc<dyn AgentTool>>>,
    /// Sender half of the scheduler command channel.
    /// Route handlers send commands here; the background `SchedulerService`
    /// task receives on the other end and reacts by registering or removing
    /// cron jobs.
    pub scheduler_tx: tokio::sync::mpsc::Sender<SchedulerCommand>,
    /// VAPID public key (base64url, no padding). Cached from app_config at startup.
    /// Served by GET /api/push/vapid-public-key without a DB round-trip.
    pub vapid_public_key: String,
    /// PKCS8 PEM-encoded VAPID private key. Loaded from app_config at startup.
    /// Used by the push dispatch service. Never logged or returned by any endpoint.
    pub vapid_private_pem: String,
}

impl AppState {
    /// Get or create the `RunState` for a given thread.
    pub fn get_run_state(&self, thread_id: &str) -> Arc<RunState> {
        self.run_states
            .entry(thread_id.to_string())
            .or_insert_with(|| Arc::new(RunState::new()))
            .clone()
    }
}

/// Build the main application router.
pub async fn build_router(
    pool: SqlitePool,
    config: Config,
) -> anyhow::Result<(Router, McpConnectionManager)> {
    // Initialize auth token and machine secret (generates on first run)
    let auth_token = auth_service::get_or_create_auth_token(&pool).await?;
    let machine_secret = auth_service::get_or_create_machine_secret(&pool).await?;

    // Print auth token to terminal so the user can access remote clients
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║                   agent-deck is running                 ║");
    tracing::info!("║                                                          ║");
    tracing::info!("║  Auth token: {}  ║", &auth_token[..32]);
    tracing::info!("║             {}  ║", &auth_token[32..]);
    tracing::info!("║                                                          ║");
    tracing::info!("║  Copy this token to authenticate from remote devices.   ║");
    tracing::info!("╚══════════════════════════════════════════════════════════╝");

    // ── SSE broadcast channel ─────────────────────────────────────────────────
    let (global_tx, _global_rx) = broadcast::channel::<GlobalEvent>(256);

    // ── copilot-api side-car ──────────────────────────────────────────────────
    let copilot = CopilotApiService::new(global_tx.clone());
    let copilot_handle = copilot.clone();

    // ── MCP connection manager ────────────────────────────────────────────────
    let master_key = credentials_service::get_or_create_master_key(&pool).await?;

    // ── VAPID key pair ────────────────────────────────────────────────────────
    let (vapid_private_pem, vapid_public_key) =
        crate::services::vapid::get_or_create_vapid_keys(&pool).await?;
    tracing::info!("vapid: public key loaded ({}…)", &vapid_public_key[..8]);
    let mcp = McpConnectionManager::new(
        pool.clone(),
        master_key.clone(),
        global_tx.clone(),
        config.mcp_dir.clone(),
    );

    // ── Scheduler command channel ─────────────────────────────────────────────
    let (scheduler_tx, scheduler_rx) = tokio::sync::mpsc::channel::<SchedulerCommand>(256);

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        machine_secret,
        credential_master_key: master_key.clone(),
        auth_token: Arc::new(RwLock::new(auth_token)),
        global_tx,
        thread_senders: Arc::new(Mutex::new(HashMap::new())),
        run_states: DashMap::new(),
        copilot: Some(copilot_handle),
        mcp: mcp.clone(),
        built_in_tools: Arc::new(tools_service::built_in_tools()),
        scheduler_tx,
        vapid_public_key,
        vapid_private_pem,
    });

    // Start copilot-api supervision in the background.
    copilot.start();

    // Connect all enabled MCP servers.
    mcp.start().await;

    // Start the routine scheduler in the background.
    let scheduler =
        crate::services::scheduler::SchedulerService::new(state.pool.clone(), state.clone())
            .await?;
    let scheduler_arc = scheduler.clone();
    tokio::spawn(async move {
        scheduler_arc.start(scheduler_rx).await;
    });

    // Public API routes (no auth required)
    let public_api = Router::new()
        .route("/api/setup/status", get(setup::get_status))
        .route("/api/setup/complete", axum::routing::post(setup::complete))
        .route("/api/auth/token", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .route("/health", get(health::health_check))
        .route(
            "/api/push/vapid-public-key",
            get(push::get_vapid_public_key),
        );

    // Protected API routes (auth required)
    let protected_api = Router::new()
        // Providers
        .route(
            "/api/providers",
            get(providers::list).post(providers::create),
        )
        .route(
            "/api/providers/:id",
            get(providers::get)
                .put(providers::update)
                .delete(providers::delete),
        )
        .route(
            "/api/providers/:id/test",
            axum::routing::post(providers::test_connection),
        )
        // Models
        .route(
            "/api/providers/:id/models",
            get(models::list).post(models::sync),
        )
        .route(
            "/api/providers/:provider_id/models/:model_id",
            get(models::get).put(models::update).delete(models::delete),
        )
        // Agent Personas
        .route("/api/personas", get(personas::list).post(personas::create))
        .route(
            "/api/personas/:id",
            get(personas::get)
                .put(personas::update)
                .delete(personas::delete),
        )
        .route(
            "/api/personas/:id/avatar",
            axum::routing::post(personas::upload_avatar),
        )
        // Credentials
        .route(
            "/api/credentials",
            get(credentials::list).post(credentials::create),
        )
        .route(
            "/api/credentials/:id",
            get(credentials::get)
                .put(credentials::update)
                .delete(credentials::delete),
        )
        // MCP Servers
        .route(
            "/api/mcp-servers",
            get(tokens::list_mcp).post(tokens::create_mcp),
        )
        .route(
            "/api/mcp-servers/:id",
            get(tokens::get_mcp)
                .put(tokens::update_mcp)
                .delete(tokens::delete_mcp),
        )
        .route("/api/mcp-servers/:id/tools", get(tokens::list_mcp_tools))
        // Threads
        .route("/api/threads", get(threads::list).post(threads::create))
        .route(
            "/api/threads/:id",
            get(threads::get)
                .put(threads::update)
                .delete(threads::delete),
        )
        .route(
            "/api/threads/:id/generate-title",
            axum::routing::post(threads::generate_title),
        )
        .route(
            "/api/threads/:id/archive",
            axum::routing::post(threads::archive),
        )
        .route(
            "/api/threads/:id/unarchive",
            axum::routing::post(threads::unarchive),
        )
        .route(
            "/api/threads/:id/mcp-servers",
            get(threads::list_mcp_servers).post(threads::attach_mcp),
        )
        .route(
            "/api/threads/:id/mcp-servers/:mcp_id",
            axum::routing::delete(threads::detach_mcp),
        )
        // Messages
        .route(
            "/api/threads/:id/messages",
            get(messages::list).post(messages::send),
        )
        .route(
            "/api/threads/:id/command",
            axum::routing::post(messages::slash_command),
        )
        // Run management
        .route(
            "/api/threads/:id/cancel",
            axum::routing::post(messages::cancel_run),
        )
        .route(
            "/api/threads/:id/notify",
            axum::routing::post(notify::notify),
        )
        // SSE streams
        .route("/api/threads/:id/stream", get(sse::thread_stream))
        .route("/api/events", get(sse::global_stream))
        // Routines
        .route(
            "/api/threads/:id/routines",
            get(routines::list).post(routines::create),
        )
        .route(
            "/api/threads/:thread_id/routines/:routine_id",
            get(routines::get)
                .put(routines::update)
                .delete(routines::delete),
        )
        .route(
            "/api/threads/:thread_id/routines/:routine_id/toggle",
            axum::routing::patch(routines::toggle),
        )
        // Memory
        .route(
            "/api/personas/:id/memory",
            get(memory::list).post(memory::create),
        )
        .route(
            "/api/personas/:persona_id/memory/:memory_id",
            axum::routing::delete(memory::delete),
        )
        .route(
            "/api/personas/:id/memory/search",
            axum::routing::post(memory::search),
        )
        // Device tokens (push notifications)
        .route(
            "/api/device-tokens",
            axum::routing::post(tokens::register_device).delete(tokens::unregister_device),
        )
        // Push subscriptions (Web Push / VAPID)
        .route(
            "/api/push/subscribe",
            axum::routing::post(push::subscribe).delete(push::unsubscribe),
        )
        // App config
        .route("/api/config", get(config::get_config))
        .route(
            "/api/auth/token/rotate",
            axum::routing::post(auth::rotate_token),
        )
        // User profile
        .route(
            "/api/profile",
            get(profile::get_profile).put(profile::update_profile),
        )
        // Filesystem explorer
        .route("/api/fs/list", get(fs::list_directory))
        .route("/api/fs/read", get(fs::read_file))
        .route("/api/fs/workspace", get(fs::get_workspace))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Static file serving (React SPA) — serves from public_dir
    let static_files = Router::new().nest_service(
        "/",
        ServeDir::new(&config.public_dir).not_found_service(tower_http::services::ServeFile::new(
            format!("{}/index.html", config.public_dir),
        )),
    );

    // ── Copilot provider auth routes — genuinely public, no auth required ──
    let copilot_routes = Router::new()
        .route(
            "/api/providers/copilot/auth-status",
            get(providers::copilot_auth_status),
        )
        .route(
            "/api/providers/copilot/auth-start",
            axum::routing::post(providers::copilot_auth_start),
        )
        .route(
            "/api/providers/copilot/auth-poll",
            axum::routing::post(providers::copilot_auth_poll),
        )
        .route(
            "/api/providers/copilot/models",
            get(providers::copilot_models),
        );

    let app = Router::new()
        .merge(public_api)
        .merge(protected_api)
        .merge(copilot_routes)
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    Ok((app, mcp))
}

/// Auth middleware: validates Bearer token or session cookie.
/// Localhost requests (127.0.0.1 / ::1) bypass auth entirely.
async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let connection_info = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0);

    // Localhost bypass
    if let Some(addr) = connection_info {
        let ip = addr.ip();
        if ip.is_loopback() {
            return Ok(next.run(request).await);
        }
    }

    // Check Authorization: Bearer <token> header
    if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                if token == *state.auth_token.read().unwrap() {
                    return Ok(next.run(request).await);
                } else {
                    warn!("Invalid Bearer token presented");
                    return Err(AppError::Unauthorized);
                }
            }
        }
    }

    // Check agent_deck_session cookie
    if let Some(cookie_header) = request.headers().get(header::COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie_part in cookie_str.split(';') {
                let cookie_part = cookie_part.trim();
                if let Some(value) = cookie_part.strip_prefix("agent_deck_session=") {
                    if value == *state.auth_token.read().unwrap() {
                        return Ok(next.run(request).await);
                    } else {
                        warn!("Invalid session cookie presented");
                        return Err(AppError::Unauthorized);
                    }
                }
            }
        }
    }

    Err(AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::sync::atomic::Ordering;
    use tower::ServiceExt;

    async fn test_app() -> (Router, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let config = Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".to_string(),
            public_dir: "./public".to_string(),
            fcm_service_account_json: None,
        };

        let token = auth_service::get_or_create_auth_token(&pool)
            .await
            .expect("token");
        let (app, _mcp) = build_router(pool, config).await.expect("router");
        (app, token)
    }

    #[tokio::test]
    async fn test_health_is_public() {
        let (app, _token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_protected_route_requires_auth() {
        let (app, _token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_valid_bearer_token_passes() {
        let (app, token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_invalid_bearer_token_rejected() {
        let (app, _token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .header("Authorization", "Bearer wrong-token-value")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_valid_session_cookie_passes() {
        let (app, token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .header("Cookie", format!("agent_deck_session={}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_setup_status_is_public() {
        let (app, _token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/setup/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn run_state_depth_increments() {
        let state = RunState::new();
        let prev = state.depth.fetch_add(1, Ordering::SeqCst);
        assert_eq!(prev, 0);
        assert_eq!(state.depth.load(Ordering::SeqCst), 1);
    }

    // ── RunState additional unit tests ────────────────────────────────────────

    #[test]
    fn run_state_new_has_zero_depth() {
        let state = RunState::new();
        assert_eq!(state.depth.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn run_state_semaphore_has_one_permit() {
        let state = RunState::new();
        // A freshly created Semaphore(1) should report 1 available permit.
        assert_eq!(state.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn run_state_semaphore_blocks_second_acquire() {
        use std::sync::Arc;
        use tokio::sync::Semaphore;
        use tokio::time::{timeout, Duration};

        // Use a standalone Arc<Semaphore> so the permit's lifetime is tied to
        // the Arc, not to a RunState owned inside the async block.
        let sem = Arc::new(Semaphore::new(1));

        // Acquire the single permit — holds it for the duration of the test.
        let _permit = sem.acquire().await.unwrap();

        // A second acquire on a capacity-1 semaphore that is already held
        // must block.  Wrap it in a short timeout to prove it never succeeds.
        let sem2 = Arc::clone(&sem);
        let result = timeout(Duration::from_millis(50), async move {
            // Drive the acquire but drop the permit inside the block so
            // nothing is returned that would reference sem2.
            let _p = sem2.acquire().await.unwrap();
            drop(_p);
        })
        .await;

        assert!(
            result.is_err(),
            "second semaphore acquire should have timed out (semaphore is at capacity)"
        );
    }

    // ── RunningTurn initial state test ───────────────────────────────────────

    #[tokio::test]
    async fn run_state_running_turn_is_none_initially() {
        let state = RunState::new();
        let slot = state.running_turn.lock().await;
        assert!(
            slot.is_none(),
            "running_turn must be None on a freshly created RunState"
        );
    }

    #[tokio::test]
    async fn get_run_state_returns_same_arc_for_same_thread_id() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        use tokio::sync::broadcast;

        let (global_tx, _) = broadcast::channel(64);
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let (mcp_tx, _) = tokio::sync::broadcast::channel(1);
        let mcp = crate::services::mcp::McpConnectionManager::new(
            pool.clone(),
            "test-master-key".to_string(),
            mcp_tx,
            std::path::PathBuf::from("/tmp/test-deck/mcp"),
        );
        let app_state = AppState {
            pool,
            config: Config {
                port: 7474,
                data_dir: std::path::PathBuf::from("/tmp/test-deck"),
                mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
                personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
                database_url: "sqlite::memory:".to_string(),
                public_dir: "./public".to_string(),
                fcm_service_account_json: None,
            },
            machine_secret: "test-secret".to_string(),
            credential_master_key: "test-master-key".to_string(),
            auth_token: Arc::new(RwLock::new("test-token".to_string())),
            global_tx,
            thread_senders: Arc::new(Mutex::new(HashMap::new())),
            run_states: dashmap::DashMap::new(),
            copilot: None,
            mcp,
            built_in_tools: std::sync::Arc::new(vec![]),
            scheduler_tx: tokio::sync::mpsc::channel(1).0,
            vapid_public_key: String::new(),
            vapid_private_pem: String::new(),
        };

        let rs1 = app_state.get_run_state("thread-abc");
        let rs2 = app_state.get_run_state("thread-abc");

        // Both calls with the same thread_id must return the exact same Arc.
        assert!(
            Arc::ptr_eq(&rs1, &rs2),
            "get_run_state must return the same Arc for the same thread_id"
        );
    }

    #[tokio::test]
    async fn get_run_state_returns_different_arc_for_different_threads() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        use tokio::sync::broadcast;

        let (global_tx, _) = broadcast::channel(64);
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let (mcp_tx, _) = tokio::sync::broadcast::channel(1);
        let mcp = crate::services::mcp::McpConnectionManager::new(
            pool.clone(),
            "test-master-key".to_string(),
            mcp_tx,
            std::path::PathBuf::from("/tmp/test-deck/mcp"),
        );
        let app_state = AppState {
            pool,
            config: Config {
                port: 7474,
                data_dir: std::path::PathBuf::from("/tmp/test-deck"),
                mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
                personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
                database_url: "sqlite::memory:".to_string(),
                public_dir: "./public".to_string(),
                fcm_service_account_json: None,
            },
            machine_secret: "test-secret".to_string(),
            credential_master_key: "test-master-key".to_string(),
            auth_token: Arc::new(RwLock::new("test-token".to_string())),
            global_tx,
            thread_senders: Arc::new(Mutex::new(HashMap::new())),
            run_states: dashmap::DashMap::new(),
            copilot: None,
            mcp,
            built_in_tools: std::sync::Arc::new(vec![]),
            scheduler_tx: tokio::sync::mpsc::channel(1).0,
            vapid_public_key: String::new(),
            vapid_private_pem: String::new(),
        };

        let rs_a = app_state.get_run_state("thread-aaa");
        let rs_b = app_state.get_run_state("thread-bbb");

        // Different thread IDs must produce distinct Arc instances.
        assert!(
            !Arc::ptr_eq(&rs_a, &rs_b),
            "get_run_state must return distinct Arcs for different thread IDs"
        );
    }

    #[tokio::test]
    async fn arc_appstate_clones_share_run_states_dashmap() {
        // This test verifies the core fix for cancellation not working.
        //
        // Before the fix: AppState was cloned on every request via Axum's State
        // extractor.  DashMap::clone() performs a deep copy, so the RunState
        // inserted by POST /messages was invisible to POST /cancel — each
        // request was operating on its own disconnected map.
        //
        // After the fix: the state type is Arc<AppState>.  Cloning the Arc just
        // bumps the reference count; both clones point at the same DashMap, so
        // a RunState registered by one "request" is immediately visible to the
        // other — exactly what cancel needs.
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        use tokio::sync::broadcast;

        let (global_tx, _) = broadcast::channel(64);
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let (mcp_tx, _) = tokio::sync::broadcast::channel(1);
        let mcp = crate::services::mcp::McpConnectionManager::new(
            pool.clone(),
            "test-master-key".to_string(),
            mcp_tx,
            std::path::PathBuf::from("/tmp/test-deck/mcp"),
        );
        let shared_state = Arc::new(AppState {
            pool,
            config: Config {
                port: 7474,
                data_dir: std::path::PathBuf::from("/tmp/test-deck"),
                mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
                personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
                database_url: "sqlite::memory:".to_string(),
                public_dir: "./public".to_string(),
                fcm_service_account_json: None,
            },
            machine_secret: "test-secret".to_string(),
            credential_master_key: "test-master-key".to_string(),
            auth_token: Arc::new(RwLock::new("test-token".to_string())),
            global_tx,
            thread_senders: Arc::new(Mutex::new(HashMap::new())),
            run_states: dashmap::DashMap::new(),
            copilot: None,
            mcp,
            built_in_tools: std::sync::Arc::new(vec![]),
            scheduler_tx: tokio::sync::mpsc::channel(1).0,
            vapid_public_key: String::new(),
            vapid_private_pem: String::new(),
        });

        // Simulate two requests receiving their own clone of the Arc —
        // this is exactly what Axum's State extractor does per request.
        let send_request_state = shared_state.clone();
        let cancel_request_state = shared_state.clone();

        // "POST /messages" registers a RunState for the thread.
        let run_state_from_send = send_request_state.get_run_state("thread-xyz");

        // "POST /cancel" looks up the same thread — must find the same Arc.
        let run_state_from_cancel = cancel_request_state.get_run_state("thread-xyz");

        assert!(
            Arc::ptr_eq(&run_state_from_send, &run_state_from_cancel),
            "send and cancel requests must see the same RunState — \
             DashMap must be shared via Arc, not deep-cloned per request"
        );
    }
}

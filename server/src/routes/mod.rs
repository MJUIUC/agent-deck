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
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::warn;

use crate::config::Config;
use crate::error::AppError;
use crate::services::auth as auth_service;
use crate::services::copilot::{CopilotApiService, GlobalEvent};
use crate::services::credentials as credentials_service;
use crate::services::mcp::McpConnectionManager;

pub mod auth;
pub mod config;
pub mod credentials;
pub mod health;
pub mod memory;
pub mod messages;
pub mod models;
pub mod notify;
pub mod personas;
pub mod providers;
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

/// Per-thread run state.  Controls concurrency and cancellation for agent
/// runs on a single thread.
///
/// - `semaphore`: capacity-1 semaphore that serialises concurrent runs.
///   The second run waits behind the first rather than being rejected.
/// - `depth`: count of tasks that have been spawned and not yet completed.
///   Used to reject sends that would exceed `MAX_DEPTH`.
/// - `cancel_token`: the cancellation token for the currently running (or
///   most recently started) agent run.  Replaced atomically before each run.
pub struct RunState {
    pub semaphore: tokio::sync::Semaphore,
    pub depth: AtomicUsize,
    pub cancel_token: tokio::sync::Mutex<CancellationToken>,
}

impl RunState {
    pub fn new() -> Self {
        Self {
            semaphore: tokio::sync::Semaphore::new(1),
            depth: AtomicUsize::new(0),
            cancel_token: tokio::sync::Mutex::new(CancellationToken::new()),
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
    pub auth_token: String,
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
    /// could not be started (e.g. `bun` not on PATH).
    pub copilot: Option<CopilotApiService>,
    /// MCP connection pool.  Manages all local and remote MCP server connections,
    /// tool caching, and status broadcasting.
    pub mcp: McpConnectionManager,
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
    let mcp = McpConnectionManager::new(
        pool.clone(),
        master_key.clone(),
        global_tx.clone(),
        config.mcp_dir.clone(),
    );

    let state = AppState {
        pool,
        config: config.clone(),
        machine_secret,
        credential_master_key: master_key.clone(),
        auth_token,
        global_tx,
        thread_senders: Arc::new(Mutex::new(HashMap::new())),
        run_states: DashMap::new(),
        copilot: Some(copilot_handle),
        mcp: mcp.clone(),
    };

    // Start copilot-api supervision in the background.
    copilot.start();

    // Connect all enabled MCP servers.
    mcp.start().await;

    // Public API routes (no auth required)
    let public_api = Router::new()
        .route("/api/setup/status", get(setup::get_status))
        .route("/api/setup/complete", axum::routing::post(setup::complete))
        .route("/api/auth/token", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .route("/health", get(health::health_check));

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
        // Mobile pairing
        .route(
            "/api/pairing/generate",
            axum::routing::post(tokens::generate_pairing),
        )
        .route(
            "/api/pairing/complete",
            axum::routing::post(tokens::complete_pairing),
        )
        // App config
        .route("/api/config", get(config::get_config))
        .route(
            "/api/auth/token/rotate",
            axum::routing::post(auth::rotate_token),
        )
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
    axum::extract::State(state): axum::extract::State<AppState>,
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
                if token == state.auth_token {
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
                    if value == state.auth_token {
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

    #[tokio::test]
    async fn run_state_cancel_token_can_be_replaced() {
        let state = RunState::new();
        let new_token = CancellationToken::new();
        {
            let mut lock = state.cancel_token.lock().await;
            *lock = new_token.clone();
        }
        new_token.cancel();
        let lock = state.cancel_token.lock().await;
        assert!(lock.is_cancelled());
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

    #[tokio::test]
    async fn run_state_cancel_triggers_token() {
        let state = RunState::new();

        // Grab the current token and cancel it.
        let token = {
            let lock = state.cancel_token.lock().await;
            lock.clone()
        };

        assert!(
            !token.is_cancelled(),
            "token should not be cancelled initially"
        );
        token.cancel();
        assert!(
            token.is_cancelled(),
            "token should be cancelled after cancel()"
        );

        // The value stored inside the Mutex reflects the cancellation.
        let lock = state.cancel_token.lock().await;
        assert!(lock.is_cancelled());
    }

    #[tokio::test]
    async fn run_state_replace_token_old_token_not_affected() {
        let state = RunState::new();

        // Capture a clone of the original token before replacing it.
        let old_token = {
            let lock = state.cancel_token.lock().await;
            lock.clone()
        };

        // Replace with a fresh token.
        let new_token = CancellationToken::new();
        {
            let mut lock = state.cancel_token.lock().await;
            *lock = new_token.clone();
        }

        // Cancel the new token — the old one must remain unaffected.
        new_token.cancel();
        assert!(new_token.is_cancelled(), "new token should be cancelled");
        assert!(
            !old_token.is_cancelled(),
            "old token must not be affected by cancelling the replacement"
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
            auth_token: "test-token".to_string(),
            global_tx,
            thread_senders: Arc::new(Mutex::new(HashMap::new())),
            run_states: dashmap::DashMap::new(),
            copilot: None,
            mcp,
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
            auth_token: "test-token".to_string(),
            global_tx,
            thread_senders: Arc::new(Mutex::new(HashMap::new())),
            run_states: dashmap::DashMap::new(),
            copilot: None,
            mcp,
        };

        let rs_a = app_state.get_run_state("thread-aaa");
        let rs_b = app_state.get_run_state("thread-bbb");

        // Different thread IDs must produce distinct Arc instances.
        assert!(
            !Arc::ptr_eq(&rs_a, &rs_b),
            "get_run_state must return distinct Arcs for different thread IDs"
        );
    }
}

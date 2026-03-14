use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::warn;

use crate::config::Config;
use crate::error::AppError;
use crate::services::auth as auth_service;
use crate::services::copilot::{CopilotApiService, GlobalEvent};

pub mod auth;
pub mod config;
pub mod credentials;
pub mod health;
pub mod memory;
pub mod messages;
pub mod models;
pub mod personas;
pub mod providers;
pub mod routines;
pub mod setup;

pub mod sse;
pub mod threads;
pub mod tokens;

/// A unit of work sent from the `send` message handler to the background
/// agent worker.  Decouples the HTTP response lifecycle from agent execution —
/// the handler enqueues the job and returns the 201 immediately; the worker
/// drains the queue independently.
pub struct AgentJob {
    pub thread_id: String,
    pub content: String,
}

/// Application state shared across all handlers via Axum's `State` extractor.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
    pub machine_secret: String,
    pub auth_token: String,
    /// Global SSE broadcast channel.  All connected global-stream clients
    /// subscribe via `global_tx.subscribe()`.
    pub global_tx: broadcast::Sender<GlobalEvent>,
    /// Per-thread SSE senders.  Keyed by thread ID; each value is a list of
    /// senders — one per currently connected client.
    pub thread_senders: Arc<
        Mutex<HashMap<String, Vec<tokio::sync::mpsc::Sender<crate::routes::sse::ThreadEvent>>>>,
    >,
    /// Agent work queue.  The `send` handler drops a job here and returns the
    /// HTTP 201 immediately.  A single background task drains this channel and
    /// runs each agent job in its own spawned task, keeping the queue itself
    /// non-blocking.
    pub agent_tx: mpsc::Sender<AgentJob>,
    /// Handle to the copilot-api side-car service.  `None` when the service
    /// could not be started (e.g. `bun` not on PATH).
    pub copilot: Option<CopilotApiService>,
}

/// Build the main application router.
/// Spawn the background task that drains the agent work queue.
///
/// Each job is run in its own `tokio::spawn` so that slow or failing agent
/// runs never block the queue from processing subsequent jobs.
fn start_agent_worker(mut rx: mpsc::Receiver<AgentJob>, state: AppState) {
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            let job_state = state.clone();
            tokio::spawn(async move {
                // Wait up to 3 seconds for the SSE client to connect before
                // starting to stream.  If no subscriber appears in time we
                // proceed anyway — message_complete and DB persistence still
                // happen so the user sees the response on next load.
                job_state
                    .wait_for_subscriber(&job.thread_id, std::time::Duration::from_secs(3))
                    .await;
                crate::services::agent::run(job_state, job.thread_id, job.content).await;
            });
        }
    });
}

pub async fn build_router(pool: SqlitePool, config: Config) -> anyhow::Result<Router> {
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
    // Capacity of 256 gives global-stream clients some slack before they start
    // lagging on bursts of events.
    let (global_tx, _global_rx) = broadcast::channel::<GlobalEvent>(256);

    // ── copilot-api side-car ──────────────────────────────────────────────────
    let copilot = CopilotApiService::new(global_tx.clone());
    let copilot_handle = copilot.clone();

    // Agent work queue — capacity of 64 is generous; in practice there will
    // rarely be more than a handful of concurrent agent runs.
    let (agent_tx, agent_rx) = mpsc::channel::<AgentJob>(64);

    let state = AppState {
        pool,
        config: config.clone(),
        machine_secret,
        auth_token,
        global_tx,
        thread_senders: Arc::new(Mutex::new(HashMap::new())),
        agent_tx,
        copilot: Some(copilot_handle),
    };

    // Start the agent worker before the router starts accepting requests.
    start_agent_worker(agent_rx, state.clone());

    // Start copilot-api supervision in the background.
    // This is non-blocking — the server starts immediately regardless of whether
    // copilot-api becomes available.
    copilot.start();

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
    // Falls back to index.html for SPA routing (404 → index.html)
    let static_files = Router::new().nest_service(
        "/",
        ServeDir::new(&config.public_dir).not_found_service(tower_http::services::ServeFile::new(
            format!("{}/index.html", config.public_dir),
        )),
    );

    // ── Copilot provider auth routes — genuinely public, no auth required ──
    // These endpoints drive the GitHub device-code flow from the UI, which
    // may be used before the user has a session token.
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

    Ok(app)
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
            database_url: "sqlite::memory:".to_string(),
            public_dir: "./public".to_string(),
            fcm_service_account_json: None,
        };

        let token = auth_service::get_or_create_auth_token(&pool)
            .await
            .expect("token");
        // build_router starts copilot supervision — that's fine in tests,
        // it will fail to spawn bun (not installed in CI) and back off quietly.
        let app = build_router(pool, config).await.expect("router");
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
        // 200 or other non-401 is fine — we just need auth to pass
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
}

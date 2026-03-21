use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    routes::AppState,
    services::auth as auth_service,
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub data: LoginData,
}

#[derive(Debug, Serialize)]
pub struct LoginData {
    pub valid: bool,
}

/// POST /api/auth/token
///
/// Validates the provided token against the stored auth token.
/// On success, sets an httpOnly session cookie and returns `{ "data": { "valid": true } }`.
/// On failure, returns 401.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    let valid = auth_service::validate_token(&state.pool, &payload.token).await?;

    if !valid {
        return Err(AppError::Unauthorized);
    }

    // Build the Set-Cookie header for the httpOnly session cookie.
    // SameSite=Lax is appropriate — the app is accessed over Tailscale, not
    // via cross-site navigation, so Strict would also work but Lax is safer
    // for future redirect-based flows.
    let cookie_value = format!(
        "agent_deck_session={}; HttpOnly; Path=/; SameSite=Lax",
        payload.token
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        cookie_value.parse().map_err(|_| {
            AppError::Internal(anyhow::anyhow!("Failed to build Set-Cookie header"))
        })?,
    );

    Ok((
        StatusCode::OK,
        headers,
        Json(json!({
            "data": { "valid": true }
        })),
    ))
}

/// POST /api/auth/logout
///
/// Clears the session cookie by setting it to an empty value with an immediate
/// expiry. Returns 200 regardless of whether a cookie was present.
pub async fn logout() -> impl IntoResponse {
    let clear_cookie = "agent_deck_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0";

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        clear_cookie.parse().expect("static cookie string is valid"),
    );

    (
        StatusCode::OK,
        headers,
        Json(json!({
            "data": { "logged_out": true }
        })),
    )
}

/// POST /api/auth/token/rotate
///
/// Generates a new auth token, invalidating the old one.
/// All existing sessions (cookies) and mobile pairings using the old token
/// will stop working immediately after this call.
///
/// The new token is returned in the response body AND logged to the terminal.
/// The caller is responsible for updating any connected devices.
pub async fn rotate_token(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let new_token = auth_service::rotate_auth_token(&state.pool).await?;
    *state.auth_token.write().unwrap() = new_token.clone();

    tracing::info!(
        "Auth token rotated. New token: {}...{} (see full token in response)",
        &new_token[..8],
        &new_token[56..]
    );
    tracing::info!("New auth token: {}", new_token);

    // Clear the session cookie since the old token is now invalid.
    let clear_cookie = "agent_deck_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0";

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        clear_cookie.parse().expect("static cookie string is valid"),
    );

    Ok((
        StatusCode::OK,
        headers,
        Json(json!({
            "data": {
                "token": new_token,
                "message": "Token rotated. All existing sessions have been invalidated. Re-authenticate on all devices."
            }
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    #[tokio::test]
    async fn test_login_with_valid_token_sets_cookie() {
        let pool = test_pool().await;
        let token = auth_service::get_or_create_auth_token(&pool)
            .await
            .expect("token");

        let config = crate::config::Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".into(),
            public_dir: "./public".into(),
            fcm_service_account_json: None,
        };

        let (app, _mcp) = crate::routes::build_router(pool, config)
            .await
            .expect("router");

        let body = format!(r#"{{"token":"{}"}}"#, token);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("Set-Cookie header should be present");

        let cookie_str = set_cookie.to_str().unwrap();
        assert!(
            cookie_str.contains("agent_deck_session="),
            "cookie name should be agent_deck_session"
        );
        assert!(cookie_str.contains("HttpOnly"), "cookie should be HttpOnly");
        assert!(
            cookie_str.contains(&token),
            "cookie should contain the token"
        );
    }

    #[tokio::test]
    async fn test_login_with_invalid_token_returns_401() {
        let pool = test_pool().await;
        let _token = auth_service::get_or_create_auth_token(&pool)
            .await
            .expect("token");

        let config = crate::config::Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".into(),
            public_dir: "./public".into(),
            fcm_service_account_json: None,
        };

        let (app, _mcp) = crate::routes::build_router(pool, config)
            .await
            .expect("router");

        let body = r#"{"token":"definitely-not-the-right-token"}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_logout_clears_cookie() {
        let pool = test_pool().await;
        let token = auth_service::get_or_create_auth_token(&pool)
            .await
            .expect("token");

        let config = crate::config::Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".into(),
            public_dir: "./public".into(),
            fcm_service_account_json: None,
        };

        let (app, _mcp) = crate::routes::build_router(pool, config)
            .await
            .expect("router");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .header("Cookie", format!("agent_deck_session={}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("Set-Cookie should be present on logout");
        let cookie_str = set_cookie.to_str().unwrap();
        assert!(
            cookie_str.contains("Max-Age=0"),
            "logout should expire the cookie"
        );
    }

    #[tokio::test]
    async fn test_rotate_token_updates_in_memory_state() {
        // This test verifies the Arc<RwLock<String>> fix: rotating the token
        // must update the shared in-memory state so the auth middleware
        // immediately enforces the new token — no server restart required.
        let pool = test_pool().await;
        let old_token = auth_service::get_or_create_auth_token(&pool)
            .await
            .expect("token");

        let config = crate::config::Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".into(),
            public_dir: "./public".into(),
            fcm_service_account_json: None,
        };

        let (app, _mcp) = crate::routes::build_router(pool, config)
            .await
            .expect("router");

        // ── Step 1: old token passes auth ─────────────────────────────────────
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .header("Authorization", format!("Bearer {}", old_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "old token should be valid before rotation"
        );

        // ── Step 2: rotate the token ──────────────────────────────────────────
        let rotate_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/token/rotate")
                    .header("Authorization", format!("Bearer {}", old_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            rotate_resp.status(),
            StatusCode::OK,
            "rotate endpoint should succeed"
        );

        let body_bytes = axum::body::to_bytes(rotate_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let new_token = json["data"]["token"]
            .as_str()
            .expect("response must contain new token")
            .to_string();

        assert_ne!(
            new_token, old_token,
            "rotated token must differ from old one"
        );

        // ── Step 3: new token passes auth — RwLock was updated in-memory ──────
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .header("Authorization", format!("Bearer {}", new_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "new token should pass auth immediately after rotation (no restart needed)"
        );

        // ── Step 4: old token is now rejected ─────────────────────────────────
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/providers")
                    .header("Authorization", format!("Bearer {}", old_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "old token must be rejected after rotation"
        );
    }
}

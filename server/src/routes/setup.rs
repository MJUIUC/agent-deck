use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::{app_config::keys, user::User},
    routes::AppState,
};

#[derive(Debug, Serialize)]
pub struct SetupStatusResponse {
    pub complete: bool,
}

#[derive(Debug, Deserialize)]
pub struct CompleteSetupRequest {
    pub display_name: String,
}

/// GET /api/setup/status
///
/// Returns whether the first-run setup wizard has been completed.
/// This endpoint is public (no auth required) so the setup wizard can
/// check state before any token is configured.
pub async fn get_status(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await?;

    let complete = user_count.0 > 0;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": { "complete": complete }
        })),
    ))
}

/// POST /api/setup/complete
///
/// Completes the first-run setup by creating the user row and marking
/// setup as done in app_config. Idempotent — calling it again after setup
/// is already complete returns an error rather than creating a second user.
pub async fn complete(
    State(state): State<AppState>,
    Json(payload): Json<CompleteSetupRequest>,
) -> AppResult<impl IntoResponse> {
    if payload.display_name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "display_name must not be empty".to_string(),
        ));
    }

    // Guard: refuse if setup is already complete
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await?;

    if user_count.0 > 0 {
        return Err(AppError::Conflict("Setup is already complete".to_string()));
    }

    // Create the single user row
    let user = User::new(payload.display_name.trim());

    sqlx::query("INSERT INTO users (id, display_name, created_at) VALUES (?, ?, ?)")
        .bind(&user.id)
        .bind(&user.display_name)
        .bind(&user.created_at)
        .execute(&state.pool)
        .await?;

    // Mark setup complete in app_config
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(keys::SETUP_COMPLETE)
    .bind("true")
    .bind(&now)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "complete": true,
                "user": {
                    "id": user.id,
                    "display_name": user.display_name
                }
            }
        })),
    ))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let config = crate::config::Config {
            port: 7474,
            database_url: "sqlite::memory:".into(),
            public_dir: "./public".into(),
            fcm_service_account_json: None,
        };

        let token = crate::services::auth::get_or_create_auth_token(&pool)
            .await
            .expect("token");
        let (app, _mcp) = crate::routes::build_router(pool, config)
            .await
            .expect("router");
        (app, token)
    }

    #[tokio::test]
    async fn test_fresh_db_returns_setup_incomplete() {
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

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["data"]["complete"], false);
    }

    #[tokio::test]
    async fn test_complete_setup_then_status_returns_complete() {
        let (app, _token) = test_app().await;

        // Complete setup
        let body = r#"{"display_name": "Marcus"}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup/complete")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        // Check status
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/setup/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["data"]["complete"], true);
    }

    #[tokio::test]
    async fn test_complete_setup_twice_returns_conflict() {
        let (app, _token) = test_app().await;

        let body = r#"{"display_name": "Marcus"}"#;

        // First call
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup/complete")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Second call — should conflict
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup/complete")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_complete_setup_rejects_empty_display_name() {
        let (app, _token) = test_app().await;

        let body = r#"{"display_name": "   "}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup/complete")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }
}

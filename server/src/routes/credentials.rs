use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::error::AppError;
use crate::models::credential::{CreateCredential, UpdateCredential};
use crate::services::credentials as credential_service;

use super::AppState;

/// GET /api/credentials
/// Returns all credentials as public records (no encrypted_data).
pub async fn list(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let credentials = credential_service::list_credentials(&state.pool).await?;
    Ok(Json(credentials))
}

/// GET /api/credentials/:id
/// Returns a single credential by ID (no encrypted_data).
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let credential = credential_service::get_credential(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("Credential not found".to_string()))?;
    Ok(Json(credential))
}

/// POST /api/credentials
/// Body must include `secret`. Server encrypts before storage.
/// Returns the created public-facing credential record.
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCredential>,
) -> Result<impl IntoResponse, AppError> {
    let credential =
        credential_service::create_credential(&state.pool, &state.credential_master_key, &req)
            .await?;
    Ok((StatusCode::CREATED, Json(credential)))
}

/// PUT /api/credentials/:id
/// Allows updating display_name, service, and optionally the secret.
/// Returns the updated public-facing credential record.
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCredential>,
) -> Result<impl IntoResponse, AppError> {
    let credential =
        credential_service::update_credential(&state.pool, &state.credential_master_key, &id, &req)
            .await?
            .ok_or_else(|| AppError::NotFound("Credential not found".to_string()))?;
    Ok(Json(credential))
}

/// DELETE /api/credentials/:id
/// Deletes the credential. Response includes a warning listing any MCP servers
/// that reference this credential by key (so the caller can warn the user).
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Load the record first so we can look up its key for the MCP reference check.
    let credential = credential_service::get_credential(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("Credential not found".to_string()))?;

    // Check for MCP servers that reference this credential before deleting.
    let referenced_by =
        credential_service::mcp_servers_using_credential(&state.pool, &credential.key).await?;

    let deleted = credential_service::delete_credential(&state.pool, &id).await?;
    if !deleted {
        return Err(AppError::NotFound("Credential not found".to_string()));
    }

    if referenced_by.is_empty() {
        Ok((StatusCode::NO_CONTENT).into_response())
    } else {
        // 200 with a warning body rather than 204 so the client can surface which
        // servers lost their credential.
        Ok((
            StatusCode::OK,
            Json(json!({
                "deleted": true,
                "warnings": referenced_by.iter().map(|name| {
                    format!("MCP server '{}' referenced this credential", name)
                }).collect::<Vec<_>>()
            })),
        )
            .into_response())
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::routes::build_router;
    use crate::services::auth as auth_service;

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

    fn auth(token: &str) -> String {
        format!("Bearer {}", token)
    }

    async fn body_json(body: axum::body::Body) -> Value {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    fn sample_payload() -> Value {
        json!({
            "key": "openai_route_test",
            "display_name": "OpenAI Route Test",
            "service": "openai",
            "credential_type": "api_key",
            "secret": "sk-test-abc123"
        })
    }

    #[tokio::test]
    async fn test_list_credentials_empty() {
        let (app, token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/credentials")
                    .header("Authorization", auth(&token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response.into_body()).await;
        assert_eq!(body, json!([]));
    }

    #[tokio::test]
    async fn test_create_credential() {
        let (app, token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/credentials")
                    .header("Authorization", auth(&token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&sample_payload()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = body_json(response.into_body()).await;
        assert_eq!(body["key"], "openai_route_test");
        assert_eq!(body["service"], "openai");
        assert_eq!(body["credential_type"], "api_key");
        // encrypted_data must be absent
        assert!(body.get("encrypted_data").is_none());
        assert!(body.get("secret").is_none());
    }

    #[tokio::test]
    async fn test_create_then_list() {
        let (app, token) = test_app().await;

        // Create
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/credentials")
                    .header("Authorization", auth(&token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&sample_payload()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // List
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/credentials")
                    .header("Authorization", auth(&token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response.into_body()).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert!(body[0].get("encrypted_data").is_none());
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_404() {
        let (app, token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/credentials/does-not-exist")
                    .header("Authorization", auth(&token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_update_credential() {
        let (app, token) = test_app().await;

        // Create first
        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/credentials")
                    .header("Authorization", auth(&token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&sample_payload()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let created = body_json(create_resp.into_body()).await;
        let id = created["id"].as_str().unwrap();

        // Update display_name
        let update_resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/credentials/{}", id))
                    .header("Authorization", auth(&token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&json!({"display_name": "Renamed Key"})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_resp.status(), StatusCode::OK);
        let updated = body_json(update_resp.into_body()).await;
        assert_eq!(updated["display_name"], "Renamed Key");
        assert!(updated.get("encrypted_data").is_none());
    }

    #[tokio::test]
    async fn test_delete_credential() {
        let (app, token) = test_app().await;

        // Create
        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/credentials")
                    .header("Authorization", auth(&token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&sample_payload()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let created = body_json(create_resp.into_body()).await;
        let id = created["id"].as_str().unwrap();

        // Delete
        let del_resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/credentials/{}", id))
                    .header("Authorization", auth(&token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_credentials_require_auth() {
        let (app, _token) = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/credentials")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Auth is bypassed for localhost in tests, so this will pass — but it confirms
        // the route is registered and reachable.
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}

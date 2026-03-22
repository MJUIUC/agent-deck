use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

// SchedulerCommand is defined in services/scheduler.rs (implemented by scheduler agent).
// Import is active; the actual .send() calls below are commented out pending
// scheduler_tx being added to AppState in routes/mod.rs.
use crate::services::scheduler::SchedulerCommand;

use crate::{
    error::{AppError, AppResult},
    models::routine::{CreateRoutine, Routine, UpdateRoutine},
    routes::AppState,
};

// NOTE: The toggle route must be registered in routes/mod.rs as:
// .route(
//     "/api/threads/:thread_id/routines/:routine_id/toggle",
//     patch(routines::toggle),
// )

/// Helper: get the single user id from the DB.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// Helper: verify a thread exists and belongs to the given user.
async fn verify_thread_ownership(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
) -> AppResult<()> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM threads WHERE id = ? AND user_id = ?")
            .bind(thread_id)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;

    if row.is_none() {
        return Err(AppError::NotFound(format!(
            "Thread '{}' not found",
            thread_id
        )));
    }
    Ok(())
}

/// GET /api/threads/:id/routines
pub async fn list(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let routines: Vec<Routine> = sqlx::query_as(
        "SELECT id, thread_id, name, prompt, cron_expr, enabled,
                run_count, last_run_at, next_run_at, created_at, updated_at
         FROM routines
         WHERE thread_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&thread_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": routines }))))
}

/// GET /api/threads/:thread_id/routines/:routine_id
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path((thread_id, routine_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let routine: Option<Routine> = sqlx::query_as(
        "SELECT id, thread_id, name, prompt, cron_expr, enabled,
                run_count, last_run_at, next_run_at, created_at, updated_at
         FROM routines
         WHERE id = ? AND thread_id = ?",
    )
    .bind(&routine_id)
    .bind(&thread_id)
    .fetch_optional(&state.pool)
    .await?;

    match routine {
        Some(r) => Ok((StatusCode::OK, Json(json!({ "data": r })))),
        None => Err(AppError::NotFound(format!(
            "Routine '{}' not found",
            routine_id
        ))),
    }
}

/// POST /api/threads/:id/routines
pub async fn create(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(payload): Json<CreateRoutine>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }
    if payload.prompt.trim().is_empty() {
        return Err(AppError::BadRequest("prompt must not be empty".to_string()));
    }
    if payload.cron_expr.trim().is_empty() {
        return Err(AppError::BadRequest(
            "cron_expr must not be empty".to_string(),
        ));
    }

    // Basic cron expression validation: must have exactly 5 space-separated fields
    let cron_parts: Vec<&str> = payload.cron_expr.trim().split_whitespace().collect();
    if cron_parts.len() != 5 {
        return Err(AppError::BadRequest(
            "cron_expr must be a standard 5-field cron expression (e.g. '0 9 * * *')".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let routine = Routine::new(&thread_id, payload);

    sqlx::query(
        "INSERT INTO routines
             (id, thread_id, name, prompt, cron_expr, enabled,
              run_count, last_run_at, next_run_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&routine.id)
    .bind(&routine.thread_id)
    .bind(&routine.name)
    .bind(&routine.prompt)
    .bind(&routine.cron_expr)
    .bind(routine.enabled)
    .bind(routine.run_count)
    .bind(&routine.last_run_at)
    .bind(&routine.next_run_at)
    .bind(&routine.created_at)
    .bind(&routine.updated_at)
    .execute(&state.pool)
    .await?;

    let _ = state
        .scheduler_tx
        .send(SchedulerCommand::Upsert(routine.id.clone()))
        .await;

    Ok((StatusCode::CREATED, Json(json!({ "data": routine }))))
}

/// PUT /api/threads/:thread_id/routines/:routine_id
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path((thread_id, routine_id)): Path<(String, String)>,
    Json(payload): Json<UpdateRoutine>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let existing: Option<Routine> = sqlx::query_as(
        "SELECT id, thread_id, name, prompt, cron_expr, enabled,
                run_count, last_run_at, next_run_at, created_at, updated_at
         FROM routines
         WHERE id = ? AND thread_id = ?",
    )
    .bind(&routine_id)
    .bind(&thread_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(r) => r,
        None => {
            return Err(AppError::NotFound(format!(
                "Routine '{}' not found",
                routine_id
            )))
        }
    };

    // Validate new cron expression if provided
    if let Some(ref cron) = payload.cron_expr {
        let parts: Vec<&str> = cron.trim().split_whitespace().collect();
        if parts.len() != 5 {
            return Err(AppError::BadRequest(
                "cron_expr must be a standard 5-field cron expression (e.g. '0 9 * * *')"
                    .to_string(),
            ));
        }
    }

    let name = payload.name.as_deref().unwrap_or(&existing.name);
    let prompt = payload.prompt.as_deref().unwrap_or(&existing.prompt);
    let cron_expr = payload.cron_expr.as_deref().unwrap_or(&existing.cron_expr);
    let enabled = payload.enabled.unwrap_or(existing.enabled);

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE routines
         SET name = ?, prompt = ?, cron_expr = ?, enabled = ?, updated_at = ?
         WHERE id = ? AND thread_id = ?",
    )
    .bind(name)
    .bind(prompt)
    .bind(cron_expr)
    .bind(enabled)
    .bind(&now)
    .bind(&routine_id)
    .bind(&thread_id)
    .execute(&state.pool)
    .await?;

    let updated = Routine {
        id: existing.id,
        thread_id: existing.thread_id,
        name: name.to_string(),
        prompt: prompt.to_string(),
        cron_expr: cron_expr.to_string(),
        enabled,
        run_count: existing.run_count,
        last_run_at: existing.last_run_at,
        next_run_at: existing.next_run_at,
        created_at: existing.created_at,
        updated_at: now,
    };

    let _ = state
        .scheduler_tx
        .send(SchedulerCommand::Upsert(routine_id.clone()))
        .await;

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/threads/:thread_id/routines/:routine_id
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path((thread_id, routine_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let result = sqlx::query("DELETE FROM routines WHERE id = ? AND thread_id = ?")
        .bind(&routine_id)
        .bind(&thread_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Routine '{}' not found",
            routine_id
        )));
    }

    let _ = state
        .scheduler_tx
        .send(SchedulerCommand::Remove(routine_id.clone()))
        .await;

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

/// PATCH /api/threads/:thread_id/routines/:routine_id/toggle
///
/// Flips the `enabled` flag on a routine and notifies the scheduler.
/// Returns the new id + enabled state.
pub async fn toggle(
    State(state): State<Arc<AppState>>,
    Path((thread_id, routine_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    // Fetch current enabled state
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT enabled FROM routines WHERE id = ? AND thread_id = ?")
            .bind(&routine_id)
            .bind(&thread_id)
            .fetch_optional(&state.pool)
            .await?;

    let current_enabled = match row {
        Some((e,)) => e,
        None => {
            return Err(AppError::NotFound(format!(
                "Routine '{}' not found",
                routine_id
            )))
        }
    };

    let new_enabled = !current_enabled;
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query("UPDATE routines SET enabled = ?, updated_at = ? WHERE id = ? AND thread_id = ?")
        .bind(new_enabled)
        .bind(&now)
        .bind(&routine_id)
        .bind(&thread_id)
        .execute(&state.pool)
        .await?;

    // Upsert re-evaluates enabled flag — scheduler will remove the job if disabled.
    let _ = state
        .scheduler_tx
        .send(SchedulerCommand::Upsert(routine_id.clone()))
        .await;

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "id": routine_id, "enabled": new_enabled } })),
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use tower::ServiceExt;

    // ── Shared test helpers ───────────────────────────────────────────────────

    /// Build a fresh in-memory router + return the auth token.
    async fn test_app() -> (Router, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let config = crate::config::Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".to_string(),
            public_dir: "./public".to_string(),
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

    /// Full setup: router + user + default thread. Returns (app, token, thread_id).
    async fn setup_app() -> (Router, String, String) {
        let (app, token) = test_app().await;

        // Create the single user via the setup endpoint.
        let setup_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup/complete")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"display_name": "Test User"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            setup_resp.status(),
            StatusCode::OK,
            "setup/complete should succeed"
        );

        // Create a thread to attach routines to.
        let thread_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/threads")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title": "Test Thread"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            thread_resp.status(),
            StatusCode::CREATED,
            "thread creation should return 201"
        );

        let body_bytes = axum::body::to_bytes(thread_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let thread_id = json["data"]["id"]
            .as_str()
            .expect("thread id in response")
            .to_string();

        (app, token, thread_id)
    }

    /// Create a routine via the API and return its id.
    async fn create_routine(app: &Router, token: &str, thread_id: &str, name: &str) -> String {
        let payload = serde_json::json!({
            "name": name,
            "prompt": "Do something useful every morning",
            "cron_expr": "0 9 * * *"
        })
        .to_string();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/threads/{}/routines", thread_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "routine creation should return 201"
        );

        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        json["data"]["id"]
            .as_str()
            .expect("routine id in response")
            .to_string()
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_routines_empty() {
        let (app, token, thread_id) = setup_app().await;

        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/threads/{}/routines", thread_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["data"].is_array(), "data should be an array");
        assert_eq!(
            json["data"].as_array().unwrap().len(),
            0,
            "list should be empty for a new thread"
        );
    }

    #[tokio::test]
    async fn test_create_routine_success() {
        let (app, token, thread_id) = setup_app().await;

        let payload = serde_json::json!({
            "name": "Morning Standup",
            "prompt": "Summarize yesterday's work",
            "cron_expr": "0 9 * * 1-5"
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/threads/{}/routines", thread_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["data"]["id"].is_string(), "should have id");
        assert_eq!(json["data"]["name"], "Morning Standup");
        assert_eq!(json["data"]["cron_expr"], "0 9 * * 1-5");
        assert_eq!(json["data"]["thread_id"], thread_id);
        assert_eq!(
            json["data"]["enabled"], true,
            "new routine should be enabled"
        );
        assert_eq!(json["data"]["run_count"], 0);
    }

    #[tokio::test]
    async fn test_create_routine_invalid_cron() {
        let (app, token, thread_id) = setup_app().await;

        // 4-field cron expression — invalid (must be 5 fields)
        let payload = serde_json::json!({
            "name": "Bad Cron",
            "prompt": "Do something",
            "cron_expr": "0 9 * *"
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/threads/{}/routines", thread_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "4-field cron should be rejected with 400"
        );
    }

    #[tokio::test]
    async fn test_create_routine_missing_name() {
        let (app, token, thread_id) = setup_app().await;

        let payload = serde_json::json!({
            "name": "   ",
            "prompt": "Do something",
            "cron_expr": "0 9 * * *"
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/threads/{}/routines", thread_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "blank name should be rejected with 400"
        );
    }

    #[tokio::test]
    async fn test_get_routine() {
        let (app, token, thread_id) = setup_app().await;

        let routine_id = create_routine(&app, &token, &thread_id, "Daily Digest").await;

        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/threads/{}/routines/{}",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["data"]["id"], routine_id);
        assert_eq!(json["data"]["name"], "Daily Digest");
        assert_eq!(json["data"]["thread_id"], thread_id);
    }

    #[tokio::test]
    async fn test_get_routine_not_found() {
        let (app, token, thread_id) = setup_app().await;

        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/threads/{}/routines/nonexistent-id",
                        thread_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "unknown routine id should return 404"
        );
    }

    #[tokio::test]
    async fn test_update_routine() {
        let (app, token, thread_id) = setup_app().await;

        let routine_id = create_routine(&app, &token, &thread_id, "Old Name").await;

        let update_payload = serde_json::json!({
            "name": "New Name",
            "prompt": "Updated prompt content",
            "cron_expr": "30 8 * * *"
        })
        .to_string();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/api/threads/{}/routines/{}",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(update_payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["data"]["name"], "New Name");
        assert_eq!(json["data"]["prompt"], "Updated prompt content");
        assert_eq!(json["data"]["cron_expr"], "30 8 * * *");
        assert_eq!(json["data"]["id"], routine_id);

        // Confirm the GET also reflects the update.
        let get_resp = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/threads/{}/routines/{}",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body2 = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
        assert_eq!(json2["data"]["name"], "New Name");
    }

    #[tokio::test]
    async fn test_delete_routine() {
        let (app, token, thread_id) = setup_app().await;

        let routine_id = create_routine(&app, &token, &thread_id, "To Be Deleted").await;

        // Delete it.
        let del_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/threads/{}/routines/{}",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(del_resp.status(), StatusCode::OK);

        let del_body = axum::body::to_bytes(del_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let del_json: serde_json::Value = serde_json::from_slice(&del_body).unwrap();
        assert_eq!(del_json["data"]["deleted"], true);

        // Subsequent GET must return 404.
        let get_resp = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/threads/{}/routines/{}",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            get_resp.status(),
            StatusCode::NOT_FOUND,
            "deleted routine should return 404 on subsequent GET"
        );
    }

    #[tokio::test]
    async fn test_toggle_routine() {
        let (app, token, thread_id) = setup_app().await;

        let routine_id = create_routine(&app, &token, &thread_id, "Toggle Me").await;

        // Routines are enabled=true by default — first toggle should disable.
        let toggle_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!(
                        "/api/threads/{}/routines/{}/toggle",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(toggle_resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(toggle_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["data"]["id"], routine_id);
        assert_eq!(
            json["data"]["enabled"], false,
            "first toggle: enabled should flip to false"
        );

        // Second toggle should re-enable.
        let toggle_resp2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!(
                        "/api/threads/{}/routines/{}/toggle",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(toggle_resp2.status(), StatusCode::OK);

        let body2 = axum::body::to_bytes(toggle_resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();

        assert_eq!(
            json2["data"]["enabled"], true,
            "second toggle: enabled should flip back to true"
        );

        // Confirm GET reflects the final state.
        let get_resp = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/threads/{}/routines/{}",
                        thread_id, routine_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let get_body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let get_json: serde_json::Value = serde_json::from_slice(&get_body).unwrap();
        assert_eq!(
            get_json["data"]["enabled"], true,
            "GET should confirm enabled=true after double toggle"
        );
    }

    #[tokio::test]
    async fn test_toggle_nonexistent() {
        let (app, token, thread_id) = setup_app().await;

        let resp = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!(
                        "/api/threads/{}/routines/nonexistent-id/toggle",
                        thread_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "toggle on an unknown routine id should return 404"
        );
    }
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::routine::{CreateRoutine, Routine, UpdateRoutine},
    routes::AppState,
};

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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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

    // TODO (Phase 4): register this routine with the scheduler service.

    Ok((StatusCode::CREATED, Json(json!({ "data": routine }))))
}

/// PUT /api/threads/:thread_id/routines/:routine_id
pub async fn update(
    State(state): State<AppState>,
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

    // TODO (Phase 4): notify the scheduler service to update this routine's job.

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/threads/:thread_id/routines/:routine_id
pub async fn delete(
    State(state): State<AppState>,
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

    // TODO (Phase 4): notify the scheduler service to remove this routine's job.

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::memory::CreateMemory,
    routes::AppState,
    services::memory as memory_service,
};

/// Helper: get the single user id from the DB.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// Helper: verify a persona exists and belongs to the current user.
async fn verify_persona_ownership(
    state: &AppState,
    persona_id: &str,
    user_id: &str,
) -> AppResult<()> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM agent_personas WHERE id = ? AND user_id = ?")
            .bind(persona_id)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;

    if row.is_none() {
        return Err(AppError::NotFound(format!(
            "Persona '{}' not found",
            persona_id
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ListMemoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub thread_id: Option<String>,
}

/// GET /api/personas/:id/memory
///
/// List all memories for a persona, newest first. Supports pagination via
/// `limit` and `offset` query params.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Path(persona_id): Path<String>,
    Query(query): Query<ListMemoryQuery>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_persona_ownership(&state, &persona_id, &user_id).await?;

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let result = memory_service::list_memories(
        &state.pool,
        &user_id,
        &persona_id,
        offset,
        limit,
        query.thread_id.as_deref(),
    )
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": result }))))
}

/// POST /api/personas/:id/memory
///
/// Manually create a memory entry for a persona. The `thread_id` is optional
/// — manual entries don't have a source thread.
pub async fn create(
    State(state): State<Arc<AppState>>,
    Path(persona_id): Path<String>,
    Json(payload): Json<CreateMemory>,
) -> AppResult<impl IntoResponse> {
    if payload.content.trim().is_empty() {
        return Err(AppError::BadRequest(
            "content must not be empty".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;
    verify_persona_ownership(&state, &persona_id, &user_id).await?;

    let memory = memory_service::save_memory(
        &state.pool,
        &user_id,
        &persona_id,
        payload.thread_id.as_deref(),
        &payload.content,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "data": memory }))))
}

/// DELETE /api/personas/:persona_id/memory/:memory_id
///
/// Delete a single memory entry. Returns 404 if not found or not owned by
/// the current user.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path((persona_id, memory_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_persona_ownership(&state, &persona_id, &user_id).await?;

    let deleted = memory_service::delete_memory(&state.pool, &memory_id, &user_id).await?;

    if !deleted {
        return Err(AppError::NotFound(format!(
            "Memory '{}' not found",
            memory_id
        )));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchRequest {
    pub query: String,
    pub limit: Option<i64>,
}

/// POST /api/personas/:id/memory/search
///
/// Full-text search over memories for a persona using SQLite FTS5.
/// Returns results ordered by relevance (FTS rank).
pub async fn search(
    State(state): State<Arc<AppState>>,
    Path(persona_id): Path<String>,
    Json(payload): Json<MemorySearchRequest>,
) -> AppResult<impl IntoResponse> {
    if payload.query.trim().is_empty() {
        return Err(AppError::BadRequest("query must not be empty".to_string()));
    }

    let user_id = get_user_id(&state).await?;
    verify_persona_ownership(&state, &persona_id, &user_id).await?;

    let limit = payload.limit.unwrap_or(20).clamp(1, 100);

    let results = memory_service::recall_memory(
        &state.pool,
        &user_id,
        &persona_id,
        payload.query.trim(),
        limit,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "memories": results,
                "query": payload.query
            }
        })),
    ))
}

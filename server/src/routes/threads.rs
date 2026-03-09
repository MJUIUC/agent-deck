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
    models::thread::{AttachMcpServer, CreateThread, Thread, ThreadMcpServer, UpdateThread},
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

#[derive(Debug, Deserialize)]
pub struct ListThreadsQuery {
    pub status: Option<String>,
}

/// GET /api/threads
/// Optional query param: ?status=archived (default: active)
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListThreadsQuery>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let status = query.status.as_deref().unwrap_or("active");

    let valid_statuses = ["active", "archived"];
    if !valid_statuses.contains(&status) {
        return Err(AppError::BadRequest(format!(
            "status must be one of: {}",
            valid_statuses.join(", ")
        )));
    }

    let threads: Vec<Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, created_at, updated_at
         FROM threads
         WHERE user_id = ? AND status = ?
         ORDER BY updated_at DESC",
    )
    .bind(&user_id)
    .bind(status)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": threads }))))
}

/// GET /api/threads/:id
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let thread: Option<Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, created_at, updated_at
         FROM threads
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    match thread {
        Some(t) => Ok((StatusCode::OK, Json(json!({ "data": t })))),
        None => Err(AppError::NotFound(format!("Thread '{}' not found", id))),
    }
}

/// POST /api/threads
pub async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreateThread>,
) -> AppResult<impl IntoResponse> {
    if payload.persona_id.trim().is_empty() {
        return Err(AppError::BadRequest(
            "persona_id must not be empty".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;

    // Verify the persona exists and belongs to this user
    let persona_exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM agent_personas WHERE id = ? AND user_id = ?")
            .bind(&payload.persona_id)
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await?;

    if persona_exists.is_none() {
        return Err(AppError::BadRequest(format!(
            "Persona '{}' not found",
            payload.persona_id
        )));
    }

    let thread = Thread::new(&user_id, payload);

    sqlx::query(
        "INSERT INTO threads
             (id, user_id, persona_id, title, active_model, active_provider,
              system_prompt_addendum, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&thread.id)
    .bind(&thread.user_id)
    .bind(&thread.persona_id)
    .bind(&thread.title)
    .bind(&thread.active_model)
    .bind(&thread.active_provider)
    .bind(&thread.system_prompt_addendum)
    .bind(&thread.status)
    .bind(&thread.created_at)
    .bind(&thread.updated_at)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "data": thread }))))
}

/// PUT /api/threads/:id
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateThread>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let existing: Option<Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, created_at, updated_at
         FROM threads
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(t) => t,
        None => return Err(AppError::NotFound(format!("Thread '{}' not found", id))),
    };

    let title = payload.title.as_deref().unwrap_or(&existing.title);
    let system_prompt_addendum: Option<&str> = match &payload.system_prompt_addendum {
        Some(v) => Some(v.as_str()),
        None => existing.system_prompt_addendum.as_deref(),
    };

    // For nullable FK fields (active_model, active_provider), use same pattern:
    // if field is present in payload (Some), update it; otherwise keep existing.
    let active_model = match &payload.active_model {
        Some(v) => Some(v.as_str()),
        None => existing.active_model.as_deref(),
    };
    let active_provider = match &payload.active_provider {
        Some(v) => Some(v.as_str()),
        None => existing.active_provider.as_deref(),
    };

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE threads
         SET title = ?, active_model = ?, active_provider = ?,
             system_prompt_addendum = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(title)
    .bind(active_model)
    .bind(active_provider)
    .bind(system_prompt_addendum)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    let updated = Thread {
        id: existing.id,
        user_id: existing.user_id,
        persona_id: existing.persona_id,
        title: title.to_string(),
        active_model: active_model.map(|s| s.to_string()),
        active_provider: active_provider.map(|s| s.to_string()),
        system_prompt_addendum: system_prompt_addendum.map(|s| s.to_string()),
        status: existing.status,
        created_at: existing.created_at,
        updated_at: now,
    };

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/threads/:id
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM threads WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Thread '{}' not found", id)));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

/// POST /api/threads/:id/archive
pub async fn archive(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    set_thread_status(&state, &id, &user_id, "archived").await
}

/// POST /api/threads/:id/unarchive
pub async fn unarchive(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    set_thread_status(&state, &id, &user_id, "active").await
}

async fn set_thread_status(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    status: &str,
) -> AppResult<impl IntoResponse> {
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    let result =
        sqlx::query("UPDATE threads SET status = ?, updated_at = ? WHERE id = ? AND user_id = ?")
            .bind(status)
            .bind(&now)
            .bind(thread_id)
            .bind(user_id)
            .execute(&state.pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Thread '{}' not found",
            thread_id
        )));
    }

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "id": thread_id, "status": status } })),
    ))
}

// ─── Thread MCP Servers ────────────────────────────────────────────────────────

/// GET /api/threads/:id/mcp-servers
pub async fn list_mcp_servers(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let servers: Vec<ThreadMcpServer> = sqlx::query_as(
        "SELECT id, thread_id, mcp_server_id, enabled
         FROM thread_mcp_servers
         WHERE thread_id = ?",
    )
    .bind(&thread_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": servers }))))
}

/// POST /api/threads/:id/mcp-servers
pub async fn attach_mcp(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(payload): Json<AttachMcpServer>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    // Verify the MCP server exists and belongs to this user
    let mcp_exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM mcp_servers WHERE id = ? AND user_id = ?")
            .bind(&payload.mcp_server_id)
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await?;

    if mcp_exists.is_none() {
        return Err(AppError::NotFound(format!(
            "MCP server '{}' not found",
            payload.mcp_server_id
        )));
    }

    let entry = ThreadMcpServer::new(&thread_id, &payload.mcp_server_id);

    sqlx::query(
        "INSERT OR IGNORE INTO thread_mcp_servers (id, thread_id, mcp_server_id, enabled)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&entry.id)
    .bind(&entry.thread_id)
    .bind(&entry.mcp_server_id)
    .bind(entry.enabled)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "data": entry }))))
}

/// DELETE /api/threads/:id/mcp-servers/:mcp_id
pub async fn detach_mcp(
    State(state): State<AppState>,
    Path((thread_id, mcp_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let result =
        sqlx::query("DELETE FROM thread_mcp_servers WHERE thread_id = ? AND mcp_server_id = ?")
            .bind(&thread_id)
            .bind(&mcp_id)
            .execute(&state.pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "MCP server '{}' not attached to thread '{}'",
            mcp_id, thread_id
        )));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

/// Verify that a thread exists and belongs to the given user.
/// Returns the thread's ID on success.
async fn verify_thread_ownership(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
) -> AppResult<String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM threads WHERE id = ? AND user_id = ?")
            .bind(thread_id)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;

    row.map(|(id,)| id)
        .ok_or_else(|| AppError::NotFound(format!("Thread '{}' not found", thread_id)))
}

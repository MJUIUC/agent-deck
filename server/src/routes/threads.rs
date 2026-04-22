use std::sync::Arc;

use tracing::{debug, error};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::services::scheduler::SchedulerCommand;

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
    State(state): State<Arc<AppState>>,
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

    debug!(user_id = %user_id, status = %status, "list: fetching threads");
    let threads: Vec<Thread> = crate::db::threads::list_by_user_and_status(&state.pool, &user_id, status)
        .await
        .map_err(|e| {
            error!(user_id = %user_id, status = %status, error = ?e, error.display = %e, "list: failed to fetch threads");
            e
        })?;
    debug!(user_id = %user_id, count = threads.len(), "list: fetched threads successfully");

    // Fetch the last visible message content for each thread in one query,
    // then merge into the response. Using a separate query with GROUP BY is
    // simpler than a correlated subquery and works well with SQLx's type system.
    let previews: Vec<(String, String)> = sqlx::query_as(
        "SELECT thread_id, content
         FROM messages
         WHERE (thread_id, created_at) IN (
             SELECT thread_id, MAX(created_at)
             FROM messages
             WHERE thread_id IN (
                 SELECT id FROM threads WHERE user_id = ? AND status = ?
             )
             AND visibility = 'visible'
             AND role IN ('user', 'assistant')
             GROUP BY thread_id
         )",
    )
    .bind(&user_id)
    .bind(status)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let preview_map: std::collections::HashMap<String, String> = previews
        .into_iter()
        .map(|(tid, content)| {
            // Truncate to 120 chars, collapse newlines to spaces
            let truncated = content
                .lines()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(120)
                .collect::<String>();
            (tid, truncated)
        })
        .collect();

    let response: Vec<serde_json::Value> = threads
        .into_iter()
        .map(|t| {
            let preview = preview_map.get(&t.id).cloned();
            let mut v = serde_json::to_value(&t).unwrap_or(serde_json::Value::Null);
            if let serde_json::Value::Object(ref mut map) = v {
                map.insert(
                    "last_message_preview".to_string(),
                    preview
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            v
        })
        .collect();

    Ok((StatusCode::OK, Json(json!({ "data": response }))))
}

/// GET /api/threads/:id
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    debug!(thread_id = %id, user_id = %user_id, "get: fetching thread");
    let thread: Option<Thread> = crate::db::threads::fetch_by_id_and_user(&state.pool, &id, &user_id)
        .await
        .map_err(|e| {
            error!(thread_id = %id, user_id = %user_id, error = ?e, error.display = %e, "get: failed to fetch thread");
            e
        })?;

    match thread {
        Some(t) => Ok((StatusCode::OK, Json(json!({ "data": t })))),
        None => Err(AppError::NotFound(format!("Thread '{}' not found", id))),
    }
}

/// POST /api/threads
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateThread>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    // Resolve the persona_id: use the provided one (with ownership check) or fall back to Default
    let resolved_persona_id = match payload.persona_id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => {
            // Verify the persona exists and belongs to this user
            let persona_exists: Option<(String,)> =
                sqlx::query_as("SELECT id FROM agent_personas WHERE id = ? AND user_id = ?")
                    .bind(id)
                    .bind(&user_id)
                    .fetch_optional(&state.pool)
                    .await?;

            if persona_exists.is_none() {
                return Err(AppError::BadRequest(format!("Persona '{}' not found", id)));
            }

            id.to_string()
        }
        None => {
            let default: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM agent_personas WHERE user_id = ? AND is_default = 1 LIMIT 1",
            )
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await?;

            match default {
                Some((id,)) => id,
                None => {
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "No Default persona found for user — setup may be incomplete"
                    )))
                }
            }
        }
    };

    let thread = Thread::new(&user_id, &resolved_persona_id, payload);

    sqlx::query(
        "INSERT INTO threads
             (id, user_id, persona_id, title, active_model, active_provider,
              system_prompt_addendum, status, show_tool_activity, show_system_events,
              summary, summary_updated_at, summary_message_count, auto_summarize,
              auto_retitle, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&thread.id)
    .bind(&thread.user_id)
    .bind(&thread.persona_id)
    .bind(&thread.title)
    .bind(&thread.active_model)
    .bind(&thread.active_provider)
    .bind(&thread.system_prompt_addendum)
    .bind(&thread.status)
    .bind(thread.show_tool_activity)
    .bind(thread.show_system_events)
    .bind(&thread.summary)
    .bind(&thread.summary_updated_at)
    .bind(thread.summary_message_count)
    .bind(thread.auto_summarize)
    .bind(thread.auto_retitle)
    .bind(&thread.created_at)
    .bind(&thread.updated_at)
    .execute(&state.pool)
    .await?;

    // Auto-attach any MCP servers that are defaults for this persona.
    // Query persona_default_mcp_servers and insert into thread_mcp_servers for each.
    let default_mcp_server_ids: Vec<(String,)> = sqlx::query_as(
        "SELECT mcp_server_id FROM persona_default_mcp_servers WHERE persona_id = ?",
    )
    .bind(&thread.persona_id)
    .fetch_all(&state.pool)
    .await?;

    for (mcp_server_id,) in default_mcp_server_ids {
        let entry_id = Uuid::new_v4().to_string();
        // INSERT OR IGNORE so a duplicate constraint never fails the thread creation
        sqlx::query(
            "INSERT OR IGNORE INTO thread_mcp_servers (id, thread_id, mcp_server_id, enabled)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&entry_id)
        .bind(&thread.id)
        .bind(&mcp_server_id)
        .bind(true)
        .execute(&state.pool)
        .await?;
    }

    Ok((StatusCode::CREATED, Json(json!({ "data": thread }))))
}

/// PUT /api/threads/:id
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateThread>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    debug!(thread_id = %id, user_id = %user_id, "update: fetching existing thread");
    let existing: Option<Thread> = crate::db::threads::fetch_by_id_and_user(&state.pool, &id, &user_id)
        .await
        .map_err(|e| {
            error!(thread_id = %id, user_id = %user_id, error = ?e, error.display = %e, "update: failed to fetch existing thread");
            e
        })?;

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

    let show_tool_activity = payload
        .show_tool_activity
        .unwrap_or(existing.show_tool_activity);

    let show_system_events = payload
        .show_system_events
        .unwrap_or(existing.show_system_events);

    let auto_summarize = payload.auto_summarize.unwrap_or(existing.auto_summarize);
    let auto_retitle = payload.auto_retitle.unwrap_or(existing.auto_retitle);

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE threads
         SET title = ?, active_model = ?, active_provider = ?,
             system_prompt_addendum = ?, show_tool_activity = ?,
             show_system_events = ?, auto_summarize = ?, auto_retitle = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(title)
    .bind(active_model)
    .bind(active_provider)
    .bind(system_prompt_addendum)
    .bind(show_tool_activity)
    .bind(show_system_events)
    .bind(auto_summarize)
    .bind(auto_retitle)
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
        show_tool_activity,
        show_system_events,
        summary: existing.summary,
        summary_updated_at: existing.summary_updated_at,
        summary_message_count: existing.summary_message_count,
        auto_summarize,
        auto_retitle,
        created_at: existing.created_at,
        updated_at: now,
    };

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/threads/:id
///
/// Hard-deletes an empty thread. Returns 400 if the thread has any messages —
/// this prevents accidental data loss and is the intended guard for the pending
/// thread discard flow (which only calls this on zero-message threads).
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    // Verify ownership first so we give a 404 rather than a misleading 400
    // when the thread simply doesn't exist.
    let _thread = verify_thread_ownership(&state, &id, &user_id).await?;

    // Refuse to delete threads that already have messages.
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE thread_id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    if count > 0 {
        return Err(AppError::BadRequest(format!(
            "Thread '{}' has {} message(s) and cannot be deleted",
            id, count
        )));
    }

    sqlx::query("DELETE FROM threads WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

/// GET /api/threads/:id/generate-title
///
/// ⚠️  DEPRECATED — manual/debug use only.
///
/// Title generation is now driven entirely server-side by `agent::run_inner`
/// immediately after the first assistant reply is persisted. A `TitleUpdated`
/// SSE event is broadcast on the global stream so the client sidebar updates
/// in real time without any HTTP call.
///
/// This endpoint is intentionally kept for development and manual testing but
/// returns 503 Service Unavailable to discourage any future client-side callers
/// from reintroducing the fragile client-side title-gen pattern.
///
/// The underlying logic still lives in `services/title.rs` and is called by
/// `agent::run_inner`.
pub async fn generate_title(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    Ok((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": "Title generation is handled server-side by the agent run-loop. \
                      This endpoint is reserved for manual/debug use only and is not \
                      called automatically. See services/title.rs and agent::run_inner \
                      step 6.5."
        })),
    ))
}

/// POST /api/threads/:id/archive
pub async fn archive(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    set_thread_status(&state, &id, &user_id, "archived").await
}

/// POST /api/threads/:id/unarchive
pub async fn unarchive(
    State(state): State<Arc<AppState>>,
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

    if status == "archived" {
        let _ = state
            .scheduler_tx
            .send(SchedulerCommand::PauseThread(thread_id.to_string()))
            .await;
    } else if status == "active" {
        let _ = state
            .scheduler_tx
            .send(SchedulerCommand::ResumeThread(thread_id.to_string()))
            .await;
    }

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "id": thread_id, "status": status } })),
    ))
}

// ─── Thread MCP Servers ────────────────────────────────────────────────────────

/// GET /api/threads/:id/mcp-servers
pub async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _thread = verify_thread_ownership(&state, &thread_id, &user_id).await?;

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
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(payload): Json<AttachMcpServer>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _thread = verify_thread_ownership(&state, &thread_id, &user_id).await?;

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
    State(state): State<Arc<AppState>>,
    Path((thread_id, mcp_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _thread = verify_thread_ownership(&state, &thread_id, &user_id).await?;

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
/// Returns the full `Thread` on success.
async fn verify_thread_ownership(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
) -> AppResult<Thread> {
    debug!(thread_id = %thread_id, user_id = %user_id, "verify_thread_ownership: fetching thread");
    let thread: Option<Thread> = crate::db::threads::fetch_by_id_and_user(&state.pool, thread_id, user_id)
        .await
        .map_err(|e| {
            error!(thread_id = %thread_id, user_id = %user_id, error = ?e, error.display = %e, "verify_thread_ownership: failed to fetch thread");
            e
        })?;

    thread.ok_or_else(|| AppError::NotFound(format!("Thread '{}' not found", thread_id)))
}

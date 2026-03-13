use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::thread::{AttachMcpServer, CreateThread, Thread, ThreadMcpServer, UpdateThread},
    routes::AppState,
    services::agent,
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
                system_prompt_addendum, status, show_tool_activity, created_at, updated_at
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
                system_prompt_addendum, status, show_tool_activity, created_at, updated_at
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
              system_prompt_addendum, status, show_tool_activity, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateThread>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let existing: Option<Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, show_tool_activity, created_at, updated_at
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

    let show_tool_activity = payload
        .show_tool_activity
        .unwrap_or(existing.show_tool_activity);

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE threads
         SET title = ?, active_model = ?, active_provider = ?,
             system_prompt_addendum = ?, show_tool_activity = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(title)
    .bind(active_model)
    .bind(active_provider)
    .bind(system_prompt_addendum)
    .bind(show_tool_activity)
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
    State(state): State<AppState>,
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

/// POST /api/threads/:id/generate-title
///
/// Called by the client after the first agent response streams in.
/// Fetches the first user message and first assistant message, asks the
/// thread's active LLM to produce a concise title (≤ 8 words / 60 chars),
/// persists it, and returns `{ "data": { "title": "..." } }`.
///
/// Falls back to the simple truncation heuristic if the LLM call fails for
/// any reason — the title is never left as "New Chat".
pub async fn generate_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let thread = verify_thread_ownership(&state, &id, &user_id).await?;

    // Fetch first user message
    let first_user: Option<(String,)> = sqlx::query_as(
        "SELECT content FROM messages
         WHERE thread_id = ? AND role = 'user'
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let user_content = match first_user {
        Some((c,)) => c,
        None => {
            // No messages yet — nothing to title, return current title unchanged
            return Ok((
                StatusCode::OK,
                Json(json!({ "data": { "title": thread.title } })),
            ));
        }
    };

    // Fetch first assistant message
    let first_assistant: Option<(String,)> = sqlx::query_as(
        "SELECT content FROM messages
         WHERE thread_id = ? AND role = 'assistant'
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let assistant_content = first_assistant.map(|(c,)| c).unwrap_or_default();

    // ── Attempt LLM title generation ──────────────────────────────────────────

    let generated_title = try_llm_title(&state, &thread, &user_content, &assistant_content).await;

    // ── Persist and return ────────────────────────────────────────────────────

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
        .bind(&generated_title)
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "title": generated_title } })),
    ))
}

/// Try to generate a title via the thread's active LLM.
/// Returns a fallback truncation title on any error.
async fn try_llm_title(
    state: &AppState,
    thread: &Thread,
    user_content: &str,
    assistant_content: &str,
) -> String {
    let fallback = agent::generate_title_from_message(user_content);

    // Resolve provider: prefer thread's active_provider, fall back to persona default
    let provider_id = match thread.active_provider.as_deref() {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            // Look up persona default_provider
            let row: Option<(String,)> =
                sqlx::query_as("SELECT default_provider FROM agent_personas WHERE id = ?")
                    .bind(&thread.persona_id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();

            match row {
                Some((p,)) if !p.is_empty() => p,
                _ => return fallback,
            }
        }
    };

    // Load the provider row
    let provider_row: Option<crate::models::provider::Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at, updated_at
         FROM providers WHERE id = ?",
    )
    .bind(&provider_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let provider_row = match provider_row {
        Some(r) => r,
        None => return fallback,
    };

    // Resolve model: prefer thread's active_model, fall back to persona default
    let model_id = match thread.active_model.as_deref() {
        Some(m) if !m.is_empty() => {
            // active_model is a models table UUID — resolve to the actual model_id string
            let row: Option<(String,)> = sqlx::query_as("SELECT model_id FROM models WHERE id = ?")
                .bind(m)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
            match row {
                Some((mid,)) => mid,
                None => return fallback,
            }
        }
        _ => {
            // Fall back to persona default_model UUID → model_id
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT m.model_id FROM models m
                 JOIN agent_personas p ON p.default_model = m.id
                 WHERE p.id = ?",
            )
            .bind(&thread.persona_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
            match row {
                Some((mid,)) => mid,
                None => return fallback,
            }
        }
    };

    // Build provider and call the LLM
    let provider = match agent::build_provider(state, &provider_row) {
        Ok(p) => p,
        Err(_) => return fallback,
    };

    let prompt = format!(
        "Generate a concise thread title (max 8 words) from this exchange:\nUser: {}\nAssistant: {}\nTitle:",
        user_content.chars().take(500).collect::<String>(),
        assistant_content.chars().take(500).collect::<String>(),
    );

    let messages = vec![async_openai::types::ChatCompletionRequestMessage::User(
        async_openai::types::ChatCompletionRequestUserMessageArgs::default()
            .content(prompt.as_str())
            .build()
            .unwrap(),
    )];

    match provider.complete(&model_id, messages, vec![]).await {
        Ok(raw) => {
            let cleaned = raw.trim().trim_matches('"').trim_matches('\'').trim();
            truncate_title(cleaned)
        }
        Err(_) => fallback,
    }
}

/// Truncate a title to at most 60 characters at a word boundary.
fn truncate_title(s: &str) -> String {
    const MAX_LEN: usize = 60;
    let s = s.trim();
    if s.is_empty() {
        return "New Chat".to_string();
    }
    if s.chars().count() <= MAX_LEN {
        return s.to_string();
    }
    // Walk back from char 60 to the last space
    let truncated: String = s.chars().take(MAX_LEN).collect();
    let at_word = truncated
        .rfind(' ')
        .map(|i| truncated[..i].trim_end().to_string())
        .unwrap_or_else(|| truncated.trim_end().to_string());
    format!("{}…", at_word)
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    let thread: Option<Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, show_tool_activity, created_at, updated_at
         FROM threads
         WHERE id = ? AND user_id = ?",
    )
    .bind(thread_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;

    thread.ok_or_else(|| AppError::NotFound(format!("Thread '{}' not found", thread_id)))
}

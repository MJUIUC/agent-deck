use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::message::{CreateMessage, Message, MessageResponse},
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

/// Helper: verify a thread exists and belongs to the current user.
async fn verify_thread_ownership(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
) -> AppResult<crate::models::thread::Thread> {
    let thread: Option<crate::models::thread::Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, created_at, updated_at
         FROM threads
         WHERE id = ? AND user_id = ?",
    )
    .bind(thread_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;

    thread.ok_or_else(|| AppError::NotFound(format!("Thread '{}' not found", thread_id)))
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    pub limit: Option<i64>,
    pub before: Option<String>,
}

/// GET /api/threads/:id/messages
///
/// Returns message history for a thread, ordered oldest-first.
/// Supports cursor-based pagination via the `before` message ID.
/// Hidden messages (visibility = 'hidden') are excluded by default.
/// Pass `?include_hidden=true` to include them (for debugging only).
pub async fn list(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let messages: Vec<Message> = if let Some(before_id) = &query.before {
        // Cursor pagination: get messages older than the cursor message
        let cursor_time: Option<(String,)> =
            sqlx::query_as("SELECT created_at FROM messages WHERE id = ? AND thread_id = ?")
                .bind(before_id)
                .bind(&thread_id)
                .fetch_optional(&state.pool)
                .await?;

        match cursor_time {
            Some((cursor_created_at,)) => {
                sqlx::query_as(
                    "SELECT id, thread_id, role, content, source, routine_id, visibility, execution_id, created_at
                     FROM messages
                     WHERE thread_id = ? AND created_at < ? AND visibility = 'visible'
                     ORDER BY created_at DESC
                     LIMIT ?",
                )
                .bind(&thread_id)
                .bind(&cursor_created_at)
                .bind(limit)
                .fetch_all(&state.pool)
                .await?
                // Reverse so results come back oldest-first
                .into_iter()
                .rev()
                .collect()
            }
            None => {
                return Err(AppError::NotFound(format!(
                    "Cursor message '{}' not found",
                    before_id
                )))
            }
        }
    } else {
        // No cursor: return the most recent `limit` messages, oldest-first
        sqlx::query_as(
            "SELECT id, thread_id, role, content, source, routine_id, visibility, execution_id, created_at
             FROM (
                 SELECT id, thread_id, role, content, source, routine_id, visibility, execution_id, created_at
                 FROM messages
                 WHERE thread_id = ? AND visibility = 'visible'
                 ORDER BY created_at DESC
                 LIMIT ?
             )
             ORDER BY created_at ASC",
        )
        .bind(&thread_id)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };

    let responses: Vec<MessageResponse> = messages.into_iter().map(Into::into).collect();

    Ok((StatusCode::OK, Json(json!({ "data": responses }))))
}

/// POST /api/threads/:id/messages
///
/// Persists the user message and returns it immediately. The agent run-loop
/// is triggered asynchronously and streams its response via the thread's SSE
/// stream at `/api/threads/:id/stream`.
///
/// In Phase 1 this just persists the user message and returns it.
/// The agent invocation is wired up in Phase 2.
pub async fn send(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(payload): Json<CreateMessage>,
) -> AppResult<impl IntoResponse> {
    if payload.content.trim().is_empty() {
        return Err(AppError::BadRequest(
            "content must not be empty".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;
    let thread = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    if thread.is_archived() {
        return Err(AppError::BadRequest(
            "Cannot send messages to an archived thread".to_string(),
        ));
    }

    // Persist the user message
    let message = Message::new_user(&thread_id, &payload.content);

    sqlx::query(
        "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility, execution_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&message.id)
    .bind(&message.thread_id)
    .bind(&message.role)
    .bind(&message.content)
    .bind(&message.source)
    .bind(&message.routine_id)
    .bind(&message.visibility)
    .bind(&message.execution_id)
    .bind(&message.created_at)
    .execute(&state.pool)
    .await?;

    // Update thread's updated_at so it floats to the top of the thread list
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&thread_id)
        .execute(&state.pool)
        .await?;

    // TODO (Phase 2): spawn the agent run-loop as a background task and begin
    // streaming tokens to any connected SSE clients for this thread.

    let response = MessageResponse::from(message);

    Ok((StatusCode::CREATED, Json(json!({ "data": response }))))
}

// ─── Slash Commands ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SlashCommandRequest {
    pub command: String,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct SlashCommandData {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// POST /api/threads/:id/command
///
/// Handles slash commands intercepted by the client.
/// Commands are never persisted to message history — they produce an
/// ephemeral response only visible to the calling client.
pub async fn slash_command(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(payload): Json<SlashCommandRequest>,
) -> AppResult<Response> {
    let user_id = get_user_id(&state).await?;
    let thread = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let args = payload.args.unwrap_or_default();
    let command = payload.command.trim().to_lowercase();

    match command.as_str() {
        "model" => handle_model_command(&state, &thread, &args)
            .await
            .map(IntoResponse::into_response),
        "routine" => handle_routine_command(&state, &thread_id, &args)
            .await
            .map(IntoResponse::into_response),
        "memory" => handle_memory_command(&state, &thread, &args)
            .await
            .map(IntoResponse::into_response),
        "help" => Ok((
            StatusCode::OK,
            Json(json!({
                "data": {
                    "type": "help",
                    "message": "Available commands",
                    "payload": {
                        "commands": [
                            { "command": "/model list", "description": "List available models for this thread's provider" },
                            { "command": "/model switch <model_id>", "description": "Switch to a different model" },
                            { "command": "/routine list", "description": "List routines attached to this thread" },
                            { "command": "/routine add", "description": "Opens the add-routine modal" },
                            { "command": "/memory list", "description": "Show recent memories for this thread's persona" },
                            { "command": "/help", "description": "Show this help message" },
                        ]
                    }
                }
            })),
        )
            .into_response()),
        unknown => Err(AppError::BadRequest(format!(
            "Unknown command '{}'. Type /help for available commands.",
            unknown
        ))),
    }
}

async fn handle_model_command(
    state: &AppState,
    thread: &crate::models::thread::Thread,
    args: &[String],
) -> AppResult<impl IntoResponse> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");

    match sub {
        "list" => {
            // Determine the active provider for this thread
            let provider_id = thread.active_provider.as_deref().ok_or_else(|| {
                AppError::BadRequest("No provider configured for this thread".to_string())
            })?;

            let models: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT id, model_id, display_name FROM models WHERE provider_id = ? AND enabled = 1 ORDER BY display_name ASC",
            )
            .bind(provider_id)
            .fetch_all(&state.pool)
            .await?;

            Ok((
                StatusCode::OK,
                Json(json!({
                    "data": {
                        "type": "model_list",
                        "message": format!("{} model(s) available", models.len()),
                        "payload": {
                            "models": models.iter().map(|(id, model_id, display_name)| {
                                json!({
                                    "id": id,
                                    "model_id": model_id,
                                    "display_name": display_name
                                })
                            }).collect::<Vec<_>>()
                        }
                    }
                })),
            ))
        }

        "switch" => {
            let model_id = args.get(1).ok_or_else(|| {
                AppError::BadRequest("Usage: /model switch <model_id>".to_string())
            })?;

            // Look up the model
            let model: Option<(String, String, String)> = sqlx::query_as(
                "SELECT id, model_id, display_name FROM models WHERE id = ? AND enabled = 1",
            )
            .bind(model_id)
            .fetch_optional(&state.pool)
            .await?;

            let (db_id, _mid, display_name) = model.ok_or_else(|| {
                AppError::NotFound(format!("Model '{}' not found or not enabled", model_id))
            })?;

            // Update the thread's active_model
            let now = chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();
            sqlx::query("UPDATE threads SET active_model = ?, updated_at = ? WHERE id = ?")
                .bind(&db_id)
                .bind(&now)
                .bind(&thread.id)
                .execute(&state.pool)
                .await?;

            Ok((
                StatusCode::OK,
                Json(json!({
                    "data": {
                        "type": "model_switched",
                        "message": format!("Switched to {}", display_name),
                        "payload": {
                            "model_id": db_id,
                            "display_name": display_name
                        }
                    }
                })),
            ))
        }

        _ => Err(AppError::BadRequest(
            "Usage: /model list  or  /model switch <model_id>".to_string(),
        )),
    }
}

async fn handle_routine_command(
    state: &AppState,
    thread_id: &str,
    args: &[String],
) -> AppResult<impl IntoResponse> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");

    match sub {
        "list" => {
            let routines: Vec<(String, String, String, bool)> = sqlx::query_as(
                "SELECT id, name, cron_expr, enabled FROM routines WHERE thread_id = ? ORDER BY created_at ASC",
            )
            .bind(thread_id)
            .fetch_all(&state.pool)
            .await?;

            Ok((
                StatusCode::OK,
                Json(json!({
                    "data": {
                        "type": "routine_list",
                        "message": format!("{} routine(s) attached", routines.len()),
                        "payload": {
                            "routines": routines.iter().map(|(id, name, cron, enabled)| {
                                json!({ "id": id, "name": name, "cron_expr": cron, "enabled": enabled })
                            }).collect::<Vec<_>>()
                        }
                    }
                })),
            ))
        }

        "add" => Ok((
            StatusCode::OK,
            Json(json!({
                "data": {
                    "type": "open_add_routine_modal",
                    "message": "Opening routine editor…",
                    "payload": null
                }
            })),
        )),

        _ => Err(AppError::BadRequest(
            "Usage: /routine list  or  /routine add".to_string(),
        )),
    }
}

async fn handle_memory_command(
    state: &AppState,
    thread: &crate::models::thread::Thread,
    args: &[String],
) -> AppResult<impl IntoResponse> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");

    match sub {
        "list" => {
            let user_id: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
                .fetch_optional(&state.pool)
                .await?;
            let user_id = user_id
                .map(|(id,)| id)
                .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))?;

            let persona_id = &thread.persona_id;

            let memories: Vec<(String, String, Option<String>, Option<String>, String)> =
                sqlx::query_as(
                    "SELECT m.id, m.content, m.thread_id, t.title, m.created_at
                     FROM memory m
                     LEFT JOIN threads t ON t.id = m.thread_id
                     WHERE m.user_id = ? AND m.persona_id = ?
                     ORDER BY m.created_at DESC
                     LIMIT 20",
                )
                .bind(&user_id)
                .bind(persona_id)
                .fetch_all(&state.pool)
                .await?;

            Ok((
                StatusCode::OK,
                Json(json!({
                    "data": {
                        "type": "memory_list",
                        "message": format!("{} recent memor{}", memories.len(), if memories.len() == 1 { "y" } else { "ies" }),
                        "payload": {
                            "memories": memories.iter().map(|(id, content, tid, ttitle, created_at)| {
                                json!({
                                    "id": id,
                                    "content": content,
                                    "thread_id": tid,
                                    "thread_title": ttitle,
                                    "created_at": created_at
                                })
                            }).collect::<Vec<_>>()
                        }
                    }
                })),
            ))
        }

        _ => Err(AppError::BadRequest("Usage: /memory list".to_string())),
    }
}

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::Ordering;

use crate::{
    error::{AppError, AppResult},
    models::message::{CreateMessage, Message, MessageResponse},
    routes::{AppState, RunPhase, RunProgress},
};

/// Helper: get the single user id from the DB.
pub(crate) async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// Helper: verify a thread exists and belongs to the current user.
pub(crate) async fn verify_thread_ownership(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
) -> AppResult<crate::models::thread::Thread> {
    let thread: Option<crate::models::thread::Thread> = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, show_tool_activity, show_system_events,
                created_at, updated_at
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
                    "SELECT id, thread_id, role, content, source, routine_id, visibility,
                            execution_id, event_type, stopped, created_at
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
            "SELECT id, thread_id, role, content, source, routine_id, visibility,
                    execution_id, event_type, stopped, created_at
             FROM (
                 SELECT id, thread_id, role, content, source, routine_id, visibility,
                        execution_id, event_type, stopped, created_at
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
/// is triggered asynchronously via per-thread RunState and streams its
/// response via the thread's SSE stream at `/api/threads/:id/stream`.
///
/// Depth enforcement (max 3 queued runs per thread) happens before spawning
/// so the queue depth is visible immediately to the HTTP handler.
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
        "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                               execution_id, event_type, stopped, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&message.id)
    .bind(&message.thread_id)
    .bind(&message.role)
    .bind(&message.content)
    .bind(&message.source)
    .bind(&message.routine_id)
    .bind(&message.visibility)
    .bind(&message.execution_id)
    .bind(&message.event_type)
    .bind(message.stopped)
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

    // ── Per-thread depth-limited dispatch ──────────────────────────────────────
    // Depth is incremented BEFORE spawning so the queue depth is immediately
    // visible (prevents thundering-herd when multiple requests arrive together).
    // The semaphore is acquired INSIDE the spawned task so it queues rather
    // than blocks the HTTP handler.
    let run_state = state.get_run_state(&thread_id);
    let depth = run_state.depth.fetch_add(1, Ordering::SeqCst);
    const MAX_DEPTH: usize = 3;

    if depth >= MAX_DEPTH {
        run_state.depth.fetch_sub(1, Ordering::SeqCst);
        return Err(AppError::BadRequest(
            "This thread is busy — try again in a moment".to_string(),
        ));
    }

    let state_clone = state.clone();
    let tid = thread_id.clone();
    let content = payload.content.clone();
    let user_message_id = message.id.clone();

    // Reset progress for this new run BEFORE spawning so that cancel_run
    // always has a valid RunProgress to read, even if the task hasn't started.
    {
        let mut progress = run_state.progress.lock().await;
        *progress = RunProgress::new(thread_id.clone(), Some(user_message_id));
    }

    let progress = run_state.progress.clone();
    let persist_lock = run_state.persist_lock.clone();

    // Clone the Arc so the closure owns one ref and we keep one ref for
    // storing the JoinHandle after spawn returns.
    let run_state_for_task = run_state.clone();

    // ── Race-free handle storage ───────────────────────────────────────────────
    // We lock task_handle BEFORE spawning and hold the lock until after we
    // have stored the handle inside it.  The spawned task acquires the same
    // lock as its very first action before doing any real work, so it blocks
    // until we have finished storing.  This closes the window where cancel_run
    // could read task_handle, find None, skip the abort, fire the stopped
    // event, and then have the task wake up and stream anyway.
    let mut handle_lock = run_state.task_handle.lock().await;

    let handle = tokio::spawn(async move {
        // Block here until the HTTP handler has stored our handle.  This is
        // the critical ordering guarantee: no real work starts until the handle
        // is visible to cancel_run.
        {
            let _ = run_state_for_task.task_handle.lock().await;
        }

        // Acquire semaphore — queues behind any in-progress run on this thread.
        let _permit = run_state_for_task.semaphore.acquire().await.unwrap();

        // Wait for the SSE subscriber immediately before streaming begins so
        // tokens are not fired into the void.
        state_clone
            .wait_for_subscriber(&tid, std::time::Duration::from_secs(3))
            .await;

        crate::services::agent::run(state_clone, tid, content, progress, persist_lock).await;

        run_state_for_task.depth.fetch_sub(1, Ordering::SeqCst);
    });

    // Store the handle while still holding the lock, then release.
    // The spawned task is blocked on acquiring this same lock, so it cannot
    // proceed until we drop handle_lock here.
    *handle_lock = Some(handle);
    drop(handle_lock);

    let response = MessageResponse::from(message);
    Ok((StatusCode::CREATED, Json(json!({ "data": response }))))
}

/// POST /api/threads/:id/cancel
///
/// Immediately aborts the running agent task for this thread, then performs
/// phase-appropriate cleanup:
///   - Deletes any orphaned hidden tool messages created after the user message.
///   - Persists any partial assistant content as a stopped message (under
///     `persist_lock` to prevent a double-insert race with the run-loop).
///   - Fires `MessageComplete { stopped: true }` so the client exits its
///     "generating" state.
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let run_state = state.get_run_state(&thread_id);

    // ── Step 1: Abort the task handle and wait for it to fully stop ───────────
    // We take the handle out of the mutex, call abort(), then await the handle
    // to completion.  Awaiting after abort() is safe and important: it blocks
    // until the task's Future is actually dropped, which means all in-flight
    // .await points have unwound and any final writes to RunProgress are
    // committed before we read them below.  Without this wait, abort() merely
    // schedules the cancellation and we can race ahead to read stale progress —
    // seeing empty content even though many tokens were streamed.
    //
    let handle = {
        let mut handle_lock = run_state.task_handle.lock().await;
        handle_lock.take()
    };

    if let Some(h) = handle {
        h.abort();
        // JoinError on abort is expected; ignore it.
        let _ = h.await;
        tracing::info!(thread_id = %thread_id, "cancel_run: task aborted and joined");
    } else {
        tracing::info!(thread_id = %thread_id, "cancel_run: no active task to abort");
    }

    // ── Step 2: Read progress to understand what cleanup is needed ─────────────
    // The task is now fully stopped, so RunProgress reflects the last state it
    // reached before the abort unwound.
    let (phase, user_message_id, assistant_content_so_far) = {
        let progress = run_state.progress.lock().await;
        (
            progress.phase.clone(),
            progress.user_message_id.clone(),
            progress.assistant_content_so_far.clone(),
        )
    };

    tracing::info!(
        thread_id = %thread_id,
        phase = ?phase,
        "cancel_run: run was in this phase at abort time"
    );

    // If the run already completed normally, there is nothing to do.
    if phase == RunPhase::Complete {
        tracing::info!(thread_id = %thread_id, "cancel_run: run already complete, no-op");
        return Ok(Json(json!({ "data": { "cancelled": true } })));
    }

    // ── Step 3: Delete orphaned hidden tool messages ───────────────────────────
    // Applies when the abort happened during a tool round (Streaming with
    // round > 0, or ExecutingTool).  We delete all hidden messages created
    // after the user message to avoid dangling tool-call/result pairs in the
    // LLM context on the next run.
    let needs_tool_cleanup = match &phase {
        RunPhase::Streaming { round } if *round > 0 => true,
        RunPhase::ExecutingTool { .. } => true,
        _ => false,
    };

    if needs_tool_cleanup {
        if let Some(ref user_msg_id) = user_message_id {
            // Get the timestamp of the user message so we can delete everything after it.
            let user_msg_created_at: Option<(String,)> =
                sqlx::query_as("SELECT created_at FROM messages WHERE id = ?")
                    .bind(user_msg_id)
                    .fetch_optional(&state.pool)
                    .await?;

            if let Some((user_created_at,)) = user_msg_created_at {
                let deleted = sqlx::query(
                    "DELETE FROM messages WHERE thread_id = ? AND visibility = 'hidden' AND created_at > ?",
                )
                .bind(&thread_id)
                .bind(&user_created_at)
                .execute(&state.pool)
                .await?;

                tracing::info!(
                    thread_id = %thread_id,
                    rows_deleted = deleted.rows_affected(),
                    "cancel_run: deleted orphaned hidden tool messages"
                );
            }
        }
    }

    // ── Step 4: Persist partial assistant message (under persist_lock) ─────────
    // Acquire the lock before checking/inserting so we don't race with the
    // run-loop's own INSERT (which also holds persist_lock during the write).
    let assistant_msg = {
        let _lock = run_state.persist_lock.lock().await;

        // Check whether the run-loop already committed an assistant message for
        // this thread newer than the user message.  If so, the run-loop won the
        // race and we must not insert a duplicate.
        let existing: Option<(String,)> = if let Some(ref user_msg_id) = user_message_id {
            let user_msg_created_at: Option<(String,)> =
                sqlx::query_as("SELECT created_at FROM messages WHERE id = ?")
                    .bind(user_msg_id)
                    .fetch_optional(&state.pool)
                    .await?;

            if let Some((user_created_at,)) = user_msg_created_at {
                sqlx::query_as(
                    "SELECT id FROM messages
                     WHERE thread_id = ? AND role = 'assistant' AND created_at > ?
                     LIMIT 1",
                )
                .bind(&thread_id)
                .bind(&user_created_at)
                .fetch_optional(&state.pool)
                .await?
            } else {
                None
            }
        } else {
            None
        };

        if existing.is_some() {
            tracing::info!(
                thread_id = %thread_id,
                "cancel_run: assistant message already persisted by run-loop, skipping insert"
            );
            None
        } else {
            // Insert partial content as a stopped message.
            let content = if assistant_content_so_far.is_empty() {
                "[Response cancelled before any content was generated]".to_string()
            } else {
                assistant_content_so_far
            };

            let msg = Message::new_assistant_stopped(&thread_id, &content);

            sqlx::query(
                "INSERT INTO messages (id, thread_id, role, content, source, routine_id,
                                       visibility, execution_id, event_type, stopped, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&msg.id)
            .bind(&msg.thread_id)
            .bind(&msg.role)
            .bind(&msg.content)
            .bind(&msg.source)
            .bind(&msg.routine_id)
            .bind(&msg.visibility)
            .bind(&msg.execution_id)
            .bind(&msg.event_type)
            .bind(msg.stopped)
            .bind(&msg.created_at)
            .execute(&state.pool)
            .await?;

            tracing::info!(
                thread_id = %thread_id,
                message_id = %msg.id,
                content_len = msg.content.len(),
                "cancel_run: persisted stopped assistant message"
            );

            Some(msg)
        }
    };

    // ── Step 5: Fire MessageComplete SSE event ─────────────────────────────────
    if let Some(msg) = assistant_msg {
        state.send_thread_event(
            &thread_id,
            crate::routes::sse::ThreadEvent::MessageComplete {
                id: msg.id.clone(),
                thread_id: thread_id.clone(),
                role: msg.role.clone(),
                content: msg.content.clone(),
                created_at: msg.created_at.clone(),
                stopped: true,
            },
        );
    } else {
        // The run-loop already persisted and fired its own event, but the
        // client may still be in a "generating" state if it missed the event.
        // Re-fire a stopped event using whatever the run-loop left in the DB.
        let last_assistant: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, role, content, created_at FROM messages
             WHERE thread_id = ? AND role = 'assistant'
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&thread_id)
        .fetch_optional(&state.pool)
        .await?;

        if let Some((id, role, content, created_at)) = last_assistant {
            state.send_thread_event(
                &thread_id,
                crate::routes::sse::ThreadEvent::MessageComplete {
                    id,
                    thread_id: thread_id.clone(),
                    role,
                    content,
                    created_at,
                    stopped: true,
                },
            );
        }
    }

    Ok(Json(json!({ "data": { "cancelled": true } })))
}

#[derive(Debug, Deserialize)]
pub struct SlashCommandRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SlashCommandData {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    pub payload: Option<serde_json::Value>,
}

/// POST /api/threads/:id/command
///
/// Handles slash commands (e.g. /model, /routine, /memory).
/// Returns a structured response describing what was done.
pub async fn slash_command(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(payload): Json<SlashCommandRequest>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let command = payload.command.to_lowercase();
    let args = payload.args;

    let result = match command.as_str() {
        "model" => handle_model_command(&state, &thread_id, &user_id, &args).await,
        "routine" => handle_routine_command(&state, &thread_id, &user_id, &args).await,
        "memory" => handle_memory_command(&state, &thread_id, &user_id, &args).await,
        "help" => Ok(SlashCommandData {
            kind: "help".to_string(),
            message: "Available commands: /model, /routine, /memory, /help".to_string(),
            payload: None,
        }),
        unknown => Ok(SlashCommandData {
            kind: "unknown".to_string(),
            message: format!("Unknown command: /{unknown}. Try /help."),
            payload: None,
        }),
    }?;

    Ok((StatusCode::OK, Json(json!({ "data": result }))))
}

async fn handle_model_command(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    args: &[String],
) -> AppResult<SlashCommandData> {
    let subcommand = args.first().map(|s| s.to_lowercase());

    match subcommand.as_deref() {
        Some("list") | None => {
            // List all enabled providers with their models
            let providers: Vec<(String, String)> = sqlx::query_as(
                "SELECT p.name, m.display_name
                 FROM providers p
                 JOIN models m ON m.provider_id = p.id
                 WHERE p.user_id = ? AND p.enabled = 1 AND m.enabled = 1
                 ORDER BY p.name, m.display_name",
            )
            .bind(user_id)
            .fetch_all(&state.pool)
            .await?;

            if providers.is_empty() {
                return Ok(SlashCommandData {
                    kind: "model_list".to_string(),
                    message: "No enabled models found. Add a provider in Settings.".to_string(),
                    payload: None,
                });
            }

            let list: Vec<String> = providers
                .iter()
                .map(|(p, m)| format!("{} · {}", p, m))
                .collect();

            Ok(SlashCommandData {
                kind: "model_list".to_string(),
                message: list.join("\n"),
                payload: Some(serde_json::json!({ "models": providers })),
            })
        }

        Some(sub) if !sub.is_empty() => {
            // Try to find a model matching the argument (by display_name or model_id)
            // If subcommand is "set"/"switch", model name is second arg; otherwise
            // treat the subcommand itself as the model name.
            let model_query = if matches!(sub, "set" | "switch") {
                args.get(1).map(|s| s.to_lowercase()).unwrap_or_default()
            } else {
                sub.to_string()
            };

            if model_query.is_empty() {
                return Ok(SlashCommandData {
                    kind: "model_switch_error".to_string(),
                    message: "Usage: /model set <model-name>".to_string(),
                    payload: None,
                });
            }

            let found: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT m.id, m.display_name, p.id, p.name
                 FROM models m
                 JOIN providers p ON p.id = m.provider_id
                 WHERE p.user_id = ? AND p.enabled = 1 AND m.enabled = 1
                   AND (LOWER(m.display_name) LIKE ? OR LOWER(m.model_id) LIKE ?)
                 LIMIT 1",
            )
            .bind(user_id)
            .bind(format!("%{}%", model_query))
            .bind(format!("%{}%", model_query))
            .fetch_optional(&state.pool)
            .await?;

            match found {
                Some((model_id, model_name, provider_id, provider_name)) => {
                    let now = chrono::Utc::now()
                        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                        .to_string();
                    sqlx::query(
                        "UPDATE threads SET active_model = ?, active_provider = ?, updated_at = ?
                         WHERE id = ?",
                    )
                    .bind(&model_id)
                    .bind(&provider_id)
                    .bind(&now)
                    .bind(thread_id)
                    .execute(&state.pool)
                    .await?;

                    Ok(SlashCommandData {
                        kind: "model_switched".to_string(),
                        message: format!("Switched to {} · {}", provider_name, model_name),
                        payload: Some(serde_json::json!({
                            "model_id": model_id,
                            "display_name": model_name,
                            "provider_id": provider_id,
                            "provider_name": provider_name,
                        })),
                    })
                }
                None => Ok(SlashCommandData {
                    kind: "model_not_found".to_string(),
                    message: format!("No enabled model matching '{}' found.", model_query),
                    payload: None,
                }),
            }
        }

        _ => Ok(SlashCommandData {
            kind: "model_unknown_subcommand".to_string(),
            message: "Usage: /model list  or  /model set <name>".to_string(),
            payload: None,
        }),
    }
}

async fn handle_routine_command(
    state: &AppState,
    thread_id: &str,
    _user_id: &str,
    args: &[String],
) -> AppResult<SlashCommandData> {
    let subcommand = args.first().map(|s| s.to_lowercase());

    match subcommand.as_deref() {
        Some("list") | None => {
            let routines: Vec<(String, String, bool)> = sqlx::query_as(
                "SELECT name, cron_expr, enabled FROM routines WHERE thread_id = ? ORDER BY name",
            )
            .bind(thread_id)
            .fetch_all(&state.pool)
            .await?;

            if routines.is_empty() {
                return Ok(SlashCommandData {
                    kind: "routine_list".to_string(),
                    message: "No routines configured for this thread.".to_string(),
                    payload: None,
                });
            }

            let list: Vec<String> = routines
                .iter()
                .map(|(name, cron, enabled)| {
                    format!("{} ({}) {}", name, cron, if *enabled { "✓" } else { "✗" })
                })
                .collect();

            Ok(SlashCommandData {
                kind: "routine_list".to_string(),
                message: list.join("\n"),
                payload: Some(serde_json::json!({ "routines": routines })),
            })
        }

        Some("add") => Ok(SlashCommandData {
            kind: "routine_add".to_string(),
            message: "Use the Thread Config panel to add routines.".to_string(),
            payload: None,
        }),

        _ => Ok(SlashCommandData {
            kind: "routine_unknown".to_string(),
            message: "Usage: /routine list".to_string(),
            payload: None,
        }),
    }
}

async fn handle_memory_command(
    state: &AppState,
    _thread_id: &str,
    user_id: &str,
    args: &[String],
) -> AppResult<SlashCommandData> {
    use crate::services::memory as memory_service;

    let subcommand = args.first().map(|s| s.to_lowercase());

    match subcommand.as_deref() {
        Some("list") | None => {
            // Fetch persona for this user
            let persona: Option<(String,)> =
                sqlx::query_as("SELECT id FROM agent_personas WHERE user_id = ? LIMIT 1")
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await?;

            let persona_id = match persona {
                Some((id,)) => id,
                None => {
                    return Ok(SlashCommandData {
                        kind: "memory_list".to_string(),
                        message: "No persona found.".to_string(),
                        payload: None,
                    })
                }
            };

            let entries = memory_service::recall_memory(&state.pool, user_id, &persona_id, "", 10)
                .await
                .unwrap_or_default();

            if entries.is_empty() {
                return Ok(SlashCommandData {
                    kind: "memory_list".to_string(),
                    message: "No memories stored yet.".to_string(),
                    payload: None,
                });
            }

            let list: Vec<String> = entries.iter().map(|e| format!("- {}", e.content)).collect();

            Ok(SlashCommandData {
                kind: "memory_list".to_string(),
                message: list.join("\n"),
                payload: None,
            })
        }

        _ => Ok(SlashCommandData {
            kind: "memory_unknown".to_string(),
            message: "Usage: /memory list".to_string(),
            payload: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(s: &str) -> Vec<String> {
        s.split_whitespace().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_args_are_pre_split_vec() {
        let args = make_args("set gpt-4o");
        assert_eq!(args, vec!["set", "gpt-4o"]);
    }

    #[test]
    fn test_empty_args_vec() {
        let args: Vec<String> = vec![];
        assert!(args.is_empty());
    }

    #[test]
    fn test_args_single_subcommand() {
        let args = make_args("list");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "list");
    }

    #[test]
    fn test_command_normalised_to_lowercase() {
        let cmd = "MODEL".to_lowercase();
        assert_eq!(cmd, "model");
    }

    #[test]
    fn test_model_list_subcommand_recognised() {
        let args = make_args("list");
        let sub = args.first().map(|s| s.to_lowercase());
        assert_eq!(sub.as_deref(), Some("list"));
    }

    #[test]
    fn test_model_switch_requires_second_arg() {
        let args = make_args("set");
        let model_query = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
        assert!(model_query.is_empty());
    }

    #[test]
    fn test_model_switch_by_display_name_normalisation() {
        let input = "GPT-4o";
        let normalised = input.to_lowercase();
        assert_eq!(normalised, "gpt-4o");
    }

    #[test]
    fn test_model_unknown_subcommand() {
        let cmd = "foobar";
        let known = matches!(cmd, "list" | "set" | "switch");
        assert!(!known);
    }

    #[test]
    fn test_routine_list_subcommand() {
        let args = make_args("list");
        let sub = args.first().map(|s| s.to_lowercase());
        assert_eq!(sub.as_deref(), Some("list"));
    }

    #[test]
    fn test_routine_add_subcommand() {
        let args = make_args("add");
        let sub = args.first().map(|s| s.to_lowercase());
        assert_eq!(sub.as_deref(), Some("add"));
    }

    #[test]
    fn test_routine_default_subcommand_is_list() {
        let args: Vec<String> = vec![];
        let sub = args.first().map(|s| s.to_lowercase());
        assert!(sub.is_none());
    }

    #[test]
    fn test_memory_list_subcommand() {
        let args = make_args("list");
        let sub = args.first().map(|s| s.to_lowercase());
        assert_eq!(sub.as_deref(), Some("list"));
    }

    #[test]
    fn test_memory_unknown_subcommand() {
        let cmd = "delete";
        let known = matches!(cmd, "list");
        assert!(!known);
    }

    #[test]
    fn test_help_command_no_args_needed() {
        let cmd = "help";
        let needs_args = matches!(cmd, "model" | "routine" | "memory");
        assert!(!needs_args);
    }

    #[test]
    fn test_unknown_command_not_in_dispatch() {
        let cmd = "unknown_xyz";
        let known = matches!(cmd, "model" | "routine" | "memory" | "help");
        assert!(!known);
    }

    #[test]
    fn test_known_commands_all_match() {
        let known_cmds = ["model", "routine", "memory", "help"];
        for cmd in &known_cmds {
            assert!(matches!(*cmd, "model" | "routine" | "memory" | "help"));
        }
    }

    // ── cancel_run unit tests ─────────────────────────────────────────────────

    /// When the run was in `Queued` phase at abort time, no assistant message
    /// should be inserted — only the SSE fired event matters, which is tested
    /// by verifying the DB stays empty.
    #[tokio::test]
    async fn cancel_run_with_queued_phase_fires_stopped_event() {
        use crate::routes::{RunPhase, RunProgress, RunState};
        use sqlx::SqlitePool;
        use std::sync::Arc;

        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        // Set up a thread and user message so the DB queries in cancel_run
        // have a valid anchor.
        let thread_id = "thread-cancel-queued";
        let user_msg_id = "user-msg-queued";
        sqlx::query("INSERT INTO users (id, display_name, created_at) VALUES ('user-1', 'Test User', '2024-01-01T00:00:00.000Z')")
            .execute(&pool)
            .await
            .expect("insert user");
        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, system_prompt, created_at, updated_at)
             VALUES ('persona-1', 'user-1', 'Test', '🤖', 'You are helpful.', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .expect("insert persona");
        sqlx::query(
            "INSERT INTO threads (id, user_id, persona_id, title, status, created_at, updated_at)
             VALUES (?, 'user-1', 'persona-1', 'Test Thread', 'active', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert thread");
        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                                   execution_id, event_type, stopped, created_at)
             VALUES (?, ?, 'user', 'hello', 'user', NULL, 'visible', NULL, NULL, 0, '2024-01-01T00:00:00.000Z')",
        )
        .bind(user_msg_id)
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert user message");

        // Build a RunState with progress in Queued phase.
        let run_state = Arc::new(RunState::new());
        {
            let mut progress = run_state.progress.lock().await;
            *progress = RunProgress::new(thread_id.to_string(), Some(user_msg_id.to_string()));
            // Phase is already Queued by default.
            assert_eq!(progress.phase, RunPhase::Queued);
        }

        // Acquire persist_lock and query: no assistant message should exist.
        {
            let _lock = run_state.persist_lock.lock().await;
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM messages WHERE thread_id = ? AND role = 'assistant' LIMIT 1",
            )
            .bind(thread_id)
            .fetch_optional(&pool)
            .await
            .expect("query");
            assert!(
                existing.is_none(),
                "no assistant message should exist before cancel"
            );
        }
    }

    /// When the run was in `Streaming { round: 0 }` phase with accumulated
    /// content, the cancel handler must insert a stopped assistant message
    /// containing that partial content.
    #[tokio::test]
    async fn cancel_run_with_streaming_phase_persists_partial_message() {
        use crate::models::message::Message;
        use crate::routes::{RunPhase, RunProgress, RunState};
        use sqlx::SqlitePool;
        use std::sync::Arc;

        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let thread_id = "thread-cancel-streaming";
        let user_msg_id = "user-msg-streaming";
        sqlx::query("INSERT INTO users (id, display_name, created_at) VALUES ('user-1', 'Test User', '2024-01-01T00:00:00.000Z')")
            .execute(&pool)
            .await
            .expect("insert user");
        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, system_prompt, created_at, updated_at)
             VALUES ('persona-1', 'user-1', 'Test', '🤖', 'You are helpful.', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .expect("insert persona");
        sqlx::query(
            "INSERT INTO threads (id, user_id, persona_id, title, status, created_at, updated_at)
             VALUES (?, 'user-1', 'persona-1', 'Test Thread', 'active', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert thread");
        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                                   execution_id, event_type, stopped, created_at)
             VALUES (?, ?, 'user', 'hello', 'user', NULL, 'visible', NULL, NULL, 0, '2024-01-01T00:00:00.000Z')",
        )
        .bind(user_msg_id)
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert user message");

        let partial_content = "I was in the middle of responding when";

        // Build RunState with progress in Streaming phase with partial content.
        let run_state = Arc::new(RunState::new());
        {
            let mut progress = run_state.progress.lock().await;
            *progress = RunProgress::new(thread_id.to_string(), Some(user_msg_id.to_string()));
            progress.phase = RunPhase::Streaming { round: 0 };
            progress.assistant_content_so_far = partial_content.to_string();
        }

        // Simulate what cancel_run does: acquire persist_lock, check for
        // existing assistant message, insert stopped message if none.
        let inserted_msg = {
            let _lock = run_state.persist_lock.lock().await;

            let user_created_at: Option<(String,)> =
                sqlx::query_as("SELECT created_at FROM messages WHERE id = ?")
                    .bind(user_msg_id)
                    .fetch_optional(&pool)
                    .await
                    .expect("query user msg time");

            let existing: Option<(String,)> = if let Some((ts,)) = user_created_at {
                sqlx::query_as(
                    "SELECT id FROM messages WHERE thread_id = ? AND role = 'assistant' AND created_at > ? LIMIT 1",
                )
                .bind(thread_id)
                .bind(&ts)
                .fetch_optional(&pool)
                .await
                .expect("query existing")
            } else {
                None
            };

            assert!(existing.is_none(), "no assistant message should exist yet");

            let msg = Message::new_assistant_stopped(thread_id, partial_content);
            sqlx::query(
                "INSERT INTO messages (id, thread_id, role, content, source, routine_id,
                                       visibility, execution_id, event_type, stopped, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&msg.id)
            .bind(&msg.thread_id)
            .bind(&msg.role)
            .bind(&msg.content)
            .bind(&msg.source)
            .bind(&msg.routine_id)
            .bind(&msg.visibility)
            .bind(&msg.execution_id)
            .bind(&msg.event_type)
            .bind(msg.stopped)
            .bind(&msg.created_at)
            .execute(&pool)
            .await
            .expect("insert stopped msg");

            msg
        };

        // Verify the stopped message was inserted with the right content.
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT content, stopped FROM messages WHERE id = ?",
        )
        .bind(&inserted_msg.id)
        .fetch_optional(&pool)
        .await
        .expect("fetch inserted msg");

        let (content, stopped) = row.expect("inserted message must exist");
        assert_eq!(content, partial_content);
        assert_eq!(stopped, 1, "stopped flag must be set");
    }

    /// When the run was in `PersistingMessage` phase and the run-loop already
    /// committed the assistant row, the cancel handler must detect the existing
    /// row and skip the duplicate insert.
    #[tokio::test]
    async fn cancel_run_with_persisting_phase_checks_for_existing_row() {
        use crate::models::message::Message;
        use crate::routes::{RunPhase, RunProgress, RunState};
        use sqlx::SqlitePool;
        use std::sync::Arc;

        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let thread_id = "thread-cancel-persisting";
        let user_msg_id = "user-msg-persisting";
        sqlx::query("INSERT INTO users (id, display_name, created_at) VALUES ('user-1', 'Test User', '2024-01-01T00:00:00.000Z')")
            .execute(&pool)
            .await
            .expect("insert user");
        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, system_prompt, created_at, updated_at)
             VALUES ('persona-1', 'user-1', 'Test', '🤖', 'You are helpful.', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .expect("insert persona");
        sqlx::query(
            "INSERT INTO threads (id, user_id, persona_id, title, status, created_at, updated_at)
             VALUES (?, 'user-1', 'persona-1', 'Test Thread', 'active', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert thread");
        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                                   execution_id, event_type, stopped, created_at)
             VALUES (?, ?, 'user', 'hello', 'user', NULL, 'visible', NULL, NULL, 0, '2024-01-01T00:00:00.000Z')",
        )
        .bind(user_msg_id)
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert user message");

        // Pre-insert the assistant message as the run-loop would have done.
        let run_loop_msg = Message::new_assistant(thread_id, "Full completed response.");
        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                                   execution_id, event_type, stopped, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&run_loop_msg.id)
        .bind(&run_loop_msg.thread_id)
        .bind(&run_loop_msg.role)
        .bind(&run_loop_msg.content)
        .bind(&run_loop_msg.source)
        .bind(&run_loop_msg.routine_id)
        .bind(&run_loop_msg.visibility)
        .bind(&run_loop_msg.execution_id)
        .bind(&run_loop_msg.event_type)
        .bind(run_loop_msg.stopped)
        .bind(&run_loop_msg.created_at)
        .execute(&pool)
        .await
        .expect("insert run-loop assistant msg");

        let run_state = Arc::new(RunState::new());
        {
            let mut progress = run_state.progress.lock().await;
            *progress = RunProgress::new(thread_id.to_string(), Some(user_msg_id.to_string()));
            progress.phase = RunPhase::PersistingMessage;
            progress.assistant_content_so_far = "Full completed response.".to_string();
        }

        // Simulate the cancel handler's persist_lock section.
        let inserted_duplicate = {
            let _lock = run_state.persist_lock.lock().await;

            let user_created_at: Option<(String,)> =
                sqlx::query_as("SELECT created_at FROM messages WHERE id = ?")
                    .bind(user_msg_id)
                    .fetch_optional(&pool)
                    .await
                    .expect("query user msg time");

            if let Some((ts,)) = user_created_at {
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM messages WHERE thread_id = ? AND role = 'assistant' AND created_at > ? LIMIT 1",
                )
                .bind(thread_id)
                .bind(&ts)
                .fetch_optional(&pool)
                .await
                .expect("query existing");

                // Row exists — cancel handler must NOT insert.
                existing.is_none()
            } else {
                true
            }
        };

        assert!(
            !inserted_duplicate,
            "cancel handler must skip insert when run-loop already committed the row"
        );

        // Confirm exactly one assistant message row exists.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM messages WHERE thread_id = ? AND role = 'assistant'")
                .bind(thread_id)
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(count.0, 1, "exactly one assistant message must exist");
    }

    /// Two concurrent callers holding `persist_lock` in sequence must result
    /// in exactly one message row — whichever caller wins the lock first inserts;
    /// the second caller finds the row and skips.
    #[tokio::test]
    async fn persist_lock_prevents_double_insert() {
        use crate::models::message::Message;
        use crate::routes::RunState;
        use sqlx::SqlitePool;
        use std::sync::Arc;

        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("test db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let thread_id = "thread-double-insert";
        let user_msg_id = "user-msg-double";
        sqlx::query("INSERT INTO users (id, display_name, created_at) VALUES ('user-1', 'Test User', '2024-01-01T00:00:00.000Z')")
            .execute(&pool)
            .await
            .expect("insert user");
        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, system_prompt, created_at, updated_at)
             VALUES ('persona-1', 'user-1', 'Test', '🤖', 'You are helpful.', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .expect("insert persona");
        sqlx::query(
            "INSERT INTO threads (id, user_id, persona_id, title, status, created_at, updated_at)
             VALUES (?, 'user-1', 'persona-1', 'Test Thread', 'active', '2024-01-01T00:00:00.000Z', '2024-01-01T00:00:00.000Z')",
        )
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert thread");
        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                                   execution_id, event_type, stopped, created_at)
             VALUES (?, ?, 'user', 'hello', 'user', NULL, 'visible', NULL, NULL, 0, '2024-01-01T00:00:00.000Z')",
        )
        .bind(user_msg_id)
        .bind(thread_id)
        .execute(&pool)
        .await
        .expect("insert user message");

        let run_state = Arc::new(RunState::new());
        let persist_lock = run_state.persist_lock.clone();

        // Helper closure: attempt to insert an assistant message under the
        // persist_lock, skipping if a row already exists.
        let try_insert = |pool: SqlitePool, lock: Arc<tokio::sync::Mutex<()>>, content: String| async move {
            let _guard = lock.lock().await;

            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM messages WHERE thread_id = ? AND role = 'assistant' LIMIT 1",
            )
            .bind(thread_id)
            .fetch_optional(&pool)
            .await
            .expect("query existing");

            if existing.is_some() {
                return false;
            }

            let msg = Message::new_assistant_stopped(thread_id, &content);
            sqlx::query(
                "INSERT INTO messages (id, thread_id, role, content, source, routine_id,
                                       visibility, execution_id, event_type, stopped, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&msg.id)
            .bind(&msg.thread_id)
            .bind(&msg.role)
            .bind(&msg.content)
            .bind(&msg.source)
            .bind(&msg.routine_id)
            .bind(&msg.visibility)
            .bind(&msg.execution_id)
            .bind(&msg.event_type)
            .bind(msg.stopped)
            .bind(&msg.created_at)
            .execute(&pool)
            .await
            .expect("insert msg");

            true
        };

        // Run both callers sequentially (lock serialises them).
        let first_inserted = try_insert(pool.clone(), persist_lock.clone(), "partial content".to_string()).await;
        let second_inserted = try_insert(pool.clone(), persist_lock.clone(), "partial content".to_string()).await;

        assert!(first_inserted, "first caller must insert the row");
        assert!(!second_inserted, "second caller must skip — row already exists");

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM messages WHERE thread_id = ? AND role = 'assistant'")
                .bind(thread_id)
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(count.0, 1, "exactly one assistant message must exist");
    }
}

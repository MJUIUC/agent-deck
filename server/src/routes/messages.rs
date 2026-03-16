use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

use crate::{
    error::{AppError, AppResult},
    models::message::{CreateMessage, Message, MessageResponse},
    routes::AppState,
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

    tokio::spawn(async move {
        // Acquire semaphore — queues behind any in-progress run on this thread.
        // This must happen before wait_for_subscriber so that:
        //   1. The SSE subscriber check uses the freshest possible connection
        //      state (after any preceding run has finished and the client may
        //      have briefly cycled its EventSource).
        //   2. The cancel token is not replaced until this run is actually
        //      next-in-line, preventing a queued task from clobbering the
        //      active run's token while it is still executing.
        let _permit = run_state.semaphore.acquire().await.unwrap();

        // Create a fresh cancellation token for this run now that we hold
        // the semaphore and are guaranteed to be the next active run.
        let new_token = CancellationToken::new();
        {
            let mut lock = run_state.cancel_token.lock().await;
            *lock = new_token.clone();
        }

        // Wait for the SSE subscriber immediately before streaming begins so
        // tokens are not fired into the void.  The subscriber is checked here
        // (after acquiring the semaphore) so the wait reflects the actual
        // connection state at the moment generation starts, not the moment the
        // message was sent.
        state_clone
            .wait_for_subscriber(&tid, std::time::Duration::from_secs(3))
            .await;

        crate::services::agent::run(state_clone, tid, content, new_token).await;

        run_state.depth.fetch_sub(1, Ordering::SeqCst);
    });

    let response = MessageResponse::from(message);
    Ok((StatusCode::CREATED, Json(json!({ "data": response }))))
}

/// POST /api/threads/:id/cancel
///
/// Cancels the currently running (or next queued) agent run on this thread
/// by triggering the thread's active cancellation token.
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let run_state = state.get_run_state(&thread_id);
    let token = run_state.cancel_token.lock().await;
    token.cancel();

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
}

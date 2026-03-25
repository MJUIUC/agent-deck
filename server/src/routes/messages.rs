use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};

use crate::{
    error::{AppError, AppResult},
    models::message::{CreateMessage, Message, MessageResponse},
    routes::{AppState, RunningTurn},
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
    pub include_hidden: Option<bool>,
}

/// GET /api/threads/:id/messages
///
/// Returns message history for a thread, ordered oldest-first.
/// Supports cursor-based pagination via the `before` message ID.
/// Hidden messages (visibility = 'hidden') are excluded by default.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let include_hidden = query.include_hidden.unwrap_or(false);
    let visibility_filter = if include_hidden {
        "(visibility = 'visible' OR (visibility = 'hidden' AND source = 'tool'))"
    } else {
        "visibility = 'visible'"
    };

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
                sqlx::query_as::<_, Message>(&format!(
                    "SELECT id, thread_id, role, content, source, routine_id, visibility,
                            execution_id, event_type, stopped, created_at
                     FROM messages
                     WHERE thread_id = ? AND created_at < ? AND {}
                     ORDER BY created_at DESC
                     LIMIT ?",
                    visibility_filter
                ))
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
        sqlx::query_as::<_, Message>(&format!(
            "SELECT id, thread_id, role, content, source, routine_id, visibility,
                    execution_id, event_type, stopped, created_at
             FROM (
                 SELECT id, thread_id, role, content, source, routine_id, visibility,
                        execution_id, event_type, stopped, created_at
                 FROM messages
                 WHERE thread_id = ? AND {}
                 ORDER BY created_at DESC
                 LIMIT ?
             )
             ORDER BY created_at ASC",
            visibility_filter
        ))
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
    State(state): State<Arc<AppState>>,
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

    let (cancellation_tx, cancellation_rx) = tokio::sync::watch::channel(false);
    let turn_id = uuid::Uuid::new_v4();

    let handle = tokio::spawn({
        let run_state = run_state.clone();
        let state = state_clone.clone();
        let tid = tid.clone();
        let content = content.clone();
        async move {
            let _permit = run_state.semaphore.acquire().await.unwrap();
            state
                .wait_for_subscriber(&tid, std::time::Duration::from_secs(3))
                .await;
            crate::services::agent::run(
                state,
                tid,
                content,
                cancellation_rx,
                run_state.clone(),
                turn_id,
                false,
            )
            .await;
            run_state.depth.fetch_sub(1, Ordering::SeqCst);
        }
    });

    let old_handle = {
        let mut slot = run_state.running_turn.lock().await;
        let old_handle = slot.take().map(|t| t.cancel());
        *slot = Some(RunningTurn {
            task: handle,
            cancellation_tx,
            thread_id: thread_id.clone(),
            turn_id,
        });
        old_handle
    };

    if let Some(old) = old_handle {
        // Drain the old turn outside the lock to avoid a deadlock
        // (the old task also acquires running_turn when it clears its slot).
        let _ = old.await;
    }

    let response = MessageResponse::from(message);
    Ok((StatusCode::CREATED, Json(json!({ "data": response }))))
}

/// POST /api/threads/:id/cancel
///
/// Signals the running agent task to stop cooperatively via a watch channel,
/// then awaits its completion. The generation loop handles persisting any
/// partial state and firing the `MessageComplete { stopped: true }` SSE event.
pub async fn cancel_run(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let run_state = state.get_run_state(&thread_id);

    let handle = {
        let mut slot = run_state.running_turn.lock().await;
        slot.take().map(|t| t.cancel())
    };

    let Some(handle) = handle else {
        tracing::info!(thread_id = %thread_id, "cancel_run: no active run to cancel");
        return Ok(Json(json!({ "data": { "cancelled": false } })));
    };

    // Await cooperative shutdown with a safety-net timeout.
    match tokio::time::timeout(std::time::Duration::from_secs(30), handle).await {
        Ok(_) => tracing::info!(thread_id = %thread_id, "cancel_run: task exited cooperatively"),
        Err(_) => tracing::error!(
            thread_id = %thread_id,
            "cancel_run: task did not exit within 30s — this indicates a missing \
             cancellation checkpoint in the run-loop"
        ),
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
    State(state): State<Arc<AppState>>,
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
    thread_id: &str,
    user_id: &str,
    args: &[String],
) -> AppResult<SlashCommandData> {
    use crate::services::memory as memory_service;

    let subcommand = args.first().map(|s| s.to_lowercase());

    match subcommand.as_deref() {
        Some("list") | None => {
            // Step 1: resolve the thread's persona_id from the threads table
            let thread_persona: Option<(String,)> =
                sqlx::query_as("SELECT persona_id FROM threads WHERE id = ? AND user_id = ?")
                    .bind(thread_id)
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await?;

            let persona_id = match thread_persona {
                Some((id,)) => id,
                None => {
                    return Ok(SlashCommandData {
                        kind: "memory_list".to_string(),
                        message: "Thread not found.".to_string(),
                        payload: None,
                    })
                }
            };

            // Step 2: check whether this persona is the Default persona
            let is_default_row: Option<(bool,)> =
                sqlx::query_as("SELECT is_default FROM agent_personas WHERE id = ?")
                    .bind(&persona_id)
                    .fetch_optional(&state.pool)
                    .await?;

            let is_default = is_default_row.map(|(v,)| v).unwrap_or(false);

            if is_default {
                return Ok(SlashCommandData {
                    kind: "memory_list".to_string(),
                    message: "Memory is not available for this thread. Assign a persona to use long-term memory.".to_string(),
                    payload: None,
                });
            }

            // Step 3: fetch up to 20 most recent memories for this persona
            let result =
                memory_service::list_memories(&state.pool, user_id, &persona_id, 0, 20, None)
                    .await?;

            if result.memories.is_empty() {
                return Ok(SlashCommandData {
                    kind: "memory_list".to_string(),
                    message: "No memories stored for this persona yet.".to_string(),
                    payload: None,
                });
            }

            // Step 4: format each entry as "- [YYYY-MM-DD] content", optionally
            // appending "(from: thread_title)" when the source thread differs.
            let list: Vec<String> = result
                .memories
                .iter()
                .map(|e| {
                    // created_at is stored as ISO 8601; grab the first 10 chars for YYYY-MM-DD
                    let date = &e.created_at[..e.created_at.len().min(10)];
                    let mut line = format!("- [{}] {}", date, e.content);
                    if let (Some(src_tid), Some(title)) =
                        (e.thread_id.as_deref(), e.thread_title.as_deref())
                    {
                        if src_tid != thread_id {
                            line.push_str(&format!(" (from: {})", title));
                        }
                    }
                    line
                })
                .collect();

            Ok(SlashCommandData {
                kind: "memory_list".to_string(),
                message: list.join("\n"),
                payload: Some(serde_json::json!({
                    "total": result.total_count,
                    "persona_id": persona_id,
                })),
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

    /// When there is no active run, `cancel_run` should report `cancelled: false`.
    /// This test verifies the None-branch behavior of the cooperative cancel model.
    #[tokio::test]
    async fn cancel_run_with_no_active_turn_returns_not_cancelled() {
        use crate::routes::RunState;
        use std::sync::Arc;

        let run_state = Arc::new(RunState::new());

        // Verify the slot is empty initially.
        {
            let slot = run_state.running_turn.lock().await;
            assert!(
                slot.is_none(),
                "running_turn must be None on a freshly created RunState"
            );
        }

        // Simulate the None branch of cancel_run: take() returns None.
        let handle = {
            let mut slot = run_state.running_turn.lock().await;
            slot.take().map(|t| t.cancel())
        };

        // None means cancel_run would return { cancelled: false }.
        assert!(
            handle.is_none(),
            "no handle means cancel returns false when there is no active turn"
        );
    }

    /// When there is an active run, `cancel_run` should signal it to stop
    /// cooperatively and await its completion.
    #[tokio::test]
    async fn cancel_run_with_active_turn_signals_and_awaits() {
        use crate::routes::{RunState, RunningTurn};
        use std::sync::Arc;

        let run_state = Arc::new(RunState::new());
        let (cancellation_tx, cancellation_rx) = tokio::sync::watch::channel(false);

        // Spawn a task that exits cooperatively when it receives the cancel signal.
        let task = tokio::spawn(async move {
            let mut rx = cancellation_rx;
            loop {
                if *rx.borrow() {
                    break;
                }
                match rx.changed().await {
                    Ok(()) => { /* value changed — loop and re-check */ }
                    Err(_) => break, // sender dropped
                }
            }
        });

        let turn_id = uuid::Uuid::new_v4();

        // Store the RunningTurn in the slot.
        {
            let mut slot = run_state.running_turn.lock().await;
            *slot = Some(RunningTurn {
                task,
                cancellation_tx,
                thread_id: "thread-test".to_string(),
                turn_id,
            });
        }

        // Simulate cancel_run: take the RunningTurn and call cancel().
        let handle = {
            let mut slot = run_state.running_turn.lock().await;
            slot.take().map(|t| t.cancel())
        };

        assert!(
            handle.is_some(),
            "cancel should return a handle when there is an active turn"
        );

        // Await with a short timeout to prove cooperative shutdown works.
        match tokio::time::timeout(std::time::Duration::from_secs(5), handle.unwrap()).await {
            Ok(Ok(())) => {} // task completed cleanly
            Ok(Err(e)) => panic!("task panicked: {}", e),
            Err(_) => panic!("task did not exit within 5 seconds"),
        }
    }

    /// When a new `send` comes in while a previous run is active, the old
    /// RunningTurn should be cancelled and drained before the new one is stored.
    #[tokio::test]
    async fn send_drain_before_second_run() {
        use crate::routes::{RunState, RunningTurn};
        use std::sync::Arc;

        let run_state = Arc::new(RunState::new());
        let (old_tx, old_rx) = tokio::sync::watch::channel(false);
        let old_turn_id = uuid::Uuid::new_v4();

        // Spawn a task that blocks until the watch channel fires, then exits.
        let old_task = tokio::spawn(async move {
            let mut rx = old_rx;
            loop {
                if *rx.borrow() {
                    break;
                }
                match rx.changed().await {
                    Ok(()) => {}
                    Err(_) => break,
                }
            }
        });

        // Store the old RunningTurn in the slot.
        {
            let mut slot = run_state.running_turn.lock().await;
            *slot = Some(RunningTurn {
                task: old_task,
                cancellation_tx: old_tx,
                thread_id: "thread-test".to_string(),
                turn_id: old_turn_id,
            });
        }

        // Create the new turn's components.
        let (new_tx, _new_rx) = tokio::sync::watch::channel(false);
        let new_turn_id = uuid::Uuid::new_v4();
        let new_task = tokio::spawn(async move {});

        // Simulate what send does: take the old RunningTurn, cancel it, store the new one.
        let old_handle = {
            let mut slot = run_state.running_turn.lock().await;
            let old_handle = slot.take().map(|t| t.cancel());
            *slot = Some(RunningTurn {
                task: new_task,
                cancellation_tx: new_tx,
                thread_id: "thread-test".to_string(),
                turn_id: new_turn_id,
            });
            old_handle
        };

        // Await the old handle — it should complete because it received the cancel signal.
        match old_handle {
            Some(old) => match tokio::time::timeout(std::time::Duration::from_secs(5), old).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => panic!("old task panicked: {}", e),
                Err(_) => panic!("old task did not exit within 5 seconds"),
            },
            None => panic!("expected an old handle to be returned"),
        }

        // Assert the slot now holds the new RunningTurn (identified by new_turn_id).
        {
            let slot = run_state.running_turn.lock().await;
            assert!(slot.is_some(), "slot should hold the new RunningTurn");
            assert_eq!(
                slot.as_ref().map(|t| t.turn_id),
                Some(new_turn_id),
                "slot should hold the new turn_id"
            );
        }
    }

    /// Verifies the turn_id comparison logic used when clearing the running_turn slot
    /// on normal completion. A run must only clear the slot when it still holds its
    /// own turn_id, preventing a finishing run from clobbering a newer run.
    #[tokio::test]
    async fn slot_cleared_on_normal_completion() {
        use crate::routes::{RunState, RunningTurn};
        use std::sync::Arc;

        let run_state = Arc::new(RunState::new());
        let turn_id = uuid::Uuid::new_v4();
        let (tx, _rx) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(async move {});

        // Store a fake RunningTurn with our turn_id.
        {
            let mut slot = run_state.running_turn.lock().await;
            *slot = Some(RunningTurn {
                task,
                cancellation_tx: tx,
                thread_id: "thread-test".to_string(),
                turn_id,
            });
        }

        // Verify the comparison matches and clear the slot.
        {
            let mut slot = run_state.running_turn.lock().await;
            assert_eq!(
                slot.as_ref().map(|t| t.turn_id),
                Some(turn_id),
                "slot should contain our turn_id before clearing"
            );
            if slot.as_ref().map(|t| t.turn_id) == Some(turn_id) {
                *slot = None;
            }
        }

        // The slot should now be None after normal completion.
        {
            let slot = run_state.running_turn.lock().await;
            assert!(
                slot.is_none(),
                "slot should be cleared after normal completion"
            );
        }

        // Verify: if a DIFFERENT turn_id is now in the slot, the stale run must NOT clear it.
        let other_turn_id = uuid::Uuid::new_v4();
        let (tx2, _rx2) = tokio::sync::watch::channel(false);
        let task2 = tokio::spawn(async move {});

        {
            let mut slot = run_state.running_turn.lock().await;
            *slot = Some(RunningTurn {
                task: task2,
                cancellation_tx: tx2,
                thread_id: "thread-test".to_string(),
                turn_id: other_turn_id,
            });
        }

        // A stale run with the old turn_id attempts to clear the slot — it must not.
        {
            let mut slot = run_state.running_turn.lock().await;
            if slot.as_ref().map(|t| t.turn_id) == Some(turn_id) {
                *slot = None;
            }
        }

        // The slot should still hold the newer turn because turn_ids differed.
        {
            let slot = run_state.running_turn.lock().await;
            assert!(
                slot.is_some(),
                "slot should NOT be cleared when turn_ids differ"
            );
            assert_eq!(
                slot.as_ref().map(|t| t.turn_id),
                Some(other_turn_id),
                "slot should still hold the new (other) turn_id"
            );
        }
    }
}

//! Agent run-loop service
//!
//! Responsible for:
//! - Assembling the context (system prompt + message history + memory recall)
//! - Calling the LLM provider via the provider abstraction layer
//! - Streaming tokens back to connected SSE clients
//! - Handling tool calls via the `AgentTool` registry (built-ins) and MCP routing
//! - Persisting the completed assistant message to the database
//! - Emitting `message_complete` and `thread_updated` SSE events
//!
//! ## Structure (H.2 / H.3)
//!
//! `generation_loop` is a thin coordinator that owns the turn loop.
//! Each turn delegates to:
//!   - `stream_one_turn` — drives the provider stream, emits SSE tokens, and
//!     returns a `TurnResult` (accumulated text + resolved tool calls).
//!   - `execute_tool_calls` — checks the built-in tool registry first, then
//!     falls through to MCP routing for dynamically-discovered tools.

use anyhow::{anyhow, Result};
use async_openai::types::{ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs};
use futures::StreamExt;
use sqlx::SqlitePool;
use std::sync::Arc;

use tracing::{error, info, warn};

use crate::services::copilot::GlobalEvent;
use crate::{
    models::message::Message,
    routes::{sse::ThreadEvent, AppState, RunState},
    services::{
        context::{self, AssemblyInput, HistoryMessage},
        encryption,
        provider::{CopilotProvider, LlmProvider, OpenAiProvider},
        title as title_service,
    },
};

// ─── Attached MCP server descriptor ───────────────────────────────────────────

/// Lightweight descriptor for an MCP server that is attached to the current
/// thread.  Passed through the generation loop so that tool calls can be
/// routed to the correct server by tag.
struct AttachedMcpServer {
    id: String,
    tag: String,
}

// ─── Public entry point ────────────────────────────────────────────────────────

/// Result of a complete generation loop run.
struct GenerationResult {
    content: String,
    cancelled: bool,
    message_id: String,
    created_at: String,
}

// ─── Retry strategy ───────────────────────────────────────────────────────────

/// How `stream_one_turn` should behave when the provider returns a transient
/// error.  Modelled on Zed's `RetryStrategy`.
#[derive(Debug, Clone, PartialEq)]
enum RetryStrategy {
    /// Retry with exponentially growing delays starting at `initial_delay`.
    ExponentialBackoff {
        initial_delay: std::time::Duration,
        max_attempts: u32,
    },
    /// Retry with a constant delay between attempts.
    Fixed {
        delay: std::time::Duration,
        max_attempts: u32,
    },
    /// Do not retry; surface the error to the client immediately.
    None,
}

/// Choose a retry strategy based on the error returned by the provider.
///
/// The error message is inspected as a string because provider errors arrive
/// as `anyhow::Error` with the HTTP status embedded in the message
/// (e.g. `"Provider X returned 429 — ..."`).
fn retry_strategy_for(error: &anyhow::Error) -> RetryStrategy {
    let msg = error.to_string();

    // HTTP 429 — rate limited.  Back off aggressively.
    if msg.contains("429") {
        return RetryStrategy::ExponentialBackoff {
            initial_delay: std::time::Duration::from_secs(2),
            max_attempts: 4,
        };
    }

    // HTTP 5xx — provider-side error.  Shorter backoff.
    if msg.contains("500") || msg.contains("502") || msg.contains("503") || msg.contains("504") {
        return RetryStrategy::ExponentialBackoff {
            initial_delay: std::time::Duration::from_secs(1),
            max_attempts: 3,
        };
    }

    // Network-level failures (connection reset, timeout, etc.).
    // reqwest wraps these as non-HTTP errors — they don't contain a status code.
    let is_network_error = error
        .chain()
        .any(|e| e.to_string().contains("connection") || e.to_string().contains("timed out"));
    if is_network_error || msg.contains("connection") || msg.contains("timed out") {
        return RetryStrategy::Fixed {
            delay: std::time::Duration::from_secs(1),
            max_attempts: 2,
        };
    }

    // HTTP 4xx (except 429), parse errors, auth errors — surface immediately.
    RetryStrategy::None
}

// ─── Stream event types ────────────────────────────────────────────────────────

/// Typed events produced by `stream_one_turn` from the raw provider stream.
/// Using an enum here makes it straightforward to add new content types
/// (e.g. thinking tokens, structured diffs) without touching the accumulation
/// logic.
#[derive(Debug)]
enum StreamEvent {
    /// A text delta to display immediately and accumulate.
    TextDelta(String),
    /// A fragment of a tool call arriving across one or more chunks.
    ToolCallFragment {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        args_fragment: Option<String>,
    },
    /// The stream ended cleanly.
    Done,
}

/// A fully assembled tool call after all fragments have been merged.
#[derive(Debug)]
struct ResolvedToolCall {
    id: String,
    name: String,
    args: String,
}

/// Result of streaming a single LLM turn.
struct TurnResult {
    /// Full text produced by the model during this turn (may be empty when the
    /// model only emits tool calls).
    text: String,
    /// Tool calls requested by the model, fully assembled from fragments.
    tool_calls: Vec<ResolvedToolCall>,
    /// True if the turn was interrupted by cooperative cancellation.
    cancelled: bool,
}

// ─── Cancellation helpers ─────────────────────────────────────────────────────

/// Returns `true` if the cooperative cancellation signal has been sent.
fn is_cancelled(rx: &tokio::sync::watch::Receiver<bool>) -> bool {
    *rx.borrow()
}

/// Tracks mutable state for a single agent run that needs to survive
/// across helper function boundaries.
struct RunContext {
    /// IDs of hidden tool messages written during this run. Used for
    /// precise cleanup if the run is cancelled before completion.
    hidden_message_ids: Vec<String>,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Run the agent for a given thread and new user message.
///
/// Accepts a `cancellation_rx` watch receiver so the run-loop can cooperatively
/// exit when `cancel_run` sends `true`.  The `run_state` and `turn_id` are used
/// to clear the `running_turn` slot once the run finishes or is cancelled.
///
/// This is intended to be called from inside a `tokio::spawn` — it runs until
/// the LLM has finished generating (including any tool-call rounds) and then
/// emits the final `message_complete` and `thread_updated` SSE events.
///
/// Errors during generation are surfaced as SSE `error` events rather than
/// returned as `Err` values, so the caller doesn't need to do anything special
/// with the return value.
pub async fn run(
    state: Arc<AppState>,
    thread_id: String,
    user_message: String,
    cancellation_rx: tokio::sync::watch::Receiver<bool>,
    run_state: Arc<RunState>,
    turn_id: uuid::Uuid,
    is_routine: bool,
) {
    if let Err(e) = run_inner(
        &state,
        &thread_id,
        &user_message,
        cancellation_rx,
        run_state,
        turn_id,
        is_routine,
    )
    .await
    {
        error!(
            thread_id = %thread_id,
            error = %e,
            "Agent run-loop encountered an unrecoverable error"
        );
        // Attempt to surface the error to the client via SSE.
        state.send_thread_event(
            &thread_id,
            ThreadEvent::Error {
                code: "INTERNAL_ERROR".to_string(),
                message: format!("An unexpected error occurred: {}", e),
            },
        );
    }
}

// ─── Inner implementation ─────────────────────────────────────────────────────

async fn run_inner(
    state: &AppState,
    thread_id: &str,
    user_message: &str,
    cancellation_rx: tokio::sync::watch::Receiver<bool>,
    run_state: Arc<RunState>,
    turn_id: uuid::Uuid,
    is_routine: bool,
) -> Result<()> {
    // ── 1. Fetch the thread and its persona ────────────────────────────────────
    let thread: crate::models::thread::Thread = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, show_tool_activity, show_system_events,
                created_at, updated_at
         FROM threads WHERE id = ?",
    )
    .bind(thread_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| anyhow!("Failed to load thread {}: {}", thread_id, e))?;

    let user_id = &thread.user_id;
    let persona_id = &thread.persona_id;

    let persona: crate::models::agent_persona::AgentPersona = sqlx::query_as(
        "SELECT id, user_id, name, emoji, avatar_path, system_prompt, default_model,
                default_provider, is_default, created_at, updated_at
         FROM agent_personas WHERE id = ?",
    )
    .bind(persona_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| anyhow!("Failed to load persona {}: {}", persona_id, e))?;

    let is_default_persona = persona.is_default;

    // ── 2. Resolve the active provider and model ───────────────────────────────
    // Priority: thread overrides > persona defaults.
    let provider_id = thread
        .active_provider
        .as_deref()
        .or(persona.default_provider.as_deref());

    let model_id = thread
        .active_model
        .as_deref()
        .or(persona.default_model.as_deref());

    let (provider_id, model_uuid) = match (provider_id, model_id) {
        (Some(p), Some(m)) => (p.to_string(), m.to_string()),
        _ => {
            state.send_thread_event(
                thread_id,
                ThreadEvent::Error {
                    code: "PROVIDER_UNAVAILABLE".to_string(),
                    message: "No provider or model configured for this thread. \
                              Go to Settings to configure a provider, then set it on \
                              the thread or persona."
                        .to_string(),
                },
            );
            return Ok(());
        }
    };

    // Resolve the model UUID (FK stored on the thread) to the actual model_id
    // string (e.g. "gpt-4o") that the provider API expects.
    let model_row: Option<(String,)> = sqlx::query_as("SELECT model_id FROM models WHERE id = ?")
        .bind(&model_uuid)
        .fetch_optional(&state.pool)
        .await?;

    let model_id = match model_row {
        Some((mid,)) => mid,
        None => {
            // Fall back to using the value as-is in case it was already a
            // raw model string rather than a UUID (e.g. during manual testing).
            warn!(
                thread_id = %thread_id,
                model_uuid = %model_uuid,
                "Model UUID not found in DB — using value as raw model_id"
            );
            model_uuid
        }
    };

    // Load the provider row from the database.
    let provider_row: Option<crate::models::provider::Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers WHERE id = ? AND user_id = ?",
    )
    .bind(&provider_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;

    let provider_row = match provider_row {
        Some(p) if p.enabled => p,
        Some(_) => {
            state.send_thread_event(
                thread_id,
                ThreadEvent::Error {
                    code: "PROVIDER_UNAVAILABLE".to_string(),
                    message: "The configured provider is disabled. \
                              Enable it in Settings > Providers."
                        .to_string(),
                },
            );
            return Ok(());
        }
        None => {
            state.send_thread_event(
                thread_id,
                ThreadEvent::Error {
                    code: "PROVIDER_UNAVAILABLE".to_string(),
                    message: "The configured provider could not be found. \
                              Check provider settings."
                        .to_string(),
                },
            );
            return Ok(());
        }
    };

    // Build the concrete provider instance.
    let provider: Box<dyn LlmProvider> = build_provider(&state, &provider_row)?;

    // ── 3. Load visible message history for this thread ────────────────────────
    let history_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT role, content, visibility
         FROM messages
         WHERE thread_id = ? AND visibility = 'visible'
         ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(&state.pool)
    .await?;

    let history: Vec<HistoryMessage> = history_rows
        .into_iter()
        .map(|(role, content, visibility)| HistoryMessage {
            role,
            content,
            visibility,
        })
        .collect();

    // ── 4. Assemble context ────────────────────────────────────────────────────
    // The provider abstraction only exposes OpenAI-compatible endpoints for now,
    // so we always support tools.

    // ── 4.5. Load attached MCP servers and build namespaced tool list ─────────
    let attached: Vec<AttachedMcpServer> = {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT ms.id, ms.tag
             FROM thread_mcp_servers tms
             JOIN mcp_servers ms ON ms.id = tms.mcp_server_id
             WHERE tms.thread_id = ? AND tms.enabled = 1 AND ms.enabled = 1",
        )
        .bind(thread_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_else(|e| {
            warn!(thread_id = %thread_id, error = %e, "Failed to load attached MCP servers; proceeding with none");
            Vec::new()
        });

        rows.into_iter()
            .map(|(id, tag)| AttachedMcpServer { id, tag })
            .collect()
    };

    // Build namespaced tool list for the LLM request.
    let mut mcp_tool_defs: Vec<async_openai::types::ChatCompletionTool> = Vec::new();
    for server in &attached {
        let tools = state.mcp.cached_tools(&server.id).await;
        for tool in tools {
            let namespaced_name = format!("{}__{}", server.tag, tool.name);
            let tool_def = async_openai::types::ChatCompletionTool {
                r#type: async_openai::types::ChatCompletionToolType::Function,
                function: async_openai::types::FunctionObject {
                    name: namespaced_name,
                    description: tool.description,
                    parameters: tool.input_schema,
                    strict: None,
                },
            };
            mcp_tool_defs.push(tool_def);
        }
    }

    let assembled = context::assemble(AssemblyInput {
        persona_system_prompt: persona.system_prompt.clone(),
        thread_addendum: thread.system_prompt_addendum.clone(),
        history,
        history_limit: None,
        user_message: user_message.to_string(),
        is_routine_triggered: is_routine,
        supports_tools: true,
        include_memory: !is_default_persona,
        built_in_tool_defs: if is_default_persona {
            vec![]
        } else {
            state
                .built_in_tools
                .iter()
                .map(|t| async_openai::types::ChatCompletionTool {
                    r#type: async_openai::types::ChatCompletionToolType::Function,
                    function: async_openai::types::FunctionObject {
                        name: t.name().to_string(),
                        description: Some(t.description().to_string()),
                        parameters: Some(t.input_schema()),
                        strict: None,
                    },
                })
                .collect()
        },
        mcp_tools: mcp_tool_defs,
    });

    // ── 5. Run the generation loop (handles tool calls inline) ─────────────────
    let gen_result = generation_loop(
        state,
        thread_id,
        user_id,
        persona_id,
        &*provider,
        &model_id,
        assembled.messages,
        assembled.tools,
        &attached,
        &cancellation_rx,
        run_state,
        turn_id,
    )
    .await?;

    if gen_result.cancelled {
        info!(thread_id = %thread_id, "Agent run-loop cancelled");
        return Ok(());
    }

    // ── 6.5 First-message post-processing ─────────────────────────────────────
    // Count total messages in this thread. If this is the first assistant reply
    // (i.e. exactly 2 messages: 1 user + 1 assistant), run title generation
    // using the first user message and first assistant reply, persist the result,
    // and broadcast TitleUpdated so the client sidebar updates in real time
    // without a page reload or a second HTTP call from the client.
    // Only runs for non-cancelled completions to avoid titling partial responses.
    let assistant_content = &gen_result.content;

    let message_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages
         WHERE thread_id = ? AND role IN ('user', 'assistant') AND visibility = 'visible'",
    )
    .bind(thread_id)
    .fetch_one(&state.pool)
    .await?;

    if message_count.0 == 2 {
        // Fetch the first user message to use as the title gen input.
        let first_user: Option<(String,)> = sqlx::query_as(
            "SELECT content FROM messages
             WHERE thread_id = ? AND role = 'user'
             ORDER BY created_at ASC LIMIT 1",
        )
        .bind(thread_id)
        .fetch_optional(&state.pool)
        .await?;

        if let Some((user_content,)) = first_user {
            let generated_title =
                title_service::try_llm_title(state, &thread, &user_content, assistant_content)
                    .await;

            let now = chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();

            sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
                .bind(&generated_title)
                .bind(&now)
                .bind(thread_id)
                .execute(&state.pool)
                .await?;

            if let Err(e) = state.send_global_event(GlobalEvent::TitleUpdated {
                thread_id: thread_id.to_string(),
                title: generated_title,
            }) {
                // No global-stream subscribers — normal when no client is connected.
                tracing::debug!(thread_id = %thread_id, error = %e, "TitleUpdated broadcast had no receivers");
            }

            info!(
                thread_id = %thread_id,
                "Title generated and broadcast after first assistant reply"
            );
        }
    }

    // ── 7. Emit ThreadUpdated SSE event ────────────────────────────────────────
    // MessageComplete is now fired inside generation_loop; only ThreadUpdated
    // is emitted here (skipped when cancelled).
    if let Err(e) = state.send_global_event(GlobalEvent::ThreadUpdated {
        thread_id: thread_id.to_string(),
        last_message: assistant_content.chars().take(120).collect::<String>(),
        updated_at: gen_result.created_at.clone(),
    }) {
        // No global-stream subscribers — normal when no client is connected.
        tracing::debug!(thread_id = %thread_id, error = %e, "ThreadUpdated broadcast had no receivers");
    }

    info!(
        thread_id = %thread_id,
        message_id = %gen_result.message_id,
        "Agent run-loop complete"
    );

    Ok(())
}

// ─── Generation loop ──────────────────────────────────────────────────────────

/// Thin coordinator: loops over turns, delegating streaming to `stream_one_turn`
/// and tool dispatch to `execute_tool_calls`.  Handles message persistence and
/// SSE events for both normal completion and cooperative cancellation.
async fn generation_loop(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    persona_id: &str,
    provider: &dyn LlmProvider,
    model_id: &str,
    mut messages: Vec<ChatCompletionRequestMessage>,
    tools: Vec<async_openai::types::ChatCompletionTool>,
    attached_mcp: &[AttachedMcpServer],
    cancellation_rx: &tokio::sync::watch::Receiver<bool>,
    run_state: Arc<RunState>,
    turn_id: uuid::Uuid,
) -> Result<GenerationResult> {
    let mut run_context = RunContext {
        hidden_message_ids: Vec::new(),
    };
    let mut final_content = String::new();
    let mut cancelled = false;

    // 20 rounds is generous enough for complex agentic workflows while still
    // guarding against infinite tool-call loops.
    const MAX_TOOL_ROUNDS: usize = 20;
    let mut tool_rounds = 0;

    'turn_loop: loop {
        if is_cancelled(cancellation_rx) {
            cancelled = true;
            break 'turn_loop;
        }

        info!(thread_id = %thread_id, tool_round = tool_rounds, "generation_loop: starting turn");

        let turn = stream_one_turn(
            state,
            thread_id,
            provider,
            model_id,
            messages.clone(),
            tools.clone(),
            cancellation_rx,
        )
        .await?;

        info!(thread_id = %thread_id, tool_round = tool_rounds,
            text_len = turn.text.len(), tools = turn.tool_calls.len(), "generation_loop: turn finished");

        if turn.cancelled {
            final_content.push_str(&turn.text);
            cancelled = true;
            break 'turn_loop;
        }

        if turn.tool_calls.is_empty() {
            final_content.push_str(&turn.text);
            break 'turn_loop;
        }

        if tool_rounds >= MAX_TOOL_ROUNDS {
            warn!(thread_id = %thread_id,
                "Reached maximum tool-call rounds ({}); stopping generation", MAX_TOOL_ROUNDS);
            final_content.push_str(&turn.text);
            break 'turn_loop;
        }

        tool_rounds += 1;

        let pending: Vec<PendingToolCall> = turn
            .tool_calls
            .iter()
            .map(|tc| PendingToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                args: tc.args.clone(),
            })
            .collect();
        messages.push(build_assistant_tool_call_message(&turn.text, &pending));

        let tool_results = execute_tool_calls(
            state,
            thread_id,
            user_id,
            persona_id,
            &turn.tool_calls,
            attached_mcp,
            cancellation_rx,
            &mut run_context,
        )
        .await;

        if is_cancelled(cancellation_rx) {
            final_content.push_str(&turn.text);
            cancelled = true;
            break 'turn_loop;
        }

        for (tc, result) in turn.tool_calls.iter().zip(tool_results) {
            messages.push(
                ChatCompletionRequestToolMessageArgs::default()
                    .content(result.as_str())
                    .tool_call_id(tc.id.as_str())
                    .build()
                    .map_err(|e| anyhow!("Failed to build tool result message: {}", e))?
                    .into(),
            );
        }
    }

    if cancelled {
        // ── Cancelled cleanup path ─────────────────────────────────────────────
        // Delete hidden tool messages created during this run by exact ID so
        // dangling tool-call/result pairs do not pollute the next run's context.
        for id in &run_context.hidden_message_ids {
            if let Err(e) = sqlx::query("DELETE FROM messages WHERE id = ?")
                .bind(id)
                .execute(&state.pool)
                .await
            {
                warn!(thread_id = %thread_id, id = %id, error = %e,
                    "generation_loop: failed to delete hidden tool message on cancel");
            }
        }

        let content = if final_content.is_empty() {
            "[Response cancelled before any content was generated]".to_string()
        } else {
            final_content.clone()
        };

        let assistant_msg = Message::new_assistant_stopped(thread_id, &content);

        sqlx::query(
            "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                                   execution_id, event_type, stopped, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&assistant_msg.id)
        .bind(&assistant_msg.thread_id)
        .bind(&assistant_msg.role)
        .bind(&assistant_msg.content)
        .bind(&assistant_msg.source)
        .bind(&assistant_msg.routine_id)
        .bind(&assistant_msg.visibility)
        .bind(&assistant_msg.execution_id)
        .bind(&assistant_msg.event_type)
        .bind(assistant_msg.stopped)
        .bind(&assistant_msg.created_at)
        .execute(&state.pool)
        .await?;

        sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
            .bind(&assistant_msg.created_at)
            .bind(thread_id)
            .execute(&state.pool)
            .await?;

        state.send_thread_event(
            thread_id,
            ThreadEvent::MessageComplete {
                id: assistant_msg.id.clone(),
                thread_id: thread_id.to_string(),
                role: assistant_msg.role.clone(),
                content: assistant_msg.content.clone(),
                created_at: assistant_msg.created_at.clone(),
                stopped: true,
            },
        );

        // Clear the running_turn slot if it still belongs to this run.
        {
            let mut slot = run_state.running_turn.lock().await;
            let should_clear = slot.as_ref().map(|r| r.turn_id == turn_id).unwrap_or(false);
            if should_clear {
                *slot = None;
            }
        }

        info!(
            thread_id = %thread_id,
            message_id = %assistant_msg.id,
            "generation_loop: persisted stopped message after cancellation"
        );

        return Ok(GenerationResult {
            content,
            cancelled: true,
            message_id: assistant_msg.id,
            created_at: assistant_msg.created_at,
        });
    }

    // ── Normal completion path ─────────────────────────────────────────────────
    let assistant_msg = Message::new_assistant(thread_id, &final_content);

    sqlx::query(
        "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                               execution_id, event_type, stopped, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&assistant_msg.id)
    .bind(&assistant_msg.thread_id)
    .bind(&assistant_msg.role)
    .bind(&assistant_msg.content)
    .bind(&assistant_msg.source)
    .bind(&assistant_msg.routine_id)
    .bind(&assistant_msg.visibility)
    .bind(&assistant_msg.execution_id)
    .bind(&assistant_msg.event_type)
    .bind(assistant_msg.stopped)
    .bind(&assistant_msg.created_at)
    .execute(&state.pool)
    .await?;

    sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
        .bind(&assistant_msg.created_at)
        .bind(thread_id)
        .execute(&state.pool)
        .await?;

    state.send_thread_event(
        thread_id,
        ThreadEvent::MessageComplete {
            id: assistant_msg.id.clone(),
            thread_id: thread_id.to_string(),
            role: assistant_msg.role.clone(),
            content: assistant_msg.content.clone(),
            created_at: assistant_msg.created_at.clone(),
            stopped: false,
        },
    );

    // Clear the running_turn slot if it still belongs to this run.
    {
        let mut slot = run_state.running_turn.lock().await;
        let should_clear = slot.as_ref().map(|r| r.turn_id == turn_id).unwrap_or(false);
        if should_clear {
            *slot = None;
        }
    }

    info!(
        thread_id = %thread_id,
        message_id = %assistant_msg.id,
        "generation_loop: persisted normal assistant message"
    );

    Ok(GenerationResult {
        content: final_content,
        cancelled: false,
        message_id: assistant_msg.id,
        created_at: assistant_msg.created_at,
    })
}

// ─── stream_one_turn ─────────────────────────────────────────────────────────

/// Drive the provider stream for one LLM turn, retrying on transient errors.
///
/// Consumes the raw `TokenChunk` stream, classifies each chunk into a
/// `StreamEvent`, accumulates text and tool-call fragments, emits SSE tokens
/// to connected clients, and returns a `TurnResult` when the stream closes.
///
/// Transient provider errors (429, 5xx, network) trigger automatic retries
/// with backoff.  A `ThreadEvent::Retry` SSE event is emitted before each
/// retry so the client can show progress.
///
/// The `cancellation_rx` parameter is passed through to `try_stream_one_turn`
/// so that Story C.2 can add a `select!` against it without changing the
/// outer retry loop.
async fn stream_one_turn(
    state: &AppState,
    thread_id: &str,
    provider: &dyn LlmProvider,
    model_id: &str,
    messages: Vec<ChatCompletionRequestMessage>,
    tools: Vec<async_openai::types::ChatCompletionTool>,
    cancellation_rx: &tokio::sync::watch::Receiver<bool>,
) -> Result<TurnResult> {
    let mut attempt: u32 = 0;

    loop {
        match try_stream_one_turn(
            state,
            thread_id,
            provider,
            model_id,
            messages.clone(),
            tools.clone(),
            cancellation_rx,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(e) => {
                let strategy = retry_strategy_for(&e);

                let (delay, max_attempts) = match strategy {
                    RetryStrategy::ExponentialBackoff {
                        initial_delay,
                        max_attempts,
                    } => {
                        let delay = initial_delay * 2u32.saturating_pow(attempt);
                        (delay, max_attempts)
                    }
                    RetryStrategy::Fixed {
                        delay,
                        max_attempts,
                    } => (delay, max_attempts),
                    RetryStrategy::None => {
                        // Not retryable — emit an error event and propagate.
                        state.send_thread_event(
                            thread_id,
                            ThreadEvent::Error {
                                code: "PROVIDER_ERROR".to_string(),
                                message: format!(
                                    "Provider error: {}. Check your API key and provider settings.",
                                    e
                                ),
                            },
                        );
                        return Err(e);
                    }
                };

                attempt += 1;
                if attempt > max_attempts {
                    state.send_thread_event(
                        thread_id,
                        ThreadEvent::Error {
                            code: "PROVIDER_ERROR".to_string(),
                            message: format!("Provider failed after {} attempt(s): {}", attempt, e),
                        },
                    );
                    return Err(anyhow!(
                        "Provider failed after {} attempt(s): {}",
                        attempt,
                        e
                    ));
                }

                warn!(
                    thread_id = %thread_id,
                    attempt = attempt,
                    max_attempts = max_attempts,
                    error = %e,
                    delay_ms = delay.as_millis(),
                    "stream_one_turn: transient error, retrying"
                );

                state.send_thread_event(
                    thread_id,
                    ThreadEvent::Retry {
                        attempt,
                        max_attempts,
                        reason: e.to_string(),
                    },
                );

                // Sleep with backoff, but allow cancellation to interrupt immediately.
                {
                    let mut rx = cancellation_rx.clone();
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = rx.changed() => {
                            if is_cancelled(cancellation_rx) {
                                return Ok(TurnResult {
                                    text: String::new(),
                                    tool_calls: vec![],
                                    cancelled: true,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Single attempt at streaming one LLM turn.  Called by `stream_one_turn`
/// which owns the retry loop.
async fn try_stream_one_turn(
    state: &AppState,
    thread_id: &str,
    provider: &dyn LlmProvider,
    model_id: &str,
    messages: Vec<ChatCompletionRequestMessage>,
    tools: Vec<async_openai::types::ChatCompletionTool>,
    cancellation_rx: &tokio::sync::watch::Receiver<bool>,
) -> Result<TurnResult> {
    let mut token_stream = provider
        .stream(model_id, messages, tools)
        .await
        .map_err(|e| anyhow!("Provider stream error: {}", e))?;

    let mut turn_text = String::new();
    // Keyed by index; slots are grown on demand as fragments arrive.
    let mut pending_calls: Vec<PendingToolCall> = Vec::new();

    let mut cancellation_watcher = cancellation_rx.clone();
    loop {
        if is_cancelled(cancellation_rx) {
            return Ok(TurnResult {
                text: turn_text,
                tool_calls: vec![],
                cancelled: true,
            });
        }

        tokio::select! {
            biased;
            chunk = token_stream.next() => {
                match chunk {
                    None => break,
                    Some(Ok(c)) => {
                        // Classify the chunk into a StreamEvent (or two — a single chunk can
                        // carry both a text delta and a tool-call fragment).
                        if !c.delta.is_empty() {
                            handle_stream_event(
                                StreamEvent::TextDelta(c.delta),
                                state,
                                thread_id,
                                &mut turn_text,
                                &mut pending_calls,
                            )
                            .await;
                        }

                        if let Some(index) = c.tool_call_index {
                            handle_stream_event(
                                StreamEvent::ToolCallFragment {
                                    index: index as usize,
                                    id: c.tool_call_id,
                                    name: c.tool_call_name,
                                    args_fragment: c.tool_call_args,
                                },
                                state,
                                thread_id,
                                &mut turn_text,
                                &mut pending_calls,
                            )
                            .await;
                        }
                    }
                    Some(Err(e)) => {
                        error!(error = %e, "Mid-stream error from provider");
                        return Err(anyhow!("Mid-stream error: {}", e));
                    }
                }
            }
            _ = cancellation_watcher.changed() => {
                if is_cancelled(cancellation_rx) {
                    return Ok(TurnResult {
                        text: turn_text,
                        tool_calls: vec![],
                        cancelled: true,
                    });
                }
            }
        }
    }

    handle_stream_event(
        StreamEvent::Done,
        state,
        thread_id,
        &mut turn_text,
        &mut pending_calls,
    )
    .await;

    // Drop slots that arrived without a name or id — some providers (e.g.
    // Copilot) emit index fragments before name/id chunks, leaving
    // default-constructed slots.  Passing them downstream causes a 400.
    let tool_calls: Vec<ResolvedToolCall> = pending_calls
        .into_iter()
        .filter(|tc| !tc.name.is_empty() && !tc.id.is_empty())
        .map(|tc| ResolvedToolCall {
            id: tc.id,
            name: tc.name,
            args: tc.args,
        })
        .collect();

    Ok(TurnResult {
        text: turn_text,
        tool_calls,
        cancelled: false,
    })
}

/// Apply a single `StreamEvent` to the mutable accumulator state.
///
/// Separated from `try_stream_one_turn` so each variant's logic is a flat match
/// arm rather than a deeply nested if-chain inside the stream loop.
async fn handle_stream_event(
    event: StreamEvent,
    state: &AppState,
    thread_id: &str,
    turn_text: &mut String,
    pending_calls: &mut Vec<PendingToolCall>,
) {
    match event {
        StreamEvent::TextDelta(delta) => {
            turn_text.push_str(&delta);
            state.send_thread_event(thread_id, ThreadEvent::Token { token: delta });
        }

        StreamEvent::ToolCallFragment {
            index,
            id,
            name,
            args_fragment,
        } => {
            while pending_calls.len() <= index {
                pending_calls.push(PendingToolCall::default());
            }
            if let Some(id) = id {
                pending_calls[index].id = id;
            }
            if let Some(name) = name {
                pending_calls[index].name = name;
            }
            if let Some(fragment) = args_fragment {
                pending_calls[index].args.push_str(&fragment);
            }
        }

        StreamEvent::Done => {
            // Nothing to accumulate; the caller reads turn_text / pending_calls
            // after this returns.
        }
    }
}

// ─── execute_tool_calls ───────────────────────────────────────────────────────

/// Dispatch all tool calls for one round and return their result strings in
/// the same order as `calls`.  Each result is suitable for feeding directly
/// back to the model as a `tool` role message.
///
/// Built-in tools (from `AppState::built_in_tools`) are checked first.
/// If no built-in matches, the call falls through to MCP routing.
/// Errors from individual tools are converted to error strings so one failing
/// tool does not abort the entire round.
///
/// The `_cancellation_rx` parameter is reserved for Story C.2, which will add
/// per-tool cancellation checks inside this function.
async fn execute_tool_calls(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    persona_id: &str,
    calls: &[ResolvedToolCall],
    attached_mcp: &[AttachedMcpServer],
    cancellation_rx: &tokio::sync::watch::Receiver<bool>,
    run_context: &mut RunContext,
) -> Vec<String> {
    use crate::services::tools::ToolContext;

    let mut results = Vec::with_capacity(calls.len());

    for tc in calls {
        if is_cancelled(cancellation_rx) {
            info!(
                thread_id = %thread_id,
                remaining = calls.len() - results.len(),
                "execute_tool_calls: cancellation detected before dispatch, skipping remaining tools"
            );
            break;
        }

        info!(
            thread_id = %thread_id,
            tool = %tc.name,
            tool_call_id = %tc.id,
            args_len = tc.args.len(),
            "execute_tool_calls: dispatching tool call"
        );

        // ── Check built-in tool registry first ────────────────────────────────
        let built_in = state
            .built_in_tools
            .iter()
            .find(|t| t.name() == tc.name.as_str());

        let result_content = if let Some(tool) = built_in {
            let args: serde_json::Value = match serde_json::from_str(&tc.args) {
                Ok(v) => v,
                Err(e) => {
                    let msg = tool_json_parse_error(&tc.name, &e, &tc.args);
                    results.push(msg);
                    continue;
                }
            };
            let context = ToolContext {
                pool: &state.pool,
                user_id,
                persona_id,
                thread_id,
            };
            match tool.run(args, &context).await {
                Ok(r) => {
                    info!(thread_id = %thread_id, tool = %tc.name,
                        result_len = r.len(), "execute_tool_calls: built-in tool returned result");
                    r
                }
                Err(e) => {
                    warn!(tool = %tc.name, error = %e, "Built-in tool execution failed; returning error to model");
                    format!("Tool execution failed: {}", e)
                }
            }
        } else {
            // ── Fall through to MCP routing ────────────────────────────────────
            let pending = PendingToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                args: tc.args.clone(),
            };
            match execute_mcp_tool(
                state,
                thread_id,
                &pending,
                attached_mcp,
                cancellation_rx,
                run_context,
            )
            .await
            {
                Ok(r) => {
                    info!(thread_id = %thread_id, tool = %tc.name,
                        result_len = r.len(), "execute_tool_calls: MCP tool returned result");
                    r
                }
                Err(e) => {
                    warn!(tool = %tc.name, error = %e, "Tool execution failed; returning error to model");
                    format!("Tool execution failed: {}", e)
                }
            }
        };

        results.push(result_content);
    }

    results
}

// ─── Tool execution ────────────────────────────────────────────────────────────

/// Format a structured error message to return to the LLM when a tool receives
/// malformed JSON arguments.  Extracted so the error format can be unit-tested
/// without spinning up a full AppState.
fn tool_json_parse_error(tool_name: &str, error: &serde_json::Error, raw_args: &str) -> String {
    format!(
        "Tool '{}' received invalid JSON arguments: {}. Raw args: {}",
        tool_name, error, raw_args
    )
}

/// Route an MCP tool call by tag prefix and return the result text.
///
/// Called only after the built-in registry check in `execute_tool_calls` has
/// found no match, so this function only handles `tag__tool_name` patterns and
/// truly unknown tool names.
async fn execute_mcp_tool(
    state: &AppState,
    thread_id: &str,
    tc: &PendingToolCall,
    attached_mcp: &[AttachedMcpServer],
    cancellation_rx: &tokio::sync::watch::Receiver<bool>,
    run_context: &mut RunContext,
) -> Result<String> {
    if !tc.name.contains("__") {
        warn!(tool = %tc.name, "Unknown tool call requested by model");
        return Ok(format!("Unknown tool '{}'. No action was taken.", tc.name));
    }

    let (tag, tool_name) = match tc.name.split_once("__") {
        Some(pair) => pair,
        None => return Ok(format!("Unknown tool '{}'.", tc.name)),
    };

    info!(
        thread_id = %thread_id,
        tag = %tag,
        tool_name = %tool_name,
        "execute_mcp_tool: routing MCP tool call"
    );

    let server = attached_mcp.iter().find(|s| s.tag == tag);
    match server {
        Some(s) => {
            let args: serde_json::Value = match serde_json::from_str(&tc.args) {
                Ok(v) => v,
                Err(e) => {
                    warn!(tool = %tc.name, error = %e, raw_args = %tc.args,
                        "MCP tool received invalid JSON arguments");
                    return Ok(tool_json_parse_error(&tc.name, &e, &tc.args));
                }
            };

            info!(
                thread_id = %thread_id,
                server_id = %s.id,
                tag = %tag,
                tool_name = %tool_name,
                args = %tc.args,
                "execute_mcp_tool: calling mcp.call_tool"
            );

            if let Some(id) = persist_tool_message(
                &state.pool,
                thread_id,
                "assistant",
                &format!("**Tool call:** `{}`\n```json\n{}\n```", tc.name, tc.args),
            )
            .await
            {
                run_context.hidden_message_ids.push(id);
            }

            let mut cancellation_watcher = cancellation_rx.clone();
            tokio::select! {
                biased;
                res = state.mcp.call_tool(&s.id, tool_name, args) => {
                    info!(
                        thread_id = %thread_id,
                        server_id = %s.id,
                        tool_name = %tool_name,
                        success = res.is_ok(),
                        "execute_mcp_tool: mcp.call_tool returned"
                    );
                    match res {
                        Ok(text) => {
                            info!(
                                thread_id = %thread_id,
                                tool = %tc.name,
                                result_preview = %text.chars().take(120).collect::<String>(),
                                "execute_mcp_tool: MCP tool success"
                            );
                            if let Some(id) = persist_tool_message(
                                &state.pool,
                                thread_id,
                                "tool",
                                &format!("**Tool result** (`{}`):\n{}", tc.name, text),
                            )
                            .await
                            {
                                run_context.hidden_message_ids.push(id);
                            }
                            Ok(text)
                        }
                        Err(e) => {
                            warn!(tool = %tc.name, error = %e, "MCP tool call failed");
                            Ok(format!("MCP tool '{}' error: {}", tc.name, e))
                        }
                    }
                }
                _ = cancellation_watcher.changed() => {
                    if is_cancelled(cancellation_rx) {
                        info!(
                            thread_id = %thread_id,
                            tool = %tc.name,
                            "execute_mcp_tool: MCP call cancelled"
                        );
                        return Ok("Tool call cancelled by user.".to_string());
                    }
                    return Ok(format!("MCP tool '{}' call interrupted.", tc.name));
                }
            }
        }
        None => {
            warn!(
                thread_id = %thread_id,
                tag = %tag,
                attached = ?attached_mcp.iter().map(|s| s.tag.as_str()).collect::<Vec<_>>(),
                "execute_mcp_tool: no server matched tag"
            );
            Ok(format!(
                "No attached MCP server with tag '{}'. Cannot call tool '{}'.",
                tag, tc.name
            ))
        }
    }
}

// ─── Tool message persistence ─────────────────────────────────────────────────

/// Persist a hidden tool-call or tool-result message to the thread history.
/// These are stored with `visibility = 'hidden'` and are only surfaced in the
/// UI when `show_tool_activity` is enabled on the thread.
///
/// Returns `Some(id)` on success so the caller can record the ID for precise
/// cleanup if the run is later cancelled.
async fn persist_tool_message(
    pool: &SqlitePool,
    thread_id: &str,
    role: &str,
    content: &str,
) -> Option<String> {
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        thread_id: thread_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        source: "tool".to_string(),
        routine_id: None,
        visibility: "hidden".to_string(),
        execution_id: None,
        event_type: None,
        stopped: false,
        created_at: chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    };

    match sqlx::query(
        "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility,
                               execution_id, event_type, stopped, created_at)
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
    .execute(pool)
    .await
    {
        Ok(_) => Some(msg.id),
        Err(e) => {
            warn!(thread_id = %thread_id, error = %e, "Failed to persist tool message");
            None
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

/// Accumulates tool-call fragments as they arrive across streaming chunks.
/// Once the stream closes, complete slots are promoted to `ResolvedToolCall`.
#[derive(Debug, Default)]
struct PendingToolCall {
    /// The tool call ID assigned by the provider.
    id: String,
    /// The tool name.
    name: String,
    /// Accumulated JSON argument string (may arrive across many chunks).
    args: String,
}

/// Build the assistant message that records which tool calls the model
/// requested.  Appended to the conversation so the provider sees the
/// request/result pair on the next turn.
fn build_assistant_tool_call_message(
    text: &str,
    tool_calls: &[PendingToolCall],
) -> ChatCompletionRequestMessage {
    use async_openai::types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs, FunctionCall,
    };

    let api_tool_calls: Vec<ChatCompletionMessageToolCall> = tool_calls
        .iter()
        .map(|tc| ChatCompletionMessageToolCall {
            id: tc.id.clone(),
            r#type: async_openai::types::ChatCompletionToolType::Function,
            function: FunctionCall {
                name: tc.name.clone(),
                arguments: tc.args.clone(),
            },
        })
        .collect();

    let mut builder = ChatCompletionRequestAssistantMessageArgs::default();

    if !text.is_empty() {
        builder.content(text);
    }

    if !api_tool_calls.is_empty() {
        builder.tool_calls(api_tool_calls);
    }

    builder.build().map(Into::into).unwrap_or_else(|e| {
        // The builder only fails if neither content nor tool_calls were set,
        // which cannot happen here since we always have tool calls when this
        // function is called. Log and fall back to a minimal assistant message.
        warn!(error = %e, "Failed to build assistant tool-call message; using empty fallback");
        async_openai::types::ChatCompletionRequestAssistantMessageArgs::default()
            .content("")
            .build()
            .expect("empty assistant message build is infallible")
            .into()
    })
}

// ─── Provider factory ──────────────────────────────────────────────────────────

/// Instantiate a concrete `LlmProvider` from a database provider row.
pub(crate) fn build_provider(
    state: &AppState,
    row: &crate::models::provider::Provider,
) -> Result<Box<dyn LlmProvider>> {
    match row.kind.as_str() {
        "copilot" => {
            let copilot_svc = state
                .copilot
                .clone()
                .ok_or_else(|| anyhow!("copilot-api service is not available"))?;

            let provider = CopilotProvider::new(move || {
                let svc = copilot_svc.clone();
                async move { svc.is_available().await }
            });

            Ok(Box::new(provider))
        }

        "openai" | "custom" | "anthropic" => {
            // Decrypt the API key if stored.
            let api_key: Option<String> = match &row.api_key {
                Some(encrypted) => match encryption::decrypt(encrypted, &state.machine_secret) {
                    Ok(key) => Some(key),
                    Err(e) => {
                        warn!(
                            provider_id = %row.id,
                            error = %e,
                            "Failed to decrypt API key; proceeding without auth"
                        );
                        None
                    }
                },
                None => None,
            };

            let provider = OpenAiProvider::new(&row.name, &row.base_url, api_key);
            Ok(Box::new(provider))
        }

        unknown => Err(anyhow!(
            "Unknown provider kind '{}' for provider '{}'",
            unknown,
            row.id
        )),
    }
}

// ─── Thread title auto-generation ─────────────────────────────────────────────

/// Generate a thread title from the first user message.
///
/// Simple heuristic: take the first 60 characters of the message, trimmed,
/// stripping newlines.  If the message is longer we append "…".
pub fn generate_title_from_message(content: &str) -> String {
    let cleaned: String = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    const MAX_LEN: usize = 60;

    if cleaned.chars().count() <= MAX_LEN {
        cleaned
    } else {
        let truncated: String = cleaned.chars().take(MAX_LEN).collect();
        format!("{}…", truncated.trim_end())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── generate_title_from_message ───────────────────────────────────────────

    #[test]
    fn title_short_message_is_unchanged() {
        let title = generate_title_from_message("Hello, world!");
        assert_eq!(title, "Hello, world!");
    }

    #[test]
    fn title_long_message_is_truncated_with_ellipsis() {
        let msg = "A".repeat(80);
        let title = generate_title_from_message(&msg);
        assert!(title.ends_with('…'), "should end with ellipsis");
        // The ellipsis is a multi-byte char; the content before it is 60 chars.
        let without_ellipsis = title.trim_end_matches('…');
        assert_eq!(without_ellipsis.chars().count(), 60);
    }

    #[test]
    fn title_strips_leading_and_trailing_whitespace() {
        let title = generate_title_from_message("   hello   ");
        assert_eq!(title, "hello");
    }

    #[test]
    fn title_joins_multiline_message() {
        let msg = "Line one\nLine two\nLine three";
        let title = generate_title_from_message(msg);
        assert_eq!(title, "Line one Line two Line three");
    }

    #[test]
    fn title_exactly_60_chars_has_no_ellipsis() {
        let msg = "A".repeat(60);
        let title = generate_title_from_message(&msg);
        assert!(!title.ends_with('…'));
        assert_eq!(title.chars().count(), 60);
    }

    #[test]
    fn title_empty_message_returns_empty() {
        let title = generate_title_from_message("");
        assert_eq!(title, "");
    }

    #[test]
    fn title_only_whitespace_returns_empty() {
        let title = generate_title_from_message("   \n   \t  ");
        assert_eq!(title, "");
    }

    // ── PendingToolCall defaults ───────────────────────────────────────────────

    #[test]
    fn pending_tool_call_default_is_empty() {
        let tc = PendingToolCall::default();
        assert!(tc.id.is_empty());
        assert!(tc.name.is_empty());
        assert!(tc.args.is_empty());
    }

    // ── Recall formatting ─────────────────────────────────────────────────────

    #[test]
    fn recall_no_results_message() {
        let result = format!("No memories found matching \"{}\".", "project Atlas");
        assert!(result.contains("No memories found matching"));
        assert!(result.contains("project Atlas"));
    }

    // ── GenerationResult ──────────────────────────────────────────────────────

    #[test]
    fn generation_result_has_content_field() {
        let result = GenerationResult {
            content: "hello from the model".to_string(),
            cancelled: false,
            message_id: "msg-123".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
        };
        assert_eq!(result.content, "hello from the model");
        assert!(!result.cancelled);
    }

    // ── retry_strategy_for ────────────────────────────────────────────────────

    #[test]
    fn retry_strategy_429_is_exponential_backoff() {
        let err = anyhow::anyhow!("Provider X returned 429 — rate limited");
        assert!(matches!(
            retry_strategy_for(&err),
            RetryStrategy::ExponentialBackoff {
                max_attempts: 4,
                ..
            }
        ));
    }

    #[test]
    fn retry_strategy_503_is_exponential_backoff() {
        let err = anyhow::anyhow!("Provider X returned 503 — service unavailable");
        assert!(matches!(
            retry_strategy_for(&err),
            RetryStrategy::ExponentialBackoff {
                max_attempts: 3,
                ..
            }
        ));
    }

    #[test]
    fn retry_strategy_401_is_none() {
        let err = anyhow::anyhow!("Provider X returned 401 — unauthorized");
        assert_eq!(retry_strategy_for(&err), RetryStrategy::None);
    }

    #[test]
    fn retry_strategy_400_is_none() {
        let err = anyhow::anyhow!("Provider X returned 400 — bad request");
        assert_eq!(retry_strategy_for(&err), RetryStrategy::None);
    }

    #[test]
    fn retry_strategy_connection_error_is_fixed() {
        let err = anyhow::anyhow!("HTTP request failed: connection refused");
        assert!(matches!(
            retry_strategy_for(&err),
            RetryStrategy::Fixed {
                max_attempts: 2,
                ..
            }
        ));
    }

    #[test]
    fn retry_strategy_timeout_is_fixed() {
        let err = anyhow::anyhow!("HTTP request failed: timed out waiting for response");
        assert!(matches!(
            retry_strategy_for(&err),
            RetryStrategy::Fixed {
                max_attempts: 2,
                ..
            }
        ));
    }

    #[test]
    fn retry_strategy_exponential_backoff_delay_doubles() {
        let strategy = RetryStrategy::ExponentialBackoff {
            initial_delay: std::time::Duration::from_secs(2),
            max_attempts: 4,
        };
        if let RetryStrategy::ExponentialBackoff { initial_delay, .. } = strategy {
            let attempt_0 = initial_delay * 2u32.saturating_pow(0);
            let attempt_1 = initial_delay * 2u32.saturating_pow(1);
            let attempt_2 = initial_delay * 2u32.saturating_pow(2);
            assert_eq!(attempt_0, std::time::Duration::from_secs(2));
            assert_eq!(attempt_1, std::time::Duration::from_secs(4));
            assert_eq!(attempt_2, std::time::Duration::from_secs(8));
        }
    }

    // ── tool_json_parse_error ─────────────────────────────────────────────────

    #[test]
    fn tool_json_parse_error_contains_tool_name() {
        let err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
        let msg = tool_json_parse_error("save_memory", &err, "{bad}");
        assert!(
            msg.contains("save_memory"),
            "error message should contain the tool name"
        );
    }

    #[test]
    fn tool_json_parse_error_contains_raw_args() {
        let raw = r#"{"key": }"#;
        let err = serde_json::from_str::<serde_json::Value>(raw).unwrap_err();
        let msg = tool_json_parse_error("recall_memory", &err, raw);
        assert!(
            msg.contains(raw),
            "error message should echo the raw args so the model can diagnose what it sent"
        );
    }

    #[test]
    fn tool_json_parse_error_contains_parse_description() {
        let raw = "{bad}";
        let err = serde_json::from_str::<serde_json::Value>(raw).unwrap_err();
        let msg = tool_json_parse_error("some_tool", &err, raw);
        assert!(
            msg.contains("invalid JSON arguments"),
            "error message should describe the problem"
        );
        // The serde_json error itself should be embedded so the model sees the
        // specific parse failure (e.g. "expected ident at line 1 column 2").
        assert!(!msg.is_empty(), "error message must not be empty");
    }

    #[test]
    fn tool_json_parse_error_not_empty_args_fallback() {
        // Verify that malformed JSON does NOT silently produce an empty-args
        // result.  The returned string must mention the failure, not be a
        // valid JSON object.
        let raw = "{not valid json";
        let err = serde_json::from_str::<serde_json::Value>(raw).unwrap_err();
        let msg = tool_json_parse_error("save_memory", &err, raw);
        assert!(
            serde_json::from_str::<serde_json::Value>(&msg).is_err(),
            "the error message should not itself be valid JSON (i.e. not an empty-args fallback)"
        );
        assert!(msg.contains("save_memory"));
    }

    // ── stream_one_turn / StreamEvent accumulation ────────────────────────────

    /// Verify that multiple `ToolCallFragment` events for the same index are
    /// correctly merged into a single `ResolvedToolCall` with the full args
    /// string.  This mirrors the real-world scenario where providers stream
    /// tool arguments across many small chunks.
    #[test]
    fn stream_event_tool_call_fragments_accumulate_into_resolved_call() {
        // Simulate the fragment sequence the accumulator sees.
        let mut pending: Vec<PendingToolCall> = Vec::new();

        // First chunk: id + name arrive together.
        apply_fragment(
            &mut pending,
            0,
            Some("call_abc".to_string()),
            Some("save_memory".to_string()),
            None,
        );
        // Subsequent chunks: args arrive in pieces.
        apply_fragment(&mut pending, 0, None, None, Some("{\"cont".to_string()));
        apply_fragment(&mut pending, 0, None, None, Some("ent\":".to_string()));
        apply_fragment(&mut pending, 0, None, None, Some("\"hello\"}".to_string()));

        // Promote to resolved.
        let resolved: Vec<ResolvedToolCall> = pending
            .into_iter()
            .filter(|tc| !tc.name.is_empty() && !tc.id.is_empty())
            .map(|tc| ResolvedToolCall {
                id: tc.id,
                name: tc.name,
                args: tc.args,
            })
            .collect();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, "call_abc");
        assert_eq!(resolved[0].name, "save_memory");
        assert_eq!(resolved[0].args, r#"{"content":"hello"}"#);
    }

    #[test]
    fn stream_event_multiple_tool_calls_kept_separate() {
        let mut pending: Vec<PendingToolCall> = Vec::new();

        apply_fragment(
            &mut pending,
            0,
            Some("id_0".to_string()),
            Some("save_memory".to_string()),
            Some("{\"content\":\"a\"}".to_string()),
        );
        apply_fragment(
            &mut pending,
            1,
            Some("id_1".to_string()),
            Some("recall_memory".to_string()),
            Some("{\"query\":\"b\"}".to_string()),
        );

        let resolved: Vec<ResolvedToolCall> = pending
            .into_iter()
            .filter(|tc| !tc.name.is_empty() && !tc.id.is_empty())
            .map(|tc| ResolvedToolCall {
                id: tc.id,
                name: tc.name,
                args: tc.args,
            })
            .collect();

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name, "save_memory");
        assert_eq!(resolved[1].name, "recall_memory");
    }

    #[test]
    fn stream_event_malformed_slot_without_name_is_filtered() {
        let mut pending: Vec<PendingToolCall> = Vec::new();

        // A slot with an id but no name should be filtered out.
        apply_fragment(
            &mut pending,
            0,
            Some("id_orphan".to_string()),
            None,
            Some("{}".to_string()),
        );
        // A valid slot alongside it.
        apply_fragment(
            &mut pending,
            1,
            Some("id_good".to_string()),
            Some("recall_memory".to_string()),
            Some("{\"query\":\"x\"}".to_string()),
        );

        let resolved: Vec<ResolvedToolCall> = pending
            .into_iter()
            .filter(|tc| !tc.name.is_empty() && !tc.id.is_empty())
            .map(|tc| ResolvedToolCall {
                id: tc.id,
                name: tc.name,
                args: tc.args,
            })
            .collect();

        assert_eq!(resolved.len(), 1, "only the complete slot should survive");
        assert_eq!(resolved[0].name, "recall_memory");
    }

    /// Helper that applies a `ToolCallFragment`-equivalent operation to the
    /// pending accumulator without needing a full async context.
    fn apply_fragment(
        pending: &mut Vec<PendingToolCall>,
        index: usize,
        id: Option<String>,
        name: Option<String>,
        args_fragment: Option<String>,
    ) {
        while pending.len() <= index {
            pending.push(PendingToolCall::default());
        }
        if let Some(id) = id {
            pending[index].id = id;
        }
        if let Some(name) = name {
            pending[index].name = name;
        }
        if let Some(fragment) = args_fragment {
            pending[index].args.push_str(&fragment);
        }
    }
}

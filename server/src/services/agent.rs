//! Agent run-loop service — Story 2.5 / 4.4 / 5.1 / H.1 / H.2
//!
//! Responsible for:
//! - Assembling the context (system prompt + message history + memory recall)
//! - Calling the LLM provider via the provider abstraction layer
//! - Streaming tokens back to connected SSE clients
//! - Handling tool calls (save_memory / recall_memory) inline
//! - Persisting the completed assistant message to the database
//! - Emitting `message_complete` and `thread_updated` SSE events
//! - Progress tracking via `RunProgress` so `cancel_run` can perform
//!   principled cleanup after aborting the task (Story 5.1 redesign)
//!
//! ## Structure (H.2)
//!
//! `generation_loop` is a thin coordinator that owns the turn loop.
//! Each turn delegates to:
//!   - `stream_one_turn` — drives the provider stream, emits SSE tokens, and
//!     returns a `TurnResult` (accumulated text + resolved tool calls).
//!   - `execute_tool_calls` — dispatches each tool call and returns results.

use anyhow::{anyhow, Result};
use async_openai::types::{ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs};
use futures::StreamExt;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use tracing::{error, info, warn};

use crate::services::copilot::GlobalEvent;
use crate::{
    models::message::Message,
    routes::{sse::ThreadEvent, AppState, RunPhase, RunProgress},
    services::{
        context::{self, AssemblyInput, HistoryMessage},
        encryption, memory as memory_service,
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

// ─── Memory tool names ─────────────────────────────────────────────────────────

const TOOL_SAVE_MEMORY: &str = "save_memory";
const TOOL_RECALL_MEMORY: &str = "recall_memory";

// ─── Max content length for save_memory ───────────────────────────────────────

const MEMORY_CONTENT_MAX_CHARS: usize = 500;
const MEMORY_RECALL_LIMIT: i64 = 10;

// ─── Public entry point ────────────────────────────────────────────────────────

/// Result of a complete generation loop run.
struct GenerationResult {
    content: String,
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
}

/// Run the agent for a given thread and new user message.
///
/// Accepts a shared `RunProgress` so the cancel handler can inspect phase and
/// partial content after an abort, and a `persist_lock` so the assistant-message
/// INSERT is serialised against any concurrent cancel-handler insert.
///
/// This is intended to be called from inside a `tokio::spawn` — it runs until
/// the LLM has finished generating (including any tool-call rounds) and then
/// emits the final `message_complete` and `thread_updated` SSE events.
///
/// Errors during generation are surfaced as SSE `error` events rather than
/// returned as `Err` values, so the caller doesn't need to do anything special
/// with the return value.
pub async fn run(
    state: AppState,
    thread_id: String,
    user_message: String,
    progress: Arc<Mutex<RunProgress>>,
    persist_lock: Arc<Mutex<()>>,
) {
    if let Err(e) = run_inner(&state, &thread_id, &user_message, progress, persist_lock).await {
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
    progress: Arc<Mutex<RunProgress>>,
    persist_lock: Arc<Mutex<()>>,
) -> Result<()> {
    // ── 0. Transition to BuildingContext ───────────────────────────────────────
    {
        let mut p = progress.lock().await;
        p.phase = RunPhase::BuildingContext;
    }

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
                default_provider, created_at, updated_at
         FROM agent_personas WHERE id = ?",
    )
    .bind(persona_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| anyhow!("Failed to load persona {}: {}", persona_id, e))?;

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
        supports_tools: true,
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
        progress.clone(),
    )
    .await?;

    // ── 6. Persist the completed assistant message ─────────────────────────────
    // Transition to PersistingMessage before acquiring the lock so that
    // cancel_run can see we are about to write when it reads the phase.
    {
        let mut p = progress.lock().await;
        p.phase = RunPhase::PersistingMessage;
    }

    let assistant_msg = Message::new_assistant(thread_id, &gen_result.content);

    // Acquire persist_lock so cancel_run cannot insert a duplicate if it races
    // this INSERT.
    {
        let _lock = persist_lock.lock().await;

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
    }

    // Transition to Complete now that the row is committed.
    {
        let mut p = progress.lock().await;
        p.phase = RunPhase::Complete;
    }

    // Update thread timestamp so it floats to the top of the sidebar.
    sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
        .bind(&assistant_msg.created_at)
        .bind(thread_id)
        .execute(&state.pool)
        .await?;

    // ── 6.5 First-message post-processing ─────────────────────────────────────
    // Count total messages in this thread. If this is the first assistant reply
    // (i.e. exactly 2 messages: 1 user + 1 assistant), run title generation
    // using the first user message and first assistant reply, persist the result,
    // and broadcast TitleUpdated so the client sidebar updates in real time
    // without a page reload or a second HTTP call from the client.
    // Only generate title for non-stopped runs to avoid titling partial responses
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

    // ── 7. Emit SSE events ─────────────────────────────────────────────────────
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

    if let Err(e) = state.send_global_event(GlobalEvent::ThreadUpdated {
        thread_id: thread_id.to_string(),
        last_message: assistant_content.chars().take(120).collect::<String>(),
        updated_at: assistant_msg.created_at.clone(),
    }) {
        // No global-stream subscribers — normal when no client is connected.
        tracing::debug!(thread_id = %thread_id, error = %e, "ThreadUpdated broadcast had no receivers");
    }

    info!(
        thread_id = %thread_id,
        message_id = %assistant_msg.id,
        "Agent run-loop complete"
    );

    Ok(())
}

// ─── Generation loop ──────────────────────────────────────────────────────────

/// Thin coordinator: loops over turns, delegating streaming to `stream_one_turn`
/// and tool dispatch to `execute_tool_calls`.  Returns the full accumulated
/// assistant text across all rounds.
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
    progress: Arc<Mutex<RunProgress>>,
) -> Result<GenerationResult> {
    let mut final_content = String::new();

    // 20 rounds is generous enough for complex agentic workflows while still
    // guarding against infinite tool-call loops.
    const MAX_TOOL_ROUNDS: usize = 20;
    let mut tool_rounds = 0;

    loop {
        info!(thread_id = %thread_id, tool_round = tool_rounds, "generation_loop: starting turn");
        {
            let mut p = progress.lock().await;
            p.phase = RunPhase::Streaming { round: tool_rounds };
            p.tool_round = tool_rounds;
        }
        let turn = stream_one_turn(
            state,
            thread_id,
            provider,
            model_id,
            messages.clone(),
            tools.clone(),
            &progress,
        )
        .await?;

        info!(thread_id = %thread_id, tool_round = tool_rounds,
            text_len = turn.text.len(), tools = turn.tool_calls.len(), "generation_loop: turn finished");

        if turn.tool_calls.is_empty() {
            final_content.push_str(&turn.text);
            break;
        }

        if tool_rounds >= MAX_TOOL_ROUNDS {
            warn!(thread_id = %thread_id,
                "Reached maximum tool-call rounds ({}); stopping generation", MAX_TOOL_ROUNDS);
            final_content.push_str(&turn.text);
            break;
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
            tool_rounds,
            &turn.tool_calls,
            attached_mcp,
            &progress,
        )
        .await;

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

    Ok(GenerationResult {
        content: final_content,
    })
}

// ─── stream_one_turn ─────────────────────────────────────────────────────────

/// Drive the provider stream for one LLM turn.
///
/// Consumes the raw `TokenChunk` stream, classifies each chunk into a
/// `StreamEvent`, accumulates text and tool-call fragments, emits SSE tokens
/// to connected clients, and returns a `TurnResult` when the stream closes.
///
/// Progress is updated on every text token so the cancel handler always has
/// the latest partial content.
async fn stream_one_turn(
    state: &AppState,
    thread_id: &str,
    provider: &dyn LlmProvider,
    model_id: &str,
    messages: Vec<ChatCompletionRequestMessage>,
    tools: Vec<async_openai::types::ChatCompletionTool>,
    progress: &Arc<Mutex<RunProgress>>,
) -> Result<TurnResult> {
    let mut token_stream = match provider.stream(model_id, messages, tools).await {
        Ok(s) => s,
        Err(e) => {
            state.send_thread_event(
                thread_id,
                ThreadEvent::Error {
                    code: "PROVIDER_UNAVAILABLE".to_string(),
                    message: format!(
                        "Failed to start streaming from provider: {}. \
                         Check your API key and provider settings.",
                        e
                    ),
                },
            );
            return Err(anyhow!("Provider stream error: {}", e));
        }
    };

    let mut turn_text = String::new();
    // Keyed by index; slots are grown on demand as fragments arrive.
    let mut pending_calls: Vec<PendingToolCall> = Vec::new();

    loop {
        let chunk = match token_stream.next().await {
            None => break,
            Some(Ok(c)) => c,
            Some(Err(e)) => {
                error!(error = %e, "Mid-stream error from provider");
                state.send_thread_event(
                    thread_id,
                    ThreadEvent::Error {
                        code: "STREAM_ERROR".to_string(),
                        message: format!(
                            "Streaming was interrupted: {}. The response may be incomplete.",
                            e
                        ),
                    },
                );
                return Err(anyhow!("Mid-stream error — response not persisted"));
            }
        };

        // Classify the chunk into a StreamEvent (or two — a single chunk can
        // carry both a text delta and a tool-call fragment).
        if !chunk.delta.is_empty() {
            handle_stream_event(
                StreamEvent::TextDelta(chunk.delta),
                state,
                thread_id,
                progress,
                &mut turn_text,
                &mut pending_calls,
            )
            .await;
        }

        if let Some(index) = chunk.tool_call_index {
            handle_stream_event(
                StreamEvent::ToolCallFragment {
                    index: index as usize,
                    id: chunk.tool_call_id,
                    name: chunk.tool_call_name,
                    args_fragment: chunk.tool_call_args,
                },
                state,
                thread_id,
                progress,
                &mut turn_text,
                &mut pending_calls,
            )
            .await;
        }
    }

    handle_stream_event(
        StreamEvent::Done,
        state,
        thread_id,
        progress,
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
    })
}

/// Apply a single `StreamEvent` to the mutable accumulator state.
///
/// Separated from `stream_one_turn` so each variant's logic is a flat match
/// arm rather than a deeply nested if-chain inside the stream loop.
async fn handle_stream_event(
    event: StreamEvent,
    state: &AppState,
    thread_id: &str,
    progress: &Arc<Mutex<RunProgress>>,
    turn_text: &mut String,
    pending_calls: &mut Vec<PendingToolCall>,
) {
    match event {
        StreamEvent::TextDelta(delta) => {
            turn_text.push_str(&delta);
            // Update progress BEFORE sending the SSE token so that if the task
            // is aborted at the next .await the cancel handler already has this
            // token in assistant_content_so_far and will persist it.
            {
                let mut p = progress.lock().await;
                p.assistant_content_so_far.push_str(&delta);
            }
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
/// Errors from individual tools are converted to error strings so one failing
/// tool does not abort the entire round.
async fn execute_tool_calls(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    persona_id: &str,
    tool_round: usize,
    calls: &[ResolvedToolCall],
    attached_mcp: &[AttachedMcpServer],
    progress: &Arc<Mutex<RunProgress>>,
) -> Vec<String> {
    let mut results = Vec::with_capacity(calls.len());

    for tc in calls {
        {
            let mut p = progress.lock().await;
            p.phase = RunPhase::ExecutingTool {
                round: tool_round,
                tool_name: tc.name.clone(),
            };
        }

        info!(
            thread_id = %thread_id,
            tool = %tc.name,
            tool_call_id = %tc.id,
            args_len = tc.args.len(),
            "execute_tool_calls: dispatching tool call"
        );

        let pending = PendingToolCall {
            id: tc.id.clone(),
            name: tc.name.clone(),
            args: tc.args.clone(),
        };

        let result_content = match execute_tool(
            state,
            &state.pool,
            user_id,
            persona_id,
            thread_id,
            &pending,
            attached_mcp,
        )
        .await
        {
            Ok(r) => {
                info!(
                    thread_id = %thread_id,
                    tool = %tc.name,
                    result_len = r.len(),
                    "execute_tool_calls: tool returned result"
                );
                r
            }
            Err(e) => {
                warn!(tool = %tc.name, error = %e, "Tool execution failed; returning error to model");
                format!("Tool execution failed: {}", e)
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

/// Execute a single tool call, returning the result text to feed back to the LLM.
async fn execute_tool(
    state: &AppState,
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    thread_id: &str,
    tc: &PendingToolCall,
    attached_mcp: &[AttachedMcpServer],
) -> Result<String> {
    match tc.name.as_str() {
        TOOL_SAVE_MEMORY => {
            let args: Value = match serde_json::from_str(&tc.args) {
                Ok(v) => v,
                Err(e) => return Ok(tool_json_parse_error(TOOL_SAVE_MEMORY, &e, &tc.args)),
            };

            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if content.is_empty() {
                return Ok("Memory not saved: content was empty.".to_string());
            }

            // Truncate to 500 chars.
            let content: String = content.chars().take(MEMORY_CONTENT_MAX_CHARS).collect();

            // Cap enforcement is handled entirely inside memory_service::save_memory.
            // If the cap is reached it returns Err; we convert that to a structured
            // tool result rather than propagating it as a hard error so the model
            // can decide what to do next (e.g. recall and discard something).
            match memory_service::save_memory(pool, user_id, persona_id, Some(thread_id), &content)
                .await
            {
                Ok(_) => Ok("Memory saved successfully.".to_string()),
                Err(e) if e.to_string().contains("Memory cap reached") => Ok(
                    "Memory store is full (500 entries). Cannot save new memory until some are deleted."
                        .to_string(),
                ),
                Err(e) => Err(e),
            }
        }

        TOOL_RECALL_MEMORY => {
            let args: Value = match serde_json::from_str(&tc.args) {
                Ok(v) => v,
                Err(e) => return Ok(tool_json_parse_error(TOOL_RECALL_MEMORY, &e, &tc.args)),
            };

            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if query.is_empty() {
                return Ok("No memories found: query was empty.".to_string());
            }

            let entries = memory_service::recall_memory(
                pool,
                user_id,
                persona_id,
                &query,
                MEMORY_RECALL_LIMIT,
            )
            .await;

            let entries = match entries {
                Ok(e) => e,
                Err(e) => {
                    warn!(error = %e, "recall_memory query failed");
                    return Ok(format!(
                        "No memories found matching \"{}\". (Search error: {})",
                        query, e
                    ));
                }
            };

            if entries.is_empty() {
                return Ok(format!("No memories found matching \"{}\".", query));
            }

            // Format per PLAN.md §7.6.5
            let mut lines = vec![format!(
                "Found {} memor{}:",
                entries.len(),
                if entries.len() == 1 { "y" } else { "ies" }
            )];

            let mut thread_sources: Vec<String> = Vec::new();

            for entry in &entries {
                // Parse date from ISO timestamp, fall back to full string.
                let date_str = entry
                    .created_at
                    .split('T')
                    .next()
                    .unwrap_or(&entry.created_at);
                lines.push(format!("- [{}] {}", date_str, entry.content));

                if let Some(title) = &entry.thread_title {
                    if !title.is_empty() && !thread_sources.contains(title) {
                        thread_sources.push(title.clone());
                    }
                }
            }

            if !thread_sources.is_empty() {
                lines.push(format!("\n(Thread sources: {})", thread_sources.join(", ")));
            }

            Ok(lines.join("\n"))
        }

        // ── MCP tool call — route by tag prefix ───────────────────────────────
        name if name.contains("__") => {
            if let Some((tag, tool_name)) = tc.name.split_once("__") {
                info!(
                    thread_id = %thread_id,
                    tag = %tag,
                    tool_name = %tool_name,
                    "execute_tool: routing MCP tool call"
                );

                let server = attached_mcp.iter().find(|s| s.tag == tag);
                match server {
                    Some(s) => {
                        let args: serde_json::Value = match serde_json::from_str(&tc.args) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!(tool = %tc.name, error = %e, raw_args = %tc.args, "MCP tool received invalid JSON arguments");
                                return Ok(tool_json_parse_error(&tc.name, &e, &tc.args));
                            }
                        };

                        info!(
                            thread_id = %thread_id,
                            server_id = %s.id,
                            tag = %tag,
                            tool_name = %tool_name,
                            args = %tc.args,
                            "execute_tool: calling mcp.call_tool"
                        );

                        // Persist hidden tool-call record.
                        persist_tool_message(
                            pool,
                            thread_id,
                            "assistant",
                            &format!("**Tool call:** `{}`\n```json\n{}\n```", tc.name, tc.args),
                        )
                        .await;

                        let result = state.mcp.call_tool(&s.id, tool_name, args).await;
                        info!(
                            thread_id = %thread_id,
                            server_id = %s.id,
                            tool_name = %tool_name,
                            success = result.is_ok(),
                            "execute_tool: mcp.call_tool returned"
                        );
                        match result {
                            Ok(text) => {
                                info!(
                                    thread_id = %thread_id,
                                    tool = %tc.name,
                                    result_preview = %text.chars().take(120).collect::<String>(),
                                    "execute_tool: MCP tool success"
                                );
                                // Persist hidden tool-result record.
                                persist_tool_message(
                                    pool,
                                    thread_id,
                                    "tool",
                                    &format!("**Tool result** (`{}`):\n{}", tc.name, text),
                                )
                                .await;
                                Ok(text)
                            }
                            Err(e) => {
                                let msg = format!("MCP tool '{}' error: {}", tc.name, e);
                                warn!(tool = %tc.name, error = %e, "MCP tool call failed");
                                Ok(msg)
                            }
                        }
                    }
                    None => {
                        let msg = format!(
                            "No attached MCP server with tag '{}'. Cannot call tool '{}'.",
                            tag, tc.name
                        );
                        warn!(
                            thread_id = %thread_id,
                            tag = %tag,
                            attached = ?attached_mcp.iter().map(|s| s.tag.as_str()).collect::<Vec<_>>(),
                            "execute_tool: no server matched tag"
                        );
                        Ok(msg)
                    }
                }
            } else {
                Ok(format!("Unknown tool '{}'.", tc.name))
            }
        }

        unknown => {
            warn!(tool = %unknown, "Unknown tool call requested by model");
            Ok(format!("Unknown tool '{}'. No action was taken.", unknown))
        }
    }
}

// ─── Tool message persistence ─────────────────────────────────────────────────

/// Persist a hidden tool-call or tool-result message to the thread history.
/// These are stored with `visibility = 'hidden'` and are only surfaced in the
/// UI when `show_tool_activity` is enabled on the thread.
async fn persist_tool_message(pool: &SqlitePool, thread_id: &str, role: &str, content: &str) {
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

    if let Err(e) = sqlx::query(
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
        warn!(thread_id = %thread_id, error = %e, "Failed to persist tool message");
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

    // ── Memory content truncation ─────────────────────────────────────────────

    #[test]
    fn memory_content_truncated_at_500_chars() {
        let long = "x".repeat(600);
        let truncated: String = long.chars().take(MEMORY_CONTENT_MAX_CHARS).collect();
        assert_eq!(truncated.len(), 500);
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
        };
        assert_eq!(result.content, "hello from the model");
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
        let msg = tool_json_parse_error(TOOL_SAVE_MEMORY, &err, raw);
        assert!(
            serde_json::from_str::<serde_json::Value>(&msg).is_err(),
            "the error message should not itself be valid JSON (i.e. not an empty-args fallback)"
        );
        assert!(msg.contains(TOOL_SAVE_MEMORY));
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

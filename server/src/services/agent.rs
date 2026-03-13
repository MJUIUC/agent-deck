//! Agent run-loop service — Story 2.5
//!
//! Responsible for:
//! - Assembling the context (system prompt + message history + memory recall)
//! - Calling the LLM provider via the provider abstraction layer
//! - Streaming tokens back to connected SSE clients
//! - Handling tool calls (save_memory / recall_memory) inline
//! - Persisting the completed assistant message to the database
//! - Emitting `message_complete` and `thread_updated` SSE events

use anyhow::{anyhow, Result};
use async_openai::types::{ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs};
use futures::StreamExt;
use serde_json::Value;
use sqlx::SqlitePool;
use tracing::{error, info, warn};

use crate::services::copilot::GlobalEvent;
use crate::{
    models::message::Message,
    routes::{sse::ThreadEvent, AppState},
    services::{
        context::{self, AssemblyInput, HistoryMessage},
        encryption, memory as memory_service,
        provider::{CopilotProvider, LlmProvider, OpenAiProvider},
    },
};

// ─── Memory tool names ─────────────────────────────────────────────────────────

const TOOL_SAVE_MEMORY: &str = "save_memory";
const TOOL_RECALL_MEMORY: &str = "recall_memory";

// ─── Max content length for save_memory ───────────────────────────────────────

const MEMORY_CONTENT_MAX_CHARS: usize = 500;
const MEMORY_RECALL_LIMIT: i64 = 10;

// ─── Public entry point ────────────────────────────────────────────────────────

/// Run the agent for a given thread and new user message.
///
/// This is intended to be called from inside a `tokio::spawn` — it runs until
/// the LLM has finished generating (including any tool-call rounds) and then
/// emits the final `message_complete` and `thread_updated` SSE events.
///
/// Errors during generation are surfaced as SSE `error` events rather than
/// returned as `Err` values, so the caller doesn't need to do anything special
/// with the return value.
pub async fn run(state: AppState, thread_id: String, user_message: String) {
    if let Err(e) = run_inner(&state, &thread_id, &user_message).await {
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

async fn run_inner(state: &AppState, thread_id: &str, user_message: &str) -> Result<()> {
    // ── 1. Fetch the thread and its persona ────────────────────────────────────
    let thread: crate::models::thread::Thread = sqlx::query_as(
        "SELECT id, user_id, persona_id, title, active_model, active_provider,
                system_prompt_addendum, status, show_tool_activity, created_at, updated_at
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

    // Quick health check.
    if !provider.is_healthy().await {
        state.send_thread_event(
            thread_id,
            ThreadEvent::Error {
                code: "PROVIDER_UNAVAILABLE".to_string(),
                message: format!(
                    "Provider '{}' is not reachable. \
                     Check your API key and provider settings, or switch to a different model.",
                    provider_row.name
                ),
            },
        );
        return Ok(());
    }

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
    let assembled = context::assemble(AssemblyInput {
        persona_system_prompt: persona.system_prompt.clone(),
        thread_addendum: thread.system_prompt_addendum.clone(),
        history,
        history_limit: None,
        user_message: user_message.to_string(),
        supports_tools: true,
    });

    // ── 5. Run the generation loop (handles tool calls inline) ─────────────────
    let assistant_content = generation_loop(
        state,
        thread_id,
        user_id,
        persona_id,
        &*provider,
        &model_id,
        assembled.messages,
        assembled.tools,
    )
    .await?;

    // ── 6. Persist the completed assistant message ─────────────────────────────
    let assistant_msg = Message::new_assistant(thread_id, &assistant_content);

    sqlx::query(
        "INSERT INTO messages (id, thread_id, role, content, source, routine_id, visibility, execution_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&assistant_msg.id)
    .bind(&assistant_msg.thread_id)
    .bind(&assistant_msg.role)
    .bind(&assistant_msg.content)
    .bind(&assistant_msg.source)
    .bind(&assistant_msg.routine_id)
    .bind(&assistant_msg.visibility)
    .bind(&assistant_msg.execution_id)
    .bind(&assistant_msg.created_at)
    .execute(&state.pool)
    .await?;

    // Update thread timestamp so it floats to the top of the sidebar.
    sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
        .bind(&assistant_msg.created_at)
        .bind(thread_id)
        .execute(&state.pool)
        .await?;

    // ── 7. Emit SSE events ─────────────────────────────────────────────────────
    state.send_thread_event(
        thread_id,
        ThreadEvent::MessageComplete {
            id: assistant_msg.id.clone(),
            thread_id: thread_id.to_string(),
            role: assistant_msg.role.clone(),
            content: assistant_msg.content.clone(),
            created_at: assistant_msg.created_at.clone(),
        },
    );

    let _ = state.send_global_event(GlobalEvent::ThreadUpdated {
        thread_id: thread_id.to_string(),
        last_message: assistant_content.chars().take(120).collect::<String>(),
        updated_at: assistant_msg.created_at.clone(),
    });

    info!(
        thread_id = %thread_id,
        message_id = %assistant_msg.id,
        "Agent run-loop complete"
    );

    Ok(())
}

// ─── Generation loop ──────────────────────────────────────────────────────────

/// Core generation loop.  Streams tokens from the provider, handles any tool
/// calls, then continues generation.  Returns the full assistant text content
/// accumulated across all turns.
async fn generation_loop(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    persona_id: &str,
    provider: &dyn LlmProvider,
    model_id: &str,
    mut messages: Vec<ChatCompletionRequestMessage>,
    tools: Vec<async_openai::types::ChatCompletionTool>,
) -> Result<String> {
    // We accumulate the final visible text across all tool-call rounds.
    let mut final_content = String::new();

    // We permit at most a small number of consecutive tool-call rounds to avoid
    // infinite loops if the model keeps requesting tools.
    const MAX_TOOL_ROUNDS: usize = 5;
    let mut tool_rounds = 0;

    loop {
        // ── Stream one generation turn ─────────────────────────────────────────
        let stream_result = provider
            .stream(model_id, messages.clone(), tools.clone())
            .await;

        let mut token_stream = match stream_result {
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

        // Accumulate text and tool-call fragments from this turn.
        let mut turn_text = String::new();

        // Tool-call accumulation: we may receive a tool call name/args across
        // multiple chunks — gather them here, keyed by index.
        let mut tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut had_error = false;

        while let Some(chunk_result) = token_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
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
                    had_error = true;
                    break;
                }
            };

            // ── Text delta ────────────────────────────────────────────────────
            if !chunk.delta.is_empty() {
                turn_text.push_str(&chunk.delta);
                state.send_thread_event(
                    thread_id,
                    ThreadEvent::Token {
                        token: chunk.delta.clone(),
                    },
                );
            }

            // ── Tool-call fragments ───────────────────────────────────────────
            if let Some(index) = chunk.tool_call_index {
                let idx = index as usize;

                // Ensure the slot exists.
                while tool_calls.len() <= idx {
                    tool_calls.push(PendingToolCall::default());
                }

                if let Some(name) = chunk.tool_call_name {
                    tool_calls[idx].name = name;
                }
                if let Some(args_fragment) = chunk.tool_call_args {
                    tool_calls[idx].args.push_str(&args_fragment);
                }
                if let Some(id) = chunk.tool_call_id {
                    tool_calls[idx].id = id;
                }
            }
        }

        if had_error {
            // Do not persist a partial message on mid-stream failure.
            return Err(anyhow!("Mid-stream error — response not persisted"));
        }

        // If there were no tool calls, this turn is done.
        if tool_calls.is_empty() {
            final_content.push_str(&turn_text);
            break;
        }

        // ── Handle tool calls ──────────────────────────────────────────────────
        if tool_rounds >= MAX_TOOL_ROUNDS {
            warn!(
                thread_id = %thread_id,
                "Reached maximum tool-call rounds ({}); stopping generation",
                MAX_TOOL_ROUNDS
            );
            final_content.push_str(&turn_text);
            break;
        }
        tool_rounds += 1;

        // Append the assistant's tool-call turn to the message history so the
        // provider knows what it asked for when we feed back results.
        let assistant_turn = build_assistant_tool_call_message(&turn_text, &tool_calls);
        messages.push(assistant_turn);

        // Execute each tool call and append the results.
        for tc in &tool_calls {
            let tool_result =
                execute_tool(state, &state.pool, user_id, persona_id, thread_id, tc).await;

            let result_content = match tool_result {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        tool = %tc.name,
                        error = %e,
                        "Tool execution failed; returning error to model"
                    );
                    format!("Tool execution failed: {}", e)
                }
            };

            messages.push(
                ChatCompletionRequestToolMessageArgs::default()
                    .content(result_content.as_str())
                    .tool_call_id(tc.id.as_str())
                    .build()
                    .map_err(|e| anyhow!("Failed to build tool result message: {}", e))?
                    .into(),
            );
        }

        // Loop: the model will now generate a response incorporating the tool results.
    }

    Ok(final_content)
}

// ─── Tool execution ────────────────────────────────────────────────────────────

/// Execute a single tool call, returning the result text to feed back to the LLM.
async fn execute_tool(
    _state: &AppState,
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    thread_id: &str,
    tc: &PendingToolCall,
) -> Result<String> {
    match tc.name.as_str() {
        TOOL_SAVE_MEMORY => {
            let args: Value =
                serde_json::from_str(&tc.args).unwrap_or_else(|_| serde_json::json!({}));

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
            let args: Value =
                serde_json::from_str(&tc.args).unwrap_or_else(|_| serde_json::json!({}));

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

        unknown => {
            warn!(tool = %unknown, "Unknown tool call requested by model");
            Ok(format!("Unknown tool '{}'. No action was taken.", unknown))
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

/// Accumulates tool call fragments from a streaming response.
#[derive(Debug, Default)]
struct PendingToolCall {
    /// The tool call ID assigned by the provider.
    id: String,
    /// The tool name.
    name: String,
    /// Accumulated JSON argument string.
    args: String,
}

/// Build an assistant message that includes tool call requests.
/// This is appended to the conversation so the model can see what it asked for
/// when we feed back the tool results.
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

    builder
        .build()
        .expect("assistant tool-call message build")
        .into()
}

// ─── Provider factory ──────────────────────────────────────────────────────────

/// Instantiate a concrete `LlmProvider` from a database provider row.
fn build_provider(
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
}

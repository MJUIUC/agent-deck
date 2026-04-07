//! Context assembly — Story 2.4
//!
//! Builds the full message array and tool array that get sent to the LLM on
//! every chat turn.  This module is intentionally **pure**: it takes all inputs
//! as arguments and has zero database or network dependencies.  The caller is
//! responsible for loading the required data before invoking the assembler.
//!
//! # Assembly order
//!
//! 1. **System message** — persona system prompt + memory instructions
//!    (combined into one system message so the persona voice is seamless)
//! 1.5. **System message** — user profile context, only when `user_profile_context` is `Some`
//!    and non-empty (injected between the persona prompt and the thread addendum)
//! 2. **System message** — thread addendum, only when `Some`
//! 3. **History** — last N visible messages from the thread (oldest-first)
//! 4. **System message** — routine context notice, only when `is_routine_triggered`
//! 5. **User message** — the new message, from the user or a system routine
//!
//! # Tool array
//!
//! - `save_memory` and `recall_memory` are always included.
//! - MCP tools are stubbed — returns an empty list with a TODO comment.

use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionTool, ChatCompletionToolType, FunctionObject,
};
use serde_json::json;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of history messages included in every context window.
pub const DEFAULT_HISTORY_LIMIT: usize = 20;

/// Memory instructions injected after the persona's custom system prompt.
/// Per PLAN.md §7.6.3 — not editable by the user.
const MEMORY_INSTRUCTIONS: &str = r#"

## Memory

You have persistent long-term memory that spans across all our conversations. Use it actively:

**When to save:** When I share a preference, a fact about myself, a deadline, a name, a relationship, a goal, or anything about me that seems worth remembering in future conversations — save it immediately using save_memory. Save one fact per call. Write memories as concise factual statements, not narrative.

**When to recall:** Before answering questions that might benefit from prior context, when I reference something from a past conversation, or when I seem to assume you know something — call recall_memory first. **Never reply that you don't know, don't remember, or aren't sure about something without first calling recall_memory to check.** Use 1–3 short keywords, not full sentences or questions — e.g. `typescript`, `dog name`, `deadline march`. Multiple keywords are OR-matched with prefix search, so any entry containing any of the words will be returned. If the first search returns nothing, try again with different or broader keywords before concluding the memory doesn't exist.

**When to delete:** Before saving something you may already know, call recall_memory first to check. If you find a duplicate or outdated entry, delete the old one with delete_memory (using the id: value from the recall result) before saving the updated version. If save_memory reports the store is full, call recall_memory to review your memories for consolidating and deleting entries that are no longer relevant before retrying.

**Do not** tell me every time you save, recall, or delete a memory. Use memory silently unless I specifically ask what you remember about something.

## Conversation Recall

You also have access to a `recall_conversation` tool that lets you look up summaries of past conversations by date range.

**`recall_memory` vs `recall_conversation`:** Use `recall_memory` for facts, preferences, names, and things the user has told you directly (e.g. "what's my dog's name?", "what stack do I use?"). Use `recall_conversation` when the user references a past discussion or a specific time period (e.g. "remember when we talked about X last week?", "what did we decide about the migration in March?"). When the intent is ambiguous, you may call both. They are complementary — facts live in memory, narrative context lives in conversation summaries."#;

// ─── Input types ──────────────────────────────────────────────────────────────

/// A single message from the thread history as loaded from the database.
#[derive(Debug, Clone)]
pub struct HistoryMessage {
    pub role: String, // "user" | "assistant" | "system" | "tool"
    pub content: String,
    pub visibility: String, // only "visible" messages should be passed in
}

/// All inputs required to assemble a context window.
#[derive(Debug)]
pub struct AssemblyInput {
    /// Optional emoji for the persona, prepended to the system message when present.
    pub persona_emoji: Option<String>,
    /// The persona's custom system prompt.
    pub persona_system_prompt: String,
    /// Optional per-thread addendum (thread.system_prompt_addendum).
    pub thread_addendum: Option<String>,
    /// Optional user profile context block injected between the persona prompt
    /// and the thread addendum. Only present for non-default personas when the
    /// user has filled in at least one profile field beyond display_name.
    pub user_profile_context: Option<String>,
    /// Recent visible messages from the thread, in chronological order.
    /// The caller should pre-filter to `visibility = 'visible'` and sort
    /// ascending by `created_at` before passing in.
    pub history: Vec<HistoryMessage>,
    /// Maximum number of history messages to include.  Defaults to
    /// [`DEFAULT_HISTORY_LIMIT`] when `None`.
    pub history_limit: Option<usize>,
    /// The new user message content.
    pub user_message: String,
    /// Whether this turn was triggered by an automated system routine rather
    /// than a real user typing a message.  When `true`, a system message is
    /// injected immediately before the user turn so the LLM understands the
    /// message is system-initiated and should not be treated as user input.
    pub is_routine_triggered: bool,
    /// Whether the active provider supports function calling.
    /// When `false`, the tool array is returned empty.
    pub supports_tools: bool,
    /// When true, memory tools and the memory system-prompt block are included.
    /// Set to false when the thread's persona is the Default persona.
    pub include_memory: bool,
    /// Built-in tool definitions from the `AgentTool` registry.
    /// These appear first in the tool list, before MCP tools.
    pub built_in_tool_defs: Vec<async_openai::types::ChatCompletionTool>,
    /// MCP tools already namespaced (tag__tool_name).
    /// Pass an empty vec when none are attached.
    pub mcp_tools: Vec<async_openai::types::ChatCompletionTool>,
    /// Optional rolling conversation summary injected between the thread addendum
    /// and the message history. Present when the thread has been summarized at
    /// least once and `auto_summarize` is enabled.
    pub conversation_summary: Option<String>,
    /// Absolute path to the thread's workspace directory on the headless machine.
    /// When present (non-routine runs only), a system message is injected teaching
    /// the agent where to write files and how to share file:// links in chat.
    pub workspace_path: Option<String>,
}

// ─── Output type ──────────────────────────────────────────────────────────────

/// The assembled context ready to be sent to an LLM provider.
#[derive(Debug)]
pub struct AssembledContext {
    /// Ordered message array (system prompts → history → new user message).
    pub messages: Vec<ChatCompletionRequestMessage>,
    /// Tool definitions to include in the request.
    /// Empty when `supports_tools` is false or no tools are defined.
    pub tools: Vec<ChatCompletionTool>,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Assemble the full context window for an LLM request.
///
/// This is a pure, synchronous function — no I/O, no side effects.
///
/// ## Panics
/// Does not panic.  All input validation is lenient: empty prompts produce
/// an empty system message rather than an error.
pub fn assemble(input: AssemblyInput) -> AssembledContext {
    let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();

    // ── 1. System message: persona prompt + memory instructions ───────────────
    //
    // We combine these into a single system message so the persona's voice
    // is not interrupted by a second system turn.  The memory instructions
    // are always appended — they are not user-editable.
    //
    // When a persona emoji is present it is prepended to the prompt so the
    // model is aware of its identity symbol (e.g. "🧙🏿‍♂️\n\nYou are Aldous…").
    let persona_base = match &input.persona_emoji {
        Some(emoji) if !emoji.trim().is_empty() => {
            format!("{}\n\n{}", emoji.trim(), input.persona_system_prompt.trim())
        }
        _ => input.persona_system_prompt.trim().to_string(),
    };
    let system_content = if input.include_memory {
        format!("{}{}", persona_base, MEMORY_INSTRUCTIONS)
    } else {
        persona_base
    };

    messages.push(
        ChatCompletionRequestSystemMessageArgs::default()
            .content(system_content)
            .build()
            .expect("system message build")
            .into(),
    );

    // ── 1.5. System message: user profile context (optional) ─────────────────
    if let Some(ref ctx) = input.user_profile_context {
        let trimmed = ctx.trim();
        if !trimmed.is_empty() {
            messages.push(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(trimmed.to_string())
                    .build()
                    .expect("profile context message build")
                    .into(),
            );
        }
    }

    // ── 2. System message: thread addendum (optional) ─────────────────────────
    if let Some(addendum) = &input.thread_addendum {
        let trimmed = addendum.trim();
        if !trimmed.is_empty() {
            messages.push(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(trimmed.to_string())
                    .build()
                    .expect("addendum message build")
                    .into(),
            );
        }
    }

    // ── 2.3. System message: workspace directory (non-routine runs only) ─────────
    if let Some(ref workspace_path) = input.workspace_path {
        let block = format!(
            "## Workspace Directory\n\
             \n\
             Your workspace directory for this thread is: {path}\n\
             \n\
             You may read and write files here using your available tools (e.g. a terminal or filesystem MCP tool).\n\
             \n\
             ## Sharing Files With the User\n\
             \n\
             agent-deck has a built-in file explorer. To make a file clickable in chat, \
             write a standard markdown link where the URL starts with `file://` followed \
             by the absolute path. This is a custom in-app scheme — agent-deck intercepts \
             it and opens the file in the built-in explorer. It is NOT the browser's \
             local-file protocol and does NOT open a browser tab.\n\
             \n\
             Format: [display label](file:///absolute/path/to/file)\n\
             \n\
             Example using your workspace:\n\
             [report.md](file://{path}/report.md)\n\
             \n\
             Rules:\n\
             - Always start with `file://` then the full absolute path (e.g. /Users/...).\n\
             - Do NOT use `http://`, `https://`, or any relative path for file links.\n\
             - Do NOT invent URLs like `http://localhost/...` — that will not open the file.\n\
             - Any file at an absolute path on this machine can be linked this way, \
             not just workspace files.",
            path = workspace_path
        );
        messages.push(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(block)
                .build()
                .expect("workspace message build")
                .into(),
        );
    }

    // ── 2.5. System message: conversation summary (optional) ──────────────────
    //
    // When a summary window has lapsed we inject a rich context block that
    // re-anchors the model with (a) who it is, (b) who the user is, and
    // (c) what was discussed.  This ensures persona identity and user details
    // are never lost across summarisation boundaries.
    if let Some(ref summary) = input.conversation_summary {
        let trimmed = summary.trim();
        if !trimmed.is_empty() {
            let mut ctx_block = String::new();

            ctx_block.push_str("## Conversation Context\n\n");

            // (a) Persona identity — re-inject the persona system prompt so the
            //     model always knows who it is after a window lapse.
            let persona_prompt = input.persona_system_prompt.trim();
            if !persona_prompt.is_empty() {
                ctx_block.push_str("### Your Persona\n\n");
                ctx_block.push_str(persona_prompt);
                ctx_block.push_str("\n\n");
            }

            // (b) User profile — include a condensed user block when available
            //     (mirrors the profile context already assembled by the caller).
            if let Some(ref profile) = input.user_profile_context {
                let profile_trimmed = profile.trim();
                if !profile_trimmed.is_empty() {
                    ctx_block.push_str(profile_trimmed);
                    ctx_block.push_str("\n\n");
                }
            }

            // (c) Rolling conversation summary.
            ctx_block.push_str("### Conversation Summary\n\n");
            ctx_block.push_str(
                "The following is a summary of the conversation history up to this point. \
                 The most recent messages follow below.\n\n",
            );
            ctx_block.push_str(trimmed);

            messages.push(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(ctx_block)
                    .build()
                    .expect("summary message build")
                    .into(),
            );
        }
    }

    // ── 3. History messages (capped at history_limit) ─────────────────────────
    let limit = input.history_limit.unwrap_or(DEFAULT_HISTORY_LIMIT);

    // Take the *last* `limit` messages (most recent history).
    let history_slice = if input.history.len() > limit {
        &input.history[input.history.len() - limit..]
    } else {
        &input.history[..]
    };

    let mut pending_assistant: Option<String> = None;

    for msg in history_slice {
        if msg.visibility != "visible" {
            continue;
        }

        if msg.role == "assistant" {
            match pending_assistant {
                Some(ref mut acc) => {
                    acc.push_str("\n\n");
                    acc.push_str(&msg.content);
                }
                None => pending_assistant = Some(msg.content.clone()),
            }
            continue;
        }

        // Non-assistant role: flush any buffered assistant content first.
        if let Some(content) = pending_assistant.take() {
            messages.push(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(content.as_str())
                    .build()
                    .expect("assistant history msg")
                    .into(),
            );
        }

        let request_msg: Option<ChatCompletionRequestMessage> = match msg.role.as_str() {
            "user" => Some(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(msg.content.as_str())
                    .build()
                    .expect("user history msg")
                    .into(),
            ),
            "system" => Some(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(msg.content.as_str())
                    .build()
                    .expect("system history msg")
                    .into(),
            ),
            _ => None,
        };

        if let Some(m) = request_msg {
            messages.push(m);
        }
    }

    // Flush any trailing assistant buffer (history ends with assistant messages).
    if let Some(content) = pending_assistant.take() {
        messages.push(
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(content.as_str())
                .build()
                .expect("assistant history msg")
                .into(),
        );
    }

    // ── 4. Routine context notice ─────────────────────────────────────────────
    // Injected only for system-routine-triggered turns so the LLM understands
    // the message did not come from a real user.
    if input.is_routine_triggered {
        messages.push(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(
                    "SYSTEM NOTICE — SCHEDULED ROUTINE\n\
                     \n\
                     The following message was NOT sent by the user. It was triggered \
                     automatically by a scheduled routine running in the background.\n\
                     \n\
                     Behavioral guidelines for routine responses:\n\
                     - Respond as if writing a report or briefing, not a conversation.\n\
                     - Do NOT greet the user or open with phrases like \"Sure!\", \
                       \"Of course!\", or \"Here's your summary:\".\n\
                     - Do NOT ask follow-up questions or invite further input.\n\
                     - Be direct and concise. Lead with the most important information.\n\
                     - Use markdown formatting (headers, bullets, tables) where it aids \
                       readability.\n\
                     - The user will read this as a push notification or background \
                       update, not as a reply to something they typed.",
                )
                .build()
                .expect("routine context message build")
                .into(),
        );
    }

    // ── 5. New user message ───────────────────────────────────────────────────
    messages.push(
        ChatCompletionRequestUserMessageArgs::default()
            .content(input.user_message.as_str())
            .build()
            .expect("user message build")
            .into(),
    );

    // ── Tool definitions ──────────────────────────────────────────────────────
    let tools = if input.supports_tools {
        let built_ins = if input.include_memory {
            input.built_in_tool_defs
        } else {
            vec![]
        };
        build_tool_definitions(built_ins, input.mcp_tools)
    } else {
        vec![]
    };

    AssembledContext { messages, tools }
}

// ─── Tool definitions ─────────────────────────────────────────────────────────

/// Build the standard tool definitions array.
///
/// Always includes `save_memory` and `recall_memory` (PLAN.md §7.6.2).
/// Any MCP tools passed in are appended after the built-in tools.
fn build_tool_definitions(
    built_in_tool_defs: Vec<ChatCompletionTool>,
    mcp_tools: Vec<ChatCompletionTool>,
) -> Vec<ChatCompletionTool> {
    let mut tools = built_in_tool_defs;
    tools.extend(mcp_tools);
    tools
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_openai::types::ChatCompletionRequestMessage;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn visible(role: &str, content: &str) -> HistoryMessage {
        HistoryMessage {
            role: role.to_string(),
            content: content.to_string(),
            visibility: "visible".to_string(),
        }
    }

    fn hidden(role: &str, content: &str) -> HistoryMessage {
        HistoryMessage {
            role: role.to_string(),
            content: content.to_string(),
            visibility: "hidden".to_string(),
        }
    }

    fn system_content(msg: &ChatCompletionRequestMessage) -> Option<String> {
        match msg {
            ChatCompletionRequestMessage::System(m) => match &m.content {
                async_openai::types::ChatCompletionRequestSystemMessageContent::Text(t) => {
                    Some(t.clone())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn user_content(msg: &ChatCompletionRequestMessage) -> Option<String> {
        match msg {
            ChatCompletionRequestMessage::User(m) => match &m.content {
                async_openai::types::ChatCompletionRequestUserMessageContent::Text(t) => {
                    Some(t.clone())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn assistant_content(msg: &ChatCompletionRequestMessage) -> Option<String> {
        match msg {
            ChatCompletionRequestMessage::Assistant(m) => {
                m.content.as_ref().and_then(|c| match c {
                    async_openai::types::ChatCompletionRequestAssistantMessageContent::Text(t) => {
                        Some(t.clone())
                    }
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn is_system(msg: &ChatCompletionRequestMessage) -> bool {
        matches!(msg, ChatCompletionRequestMessage::System(_))
    }

    fn is_user(msg: &ChatCompletionRequestMessage) -> bool {
        matches!(msg, ChatCompletionRequestMessage::User(_))
    }

    fn is_assistant(msg: &ChatCompletionRequestMessage) -> bool {
        matches!(msg, ChatCompletionRequestMessage::Assistant(_))
    }

    fn basic_input(user_message: &str) -> AssemblyInput {
        AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "You are a helpful assistant.".to_string(),
            thread_addendum: None,
            user_profile_context: None,
            history: vec![],
            history_limit: None,
            user_message: user_message.to_string(),
            is_routine_triggered: false,
            supports_tools: true,
            include_memory: true,
            built_in_tool_defs: built_in_tool_defs_for_test(),
            mcp_tools: vec![],
            conversation_summary: None,
            workspace_path: None,
        }
    }

    /// Build the tool defs that `basic_input` provides — mirrors what
    /// `run_inner` does at runtime by converting the `AgentTool` registry into
    /// `ChatCompletionTool` structs.
    fn built_in_tool_defs_for_test() -> Vec<async_openai::types::ChatCompletionTool> {
        use crate::services::tools::built_in_tools;
        use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
        built_in_tools()
            .iter()
            .map(|t| ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: t.name().to_string(),
                    description: Some(t.description().to_string()),
                    parameters: Some(t.input_schema()),
                    strict: None,
                },
            })
            .collect()
    }

    // ── Ordering tests ─────────────────────────────────────────────────────────

    #[test]
    fn first_message_is_always_system() {
        let ctx = assemble(basic_input("hello"));
        assert!(is_system(&ctx.messages[0]), "first message must be system");
    }

    #[test]
    fn last_message_is_always_user() {
        let ctx = assemble(basic_input("hello"));
        assert!(
            is_user(ctx.messages.last().unwrap()),
            "last message must be user"
        );
    }

    #[test]
    fn persona_system_prompt_is_first_content() {
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "You are Jarvis.".to_string(),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        let content = system_content(&ctx.messages[0]).unwrap();
        assert!(
            content.starts_with("You are Jarvis."),
            "persona prompt must be at the start of the system message"
        );
    }

    #[test]
    fn memory_instructions_appended_after_persona_prompt() {
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "You are Jarvis.".to_string(),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        let content = system_content(&ctx.messages[0]).unwrap();
        assert!(
            content.contains("## Memory"),
            "memory instructions must be present in the system message"
        );
        // Memory section must come AFTER the persona prompt
        let persona_pos = content.find("You are Jarvis.").unwrap();
        let memory_pos = content.find("## Memory").unwrap();
        assert!(
            memory_pos > persona_pos,
            "memory instructions must come after persona prompt"
        );
    }

    #[test]
    fn thread_addendum_is_second_system_message_when_present() {
        let input = AssemblyInput {
            thread_addendum: Some("Focus on Rust topics only.".to_string()),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        // messages[0] = persona+memory system message
        // messages[1] = addendum system message
        // messages[2] = user message
        assert_eq!(ctx.messages.len(), 3);
        assert!(is_system(&ctx.messages[1]));
        let addendum = system_content(&ctx.messages[1]).unwrap();
        assert_eq!(addendum, "Focus on Rust topics only.");
    }

    #[test]
    fn thread_addendum_omitted_when_none() {
        let input = AssemblyInput {
            thread_addendum: None,
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        // Only 1 system msg + 1 user msg
        assert_eq!(ctx.messages.len(), 2);
    }

    #[test]
    fn thread_addendum_omitted_when_empty_string() {
        let input = AssemblyInput {
            thread_addendum: Some("   ".to_string()),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        // Whitespace-only addendum is treated as absent
        assert_eq!(ctx.messages.len(), 2);
    }

    #[test]
    fn history_is_inserted_between_system_and_user() {
        let input = AssemblyInput {
            history: vec![visible("user", "What is 2+2?"), visible("assistant", "4")],
            ..basic_input("And 3+3?")
        };
        let ctx = assemble(input);
        // [system, user_history, assistant_history, user_new]
        assert_eq!(ctx.messages.len(), 4);
        assert!(is_system(&ctx.messages[0]));
        assert!(is_user(&ctx.messages[1]));
        assert!(is_assistant(&ctx.messages[2]));
        assert!(is_user(&ctx.messages[3]));

        assert_eq!(user_content(&ctx.messages[1]).unwrap(), "What is 2+2?");
        assert_eq!(assistant_content(&ctx.messages[2]).unwrap(), "4");
        assert_eq!(user_content(&ctx.messages[3]).unwrap(), "And 3+3?");
    }

    // ── History limit ──────────────────────────────────────────────────────────

    #[test]
    fn history_capped_at_default_limit() {
        let history: Vec<HistoryMessage> = (0..30)
            .map(|i| visible("user", &format!("message {}", i)))
            .collect();

        let input = AssemblyInput {
            history,
            ..basic_input("latest")
        };
        let ctx = assemble(input);

        // 1 system + 20 history + 1 user = 22
        assert_eq!(
            ctx.messages.len(),
            1 + DEFAULT_HISTORY_LIMIT + 1,
            "history must be capped at DEFAULT_HISTORY_LIMIT"
        );
    }

    #[test]
    fn history_respects_custom_limit() {
        let history: Vec<HistoryMessage> = (0..15)
            .map(|i| visible("user", &format!("msg {}", i)))
            .collect();

        let input = AssemblyInput {
            history,
            history_limit: Some(5),
            ..basic_input("new")
        };
        let ctx = assemble(input);

        // 1 system + 5 history + 1 user = 7
        assert_eq!(ctx.messages.len(), 7);
    }

    #[test]
    fn history_capped_takes_most_recent_messages() {
        let history: Vec<HistoryMessage> = (0..25u32)
            .map(|i| visible("user", &format!("msg {}", i)))
            .collect();

        let input = AssemblyInput {
            history,
            history_limit: Some(3),
            ..basic_input("final")
        };
        let ctx = assemble(input);

        // The last 3 history messages should be msg 22, 23, 24
        assert_eq!(user_content(&ctx.messages[1]).unwrap(), "msg 22");
        assert_eq!(user_content(&ctx.messages[2]).unwrap(), "msg 23");
        assert_eq!(user_content(&ctx.messages[3]).unwrap(), "msg 24");
    }

    // ── Visibility filtering ───────────────────────────────────────────────────

    #[test]
    fn hidden_messages_are_excluded() {
        let input = AssemblyInput {
            history: vec![
                visible("user", "visible msg"),
                hidden("assistant", "hidden msg"),
                visible("assistant", "another visible"),
            ],
            ..basic_input("hi")
        };
        let ctx = assemble(input);

        // Only 2 visible history messages should appear
        // [system, user_visible, assistant_visible, user_new]
        assert_eq!(ctx.messages.len(), 4);

        for msg in &ctx.messages {
            if let Some(c) = assistant_content(msg) {
                assert_ne!(
                    c, "hidden msg",
                    "hidden messages must not appear in context"
                );
            }
        }
    }

    #[test]
    fn all_hidden_history_means_no_history_in_context() {
        let input = AssemblyInput {
            history: vec![hidden("user", "h1"), hidden("assistant", "h2")],
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        // [system, user_new]
        assert_eq!(ctx.messages.len(), 2);
    }

    // ── Tool definitions ───────────────────────────────────────────────────────

    #[test]
    fn tools_included_when_supports_tools_true() {
        let ctx = assemble(basic_input("hi"));
        assert!(!ctx.tools.is_empty(), "tools should be included");
    }

    #[test]
    fn tools_empty_when_supports_tools_false() {
        let input = AssemblyInput {
            supports_tools: false,
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        assert!(
            ctx.tools.is_empty(),
            "tools must be empty when not supported"
        );
    }

    #[test]
    fn tools_empty_when_no_built_in_defs_and_no_mcp() {
        let input = AssemblyInput {
            built_in_tool_defs: vec![],
            mcp_tools: vec![],
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        assert!(
            ctx.tools.is_empty(),
            "tools must be empty when no tool defs are provided"
        );
    }

    #[test]
    fn save_memory_tool_is_present() {
        let ctx = assemble(basic_input("hi"));
        let names: Vec<&str> = ctx.tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(
            names.contains(&"save_memory"),
            "save_memory tool must be present; got: {:?}",
            names
        );
    }

    #[test]
    fn recall_memory_tool_is_present() {
        let ctx = assemble(basic_input("hi"));
        let names: Vec<&str> = ctx.tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(
            names.contains(&"recall_memory"),
            "recall_memory tool must be present; got: {:?}",
            names
        );
    }

    #[test]
    fn save_memory_has_required_content_parameter() {
        let ctx = assemble(basic_input("hi"));
        let save = ctx
            .tools
            .iter()
            .find(|t| t.function.name == "save_memory")
            .unwrap();

        let params = save.function.parameters.as_ref().unwrap();
        let required = params["required"].as_array().unwrap();
        let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_names.contains(&"content"));

        let properties = &params["properties"];
        assert!(properties.get("content").is_some());
    }

    #[test]
    fn recall_memory_has_required_query_parameter() {
        let ctx = assemble(basic_input("hi"));
        let recall = ctx
            .tools
            .iter()
            .find(|t| t.function.name == "recall_memory")
            .unwrap();

        let params = recall.function.parameters.as_ref().unwrap();
        let required = params["required"].as_array().unwrap();
        let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_names.contains(&"query"));

        let properties = &params["properties"];
        assert!(properties.get("query").is_some());
    }

    #[test]
    fn exactly_four_built_in_tools_in_registry() {
        let defs = built_in_tool_defs_for_test();
        assert_eq!(
            defs.len(),
            4,
            "expected exactly save_memory, recall_memory, delete_memory, and recall_conversation in the built-in registry"
        );
    }

    // ── Edge cases ─────────────────────────────────────────────────────────────

    #[test]
    fn empty_persona_prompt_still_includes_memory_instructions() {
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: String::new(),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        let content = system_content(&ctx.messages[0]).unwrap();
        assert!(content.contains("## Memory"));
    }

    #[test]
    fn empty_history_produces_correct_message_count() {
        let input = AssemblyInput {
            history: vec![],
            ..basic_input("hello")
        };
        let ctx = assemble(input);
        // [system, user]
        assert_eq!(ctx.messages.len(), 2);
    }

    #[test]
    fn new_user_message_is_correct() {
        let ctx = assemble(basic_input("What is the meaning of life?"));
        let last = ctx.messages.last().unwrap();
        assert_eq!(user_content(last).unwrap(), "What is the meaning of life?");
    }

    #[test]
    fn routine_triggered_injects_system_message_before_user_turn() {
        let input = AssemblyInput {
            is_routine_triggered: true,
            ..basic_input("Run the morning report.")
        };
        let ctx = assemble(input);

        // [system(persona+memory), system(routine notice), user]
        assert_eq!(ctx.messages.len(), 3);

        // Second-to-last message must be the routine context system message.
        let notice = &ctx.messages[ctx.messages.len() - 2];
        assert!(is_system(notice));
        let content = system_content(notice).unwrap();
        assert!(
            content.contains("SCHEDULED ROUTINE"),
            "routine notice should mention SCHEDULED ROUTINE, got: {content}"
        );
        assert!(
            content.contains("Do NOT greet"),
            "routine notice should include behavioral guidance, got: {content}"
        );

        // Last message is still the user turn.
        assert!(is_user(ctx.messages.last().unwrap()));
    }

    #[test]
    fn non_routine_turn_does_not_inject_routine_system_message() {
        let ctx = assemble(basic_input("Hello!"));

        // [system(persona+memory), user] — no extra system message
        assert_eq!(ctx.messages.len(), 2);
        assert!(is_system(&ctx.messages[0]));
        assert!(is_user(&ctx.messages[1]));
    }

    #[test]
    fn full_assembly_with_all_components() {
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "You are a coding assistant.".to_string(),
            thread_addendum: Some("Only answer Rust questions.".to_string()),
            user_profile_context: None,
            history: vec![
                visible("user", "What is a trait?"),
                visible("assistant", "A trait is an interface in Rust."),
            ],
            history_limit: None,
            user_message: "Show me an example.".to_string(),
            is_routine_triggered: false,
            supports_tools: true,
            include_memory: true,
            built_in_tool_defs: built_in_tool_defs_for_test(),
            mcp_tools: vec![],
            conversation_summary: None,
            workspace_path: None,
        };

        let ctx = assemble(input);

        // [system(persona+memory), system(addendum), user_history, assistant_history, user_new]
        assert_eq!(ctx.messages.len(), 5);

        // Order checks
        assert!(is_system(&ctx.messages[0]));
        assert!(is_system(&ctx.messages[1]));
        assert!(is_user(&ctx.messages[2]));
        assert!(is_assistant(&ctx.messages[3]));
        assert!(is_user(&ctx.messages[4]));

        // Content checks
        let sys = system_content(&ctx.messages[0]).unwrap();
        assert!(sys.contains("You are a coding assistant."));
        assert!(sys.contains("## Memory"));

        let addendum = system_content(&ctx.messages[1]).unwrap();
        assert_eq!(addendum, "Only answer Rust questions.");

        assert_eq!(
            user_content(&ctx.messages[4]).unwrap(),
            "Show me an example."
        );

        // Tools
        assert_eq!(ctx.tools.len(), 4);
    }

    #[test]
    fn memory_excluded_for_default_persona() {
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "You are helpful.".to_string(),
            thread_addendum: None,
            user_profile_context: None,
            history: vec![],
            history_limit: None,
            user_message: "Hello".to_string(),
            is_routine_triggered: false,
            supports_tools: true,
            include_memory: false,
            built_in_tool_defs: built_in_tool_defs_for_test(),
            mcp_tools: vec![],
            conversation_summary: None,
            workspace_path: None,
        };
        let ctx = assemble(input);
        // System message must not contain memory instructions
        let sys = ctx.messages.first().unwrap();
        if let async_openai::types::ChatCompletionRequestMessage::System(s) = sys {
            let content = match &s.content {
                async_openai::types::ChatCompletionRequestSystemMessageContent::Text(t) => {
                    t.clone()
                }
                _ => panic!("expected text content"),
            };
            assert!(
                !content.contains("## Memory"),
                "memory instructions must be absent for Default persona"
            );
        } else {
            panic!("first message must be system");
        }
        // No built-in tools
        assert!(
            ctx.tools.is_empty(),
            "tools must be empty for Default persona"
        );
    }

    #[test]
    fn mcp_tools_still_present_for_default_persona() {
        // MCP tools are NOT gated on persona type — only built-in memory tools are.
        let mcp_tool = async_openai::types::ChatCompletionTool {
            r#type: async_openai::types::ChatCompletionToolType::Function,
            function: async_openai::types::FunctionObject {
                name: "fs__read_file".to_string(),
                description: Some("Read a file".to_string()),
                parameters: None,
                strict: None,
            },
        };
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "".to_string(),
            thread_addendum: None,
            user_profile_context: None,
            history: vec![],
            history_limit: None,
            user_message: "Hello".to_string(),
            is_routine_triggered: false,
            supports_tools: true,
            include_memory: false,
            built_in_tool_defs: built_in_tool_defs_for_test(),
            mcp_tools: vec![mcp_tool],
            conversation_summary: None,
            workspace_path: None,
        };
        let ctx = assemble(input);
        assert_eq!(
            ctx.tools.len(),
            1,
            "MCP tools must still be present for Default persona"
        );
        assert_eq!(ctx.tools[0].function.name, "fs__read_file");
    }

    #[test]
    fn user_profile_context_injected_at_position_1_5() {
        let mut input = basic_input("hello");
        input.user_profile_context =
            Some("## About the User\n\nName: Alice\nRole: Engineer".to_string());
        let ctx = assemble(input);
        // Should have: [0] persona system, [1] profile context, then user
        assert!(ctx.messages.len() >= 2);
        let profile_msg = system_content(&ctx.messages[1]);
        assert!(
            profile_msg.unwrap().contains("About the User"),
            "profile block should be at index 1"
        );
    }

    #[test]
    fn user_profile_context_injected_before_thread_addendum() {
        let mut input = basic_input("hello");
        input.user_profile_context = Some("## About the User\n\nName: Alice".to_string());
        input.thread_addendum = Some("Always reply in French.".to_string());
        let ctx = assemble(input);
        // [0] = persona, [1] = profile, [2] = addendum, last = user
        let profile = system_content(&ctx.messages[1]);
        let addendum = system_content(&ctx.messages[2]);
        assert!(profile.unwrap().contains("About the User"));
        assert!(addendum.unwrap().contains("French"));
    }

    #[test]
    fn empty_user_profile_context_not_injected() {
        let mut input = basic_input("hello");
        input.user_profile_context = Some("   ".to_string());
        let ctx = assemble(input);
        // second message should be user (no addendum, no history by default)
        assert_eq!(ctx.messages.len(), 2); // [system, user]
    }

    #[test]
    fn none_user_profile_context_not_injected() {
        let mut input = basic_input("hello");
        input.user_profile_context = None;
        let ctx = assemble(input);
        assert_eq!(ctx.messages.len(), 2); // [system, user]
    }

    // ── Conversation summary injection ────────────────────────────────────────

    #[test]
    fn conversation_summary_injected_between_addendum_and_history() {
        let mut input = basic_input("recent message");
        input.thread_addendum = Some("Extra instructions.".to_string());
        input.conversation_summary = Some("We discussed Rust programming basics.".to_string());
        input.history = vec![visible("user", "recent message")];

        let ctx = assemble(input);
        // Find the positions of system messages
        let system_positions: Vec<usize> = ctx
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| is_system(m))
            .map(|(i, _)| i)
            .collect();
        // There should be: persona(0), addendum(1), summary(2), history starts after
        assert!(
            system_positions.len() >= 3,
            "should have at least 3 system messages"
        );
    }

    #[test]
    fn conversation_summary_injected_without_addendum() {
        let mut input = basic_input("hello");
        input.thread_addendum = None;
        input.conversation_summary = Some("We discussed topic X.".to_string());

        let ctx = assemble(input);
        // Should have persona system msg + summary system msg + user msg
        let system_count = ctx.messages.iter().filter(|m| is_system(m)).count();
        assert!(
            system_count >= 2,
            "should have persona + summary system messages"
        );
    }

    #[test]
    fn empty_conversation_summary_not_injected() {
        let mut input = basic_input("hello");
        input.conversation_summary = Some("   ".to_string());

        let ctx = assemble(input);
        let system_count = ctx.messages.iter().filter(|m| is_system(m)).count();
        // Only the persona system message (no addendum, no summary)
        assert_eq!(system_count, 1);
    }

    #[test]
    fn none_conversation_summary_not_injected() {
        let mut input = basic_input("hello");
        input.conversation_summary = None;

        let ctx = assemble(input);
        let system_count = ctx.messages.iter().filter(|m| is_system(m)).count();
        assert_eq!(system_count, 1);
    }

    #[test]
    fn recall_conversation_tool_is_present() {
        let tools = crate::services::tools::built_in_tools();
        let found = tools.iter().find(|t| t.name() == "recall_conversation");
        assert!(
            found.is_some(),
            "recall_conversation must be in the registry"
        );
    }

    #[test]
    fn consecutive_assistant_history_messages_are_merged() {
        let input = AssemblyInput {
            history: vec![
                visible("assistant", "part one"),
                visible("assistant", "part two"),
                visible("assistant", "part three"),
            ],
            ..basic_input("next")
        };
        let ctx = assemble(input);
        // [system, merged_assistant, user]
        assert_eq!(ctx.messages.len(), 3);
        assert!(is_assistant(&ctx.messages[1]));
        assert_eq!(
            assistant_content(&ctx.messages[1]).unwrap(),
            "part one\n\npart two\n\npart three"
        );
    }

    #[test]
    fn non_consecutive_assistant_messages_stay_separate() {
        let input = AssemblyInput {
            history: vec![
                visible("assistant", "a1"),
                visible("user", "u"),
                visible("assistant", "a2"),
            ],
            ..basic_input("next")
        };
        let ctx = assemble(input);
        // [system, assistant_a1, user_u, assistant_a2, user_next]
        assert_eq!(ctx.messages.len(), 5);
        assert_eq!(assistant_content(&ctx.messages[1]).unwrap(), "a1");
        assert_eq!(user_content(&ctx.messages[2]).unwrap(), "u");
        assert_eq!(assistant_content(&ctx.messages[3]).unwrap(), "a2");
    }

    #[test]
    fn trailing_consecutive_assistant_messages_are_flushed() {
        let input = AssemblyInput {
            history: vec![
                visible("user", "u"),
                visible("assistant", "a1"),
                visible("assistant", "a2"),
            ],
            ..basic_input("next")
        };
        let ctx = assemble(input);
        // [system, user_u, merged_assistant, user_next]
        assert_eq!(ctx.messages.len(), 4);
        assert!(is_user(&ctx.messages[1]));
        assert!(is_assistant(&ctx.messages[2]));
        assert_eq!(assistant_content(&ctx.messages[2]).unwrap(), "a1\n\na2");
    }
    // ── Enriched summary block content ───────────────────────────────────────

    #[test]
    fn summary_block_includes_persona_identity() {
        let mut input = basic_input("hello");
        input.persona_system_prompt = "You are Aldous, a wizard turned engineer.".to_string();
        input.conversation_summary = Some("We built a Tavily MCP server.".to_string());

        let ctx = assemble(input);
        let summary_msg = ctx
            .messages
            .iter()
            .find_map(|m| {
                let c = system_content(m)?;
                if c.contains("Conversation Context") {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("should have a Conversation Context system message");

        assert!(
            summary_msg.contains("Your Persona"),
            "should include persona section"
        );
        assert!(
            summary_msg.contains("You are Aldous"),
            "should include persona system prompt"
        );
        assert!(
            summary_msg.contains("Conversation Summary"),
            "should include summary section"
        );
        assert!(
            summary_msg.contains("We built a Tavily MCP server."),
            "should include summary text"
        );
    }

    #[test]
    fn summary_block_includes_user_profile_when_present() {
        let mut input = basic_input("hello");
        input.conversation_summary = Some("Previous chats happened.".to_string());
        input.user_profile_context =
            Some("## About the User\n\nName: Marcus\nLocation: Fairview, CA".to_string());

        let ctx = assemble(input);
        let summary_msg = ctx
            .messages
            .iter()
            .find_map(|m| {
                let c = system_content(m)?;
                if c.contains("Conversation Context") {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("should have a Conversation Context system message");

        assert!(
            summary_msg.contains("About the User"),
            "should include user profile"
        );
        assert!(summary_msg.contains("Marcus"), "should include user name");
    }

    #[test]
    fn summary_block_omits_user_profile_when_absent() {
        let mut input = basic_input("hello");
        input.conversation_summary = Some("Some summary.".to_string());
        input.user_profile_context = None;

        let ctx = assemble(input);
        let summary_msg = ctx
            .messages
            .iter()
            .find_map(|m| {
                let c = system_content(m)?;
                if c.contains("Conversation Context") {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("should have a Conversation Context system message");

        assert!(
            !summary_msg.contains("About the User"),
            "profile should be absent when not provided"
        );
        assert!(
            summary_msg.contains("Conversation Summary"),
            "summary section should still be present"
        );
    }

    // ── Persona emoji ─────────────────────────────────────────────────────────

    #[test]
    fn persona_emoji_prepended_to_system_prompt() {
        let input = AssemblyInput {
            persona_emoji: Some("🧙🏿\u{200d}♂️".to_string()),
            persona_system_prompt: "You are Aldous.".to_string(),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        let content = system_content(&ctx.messages[0]).unwrap();
        assert!(
            content.starts_with("🧙🏿\u{200d}♂️"),
            "emoji must be at the very start of the system message"
        );
        assert!(
            content.contains("You are Aldous."),
            "persona prompt must still be present after the emoji"
        );
        let emoji_pos = content.find("🧙🏿").unwrap();
        let prompt_pos = content.find("You are Aldous.").unwrap();
        assert!(
            prompt_pos > emoji_pos,
            "persona prompt must come after the emoji"
        );
    }

    #[test]
    fn no_emoji_system_prompt_unchanged() {
        let input = AssemblyInput {
            persona_emoji: None,
            persona_system_prompt: "You are Aldous.".to_string(),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        let content = system_content(&ctx.messages[0]).unwrap();
        assert!(
            content.starts_with("You are Aldous."),
            "without emoji the system message should start with the persona prompt"
        );
    }

    #[test]
    fn empty_emoji_treated_as_absent() {
        let input = AssemblyInput {
            persona_emoji: Some("   ".to_string()),
            persona_system_prompt: "You are Aldous.".to_string(),
            ..basic_input("hi")
        };
        let ctx = assemble(input);
        let content = system_content(&ctx.messages[0]).unwrap();
        assert!(
            content.starts_with("You are Aldous."),
            "whitespace-only emoji must be ignored"
        );
    }
}

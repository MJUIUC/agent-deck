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
//! 2. **System message** — thread addendum, only when `Some`
//! 3. **History** — last N visible messages from the thread (oldest-first)
//! 4. **User message** — the new message just sent by the user
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

**When to save:** When I share a preference, a fact about myself, a project detail, a deadline, a name, a relationship, a goal, or anything that seems worth remembering in future conversations — save it immediately using save_memory. Save one fact per call. Write memories as concise factual statements, not narrative.

**When to recall:** Before answering questions that might benefit from prior context, when I reference something from a past conversation, or when I seem to assume you know something — check your memory using recall_memory. Use specific keywords, not full sentences.

**Do not** tell me every time you save or recall a memory. Use memory silently unless I specifically ask what you remember about something."#;

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
    /// The persona's custom system prompt.
    pub persona_system_prompt: String,
    /// Optional per-thread addendum (thread.system_prompt_addendum).
    pub thread_addendum: Option<String>,
    /// Recent visible messages from the thread, in chronological order.
    /// The caller should pre-filter to `visibility = 'visible'` and sort
    /// ascending by `created_at` before passing in.
    pub history: Vec<HistoryMessage>,
    /// Maximum number of history messages to include.  Defaults to
    /// [`DEFAULT_HISTORY_LIMIT`] when `None`.
    pub history_limit: Option<usize>,
    /// The new user message content.
    pub user_message: String,
    /// Whether the active provider supports function calling.
    /// When `false`, the tool array is returned empty.
    pub supports_tools: bool,
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
    let system_content = format!(
        "{}{}",
        input.persona_system_prompt.trim(),
        MEMORY_INSTRUCTIONS
    );

    messages.push(
        ChatCompletionRequestSystemMessageArgs::default()
            .content(system_content)
            .build()
            .expect("system message build")
            .into(),
    );

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

    // ── 3. History messages (capped at history_limit) ─────────────────────────
    let limit = input.history_limit.unwrap_or(DEFAULT_HISTORY_LIMIT);

    // Take the *last* `limit` messages (most recent history).
    let history_slice = if input.history.len() > limit {
        &input.history[input.history.len() - limit..]
    } else {
        &input.history[..]
    };

    for msg in history_slice {
        // Skip hidden messages defensively — the caller should have filtered
        // these out already, but we guard here for correctness.
        if msg.visibility != "visible" {
            continue;
        }

        let request_msg: Option<ChatCompletionRequestMessage> = match msg.role.as_str() {
            "user" => Some(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(msg.content.as_str())
                    .build()
                    .expect("user history msg")
                    .into(),
            ),
            "assistant" => Some(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(msg.content.as_str())
                    .build()
                    .expect("assistant history msg")
                    .into(),
            ),
            "system" => Some(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(msg.content.as_str())
                    .build()
                    .expect("system history msg")
                    .into(),
            ),
            // "tool" and other roles: skip for now — tool result handling is
            // wired in the agent run-loop which re-assembles context inline.
            _ => None,
        };

        if let Some(m) = request_msg {
            messages.push(m);
        }
    }

    // ── 4. New user message ───────────────────────────────────────────────────
    messages.push(
        ChatCompletionRequestUserMessageArgs::default()
            .content(input.user_message.as_str())
            .build()
            .expect("user message build")
            .into(),
    );

    // ── Tool definitions ──────────────────────────────────────────────────────
    let tools = if input.supports_tools {
        build_tool_definitions()
    } else {
        vec![]
    };

    AssembledContext { messages, tools }
}

// ─── Tool definitions ─────────────────────────────────────────────────────────

/// Build the standard tool definitions array.
///
/// Always includes `save_memory` and `recall_memory` (PLAN.md §7.6.2).
/// MCP tools are stubbed — returns an empty list with a TODO marker.
fn build_tool_definitions() -> Vec<ChatCompletionTool> {
    let mut tools = vec![
        // ── save_memory ───────────────────────────────────────────────────────
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: "save_memory".to_string(),
                description: Some(
                    "Save a piece of information to your long-term memory for future recall. \
                     Use this when the user shares a preference, a fact about themselves, a \
                     project name, a deadline, a relationship detail, or anything worth \
                     remembering across conversations. Save one fact per call. Write the \
                     memory as a concise factual statement."
                        .to_string(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "A concise factual statement to remember. \
                                Examples: 'User prefers TypeScript over JavaScript', \
                                'User\\'s dog is named Pepper', \
                                'Project Atlas deadline is March 15 2025', \
                                'User dislikes being called buddy'"
                        }
                    },
                    "required": ["content"]
                })),
                strict: None,
            },
        },
        // ── recall_memory ─────────────────────────────────────────────────────
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: "recall_memory".to_string(),
                description: Some(
                    "Search your long-term memory for information you've previously saved \
                     about the user. Use this before answering questions that might benefit \
                     from prior context, when the user references something from a past \
                     conversation, or when you need to check if you already know something. \
                     Returns up to 10 matching entries."
                        .to_string(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Keywords to search for in memory. Use specific \
                                nouns and terms rather than full sentences. \
                                Examples: 'project deadline', 'dog name', \
                                'programming language preference'"
                        }
                    },
                    "required": ["query"]
                })),
                strict: None,
            },
        },
    ];

    // TODO: Phase 7 — load tools from attached MCP servers and append here.
    let _mcp_tools: Vec<ChatCompletionTool> = vec![];

    tools.extend(_mcp_tools);
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
            persona_system_prompt: "You are a helpful assistant.".to_string(),
            thread_addendum: None,
            history: vec![],
            history_limit: None,
            user_message: user_message.to_string(),
            supports_tools: true,
        }
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
    fn exactly_two_memory_tools_defined() {
        let ctx = assemble(basic_input("hi"));
        assert_eq!(
            ctx.tools.len(),
            2,
            "expected exactly save_memory and recall_memory"
        );
    }

    // ── Edge cases ─────────────────────────────────────────────────────────────

    #[test]
    fn empty_persona_prompt_still_includes_memory_instructions() {
        let input = AssemblyInput {
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
    fn full_assembly_with_all_components() {
        let input = AssemblyInput {
            persona_system_prompt: "You are a coding assistant.".to_string(),
            thread_addendum: Some("Only answer Rust questions.".to_string()),
            history: vec![
                visible("user", "What is a trait?"),
                visible("assistant", "A trait is an interface in Rust."),
            ],
            history_limit: None,
            user_message: "Show me an example.".to_string(),
            supports_tools: true,
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
        assert_eq!(ctx.tools.len(), 2);
    }
}

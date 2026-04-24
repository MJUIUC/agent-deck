//! Title generation service — shared between agent::run_inner and the manual
//! generate-title HTTP endpoint.
//!
//! # Why this exists
//!
//! Title generation was originally inlined in `routes/threads.rs`. Moving it
//! here allows `services/agent.rs` to call the same logic without creating a
//! dependency between two route modules.
//!
//! # Entry point
//!
//! [`try_llm_title`] — attempts LLM-based title generation for a thread's first
//! exchange. Falls back to a plain truncation of the user's message on any
//! error so the caller always receives a usable string.

use tracing::warn;

use crate::{
    models::thread::Thread,
    routes::AppState,
    services::{agent, provider::LlmProvider},
};

// ─── Public API ───────────────────────────────────────────────────────────────

/// Attempt to generate a thread title using the thread's active LLM.
///
/// The caller provides the first user message and the first assistant reply.
/// On any error (provider not found, model not resolved, LLM call failed) the
/// function returns a plain truncation of `user_content` so the caller always
/// gets a usable string and never needs to handle an error case.
///
/// # Arguments
///
/// * `state`             — application state (DB pool, provider registry, etc.)
/// * `thread`            — the thread whose title is being generated
/// * `user_content`      — content of the first user message in the thread
/// * `assistant_content` — content of the first assistant reply (may be empty)
pub async fn try_llm_title(
    state: &AppState,
    thread: &Thread,
    user_content: &str,
    assistant_content: &str,
) -> String {
    let fallback = agent::generate_title_from_message(user_content);

    // ── Resolve provider ──────────────────────────────────────────────────────
    // Prefer the thread's own active_provider; fall back to the persona's
    // default_provider.

    let provider_id = match thread.active_provider.as_deref() {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            let row: Option<(String,)> =
                sqlx::query_as("SELECT default_provider FROM agent_personas WHERE id = ?")
                    .bind(&thread.persona_id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();

            match row {
                Some((p,)) if !p.is_empty() => p,
                _ => {
                    warn!(
                        thread_id = %thread.id,
                        "try_llm_title: no provider resolved — using fallback title"
                    );
                    return fallback;
                }
            }
        }
    };

    // ── Load the provider row ─────────────────────────────────────────────────

    let provider_row: Option<crate::models::provider::Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, vision, created_at
         FROM providers WHERE id = ? AND user_id = ?",
    )
    .bind(&provider_id)
    .bind(&thread.user_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let provider_row = match provider_row {
        Some(r) => r,
        None => {
            warn!(
                thread_id = %thread.id,
                provider_id = %provider_id,
                "try_llm_title: provider row not found — using fallback title"
            );
            return fallback;
        }
    };

    // ── Resolve model ─────────────────────────────────────────────────────────
    // Prefer the thread's active_model UUID (resolved to a model_id string);
    // fall back to the persona's default_model UUID.

    let model_id = match thread.active_model.as_deref() {
        Some(m) if !m.is_empty() => {
            let row: Option<(String,)> = sqlx::query_as("SELECT model_id FROM models WHERE id = ?")
                .bind(m)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
            match row {
                Some((mid,)) => mid,
                None => {
                    warn!(
                        thread_id = %thread.id,
                        model_uuid = %m,
                        "try_llm_title: active_model UUID not in models table — using fallback title"
                    );
                    return fallback;
                }
            }
        }
        _ => {
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
                None => {
                    warn!(
                        thread_id = %thread.id,
                        "try_llm_title: persona default_model not resolved — using fallback title"
                    );
                    return fallback;
                }
            }
        }
    };

    // ── Call the LLM ──────────────────────────────────────────────────────────

    let provider: Box<dyn LlmProvider> = match agent::build_provider(state, &provider_row) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                thread_id = %thread.id,
                error = %e,
                "try_llm_title: build_provider failed — using fallback title"
            );
            return fallback;
        }
    };

    let prompt = build_title_prompt(user_content, assistant_content);

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
        Err(e) => {
            warn!(
                thread_id = %thread.id,
                error = %e,
                "try_llm_title: LLM call failed — using fallback title"
            );
            fallback
        }
    }
}

/// Build the LLM prompt used to generate a thread title from the first exchange.
///
/// Exposed as a pure function so it can be unit-tested independently of the
/// database and provider machinery in [`try_llm_title`].
pub fn build_title_prompt(user_content: &str, assistant_content: &str) -> String {
    format!(
        "Generate a chat title no longer than 7 words that describes the intent \
         of the conversation below. Reply with only the title — no punctuation, \
         no quotes, no explanation. If the messages are unclear, invent a \
         plausible short title rather than asking for more context.\n\n\
         User: {}\n\
         Assistant: {}",
        user_content.chars().take(500).collect::<String>(),
        assistant_content.chars().take(500).collect::<String>(),
    )
}

/// Truncate a title string to at most 60 characters, breaking at a word
/// boundary and appending `…` when truncation occurs.
///
/// Returns `"New Chat"` for blank input.
pub fn truncate_title(s: &str) -> String {
    const MAX_LEN: usize = 60;
    let s = s.trim();
    if s.is_empty() {
        return "New Chat".to_string();
    }
    if s.chars().count() <= MAX_LEN {
        return s.to_string();
    }
    let truncated: String = s.chars().take(MAX_LEN).collect();
    let at_word = truncated
        .rfind(' ')
        .map(|i| truncated[..i].trim_end().to_string())
        .unwrap_or_else(|| truncated.trim_end().to_string());
    format!("{}…", at_word)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{build_title_prompt, truncate_title};

    #[test]
    fn short_title_is_unchanged() {
        assert_eq!(truncate_title("Hello world"), "Hello world");
    }

    #[test]
    fn blank_returns_new_chat() {
        assert_eq!(truncate_title(""), "New Chat");
        assert_eq!(truncate_title("   "), "New Chat");
    }

    #[test]
    fn exactly_60_chars_has_no_ellipsis() {
        let s: String = "a".repeat(60);
        assert_eq!(truncate_title(&s), s);
    }

    #[test]
    fn long_title_truncates_at_word_boundary_with_ellipsis() {
        let s = "one two three four five six seven eight nine ten eleven twelve";
        let result = truncate_title(s);
        assert!(result.ends_with('…'));
        assert!(result.chars().count() <= 61); // ≤60 chars + ellipsis char
    }

    #[test]
    fn long_title_no_spaces_truncates_at_char_limit() {
        let s: String = "a".repeat(80);
        let result = truncate_title(&s);
        assert!(result.ends_with('…'));
    }

    // ── build_title_prompt ────────────────────────────────────────────────────

    #[test]
    fn title_prompt_contains_user_and_assistant_content() {
        let prompt = build_title_prompt("Hello there", "Hi, how can I help?");
        assert!(prompt.contains("Hello there"));
        assert!(prompt.contains("Hi, how can I help?"));
    }

    #[test]
    fn title_prompt_truncates_long_user_content_to_500_chars() {
        let long = "u".repeat(600);
        let prompt = build_title_prompt(&long, "");
        // The 600-char string should be truncated to 500 in the prompt
        let user_section = prompt
            .split("User: ")
            .nth(1)
            .unwrap()
            .split('\n')
            .next()
            .unwrap();
        assert_eq!(user_section.chars().count(), 500);
    }

    #[test]
    fn title_prompt_truncates_long_assistant_content_to_500_chars() {
        let long = "a".repeat(600);
        let prompt = build_title_prompt("Hi", &long);
        let assistant_section = prompt.split("Assistant: ").nth(1).unwrap();
        assert_eq!(assistant_section.chars().count(), 500);
    }

    #[test]
    fn title_prompt_includes_instruction_keywords() {
        let prompt = build_title_prompt("anything", "anything");
        assert!(prompt.contains("7 words"));
        assert!(prompt.contains("no punctuation"));
        assert!(prompt.contains("no quotes"));
    }

    #[test]
    fn title_prompt_empty_assistant_content_is_valid() {
        let prompt = build_title_prompt("What is Rust?", "");
        assert!(prompt.contains("What is Rust?"));
        assert!(prompt.contains("Assistant: \n") || prompt.ends_with("Assistant: "));
    }
}

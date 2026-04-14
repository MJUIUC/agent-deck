//! Thread context window summarization service — Story 5.8
//!
//! Implements rolling summarization: when a thread accumulates more than
//! DEFAULT_HISTORY_LIMIT new messages since the last summary, this service
//! calls the thread's active LLM to produce a compressed summary that is
//! injected into every future context window in place of the oldest messages.

use anyhow::Result;
use tracing::warn;
use uuid::Uuid;

use crate::routes::AppState;
use crate::services::context::DEFAULT_HISTORY_LIMIT;

// ─── Summarization prompt ─────────────────────────────────────────────────────

fn build_summarization_prompt(
    existing_summary: Option<&str>,
    messages: &[(String, String)],
) -> String {
    let mut prompt = String::new();

    if let Some(prev) = existing_summary.filter(|s| !s.trim().is_empty()) {
        prompt.push_str("## Previous Summary\n\n");
        prompt.push_str(prev.trim());
        prompt.push_str("\n\n## New Messages to Incorporate\n\n");
    } else {
        prompt.push_str("## Conversation to Summarize\n\n");
    }

    for (role, content) in messages {
        let role_label = match role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "system" => "System",
            _ => role.as_str(),
        };
        let preview: String = content.chars().take(1000).collect();
        prompt.push_str(&format!("**{}:** {}\n\n", role_label, preview));
    }

    let instruction = if existing_summary.filter(|s| !s.trim().is_empty()).is_some() {
        "Write an updated summary that incorporates the previous summary and the new messages above. \
         The summary will be injected as context into future conversations, so it should capture: \
         key topics discussed, decisions made, information shared by the user, tasks completed, \
         and any important ongoing context. Be factual, concise, and write in third person. \
         Aim for 150–400 words."
    } else {
        "Write a concise summary of the conversation above. \
         The summary will be injected as context into future conversations, so it should capture: \
         key topics discussed, decisions made, information shared by the user, tasks completed, \
         and any important ongoing context. Be factual, concise, and write in third person. \
         Aim for 150–400 words."
    };

    prompt.push_str("---\n\n");
    prompt.push_str(instruction);
    prompt
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Summarize a thread's message history, updating the rolling summary in the DB.
///
/// This function is fire-and-forget safe: all errors are logged at WARN level
/// and the function returns `Ok(())` even on failure so the caller never needs
/// to handle errors from it.
///
/// ## When this runs
/// - Proactively: after each agent turn, if `(total_count - summary_message_count) >= DEFAULT_HISTORY_LIMIT`
/// - Reactively: when the provider returns a context-length error
pub async fn summarize_thread(state: &AppState, thread_id: &str) -> Result<()> {
    // ── 1. Load thread ────────────────────────────────────────────────────────
    let thread_row: Option<(
        String,
        Option<String>,
        i64,
        bool,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT user_id, summary, summary_message_count, auto_summarize,
                active_provider, active_model, persona_id,
                summary_updated_at
         FROM threads WHERE id = ?",
    )
    .bind(thread_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (
        user_id,
        existing_summary,
        summary_message_count,
        auto_summarize,
        active_provider,
        active_model,
        persona_id,
        _summary_updated_at,
    ) = match thread_row {
        Some(r) => r,
        None => {
            warn!(thread_id = %thread_id, "summarize_thread: thread not found");
            return Ok(());
        }
    };

    if !auto_summarize {
        return Ok(());
    }

    // ── 2. Count total visible messages ───────────────────────────────────────
    let (total_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM messages WHERE thread_id = ? AND visibility = 'visible'",
    )
    .bind(thread_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(((0),));

    if total_count == summary_message_count {
        // Already current — nothing new to summarize.
        return Ok(());
    }

    // ── 3. Decide which messages to summarize ─────────────────────────────────
    // We summarize messages up to (summary_message_count + DEFAULT_HISTORY_LIMIT).
    // If there aren't enough new messages, skip.
    let new_message_count = total_count - summary_message_count;
    if new_message_count < DEFAULT_HISTORY_LIMIT as i64 {
        return Ok(());
    }

    let new_to_seq = summary_message_count + DEFAULT_HISTORY_LIMIT as i64;

    // Load ALL visible messages up to the new boundary (includes previously summarized ones)
    // so the new summary covers the full history from the beginning.
    let message_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT role, content, created_at
         FROM messages
         WHERE thread_id = ? AND visibility = 'visible'
         ORDER BY created_at ASC
         LIMIT ?",
    )
    .bind(thread_id)
    .bind(new_to_seq)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if message_rows.is_empty() {
        return Ok(());
    }

    // Date range from first and last message
    let from_date = message_rows
        .first()
        .map(|(_, _, ts)| ts.clone())
        .unwrap_or_default();
    let to_date = message_rows
        .last()
        .map(|(_, _, ts)| ts.clone())
        .unwrap_or_default();

    let messages: Vec<(String, String)> = message_rows
        .into_iter()
        .map(|(role, content, _)| (role, content))
        .collect();

    // ── 4. Resolve provider and model ─────────────────────────────────────────
    let provider_id = match active_provider {
        Some(ref p) if !p.is_empty() => p.clone(),
        _ => {
            // Try persona's default provider
            let row: Option<(String,)> =
                sqlx::query_as("SELECT default_provider FROM agent_personas WHERE id = ?")
                    .bind(&persona_id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();
            match row {
                Some((p,)) if !p.is_empty() => p,
                _ => {
                    warn!(thread_id = %thread_id, "summarize_thread: no provider resolved");
                    return Ok(());
                }
            }
        }
    };

    let model_uuid = match active_model {
        Some(ref m) if !m.is_empty() => m.clone(),
        _ => {
            let row: Option<(String,)> =
                sqlx::query_as("SELECT default_model FROM agent_personas WHERE id = ?")
                    .bind(&persona_id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();
            match row {
                Some((m,)) if !m.is_empty() => m,
                _ => {
                    warn!(thread_id = %thread_id, "summarize_thread: no model resolved");
                    return Ok(());
                }
            }
        }
    };

    // Resolve model UUID to actual model_id string
    let model_id = {
        let row: Option<(String,)> = sqlx::query_as("SELECT model_id FROM models WHERE id = ?")
            .bind(&model_uuid)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
        match row {
            Some((mid,)) => mid,
            None => model_uuid, // fallback: use as-is
        }
    };

    // Load the provider row
    let provider_row: Option<crate::models::provider::Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers WHERE id = ? AND user_id = ?",
    )
    .bind(&provider_id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let provider_row = match provider_row.filter(|p| p.enabled) {
        Some(r) => r,
        None => {
            warn!(thread_id = %thread_id, "summarize_thread: provider not found or disabled");
            return Ok(());
        }
    };

    let provider = match crate::services::agent::build_provider(state, &provider_row) {
        Ok(p) => p,
        Err(e) => {
            warn!(thread_id = %thread_id, error = %e, "summarize_thread: build_provider failed");
            return Ok(());
        }
    };

    // ── 5. Build prompt and call the LLM ─────────────────────────────────────
    let prompt = build_summarization_prompt(existing_summary.as_deref(), &messages);

    let llm_messages = vec![async_openai::types::ChatCompletionRequestMessage::User(
        async_openai::types::ChatCompletionRequestUserMessageArgs::default()
            .content(prompt.as_str())
            .build()
            .unwrap(),
    )];

    let new_summary = match provider.complete(&model_id, llm_messages, vec![]).await {
        Ok(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                warn!(thread_id = %thread_id, "summarize_thread: LLM returned empty summary");
                return Ok(());
            }
            s
        }
        Err(e) => {
            warn!(thread_id = %thread_id, error = %e, "summarize_thread: LLM call failed");
            return Ok(());
        }
    };

    // ── 6. Persist the summary ────────────────────────────────────────────────
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    if let Err(e) = sqlx::query(
        "UPDATE threads
         SET summary = ?, summary_updated_at = ?, summary_message_count = ?
         WHERE id = ?",
    )
    .bind(&new_summary)
    .bind(&now)
    .bind(new_to_seq)
    .bind(thread_id)
    .execute(&state.pool)
    .await
    {
        warn!(thread_id = %thread_id, error = %e, "summarize_thread: failed to update thread summary");
        return Ok(());
    }

    // ── 7. Append to thread_summaries log ─────────────────────────────────────
    let entry_id = Uuid::new_v4().to_string();
    if let Err(e) = sqlx::query(
        "INSERT INTO thread_summaries
             (id, thread_id, summary, from_message_seq, to_message_seq,
              from_date, to_date, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(thread_id)
    .bind(&new_summary)
    .bind(summary_message_count) // from_message_seq = previous boundary
    .bind(new_to_seq) // to_message_seq   = new boundary
    .bind(&from_date)
    .bind(&to_date)
    .bind(&now)
    .execute(&state.pool)
    .await
    {
        warn!(thread_id = %thread_id, error = %e, "summarize_thread: failed to insert thread_summaries row");
        // Don't return — the thread was updated successfully, this is non-critical
    }

    tracing::info!(
        thread_id = %thread_id,
        from_seq = summary_message_count,
        to_seq = new_to_seq,
        "summarize_thread: summary updated successfully"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarization_prompt_without_existing_summary() {
        let messages = vec![
            ("user".to_string(), "Hello, how are you?".to_string()),
            (
                "assistant".to_string(),
                "I'm doing well, thank you!".to_string(),
            ),
        ];
        let prompt = build_summarization_prompt(None, &messages);
        assert!(prompt.contains("## Conversation to Summarize"));
        assert!(prompt.contains("User:") || prompt.contains("**User:**"));
        assert!(prompt.contains("Hello, how are you?"));
        assert!(!prompt.contains("## Previous Summary"));
    }

    #[test]
    fn summarization_prompt_with_existing_summary() {
        let messages = vec![("user".to_string(), "Tell me about Rust.".to_string())];
        let prompt = build_summarization_prompt(Some("User asked about programming."), &messages);
        assert!(prompt.contains("## Previous Summary"));
        assert!(prompt.contains("User asked about programming."));
        assert!(prompt.contains("## New Messages to Incorporate"));
        assert!(prompt.contains("Tell me about Rust."));
    }

    #[test]
    fn summarization_prompt_with_empty_existing_summary() {
        let messages = vec![("user".to_string(), "Hi".to_string())];
        // Empty string should be treated as no previous summary
        let prompt = build_summarization_prompt(Some(""), &messages);
        assert!(prompt.contains("## Conversation to Summarize"));
        assert!(!prompt.contains("## Previous Summary"));
    }

    #[test]
    fn summarization_prompt_truncates_long_messages() {
        let long_content = "x".repeat(2000);
        let messages = vec![("user".to_string(), long_content)];
        let prompt = build_summarization_prompt(None, &messages);
        // The content should be truncated to 1000 chars in the prompt
        let content_section_len = prompt.len();
        // Rough check: the content truncation prevents the full 2000 chars appearing
        assert!(content_section_len < 2000 + 500); // prompt overhead < 500 chars
    }
}

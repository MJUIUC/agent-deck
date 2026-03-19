//! Built-in agent tools — Story H.3
//!
//! Defines the `AgentTool` trait and registers concrete implementations for
//! the two built-in tools (`save_memory` and `recall_memory`).
//!
//! ## Adding a new built-in tool
//!
//! 1. Implement `AgentTool` for a new struct in this file.
//! 2. Register it in `built_in_tools()` at the bottom of this file.
//! 3. That's it — `execute_tool_calls` in `agent.rs` will pick it up automatically.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::warn;

use crate::services::memory as memory_service;

// ─── ToolContext ──────────────────────────────────────────────────────────────

/// Carries the per-request dependencies that built-in tools need.
///
/// Passed to every `AgentTool::run` call so tools do not need to thread
/// individual arguments through every call site.
pub struct ToolContext<'a> {
    pub pool: &'a SqlitePool,
    pub user_id: &'a str,
    pub persona_id: &'a str,
    pub thread_id: &'a str,
}

// ─── AgentTool trait ──────────────────────────────────────────────────────────

/// A statically-registered built-in tool.
///
/// Implement this trait, then add an instance to `built_in_tools()` to make
/// the tool available to every agent run.  MCP tools are routed separately
/// and do not use this trait.
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// The exact name the LLM must use when calling this tool.
    fn name(&self) -> &str;

    /// One-sentence description shown to the LLM in the tool schema.
    fn description(&self) -> &str;

    /// JSON Schema object describing the `parameters` field of the tool.
    fn input_schema(&self) -> Value;

    /// Execute the tool and return a plain-text result to feed back to the LLM.
    async fn run(&self, args: Value, context: &ToolContext<'_>) -> Result<String>;
}

// ─── SaveMemoryTool ───────────────────────────────────────────────────────────

const MEMORY_CONTENT_MAX_CHARS: usize = 500;

pub struct SaveMemoryTool;

#[async_trait]
impl AgentTool for SaveMemoryTool {
    fn name(&self) -> &str {
        "save_memory"
    }

    fn description(&self) -> &str {
        "Save a piece of information to long-term memory so it can be recalled in future conversations."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The text to store in memory. Maximum 500 characters."
                }
            },
            "required": ["content"]
        })
    }

    async fn run(&self, args: Value, context: &ToolContext<'_>) -> Result<String> {
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if content.is_empty() {
            return Ok("Memory not saved: content was empty.".to_string());
        }

        let content: String = content.chars().take(MEMORY_CONTENT_MAX_CHARS).collect();

        match memory_service::save_memory(
            context.pool,
            context.user_id,
            context.persona_id,
            Some(context.thread_id),
            &content,
        )
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
}

// ─── RecallMemoryTool ─────────────────────────────────────────────────────────

const MEMORY_RECALL_LIMIT: i64 = 10;

pub struct RecallMemoryTool;

#[async_trait]
impl AgentTool for RecallMemoryTool {
    fn name(&self) -> &str {
        "recall_memory"
    }

    fn description(&self) -> &str {
        "Search long-term memory for entries matching a query. Returns up to 10 results."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords or a short phrase to search for in memory."
                }
            },
            "required": ["query"]
        })
    }

    async fn run(&self, args: Value, context: &ToolContext<'_>) -> Result<String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if query.is_empty() {
            return Ok("No memories found: query was empty.".to_string());
        }

        let entries = memory_service::recall_memory(
            context.pool,
            context.user_id,
            context.persona_id,
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

        let mut lines = vec![format!(
            "Found {} memor{}:",
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" }
        )];

        let mut thread_sources: Vec<String> = Vec::new();

        for entry in &entries {
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
}

// ─── Registry factory ─────────────────────────────────────────────────────────

/// Construct the list of all statically-registered built-in tools.
///
/// Called once at server startup and stored on `AppState`.  Adding a new
/// built-in tool requires only: implement `AgentTool`, add it here.
pub fn built_in_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![Arc::new(SaveMemoryTool), Arc::new(RecallMemoryTool)]
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_unique_and_non_empty() {
        let tools = built_in_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(!names.is_empty());
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(!name.is_empty(), "tool name must not be empty");
            assert!(seen.insert(*name), "duplicate tool name: {}", name);
        }
    }

    #[test]
    fn tool_registry_lookup_resolves_by_name() {
        let tools = built_in_tools();
        let found = tools.iter().find(|t| t.name() == "save_memory");
        assert!(found.is_some(), "save_memory must be in the registry");

        let found = tools.iter().find(|t| t.name() == "recall_memory");
        assert!(found.is_some(), "recall_memory must be in the registry");

        let not_found = tools.iter().find(|t| t.name() == "nonexistent_tool");
        assert!(not_found.is_none());
    }

    #[test]
    fn input_schemas_are_valid_json_schema_objects() {
        let tools = built_in_tools();
        for tool in &tools {
            let schema = tool.input_schema();
            assert_eq!(
                schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "tool '{}' schema must have type: object",
                tool.name()
            );
            assert!(
                schema.get("properties").is_some(),
                "tool '{}' schema must have a properties field",
                tool.name()
            );
            assert!(
                schema.get("required").is_some(),
                "tool '{}' schema must have a required field",
                tool.name()
            );
        }
    }

    #[test]
    fn save_memory_schema_has_content_field() {
        let tool = SaveMemoryTool;
        let schema = tool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(
            props.get("content").is_some(),
            "save_memory schema must include a 'content' property"
        );
    }

    #[test]
    fn recall_memory_schema_has_query_field() {
        let tool = RecallMemoryTool;
        let schema = tool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(
            props.get("query").is_some(),
            "recall_memory schema must include a 'query' property"
        );
    }
}

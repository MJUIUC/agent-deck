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
        "Search long-term memory for entries matching a query. Use 1–3 short keywords, not full sentences. Multiple keywords are OR-matched with prefix search, so 'dog name' returns any entry containing 'dog' or 'name'. Call this multiple times with different keywords if the first result is empty."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "1–3 keywords to search for. Short and specific — e.g. 'typescript', 'dog name', 'deadline march'. Do NOT use full sentences or questions."
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
            lines.push(format!(
                "- [id:{}] [{}] {}",
                entry.id, date_str, entry.content
            ));

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

// ─── DeleteMemoryTool ─────────────────────────────────────────────────────────

pub struct DeleteMemoryTool;

#[async_trait]
impl AgentTool for DeleteMemoryTool {
    fn name(&self) -> &str {
        "delete_memory"
    }

    fn description(&self) -> &str {
        "Delete a specific memory entry by its ID. Use this to remove duplicate, \
         outdated, or low-value memories. Always call recall_memory first to \
         obtain the memory ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "memory_id": {
                    "type": "string",
                    "description": "The ID of the memory to delete, as returned by recall_memory (the value after 'id:')."
                }
            },
            "required": ["memory_id"]
        })
    }

    async fn run(&self, args: Value, context: &ToolContext<'_>) -> Result<String> {
        let memory_id = args
            .get("memory_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if memory_id.is_empty() {
            return Ok("No memory_id provided.".to_string());
        }

        match memory_service::delete_memory(context.pool, &memory_id, context.user_id).await {
            Ok(true) => Ok("Memory deleted.".to_string()),
            Ok(false) => Ok(format!(
                "Memory '{}' not found — it may have already been deleted.",
                memory_id
            )),
            Err(e) => Err(e),
        }
    }
}

// ─── RecallConversationTool ───────────────────────────────────────────────────

const RECALL_CONVERSATION_LIMIT: i64 = 5;

pub struct RecallConversationTool;

#[async_trait]
impl AgentTool for RecallConversationTool {
    fn name(&self) -> &str {
        "recall_conversation"
    }

    fn description(&self) -> &str {
        "Look up summaries of past conversations by date range. Use this when the user asks \
         about something discussed in a previous conversation or references a specific time \
         period ('last week', 'back in March'). Returns narrative summaries of what was discussed. \
         Use recall_memory for facts and preferences — use this tool for conversational context \
         and history."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "from_date": {
                    "type": "string",
                    "description": "Start of the date range (ISO 8601, e.g. '2024-03-01'). Omit to search from the beginning."
                },
                "to_date": {
                    "type": "string",
                    "description": "End of the date range (ISO 8601, e.g. '2024-03-31'). Omit to search up to the present."
                },
                "keywords": {
                    "type": "string",
                    "description": "Optional keywords for substring filtering of summary text (case-insensitive)."
                }
            },
            "required": []
        })
    }

    async fn run(&self, args: Value, context: &ToolContext<'_>) -> Result<String> {
        let from_date = args
            .get("from_date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let to_date = args
            .get("to_date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let keywords = args
            .get("keywords")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Load the persona's cross-thread setting
        let cross_thread: bool = sqlx::query_as::<_, (bool,)>(
            "SELECT recall_conversation_cross_thread FROM agent_personas WHERE id = ?",
        )
        .bind(context.persona_id)
        .fetch_optional(context.pool)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
        .unwrap_or(true); // default: cross-thread enabled

        // Build query dynamically based on filters
        // Base: join thread_summaries with threads, filter by persona
        let mut conditions: Vec<String> = Vec::new();
        let mut bind_values: Vec<String> = Vec::new();

        conditions.push("t.persona_id = ?".to_string());
        bind_values.push(context.persona_id.to_string());

        if !cross_thread {
            conditions.push("ts.thread_id = ?".to_string());
            bind_values.push(context.thread_id.to_string());
        }

        if !from_date.is_empty() {
            conditions.push("ts.to_date >= ?".to_string());
            bind_values.push(from_date.clone());
        }

        if !to_date.is_empty() {
            conditions.push("ts.from_date <= ?".to_string());
            bind_values.push(to_date.clone());
        }

        if !keywords.is_empty() {
            conditions.push("LOWER(ts.summary) LIKE ?".to_string());
            bind_values.push(format!("%{}%", keywords.to_lowercase()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT ts.summary, ts.from_date, ts.to_date, t.title
             FROM thread_summaries ts
             JOIN threads t ON t.id = ts.thread_id
             {}
             ORDER BY ts.to_date DESC
             LIMIT {}",
            where_clause, RECALL_CONVERSATION_LIMIT
        );

        // Execute with dynamic bindings using a raw query
        let mut query = sqlx::query_as::<_, (String, String, String, String)>(&sql);
        for val in &bind_values {
            query = query.bind(val.as_str());
        }

        let rows = match query.fetch_all(context.pool).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "recall_conversation query failed");
                return Ok("No conversation summaries found for that period.".to_string());
            }
        };

        if rows.is_empty() {
            return Ok("No conversation summaries found for that period.".to_string());
        }

        let mut lines = Vec::new();
        for (summary, from_d, to_d, thread_title) in &rows {
            let from_short = from_d.split('T').next().unwrap_or(from_d);
            let to_short = to_d.split('T').next().unwrap_or(to_d);
            lines.push(format!(
                "[thread: {}] {} – {}\n{}",
                thread_title, from_short, to_short, summary
            ));
        }

        Ok(lines.join("\n\n---\n\n"))
    }
}

// ─── TailscaleStatusTool ──────────────────────────────────────────────────────

pub struct TailscaleStatusTool;

#[async_trait]
impl AgentTool for TailscaleStatusTool {
    fn name(&self) -> &str {
        "tailscale_status"
    }

    fn description(&self) -> &str {
        "Check the current Tailscale VPN and Funnel status for this agent-deck server. \
         Returns whether Tailscale is installed, connected, the device hostname, IP address, \
         version, and whether Funnel (public HTTPS access) is enabled. Use this when the user \
         asks about remote access, webhook configuration, or Tailscale connectivity."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn run(&self, _args: Value, _context: &ToolContext<'_>) -> Result<String> {
        let status = crate::services::tailscale::get_status(7474).await;

        let installed_str = if status.installed { "yes" } else { "no" };
        let connected_str = if status.connected { "yes" } else { "no" };
        let hostname_str = status.hostname.as_deref().unwrap_or("unknown");
        let ip_str = status.ip_address.as_deref().unwrap_or("unknown");
        let version_str = status.version.as_deref().unwrap_or("unknown");

        let funnel_str = if status.funnel_enabled {
            format!(
                "enabled — {}",
                status.funnel_url.as_deref().unwrap_or("URL unknown")
            )
        } else {
            "not enabled".to_string()
        };

        let mut lines = vec![
            "Tailscale status:".to_string(),
            format!("- Installed: {}", installed_str),
            format!("- Connected: {}", connected_str),
            format!("- Hostname: {}", hostname_str),
            format!("- IP: {}", ip_str),
            format!("- Version: {}", version_str),
            format!("- Funnel: {}", funnel_str),
        ];

        if status.funnel_enabled {
            if let Some(ref hostname) = status.hostname {
                lines.push(format!(
                    "- Webhook address: https://{}/api/webhooks",
                    hostname
                ));
            }
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
    vec![
        Arc::new(SaveMemoryTool),
        Arc::new(RecallMemoryTool),
        Arc::new(DeleteMemoryTool),
        Arc::new(RecallConversationTool),
        Arc::new(TailscaleStatusTool),
    ]
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

        let found = tools.iter().find(|t| t.name() == "delete_memory");
        assert!(found.is_some(), "delete_memory must be in the registry");

        let found = tools.iter().find(|t| t.name() == "recall_conversation");
        assert!(
            found.is_some(),
            "recall_conversation must be in the registry"
        );

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

    /// Content longer than 500 chars must be truncated to exactly 500 characters.
    #[tokio::test]
    async fn save_memory_truncates_content_at_500_chars() {
        // Build a pool with the minimal schema
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("pool");

        sqlx::query(
            "CREATE TABLE memory (
                id          TEXT PRIMARY KEY,
                user_id     TEXT NOT NULL,
                persona_id  TEXT NOT NULL,
                thread_id   TEXT,
                content     TEXT NOT NULL,
                created_at  TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE VIRTUAL TABLE memory_fts USING fts5(content, content=memory, content_rowid=rowid)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create a context with a long content value
        let long_content = "x".repeat(600);
        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };

        let tool = SaveMemoryTool;
        let args = serde_json::json!({ "content": long_content });
        let result = tool.run(args, &ctx).await.unwrap();
        assert_eq!(result, "Memory saved successfully.");

        // Verify stored content is exactly 500 chars
        let stored: (String,) = sqlx::query_as("SELECT content FROM memory WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            stored.0.chars().count(),
            500,
            "content must be truncated to 500 characters"
        );
    }

    /// Empty content returns an error message without saving anything.
    #[tokio::test]
    async fn save_memory_rejects_empty_content() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("pool");

        sqlx::query(
            "CREATE TABLE memory (
                id TEXT PRIMARY KEY, user_id TEXT NOT NULL, persona_id TEXT NOT NULL,
                thread_id TEXT, content TEXT NOT NULL, created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE VIRTUAL TABLE memory_fts USING fts5(content, content=memory, content_rowid=rowid)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };

        let tool = SaveMemoryTool;
        let result = tool
            .run(serde_json::json!({ "content": "" }), &ctx)
            .await
            .unwrap();
        assert!(result.contains("empty"), "must report empty content");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "nothing must be stored for empty content");
    }

    async fn make_tool_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE memory (
                id TEXT PRIMARY KEY, user_id TEXT NOT NULL, persona_id TEXT NOT NULL,
                thread_id TEXT, content TEXT NOT NULL, created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE VIRTUAL TABLE memory_fts USING fts5(content, content=memory, content_rowid=rowid)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// recall_memory tool returns the no-memories-found message for an empty store.
    #[tokio::test]
    async fn recall_memory_tool_returns_no_memories_message_for_empty_store() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("pool");

        sqlx::query(
            "CREATE TABLE memory (
                id TEXT PRIMARY KEY, user_id TEXT NOT NULL, persona_id TEXT NOT NULL,
                thread_id TEXT, content TEXT NOT NULL, created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '')")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "CREATE VIRTUAL TABLE memory_fts USING fts5(content, content=memory, content_rowid=rowid)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };

        let tool = RecallMemoryTool;
        let result = tool
            .run(serde_json::json!({ "query": "anything" }), &ctx)
            .await
            .unwrap();
        assert!(
            result.contains("No memories found"),
            "must return no-memories message, got: {}",
            result
        );
    }

    #[tokio::test]
    async fn delete_memory_tool_removes_existing_entry() {
        let pool = make_tool_pool().await;
        // Save a memory directly via the service
        crate::services::memory::save_memory(&pool, "u1", "p1", None, "User likes Rust")
            .await
            .unwrap();

        // Get its ID
        let (id,): (String,) = sqlx::query_as("SELECT id FROM memory WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();

        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };
        let result = DeleteMemoryTool
            .run(serde_json::json!({ "memory_id": id }), &ctx)
            .await
            .unwrap();
        assert_eq!(result, "Memory deleted.");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "memory must be gone after deletion");
    }

    #[tokio::test]
    async fn delete_memory_tool_returns_not_found_for_missing_id() {
        let pool = make_tool_pool().await;
        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };
        let result = DeleteMemoryTool
            .run(serde_json::json!({ "memory_id": "nonexistent-id" }), &ctx)
            .await
            .unwrap();
        assert!(result.contains("not found"), "got: {}", result);
    }

    #[tokio::test]
    async fn recall_memory_tool_output_includes_memory_id() {
        let pool = make_tool_pool().await;
        crate::services::memory::save_memory(&pool, "u1", "p1", None, "User prefers dark mode")
            .await
            .unwrap();

        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };
        let result = RecallMemoryTool
            .run(serde_json::json!({ "query": "dark mode" }), &ctx)
            .await
            .unwrap();

        assert!(
            result.contains("id:"),
            "recall output must include memory ID, got: {}",
            result
        );
        assert!(
            result.contains("dark mode"),
            "recall output must include content"
        );
    }

    #[tokio::test]
    async fn delete_memory_tool_rejects_empty_id() {
        let pool = make_tool_pool().await;
        let ctx = ToolContext {
            pool: &pool,
            user_id: "u1",
            persona_id: "p1",
            thread_id: "t1",
        };
        let result = DeleteMemoryTool
            .run(serde_json::json!({ "memory_id": "" }), &ctx)
            .await
            .unwrap();
        assert!(result.contains("No memory_id"), "got: {}", result);
    }
}

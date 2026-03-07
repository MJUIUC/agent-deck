//! Agent run-loop service.
//!
//! Responsible for:
//! - Assembling the context (system prompt + message history + memory recall)
//! - Calling the LLM provider via the provider abstraction layer
//! - Streaming tokens back to connected SSE clients
//! - Handling tool calls (memory save/recall, skills)
//! - Persisting the completed assistant message to the database

use anyhow::Result;
use sqlx::SqlitePool;

/// Placeholder state — will be fleshed out in Phase 2.
#[allow(dead_code)]
pub struct AgentService {
    pool: SqlitePool,
}

impl AgentService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Run the agent for a given thread and user message.
/// Returns the completed assistant message content.
///
/// # Stub
/// Full implementation lands in Phase 2. This function currently does nothing.
#[allow(dead_code)]
pub async fn run(_pool: &SqlitePool, _thread_id: &str, _user_message: &str) -> Result<String> {
    // Phase 2: assemble context, call provider, stream tokens, persist message
    Ok(String::new())
}

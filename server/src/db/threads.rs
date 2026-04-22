//! Repository functions for the `threads` table.
//!
//! All code that needs to SELECT a full [`Thread`] row must go through one of
//! these helpers. That way the column list exists in exactly one place: if a
//! migration adds or renames a column, you update the `COLUMNS` constant below
//! and the rest of the codebase compiles (or, if you missed it, fails to
//! compile) cleanly.

use sqlx::SqlitePool;

use crate::models::thread::Thread;

/// Every column returned by a `SELECT *`-style thread query, in struct order.
/// This is the single source of truth for the Thread SELECT column list.
const COLUMNS: &str = "id, user_id, persona_id, title, active_model, active_provider,
     system_prompt_addendum, status, show_tool_activity, show_system_events,
     summary, summary_updated_at, summary_message_count, auto_summarize,
     auto_retitle, created_at, updated_at";

/// Fetch a single thread by its ID (no user ownership check).
/// Used by internal services (agent runner, summarization) that already trust
/// the thread ID.
pub async fn fetch_by_id(pool: &SqlitePool, thread_id: &str) -> sqlx::Result<Option<Thread>> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM threads WHERE id = ?"))
        .bind(thread_id)
        .fetch_optional(pool)
        .await
}

/// Fetch a single thread by ID, enforcing that it belongs to `user_id`.
/// Used by route handlers that need both existence and ownership checks.
pub async fn fetch_by_id_and_user(
    pool: &SqlitePool,
    thread_id: &str,
    user_id: &str,
) -> sqlx::Result<Option<Thread>> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM threads WHERE id = ? AND user_id = ?"
    ))
    .bind(thread_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// List all threads for a user filtered by status, newest first.
pub async fn list_by_user_and_status(
    pool: &SqlitePool,
    user_id: &str,
    status: &str,
) -> sqlx::Result<Vec<Thread>> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM threads
         WHERE user_id = ? AND status = ?
         ORDER BY updated_at DESC"
    ))
    .bind(user_id)
    .bind(status)
    .fetch_all(pool)
    .await
}

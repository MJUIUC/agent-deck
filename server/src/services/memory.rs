//! Memory service.
//!
//! Responsible for:
//! - Saving memory entries scoped to a user + persona
//! - Recalling memory entries via FTS5 full-text search
//! - Enforcing the 500-entry cap per persona
//! - Deduplication helpers

// Full implementation lands in Phase 4.

use anyhow::Result;
use sqlx::SqlitePool;

use crate::models::memory::{Memory, MemoryEntry, MemoryListResponse};

/// Save a new memory entry for a persona.
/// Enforces the 500-entry cap — if at capacity, returns `Err` so the caller
/// can surface a "memory store is full" message to the model.
/// Does NOT silently evict old entries; the model is responsible for deciding
/// what to delete via the memory management UI or future tooling.
#[allow(dead_code)]
pub async fn save_memory(
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    thread_id: Option<&str>,
    content: &str,
) -> Result<Memory> {
    const MAX_MEMORIES: i64 = 500;

    // Enforce cap: count existing memories for this persona
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory WHERE user_id = ? AND persona_id = ?")
            .bind(user_id)
            .bind(persona_id)
            .fetch_one(pool)
            .await?;

    if count.0 >= MAX_MEMORIES {
        // Reject — cap is enforced here as the single source of truth.
        // The agent run-loop catches this error and returns a structured
        // "memory store is full" tool result to the model.
        return Err(anyhow::anyhow!(
            "Memory cap reached: {} entries already stored for this persona.",
            count.0
        ));
    }

    let memory = Memory::new_for_thread(user_id, persona_id, thread_id.unwrap_or(""), content);

    // Fix: use None for thread_id when empty string
    let tid = if thread_id.map(|s| s.is_empty()).unwrap_or(true) {
        None::<String>
    } else {
        thread_id.map(|s| s.to_string())
    };

    sqlx::query(
        "INSERT INTO memory (id, user_id, persona_id, thread_id, content, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&memory.id)
    .bind(&memory.user_id)
    .bind(&memory.persona_id)
    .bind(&tid)
    .bind(&memory.content)
    .bind(&memory.created_at)
    .execute(pool)
    .await?;

    // Keep FTS index in sync
    sqlx::query(
        "INSERT INTO memory_fts(rowid, content) SELECT rowid, content FROM memory WHERE id = ?",
    )
    .bind(&memory.id)
    .execute(pool)
    .await?;

    Ok(Memory {
        thread_id: tid,
        ..memory
    })
}

/// Recall memories using FTS5 full-text search.
/// Results are returned in relevance order (FTS rank).
#[allow(dead_code)]
pub async fn recall_memory(
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<MemoryEntry>> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
        "SELECT m.id, m.content, m.thread_id, t.title, m.created_at
         FROM memory m
         JOIN memory_fts fts ON fts.rowid = m.rowid
         LEFT JOIN threads t ON t.id = m.thread_id
         WHERE fts.content MATCH ?
           AND m.user_id = ?
           AND m.persona_id = ?
         ORDER BY rank
         LIMIT ?",
    )
    .bind(query)
    .bind(user_id)
    .bind(persona_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, content, thread_id, thread_title, created_at)| MemoryEntry {
                id,
                content,
                thread_id,
                thread_title,
                created_at,
            },
        )
        .collect())
}

/// List all memories for a persona, most recent first.
#[allow(dead_code)]
pub async fn list_memories(
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    offset: i64,
    limit: i64,
) -> Result<MemoryListResponse> {
    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory WHERE user_id = ? AND persona_id = ?")
            .bind(user_id)
            .bind(persona_id)
            .fetch_one(pool)
            .await?;

    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
        "SELECT m.id, m.content, m.thread_id, t.title, m.created_at
         FROM memory m
         LEFT JOIN threads t ON t.id = m.thread_id
         WHERE m.user_id = ? AND m.persona_id = ?
         ORDER BY m.created_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(persona_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let memories = rows
        .into_iter()
        .map(
            |(id, content, thread_id, thread_title, created_at)| MemoryEntry {
                id,
                content,
                thread_id,
                thread_title,
                created_at,
            },
        )
        .collect();

    Ok(MemoryListResponse {
        memories,
        total_count: total.0,
    })
}

/// Delete a single memory entry by ID.
#[allow(dead_code)]
pub async fn delete_memory(pool: &SqlitePool, memory_id: &str, user_id: &str) -> Result<bool> {
    // First get the rowid for FTS cleanup
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT rowid FROM memory WHERE id = ? AND user_id = ?")
            .bind(memory_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    let Some((rowid,)) = row else {
        return Ok(false);
    };

    // Remove from FTS index
    sqlx::query("DELETE FROM memory_fts WHERE rowid = ?")
        .bind(rowid)
        .execute(pool)
        .await?;

    // Remove from main table
    let result = sqlx::query("DELETE FROM memory WHERE id = ? AND user_id = ?")
        .bind(memory_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Create an in-memory SQLite pool with the minimal schema needed for
    /// memory service tests.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

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
        .expect("create memory table");

        // memory_fts is referenced by save_memory — create a stub that accepts
        // the same INSERT … SELECT pattern.
        sqlx::query(
            "CREATE VIRTUAL TABLE memory_fts USING fts5(content, content=memory, content_rowid=rowid)",
        )
        .execute(&pool)
        .await
        .expect("create memory_fts table");

        pool
    }

    /// Saving up to 499 entries must succeed; the 500th must also succeed;
    /// the 501st must return an `Err` containing "Memory cap reached" and must
    /// NOT silently evict an existing entry.
    #[tokio::test]
    async fn save_memory_rejects_at_cap_without_eviction() {
        let pool = test_pool().await;
        let user_id = "user-1";
        let persona_id = "persona-1";

        // Fill the store to exactly the cap.
        for i in 0..500_i64 {
            save_memory(
                &pool,
                user_id,
                persona_id,
                None,
                &format!("memory entry {}", i),
            )
            .await
            .unwrap_or_else(|e| panic!("unexpected error at entry {}: {}", i, e));
        }

        // Confirm exactly 500 entries are stored.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memory WHERE user_id = ? AND persona_id = ?")
                .bind(user_id)
                .bind(persona_id)
                .fetch_one(&pool)
                .await
                .expect("count query");
        assert_eq!(
            count.0, 500,
            "should have exactly 500 entries after filling"
        );

        // The 501st save must fail with the cap error.
        let result = save_memory(&pool, user_id, persona_id, None, "one too many").await;
        assert!(result.is_err(), "save_memory should return Err when at cap");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Memory cap reached"),
            "error should mention 'Memory cap reached', got: {}",
            err_msg
        );

        // The store must still have exactly 500 entries — no eviction occurred.
        let count_after: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memory WHERE user_id = ? AND persona_id = ?")
                .bind(user_id)
                .bind(persona_id)
                .fetch_one(&pool)
                .await
                .expect("count query after cap");
        assert_eq!(
            count_after.0, 500,
            "entry count must not change when cap is reached (no eviction)"
        );
    }
}

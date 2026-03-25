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
/// Transform a raw user/model query string into a FTS5 OR expression with
/// prefix matching on each token.
///
/// Rules:
/// - If the query already contains explicit FTS5 operators (OR, AND, NOT, `"`,
///   `*`, `-`) it is returned unchanged so callers can use advanced syntax.
/// - Otherwise each whitespace-separated token is suffixed with `*` (prefix
///   match) and joined with ` OR `.  This means a multi-word query like
///   "dog name" becomes `dog* OR name*`, which surfaces any entry that
///   contains *either* word rather than requiring all of them.
///
/// Examples:
///   "typescript" → "typescript*"
///   "dog name"   → "dog* OR name*"
///   "user OR pet" → "user OR pet"   (unchanged — already has operator)
fn build_fts_query(query: &str) -> String {
    // Detect explicit FTS5 operator usage — leave those alone.
    let has_operators = query.contains(" OR ")
        || query.contains(" AND ")
        || query.contains(" NOT ")
        || query.contains('"')
        || query.contains('*')
        || query.contains('-');

    if has_operators {
        return query.to_string();
    }

    // Split on whitespace, drop empty tokens, append `*` to each.
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("{}*", t))
        .collect();

    if terms.is_empty() {
        return query.to_string();
    }

    terms.join(" OR ")
}

/// Results are returned in relevance order (FTS rank).
#[allow(dead_code)]
pub async fn recall_memory(
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<MemoryEntry>> {
    let fts_query = build_fts_query(query);

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
    .bind(fts_query)
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
/// When `thread_id` is `Some`, results are filtered to that thread only.
#[allow(dead_code)]
pub async fn list_memories(
    pool: &SqlitePool,
    user_id: &str,
    persona_id: &str,
    offset: i64,
    limit: i64,
    thread_id: Option<&str>,
) -> Result<MemoryListResponse> {
    if let Some(tid) = thread_id {
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memory WHERE user_id = ? AND persona_id = ? AND thread_id = ?",
        )
        .bind(user_id)
        .bind(persona_id)
        .bind(tid)
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
            "SELECT m.id, m.content, m.thread_id, t.title, m.created_at
             FROM memory m
             LEFT JOIN threads t ON t.id = m.thread_id
             WHERE m.user_id = ? AND m.persona_id = ? AND m.thread_id = ?
             ORDER BY m.created_at DESC
             LIMIT ? OFFSET ?",
        )
        .bind(user_id)
        .bind(persona_id)
        .bind(tid)
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
    } else {
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

        // threads is LEFT JOINed by recall_memory — create a minimal stub so
        // the query does not fail with "no such table".
        sqlx::query(
            "CREATE TABLE threads (
                id     TEXT PRIMARY KEY,
                title  TEXT NOT NULL DEFAULT ''
            )",
        )
        .execute(&pool)
        .await
        .expect("create threads stub");

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

    // ── build_fts_query unit tests ────────────────────────────────────────────

    #[test]
    fn single_keyword_gets_prefix_wildcard() {
        assert_eq!(build_fts_query("typescript"), "typescript*");
    }

    #[test]
    fn multi_word_becomes_or_with_wildcards() {
        assert_eq!(build_fts_query("dog name"), "dog* OR name*");
    }

    #[test]
    fn three_words_become_or_chain() {
        assert_eq!(
            build_fts_query("deadline march project"),
            "deadline* OR march* OR project*"
        );
    }

    #[test]
    fn explicit_or_operator_passes_through_unchanged() {
        assert_eq!(build_fts_query("user OR pet"), "user OR pet");
    }

    #[test]
    fn explicit_and_operator_passes_through_unchanged() {
        assert_eq!(
            build_fts_query("user AND typescript"),
            "user AND typescript"
        );
    }

    #[test]
    fn quoted_phrase_passes_through_unchanged() {
        assert_eq!(
            build_fts_query("\"prefers typescript\""),
            "\"prefers typescript\""
        );
    }

    #[test]
    fn existing_wildcard_passes_through_unchanged() {
        assert_eq!(build_fts_query("type*"), "type*");
    }

    #[test]
    fn extra_whitespace_is_collapsed() {
        assert_eq!(build_fts_query("dog  name"), "dog* OR name*");
    }

    /// FTS search must return entries whose content matches the query.
    #[tokio::test]
    async fn recall_returns_matching_entries() {
        let pool = test_pool().await;
        save_memory(
            &pool,
            "u1",
            "p1",
            None,
            "User prefers TypeScript over JavaScript",
        )
        .await
        .unwrap();
        save_memory(&pool, "u1", "p1", None, "User's dog is named Pepper")
            .await
            .unwrap();
        save_memory(
            &pool,
            "u1",
            "p1",
            None,
            "Project Atlas deadline is March 15 2025",
        )
        .await
        .unwrap();

        let results = recall_memory(&pool, "u1", "p1", "TypeScript", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("TypeScript"));
    }

    /// Memories are scoped to persona — a different persona must not see them.
    #[tokio::test]
    async fn recall_is_scoped_to_persona() {
        let pool = test_pool().await;
        save_memory(&pool, "u1", "persona-A", None, "User likes cats")
            .await
            .unwrap();

        // Same user, different persona — must return nothing.
        let results = recall_memory(&pool, "u1", "persona-B", "cats", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "memories must not cross persona boundaries"
        );
    }

    /// A memory saved in thread-1 must be recallable from a query in thread-2
    /// (same user + persona — provenance does not restrict access).
    #[tokio::test]
    async fn memories_cross_thread_within_same_persona() {
        let pool = test_pool().await;
        save_memory(
            &pool,
            "u1",
            "p1",
            Some("thread-1"),
            "User timezone is US Pacific",
        )
        .await
        .unwrap();

        // Recall without specifying thread — simulates a query from thread-2.
        let results = recall_memory(&pool, "u1", "p1", "timezone", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].thread_id.as_deref(), Some("thread-1"));
    }

    /// Recall results are capped at the requested limit.
    #[tokio::test]
    async fn recall_capped_at_requested_limit() {
        let pool = test_pool().await;
        for i in 0..15_i64 {
            save_memory(&pool, "u1", "p1", None, &format!("fact number {}", i))
                .await
                .unwrap();
        }
        let results = recall_memory(&pool, "u1", "p1", "fact", 10).await.unwrap();
        assert!(
            results.len() <= 10,
            "recall must be capped at the requested limit"
        );
    }

    /// Empty recall must return an empty vec (not an error).
    #[tokio::test]
    async fn recall_empty_result_is_ok() {
        let pool = test_pool().await;
        let results = recall_memory(&pool, "u1", "p1", "nonexistent_keyword_xyz", 10)
            .await
            .unwrap();
        assert!(results.is_empty(), "empty recall must return Ok(empty vec)");
    }
}

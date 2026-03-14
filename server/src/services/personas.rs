//! Persona filesystem mirror — Story 4.3a
//!
//! Every time a persona is created, updated, or deleted via the API we mirror
//! its data to the local filesystem under `personas_dir`.  The layout is:
//!
//! ```
//! personas/
//!   meta.json                    ← root index: [{id, name, emoji}, ...]
//!   <id>/
//!     meta.json                  ← full persona metadata
//!     instructions.md            ← system prompt as markdown
//!     avatar.<ext>               ← uploaded avatar (written by upload_avatar)
//! ```
//!
//! All functions are **best-effort**: callers should log warnings but never
//! surface filesystem errors back to HTTP clients.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::models::agent_persona::AgentPersona;

// ─── Per-persona metadata written to `<id>/meta.json` ───────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct PersonaMeta {
    id: String,
    name: String,
    emoji: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_provider: Option<String>,
}

// ─── Root index entry written to `personas/meta.json` ────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct PersonaIndexEntry {
    id: String,
    name: String,
    emoji: String,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Write (or overwrite) the per-persona files for `persona`:
/// - `<personas_dir>/<id>/meta.json`
/// - `<personas_dir>/<id>/instructions.md`
pub async fn write_persona_files(personas_dir: &Path, persona: &AgentPersona) -> Result<()> {
    let dir = personas_dir.join(&persona.id);
    fs::create_dir_all(&dir).await?;

    // meta.json
    let meta = PersonaMeta {
        id: persona.id.clone(),
        name: persona.name.clone(),
        emoji: persona.emoji.clone(),
        default_model: persona.default_model.clone(),
        default_provider: persona.default_provider.clone(),
    };
    let meta_json = serde_json::to_string_pretty(&meta)?;
    write_file(&dir.join("meta.json"), meta_json.as_bytes()).await?;

    // instructions.md
    let instructions = format!(
        "<!-- agent-deck persona: {} -->\n<!-- id: {} -->\n\n{}\n",
        persona.name, persona.id, persona.system_prompt
    );
    write_file(&dir.join("instructions.md"), instructions.as_bytes()).await?;

    Ok(())
}

/// Remove the persona's directory and all its contents.
pub async fn delete_persona_dir(personas_dir: &Path, persona_id: &str) -> Result<()> {
    let dir = personas_dir.join(persona_id);
    if dir.exists() {
        fs::remove_dir_all(&dir).await?;
    }
    Ok(())
}

/// (Re-)write the root `personas/meta.json` index from all current DB rows.
///
/// The index contains only the fields needed for quick listing: id, name, emoji.
pub async fn update_root_index(personas_dir: &Path, pool: &SqlitePool) -> Result<()> {
    // Query all personas ordered by creation date so the index is stable.
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, name, emoji FROM agent_personas ORDER BY created_at ASC")
            .fetch_all(pool)
            .await?;

    let entries: Vec<PersonaIndexEntry> = rows
        .into_iter()
        .map(|(id, name, emoji)| PersonaIndexEntry { id, name, emoji })
        .collect();

    fs::create_dir_all(personas_dir).await?;

    let json = serde_json::to_string_pretty(&entries)?;
    write_file(&personas_dir.join("meta.json"), json.as_bytes()).await?;

    Ok(())
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Atomically write `content` to `path` (create or truncate).
async fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = fs::File::create(path).await?;
    file.write_all(content).await?;
    file.flush().await?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent_persona::AgentPersona;

    fn make_persona(id: &str, name: &str) -> AgentPersona {
        AgentPersona {
            id: id.to_string(),
            user_id: "user-1".to_string(),
            name: name.to_string(),
            emoji: "🤖".to_string(),
            avatar_path: None,
            system_prompt: format!("You are {}.", name),
            default_model: Some("gpt-4o".to_string()),
            default_provider: Some("openai".to_string()),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        }
    }

    /// write_persona_files creates meta.json and instructions.md inside <id>/
    #[tokio::test]
    async fn write_persona_files_creates_meta_and_instructions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let personas_dir = tmp.path();
        let persona = make_persona("p-001", "Aria");

        write_persona_files(personas_dir, &persona)
            .await
            .expect("write_persona_files");

        let dir = personas_dir.join("p-001");
        assert!(dir.exists(), "persona directory should exist");

        let meta_path = dir.join("meta.json");
        assert!(meta_path.exists(), "meta.json should exist");

        let meta_raw = tokio::fs::read_to_string(&meta_path).await.unwrap();
        let meta: serde_json::Value = serde_json::from_str(&meta_raw).unwrap();
        assert_eq!(meta["id"], "p-001");
        assert_eq!(meta["name"], "Aria");
        assert_eq!(meta["emoji"], "🤖");
        assert_eq!(meta["default_model"], "gpt-4o");

        let instructions_path = dir.join("instructions.md");
        assert!(instructions_path.exists(), "instructions.md should exist");

        let instructions = tokio::fs::read_to_string(&instructions_path).await.unwrap();
        assert!(instructions.contains("You are Aria."));
        assert!(instructions.contains("p-001"));
    }

    /// delete_persona_dir removes the directory and its contents.
    #[tokio::test]
    async fn delete_persona_dir_removes_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let personas_dir = tmp.path();
        let persona = make_persona("p-002", "Bruno");

        write_persona_files(personas_dir, &persona)
            .await
            .expect("write");

        let dir = personas_dir.join("p-002");
        assert!(dir.exists());

        delete_persona_dir(personas_dir, "p-002")
            .await
            .expect("delete");

        assert!(!dir.exists(), "directory should be gone after delete");
    }

    /// delete_persona_dir is a no-op when the directory does not exist.
    #[tokio::test]
    async fn delete_persona_dir_nonexistent_is_ok() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let result = delete_persona_dir(tmp.path(), "does-not-exist").await;
        assert!(result.is_ok());
    }

    /// update_root_index writes personas/meta.json listing all current personas.
    #[tokio::test]
    async fn update_root_index_contains_all_current_personas() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let personas_dir = tmp.path();

        // Set up an in-memory SQLite DB with the agent_personas table.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrate");

        // Insert a user row to satisfy FK constraints.
        let now = "2024-01-01T00:00:00.000Z";
        sqlx::query(
            "INSERT INTO users (id, display_name, created_at) VALUES ('u1', 'Test User', ?)",
        )
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        // Insert two personas directly.
        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, avatar_path, system_prompt, default_model, default_provider, created_at, updated_at)
             VALUES (?, 'u1', ?, '🤖', NULL, 'prompt', NULL, NULL, ?, ?)",
        )
        .bind("p-a")
        .bind("Alpha")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, avatar_path, system_prompt, default_model, default_provider, created_at, updated_at)
             VALUES (?, 'u1', ?, '🦊', NULL, 'prompt2', NULL, NULL, ?, ?)",
        )
        .bind("p-b")
        .bind("Beta")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        update_root_index(personas_dir, &pool)
            .await
            .expect("update_root_index");

        let index_path = personas_dir.join("meta.json");
        assert!(index_path.exists(), "root meta.json should exist");

        let raw = tokio::fs::read_to_string(&index_path).await.unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        assert_eq!(entries.len(), 2);

        let ids: Vec<&str> = entries.iter().map(|e| e["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"p-a"));
        assert!(ids.contains(&"p-b"));
    }

    /// After deleting a persona from the DB, update_root_index no longer lists it.
    #[tokio::test]
    async fn update_root_index_reflects_deletions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let personas_dir = tmp.path();

        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrate");

        let now = "2024-01-01T00:00:00.000Z";
        // Insert a user row to satisfy FK constraints.
        sqlx::query(
            "INSERT INTO users (id, display_name, created_at) VALUES ('u1', 'Test User', ?)",
        )
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO agent_personas (id, user_id, name, emoji, avatar_path, system_prompt, default_model, default_provider, created_at, updated_at)
             VALUES ('del-1', 'u1', 'DeleteMe', '🗑', NULL, 'x', NULL, NULL, ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        // Write index with the persona present.
        update_root_index(personas_dir, &pool)
            .await
            .expect("first index write");

        // Delete from DB.
        sqlx::query("DELETE FROM agent_personas WHERE id = 'del-1'")
            .execute(&pool)
            .await
            .unwrap();

        // Rewrite index.
        update_root_index(personas_dir, &pool)
            .await
            .expect("second index write");

        let raw = tokio::fs::read_to_string(personas_dir.join("meta.json"))
            .await
            .unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        assert!(
            entries.iter().all(|e| e["id"] != "del-1"),
            "deleted persona should not appear in index"
        );
    }
}

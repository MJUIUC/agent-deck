//! Skills service.
//!
//! Responsible for:
//! - Loading skill definitions attached to a thread
//! - Converting skill definitions into OpenAI-compatible tool call schemas
//! - Executing skill instructions when the agent invokes a skill tool
//! - Returning skill results back into the agent context

// Full implementation lands in Phase 7.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::models::skill::Skill;

/// An OpenAI-compatible tool definition derived from a Skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTool {
    pub r#type: String,
    pub function: SkillFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl From<&Skill> for SkillTool {
    fn from(skill: &Skill) -> Self {
        Self {
            r#type: "function".to_string(),
            function: SkillFunction {
                name: skill.name.clone(),
                description: skill.description.clone(),
                // Skills don't take structured parameters — they run their
                // instructions as a self-contained prompt. The model invokes
                // a skill by name; no arguments are required.
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }
}

/// Load all enabled skills attached to a thread.
#[allow(dead_code)]
pub async fn load_thread_skills(pool: &SqlitePool, thread_id: &str) -> Result<Vec<Skill>> {
    let skills = sqlx::query_as::<_, Skill>(
        "SELECT s.id, s.user_id, s.name, s.display_name, s.description, s.instructions,
                s.enabled, s.created_at, s.updated_at
         FROM skills s
         JOIN thread_skills ts ON ts.skill_id = s.id
         WHERE ts.thread_id = ?
           AND ts.enabled = 1
           AND s.enabled = 1",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;

    Ok(skills)
}

/// Convert a list of skills into OpenAI-compatible tool definitions
/// ready to be passed to the LLM.
#[allow(dead_code)]
pub fn skills_to_tools(skills: &[Skill]) -> Vec<SkillTool> {
    skills.iter().map(SkillTool::from).collect()
}

/// Execute a skill by name for a given thread.
/// Looks up the skill's instructions and returns them as the tool result
/// so the agent can incorporate the skill output into its response.
///
/// # Stub
/// Full execution logic (sub-agent call, structured output, etc.) lands in Phase 7.
#[allow(dead_code)]
pub async fn execute_skill(pool: &SqlitePool, thread_id: &str, skill_name: &str) -> Result<String> {
    let skill: Option<Skill> = sqlx::query_as(
        "SELECT s.id, s.user_id, s.name, s.display_name, s.description, s.instructions,
                s.enabled, s.created_at, s.updated_at
         FROM skills s
         JOIN thread_skills ts ON ts.skill_id = s.id
         WHERE ts.thread_id = ?
           AND s.name = ?
           AND ts.enabled = 1
           AND s.enabled = 1",
    )
    .bind(thread_id)
    .bind(skill_name)
    .fetch_optional(pool)
    .await?;

    match skill {
        Some(s) => Ok(s.instructions),
        None => Ok(format!(
            "Skill '{}' not found or not enabled for this thread.",
            skill_name
        )),
    }
}

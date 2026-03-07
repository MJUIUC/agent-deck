use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::skill::{CreateSkill, Skill, UpdateSkill},
    routes::AppState,
};

/// Helper: get the single user id from the DB.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// GET /api/skills
pub async fn list(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let skills: Vec<Skill> = sqlx::query_as(
        "SELECT id, user_id, name, display_name, description, instructions, enabled, created_at, updated_at
         FROM skills
         WHERE user_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": skills }))))
}

/// GET /api/skills/:id
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let skill: Option<Skill> = sqlx::query_as(
        "SELECT id, user_id, name, display_name, description, instructions, enabled, created_at, updated_at
         FROM skills
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    match skill {
        Some(s) => Ok((StatusCode::OK, Json(json!({ "data": s })))),
        None => Err(AppError::NotFound(format!("Skill '{}' not found", id))),
    }
}

/// POST /api/skills
pub async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreateSkill>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }
    if payload.display_name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "display_name must not be empty".to_string(),
        ));
    }
    if payload.description.trim().is_empty() {
        return Err(AppError::BadRequest(
            "description must not be empty".to_string(),
        ));
    }

    // Validate slug format: lowercase letters, numbers, hyphens only
    if !payload
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::BadRequest(
            "name must be a slug: lowercase letters, numbers, and hyphens only".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;

    // Check for duplicate slug within this user's skills
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM skills WHERE user_id = ? AND name = ?")
            .bind(&user_id)
            .bind(&payload.name)
            .fetch_optional(&state.pool)
            .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "A skill with the name '{}' already exists",
            payload.name
        )));
    }

    let skill = Skill::new(&user_id, payload);

    sqlx::query(
        "INSERT INTO skills (id, user_id, name, display_name, description, instructions, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&skill.id)
    .bind(&skill.user_id)
    .bind(&skill.name)
    .bind(&skill.display_name)
    .bind(&skill.description)
    .bind(&skill.instructions)
    .bind(skill.enabled)
    .bind(&skill.created_at)
    .bind(&skill.updated_at)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "data": skill }))))
}

/// PUT /api/skills/:id
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateSkill>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let existing: Option<Skill> = sqlx::query_as(
        "SELECT id, user_id, name, display_name, description, instructions, enabled, created_at, updated_at
         FROM skills
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(s) => s,
        None => return Err(AppError::NotFound(format!("Skill '{}' not found", id))),
    };

    // Validate new slug if provided
    if let Some(ref new_name) = payload.name {
        if !new_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(AppError::BadRequest(
                "name must be a slug: lowercase letters, numbers, and hyphens only".to_string(),
            ));
        }

        // Check for duplicate slug (exclude current skill)
        let conflict: Option<(String,)> =
            sqlx::query_as("SELECT id FROM skills WHERE user_id = ? AND name = ? AND id != ?")
                .bind(&user_id)
                .bind(new_name)
                .bind(&id)
                .fetch_optional(&state.pool)
                .await?;

        if conflict.is_some() {
            return Err(AppError::Conflict(format!(
                "A skill with the name '{}' already exists",
                new_name
            )));
        }
    }

    let name = payload.name.as_deref().unwrap_or(&existing.name);
    let display_name = payload
        .display_name
        .as_deref()
        .unwrap_or(&existing.display_name);
    let description = payload
        .description
        .as_deref()
        .unwrap_or(&existing.description);
    let instructions = payload
        .instructions
        .as_deref()
        .unwrap_or(&existing.instructions);
    let enabled = payload.enabled.unwrap_or(existing.enabled);

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE skills
         SET name = ?, display_name = ?, description = ?, instructions = ?, enabled = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(instructions)
    .bind(enabled)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    let updated = Skill {
        id: existing.id,
        user_id: existing.user_id,
        name: name.to_string(),
        display_name: display_name.to_string(),
        description: description.to_string(),
        instructions: instructions.to_string(),
        enabled,
        created_at: existing.created_at,
        updated_at: now,
    };

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/skills/:id
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM skills WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Skill '{}' not found", id)));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

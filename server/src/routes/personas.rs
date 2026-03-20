use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::{
    error::{AppError, AppResult},
    models::agent_persona::{AgentPersona, CreateAgentPersona, UpdateAgentPersona},
    routes::AppState,
    services::personas as personas_service,
};

/// Helper: get the single user id from the DB.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// GET /api/personas
pub async fn list(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let personas: Vec<AgentPersona> = sqlx::query_as(
        "SELECT id, user_id, name, emoji, avatar_path, system_prompt,
                default_model, default_provider, created_at, updated_at
         FROM agent_personas
         WHERE user_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": personas }))))
}

/// GET /api/personas/:id
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let persona: Option<AgentPersona> = sqlx::query_as(
        "SELECT id, user_id, name, emoji, avatar_path, system_prompt,
                default_model, default_provider, created_at, updated_at
         FROM agent_personas
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    match persona {
        Some(p) => Ok((StatusCode::OK, Json(json!({ "data": p })))),
        None => Err(AppError::NotFound(format!("Persona '{}' not found", id))),
    }
}

/// POST /api/personas
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateAgentPersona>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }
    if payload.emoji.trim().is_empty() {
        return Err(AppError::BadRequest("emoji must not be empty".to_string()));
    }
    if payload.system_prompt.trim().is_empty() {
        return Err(AppError::BadRequest(
            "system_prompt must not be empty".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;
    let persona = AgentPersona::new(&user_id, payload);

    sqlx::query(
        "INSERT INTO agent_personas
             (id, user_id, name, emoji, avatar_path, system_prompt,
              default_model, default_provider, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&persona.id)
    .bind(&persona.user_id)
    .bind(&persona.name)
    .bind(&persona.emoji)
    .bind(&persona.avatar_path)
    .bind(&persona.system_prompt)
    .bind(&persona.default_model)
    .bind(&persona.default_provider)
    .bind(&persona.created_at)
    .bind(&persona.updated_at)
    .execute(&state.pool)
    .await?;

    // Mirror to filesystem (best-effort).
    if let Err(e) =
        personas_service::write_persona_files(&state.config.personas_dir, &persona).await
    {
        tracing::warn!(
            "personas: failed to write persona files for '{}': {}",
            persona.id,
            e
        );
    }
    if let Err(e) =
        personas_service::update_root_index(&state.config.personas_dir, &state.pool).await
    {
        tracing::warn!("personas: failed to update root index after create: {}", e);
    }

    Ok((StatusCode::CREATED, Json(json!({ "data": persona }))))
}

/// PUT /api/personas/:id
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateAgentPersona>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let existing: Option<AgentPersona> = sqlx::query_as(
        "SELECT id, user_id, name, emoji, avatar_path, system_prompt,
                default_model, default_provider, created_at, updated_at
         FROM agent_personas
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(p) => p,
        None => return Err(AppError::NotFound(format!("Persona '{}' not found", id))),
    };

    let name = payload.name.as_deref().unwrap_or(&existing.name);
    let emoji = payload.emoji.as_deref().unwrap_or(&existing.emoji);
    let system_prompt = payload
        .system_prompt
        .as_deref()
        .unwrap_or(&existing.system_prompt);

    // For optional FK fields, we need special handling: the update payload may
    // explicitly set them to null (clearing) or leave them unchanged (None in the
    // Rust Option sense means "not provided"). We use a sentinel pattern here —
    // if the field is present in the JSON (even as null), we update it.
    // Since serde deserializes missing fields as None and explicit null also as None,
    // we use `Option<Option<T>>` (double-Option) via a custom helper below.
    // For simplicity in Phase 1, if the field is Some(_) we update, None means unchanged.
    let default_model = match &payload.default_model {
        Some(v) => Some(v.as_str()),
        None => existing.default_model.as_deref(),
    };
    let default_provider = match &payload.default_provider {
        Some(v) => Some(v.as_str()),
        None => existing.default_provider.as_deref(),
    };

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE agent_personas
         SET name = ?, emoji = ?, system_prompt = ?,
             default_model = ?, default_provider = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(name)
    .bind(emoji)
    .bind(system_prompt)
    .bind(default_model)
    .bind(default_provider)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    let updated = AgentPersona {
        id: existing.id,
        user_id: existing.user_id,
        name: name.to_string(),
        emoji: emoji.to_string(),
        avatar_path: existing.avatar_path,
        system_prompt: system_prompt.to_string(),
        default_model: default_model.map(str::to_string),
        default_provider: default_provider.map(str::to_string),
        created_at: existing.created_at,
        updated_at: now,
    };

    // Mirror to filesystem (best-effort).
    if let Err(e) =
        personas_service::write_persona_files(&state.config.personas_dir, &updated).await
    {
        tracing::warn!(
            "personas: failed to write persona files for '{}': {}",
            updated.id,
            e
        );
    }
    if let Err(e) =
        personas_service::update_root_index(&state.config.personas_dir, &state.pool).await
    {
        tracing::warn!("personas: failed to update root index after update: {}", e);
    }

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/personas/:id
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM agent_personas WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Persona '{}' not found", id)));
    }

    // Mirror deletion to filesystem (best-effort).
    if let Err(e) = personas_service::delete_persona_dir(&state.config.personas_dir, &id).await {
        tracing::warn!("personas: failed to delete persona dir for '{}': {}", id, e);
    }
    if let Err(e) =
        personas_service::update_root_index(&state.config.personas_dir, &state.pool).await
    {
        tracing::warn!("personas: failed to update root index after delete: {}", e);
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

/// POST /api/personas/:id/avatar
///
/// Accepts a multipart upload containing an image file.
/// Stores it in `data/avatars/<persona_id>.<ext>` and updates the persona's
/// `avatar_path` field. Returns a serveable URL for the uploaded avatar.
pub async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    // Verify the persona exists and belongs to this user
    let persona: Option<AgentPersona> = sqlx::query_as(
        "SELECT id, user_id, name, emoji, avatar_path, system_prompt,
                default_model, default_provider, created_at, updated_at
         FROM agent_personas
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    if persona.is_none() {
        return Err(AppError::NotFound(format!("Persona '{}' not found", id)));
    }

    // Parse the first file field from the multipart form
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
        .ok_or_else(|| AppError::BadRequest("No file provided in multipart form".to_string()))?;

    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    // Only allow image types
    let ext = match content_type.as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        other => {
            return Err(AppError::BadRequest(format!(
                "Unsupported content type '{}'. Allowed: jpeg, png, gif, webp",
                other
            )))
        }
    };

    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read file bytes: {}", e)))?;

    if bytes.is_empty() {
        return Err(AppError::BadRequest("Uploaded file is empty".to_string()));
    }

    // Limit avatar size to 5 MB
    const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err(AppError::BadRequest(
            "Avatar file must be 5 MB or smaller".to_string(),
        ));
    }

    // TODO: avatar UI
    // Ensure the persona subdirectory exists under personas_dir.
    let avatar_dir = state.config.personas_dir.join(&id);
    tokio::fs::create_dir_all(&avatar_dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create avatar dir: {}", e)))?;

    let filename = format!("avatar.{}", ext);
    let file_path = avatar_dir.join(&filename);

    // Write the file
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create avatar file: {}", e)))?;
    file.write_all(&bytes)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write avatar file: {}", e)))?;

    let file_path_str = file_path.to_string_lossy().to_string();

    // Update the persona's avatar_path
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE agent_personas SET avatar_path = ?, updated_at = ? WHERE id = ? AND user_id = ?",
    )
    .bind(&file_path_str)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    // Return a URL the frontend can use to display the avatar.
    // The avatar is stored under the personas data directory.
    let avatar_url = format!("/api/personas/{}/avatar", id);

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "avatar_url": avatar_url,
                "avatar_path": file_path_str
            }
        })),
    ))
}

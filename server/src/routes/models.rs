use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::{
        model::{Model, UpdateModel},
        provider::Provider,
    },
    routes::AppState,
    services::{encryption, provider as provider_service},
};

/// Helper: get the single user id from the DB.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// Helper: verify a provider belongs to the current user and return it.
async fn get_provider(state: &AppState, provider_id: &str, user_id: &str) -> AppResult<Provider> {
    let provider: Option<Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers
         WHERE id = ? AND user_id = ?",
    )
    .bind(provider_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;

    provider.ok_or_else(|| AppError::NotFound(format!("Provider '{}' not found", provider_id)))
}

/// GET /api/providers/:id/models
///
/// List all models stored for a provider.
pub async fn list(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = get_provider(&state, &provider_id, &user_id).await?;

    let models: Vec<Model> = sqlx::query_as(
        "SELECT id, provider_id, model_id, display_name, enabled
         FROM models
         WHERE provider_id = ?
         ORDER BY display_name ASC",
    )
    .bind(&provider_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": models }))))
}

/// GET /api/providers/:provider_id/models/:model_id
pub async fn get(
    State(state): State<AppState>,
    Path((provider_id, model_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = get_provider(&state, &provider_id, &user_id).await?;

    let model: Option<Model> = sqlx::query_as(
        "SELECT id, provider_id, model_id, display_name, enabled
         FROM models
         WHERE id = ? AND provider_id = ?",
    )
    .bind(&model_id)
    .bind(&provider_id)
    .fetch_optional(&state.pool)
    .await?;

    match model {
        Some(m) => Ok((StatusCode::OK, Json(json!({ "data": m })))),
        None => Err(AppError::NotFound(format!(
            "Model '{}' not found",
            model_id
        ))),
    }
}

/// POST /api/providers/:id/models/sync
///
/// Re-fetches the model list from the provider's /v1/models endpoint and
/// upserts all returned models into the local DB. Models not present in the
/// remote response are left in place (they may have been manually added or
/// the provider may have returned a partial list).
pub async fn sync(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let provider = get_provider(&state, &provider_id, &user_id).await?;

    // Decrypt the API key for the outbound request
    let decrypted_key = match &provider.api_key {
        Some(encrypted) => {
            let key = encryption::decrypt(encrypted, &state.machine_secret).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Failed to decrypt API key: {}", e))
            })?;
            Some(key)
        }
        None => None,
    };

    let remote_models =
        provider_service::test_provider(&provider.base_url, decrypted_key.as_deref())
            .await
            .map_err(|e| AppError::Provider(e.to_string()))?;

    let mut synced_count = 0usize;

    for remote in &remote_models {
        // Check if this model_id already exists for this provider
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM models WHERE provider_id = ? AND model_id = ?")
                .bind(&provider_id)
                .bind(&remote.id)
                .fetch_optional(&state.pool)
                .await?;

        if existing.is_none() {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO models (id, provider_id, model_id, display_name, enabled)
                 VALUES (?, ?, ?, ?, 1)",
            )
            .bind(&id)
            .bind(&provider_id)
            .bind(&remote.id)
            .bind(&remote.display_name)
            .execute(&state.pool)
            .await?;
            synced_count += 1;
        }
    }

    // Return the full updated list
    let models: Vec<Model> = sqlx::query_as(
        "SELECT id, provider_id, model_id, display_name, enabled
         FROM models
         WHERE provider_id = ?
         ORDER BY display_name ASC",
    )
    .bind(&provider_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "models": models,
                "synced": synced_count
            }
        })),
    ))
}

/// PUT /api/providers/:provider_id/models/:model_id
pub async fn update(
    State(state): State<AppState>,
    Path((provider_id, model_id)): Path<(String, String)>,
    Json(payload): Json<UpdateModel>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = get_provider(&state, &provider_id, &user_id).await?;

    let existing: Option<Model> = sqlx::query_as(
        "SELECT id, provider_id, model_id, display_name, enabled
         FROM models
         WHERE id = ? AND provider_id = ?",
    )
    .bind(&model_id)
    .bind(&provider_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(m) => m,
        None => {
            return Err(AppError::NotFound(format!(
                "Model '{}' not found",
                model_id
            )))
        }
    };

    let display_name = payload
        .display_name
        .as_deref()
        .unwrap_or(&existing.display_name);
    let enabled = payload.enabled.unwrap_or(existing.enabled);

    sqlx::query("UPDATE models SET display_name = ?, enabled = ? WHERE id = ? AND provider_id = ?")
        .bind(display_name)
        .bind(enabled)
        .bind(&model_id)
        .bind(&provider_id)
        .execute(&state.pool)
        .await?;

    let updated = Model {
        id: existing.id,
        provider_id: existing.provider_id,
        model_id: existing.model_id,
        display_name: display_name.to_string(),
        enabled,
    };

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/providers/:provider_id/models/:model_id
pub async fn delete(
    State(state): State<AppState>,
    Path((provider_id, model_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    let _ = get_provider(&state, &provider_id, &user_id).await?;

    let result = sqlx::query("DELETE FROM models WHERE id = ? AND provider_id = ?")
        .bind(&model_id)
        .bind(&provider_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Model '{}' not found",
            model_id
        )));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

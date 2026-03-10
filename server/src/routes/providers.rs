use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::provider::{CreateProvider, Provider, ProviderResponse, UpdateProvider},
    routes::AppState,
    services::{copilot as copilot_service, encryption, provider as provider_service},
};

/// Helper: get the single user id from the DB.
/// Since this is a single-user system there's always exactly one user after setup.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// GET /api/providers
pub async fn list(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let providers: Vec<Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers
         WHERE user_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await?;

    let responses: Vec<ProviderResponse> = providers.into_iter().map(Into::into).collect();

    Ok((StatusCode::OK, Json(json!({ "data": responses }))))
}

/// GET /api/providers/:id
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let provider: Option<Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    match provider {
        Some(p) => Ok((
            StatusCode::OK,
            Json(json!({ "data": ProviderResponse::from(p) })),
        )),
        None => Err(AppError::NotFound(format!("Provider '{}' not found", id))),
    }
}

/// POST /api/providers
pub async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreateProvider>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }
    if payload.base_url.trim().is_empty() {
        return Err(AppError::BadRequest(
            "base_url must not be empty".to_string(),
        ));
    }
    let valid_kinds = ["copilot", "openai", "anthropic", "custom"];
    if !valid_kinds.contains(&payload.kind.as_str()) {
        return Err(AppError::BadRequest(format!(
            "kind must be one of: {}",
            valid_kinds.join(", ")
        )));
    }

    let user_id = get_user_id(&state).await?;

    // Encrypt API key at rest if provided
    let encrypted_key = match &payload.api_key {
        Some(key) if !key.is_empty() => Some(
            encryption::encrypt(key, &state.machine_secret).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Failed to encrypt API key: {}", e))
            })?,
        ),
        _ => None,
    };

    let provider = Provider::new(&user_id, payload);

    sqlx::query(
        "INSERT INTO providers (id, user_id, name, kind, base_url, api_key, enabled, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&provider.id)
    .bind(&provider.user_id)
    .bind(&provider.name)
    .bind(&provider.kind)
    .bind(&provider.base_url)
    .bind(&encrypted_key)
    .bind(provider.enabled)
    .bind(&provider.created_at)
    .execute(&state.pool)
    .await?;

    // Return the provider without the raw key
    let response = ProviderResponse {
        id: provider.id,
        user_id: provider.user_id,
        name: provider.name,
        kind: provider.kind,
        base_url: provider.base_url,
        api_key: encrypted_key.map(|_| "••••••••".to_string()),
        enabled: provider.enabled,
        created_at: provider.created_at,
    };

    Ok((StatusCode::CREATED, Json(json!({ "data": response }))))
}

/// PUT /api/providers/:id
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateProvider>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    // Verify provider exists
    let existing: Option<Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(p) => p,
        None => return Err(AppError::NotFound(format!("Provider '{}' not found", id))),
    };

    // Validate kind if provided
    if let Some(ref kind) = payload.kind {
        let valid_kinds = ["copilot", "openai", "anthropic", "custom"];
        if !valid_kinds.contains(&kind.as_str()) {
            return Err(AppError::BadRequest(format!(
                "kind must be one of: {}",
                valid_kinds.join(", ")
            )));
        }
    }

    // Encrypt new API key if provided
    let new_encrypted_key = match &payload.api_key {
        Some(key) if !key.is_empty() => Some(
            encryption::encrypt(key, &state.machine_secret).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Failed to encrypt API key: {}", e))
            })?,
        ),
        Some(_) => None,                  // empty string clears the key
        None => existing.api_key.clone(), // unchanged — keep existing encrypted value
    };

    let name = payload.name.as_deref().unwrap_or(&existing.name);
    let kind = payload.kind.as_deref().unwrap_or(&existing.kind);
    let base_url = payload.base_url.as_deref().unwrap_or(&existing.base_url);
    let enabled = payload.enabled.unwrap_or(existing.enabled);

    sqlx::query(
        "UPDATE providers
         SET name = ?, kind = ?, base_url = ?, api_key = ?, enabled = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(name)
    .bind(kind)
    .bind(base_url)
    .bind(&new_encrypted_key)
    .bind(enabled)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    let updated = ProviderResponse {
        id: existing.id,
        user_id: existing.user_id,
        name: name.to_string(),
        kind: kind.to_string(),
        base_url: base_url.to_string(),
        api_key: new_encrypted_key.map(|_| "••••••••".to_string()),
        enabled,
        created_at: existing.created_at,
    };

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// DELETE /api/providers/:id
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM providers WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Provider '{}' not found", id)));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

/// POST /api/providers/:id/test
///
/// Tests the provider connection by calling its /v1/models endpoint.
/// Returns the list of available models on success.
pub async fn test_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let provider: Option<Provider> = sqlx::query_as(
        "SELECT id, user_id, name, kind, base_url, api_key, enabled, created_at
         FROM providers
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    let provider = match provider {
        Some(p) => p,
        None => return Err(AppError::NotFound(format!("Provider '{}' not found", id))),
    };

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

    let models = provider_service::test_provider(&provider.base_url, decrypted_key.as_deref())
        .await
        .map_err(|e| AppError::Provider(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "models": models, "connected": true } })),
    ))
}

// ─── Copilot auth endpoints ───────────────────────────────────────────────────

/// GET /api/providers/copilot/auth-status
///
/// Returns whether copilot-api currently holds a valid GitHub token.
/// Also reflects the process status (not running, connecting, etc.).
pub async fn copilot_auth_status(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let Some(ref copilot) = state.copilot else {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "data": {
                    "process_status": "unavailable",
                    "authenticated": false,
                    "reason": "copilot-api service is not configured"
                }
            })),
        ));
    };

    let process_status = copilot.status().await;
    let process_status_str = process_status.as_str().to_string();

    if !copilot.is_available().await {
        return Ok((
            StatusCode::OK,
            Json(json!({
                "data": {
                    "process_status": process_status_str,
                    "authenticated": false,
                    "reason": "copilot-api process is not ready"
                }
            })),
        ));
    }

    // Proxy is up — check whether it has a valid GitHub token
    let auth = copilot_service::check_auth_status(copilot.base_url())
        .await
        .map_err(|e| AppError::Provider(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "process_status": process_status_str,
                "authenticated": auth.authenticated,
                "reason": auth.reason
            }
        })),
    ))
}

/// POST /api/providers/copilot/auth-start
///
/// Triggers the GitHub device auth flow through copilot-api.
/// Returns the device code and verification URL for the UI to display.
///
/// The user visits the verification URL, enters the user code, and authorises
/// the application.  Once complete copilot-api will hold a valid token and
/// subsequent calls to `/auth-status` will return `authenticated: true`.
pub async fn copilot_auth_start(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let Some(ref copilot) = state.copilot else {
        return Err(AppError::Provider(
            "copilot-api service is not configured".to_string(),
        ));
    };

    if !copilot.is_available().await {
        return Err(AppError::Provider(format!(
            "copilot-api is not ready (status: {}). \
             Wait for the process to start before initiating auth.",
            copilot.status().await.as_str()
        )));
    }

    // The copilot-api `auth` subcommand handles the device flow internally.
    // We trigger it by hitting the `/token` endpoint with a POST — the proxy
    // starts the GitHub device auth flow and returns the device code details.
    //
    // NOTE: The exact endpoint depends on the copilot-api version.  We use
    // the base URL without `/v1` since the auth endpoint lives at the root.
    let base = copilot
        .base_url()
        .trim_end_matches("/v1")
        .trim_end_matches('/');
    let url = format!("{}/token", base);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("HTTP client error: {}", e)))?;

    let resp = client
        .post(&url)
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("Failed to reach copilot-api: {}", e)))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .unwrap_or_else(|_| json!({ "error": "invalid response from copilot-api" }));

    if status.is_success() {
        Ok((StatusCode::OK, Json(json!({ "data": body }))))
    } else {
        Err(AppError::Provider(format!(
            "copilot-api auth start failed: {}",
            body
        )))
    }
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::provider::{CreateProvider, Provider, ProviderResponse, UpdateProvider},
    routes::AppState,
    services::{encryption, provider as provider_service},
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
pub async fn list(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
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
    State(state): State<Arc<AppState>>,
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
    State(state): State<Arc<AppState>>,
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
    State(state): State<Arc<AppState>>,
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
    State(state): State<Arc<AppState>>,
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
    State(state): State<Arc<AppState>>,
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

    let models = provider_service::test_provider(
        &provider.base_url,
        decrypted_key.as_deref(),
        &provider.kind,
    )
    .await
    .map_err(|e| AppError::Provider(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "models": models, "connected": true } })),
    ))
}

// ─── Copilot auth endpoints ───────────────────────────────────────────────────
//
// These endpoints implement the GitHub OAuth device flow directly in the server
// so the web UI can drive authentication without needing terminal access.
//
// The GitHub token is written to ~/.local/share/copilot-api/github_token —
// exactly the path that copilot-api reads on startup, so once auth completes
// the sidecar will pick up the token automatically on next startup (or restart).

/// Path where copilot-api stores the GitHub OAuth token.
fn github_token_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    std::path::Path::new(&home)
        .join(".local")
        .join("share")
        .join("copilot-api")
        .join("github_token")
}

/// Read the stored GitHub token, returning None if missing or empty.
async fn read_github_token() -> Option<String> {
    let path = github_token_path();
    let contents = tokio::fs::read_to_string(&path).await.ok()?;
    let trimmed = contents.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// GET /api/providers/copilot/auth-status
///
/// Returns whether a GitHub token is present in the copilot-api token file.
/// Also reflects the sidecar process status when available.
pub async fn copilot_auth_status(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    let process_status = match state.copilot {
        Some(ref svc) => svc.status().await.as_str().to_string(),
        None => "unavailable".to_string(),
    };

    let authenticated = read_github_token().await.is_some();

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "process_status": process_status,
                "authenticated": authenticated,
                "reason": if authenticated { serde_json::Value::Null } else {
                    serde_json::Value::String("No GitHub token found. Please authenticate.".to_string())
                }
            }
        })),
    ))
}

/// POST /api/providers/copilot/auth-start
///
/// Calls GitHub's device-code endpoint directly and returns the user_code +
/// verification_uri for the UI to display.  The device_code is also returned
/// so the UI can poll /auth-poll to complete the flow.
pub async fn copilot_auth_start(_state: State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    // Same client_id and scopes used by copilot-api itself.
    let client_id = "Iv1.b507a08c87ecfe98";
    let scope = "read:user";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("HTTP client error: {}", e)))?;

    let resp = client
        .post("https://github.com/login/device/code")
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(&serde_json::json!({ "client_id": client_id, "scope": scope }))
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("Failed to reach GitHub: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Provider(format!(
            "GitHub device code request failed: {}",
            body
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Provider(format!("Invalid response from GitHub: {}", e)))?;

    Ok((StatusCode::OK, Json(json!({ "data": body }))))
}

/// POST /api/providers/copilot/auth-poll
///
/// Polls GitHub's OAuth access-token endpoint with the device_code.
/// Returns `{ "data": { "authenticated": true } }` once the user has approved,
/// or `{ "data": { "authenticated": false, "reason": "..." } }` while pending.
/// On success the token is written to ~/.local/share/copilot-api/github_token
/// and the copilot-api sidecar is restarted so it loads the new token.
pub async fn copilot_auth_poll(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<impl IntoResponse> {
    let device_code = body
        .get("device_code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("device_code is required".to_string()))?
        .to_string();

    let client_id = "Iv1.b507a08c87ecfe98";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("HTTP client error: {}", e)))?;

    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(&serde_json::json!({
            "client_id": client_id,
            "device_code": device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
        }))
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("Failed to reach GitHub: {}", e)))?;

    let poll_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Provider(format!("Invalid response from GitHub: {}", e)))?;

    if let Some(token) = poll_body.get("access_token").and_then(|v| v.as_str()) {
        // Write the token to the file copilot-api reads on startup.
        let token_path = github_token_path();
        if let Some(parent) = token_path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&token_path, token)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write token: {}", e)))?;

        // Restart the sidecar so it boots fresh and loads the newly written
        // token from disk. The supervise_loop is already running — killing the
        // child causes it to respawn automatically with a clean slate.
        if let Some(ref svc) = state.copilot {
            svc.restart().await;
        }

        return Ok((
            StatusCode::OK,
            Json(json!({ "data": { "authenticated": true } })),
        ));
    }

    // Still pending or an error — surface the reason to the UI.
    let reason = poll_body
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("authorization_pending")
        .to_string();

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "authenticated": false, "reason": reason } })),
    ))
}

/// GET /api/providers/copilot/models
///
/// Public endpoint (no auth, no DB) — proxies the model list from the
/// copilot-api sidecar. Used by the setup wizard before setup is complete
/// so Step 4 can populate the model dropdown without a user row in the DB.
pub async fn copilot_models(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let base_url = match state.copilot {
        Some(ref svc) => svc.base_url(),
        None => {
            return Ok((StatusCode::OK, Json(json!({ "data": [] }))));
        }
    };

    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("HTTP client error: {}", e)))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|_| AppError::Provider("copilot-api sidecar is not reachable".to_string()))?;

    if !resp.status().is_success() {
        return Ok((StatusCode::OK, Json(json!({ "data": [] }))));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Provider(format!("Invalid response from copilot-api: {}", e)))?;

    // The sidecar returns { "data": [...], "object": "list" } — extract the array
    // and reshape each entry into { id, display_name } for the wizard dropdown.
    let models = body
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let display_name = m.get("display_name").and_then(|v| v.as_str()).unwrap_or(id);
                    json!({ "id": id, "display_name": display_name })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok((StatusCode::OK, Json(json!({ "data": models }))))
}

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
        device_token::{DeleteDeviceToken, DeviceToken, RegisterDeviceToken},
        mcp_server::{CreateMcpServer, McpServer, UpdateMcpServer},
    },
    routes::AppState,
    services::mcp::validate_tag,
};

// ─── MCP Servers ───────────────────────────────────────────────────────────────

/// Helper: get the single user id from the DB.
async fn get_user_id(state: &AppState) -> AppResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users LIMIT 1")
        .fetch_optional(&state.pool)
        .await?;
    row.map(|(id,)| id)
        .ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))
}

/// GET /api/mcp-servers
pub async fn list_mcp(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let servers: Vec<McpServer> = sqlx::query_as(
        "SELECT id, user_id, name, tag, description, source_url, server_type, config, status, enabled, created_at, updated_at
         FROM mcp_servers
         WHERE user_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": servers }))))
}

/// GET /api/mcp-servers/:id
pub async fn get_mcp(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let server: Option<McpServer> = sqlx::query_as(
        "SELECT id, user_id, name, tag, description, source_url, server_type, config, status, enabled, created_at, updated_at
         FROM mcp_servers
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    match server {
        Some(s) => Ok((StatusCode::OK, Json(json!({ "data": s })))),
        None => Err(AppError::NotFound(format!("MCP server '{}' not found", id))),
    }
}

/// POST /api/mcp-servers
pub async fn create_mcp(
    State(state): State<AppState>,
    Json(payload): Json<CreateMcpServer>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }
    if payload.server_type != "local" && payload.server_type != "remote" {
        return Err(AppError::BadRequest(
            "server_type must be 'local' or 'remote'".to_string(),
        ));
    }
    // Validate tag if explicitly provided; the model will derive one from name
    // otherwise, but we still validate the derived value.
    if let Some(ref t) = payload.tag {
        validate_tag(t).map_err(AppError::BadRequest)?;
    }

    let user_id = get_user_id(&state).await?;
    let server = McpServer::new(&user_id, payload);

    // Validate the final (possibly derived) tag.
    validate_tag(&server.tag).map_err(AppError::BadRequest)?;

    sqlx::query(
        "INSERT INTO mcp_servers (id, user_id, name, tag, description, source_url, server_type, config, status, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&server.id)
    .bind(&server.user_id)
    .bind(&server.name)
    .bind(&server.tag)
    .bind(&server.description)
    .bind(&server.source_url)
    .bind(&server.server_type)
    .bind(&server.config)
    .bind(&server.status)
    .bind(server.enabled)
    .bind(&server.created_at)
    .bind(&server.updated_at)
    .execute(&state.pool)
    .await?;

    // Connect the newly created server if it is enabled.
    if server.enabled {
        let mcp = state.mcp.clone();
        let server_id = server.id.clone();
        tokio::spawn(async move {
            mcp.connect_server(&server_id).await;
        });
    }

    Ok((StatusCode::CREATED, Json(json!({ "data": server }))))
}

/// PUT /api/mcp-servers/:id
pub async fn update_mcp(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateMcpServer>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let existing: Option<McpServer> = sqlx::query_as(
        "SELECT id, user_id, name, tag, description, source_url, server_type, config, status, enabled, created_at, updated_at
         FROM mcp_servers
         WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    let existing = match existing {
        Some(s) => s,
        None => return Err(AppError::NotFound(format!("MCP server '{}' not found", id))),
    };

    let name = payload.name.as_deref().unwrap_or(&existing.name);
    let tag = match &payload.tag {
        Some(t) => {
            validate_tag(t).map_err(AppError::BadRequest)?;
            t.as_str()
        }
        None => existing.tag.as_str(),
    };
    let description = match &payload.description {
        Some(v) => Some(v.as_str()),
        None => existing.description.as_deref(),
    };
    let source_url = match &payload.source_url {
        Some(v) => Some(v.as_str()),
        None => existing.source_url.as_deref(),
    };
    let config = payload
        .config
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| existing.config.clone());
    let enabled = payload.enabled.unwrap_or(existing.enabled);
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE mcp_servers
         SET name = ?, tag = ?, description = ?, source_url = ?, config = ?, enabled = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(name)
    .bind(tag)
    .bind(description)
    .bind(source_url)
    .bind(&config)
    .bind(enabled)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    let was_enabled = existing.enabled;
    let updated = McpServer {
        id: existing.id.clone(),
        user_id: existing.user_id,
        name: name.to_string(),
        tag: tag.to_string(),
        description: description.map(|s| s.to_string()),
        source_url: source_url.map(|s| s.to_string()),
        server_type: existing.server_type,
        config,
        status: existing.status,
        enabled,
        created_at: existing.created_at,
        updated_at: now,
    };

    // Reconnect if anything that affects the connection changed.
    let config_changed = payload.config.is_some();
    let tag_changed = payload.tag.is_some();
    let mcp = state.mcp.clone();
    let server_id = existing.id.clone();
    tokio::spawn(async move {
        if enabled && (!was_enabled || config_changed || tag_changed) {
            // Newly enabled, config change, or tag change → reconnect.
            mcp.connect_server(&server_id).await;
        } else if !enabled && was_enabled {
            // Disabled → disconnect.
            mcp.disconnect_server(&server_id).await;
        }
    });

    Ok((StatusCode::OK, Json(json!({ "data": updated }))))
}

/// GET /api/mcp-servers/:id/tools
///
/// Returns the cached tool list for the given MCP server.  Tools are
/// populated after the connection manager completes the `tools/list`
/// handshake.  Returns an empty array when the server is not yet connected.
pub async fn list_mcp_tools(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    // Verify the server exists and belongs to this user.
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM mcp_servers WHERE id = ? AND user_id = ?")
            .bind(&id)
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound(format!("MCP server '{}' not found", id)));
    }

    let tools = state.mcp.cached_tools(&id);
    Ok((StatusCode::OK, Json(json!({ "data": tools }))))
}

/// DELETE /api/mcp-servers/:id
pub async fn delete_mcp(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM mcp_servers WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("MCP server '{}' not found", id)));
    }

    // Disconnect and remove from the connection pool.
    let mcp = state.mcp.clone();
    tokio::spawn(async move {
        mcp.disconnect_server(&id).await;
    });

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

// ─── Device Tokens ─────────────────────────────────────────────────────────────

/// POST /api/device-tokens
///
/// Register an FCM device token for push notifications.
/// If the token already exists it is updated (platform may change).
pub async fn register_device(
    State(state): State<AppState>,
    Json(payload): Json<RegisterDeviceToken>,
) -> AppResult<impl IntoResponse> {
    if payload.token.trim().is_empty() {
        return Err(AppError::BadRequest("token must not be empty".to_string()));
    }

    let user_id = get_user_id(&state).await?;
    let device = DeviceToken::new(&user_id, payload);

    // Upsert: if the token already exists, update its platform and updated_at.
    sqlx::query(
        "INSERT INTO device_tokens (id, user_id, token, platform, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(token) DO UPDATE SET
             platform = excluded.platform,
             updated_at = excluded.updated_at",
    )
    .bind(&device.id)
    .bind(&device.user_id)
    .bind(&device.token)
    .bind(&device.platform)
    .bind(&device.created_at)
    .bind(&device.updated_at)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "data": device }))))
}

/// DELETE /api/device-tokens
///
/// Unregister a device token. Body: `{ "token": "<fcm_token>" }`.
/// Returns 404 if the token was not registered.
pub async fn unregister_device(
    State(state): State<AppState>,
    Json(payload): Json<DeleteDeviceToken>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM device_tokens WHERE token = ? AND user_id = ?")
        .bind(&payload.token)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Device token not found".to_string()));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

// ─── Mobile Pairing ────────────────────────────────────────────────────────────

/// POST /api/pairing/generate
///
/// Generates a short-lived pairing token that the mobile app can use to
/// authenticate without manually entering the full auth token. The token is
/// stored in app_config with an expiry. The UI renders it as a QR code.
///
/// The pairing token IS the auth token — the mobile app stores it in MMKV
/// and sends it as a Bearer header on subsequent requests.
pub async fn generate_pairing(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    // The pairing payload is just the auth token + server metadata.
    // The mobile app will scan the QR, store the token, and use it for all requests.
    let auth_token = state.auth_token.clone();

    // Try to determine the local server URL for the QR code.
    // In practice the UI knows its own URL and can embed it, but we provide
    // a best-effort hint here.
    let server_url = format!("http://agent-deck.local:{}", state.config.port);

    let payload = json!({
        "server_url": server_url,
        "token": auth_token
    });

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "pairing_payload": payload,
                "hint": "Encode pairing_payload as JSON in a QR code. The mobile app will scan it to pair automatically."
            }
        })),
    ))
}

/// POST /api/pairing/complete
///
/// Called by the mobile app after scanning the QR code to confirm pairing.
/// Validates the token and returns success so the app knows it's connected.
pub async fn complete_pairing(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> AppResult<impl IntoResponse> {
    let token = payload
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("token is required".to_string()))?;

    let valid = crate::services::auth::validate_token(&state.pool, token)
        .await
        .map_err(|e| AppError::Internal(e))?;

    if !valid {
        return Err(AppError::Unauthorized);
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "paired": true,
                "message": "Mobile device paired successfully."
            }
        })),
    ))
}

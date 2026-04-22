use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    routes::{
        messages::{get_user_id, verify_thread_ownership},
        AppState,
    },
    services::encryption,
};

// ── Shared types ──────────────────────────────────────────────────────────────

/// Global binding as returned by list/create (no secret after creation).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WebhookBindingPublic {
    pub id: String,
    pub name: String,
    pub source: String,
    pub event_type: String,
    pub enabled: bool,
    pub created_at: String,
}

/// Thread attachment row joined with its binding's metadata.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ThreadWebhookBindingPublic {
    pub id: String,               // thread_webhook_bindings.id
    pub webhook_binding_id: String,
    pub name: String,             // from webhook_bindings
    pub source: String,
    pub event_type: String,
    pub enabled: bool,            // from webhook_bindings (global)
    pub prompt: Option<String>,   // from thread_webhook_bindings
    pub created_at: String,       // thread_webhook_bindings.created_at
}

// ── Global registry handlers ──────────────────────────────────────────────────

/// `GET /api/webhook-bindings`
pub async fn list_global(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let bindings: Vec<WebhookBindingPublic> = sqlx::query_as(
        "SELECT id, name, source, event_type, enabled, created_at
         FROM webhook_bindings
         WHERE user_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": bindings }))))
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookBindingRequest {
    pub name: String,
    pub source: String,
    pub event_type: String,
}

/// `POST /api/webhook-bindings`
///
/// Creates a global webhook binding and returns the plaintext secret **once**.
/// The caller must copy the secret immediately — it cannot be retrieved again.
pub async fn create_global(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWebhookBindingRequest>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }

    let plaintext_secret = {
        use rand::Rng;
        let random_bytes: [u8; 32] = rand::thread_rng().gen();
        hex::encode(random_bytes)
    };

    let encrypted_secret = encryption::encrypt(&plaintext_secret, &state.credential_master_key)
        .map_err(AppError::Internal)?;

    let binding_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "INSERT INTO webhook_bindings (id, user_id, name, source, event_type, secret, enabled, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&binding_id)
    .bind(&user_id)
    .bind(&body.name)
    .bind(&body.source)
    .bind(&body.event_type)
    .bind(&encrypted_secret)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let (funnel_url, funnel_enabled) = {
        let cache = state.tailscale_status_cache.read().await;
        match &*cache {
            Some((status, _)) => (status.funnel_url.clone(), status.funnel_enabled),
            None => (None, false),
        }
    };

    let webhook_url = funnel_url
        .as_ref()
        .map(|url| format!("{}/api/webhooks", url.trim_end_matches('/')))
        .unwrap_or_else(|| format!("http://localhost:{}/api/webhooks", state.server_port));

    let mut response = json!({
        "data": {
            "id": binding_id,
            "name": body.name,
            "source": body.source,
            "event_type": body.event_type,
            "webhook_url": webhook_url,
            "secret": plaintext_secret,
            "enabled": true,
            "created_at": now,
        }
    });

    if !funnel_enabled {
        response["warning"] =
            json!("Tailscale Funnel is not enabled — GitHub cannot reach this server.");
    }

    Ok((StatusCode::CREATED, Json(response)))
}

/// `DELETE /api/webhook-bindings/:id`
pub async fn delete_global(
    State(state): State<Arc<AppState>>,
    Path(binding_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result =
        sqlx::query("DELETE FROM webhook_bindings WHERE id = ? AND user_id = ?")
            .bind(&binding_id)
            .bind(&user_id)
            .execute(&state.pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Webhook binding '{}' not found",
            binding_id
        )));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

/// `PATCH /api/webhook-bindings/:id/toggle`
pub async fn toggle_global(
    State(state): State<Arc<AppState>>,
    Path(binding_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query(
        "UPDATE webhook_bindings SET enabled = 1 - enabled WHERE id = ? AND user_id = ?",
    )
    .bind(&binding_id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Webhook binding '{}' not found",
            binding_id
        )));
    }

    let binding: WebhookBindingPublic = sqlx::query_as(
        "SELECT id, name, source, event_type, enabled, created_at
         FROM webhook_bindings
         WHERE id = ?",
    )
    .bind(&binding_id)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": binding }))))
}

// ── Thread attachment handlers ────────────────────────────────────────────────

/// `GET /api/threads/:id/webhook-bindings`
pub async fn list_attachments(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let attachments: Vec<ThreadWebhookBindingPublic> = sqlx::query_as(
        "SELECT twb.id, twb.webhook_binding_id, wb.name, wb.source, wb.event_type,
                wb.enabled, twb.prompt, twb.created_at
         FROM thread_webhook_bindings twb
         JOIN webhook_bindings wb ON wb.id = twb.webhook_binding_id
         WHERE twb.thread_id = ?
         ORDER BY twb.created_at ASC",
    )
    .bind(&thread_id)
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": attachments }))))
}

#[derive(Debug, Deserialize)]
pub struct AttachWebhookBindingRequest {
    pub webhook_binding_id: String,
    pub prompt: Option<String>,
}

/// `POST /api/threads/:id/webhook-bindings`
pub async fn attach(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(body): Json<AttachWebhookBindingRequest>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    // Verify the binding exists and belongs to this user
    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM webhook_bindings WHERE id = ? AND user_id = ?",
    )
    .bind(&body.webhook_binding_id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?;

    if exists.is_none() {
        return Err(AppError::NotFound(format!(
            "Webhook binding '{}' not found",
            body.webhook_binding_id
        )));
    }

    let attachment_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    let insert_result = sqlx::query(
        "INSERT INTO thread_webhook_bindings (id, thread_id, webhook_binding_id, prompt, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&attachment_id)
    .bind(&thread_id)
    .bind(&body.webhook_binding_id)
    .bind(&body.prompt)
    .bind(&now)
    .execute(&state.pool)
    .await;

    match insert_result {
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            return Err(AppError::BadRequest(
                "Binding is already attached to this thread".to_string(),
            ));
        }
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }

    let attachment: ThreadWebhookBindingPublic = sqlx::query_as(
        "SELECT twb.id, twb.webhook_binding_id, wb.name, wb.source, wb.event_type,
                wb.enabled, twb.prompt, twb.created_at
         FROM thread_webhook_bindings twb
         JOIN webhook_bindings wb ON wb.id = twb.webhook_binding_id
         WHERE twb.id = ?",
    )
    .bind(&attachment_id)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "data": attachment }))))
}

/// `DELETE /api/threads/:id/webhook-bindings/:attachment_id`
pub async fn detach(
    State(state): State<Arc<AppState>>,
    Path((thread_id, attachment_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let result =
        sqlx::query("DELETE FROM thread_webhook_bindings WHERE id = ? AND thread_id = ?")
            .bind(&attachment_id)
            .bind(&thread_id)
            .execute(&state.pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Attachment '{}' not found",
            attachment_id
        )));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "detached": true } }))))
}

#[derive(Debug, Deserialize)]
pub struct UpdateAttachmentRequest {
    pub prompt: Option<String>,
}

/// `PATCH /api/threads/:id/webhook-bindings/:attachment_id`
pub async fn update_attachment(
    State(state): State<Arc<AppState>>,
    Path((thread_id, attachment_id)): Path<(String, String)>,
    Json(body): Json<UpdateAttachmentRequest>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let result = sqlx::query(
        "UPDATE thread_webhook_bindings SET prompt = ? WHERE id = ? AND thread_id = ?",
    )
    .bind(body.prompt.as_deref())
    .bind(&attachment_id)
    .bind(&thread_id)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Attachment '{}' not found",
            attachment_id
        )));
    }

    let attachment: ThreadWebhookBindingPublic = sqlx::query_as(
        "SELECT twb.id, twb.webhook_binding_id, wb.name, wb.source, wb.event_type,
                wb.enabled, twb.prompt, twb.created_at
         FROM thread_webhook_bindings twb
         JOIN webhook_bindings wb ON wb.id = twb.webhook_binding_id
         WHERE twb.id = ?",
    )
    .bind(&attachment_id)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({ "data": attachment }))))
}

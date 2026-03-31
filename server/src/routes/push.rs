use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::push_subscription::{CreatePushSubscription, DeletePushSubscription, PushSubscription},
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

/// GET /api/push/vapid-public-key (public — no auth required)
///
/// Returns the server's VAPID public key encoded as base64url (no padding).
/// The client uses this to subscribe via `pushManager.subscribe({ applicationServerKey })`.
pub async fn get_vapid_public_key(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "public_key": state.vapid_public_key } })),
    ))
}

/// POST /api/push/subscribe (authenticated)
///
/// Upserts a browser push subscription. If the endpoint already exists for
/// this user the keys are updated (the browser may rotate them).
pub async fn subscribe(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreatePushSubscription>,
) -> AppResult<impl IntoResponse> {
    if payload.endpoint.trim().is_empty() {
        return Err(AppError::BadRequest(
            "endpoint must not be empty".to_string(),
        ));
    }

    let user_id = get_user_id(&state).await?;
    let sub = PushSubscription::new(&user_id, payload);
    let now = sub.updated_at.clone();

    sqlx::query(
        "INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh, auth, user_agent, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, endpoint) DO UPDATE SET
             p256dh      = excluded.p256dh,
             auth        = excluded.auth,
             user_agent  = excluded.user_agent,
             updated_at  = excluded.updated_at",
    )
    .bind(&sub.id)
    .bind(&sub.user_id)
    .bind(&sub.endpoint)
    .bind(&sub.p256dh)
    .bind(&sub.auth)
    .bind(&sub.user_agent)
    .bind(&sub.created_at)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({ "data": { "subscribed": true } })),
    ))
}

/// DELETE /api/push/subscribe (authenticated)
///
/// Removes a browser push subscription by endpoint URL.
pub async fn unsubscribe(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeletePushSubscription>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;

    let result = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ? AND user_id = ?")
        .bind(&payload.endpoint)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Push subscription not found".to_string(),
        ));
    }

    Ok((StatusCode::OK, Json(json!({ "data": { "deleted": true } }))))
}

use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::AppResult;
use crate::routes::AppState;
use crate::services;

pub async fn get_status(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    {
        let cache = state.tailscale_status_cache.read().await;
        if let Some((ref cached_status, cached_at)) = *cache {
            if cached_at.elapsed() < Duration::from_secs(30) {
                return Ok(Json(json!({ "data": cached_status })));
            }
        }
    }

    let status = services::tailscale::get_status(state.server_port).await;

    {
        let mut cache = state.tailscale_status_cache.write().await;
        *cache = Some((status.clone(), Instant::now()));
    }

    Ok(Json(json!({ "data": status })))
}

pub async fn connect(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let status = services::tailscale::run_connect(state.server_port).await;

    {
        let mut cache = state.tailscale_status_cache.write().await;
        *cache = None;
    }

    Ok(Json(json!({ "data": status })))
}

pub async fn enable_funnel(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let status = services::tailscale::enable_funnel(state.server_port).await;

    {
        let mut cache = state.tailscale_status_cache.write().await;
        *cache = None;
    }

    Ok(Json(json!({ "data": status })))
}

pub async fn disable_funnel(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let status = services::tailscale::disable_funnel(state.server_port).await;

    {
        let mut cache = state.tailscale_status_cache.write().await;
        *cache = None;
    }

    Ok(Json(json!({ "data": status })))
}

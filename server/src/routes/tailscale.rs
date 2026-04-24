use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

use crate::error::AppResult;
use crate::routes::AppState;
use crate::services;
use crate::services::tailscale::TailscaleStatus;

/// Log a one-line status summary at info level.
pub fn log_status(status: &TailscaleStatus) {
    if !status.installed {
        info!("tailscale: not installed");
        return;
    }
    if !status.connected {
        info!("tailscale: installed, not connected");
        return;
    }
    let hostname = status.hostname.as_deref().unwrap_or("unknown");
    if status.funnel_enabled {
        let funnel_url = status.funnel_url.as_deref().unwrap_or("unknown");
        info!(
            "tailscale: connected — {} (funnel: {})",
            hostname, funnel_url
        );
    } else {
        info!("tailscale: connected — {} (funnel: off)", hostname);
    }
    if status.serving {
        let serve_url = status.serve_url.as_deref().unwrap_or("unknown URL");
        info!("tailscale: serving at {}", serve_url);
    } else if status.connected {
        info!("tailscale: connected but not serving");
    }
}

/// Returns true if the fields that are worth surfacing in logs have changed.
fn status_changed(old: &TailscaleStatus, new: &TailscaleStatus) -> bool {
    // Only compare boolean state — URL fields can flicker between polls due to
    // non-deterministic CLI output parsing and should not trigger a log line.
    old.installed != new.installed
        || old.connected != new.connected
        || old.funnel_enabled != new.funnel_enabled
        || old.serving != new.serving
}

pub async fn get_status(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    {
        let cache = state.tailscale_status_cache.read().await;
        if let Some((ref cached_status, cached_at)) = *cache {
            if cached_at.elapsed() < Duration::from_secs(30) {
                return Ok(Json(json!({ "data": cached_status })));
            }
        }
    }

    let new_status = services::tailscale::get_status(state.server_port).await;

    {
        let mut cache = state.tailscale_status_cache.write().await;
        let changed = match *cache {
            Some((ref old_status, _)) => status_changed(old_status, &new_status),
            // None means this is the first refresh after the startup warm-up,
            // which already logged. Only log again if something changed.
            None => false,
        };
        if changed {
            log_status(&new_status);
        }
        *cache = Some((new_status.clone(), Instant::now()));
    }

    Ok(Json(json!({ "data": new_status })))
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

pub async fn serve(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let status = services::tailscale::enable_serve(state.server_port).await;
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

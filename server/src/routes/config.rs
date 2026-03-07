use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::{error::AppResult, routes::AppState};

/// GET /api/config
///
/// Returns public application configuration values.
/// Currently returns whether setup is complete and the server port.
/// Sensitive values (auth token) are never returned here — they are
/// managed via the auth endpoints.
pub async fn get_config(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await?;

    let setup_complete = user_count.0 > 0;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "setup_complete": setup_complete,
                "port": state.config.port,
                "version": env!("CARGO_PKG_VERSION"),
            }
        })),
    ))
}

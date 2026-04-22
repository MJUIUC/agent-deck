use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Database error: {0}")]
    Database(sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

impl From<sqlx::Error> for AppError {
    #[track_caller]
    fn from(e: sqlx::Error) -> Self {
        let location = std::panic::Location::caller();
        tracing::error!(
            error = ?e,
            error.display = %e,
            file = %location.file(),
            line = %location.line(),
            "sqlx::Error at {}:{}", location.file(), location.line()
        );
        AppError::Database(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Database(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("A database error occurred: {e}"),
            ),
            AppError::Internal(e) => {
                tracing::error!("Internal error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error occurred".to_string(),
                )
            }
            AppError::Provider(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::Validation(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
        };

        (
            status,
            Json(json!({
                "error": message
            })),
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

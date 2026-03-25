use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::user::User,
    routes::AppState,
};

/// GET /api/profile
///
/// Returns all profile fields for the current user.
pub async fn get_profile(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let user: Option<User> = sqlx::query_as(
        "SELECT id, display_name, pronouns, role, organization, location,
                timezone, about, profile_updated_at, created_at
         FROM users LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await?;

    let user = user.ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "display_name": user.display_name,
                "pronouns": user.pronouns,
                "role": user.role,
                "organization": user.organization,
                "location": user.location,
                "timezone": user.timezone,
                "about": user.about,
                "profile_updated_at": user.profile_updated_at,
            }
        })),
    ))
}

/// PUT /api/profile
///
/// Partial update: only fields present in the JSON body are updated.
/// Sending a field as `null` explicitly clears it.
/// `profile_updated_at` is always set to the current timestamp on success.
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> AppResult<impl IntoResponse> {
    let obj = payload
        .as_object()
        .ok_or_else(|| AppError::BadRequest("Request body must be a JSON object".to_string()))?;

    // Load current profile
    let current: Option<User> = sqlx::query_as(
        "SELECT id, display_name, pronouns, role, organization, location,
                timezone, about, profile_updated_at, created_at
         FROM users LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await?;

    let current = current.ok_or_else(|| AppError::BadRequest("Setup not complete".to_string()))?;

    // Helper: extract a field from the JSON object.
    // - Key absent → keep current value
    // - Key present as null → clear (None)
    // - Key present as string → use value (Some(string))
    let resolve_str = |key: &str, current_val: Option<String>| -> AppResult<Option<String>> {
        match obj.get(key) {
            None => Ok(current_val),
            Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(s)) => {
                let trimmed = s.trim().to_string();
                Ok(if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                })
            }
            Some(other) => {
                // Accept numbers/bools coerced to string for display_name
                Ok(Some(other.to_string().trim_matches('"').to_string()))
            }
        }
    };

    let new_display_name = match obj.get("display_name") {
        None => current.display_name.clone(),
        Some(serde_json::Value::String(s)) => {
            let t = s.trim().to_string();
            if t.is_empty() {
                return Err(AppError::BadRequest(
                    "display_name must not be empty".to_string(),
                ));
            }
            t
        }
        Some(serde_json::Value::Null) => {
            return Err(AppError::BadRequest(
                "display_name cannot be null".to_string(),
            ));
        }
        _ => current.display_name.clone(),
    };

    let new_pronouns = resolve_str("pronouns", current.pronouns)?;
    let new_role = resolve_str("role", current.role)?;
    let new_organization = resolve_str("organization", current.organization)?;
    let new_location = resolve_str("location", current.location)?;
    let new_timezone = resolve_str("timezone", current.timezone)?;

    // about has a 500-character limit — truncate server-side
    let new_about = resolve_str("about", current.about)?.map(|s| {
        if s.chars().count() > 500 {
            s.chars().take(500).collect::<String>()
        } else {
            s
        }
    });

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    sqlx::query(
        "UPDATE users
         SET display_name        = ?,
             pronouns            = ?,
             role                = ?,
             organization        = ?,
             location            = ?,
             timezone            = ?,
             about               = ?,
             profile_updated_at  = ?",
    )
    .bind(&new_display_name)
    .bind(&new_pronouns)
    .bind(&new_role)
    .bind(&new_organization)
    .bind(&new_location)
    .bind(&new_timezone)
    .bind(&new_about)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": {
                "display_name": new_display_name,
                "pronouns": new_pronouns,
                "role": new_role,
                "organization": new_organization,
                "location": new_location,
                "timezone": new_timezone,
                "about": new_about,
                "profile_updated_at": now,
            }
        })),
    ))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let config = crate::config::Config {
            port: 7474,
            data_dir: std::path::PathBuf::from("/tmp/test-deck"),
            mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
            personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
            database_url: "sqlite::memory:".into(),
            public_dir: "./public".into(),
            fcm_service_account_json: None,
        };

        let token = crate::services::auth::get_or_create_auth_token(&pool)
            .await
            .expect("token");
        let (app, _mcp) = crate::routes::build_router(pool, config)
            .await
            .expect("router");
        (app, token)
    }

    async fn setup_user(app: &axum::Router, _token: &str) {
        let body = r#"{"display_name": "Alice"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup/complete")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_profile_returns_all_fields() {
        let (app, token) = test_app().await;
        setup_user(&app, &token).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/profile")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["data"]["display_name"], "Alice");
        assert!(parsed["data"]["role"].is_null());
        assert!(parsed["data"]["pronouns"].is_null());
    }

    #[tokio::test]
    async fn put_profile_updates_only_sent_fields() {
        let (app, token) = test_app().await;
        setup_user(&app, &token).await;

        // Update role only
        let body = r#"{"role": "Senior Engineer"}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/profile")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["data"]["role"], "Senior Engineer");
        // display_name unchanged
        assert_eq!(parsed["data"]["display_name"], "Alice");
    }

    #[tokio::test]
    async fn put_profile_null_clears_field() {
        let (app, token) = test_app().await;
        setup_user(&app, &token).await;

        // First set a role
        let body = r#"{"role": "Engineer"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/profile")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Now clear it
        let body = r#"{"role": null}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/profile")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(parsed["data"]["role"].is_null());
    }

    #[tokio::test]
    async fn put_profile_updates_profile_updated_at() {
        let (app, token) = test_app().await;
        setup_user(&app, &token).await;

        let body = r#"{"role": "Designer"}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/profile")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            !parsed["data"]["profile_updated_at"].is_null(),
            "profile_updated_at should be set after PUT"
        );
    }

    #[tokio::test]
    async fn put_profile_about_is_truncated_at_500_chars() {
        let (app, token) = test_app().await;
        setup_user(&app, &token).await;

        let long_about = "x".repeat(600);
        let body = serde_json::json!({ "about": long_about }).to_string();
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/profile")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let about = parsed["data"]["about"].as_str().unwrap_or("");
        assert_eq!(about.chars().count(), 500);
    }
}

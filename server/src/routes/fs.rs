use crate::error::{AppError, AppResult};
use crate::routes::AppState;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct PathParams {
    path: String,
}

#[derive(Deserialize)]
pub struct WorkspaceParams {
    thread_id: String,
}

fn validate_path(raw: &str) -> anyhow::Result<std::path::PathBuf> {
    let expanded = if raw.starts_with('~') {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        home.join(raw.trim_start_matches("~/").trim_start_matches('~'))
    } else {
        std::path::PathBuf::from(raw)
    };

    let canonical = std::fs::canonicalize(&expanded)
        .map_err(|e| anyhow::anyhow!("Path does not exist or cannot be resolved: {}", e))?;

    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    let allowed = canonical.starts_with(&home) || canonical.starts_with("/Volumes");
    if !allowed {
        anyhow::bail!(
            "Path '{}' is outside permitted directories",
            canonical.display()
        );
    }

    Ok(canonical)
}

pub async fn list_directory(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<PathParams>,
) -> AppResult<impl IntoResponse> {
    let canonical = validate_path(&params.path).map_err(|e| AppError::Forbidden(e.to_string()))?;

    if !canonical.is_dir() {
        return Err(AppError::BadRequest(format!(
            "'{}' is not a directory",
            canonical.display()
        )));
    }

    let mut read_dir = tokio::fs::read_dir(&canonical)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read directory: {}", e)))?;

    let mut entries: Vec<serde_json::Value> = Vec::new();

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read entry: {}", e)))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let full_path = entry.path().to_string_lossy().into_owned();

        let metadata = match tokio::fs::metadata(entry.path()).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let kind = if metadata.is_dir() { "dir" } else { "file" };

        let size: Option<u64> = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };

        let modified: Option<String> = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                let secs = d.as_secs();
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                    .unwrap_or_default();
                dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            });

        let extension: Option<String> = if metadata.is_file() {
            entry
                .path()
                .extension()
                .map(|ext| ext.to_string_lossy().into_owned())
        } else {
            None
        };

        entries.push(json!({
            "name": name,
            "path": full_path,
            "kind": kind,
            "size": size,
            "modified": modified,
            "extension": extension,
        }));
    }

    entries.sort_by(|a, b| {
        let a_kind = a["kind"].as_str().unwrap_or("file");
        let b_kind = b["kind"].as_str().unwrap_or("file");
        let a_name = a["name"].as_str().unwrap_or("").to_lowercase();
        let b_name = b["name"].as_str().unwrap_or("").to_lowercase();

        match (a_kind, b_kind) {
            ("dir", "file") => std::cmp::Ordering::Less,
            ("file", "dir") => std::cmp::Ordering::Greater,
            _ => a_name.cmp(&b_name),
        }
    });

    let canonical_path_str = canonical.to_string_lossy().into_owned();

    Ok(Json(json!({
        "data": {
            "path": canonical_path_str,
            "entries": entries,
        }
    })))
}

pub async fn read_file(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<PathParams>,
) -> AppResult<impl IntoResponse> {
    let canonical = validate_path(&params.path).map_err(|e| AppError::Forbidden(e.to_string()))?;

    if !canonical.is_file() {
        return Err(AppError::BadRequest(format!(
            "'{}' is not a file",
            canonical.display()
        )));
    }

    let path_str = canonical.to_string_lossy().into_owned();

    let extension: Option<String> = canonical
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned());

    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read metadata: {}", e)))?;

    let len = metadata.len();

    const MAX_BYTES: u64 = 5 * 1024 * 1024;
    if len > MAX_BYTES {
        return Ok(Json(json!({
            "data": {
                "path": path_str,
                "previewable": false,
                "reason": "too_large",
                "size": len,
            }
        })));
    }

    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read file: {}", e)))?;

    match String::from_utf8(bytes.clone()) {
        Ok(text) => Ok(Json(json!({
            "data": {
                "path": path_str,
                "previewable": true,
                "content": text,
                "size": len,
                "extension": extension,
                "is_image": false,
            }
        }))),
        Err(_) => {
            let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"];
            let is_image = extension
                .as_deref()
                .map(|ext| image_extensions.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false);

            if is_image {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(Json(json!({
                    "data": {
                        "path": path_str,
                        "previewable": true,
                        "is_image": true,
                        "image_data": b64,
                        "extension": extension,
                        "size": len,
                    }
                })))
            } else {
                Ok(Json(json!({
                    "data": {
                        "path": path_str,
                        "previewable": false,
                        "reason": "binary",
                        "size": len,
                    }
                })))
            }
        }
    }
}

pub async fn get_workspace(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<WorkspaceParams>,
) -> AppResult<impl IntoResponse> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Could not determine home directory")))?;

    let workspace_path = home.join("agent-deck-workspaces").join(&params.thread_id);

    tokio::fs::create_dir_all(&workspace_path)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to create workspace directory: {}",
                e
            ))
        })?;

    Ok(Json(json!({
        "data": {
            "path": workspace_path.to_string_lossy()
        }
    })))
}

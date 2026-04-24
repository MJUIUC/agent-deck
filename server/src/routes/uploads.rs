use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, State},
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    routes::{
        messages::{get_user_id, verify_thread_ownership},
        AppState,
    },
};

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    // 1. Look up the current user
    let user_id = get_user_id(&state).await?;

    // 2. Verify thread ownership — 404 if not found or not owned
    let _ = verify_thread_ownership(&state, &thread_id, &user_id).await?;

    // 3. Resolve & create the workspace directory
    let workspace = state.config.workspaces_dir.join(&thread_id);
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create workspace: {}", e)))?;

    // 4. Pull the first multipart field
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
        .ok_or_else(|| AppError::BadRequest("No file field found in multipart body".to_string()))?;

    // 5. Extract and sanitize filename
    let raw_filename = field.file_name().unwrap_or("upload").to_string();

    let sanitized: String = raw_filename.replace('/', "").replace('\\', "");

    // Keep only the last component after any remaining separators (belt-and-suspenders)
    let last_component = sanitized
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("upload")
        .to_string();

    let mut filename = if last_component.len() > 200 {
        last_component[..200].to_string()
    } else {
        last_component
    };

    if filename.is_empty() {
        filename = "upload".to_string();
    }

    // 6. Extract content type, defaulting to octet-stream
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    // 7. Read bytes and enforce the 20 MB limit
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read file bytes: {}", e)))?;

    const MAX_BYTES: usize = 20 * 1024 * 1024;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::BadRequest(
            "File exceeds the 20 MB limit".to_string(),
        ));
    }

    // 8. Resolve output path, handling filename collisions
    let (stem, ext) = match filename.rfind('.') {
        Some(dot_pos) => {
            let stem = filename[..dot_pos].to_string();
            let ext = filename[dot_pos + 1..].to_string();
            (stem, Some(ext))
        }
        None => (filename.clone(), None),
    };

    let final_path = {
        let candidate = workspace.join(&filename);
        if !candidate.exists() {
            candidate
        } else {
            let mut counter: u32 = 1;
            loop {
                let new_name = match &ext {
                    Some(e) => format!("{}_{}.{}", stem, counter, e),
                    None => format!("{}_{}", stem, counter),
                };
                let candidate = workspace.join(&new_name);
                if !candidate.exists() {
                    break candidate;
                }
                counter += 1;
            }
        }
    };

    // Derive the actual filename used on disk
    let final_filename = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.clone());

    // 9. Write bytes to disk
    let file_size = bytes.len();
    tokio::fs::write(&final_path, &bytes)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write file: {}", e)))?;

    // 10. Return metadata
    let is_image = content_type.starts_with("image/");
    let abs_path = final_path.to_string_lossy().into_owned();

    Ok(Json(json!({
        "data": {
            "path": abs_path,
            "filename": final_filename,
            "size": file_size,
            "content_type": content_type,
            "is_image": is_image,
        }
    })))
}

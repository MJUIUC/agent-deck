use image::ImageEncoder;
use std::sync::Arc;
use uuid::Uuid;

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

const MAX_IMAGE_DIMENSION: u32 = 2048;
const JPEG_QUALITY: u8 = 82;

/// Decode an image from raw bytes, resize if either dimension exceeds
/// MAX_IMAGE_DIMENSION (preserving aspect ratio), and re-encode as JPEG at
/// JPEG_QUALITY. Returns the compressed bytes, or an error if the image
/// format is unsupported or encoding fails (caller falls back to original).
fn compress_image(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let img = image::load_from_memory(data)?;

    // Resize only when necessary — Lanczos3 is high quality and fast enough
    // on the host hardware for images up to ~48 MP (iPhone 15 Pro camera).
    let img = if img.width() > MAX_IMAGE_DIMENSION || img.height() > MAX_IMAGE_DIMENSION {
        img.resize(
            MAX_IMAGE_DIMENSION,
            MAX_IMAGE_DIMENSION,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    // JPEG does not support an alpha channel — flatten to RGB8 first.
    let rgb = img.to_rgb8();

    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    encoder.write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;

    Ok(buf)
}

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
    let mut content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    // 7. Read bytes and enforce the 20 MB limit
    let raw_bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read file bytes: {}", e)))?;

    const MAX_BYTES: usize = 20 * 1024 * 1024;
    if raw_bytes.len() > MAX_BYTES {
        return Err(AppError::BadRequest(
            "File exceeds the 20 MB limit".to_string(),
        ));
    }

    // 8. Compress images — resize to ≤2048 px and re-encode as JPEG quality 82.
    //    Runs in spawn_blocking so the async runtime isn't stalled by CPU work.
    //    On failure we fall back to storing the original bytes unchanged.
    let final_bytes: Vec<u8> = if content_type.starts_with("image/") {
        let data = raw_bytes.to_vec();
        match tokio::task::spawn_blocking(move || compress_image(&data)).await {
            Ok(Ok(compressed)) => {
                // Switch content-type and normalise the filename extension to .jpg
                content_type = "image/jpeg".to_string();
                let stem = filename
                    .rsplit_once('.')
                    .map(|(s, _)| s)
                    .unwrap_or(&filename);
                filename = format!("{}.jpg", stem);
                compressed
            }
            Ok(Err(e)) => {
                tracing::warn!("Image compression failed, storing original: {}", e);
                raw_bytes.to_vec()
            }
            Err(e) => {
                tracing::warn!("spawn_blocking panicked during image compression: {}", e);
                raw_bytes.to_vec()
            }
        }
    } else {
        raw_bytes.to_vec()
    };

    // 9. Always store with a UUID prefix so generic names like "image.jpeg"
    //    (produced by every iOS camera capture) never collide within a thread.
    let unique_filename = format!("{}_{}", Uuid::new_v4(), filename);
    let final_path = workspace.join(&unique_filename);

    // 10. Write bytes to disk
    let file_size = final_bytes.len();
    tokio::fs::write(&final_path, &final_bytes)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write file: {}", e)))?;

    // 11. Return metadata
    let is_image = content_type.starts_with("image/");
    let abs_path = final_path.to_string_lossy().into_owned();

    Ok(Json(json!({
        "data": {
            "path": abs_path,
            "filename": unique_filename,
            "size": file_size,
            "content_type": content_type,
            "is_image": is_image,
        }
    })))
}

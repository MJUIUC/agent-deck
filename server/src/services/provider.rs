//! Provider abstraction layer.
//!
//! Responsible for:
//! - Translating generic LLM requests into provider-specific HTTP calls
//! - OpenAI-compatible API (all providers use the same wire format)
//! - Fetching available models from a provider's /v1/models endpoint
//! - Testing provider connectivity

// Full streaming implementation lands in Phase 2.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// A simplified model entry returned from a provider's /v1/models endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteModel {
    pub id: String,
    pub display_name: String,
}

/// Test a provider connection by hitting its /v1/models endpoint.
/// Returns a list of available models on success.
pub async fn test_provider(base_url: &str, api_key: Option<&str>) -> Result<Vec<RemoteModel>> {
    let client = reqwest::Client::new();

    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let mut req = client.get(&url);

    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let response = req
        .send()
        .await
        .map_err(|e| anyhow!("Failed to reach provider at {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Provider returned status {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    #[derive(Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelData>,
    }

    #[derive(Deserialize)]
    struct ModelData {
        id: String,
        #[serde(default)]
        name: Option<String>,
    }

    let body: ModelsResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse models response: {}", e))?;

    let models = body
        .data
        .into_iter()
        .map(|m| RemoteModel {
            display_name: m.name.clone().unwrap_or_else(|| m.id.clone()),
            id: m.id,
        })
        .collect();

    Ok(models)
}

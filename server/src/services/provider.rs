//! Provider abstraction layer.
//!
//! Responsible for:
//! - Defining the `LlmProvider` trait that all concrete providers implement
//! - `CopilotProvider` — routes through the local copilot-api proxy at port 4141
//! - `OpenAiProvider` — calls any OpenAI-compatible endpoint (OpenAI, custom, etc.)
//! - Building a `ProviderRegistry` held in `AppState` keyed by provider DB id
//!
//! Anthropic (non-OpenAI-compatible) can be added later by implementing the
//! trait with a custom HTTP client — the trait is designed to accommodate this.

use std::pin::Pin;

use anyhow::{anyhow, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionTool, CreateChatCompletionRequest,
        CreateChatCompletionStreamResponse,
    },
    Client as OpenAIClient,
};
use futures::Stream;
use serde::{Deserialize, Serialize};

// ─── Types ────────────────────────────────────────────────────────────────────

/// A simplified model entry as returned from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteModel {
    pub id: String,
    pub display_name: String,
}

/// A single streamed chunk from an LLM provider.
#[derive(Debug, Clone)]
pub struct TokenChunk {
    /// The text delta (may be empty for role/finish chunks).
    pub delta: String,
    /// Whether this is the final chunk in the stream.
    pub finish_reason: Option<String>,
    /// If the model decided to call a tool, its name is here.
    pub tool_call_name: Option<String>,
    /// Accumulated tool-call argument JSON fragment (may arrive across chunks).
    pub tool_call_args: Option<String>,
    /// Index of the tool call (to correlate fragments across chunks).
    pub tool_call_index: Option<u32>,
    /// Tool call ID assigned by the provider.
    pub tool_call_id: Option<String>,
}

/// Boxed async stream of `TokenChunk`s.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<TokenChunk>> + Send + 'static>>;

// ─── Trait ────────────────────────────────────────────────────────────────────

/// All LLM providers implement this trait.
///
/// The trait is object-safe so it can be stored as `Box<dyn LlmProvider>` in
/// the registry.  An `async_trait` macro is not needed here because we use
/// `Pin<Box<dyn Future>>` return types explicitly — keeping the crate-dependency
/// surface small.
///
/// **Adding non-OpenAI providers (e.g. Anthropic):** Implement this trait with
/// a custom `reqwest`-based client in its own module.  The trait contract does
/// not leak any OpenAI-specific types.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name (for logging / error messages).
    fn name(&self) -> &str;

    /// Returns `true` when the provider is reachable and authenticated.
    async fn is_healthy(&self) -> bool;

    /// List the model IDs available from this provider.
    async fn list_models(&self) -> Result<Vec<RemoteModel>>;

    /// Non-streaming chat completion. Useful for simple one-shot calls.
    async fn complete(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<String>;

    /// Streaming chat completion.  Returns an async stream of [`TokenChunk`]s.
    async fn stream(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<TokenStream>;
}

// ─── OpenAiProvider (handles openai / custom) ─────────────────────────────────

/// An OpenAI-compatible provider.  Works for:
/// - `kind = "openai"` (api.openai.com)
/// - `kind = "custom"` (any OpenAI-compatible endpoint)
///
/// The same type handles both because they share the same wire format.
pub struct OpenAiProvider {
    name: String,
    client: OpenAIClient<OpenAIConfig>,
}

impl OpenAiProvider {
    /// Construct from the stored `base_url` and optional (decrypted) `api_key`.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<impl Into<String>>,
    ) -> Self {
        let mut config = OpenAIConfig::new().with_api_base(base_url.into());
        if let Some(key) = api_key {
            config = config.with_api_key(key.into());
        }
        Self {
            name: name.into(),
            client: OpenAIClient::with_config(config),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn is_healthy(&self) -> bool {
        self.list_models().await.is_ok()
    }

    async fn list_models(&self) -> Result<Vec<RemoteModel>> {
        let response = self
            .client
            .models()
            .list()
            .await
            .map_err(|e| anyhow!("Failed to list models from {}: {}", self.name, e))?;

        let models = response
            .data
            .into_iter()
            .map(|m| RemoteModel {
                display_name: m.id.clone(),
                id: m.id,
            })
            .collect();

        Ok(models)
    }

    async fn complete(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<String> {
        let mut req = CreateChatCompletionRequest {
            model: model.to_string(),
            messages,
            ..Default::default()
        };

        if !tools.is_empty() {
            req.tools = Some(tools);
        }

        let response = self
            .client
            .chat()
            .create(req)
            .await
            .map_err(|e| anyhow!("Chat completion failed ({}): {}", self.name, e))?;

        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(content)
    }

    async fn stream(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<TokenStream> {
        let mut req = CreateChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: Some(true),
            ..Default::default()
        };

        if !tools.is_empty() {
            req.tools = Some(tools);
        }

        let raw_stream = self
            .client
            .chat()
            .create_stream(req)
            .await
            .map_err(|e| anyhow!("Streaming chat completion failed ({}): {}", self.name, e))?;

        let mapped = map_openai_stream(raw_stream);
        Ok(Box::pin(mapped))
    }
}

// ─── CopilotProvider ──────────────────────────────────────────────────────────

/// Routes all requests through the local `copilot-api` proxy at port 4141.
///
/// Before making any request, it checks `CopilotApiService::is_available()` via
/// the `availability_check` closure (injected so this module stays decoupled from
/// the service type).
pub struct CopilotProvider {
    inner: OpenAiProvider,
    /// Async closure that returns `true` when copilot-api is reachable.
    availability_check: Box<dyn Fn() -> futures::future::BoxFuture<'static, bool> + Send + Sync>,
}

impl CopilotProvider {
    /// `availability_fn` should delegate to `CopilotApiService::is_available()`.
    pub fn new<F, Fut>(availability_fn: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = bool> + Send + 'static,
    {
        Self {
            inner: OpenAiProvider::new(
                "GitHub Copilot",
                "http://localhost:4141/v1",
                None::<String>,
            ),
            availability_check: Box::new(move || Box::pin(availability_fn())),
        }
    }

    /// Check availability before forwarding a request.
    async fn ensure_available(&self) -> Result<()> {
        if !(self.availability_check)().await {
            return Err(anyhow!(
                "copilot-api is not available. \
                 Check that the process is running and GitHub authentication is complete."
            ));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProvider for CopilotProvider {
    fn name(&self) -> &str {
        "GitHub Copilot"
    }

    async fn is_healthy(&self) -> bool {
        (self.availability_check)().await
    }

    async fn list_models(&self) -> Result<Vec<RemoteModel>> {
        self.ensure_available().await?;
        self.inner.list_models().await
    }

    async fn complete(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<String> {
        self.ensure_available().await?;
        self.inner.complete(model, messages, tools).await
    }

    async fn stream(
        &self,
        model: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<TokenStream> {
        self.ensure_available().await?;
        self.inner.stream(model, messages, tools).await
    }
}

// ─── Stream mapping helper ────────────────────────────────────────────────────

/// Map the raw `async-openai` stream into our `TokenChunk` stream.
fn map_openai_stream<S>(stream: S) -> impl Stream<Item = Result<TokenChunk>> + Send + 'static
where
    S: Stream<
            Item = std::result::Result<
                CreateChatCompletionStreamResponse,
                async_openai::error::OpenAIError,
            >,
        > + Send
        + 'static,
{
    use futures::StreamExt;

    stream.map(|item| {
        let response = item.map_err(|e| anyhow!("Stream error: {}", e))?;

        let choice = match response.choices.into_iter().next() {
            Some(c) => c,
            None => {
                return Ok(TokenChunk {
                    delta: String::new(),
                    finish_reason: None,
                    tool_call_name: None,
                    tool_call_args: None,
                    tool_call_index: None,
                    tool_call_id: None,
                })
            }
        };

        let finish_reason = choice.finish_reason.map(|r| format!("{:?}", r));
        let delta = choice.delta;

        // Extract tool call fragments if present
        let (tc_name, tc_args, tc_index, tc_id) = if let Some(tool_calls) = delta.tool_calls {
            if let Some(tc) = tool_calls.into_iter().next() {
                let name = tc.function.as_ref().and_then(|f| f.name.clone());
                let args = tc.function.as_ref().and_then(|f| f.arguments.clone());
                (name, args, Some(tc.index as u32), tc.id)
            } else {
                (None, None, None, None)
            }
        } else {
            (None, None, None, None)
        };

        Ok(TokenChunk {
            delta: delta.content.unwrap_or_default(),
            finish_reason,
            tool_call_name: tc_name,
            tool_call_args: tc_args,
            tool_call_index: tc_index,
            tool_call_id: tc_id,
        })
    })
}

// ─── Registry ─────────────────────────────────────────────────────────────────

/// Registry of active providers keyed by their DB provider `id`.
///
/// Held in `AppState` and updated whenever providers are created/deleted.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: std::collections::HashMap<String, Box<dyn LlmProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a provider in the registry.
    pub fn register(&mut self, id: impl Into<String>, provider: Box<dyn LlmProvider>) {
        self.providers.insert(id.into(), provider);
    }

    /// Remove a provider from the registry.
    pub fn remove(&mut self, id: &str) {
        self.providers.remove(id);
    }

    /// Look up a provider by its DB id.
    pub fn get(&self, id: &str) -> Option<&dyn LlmProvider> {
        self.providers.get(id).map(|b| b.as_ref())
    }

    /// Number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

// ─── Legacy test_provider helper (kept for backward compat with routes) ───────

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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProviderRegistry ──────────────────────────────────────────────────────

    struct MockProvider {
        name: String,
        healthy: bool,
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        async fn is_healthy(&self) -> bool {
            self.healthy
        }

        async fn list_models(&self) -> Result<Vec<RemoteModel>> {
            if self.healthy {
                Ok(vec![RemoteModel {
                    id: "mock-model".to_string(),
                    display_name: "Mock Model".to_string(),
                }])
            } else {
                Err(anyhow!("provider unavailable"))
            }
        }

        async fn complete(
            &self,
            _model: &str,
            _messages: Vec<ChatCompletionRequestMessage>,
            _tools: Vec<ChatCompletionTool>,
        ) -> Result<String> {
            Ok("mock response".to_string())
        }

        async fn stream(
            &self,
            _model: &str,
            _messages: Vec<ChatCompletionRequestMessage>,
            _tools: Vec<ChatCompletionTool>,
        ) -> Result<TokenStream> {
            use futures::stream;
            Ok(Box::pin(stream::empty()))
        }
    }

    #[test]
    fn registry_register_and_get() {
        let mut registry = ProviderRegistry::new();
        assert!(registry.is_empty());

        registry.register(
            "provider-1",
            Box::new(MockProvider {
                name: "test".to_string(),
                healthy: true,
            }),
        );

        assert_eq!(registry.len(), 1);
        assert!(registry.get("provider-1").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_remove() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            "p1",
            Box::new(MockProvider {
                name: "p1".to_string(),
                healthy: true,
            }),
        );
        registry.remove("p1");
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_replace_existing() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            "p1",
            Box::new(MockProvider {
                name: "first".to_string(),
                healthy: true,
            }),
        );
        registry.register(
            "p1",
            Box::new(MockProvider {
                name: "second".to_string(),
                healthy: false,
            }),
        );
        assert_eq!(registry.len(), 1);
        let p = registry.get("p1").unwrap();
        assert_eq!(p.name(), "second");
    }

    // ── CopilotProvider availability guard ────────────────────────────────────

    #[tokio::test]
    async fn copilot_provider_unavailable_returns_error() {
        let provider = CopilotProvider::new(|| async { false });

        let result = provider.complete("gpt-4o", vec![], vec![]).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("copilot-api is not available"));
    }

    #[tokio::test]
    async fn copilot_provider_healthy_reflects_availability() {
        let available = CopilotProvider::new(|| async { true });
        assert!(available.is_healthy().await);

        let unavailable = CopilotProvider::new(|| async { false });
        assert!(!unavailable.is_healthy().await);
    }

    #[tokio::test]
    async fn copilot_provider_stream_unavailable_returns_error() {
        let provider = CopilotProvider::new(|| async { false });
        let result = provider.stream("gpt-4o", vec![], vec![]).await;
        assert!(result.is_err());
    }

    // ── TokenChunk defaults ───────────────────────────────────────────────────

    #[test]
    fn token_chunk_fields_accessible() {
        let chunk = TokenChunk {
            delta: "hello".to_string(),
            finish_reason: None,
            tool_call_name: None,
            tool_call_args: None,
            tool_call_index: None,
            tool_call_id: None,
        };
        assert_eq!(chunk.delta, "hello");
        assert!(chunk.finish_reason.is_none());
    }
}

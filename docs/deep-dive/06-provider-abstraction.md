# 06 — Provider Abstraction Layer

Provider logic lives in `server/src/services/provider.rs`.

---

## LlmProvider trait

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn is_healthy(&self) -> bool;
    async fn list_models(&self) -> Result<Vec<RemoteModel>>;
    async fn complete(&self, model: &str, messages: Vec<ChatCompletionRequestMessage>, tools: Vec<ChatCompletionTool>) -> Result<String>;
    async fn stream(&self, model: &str, messages: Vec<ChatCompletionRequestMessage>, tools: Vec<ChatCompletionTool>) -> Result<TokenStream>;
}
```

Object-safe — stored as `Box<dyn LlmProvider>` in the agent run context. Built at runtime in `build_provider()`.

**TokenStream type alias:**
```rust
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<TokenChunk>> + Send + 'static>>;
```

**TokenChunk fields:**
```rust
pub struct TokenChunk {
    pub delta: String,                    // text content delta
    pub finish_reason: Option<String>,
    pub tool_call_name: Option<String>,
    pub tool_call_args: Option<String>,   // fragment; accumulated across chunks
    pub tool_call_index: Option<u32>,     // slot index for merging
    pub tool_call_id: Option<String>,     // provider-assigned call ID
}
```

---

## OpenAiProvider

Handles `kind = "openai"`, `"custom"`, and `"anthropic"` (all via OpenAI-compatible wire format).

```rust
pub struct OpenAiProvider {
    name: String,
    base_url: String,       // e.g. "https://api.openai.com/v1"
    api_key: Option<String>,
}
```

Uses **raw reqwest** (not the `async-openai` streaming helper) because several proxies emit non-standard chunks missing required fields like `model`, which breaks `async-openai`'s strict deserializer.

### Custom SSE streaming parser

```
POST {base_url}/chat/completions  stream=true
  -> byte stream
  -> line buffer
  -> SSE lines "data: {...}"
  -> lenient JSON parse (StreamChunk)
  -> extract choice.delta
  -> emit TokenChunk
```

The JSON structs are all `#[serde(default)]` to handle missing fields gracefully:

```rust
struct StreamChunk { choices: Vec<StreamChoice> }
struct StreamChoice { delta: StreamDelta, finish_reason: Option<Value> }
struct StreamDelta { content: Option<String>, tool_calls: Option<Vec<StreamToolCall>> }
struct StreamToolCall { index: Option<u32>, id: Option<String>, function: Option<StreamFunction> }
```

### Error format

All HTTP errors surface as:
```
Provider {name} returned {status} — {body}
```

This format is inspected by `retry_strategy_for` in the agent run-loop to classify the error.

---

## CopilotProvider

Routes through `copilot-api` (a Bun-based Node.js proxy) running at `localhost:4141`.

```rust
pub struct CopilotProvider {
    inner: OpenAiProvider,  // base_url = "http://localhost:4141/v1"
    availability_check: Box<dyn Fn() -> BoxFuture<'static, bool> + Send + Sync>,
}
```

Before any request, calls `availability_check()` (which delegates to `CopilotApiService::is_available()`). If unavailable, returns an error immediately instead of attempting the request.

The `copilot-api` side-car:
- Manages GitHub device-flow OAuth authentication
- Maintains a short-lived Copilot token (refreshed automatically)
- Translates OpenAI-format requests to the GitHub Copilot API
- Runs on port 4141

---

## Models listing

```rust
// GET {base_url}/models
struct ModelsResponse { data: Vec<ModelEntry> }
struct ModelEntry { id: String }
```

The `display_name` is inferred from `display_name` (copilot-api), then `name` (OpenAI), then falls back to the raw `id`.

---

## Non-streaming completion

Used by title generation and summarization (single-shot, no streaming needed):

```rust
// POST {base_url}/chat/completions  (no stream=true)
struct CompletionResponse { choices: Vec<CompletionChoice> }
struct CompletionChoice { message: CompletionMessage }
struct CompletionMessage { content: Option<String> }
```

---

## ProviderRegistry

Held in `AppState`. Keyed by provider DB id. Updated when providers are created/deleted via the API.

```rust
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn LlmProvider>>,
}
```

In practice, providers are not long-lived in the registry — the agent run creates a fresh `Box<dyn LlmProvider>` per run via `build_provider`. The registry is used for health checks and model listing.

---

## Adding a new provider type

1. Implement `LlmProvider` trait in a new module under `services/`
2. Add a `kind` variant to the match in `build_provider` in `agent.rs`
3. Add the kind to the `providers` table validation if needed

The trait contract is OpenAI-compatible streaming, but nothing prevents a new impl from using a custom HTTP client with a different wire format internally.

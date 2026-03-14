# Phase 4 — Implementation Guide

---

## Phase 4.1a — Streaming Fix and Frontend State Machine ✅ Complete

**Branch:** `feature/phase4-1a-frontend-state` → merged into `dev`
**Commits:** `c4ae600`, `6162339`

### What was built

This work was triggered by a bug where the POST `/api/threads/:id/messages` response appeared to block until the agent finished generating — making the UI look frozen for the duration of the entire LLM response. Investigation revealed two separate root causes, one on the server and one on the client.

#### Server: agent execution decoupled from HTTP response lifecycle

**Problem:** `tokio::spawn` + `yield_now()` was not a reliable way to ensure the HTTP 201 response flushed before the agent task started work. The spawned task could and did race the handler for DB connections, causing the response to be held up until the agent completed on long threads.

**Fix:** Introduced an `AgentJob` mpsc channel on `AppState`. The `send` handler writes a job to the channel and returns `201` immediately. A background worker spawned at startup (`start_agent_worker`) drains the channel and runs each job in its own `tokio::spawn`, fully decoupled from the HTTP request lifecycle.

Additionally, SQLite pool set to `max_connections(1)` with `.serialized(true)` to eliminate write contention between concurrent handler and agent DB access.

#### Client: Vite proxy HTTP/1.1 head-of-line blocking

**Problem:** The persistent SSE connection (`/api/threads/:id/stream`) and the POST request were being routed through the Vite dev proxy over the same HTTP/1.1 keep-alive TCP connection to the Rust server. HTTP/1.1 allows only one in-flight response per connection, so the POST queued behind the infinite SSE response and wasn't delivered to the browser until the SSE connection cycled.

**Fix:** Added `agent: new http.Agent({ keepAlive: false })` to the Vite proxy config for `/api`, forcing a fresh TCP connection per request.

#### Client: streaming phase state machine

**Problems:**
- `StreamingBubble` only rendered when `phase === "streaming"`, meaning nothing appeared between hitting send and the first SSE token arriving
- `sendMessage`'s POST success path was forcing `phase → idle` when it saw `sending`, which dismissed the bubble before tokens arrived
- Every individual SSE token triggered a full Zustand set → React render → ReactMarkdown parse cycle, causing visible jank on fast streams
- Leftover tokens in the RAF buffer after `message_complete` arrived caused a ghost bubble fragment

**Fixes:**
- `StreamingBubble` now renders on `isSending || isStreaming` — appears immediately on send
- POST success path never touches `phase` — all phase transitions are SSE-driven: `sending → streaming` (first token), `streaming → idle` (message_complete)
- Introduced `tokenBuffer.ts`: batches incoming tokens and flushes once per animation frame (~60fps) via `requestAnimationFrame`, one Zustand update and one ReactMarkdown parse per frame
- `finalizeStream` calls `cancelTokenBuffer` to cancel any pending RAF and discard buffered tokens before transitioning to idle

### Files changed

**Server:**
- `server/src/routes/mod.rs` — `AgentJob` struct, `agent_tx` on `AppState`, `start_agent_worker`
- `server/src/routes/messages.rs` — send handler uses `agent_tx.send()` instead of `tokio::spawn`
- `server/src/routes/sse.rs` — `make_state()` test helper updated for `agent_tx`
- `server/src/db/mod.rs` — `max_connections(1)`, `.serialized(true)`

**Client:**
- `web/src/stores/tokenBuffer.ts` _(new)_ — `bufferToken`, `cancelTokenBuffer`
- `web/src/stores/useMessageStore.ts` — `finalizeStream` calls `cancelTokenBuffer`; POST success never touches phase
- `web/src/stores/useSseStore.ts` — token handler uses `bufferToken` instead of calling `appendToken` directly
- `web/src/components/ChatView.tsx` — `StreamingBubble` shown on `isSending || isStreaming`
- `web/vite.config.ts` — `agent: new http.Agent({ keepAlive: false })` on proxy

**Tests:**
- `web/src/stores/tokenBuffer.test.ts` _(new)_ — 11 tests covering buffering, RAF scheduling, cancellation, multi-thread isolation
- `web/src/stores/useMessageStore.test.ts` — 8 new tests: phase isolation on POST success, `cancelTokenBuffer` called by `finalizeStream`, `appendToken` from all phases
- `server/src/routes/mod.rs` — 5 new tests: channel non-blocking send, FIFO ordering, clean shutdown, backpressure, field roundtrip

### Acceptance criteria met

- ✅ POST `/messages` returns 201 in ~3ms regardless of agent response length
- ✅ Streaming bubble appears immediately on send (no delay waiting for first token)
- ✅ Tokens stream smoothly at display rate without jank
- ✅ No ghost bubble fragment after stream completes
- ✅ 139 Rust tests passing, frontend type-checks clean

---

# Phase 4 — Credentials and MCP Integration: Implementation Guide

**Scope:** Encrypted credential storage for static secrets, MCP server connection management, tool discovery, agent integration, and UI polish. After this phase, agents can use external tools in chat.

**What is NOT in scope:** OAuth browser flows, token refresh, Google/Gmail integration, persona-level credential ownership. These are deferred to future work.

---

## Story 4.1 — Credential Store and Encryption

**Branch:** `feature/phase4-credential-store`

### What to build

**1. Master key generation**

On first server run, generate a random 256-bit key. Store it in `app_config` under key `credential_master_key` as a hex-encoded string. On subsequent runs, read it from the DB. This key never leaves the server process — never logged, never in any API response.

Suggested crate: `ring` or `aes-gcm` for AES-256-GCM.

**2. Encrypt/decrypt helpers**

```rust
// services/crypto.rs
pub fn encrypt(plaintext: &[u8], master_key: &[u8]) -> Result<String>;  // returns base64(nonce + ciphertext + tag)
pub fn decrypt(encrypted: &str, master_key: &[u8]) -> Result<Vec<u8>>;
```

Each encryption generates a random 96-bit nonce. The output format is `base64(nonce || ciphertext || auth_tag)`. Store this as the `encrypted_data` column value.

**3. Database migration**

Create the `credentials` table:

```sql
CREATE TABLE credentials (
  id              TEXT PRIMARY KEY,
  key             TEXT NOT NULL UNIQUE,
  display_name    TEXT NOT NULL,
  service         TEXT NOT NULL,
  credential_type TEXT NOT NULL CHECK (credential_type IN ('api_key', 'pat', 'bearer_token', 'key_secret_pair')),
  service_url     TEXT,
  username        TEXT,
  email           TEXT,
  encrypted_data  TEXT NOT NULL,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
```

The `service_url`, `username`, and `email` columns are plaintext — safe for agent context and UI display (e.g. "this PAT belongs to johndoe on github.com"). The `encrypted_data` column holds an AES-256-GCM encrypted JSON blob:

```json
{
  "secret": "ghp_abc123...",
  "password": "optional-password"
}
```

Only `secret` is required. `password` is optional.

**4. CRUD endpoints**

| Method | Path | Notes |
|--------|------|-------|
| `GET` | `/api/credentials` | Returns metadata only. Never includes `encrypted_data`. |
| `POST` | `/api/credentials` | Body includes `secret` field. Server encrypts before storage. |
| `PUT` | `/api/credentials/:id` | Allows updating display_name, service, and optionally the secret. |
| `DELETE` | `/api/credentials/:id` | Deletes the credential. Warn if referenced by an MCP server config. |

**5. Provider API key migration**

The `providers.api_key` column currently stores keys as plaintext. Write a migration that:

1. For each provider with a non-null `api_key`:
   - Creates a `credentials` row with `key = "{provider_kind}_{provider_id_short}"`, `service = provider.kind`, `credential_type = "api_key"`, `encrypted_data = encrypt(api_key)`
   - Updates the provider to reference the credential key (add a `credential_key` column to `providers`, or resolve by convention)
2. After migration, the provider model's key resolution goes through `credentials` table → decrypt

**Important:** The provider settings UI (`/settings/providers`) continues to accept API keys in its existing form fields. Under the hood, saving a provider key now writes through the credential store. The user doesn't need to visit `/settings/credentials` to manage provider keys.

### Testing checklist

- [ ] Master key generated on first run, same key on restart
- [ ] Master key absent from all log output
- [ ] Encrypt → decrypt round-trip produces original plaintext
- [ ] Different encryptions of the same plaintext produce different ciphertext (nonce uniqueness)
- [ ] `GET /api/credentials` never contains `encrypted_data`
- [ ] Provider chat still works after migration (key resolution through credential store)
- [ ] `cargo build` passes, all tests pass

---

## Story 4.2 — Credentials Settings UI

**Branch:** `feature/phase4-credentials-ui`

### What to build

New route: `/settings/credentials`

**List view:** Table showing all credentials with columns: display name, service, credential type, created date. No secret values — show a masked indicator like `••••••••`. Edit and delete buttons per row.

**Add/edit form:** Fields: display name (text), service (text or dropdown with common values: github, openai, anthropic, custom), credential type (dropdown: API Key, Personal Access Token, Bearer Token, Key/Secret Pair), secret value (password input, masked). For `key_secret_pair` type, show two fields (key and secret).

**Delete:** Confirmation dialog. If the credential is referenced by an MCP server's `credential_key`, show a warning listing which servers use it.

**Provider settings integration:** The existing provider add/edit form's API key field now reads from and writes to the credential store. When editing a provider, if it has a linked credential, the API key field shows `••••••••` with a "Change" button. Saving with a new key value updates the credential.

### Navigation

Add "Credentials" to the settings sidebar nav, between "Providers" and "MCP Servers" (logical flow: you set up credentials, then reference them when configuring MCP servers).

---

## Story 4.3 — MCP Connection Manager

**Branch:** `feature/phase4-mcp-connection-manager`

### What to build

**1. Connection pool**

```rust
// services/mcp.rs
pub struct McpConnectionManager {
    connections: DashMap<String, McpConnection>,  // keyed by mcp_server_id
    credential_store: Arc<CredentialStore>,
}
```

**2. Local server transport (stdio)**

- Spawn the configured executable as a child process using `tokio::process::Command`
- Communicate over stdin/stdout using the MCP JSON-RPC protocol
- Monitor the process: if it exits unexpectedly, restart with exponential backoff (1s, 2s, 4s, max 30s, reset after 5 minutes of stability)
- Capture stderr for error logging

Key MCP protocol messages to implement:
- `initialize` → send on connect
- `initialized` → notification after init handshake
- `tools/list` → discover available tools (used in Story 4.4)
- `tools/call` → execute a tool (used in Story 4.4)

**3. Remote server transport (HTTP/SSE)**

- Connect to the URL from the server config
- Resolve `credential_key` from the credential store: look up the key, decrypt, attach as the configured auth header (e.g. `Authorization: Bearer <decrypted_value>`)
- Use SSE for server-to-client messages, HTTP POST for client-to-server

**4. Connection lifecycle**

On server startup: connect to all MCP servers that have `enabled = 1`. On MCP server create/update via API: connect or reconnect. On delete: disconnect and remove from pool.

Update `mcp_servers.status` as connections change: `inactive` → `connecting` → `connected` or `error`. Broadcast status changes via global SSE event so the UI can update badges in real time.

**5. Graceful shutdown**

On Rust server exit (SIGTERM/SIGINT): disconnect all remote servers, kill all local child processes. Use a shutdown hook in the Axum server.

### Testing checklist

- [ ] Local server starts as subprocess, responds to `initialize`
- [ ] Remote server connects with credential in auth header
- [ ] Reconnect with backoff after connection loss
- [ ] Status field updates correctly through lifecycle
- [ ] All child processes killed on server shutdown
- [ ] `GET /api/mcp-servers` shows live status

---

## Story 4.4 — MCP Tool Discovery and Agent Integration

**Branch:** `feature/phase4-mcp-agent-integration`

### What to build

**1. Tool discovery and caching**

After a successful `initialize` handshake, send `tools/list`. Cache the response (tool name, description, input schema JSON) in memory keyed by server ID. Expose via `GET /api/mcp-servers/:id/tools`.

Refresh the cache on reconnect.

**2. Tool namespacing**

Tools are prefixed with the server's `tag` field and a double-underscore separator:

```
Server tag: "github"
Tool name: "create_issue"
Namespaced: "github__create_issue"
```

The double underscore is the separator — split on `__` to determine which server to route to.

**3. Agent integration**

In `agent::run_inner` (or wherever the LLM request is assembled):

1. Load the thread's attached MCP servers from `thread_mcp_servers` (only enabled ones)
2. For each connected server, fetch cached tools
3. Namespace each tool with `{tag}__{tool_name}`
4. Merge with any built-in tools (memory tools come in Phase 5)
5. Include the merged tool list in the LLM request's `tools` / `functions` parameter

**4. Tool call routing**

When the model's response includes a tool call:

1. Parse the tool name: split on `__` to get `(server_tag, tool_name)`
2. Look up the MCP server by tag from the thread's attached servers
3. Send `tools/call` with `{ name: tool_name, arguments: <from model> }` to that server
4. Return the result to the model as a tool result message
5. Continue the generation loop

If the tool call doesn't contain `__`, it's a built-in tool (handle normally). If the server tag doesn't match any attached server, return an error result to the model.

**5. Tool call messages**

Tool calls and results should be stored as messages with `visibility: hidden` (same pattern as will be used for routine intermediate steps). They appear in chat history only when `show_tool_activity` is enabled on the thread.

### Testing checklist

- [ ] `GET /api/mcp-servers/:id/tools` returns tool list after connection
- [ ] Tools appear in LLM request with correct namespacing
- [ ] Model can call a namespaced tool and get results
- [ ] Multi-turn tool use works (model calls tool, gets result, continues)
- [ ] Multiple servers' tools coexist without collision
- [ ] Tool call messages stored as hidden, visible with toggle

---

## Story 4.5 — MCP UI Integration

**Branch:** `feature/phase4-mcp-ui`

### What to build

**1. Global default servers**

In `/settings/mcp-servers`, add an "Auto-attach to new threads" toggle per server row. State is stored in `app_config` under key `default_mcp_servers` as a JSON array of server IDs.

In the thread creation handler (`POST /api/threads`), after creating the thread, read `default_mcp_servers` from `app_config` and insert rows into `thread_mcp_servers` for each.

**2. Tag field in MCP server form**

Add a `tag` text field to the MCP server add/edit form. Default value: copy from the `name` field. Shown with a hint: "Used as the tool name prefix (e.g. github__create_issue)". Validate: alphanumeric, hyphens, underscores only. No spaces.

**3. Tool inspector**

In the thread config pane's MCP section, each attached server gets an expandable disclosure:
- Server name + tag (as a badge) + status indicator (dot: green/yellow/red/gray)
- Expanded: list of tools with name and one-line description
- Source URL as a clickable link when present

Same tool inspector in `/settings/mcp-servers` per server row (expandable).

**4. Connection status badges**

Status dot next to each server: gray (inactive), yellow (connecting), green (connected), red (error). Updated in real time via global SSE `mcp_status_changed` event.

### Testing checklist

- [ ] Auto-attach toggle persists and new threads get default servers
- [ ] Tag field defaults to name, validates correctly
- [ ] Tool inspector shows tools after connection
- [ ] Status badges update in real time
- [ ] Source URL is clickable

---

## First MCP Server to Test With

**GitHub MCP Server (local, Docker)** is the recommended first target:

```json
{
  "name": "github",
  "tag": "github",
  "server_type": "local",
  "config": {
    "executable": "docker",
    "args": ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"],
    "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "{credential:github_pat}" }
  }
}
```

The `{credential:github_pat}` placeholder in env vars is resolved by the connection manager at spawn time — it looks up the credential with key `github_pat`, decrypts, and substitutes the value into the environment.

Setup for testing:
1. Generate a GitHub PAT at github.com/settings/tokens (classic or fine-grained)
2. Store it in agent-deck via `/settings/credentials` as key `github_pat`
3. Add the MCP server via `/settings/mcp-servers`
4. Attach to a thread, send "List my recent GitHub repos"
5. The agent should call `github__list_repos` and return results

**Filesystem MCP Server (local, no auth)** is even simpler for initial testing since it needs no credentials at all.

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

## Story 4.1 — Credential Store and Encryption ✅ Complete

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

- [x] Master key generated on first run, same key on restart
- [x] Master key absent from all log output
- [x] Encrypt → decrypt round-trip produces original plaintext
- [x] Different encryptions of the same plaintext produce different ciphertext (nonce uniqueness)
- [x] `GET /api/credentials` never contains `encrypted_data`
- [x] Provider chat still works after migration (key resolution through credential store)
- [x] `cargo build` passes, all tests pass (139/139)

### What was built

- `server/src/services/encryption.rs` — AES-256-GCM `encrypt`/`decrypt` helpers with random 96-bit nonce, base64-encoded `nonce || ciphertext || auth_tag` output format
- `server/src/services/credentials.rs` — master key bootstrap (`get_or_create_master_key`), full CRUD service, `resolve_secret`, `migrate_provider_api_key`, 20 unit tests
- `server/src/models/credential.rs` — `Credential`, `CredentialWithData`, `CreateCredential`, `UpdateCredential`, `CredentialSecret` structs. `service` field is `Option<String>` with `#[serde(default)]`
- `server/src/routes/credentials.rs` — REST handlers for `GET/POST/PUT/DELETE /api/credentials` + route-level integration tests
- `server/src/db/migrations/004_phase4_credentials.sql` — `credentials` table, `credential_key` column on `providers`
- `server/src/db/migrations/005_credential_type_service_account.sql` — adds `service_account` to `credential_type` CHECK constraint via table-recreate (SQLite pattern)

---

## Story 4.2 — Credentials Settings UI ✅ Complete

**Branch:** `feature/phase4-credentials-ui`

### What was built

**Navigation:** 🔑 Credentials nav item added to `SettingsNav` between Providers and Personas. `"credentials"` added to `SettingsTab` union. `SettingsModal` renders `<CredentialsSettings />` on that tab.

**List view:** Table with columns: Name/Key, Type, Secret (masked `••••••••`), Created. Always-visible Edit button per row. No hover overlay — edit opens the form inline, delete lives inside the edit form.

**Credential types:** `api_key`, `pat`, `bearer_token`, `key_secret_pair`, `service_account`. Each type has a `CREDENTIAL_TYPE_CONFIG` entry with `label`, `keyLabel`, and `keyPlaceholder` — all per-type string logic is centralised here.

**Add form:**
- Display Name + Credential Type (top row)
- Service URL (all types)
- `service_account` type only: Service Account Username, Service Account Email (plaintext — visible to agents), Service Account Password (encrypted)
- All other types: API Key field (+ API Secret for `key_secret_pair`)
- Key field: auto-generated from display name on blur via `slugify()`, only alphanumerics and underscores accepted (`sanitizeKey()` enforced on every keystroke), `keyTouched` flag prevents overwriting manual edits
- Credential type locked to dropdown selection — cannot be changed after creation

**Edit form:**
- Same fields as add, key shown read-only with hint
- Credential type select disabled with hint
- Save Changes button disabled until form is actually dirty (compares all fields against original `editing` record)
- If a secret field is dirty, clicking Save shows an inline `WarningBox` ("Replace existing secret?") before the API call — matches the `GeneralSettings` pattern
- Delete button on the left of the action bar, triggers `DeleteConfirmDialog` with MCP server reference warnings

**API client:** `credentialsApi` (`list`, `get`, `create`, `update`, `delete`) + `Credential`, `CreateCredentialPayload`, `UpdateCredentialPayload`, `CredentialType` types. `apiFetch` fixed to handle 204/empty responses without throwing.

**Not yet done:** Provider settings integration (masked API key field + "Change" button linked to credential store) — deferred to after 4.3 when provider ↔ credential linkage is settled.

### Navigation

Credentials sits between Providers and Personas in the sidebar. Logical flow: Providers → Credentials → MCP Servers.

---

## Story 4.3 — MCP Connection Manager ✅ Complete

**Branch:** `feature/phase4-mcp-connection-manager` → merged into `dev`

### What was built

**`server/src/services/mcp.rs`** _(new)_ — `McpConnectionManager`: cloneable `Arc`-backed handle, `DashMap<String, Arc<McpConnection>>` keyed by server ID.

- **Local (stdio) transport** — spawns executable via `tokio::process::Command` with `kill_on_drop(true)`, newline-delimited JSON-RPC over stdin/stdout, stderr captured to `warn!` logs
- **Remote (HTTP/SSE) transport** — POST-based JSON-RPC, resolves `credential_key` from credential store, formats auth header
- **MCP handshake** — `initialize` → `notifications/initialized` → `tools/list` on connect; tools cached in memory per connection
- **`{credential:key}` placeholder resolution** — env var values like `{credential:github_pat}` decrypted and substituted at spawn time
- **Supervision loop** — exponential backoff 1s → 2s → 4s → … → 30s max, resets after 5 min of stability
- **`connect_server` / `disconnect_server` / `shutdown_all`** — full lifecycle management; child processes killed on graceful shutdown
- **`validate_tag()`** — public fn: alphanumeric/hyphen/underscore, max 64 chars

**`server/src/db/migrations/006_mcp_tag.sql`** _(new)_ — adds `tag TEXT NOT NULL DEFAULT ''` to `mcp_servers`, back-fills from `name`

**`server/src/models/mcp_server.rs`** — added `tag` field to `McpServer`, `CreateMcpServer`, `UpdateMcpServer`; `McpServer::new` derives tag from name when omitted

**`server/src/services/copilot.rs`** — added `McpStatusChanged { mcp_server_id, status, reason }` variant to `GlobalEvent`

**`server/src/routes/tokens.rs`** — all MCP CRUD queries include `tag`; create/update/delete call `connect_server`/`disconnect_server` on the live manager; `list_mcp_tools` returns live cached tool list

**`server/src/routes/mod.rs`** — `AppState` gains `pub mcp: McpConnectionManager`; `build_router` returns `(Router, McpConnectionManager)`; manager started at startup

**`server/src/main.rs`** — graceful shutdown via `.with_graceful_shutdown`, calls `mcp.shutdown_all()` on SIGTERM/SIGINT

### Design notes

- The manager is **"bring your own binary"** — it assumes the executable is already installed and runnable. `npx`, `uvx`, and `docker` are the supported install mechanisms. Git-clone + venv setup is intentionally out of scope here (see Story 4.3a).
- `working_dir` is **not yet implemented** on `LocalConfig` — deferred to 4.3a where the full `~/.agent-deck` directory structure is established.
- Avatar upload (`POST /api/personas/:id/avatar`) exists in `routes/personas.rs` but has no UI and is effectively dead code. Deferred to a future story once the personas directory structure (4.3a) is in place.

### Testing checklist

- [x] 13 new unit tests: tag validation, credential placeholder parsing, JSON-RPC serialisation, `McpStatus` helpers
- [x] 152 total Rust tests passing
- [ ] Live integration: local server starts as subprocess, responds to `initialize`
- [ ] Live integration: remote server connects with credential in auth header
- [ ] Live integration: reconnect with backoff after connection loss
- [ ] Live integration: all child processes killed on server shutdown

---

## Story 4.3a — `~/.agent-deck` Data Directory and MCP Filesystem Sync

**Branch:** `feature/phase4.3a-mcp-data-dir`

### Context

During 4.3 implementation two structural gaps were identified:

1. The server has no stable data directory. The database defaults to `sqlite:./data/agent-deck.db` relative to the working directory, which breaks headless/service installs. All user-accessible files (MCP configs, persona instructions) need a home that survives binary updates and `cargo clean`.
2. MCP server configs live only in the database. For the filesystem to be a usable interface for power users — and to support future `working_dir`-based setups (venvs, cloned repos) — the `~/.agent-deck/mcp/` directory should be the authoritative config store, with the DB acting as an index and runtime state cache.

### Directory layout

```
~/.agent-deck/
├── .database/
│   └── agent-deck.db          # DB hidden from casual browsing
├── mcp/
│   └── <server-id>/
│       ├── config.json        # Authoritative MCP server config (name, tag, server_type, executable/url, args, env)
│       └── ...                # Working files — venv, repo, logs — unmanaged, user's space
└── personas/
    ├── meta.json              # Index: [{ id, name, emoji }]
    └── <persona-id>/
        ├── meta.json          # { id, name, emoji, default_model, default_provider }
        └── instructions.md   # system_prompt content — DB authoritative, file is mirror
```

### Source of truth

| Data | Authoritative | Synced to |
|---|---|---|
| MCP server config | `mcp/<id>/config.json` | DB `config` column — on startup sync + every API write |
| MCP runtime state | DB (`status`, `enabled`, tool cache) | Not written to disk |
| Persona metadata | DB | `personas/<id>/meta.json` on every create/update |
| Persona instructions | DB | `personas/<id>/instructions.md` on every create/update |
| Credentials | DB (encrypted) | Never written to disk |

### What to build

**1. `config.rs` — introduce `data_dir`**

- Default: `~/.agent-deck` (resolved via `dirs::home_dir()`)
- Override: `AGENT_DECK_DATA_DIR` env var
- Derived paths (all computed from `data_dir`, never configured separately):
  - `database_url` → `{data_dir}/.database/agent-deck.db`
  - `mcp_dir` → `{data_dir}/mcp`
  - `personas_dir` → `{data_dir}/personas`
- Add `dirs` crate to workspace dependencies

**2. `db/mod.rs` — dev-path migration hint**

On startup, if `~/.agent-deck/.database/agent-deck.db` does not exist but `./data/agent-deck.db` does, log a clearly visible hint:
```
WARN  Found existing database at ./data/agent-deck.db
WARN  Default location is now ~/.agent-deck/.database/agent-deck.db
WARN  To migrate: mv ./data/agent-deck.db ~/.agent-deck/.database/agent-deck.db
WARN  Using ./data/agent-deck.db for this session
```
Do not auto-migrate. Use the old path for the session so existing dev setups keep working.

**3. `services/mcp.rs` — filesystem sync**

`startup_sync(data_dir)`:
- Create `~/.agent-deck/mcp/` if absent
- Scan for `<server-id>/config.json` files
- For each: upsert into `mcp_servers` (insert if missing, update `config` column if changed)
- For any DB row whose directory no longer exists: set `enabled = 0`, status = `inactive`
- Called from `McpConnectionManager::start()` before connecting

On API **create** (`POST /api/mcp-servers`):
- Write `~/.agent-deck/mcp/<id>/config.json` after DB insert

On API **update** (`PUT /api/mcp-servers/:id`):
- Rewrite `config.json` after DB update

On API **delete** (`DELETE /api/mcp-servers/:id`):
- Remove `~/.agent-deck/mcp/<id>/` directory after DB delete

`LocalConfig` gains `working_dir: Option<String>`. If `None`, defaults to `~/.agent-deck/mcp/<server-id>/` at spawn time. `Command::current_dir()` set accordingly.

**4. `routes/personas.rs` — persona directory mirror**

On **create**: write `~/.agent-deck/personas/<id>/meta.json` and `instructions.md`

On **update**: rewrite both files

On **delete**: remove `~/.agent-deck/personas/<id>/` directory

Update root `~/.agent-deck/personas/meta.json` index on every create/update/delete.

Avatar upload (`POST /api/personas/:id/avatar`): write to `~/.agent-deck/personas/<id>/avatar.<ext>` instead of `data/avatars/`. Note: avatar upload has no UI yet — this is plumbing only.

**5. Startup sequence in `main.rs`**

```
1. Resolve data_dir (~/.agent-deck or AGENT_DECK_DATA_DIR)
2. Create directory structure (data_dir, .database/, mcp/, personas/)
3. Check for dev-path DB migration hint
4. Init DB at data_dir/.database/agent-deck.db
5. Build router (MCP manager startup_sync runs here before connecting)
6. Bind and serve
```

### Testing checklist

- [ ] `~/.agent-deck` directory tree created on first run
- [ ] `AGENT_DECK_DATA_DIR` override respected
- [ ] Dev-path migration hint logged when old DB exists
- [ ] `startup_sync` upserts config.json files into DB
- [ ] `startup_sync` disables DB rows whose directories are gone
- [ ] `POST /api/mcp-servers` writes `config.json`
- [ ] `PUT /api/mcp-servers/:id` rewrites `config.json`
- [ ] `DELETE /api/mcp-servers/:id` removes directory
- [ ] `LocalConfig` `working_dir` defaults to `~/.agent-deck/mcp/<id>/`
- [ ] Persona create/update writes `meta.json` + `instructions.md`
- [ ] Persona delete removes directory
- [ ] Root personas `meta.json` index stays in sync

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

# 09 — API Endpoints Reference

This is a complete reference for all REST endpoints. For the route map see `03-appstate-and-router.md`.

---

## Authentication

All protected endpoints require one of:
- `Authorization: Bearer <token>` header
- `agent_deck_session=<token>` cookie

Requests from `127.0.0.1` or `::1` bypass authentication (localhost bypass).

---

## Setup

### `GET /api/setup/status`
Returns whether initial setup has been completed.
```json
{ "completed": true }
```

### `POST /api/setup/complete`
Complete the setup wizard. Creates the first user, persona, and provider.
```json
{
  "display_name": "Alice",
  "provider_kind": "openai",
  "api_key": "sk-...",
  "model_id": "gpt-4o"
}
```

---

## Auth

### `POST /api/auth/token`
Exchange auth token for a session cookie.
```json
{ "token": "..." }
```
Sets `agent_deck_session` cookie. Returns `200` on success, `401` on wrong token.

### `POST /api/auth/logout`
Clears the session cookie.

### `POST /api/auth/token/rotate`
Generates a new auth token, persists it, and updates the in-memory `auth_token` RwLock. Old token is immediately invalidated.

---

## Providers

### `GET /api/providers`
List all providers for the current user.

### `POST /api/providers`
Create a provider.
```json
{
  "name": "My OpenAI",
  "kind": "openai",
  "base_url": "https://api.openai.com/v1",
  "api_key": "sk-..."
}
```
API key is encrypted with `machine_secret` (AES-256-GCM) before storage.

### `PUT /api/providers/:id`
Update a provider. Partial updates supported.

### `DELETE /api/providers/:id`
Delete provider and all associated models.

### `POST /api/providers/:id/test`
Test provider connectivity by hitting `GET /models`. Returns list of available models.

### Copilot auth

#### `GET /api/providers/copilot/auth-status`
```json
{ "authenticated": true, "username": "octocat" }
```

#### `POST /api/providers/copilot/auth-start`
Begin GitHub device flow. Returns `device_code`, `user_code`, `verification_uri`, `expires_in`, `interval`.

#### `POST /api/providers/copilot/auth-poll`
Poll for device flow completion. Returns `{ "authenticated": true }` when done.

---

## Models

### `GET /api/providers/:id/models`
List models for a provider (from DB, not live).

### `POST /api/providers/:id/models`
Sync models from the provider's live API. Fetches `/models`, upserts into DB.

### `PUT /api/providers/:provider_id/models/:model_id`
Update a model (e.g. set `vision: true`).

---

## Personas

### `GET /api/personas`
List all personas.

### `POST /api/personas`
Create a persona.
```json
{
  "name": "Aldous",
  "emoji": "🦉",
  "system_prompt": "You are Aldous, a wise and patient coding mentor.",
  "default_provider": "provider-uuid",
  "default_model": "model-uuid"
}
```

### `PUT /api/personas/:id`
Update a persona. System prompt changes take effect on the next run.

### `POST /api/personas/:id/avatar`
Upload avatar image (multipart/form-data, max 25MB). Stored at `personas_dir/{persona_id}/avatar.{ext}`.

---

## Credentials

### `GET /api/credentials`
List credentials (values are never returned; only metadata).

### `POST /api/credentials`
Create a credential.
```json
{
  "key": "schwab",
  "kind": "api_key",
  "api_key": "...",
  "secret_key": "..."
}
```
All value fields are encrypted with the credential master key (AES-256-GCM).

### `PUT /api/credentials/:id`
Update a credential. Pass only the fields to change.

---

## MCP Servers

### `GET /api/mcp-servers`
List all MCP servers with current status.

### `POST /api/mcp-servers`
Create an MCP server.
```json
{
  "name": "github",
  "tag": "github",
  "server_type": "local",
  "config": {
    "executable": "docker",
    "args": ["run", "--rm", "-i", "ghcr.io/github/github-mcp-server"],
    "env": {}
  }
}
```
Writes `config.json` to `mcp_dir/{tag}/`. Triggers `connect_server`.

### `PUT /api/mcp-servers/:id`
Update an MCP server. Triggers disconnect + reconnect.

### `DELETE /api/mcp-servers/:id`
Disconnect, delete DB row, remove `mcp_dir/{tag}/`.

### `GET /api/mcp-servers/:id/tools`
Return cached tool list for server (empty if not connected).

### `POST /api/mcp-servers/:id/restart`
Disconnect and reconnect the server.

---

## Threads

### `GET /api/threads`
List threads. Query params: `?archived=true` for archived.

### `POST /api/threads`
Create a thread.
```json
{
  "persona_id": "persona-uuid",
  "title": "New Conversation"
}
```

### `PUT /api/threads/:id`
Update thread settings (title, active_model, active_provider, system_prompt_addendum, show_tool_activity, auto_summarize, etc.).

### `DELETE /api/threads/:id`
Delete thread and all messages (cascade).

### `POST /api/threads/:id/archive`
Archive a thread (sets `archived = true`).

### `POST /api/threads/:id/generate-title`
Force title regeneration from the first user/assistant exchange.

### `GET /api/threads/:id/mcp-servers`
List attached MCP servers for this thread.

### `POST /api/threads/:id/mcp-servers`
Attach an MCP server to a thread.
```json
{ "mcp_server_id": "server-uuid" }
```

### `PATCH /api/threads/:id/mcp-servers/:mcp_id`
Update the thread-level MCP attachment (e.g. override timeout, disable specific tools).

---

## Messages

### `GET /api/threads/:id/messages`
List messages. Respects `visibility` — hidden tool messages only included if `show_tool_activity = true` on the thread. Reverse chronological order by default.

### `POST /api/threads/:id/messages`
Send a message and trigger an agent run.
```json
{
  "content": "What is Rust's borrow checker?",
  "attachments": []
}
```

Flow:
1. Validates thread exists
2. Checks `run_state.depth < MAX_DEPTH` (rejects with 429 if too deep)
3. Persists user message to DB
4. Acquires semaphore permit (waits if another run is active)
5. Spawns `tokio::spawn(agent::run(...))`
6. Stores `RunningTurn` in `run_states[thread_id]`
7. Returns `{ "message_id": "...", "turn_id": "..." }` immediately

### `POST /api/threads/:id/cancel`
Cancel the running agent turn.

Flow:
1. Looks up `run_states[thread_id].running_turn`
2. Calls `running_turn.cancel()` (sends `true` on watch channel)
3. Returns immediately; agent task cleans up asynchronously

### `POST /api/threads/:id/notify`
Internal endpoint used by the cron scheduler to trigger a routine agent run.
```json
{
  "routine_id": "routine-uuid",
  "routine_name": "Morning Briefing",
  "instructions": "...",
  "fired_at": "...",
  "is_routine": true
}
```

### `POST /api/threads/:id/command`
Slash command handler (e.g. `/clear`, `/summarize`).

---

## SSE Streams

### `GET /api/threads/:id/stream`
Per-thread SSE stream. Connect before or after a run starts — events are buffered.

Events emitted:
- `token` — streaming text delta
- `chat_segment` — persisted text segment (before a tool call)
- `message_complete` — final message
- `tool_start` — tool call beginning
- `tool_round_complete` — all tools in a round finished
- `tool_activity` — hidden tool message (call or result)
- `routine_message` — routine-triggered message
- `error` — error event
- `retry` — retry in progress

### `GET /api/events`
Global SSE stream.

Events emitted:
- `thread_updated` — thread received a new message
- `title_updated` — thread title changed
- `mcp_status_changed` — MCP server status changed
- `routine_fired` — routine execution completed
- `copilot_auth_state_changed` — Copilot auth status changed

---

## Routines

### `GET /api/threads/:id/routines`
List routines for a thread.

### `POST /api/threads/:id/routines`
Create a routine.
```json
{
  "name": "Morning Briefing",
  "instructions": "Summarize the key things I should know this morning.",
  "cron_expr": "0 9 * * 1-5"
}
```

### `PATCH /api/threads/:thread_id/routines/:routine_id/toggle`
Enable/disable a routine. Sends `SchedulerCommand::Register` or `SchedulerCommand::Remove`.

---

## Memory

### `POST /api/personas/:id/memory`
Create a memory entry.
```json
{ "content": "User prefers TypeScript over JavaScript." }
```
Max 500 entries per persona. Exceeding limit returns 400.

### `POST /api/personas/:id/memory/search`
Full-text search with prefix matching.
```json
{ "query": "typescript preference" }
```

---

## Profile

### `GET /api/profile`
Get the current user's profile.

### `PUT /api/profile`
Update profile fields.
```json
{
  "display_name": "Alice",
  "pronouns": "she/her",
  "role": "Staff Engineer",
  "organization": "Acme Corp",
  "location": "San Francisco",
  "timezone": "America/Los_Angeles",
  "about": "I prefer concise answers."
}
```

---

## Filesystem

### `GET /api/fs/list?path=<absolute-path>`
List directory contents. Returns `{ files: [{name, type, size, path}] }`.

### `GET /api/fs/read?path=<absolute-path>`
Read file contents as text. Used by the built-in file explorer.

### `GET /api/fs/download?path=<absolute-path>`
Download file with `Content-Disposition: attachment`.

### `GET /api/fs/workspace?thread_id=<id>`
Get workspace info for a thread: path, title, file count.

### `GET /api/fs/image?path=<absolute-path>`
Serve an image file with correct Content-Type.

---

## Config

### `GET /api/config`
Returns server configuration for the frontend.
```json
{
  "setup_complete": true,
  "vapid_public_key": "...",
  "version": "0.x.x"
}
```

---

## Webhooks

### `POST /api/webhooks`
Receive inbound webhook payloads. Routes to attached threads based on `webhook_binding_attachments`. Triggers an agent run with the payload formatted as the user message.

### `GET /api/webhook-bindings`
List global webhook binding definitions.

### `POST /api/webhook-bindings`
Create a global webhook binding definition.

### `GET /api/threads/:id/webhook-bindings`
List bindings attached to a thread.

### `POST /api/threads/:id/webhook-bindings`
Attach a webhook binding to a thread.

---

## Tailscale

### `GET /api/tailscale/status`
Get Tailscale connection status (cached, 5-minute TTL).
```json
{
  "installed": true,
  "connected": true,
  "hostname": "macmini",
  "ip": "100.x.x.x",
  "version": "1.x.x",
  "serving": true,
  "funnel": false
}
```

### `POST /api/tailscale/connect`
Run `tailscale up`.

### `POST /api/tailscale/serve`
Enable Tailscale Serve on the server's port.

### `POST /api/tailscale/funnel/enable`
Enable Tailscale Funnel (public HTTPS access).

### `POST /api/tailscale/funnel/disable`
Disable Tailscale Funnel.

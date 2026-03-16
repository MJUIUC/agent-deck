# Agent-Deck — Project Plan (Part 2: API Contract, Features, UI, Dev Guide)

> See also: **PLAN_1.md** (Overview, Architecture, Data Model) · **PLAN_3.md** (Phased Execution Plan)

---

## 6. API Contract

All endpoints are prefixed with `/api`. Auth is via a shared bearer token stored in `app_config` and provided by clients in the `Authorization: Bearer <token>` header.

### 6.0 Authentication Model

**First device (localhost):** When the React SPA is loaded from the same machine the server runs on (request origin is `127.0.0.1` or `::1`), auth is not required. This allows the first-run setup wizard to work without a chicken-and-egg problem — you need to access the UI to see the token, but you'd need the token to access the UI.

**Remote devices (Tailscale):** Any browser accessing via a Tailscale IP or hostname must provide the auth token. On first visit, the SPA detects a `401` response and shows a **token entry screen** — a single input field with a paste button and a "Connect" action. The user copies the token from the server's terminal output (printed on first run) or from the General settings page on the localhost device. Once entered, the token is stored in a `httpOnly` cookie set by the server via a `POST /api/auth/token` endpoint, so the user doesn't need to re-enter it on subsequent visits.

**Token lifecycle:**
- Generated automatically on first server run (random 64-character hex string)
- Displayed in server terminal output on startup: `Auth token: <token>` (logged once at INFO level)
- Visible in General settings (masked with reveal toggle) on the localhost device
- Rotatable from General settings — rotation invalidates all existing cookies and mobile pairings. After rotating, the user must: (1) re-enter the new token on any other browsers accessing via Tailscale, and (2) re-scan the QR code on the mobile app. The General settings page should display a clear warning to this effect before confirming the rotation.
- Stored server-side in `app_config` table under key `auth_token`

**Mobile app:** The mobile app receives the token via QR pairing (section 7.10) and stores it in MMKV. No cookie mechanism — the token is sent as a Bearer header on every request.

**Auth endpoints:**

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/auth/token` | Validate token, set auth cookie. Body: `{ "token": "<token>" }` |
| `POST` | `/api/auth/logout` | Clear auth cookie |

`POST /api/auth/token` response on success:
```json
{
  "data": { "valid": true }
}
```
Sets a `httpOnly` secure cookie (`agent_deck_session`) with the token hash. Subsequent requests are authenticated via this cookie OR the `Authorization` header (mobile app path).

Responses follow the shape:
- Success: `{ "data": <payload> }`
- Error: `{ "error": { "code": "ERROR_CODE", "message": "Human readable message" } }`

### 6.1 Setup

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/setup/status` | Returns whether first-run setup is complete |
| `POST` | `/api/setup/complete` | Marks setup complete, creates user record |

### 6.2 Providers

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/providers` | List all providers |
| `POST` | `/api/providers` | Create a provider |
| `GET` | `/api/providers/:id` | Get a provider |
| `PUT` | `/api/providers/:id` | Update a provider |
| `DELETE` | `/api/providers/:id` | Delete a provider |
| `POST` | `/api/providers/:id/test` | Test connection, returns model list |
| `GET` | `/api/providers/copilot/auth-status` | Is copilot-api authenticated? |
| `POST` | `/api/providers/copilot/auth-start` | Trigger GitHub device auth flow |

**POST /api/providers body:**
```json
{
  "name": "GitHub Copilot",
  "kind": "copilot",
  "base_url": "http://localhost:4141/v1",
  "api_key": null
}
```

### 6.3 Models

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/providers/:id/models` | List models for a provider |
| `POST` | `/api/providers/:id/models/sync` | Re-fetch models from provider |

### 6.4 Agent Personas

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/personas` | List all personas |
| `POST` | `/api/personas` | Create a persona |
| `GET` | `/api/personas/:id` | Get a persona |
| `PUT` | `/api/personas/:id` | Update a persona |
| `DELETE` | `/api/personas/:id` | Delete a persona |
| `POST` | `/api/personas/:id/avatar` | Upload avatar image (multipart) |

**POST /api/personas body:**
```json
{
  "name": "Aldous",
  "emoji": "🦉",
  "system_prompt": "You are Aldous, a thoughtful and precise assistant...",
  "default_model": "<model_id>",
  "default_provider": "<provider_id>"
}
```

### 6.5 Credentials

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/credentials` | List all credentials (metadata only, never encrypted_data) |
| `POST` | `/api/credentials` | Store a new credential |
| `PUT` | `/api/credentials/:id` | Update a credential |
| `DELETE` | `/api/credentials/:id` | Delete a credential |

**POST /api/credentials body:**
```json
{
  "key": "github_pat",
  "display_name": "GitHub Personal Access Token",
  "service": "github",
  "credential_type": "pat",
  "service_url": "https://github.com",
  "username": "johndoe",
  "email": "john@example.com",
  "secret": "ghp_...",
  "password": null
}
```

`secret` is required. `service_url`, `username`, and `email` are optional plaintext fields (available for agent context and UI display). `password` is optional and encrypted alongside `secret`. No encrypted values are ever returned in any API response.

**GET /api/credentials response** (never returns secrets):
```json
{
  "data": [
    {
      "id": "<id>",
      "key": "github_pat",
      "display_name": "GitHub Personal Access Token",
      "service": "github",
      "credential_type": "pat",
      "service_url": "https://github.com",
      "username": "johndoe",
      "email": "john@example.com",
      "created_at": "2026-03-01T00:00:00Z"
    }
  ]
}
```

### 6.6 MCP Servers

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/mcp-servers` | List all MCP servers with status |
| `POST` | `/api/mcp-servers` | Add an MCP server |
| `GET` | `/api/mcp-servers/:id` | Get an MCP server |
| `GET` | `/api/mcp-servers/:id/tools` | List tools exposed by this server |
| `PUT` | `/api/mcp-servers/:id` | Update an MCP server |
| `DELETE` | `/api/mcp-servers/:id` | Delete an MCP server |
| `POST` | `/api/mcp-servers/:id/connect` | Trigger connection attempt (for local servers) |

**POST /api/mcp-servers body (local):**
```json
{
  "name": "filesystem",
  "description": "Read and write local files",
  "source_url": "https://github.com/anthropics/mcp-filesystem",
  "server_type": "local",
  "config": {
    "executable": "/usr/local/bin/mcp-filesystem",
    "args": ["--root", "/Users/me/projects"],
    "env": {}
  }
}
```

**POST /api/mcp-servers body (remote):**
```json
{
  "name": "gmail",
  "description": "Read and send Gmail",
  "source_url": "https://github.com/example/mcp-gmail",
  "server_type": "remote",
  "config": {
    "url": "https://my-mcp-host.example.com/gmail",
    "auth_header": "Authorization",
    "credential_key": "google_oauth"
  }
}
```

### 6.7 Threads

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/threads` | List threads (default: active only) |
| `GET` | `/api/threads?status=archived` | List archived threads |
| `POST` | `/api/threads` | Create a thread |
| `GET` | `/api/threads/:id` | Get a thread with config |
| `PATCH` | `/api/threads/:id` | Update thread (title, model, addendum) |
| `DELETE` | `/api/threads/:id` | Hard-delete a thread (only if zero messages — used to discard pending threads) |
| `POST` | `/api/threads/:id/archive` | Archive a thread |
| `POST` | `/api/threads/:id/unarchive` | Restore a thread |
| `POST` | `/api/threads/:id/generate-title` | Ask the LLM to generate a title from the first exchange; falls back to truncation |
| `GET` | `/api/threads/:id/mcp-servers` | List MCP servers for thread |
| `POST` | `/api/threads/:id/mcp-servers` | Attach MCP server to thread |
| `DELETE` | `/api/threads/:id/mcp-servers/:mcpId` | Detach MCP server from thread |

**POST /api/threads body:**
```json
{
  "persona_id": "<persona_id>"
}
```

Title is not provided at creation — it defaults to "New Chat". The thread is not created at all until the user sends their first message (pending thread pattern — see §7.2). After the first agent response completes, the client calls `POST /api/threads/:id/generate-title` which uses the LLM to produce a short title from the first user+agent exchange (max 8 words, truncated to 60 chars). Falls back to the `generate_title_from_message()` truncation if the LLM call fails.

### 6.8 Messages

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/threads/:id/messages` | Get message history (paginated) |
| `POST` | `/api/threads/:id/messages` | Send a message (triggers agent run-loop) |
| `POST` | `/api/threads/:id/cancel` | Cancel the active agent run on this thread |

**GET /api/threads/:id/messages query params:**
- `limit` — default 50
- `before` — message ID cursor for pagination

**POST /api/threads/:id/messages body:**
```json
{
  "content": "Hello, can you help me with something?"
}
```

**POST /api/threads/:id/messages response:**
Returns immediately with the persisted user message. The agent response streams via SSE on `/api/threads/:id/stream`.

```json
{
  "data": {
    "id": "<message_id>",
    "thread_id": "<thread_id>",
    "role": "user",
    "content": "Hello, can you help me with something?",
    "source": "chat",
    "created_at": "2025-01-01T09:00:00Z"
  }
}
```

**POST /api/threads/:id/cancel response:**
Signals cancellation of the active agent run. If no run is active, returns `200` with `"cancelled": false` (idempotent, not an error). If a run was active, the `CancellationToken` is triggered and the response returns immediately — the run winds down asynchronously. The partial response (if any) is persisted with `stopped = 1`. An SSE `message_complete` event is emitted with the `stopped` flag when the run finishes winding down.

```json
{
  "data": {
    "cancelled": true
  }
}
```

### 6.8.1 Slash Commands

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/threads/:id/command` | Execute a slash command |

Slash commands are intercepted client-side (the UI detects the `/` prefix) and sent to this dedicated endpoint instead of the message endpoint. Commands are never persisted to message history.

**POST /api/threads/:id/command body:**
```json
{
  "command": "model",
  "args": ["switch", "gpt-4o"]
}
```

`args` is a `Vec<String>` / `string[]` — the client is responsible for splitting the input. The server receives a pre-split array and passes it directly to command handlers with no further parsing.

**Response:**
```json
{
  "data": {
    "type": "model_switched",
    "message": "Switched to gpt-4o",
    "payload": { "model_id": "<id>", "display_name": "GPT-4o" }
  }
}
```

`payload` shape varies by command type. The TypeScript client treats it as a loose `{ [key: string]: unknown }` map and pattern-matches on `type` to render the result. Unknown commands return `400 Bad Request` — the client displays the error message as an ephemeral chat entry.

**Supported commands:**

| Command | Args | Server action |
|---|---|---|
| `model` | `list` | Returns available models for the thread's active provider |
| `model` | `switch <name-or-id>` | Matches by UUID first, then case-insensitive `display_name`; updates `active_model` on the thread |
| `routine` | `list` | Returns routines attached to this thread |
| `routine` | `add` | Returns `{ "type": "open_add_routine_modal" }` signal; client shows "coming soon" toast until Phase 4 |
| `memory` | `list` | Returns last 20 memories for this thread's persona, ordered by recency |
| `help` | (none) | Returns all available commands with descriptions |

**Client autocomplete panel** — shown when the user types `/` as the first character:

| Command | Icon | Arg hint | Description | Badge |
|---|---|---|---|---|
| `/model` | 🔄 | `list \| switch <name>` | List or switch the active model for this thread | Model |
| `/routine` | ⚡ | `list \| add` | List routines or open the add-routine editor | System |
| `/memory` | 🗂 | `list` | Show recent memories for this thread's persona | Memory |
| `/help` | ❓ | *(none)* | Show all available commands | System |

**Toast notifications** — a `ToastProvider` / `useToast` hook is introduced in Story 3.6 for app-level feedback:
- Three variants: `success` (green), `error` (red), `neutral` (muted default)
- Auto-dismisses after 3 s; toasts stack and dismiss FIFO
- Used for: `/model switch` confirmation, `/routine add` "coming soon", future provider token refresh errors, etc.
- Command errors (unknown command, bad args) are shown as ephemeral chat messages, **not** toasts

### 6.8.2 System Notifications

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/threads/:id/notify` | Deliver a system notification to a thread |

Used by both the client (e.g. after a model switch) and server-side services (e.g. the routine scheduler) to notify the agent of a system-level event. The server resolves the event type against the registry, inserts a message if `persist` is true, and triggers an agent run if `trigger` is true.

**Request body:**
```json
{
  "event_type": "model_switched",
  "payload": { "provider_name": "OpenAI", "model_name": "gpt-4o" }
}
```

**Event type registry** — all valid event types, their defaults, and behavior:

| event_type | persist | trigger | Description |
|---|---|---|---|
| `model_switched` | true | false | Active model changed. Agent sees it in history context. |
| `provider_switched` | true | false | Active provider changed. |
| `mcp_server_attached` | true | false | An MCP server was attached to this thread. |
| `mcp_server_detached` | true | false | An MCP server was detached from this thread. |
| `addendum_updated` | true | false | System prompt addendum was changed. |
| `routine_fired` | false | true | A scheduled routine should execute. Payload contains routine instructions. |

Unknown event types are rejected with `400 Bad Request`.

**Behavior by flag combination:**

| persist | trigger | Behavior |
|---|---|---|
| true | false | Insert hidden system message, broadcast `system_event` SSE. No agent run. |
| false | true | Inject payload ephemerally as triggering prompt, run agent, emit visible response. Do not store stimulus. |
| true | true | Insert hidden system message AND trigger an agent run. |
| false | false | Invalid — rejected by registry. |

**persist: true messages** are stored with `role: system`, `source: system_event`, `visibility: hidden`, `event_type` set. They appear in agent history context on every subsequent turn. The client renders them only when `show_system_events` is enabled on the thread.

**persist: false, trigger: true messages** are injected as the triggering prompt for an agent run but never written to `messages`. The agent's visible response is stored normally.

**Response:**
```json
{
  "data": {
    "event_type": "model_switched",
    "persisted": true,
    "triggered": false,
    "message_id": "<uuid or null>"
  }
}
```

### 6.9 SSE Streams

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/threads/:id/stream` | Per-thread SSE event stream |
| `GET` | `/api/events` | Global SSE event stream |

**Thread stream events:**

```
event: token
data: {"token": "Hello"}

event: message_complete
data: {"id": "<id>", "thread_id": "<id>", "role": "assistant", "content": "Hello! How can I help?", "stopped": false, "created_at": "..."}

event: routine_message
data: {"id": "<id>", "thread_id": "<id>", "role": "assistant", "content": "...", "routine_id": "<id>", "created_at": "..."}

event: system_event
data: {"id": "<id>", "thread_id": "<id>", "event_type": "model_switched", "content": "Model switched to OpenAI · gpt-4o", "created_at": "..."}

event: error
data: {"code": "PROVIDER_ERROR", "message": "Failed to connect to provider"}
```

**Global stream events:**

```
event: thread_updated
data: {"thread_id": "<id>", "last_message": "...", "updated_at": "..."}

event: routine_fired
data: {"thread_id": "<id>", "routine_id": "<id>", "routine_name": "Morning Briefing"}
```

### 6.10 Routines

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/threads/:id/routines` | List routines for thread |
| `POST` | `/api/threads/:id/routines` | Create a routine |
| `GET` | `/api/threads/:id/routines/:routineId` | Get a routine |
| `PUT` | `/api/threads/:id/routines/:routineId` | Update a routine |
| `DELETE` | `/api/threads/:id/routines/:routineId` | Delete a routine |
| `PATCH` | `/api/threads/:id/routines/:routineId/toggle` | Enable or disable a routine |

**POST /api/threads/:id/routines body:**
```json
{
  "name": "Morning Briefing",
  "prompt": "Give me a brief summary of what I should focus on today.",
  "cron_expr": "0 9 * * *"
}
```

### 6.11 Memory

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/personas/:id/memories` | List all memories for a persona (paginated, newest first) |
| `GET` | `/api/threads/:id/memories` | List memories with provenance from this thread |
| `DELETE` | `/api/memories/:memoryId` | Delete a single memory entry |

**GET /api/personas/:id/memories query params:**
- `limit` — default 20, max 100
- `before` — memory ID cursor for pagination

**GET response:**
```json
{
  "data": {
    "memories": [
      {
        "id": "<memory_id>",
        "content": "User prefers TypeScript over JavaScript",
        "thread_id": "<thread_id>",
        "thread_title": "Coding session — Atlas project",
        "created_at": "2025-01-15T10:30:00Z"
      }
    ],
    "total_count": 47
  }
}
```

### 6.12 Device Tokens (Push Notifications)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/device-tokens` | Register a device token |
| `DELETE` | `/api/device-tokens/:token` | Unregister a device token |

### 6.13 Mobile Pairing

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/pairing/qr` | Returns server URL + auth token as QR-encodable JSON |

### 6.14 App Config

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/config` | Get all config key/value pairs |
| `PUT` | `/api/config/:key` | Update a config value |

---

## 7. Feature Specifications

### 7.1 Agent Personas

A persona is the identity of an agent. It defines:
- **Name** — displayed in chat headers and thread lists
- **Emoji** — shown as a quick identifier in lists and notifications
- **Avatar** — uploaded image shown next to every agent message in chat, like a contact photo
- **System prompt** — the core instructions that shape the agent's personality and behavior
- **Default model** — the model used unless overridden at the thread level
- **Default provider** — the provider used unless overridden at the thread level

Once a thread is created with a persona, the persona is locked to that thread. Editing a persona affects all future messages in all threads using it but does not rewrite history.

Deleting a persona is only allowed if no active threads use it. Archived threads retain their persona reference but the persona can be soft-deleted.

### 7.2 Threads

A thread is a persistent conversation session between the user and one agent persona.

**Creation:** The user selects a persona. No thread record is written to the database at this point — the client enters a "pending thread" draft state. The thread is created (via `POST /api/threads`) atomically with the first message send. If the user navigates away before sending, no record is created.

**Title generation:** The default title "New Chat" is replaced after the first full exchange. Once the first agent response finishes streaming, the client calls `POST /api/threads/:id/generate-title`. The server fetches the first user message and first assistant message, asks the active LLM to produce a concise title (max 8 words), strips quotes, truncates to 60 chars at a word boundary, and saves it. The sidebar updates immediately via `upsertThread`. Falls back to simple truncation of the first user message if the LLM call fails. Titles are always editable (inline edit — planned, not yet built).

**Configuration pane:** Each thread has a slide-in config panel accessible from the chat UI. From here the user can:
- See which persona the thread uses (display only, not editable)
- Switch the active model (calls `/api/threads/:id` PATCH)
- Add/remove MCP servers
- Add/manage routines
- Add a thread-level system prompt addendum

**Archiving:** Archived threads disappear from the main list but are fully preserved. Routines on archived threads are automatically paused. Threads can be unarchived at any time.

**Deletion:** Soft-delete only in v1 (a `deleted` status flag). Hard delete with full cascade is a future feature — requires careful design to ensure clean removal.

### 7.3 Slash Commands

Slash commands are typed in the chat input and intercepted by the client before sending. The client detects the `/` prefix, parses the command locally into `{ command: string, args: string[] }`, and sends it to `POST /api/threads/:id/command` instead of the message endpoint. Commands are never persisted to message history.

The server owns all command logic via the dedicated command endpoint. This keeps the client thin (just parsing and display) while giving the server a clean, testable contract for command execution.

| Command | Action |
|---|---|
| `/model list` | Fetches and displays available models for the active provider |
| `/model switch <name-or-id>` | Updates `active_model` on the thread; accepts UUID or case-insensitive display name match |
| `/routine list` | Shows routines attached to this thread |
| `/routine add` | Client receives `open_add_routine_modal` signal; shows "coming soon" toast until Phase 4 |
| `/memory list` | Shows last 20 memories for this thread's persona |
| `/help` | Shows all available commands |

**Client behavior:** When the user types `/`, a floating `SlashDropdown` autocomplete panel appears above the input showing the four supported commands with icons, arg hints, descriptions, and category badges. Arrow keys navigate, Enter fills the selected command into the input (does not submit), Escape dismisses, Tab also fills. Typing after `/` filters the list by command name prefix.

Command responses are displayed as **ephemeral messages** inline in the chat — visually distinct from real messages (muted background, "⚡ Slash Command" tag, italic "ephemeral — not saved" metadata). Ephemeral messages are stored in `ChatView` local state (`Record<threadId, EphemeralMessage[]>`) and are cleared on thread switch or page refresh. They are interleaved with real messages by timestamp when rendering.

For `/model switch`: after a successful response the client calls `upsertThread` with the updated `active_model` so the config pane and chat header reflect the change immediately, and fires a `success` toast.

See §6.8.1 for the full API contract and toast behaviour spec.

### 7.4 Routines

A routine is a scheduled prompt that fires automatically on a cron schedule. It executes silently in the background and surfaces only the final synthesized result to the chat thread.

**Two-phase execution model:**

**Phase 1 — Background execution (hidden)**
1. `tokio-cron-scheduler` fires the job at the scheduled time
2. A `routine_executions` row is created with `status: running`
3. A background agent context is created for the thread (in-process, no SSE emission)
4. The agent receives a structured routine invocation message with the routine's instructions
5. The agent executes, including any MCP tool calls, in a private scratchpad
6. All intermediate messages (tool calls, MCP results, partial reasoning) are written to `messages` with `visibility: hidden` and `execution_id` set — they are stored but not rendered in chat

**Phase 2 — Result emission (visible)**
1. The agent produces a final synthesized response
2. This is written to `messages` as a standard assistant message with `source: routine`, `visibility: visible`, and `execution_id` set
3. The `routine_executions` row is updated with `status: completed` and `output_message_id`
4. The server pushes a `routine_message` event on the thread's SSE stream
5. The server checks for connected SSE clients — if none, dispatches an FCM push notification

**Routine invocation message schema** (injected as the triggering user message in the background context, not persisted to visible thread):
```json
{
  "type": "routine_invocation",
  "routine_id": "<uuid>",
  "routine_name": "Morning check-in",
  "instructions": "Fetch unread emails from the last 12 hours using the gmail tool. Summarize them by sender and urgency. Output a brief digest.",
  "fired_at": "2026-03-09T09:00:00Z"
}
```

**Tool activity disclosure:** In the chat view, routine result messages include a collapsed "Show work" disclosure. Expanding it reveals the hidden intermediate steps (tool calls and results) in a read-only, visually distinct format. This is the only way intermediate steps are ever visible — they never appear inline in the chat stream.

**Parameters:**
- Name (human-readable label)
- Prompt (what gets sent to the agent)
- Cron expression (schedule)
- Enabled toggle

Routines automatically pause when a thread is archived and resume when unarchived.

Routines have access to all skills and MCP servers attached to their thread.

The cron expression field in the UI shows a human-friendly description below it ("Runs every day at 9:00 AM") to make scheduling accessible to non-engineers.

### 7.x — System Notification Channel

The system notification channel is a general-purpose mechanism for delivering structured events to a thread's agent context. It replaces all ad-hoc system message insertions with a single normalized endpoint and a server-side event registry.

#### Motivation

Several actors need to notify the agent of state changes or trigger autonomous agent runs:
- The client (model switched, MCP server attached, addendum changed)
- The cron scheduler (routine fired)
- Future server-side services (webhooks, push triggers, external integrations)

Rather than each caller inserting messages directly or using bespoke invocation formats, all of them go through `POST /api/threads/:id/notify`. This keeps the agent's context clean, auditable, and consistent.

#### Two Orthogonal Flags

Each event type in the registry declares two independent boolean flags:

**`persist`** — should the notification be stored in `messages`?
- `true`: inserted as `role: system`, `source: system_event`, `visibility: hidden`. Stays in agent history permanently. Use for durable state changes the agent should always remember (model switch, MCP attach/detach).
- `false`: used only as an ephemeral triggering prompt for the current agent run. Not stored. Use for transient data (routine instructions, weather briefing) where the stimulus doesn't need to outlive the response.

**`trigger`** — should this notification cause an agent run?
- `true`: the agent is invoked with the event payload as its prompt. The agent produces a visible response. Use for anything requiring the agent to act (routines, alarms, external triggers).
- `false`: no agent run. The message is silently inserted into history. Use for passive state updates (model switch, config changes).

The combination `persist: false, trigger: false` is invalid and rejected by the registry.

#### Event Type Registry

All valid event types are enumerated server-side. Unknown types are rejected with `400`. The registry is the single source of truth for what the system can emit.

| event_type | persist | trigger | Agent-visible content |
|---|---|---|---|
| `model_switched` | true | false | "Model switched to {{provider}} · {{model}}" |
| `provider_switched` | true | false | "Provider switched to {{provider}}" |
| `mcp_server_attached` | true | false | "MCP server '{{name}}' attached to this thread" |
| `mcp_server_detached` | true | false | "MCP server '{{name}}' detached from this thread" |
| `addendum_updated` | true | false | "System prompt addendum updated" |
| `routine_fired` | false | true | Routine invocation payload (see section 7.4) |

#### Per-Thread Concurrency — Agent Run Lock

The agent run loop is stateless and per-message, but a single thread must never have two agent runs executing simultaneously. This becomes critical when trigger notifications (routines, system events) can fire independently of the user.

Each thread is protected by a per-thread semaphore held in `AppState` (`DashMap<thread_id, Semaphore>` with a permit count of 1). Any code path that invokes the agent — user message, slash command, or notify trigger — must acquire this semaphore before running and release it when done.

Incoming runs that cannot immediately acquire the semaphore queue behind it (Tokio semaphores handle this natively — no manual queue needed). If the queue depth exceeds a configurable limit (default: 3), user-initiated requests are rejected with `429 Too Many Requests` and a clear message ("This thread is busy — try again in a moment"). **Routine-initiated runs are exempt from the depth limit** — they always queue regardless of depth, so a scheduled routine is never silently dropped.

**Client-side — queued messages:** The message input is **not disabled** while streaming. Users can send additional messages while the agent is responding — they queue on the server and execute in order after the current run completes. Each queued run sees the full history including prior responses. The UI shows a subtle indicator when messages are queued (e.g. "1 message queued") so the user knows their input was received and is waiting.

#### Cancellation

Any active agent run can be cancelled via `POST /api/threads/:id/cancel`. Each active run is associated with a `CancellationToken` (from `tokio_util`) stored in `AppState` alongside the semaphore. The token is checked at three points in the generation loop:

1. **Between streaming chunks** — stop accumulating tokens, persist what exists so far
2. **Before dispatching each tool call** — if the model requested 3 tool calls and 1 has completed, do not start the 2nd
3. **During an in-flight MCP/tool call** — `tokio::select!` the cancellation token against the HTTP request so a hung tool doesn't block cancellation

On cancellation:
- **Completed tool calls and results** from the current run are kept as hidden messages (they're accurate history)
- **In-flight tool calls** that were aborted are not persisted (no result to store)
- **Partial assistant text** (if cancel hit during streaming) is persisted with `stopped = 1`
- **If cancel hit during tool execution before any text was generated**, persist a short message like "Execution stopped" with `stopped = 1` so there is always something visible in the chat
- The semaphore is released and any queued runs proceed
- A `message_complete` SSE event is emitted with `stopped: true`

**Client-side:** While the agent is streaming, the send button transforms into a **stop button** (■ icon). Tapping it calls `POST /api/threads/:id/cancel`. The partial response stays in chat with a visual indicator (e.g. a subtle "stopped" label, similar to the "routine" label on routine messages). After cancellation, the input returns to normal send mode.

#### Visibility Toggles

System event messages respect two independent per-thread toggles (stored on `threads`):
- `show_tool_activity` — reveals hidden tool call/result messages
- `show_system_events` — reveals hidden system event messages

Both default to off. Either can be toggled independently from the Thread Config pane. Both are development/power-user features — the default chat experience remains clean.

#### Client Integration

After any config change that warrants agent awareness, the client calls `POST /api/threads/:id/notify` immediately after the successful mutation:

- After `PUT /api/threads/:id` changes `active_model` → emit `model_switched`
- After `POST /api/threads/:id/mcp-servers` → emit `mcp_server_attached`
- After `DELETE /api/threads/:id/mcp-servers/:id` → emit `mcp_server_detached`
- After addendum blur-save → emit `addendum_updated`

These are fire-and-forget from the client's perspective. A failure to notify is non-fatal — the config change already persisted.

### 7.5 MCP Integration

MCP (Model Context Protocol) is the primary capability extension layer for Agent-Deck. Every integration — email, calendar, file access, code tools — is delivered as an MCP server. There are no skills; MCP replaces that concept entirely.

#### Local vs Remote MCP

**Local servers** are executables managed by the Rust server. The server starts them as child processes, monitors health, and restarts them on crash. Lifecycle is identical to copilot-api management. The `config.executable` and `config.args` fields define the command. Environment variables can be injected via `config.env`.

**Remote servers** are externally hosted HTTP/SSE endpoints. The Rust server connects to them but does not manage their lifecycle. If they go down, graceful degradation applies (same `PROVIDER_UNAVAILABLE` pattern as copilot-api). Auth headers and credential key references are defined in `config`.

#### Per-Persona Defaults

Each persona has a set of default MCP servers (`persona_default_mcp_servers`). When a new thread is created with that persona, those servers are automatically attached to the thread. The user can add or remove servers per-thread from the Thread Config pane at any time.

#### Credential Resolution

When a remote MCP server config references a `credential_key`, the server resolves the credential at connection time:
1. Look up `credentials` by `key`
2. Decrypt the blob using the master key
3. If OAuth and expired, refresh transparently and update the stored token
4. Inject the resolved token into the MCP server's auth header

The MCP server process or endpoint receives only the resolved token for its credential — not the full credential store.

#### Tool Visibility

All MCP tool calls and results are stored as `hidden` messages in the `messages` table. They do not appear in the chat thread by default. A per-thread "Show tool activity" toggle in Thread Config changes client rendering — when enabled, hidden messages appear as collapsible disclosure rows between regular messages, showing what tools were called and what they returned.

### 7.6 Agent Memory

Agents have two forms of memory:

**Short-term (in-context):** The last N messages from the thread are included in every LLM request. N is configurable per thread (default: 20). This is simple conversation continuity.

**Long-term (persistent, keyword search):** The agent has access to tool calls for saving and recalling information that persists across threads. This is what gives each persona a sense of continuity and personality over time.

#### 7.6.1 Scoping

Memory entries are scoped to **user + persona**. Each agent persona maintains its own memory across all threads. An agent "knows you" regardless of which conversation a memory was created in, but different personas have independent memories. The `thread_id` is stored as provenance metadata — useful for debugging and the memory viewer ("this memory came from your Thursday coding session") — but does not restrict access.

This means: if you tell your coding assistant persona about a project deadline in one thread, it can recall that deadline in a completely different thread. But your creative writing persona won't know about it — each persona's knowledge is independent, just like talking to different people.

#### 7.6.2 Tool Definitions

Two tools are included in every LLM request where the thread's persona is not the Default persona (for providers that support function calling). When the thread uses the Default persona ("No persona"), these tools are omitted entirely:

**`save_memory`**
```json
{
  "name": "save_memory",
  "description": "Save a piece of information to your long-term memory for future recall. Use this when the user shares a preference, a fact about themselves, a project name, a deadline, a relationship detail, or anything worth remembering across conversations. Save one fact per call. Write the memory as a concise factual statement.",
  "parameters": {
    "type": "object",
    "properties": {
      "content": {
        "type": "string",
        "description": "A concise factual statement to remember. Examples: 'User prefers TypeScript over JavaScript', 'User's dog is named Pepper', 'Project Atlas deadline is March 15 2025', 'User dislikes being called buddy'"
      }
    },
    "required": ["content"]
  }
}
```

**`recall_memory`**
```json
{
  "name": "recall_memory",
  "description": "Search your long-term memory for information you've previously saved about the user. Use this before answering questions that might benefit from prior context, when the user references something from a past conversation, or when you need to check if you already know something. Returns up to 10 matching entries.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Keywords to search for in memory. Use specific nouns and terms rather than full sentences. Examples: 'project deadline', 'dog name', 'programming language preference'"
      }
    },
    "required": ["query"]
  }
}
```

#### 7.6.3 System Prompt Instructions

Every non-default persona's system prompt has the following memory instructions appended automatically by the agent run-loop. These are not editable by the user — they are injected by the system after the persona's custom system prompt. When the thread uses the Default persona ("No persona"), this block is not appended:

```
## Memory

You have persistent long-term memory that spans across all our conversations. Use it actively:

**When to save:** When I share a preference, a fact about myself, a project detail, a deadline, a name, a relationship, a goal, or anything that seems worth remembering in future conversations — save it immediately using save_memory. Save one fact per call. Write memories as concise factual statements, not narrative.

**When to recall:** Before answering questions that might benefit from prior context, when I reference something from a past conversation, or when I seem to assume you know something — check your memory using recall_memory. Use specific keywords, not full sentences.

**Do not** tell me every time you save or recall a memory. Use memory silently unless I specifically ask what you remember about something.
```

The "do not narrate" instruction is important — without it, models tend to say things like "I've saved that to my memory!" after every save, which is annoying. The agent should use memory transparently, the way a person would.

#### 7.6.4 Memory Content Format

Memories are stored as concise factual statements. The system prompt instructs the model to format them this way, but the server does not enforce a schema — the content field is free text. Examples of well-formed memories:

- "User's name is Marcus"
- "User prefers dark mode in all applications"
- "User's project Atlas has a deadline of March 15 2025"
- "User's dog is a golden retriever named Pepper"
- "User works at Acme Corp as a senior engineer"
- "User dislikes when I use bullet points in casual conversation"
- "User's timezone is US Pacific"

Poorly-formed memories that the system prompt discourages:

- "The user told me about their project called Atlas which has a deadline coming up in March" (too narrative, hard to keyword-match)
- "Remember this for later" (no actual content)
- "User said hello" (ephemeral, not worth saving)

#### 7.6.5 Recall Behavior

`recall_memory` queries the `memory_fts` FTS5 index filtered by the current `user_id` and `persona_id`. Results are capped at **10 entries** to avoid blowing up the context window. Results are returned as a tool response in the format:

```
Found 3 memories:
- [2025-01-15] User's project Atlas has a deadline of March 15 2025
- [2025-01-10] User works at Acme Corp as a senior engineer
- [2025-01-08] User prefers concise responses without excessive formatting

(Thread sources: coding-session, general-chat, preferences)
```

The date prefix helps the model assess recency. The thread source names (derived from thread titles via provenance) give contextual grounding without cluttering the response.

If no memories match, the tool returns: `No memories found matching "query terms".`

#### 7.6.6 Deduplication and Maintenance

v1 does **not** implement automatic deduplication. If the model saves "User likes Python" twice, both entries exist. This is acceptable for the initial version because:
- FTS5 search returns the most relevant matches regardless of duplicates
- The 10-result cap prevents duplicate entries from dominating recall
- Manual cleanup is available via the memory viewer (see 7.6.7)

**Future improvement:** A background job that periodically scans for near-duplicate entries (same persona, similar FTS5 match score) and merges them. Or a `consolidate_memory` tool the model can call to rewrite/merge related entries.

#### 7.6.7 Memory Viewer

A lightweight memory viewer is accessible from two places:

1. **Thread config pane** — a "Memory" section that shows recent memories saved from the current thread (filtered by provenance `thread_id`). This is a quick glance at what the agent learned from this specific conversation.

2. **Settings → Personas → [persona] → Memory** — a full list of all memories for that persona, ordered by recency. Each entry shows content, date, and source thread name. Entries can be manually deleted by the user.

The viewer is read-only in v1 (no editing, only deletion). If the user wants to correct a memory, they tell the agent in conversation and the agent saves a new corrected version.

#### 7.6.8 Slash Command

`/memory list` — shows the last 20 memories for the current thread's persona, displayed as an ephemeral message in the chat. Uses the same command endpoint as other slash commands.

#### 7.6.9 Limits and Safety

- **Max memory content length:** 500 characters per entry. The server truncates silently if the model tries to save a novel.
- **Max recall results:** 10 entries per query.
- **Max total memories per persona:** 500 entries. When the limit is reached, `save_memory` returns a tool result telling the model the memory store is full, and suggests it use `recall_memory` to find and decide what's still relevant. The memory viewer shows the count and a warning when approaching the limit (e.g., "423 / 500 memories"). This cap prevents duplicate or low-value entries from accumulating unboundedly — without deduplication in v1, a chatty model could easily generate hundreds of near-duplicate memories.
- **No automatic saving:** The model decides when to save. The system prompt instructs it, but there is no background process extracting memories from conversations. This keeps the system predictable and transparent.

#### 7.6.10 Future Expansion

This keyword-based memory system is intentionally simple. Future versions should explore:
- **Semantic/vector memory** using `sqlite-vec` and an embeddings API for recall that works on meaning rather than exact keywords
- **Memory categories** (preferences, facts, projects, relationships) for filtered recall
- **Automatic deduplication** via similarity scoring
- **Memory decay** — entries that are never recalled gradually lose priority in search results
- **Cross-persona memory** (opt-in) — a shared "facts about me" store that all personas can access

### 7.7 Model Providers

Each provider is stored in the `providers` table with a kind, base URL, and optional API key.

**Supported kinds in v1:**
- `copilot` — GitHub Copilot via local `copilot-api` proxy at `http://localhost:4141/v1`. No API key needed — auth is via GitHub device flow. The Rust server manages the `copilot-api` process lifecycle.
- `openai` — Direct OpenAI API. Requires API key.
- `anthropic` — Anthropic API. Requires API key. Base URL: `https://api.anthropic.com/v1`
- `custom` — Any OpenAI-compatible endpoint. Requires base URL and optionally an API key.

**API keys** are stored encrypted at rest in SQLite. The encryption key is derived from a machine-specific secret stored in the system keychain (macOS Keychain on the Mac mini).

**Copilot auth flow:**
1. User clicks "Connect GitHub Copilot" in provider settings
2. Server calls `copilot-api auth` which initiates GitHub device auth
3. A modal in the UI shows the device code and a link to github.com/login/device
4. The user authorizes on GitHub
5. `copilot-api` stores the token locally; the server marks Copilot as authenticated

**Copilot degradation behavior:** When the `copilot-api` process is unavailable (crashed, not started, auth expired), the system should degrade gracefully rather than fail silently:
- Provider status in the settings UI shows a red "Disconnected" badge with a reason ("Process not running", "Auth expired", "Connection refused")
- If a thread's active provider is Copilot and the process is down, sending a message returns a clear error via SSE: `event: error` with `"code": "PROVIDER_UNAVAILABLE"` and a human-readable message suggesting the user check provider settings or switch models
- Routines attached to threads using an unavailable provider skip execution and log a warning rather than failing hard — the routine's `last_run_at` is not updated, so it effectively retries on the next cron cycle
- The server does not block startup waiting for `copilot-api` — it starts the process asynchronously and marks Copilot as "connecting" until the health check passes

### 7.8 Push Notifications (Android/FCM)

Push notifications are sent when a routine fires and no SSE client is connected for that thread.

The Android app registers its FCM token with the server on startup via `POST /api/device-tokens`. The server stores it in the `device_tokens` table.

When a notification needs to be sent, the Rust server makes a direct HTTPS call to the FCM v1 API using a Firebase service account credential.

Notification payload:
- **Title:** The agent's name and emoji (e.g. "🦉 Aldous")
- **Body:** First 100 characters of the routine response
- **Data:** `thread_id` — the mobile app uses this to deep-link directly into the thread

### 7.9 First-Run Setup Wizard

When `GET /api/setup/status` returns `{ complete: false }`, the SPA shows a full-screen setup wizard instead of the main app. The wizard walks through:

1. **Welcome screen** — what agent-deck is, what you're about to set up
2. **Add a provider** — pick Copilot, OpenAI, Anthropic, or custom. Complete auth/key entry.
3. **Create your first persona** — name, emoji, system prompt (with a starter template), model selection
4. **Done** — calls `POST /api/setup/complete`, redirects to main chat UI

The wizard is skippable from step 2 onward — the user can always finish setup later from settings.

### 7.10 Mobile App Pairing

The mobile app needs to know the server's Tailscale address and the auth token.

On the server, `GET /api/pairing/qr` returns:
```json
{
  "server_url": "http://mac-mini.tailnet-name.ts.net:7474",
  "token": "<auth_token>"
}
```

This is displayed as a QR code in the browser UI under Settings → Mobile App.

The React Native app has a first-launch screen where the user taps "Scan QR Code", scans it, and the server URL + token are stored in `react-native-mmkv`. On every subsequent launch, the app connects automatically using the stored values.

---

## 8. UI/UX Specification

### 8.1 React SPA — Layout

The SPA is a single-page application with a persistent two-column layout on desktop:

```
┌─────────────────────────────────────────────────────┐
│  ● agent-deck          [persona avatar] [settings]  │  ← Top bar
├────────────────┬────────────────────────────────────┤
│                │                                    │
│  Thread List   │   Chat View / Settings Panel       │
│                │                                    │
│  [search]      │                                    │
│                │                                    │
│  ● Thread 1    │                                    │
│  ● Thread 2    │                                    │
│  ● Thread 3    │                                    │
│                │                                    │
│  [+ New Chat]  │                                    │
│                │                                    │
│  [Settings]    │                                    │
│  [Archived]    │                                    │
└────────────────┴────────────────────────────────────┘
```

### 8.2 Chat View

The chat view shows:
- Thread title (editable inline on click) at the top
- Agent name, emoji, and avatar in the header
- Messages list — user messages right-aligned, agent messages left-aligned with avatar
- Routine-generated messages visually distinct (subtle different background, small "routine" label)
- Streaming tokens animate in naturally as they arrive
- Input bar at the bottom with slash command support
- A config/settings icon in the top-right that opens the thread config pane

### 8.3 Thread Config Pane

Slides in from the right over the chat view. Sections:
- **Persona** — name, emoji, avatar (read-only)
- **Model** — dropdown of available models, shows current selection
- **Routines** — list with toggle, add button, edit/delete per routine. Routine-generated messages show a collapsed "Show work" disclosure in chat.
- **MCP Servers** — list of attached servers, each showing:
  - Name, status badge (connected / connecting / error), type badge (local / remote)
  - One-line description
  - Source URL link (if set)
  - Expandable tool list — each tool the server exposes with its description
  - Remove button
  - "+ Attach server" opens a searchable picker of all configured MCP servers
- **Tool Activity** — toggle: "Show tool activity in chat" (default: off). When on, hidden tool call messages render as collapsible disclosure rows.
- **System Prompt Addendum** — textarea, applied on top of persona prompt. Editable at any time; changes are noted in message history with a timestamp marker.

### 8.4 Settings / Management Pages

Accessible from the sidebar "Settings" link. Sections as sub-routes:

- `/settings/providers` — provider list, add/edit/delete, Copilot auth status
- `/settings/personas` — persona list, create/edit/delete, avatar upload, default MCP servers per persona, persona-owned Accounts tab
- `/settings/mcp-servers` — MCP server list with local/remote type, add/edit/delete, per-server tool inspector (tool names and descriptions), source URL display
- `/settings/accounts` — user-owned OAuth connections and API keys; connect Google (with scope selection), connect GitHub, add custom API key, disconnect
- `/settings/mobile` — QR code for mobile pairing
- `/settings/general` — server name, auth token rotation

**Accounts UI pattern (used in both `/settings/accounts` and persona settings):**
Each connected account shows: provider logo, display name, connected email or handle, granted scopes (as pills), and a Disconnect button. A "+ Connect account" button opens a provider picker. Selecting a provider with OAuth begins the redirect flow. For personas, the heading reads "{Persona name}'s accounts" to make the identity distinction clear.

### 8.5 React Native App — Screens

- **Scan Screen** — first launch only, QR code scanner for server pairing
- **Thread List Screen** — shows active threads with agent avatar, name, last message preview, timestamp
- **Chat Screen** — full chat UI matching SPA aesthetics, with streaming, agent avatar on every message
- **Thread Config Screen** — model switcher, view routines, view skills (modal or push navigation)
- **Archived Threads Screen** — accessible from thread list header menu

The mobile app does **not** include management screens (persona creation, provider setup, skill authoring). Those are browser-only in v1.

### 8.6 First-Run / Empty States

- Empty thread list: "No conversations yet. Tap + to start chatting."
- No personas configured: redirect to setup wizard
- No provider connected: show a banner with a link to provider settings
- Routine list empty: "No routines yet. Add one to schedule automated messages."

---

## 9. Development Guide

### 9.1 Repository Structure

```
agent-deck/
├── server/                  # Rust server (Cargo workspace member)
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   ├── db/
│   │   │   ├── mod.rs
│   │   │   └── migrations/  # SQLx migration files
│   │   ├── routes/          # Axum route handlers
│   │   ├── services/        # Business logic
│   │   │   ├── agent.rs     # Agent run-loop
│   │   │   ├── provider.rs  # Provider abstraction
│   │   │   ├── scheduler.rs # Routine cron scheduler
│   │   │   ├── memory.rs    # Memory tool implementations
│   │   │   └── skills.rs    # Skill tool implementations
│   │   ├── models/          # Rust structs matching DB tables
│   │   └── error.rs
│   ├── tests/               # Integration tests
│   ├── Cargo.toml
│   └── .env.example
├── web/                     # React SPA (Vite)
│   ├── src/
│   │   ├── main.tsx
│   │   ├── routes/          # React Router pages
│   │   ├── components/      # Shared UI components
│   │   ├── features/        # Feature-grouped components
│   │   ├── services/        # API client functions
│   │   ├── stores/          # Zustand stores
│   │   └── theme/
│   │       └── colors.ts    # Color tokens (source of truth)
│   ├── public/
│   ├── index.html
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   └── package.json
├── mobile/                  # React Native app
│   └── BotRelayApp/         # Existing scaffold (rename to AgentDeck)
│       ├── src/
│       │   ├── screens/
│       │   ├── services/
│       │   ├── theme/
│       │   │   └── colors.ts  # Same tokens as web/src/theme/colors.ts
│       │   └── types/
│       └── ...
├── Cargo.toml               # Workspace root
├── vendor/
│   └── copilot-api/         # Git submodule (ericc-ch/copilot-api)
├── mockups/                 # Static HTML mockups (visual reference for all UI)
│   ├── chat-view.html
│   ├── thread-config.html
│   ├── settings-providers.html
│   ├── settings-personas.html
│   ├── settings-other.html
│   ├── setup-wizard.html
│   ├── slash-commands.html
│   ├── empty-states.html
│   ├── token-entry.html
│   ├── mobile-thread-list.html
│   ├── mobile-chat.html
│   ├── mobile-thread-config.html
│   └── mobile-pairing.html
├── PLAN.md                  # This file
└── README.md
```

### 9.2 Git Workflow

- **Main branch** (`main`) — always in a working, tested state. No direct commits.
- **Feature branches** — all work happens on branches. Branch naming:
  - `feature/<phase>-<short-description>` — new features
  - `fix/<short-description>` — bug fixes
  - `chore/<short-description>` — dependency updates, config, non-functional changes

- **Commit granularity** — commit after each logical task within a story, not just once at the end. A "logical task" is a self-contained unit of work that leaves the codebase in a coherent state (e.g. "server contract change", "toast component", "slash dropdown UI", "ChatView ephemeral messages"). This keeps the history readable and makes bisection easy. Commit format: `feat(phaseN): <short description>` for features, `fix(phaseN): <short description>` for fixes within a phase branch. Never batch unrelated changes into one commit.

Examples:
```
feature/phase1-cargo-workspace-setup
feature/phase1-sqlite-schema-and-migrations
feature/phase2-agent-runloop-streaming
feature/phase3-routine-scheduler
fix/sse-stream-disconnect-handling
chore/update-axum-to-0-8
```

- Each feature branch corresponds to **one story** in the phase plan. Never combine multiple stories on a single branch.
- Branches are merged to main only when:
  1. The feature is complete per its acceptance criteria
  2. All tests pass
  3. No regressions in related areas
- **Branches are never deleted** — not after merging, not ever. Every feature branch must remain accessible for reference, bisection, and history. Do not pass `--delete` or `-d` / `-D` to `git branch`, and do not use `git push origin --delete`.

### 9.3 Debugging and Bug Fixes

When a bug or unexpected behaviour is discovered during development, the same plan-before-code discipline that applies to stories applies to fixes:

1. **Diagnose first.** Gather evidence — logs, `ps`, `lsof`, `git log`, source reading — until the root cause is understood. Do not guess.
2. **State the root cause clearly.** Write one or two sentences explaining exactly what is wrong and why, before touching any code.
3. **Propose the fix and wait for confirmation.** Describe what you intend to change and why it addresses the root cause. Stop and wait for the human to confirm before writing any code.
4. **Then implement.** Only after explicit confirmation, make the change, verify it builds and tests pass, and commit with a `fix:` prefix.

This rule applies even for "obvious" single-line fixes. The cost of a 30-second plan confirmation is always lower than the cost of a wrong or incomplete fix that has to be reverted.

### 9.3 Testing Philosophy

The goal is not 100% coverage — it is confidence that critical logic works and regressions are caught.

**What must have tests:**
- All database query functions (unit tests with an in-memory SQLite DB)
- Agent run-loop logic (context assembly, slash command parsing, tool call handling)
- Provider abstraction layer (mock HTTP responses, test error handling)
- Routine scheduler (test that jobs fire, test enable/disable, test pause on archive)
- All REST API endpoints (integration tests using `axum::test` or `reqwest` against a test server)
- Memory tool search logic

**What does not need tests:**
- Pure configuration structs
- Simple CRUD that is already covered by endpoint tests
- React component rendering (unless the component has meaningful logic)
- Tailwind class names

**Test file conventions:**
- Rust: `#[cfg(test)]` modules within the same file for unit tests, `server/tests/` for integration tests
- React/TypeScript: `*.test.ts` or `*.test.tsx` co-located with the file being tested
- React Native: same convention, in `mobile/BotRelayApp/src/__tests__/` or co-located

### 9.4 Environment Setup

**Prerequisites:**
- Rust (latest stable via `rustup`)
- Node.js 22+
- Bun (for copilot-api)
- Android Studio + JDK 17 + Android SDK (for mobile)
- SQLx CLI: `cargo install sqlx-cli --features sqlite`

**Server setup:**
```bash
cd server
cp .env.example .env
sqlx database create
sqlx migrate run
cargo run
```

**SQLx offline mode:** SQLx's compile-time query checking requires a live database connection during `cargo build`. To allow builds without a running database (e.g., CI, fresh clones), use offline mode:
1. After any migration change, run `cargo sqlx prepare` to generate query metadata in a `.sqlx/` directory
2. Commit the `.sqlx/` directory to git
3. Set `SQLX_OFFLINE=true` in CI environments
4. The `.env` file with `DATABASE_URL` is still required for local development with live checking

**Web setup:**
```bash
cd web
npm install
npm run dev   # dev server proxies /api to localhost:7474
```

**Mobile setup:**
```bash
cd mobile/BotRelayApp
npm install
npx react-native run-android
```

**Environment variables (server/.env):**
```
PORT=7474
DATABASE_URL=sqlite:./data/agent-deck.db
RUST_LOG=info
FCM_SERVICE_ACCOUNT_JSON=./config/firebase-service-account.json
```

### 9.5 Frontend Component Design Principles

These principles apply to the React SPA. They exist to keep the codebase maintainable as the UI grows in complexity across phases.

**Build chrome separately from content.**
Any UI pattern that has a reusable visual frame — a modal shell, a wizard overlay, a slide-in pane — must separate the chrome (the frame, header, transitions, layout) from the content (the step or page rendered inside it). The chrome goes in a shared component; only the content is specific to the feature. This prevents the pattern of rebuilding the same visual container for every new instance of a similar UI.

Examples:
- `wizards/shared/WizardShell` owns the full-screen overlay, brand header, step indicator, and card — `setup-wizard/` only provides step content
- `SettingsModal` owns the modal shell and tab nav — individual settings panels (`ProviderSettings`, `PersonaSettings`, etc.) only provide their own content

**Shared components must be independently usable.**
A component in a `shared/` directory must have no knowledge of the specific feature it was first built for. If a shared component imports from a sibling feature directory, it is not actually shared — it is a coupled component living in the wrong place. Acceptance criteria for any story that introduces shared components must explicitly verify this.

**Co-locate feature components.**
Components that are only ever used by one feature live next to that feature, not in a global `components/` directory. Global `components/` is for things used in three or more unrelated places. This keeps the blast radius of feature changes small and makes it obvious what can be deleted when a feature is removed.

**Directory structure for the SPA:**
```
web/src/
  components/          # Truly global, used across 3+ unrelated features
  components/wizards/
    shared/            # Wizard chrome — WizardShell, WizardStepIndicator, WizardNavRow, WizardCard, types.ts
    setup-wizard/      # Setup wizard steps — only used by the setup flow
  components/settings/ # Settings modal chrome and all settings panels
  stores/              # Zustand stores — one file per domain
  api/                 # Typed API client — one file, all endpoints
  types/               # Shared TypeScript types
```

**Props over context for shared components.**
Shared chrome components receive everything they need via props. They do not reach into Zustand stores or call API functions directly. This makes them trivially testable and reusable in any context.

---


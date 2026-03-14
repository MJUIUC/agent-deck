# Agent-Deck — Project Plan

**Version:** 1.5  
**Project:** agent-deck  
**Purpose:** A self-hosted, highly configurable personal AI agent platform designed to make working with LLMs accessible to non-engineers. Runs on a Mac mini, accessible privately over Tailscale, with a browser UI and Android mobile app.

**v1.5 Changes:**
- Closed Phase 3 — all configuration and management stories complete; app is fully usable for chat with no curl required
- Removed credential store (3.x), OAuth (3.y, 3.z), and Story 3.3 Delta (persona default MCP servers) from Phase 3 — deferred to Phase 4
- Restructured Phase 4: credentials and encryption first, then MCP integration (some MCP servers require credentials), then memory and routines
- Renamed Phase 4 from "Memory and Routines" to "Credentials, MCP, Memory, and Routines"
- Story 3.8 (pending thread + server-side title generation) complete and verified working
- Fixed title generation trigger condition (== 2 messages, not <= 2)
- Replaced async_openai HTTP client with raw reqwest in list_models to eliminate spurious ERROR logs from providers that omit non-standard fields

**v1.4 Changes:**
- Removed Skills system entirely (`skills` table, `thread_skills`, `/api/skills`, Phase 7 skills stories, all related UI)
- MCP is now the primary capability layer: local and remote server types, per-persona default MCP servers, tool inspector UI
- Added credential system: encrypted credential store, OAuth2 provider framework (Google + GitHub), agent-owned persona credentials, modular provider pattern for future integrations
- Routine execution revised to two-phase model: silent background execution → single synthesized output message; intermediate steps stored but hidden by default
- Added `visibility` field to messages (`visible` | `hidden`); tool calls and MCP results hidden by default, optionally surfaced via Thread Config toggle
- Added new tables: `credentials`, `persona_default_mcp_servers`, `routine_executions`
- Removed tables: `skills`, `thread_skills`
- Revised `mcp_servers` schema: added `server_type` (local/remote), `source_url`, unified `config` JSON column
- Updated Phase 3: added credential store + OAuth stories (3.x, 3.y, 3.z); removed skills settings story
- Replaced Phase 7 (Skills Experimental) with Phase 7 (MCP Depth): tool inspector, local process management
- Updated Thread Config pane spec: removed Skills section, expanded MCP section with tool inspector and local/remote distinction
- Updated Settings spec: removed skills page, added Accounts tab to global and persona settings

**v1.3 Changes:**
- Added full browser authentication model (section 6.0): localhost bypass, token entry screen for remote Tailscale devices, cookie-based session, auth endpoints
- Added copilot-api graceful degradation behavior (section 7.7): UI error states, SSE error events, routine skip-on-unavailable
- Added 500-entry memory cap per persona (section 7.6.9) to prevent unbounded growth without deduplication
- Added SQLx offline mode documentation (section 9.4) for CI and fresh-clone builds
- Added token-entry.html to mockups
- Fixed `etch` → `fetch` typo in tech stack
- Added explicit Firebase prerequisite callout on Phase 6
- Expanded Story 1.4 (auth middleware) to cover cookie auth, localhost bypass, and token logging

**v1.2 Changes:**
- Added detailed memory implementation spec (sections 7.6.1–7.6.10): tool schemas, system prompt injection, content format, recall behavior, memory viewer, limits
- Added memory REST API endpoints (section 6.11)
- Added `/memory list` slash command
- Added Story 1.9: UI mockups for all screens (static HTML with interactions) as approved visual reference before implementation
- All UI implementation stories now reference their corresponding mockup files

**v1.1 Changes:**
- Reorganized phased execution plan — phases now interleave backend and frontend for earlier dogfooding
- Memory scoped to user + persona (cross-thread, per-agent personality) with thread provenance metadata
- Slash commands handled via dedicated server endpoint (`POST /api/threads/:id/command`), intercepted client-side
- copilot-api included as a vendor submodule
- Setup wizard moved to Phase 3 (bootstrap via API during early development)

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Architecture](#2-architecture)
3. [Technology Stack](#3-technology-stack)
4. [Shared Theme](#4-shared-theme)
5. [Data Model](#5-data-model)
6. [API Contract](#6-api-contract)
7. [Feature Specifications](#7-feature-specifications)
8. [UI/UX Specification](#8-uiux-specification)
9. [Development Guide](#9-development-guide)
10. [Phased Execution Plan](#10-phased-execution-plan)
11. [Deferred / Future Work](#11-deferred--future-work)

---

## 1. Project Overview

Agent-Deck is a self-hosted personal AI agent platform. The goal is to make working with large language models easy, private, and configurable — without requiring engineering knowledge to set up or maintain.

**The problem it solves:** Using LLMs today either means trusting a third-party platform with your conversation history, or being an engineer who can stitch together APIs, manage keys, and build tooling yourself. Agent-Deck closes that gap. It runs on hardware you own (a Mac mini), communicates only over your private Tailscale network, persists everything locally in SQLite, and presents a polished interface that non-engineers can use confidently.

**Core principles:**
- Private by default — all data stays on your hardware, all traffic over Tailscale
- Obvious UX — configuration should feel natural, not technical
- Highly configurable — personas, models, routines, tools, skills, mcp's, all tunable
- Single user — one person per installation (multi-user is a future consideration)
- Persistent — everything survives restarts; nothing is lost between sessions

**Primary hardware target:** Mac mini (always-on home server)  
**Network:** Tailscale (private mesh network)  
**Default port:** 7474

---

## 2. Architecture

### 2.1 System Diagram

```
┌─────────────────────────────────────────────────────────┐
│                     Tailscale Network                    │
│                                                         │
│  ┌──────────────────┐         ┌──────────────────────┐  │
│  │   Browser        │         │  Android App         │  │
│  │   React SPA      │         │  React Native        │  │
│  │   (any device)   │         │  (Pixel 4a)          │  │
│  └────────┬─────────┘         └──────────┬───────────┘  │
│           │ HTTP + SSE                   │ HTTP + SSE   │
│           │                              │              │
│  ┌────────▼──────────────────────────────▼───────────┐  │
│  │              Rust Server (port 7474)               │  │
│  │                                                    │  │
│  │  ┌─────────────┐  ┌──────────────┐  ┌──────────┐  │  │
│  │  │  Axum HTTP  │  │  Agent       │  │  Cron    │  │  │
│  │  │  + SSE      │  │  Run-Loop    │  │  Scheduler│  │  │
│  │  └─────────────┘  └──────┬───────┘  └────┬─────┘  │  │
│  │                          │               │         │  │
│  │  ┌───────────────────────▼───────────────▼──────┐  │  │
│  │  │              SQLite Database                  │  │  │
│  │  └───────────────────────────────────────────────┘  │  │
│  │                                                    │  │
│  │  ┌─────────────────────┐  ┌─────────────────────┐  │  │
│  │  │  Provider Abstraction│  │  Static File Server │  │  │
│  │  │  Layer               │  │  (React SPA build)  │  │  │
│  │  └──────────┬──────────┘  └─────────────────────┘  │  │
│  └─────────────┼──────────────────────────────────────┘  │
│                │                                         │
│  ┌─────────────▼──────────┐                              │
│  │  copilot-api process   │                              │
│  │  (Node/Bun, port 4141) │                              │
│  │  Only runs when Copilot│                              │
│  │  provider is active    │                              │
│  └────────────────────────┘                              │
└─────────────────────────────────────────────────────────┘
                        │
                        │ HTTPS (FCM)
                        ▼
              Google Firebase (FCM)
              Push notifications only
```

### 2.2 Process Relationships

The Rust server is the central process. It:
- Serves the React SPA as static files from its `public/` directory
- Exposes a REST + SSE API consumed by both the browser and mobile app
- Owns the agent run-loop — LLM calls happen here, not on the client
- Manages the `copilot-api` child process (starts it, monitors it, restarts if it crashes)
- Owns the cron scheduler for routines — routines deliver invocations via the system notification channel (not direct function calls)
- Dispatches FCM push notifications when no SSE client is connected for a thread
- Enforces a per-thread agent run lock — only one agent run may execute per thread at a time; additional runs queue behind it (see section 7.x)

### 2.3 Communication Patterns

- **Client → Server:** Standard HTTP REST (POST, GET, PUT, DELETE, PATCH)
- **Server → Client (streaming):** Server-Sent Events (SSE) — used for LLM token streaming and live event delivery (new messages, routine completions, system events)
- **Server → Mobile (background):** Firebase Cloud Messaging (FCM) push notifications
- **Server → LLM Provider:** HTTP via provider abstraction layer (OpenAI-compatible API)
- **Routine → Agent:** Via `POST /api/threads/:id/notify` with `event_type: routine_fired` — same path as all other system notifications
- **Config change → Agent:** Client calls `POST /api/threads/:id/notify` after any mutation the agent should be aware of (model switch, MCP attach/detach, addendum update)

### 2.4 SSE Event Streams

Two SSE endpoints exist:

**`GET /api/threads/:id/stream`** — per-thread event stream
- `token` — a single streamed LLM token (chat streaming)
- `message_complete` — full message object once streaming is done
- `routine_message` — a new message produced by a routine firing
- `system_event` — a system notification was inserted into the thread (for UI to render if `show_system_events` is on)
- `error` — streaming error

**`GET /api/events`** — global event stream (for thread list updates, notifications)
- `thread_updated` — last message preview or unread count changed
- `routine_fired` — a routine ran (includes thread_id for navigation)

---

## 3. Technology Stack

### 3.1 Rust Server

| Concern | Crate | Reason |
|---|---|---|
| HTTP framework | `axum` | Ergonomic, async-first, tower middleware ecosystem |
| Async runtime | `tokio` | Standard async runtime for Rust |
| Database | `sqlx` | Async SQLite driver with compile-time query checking |
| LLM HTTP client | `async-openai` | OpenAI-compatible API client, points at any base URL |
| SSE | `axum` built-in | Native SSE support via `axum::response::Sse` |
| Cron scheduling | `tokio-cron-scheduler` | Async cron scheduler built on tokio |
| Serialization | `serde` + `serde_json` | Standard Rust serialization |
| Config / env | `dotenvy` | `.env` file loading |
| UUID generation | `uuid` | v4 UUIDs |
| Error handling | `anyhow` + `thiserror` | Ergonomic error types |
| Logging | `tracing` + `tracing-subscriber` | Structured async logging |
| FCM | `fcm` or direct HTTP via `reqwest` | Push notification dispatch |
| Child process mgmt | `tokio::process` | Managing the copilot-api process |

### 3.2 React SPA (Browser Frontend)

| Concern | Library | Reason |
|---|---|---|
| Framework | React 19 + TypeScript | Standard, well-supported |
| Build tool | Vite | Fast dev server and build |
| Styling | Tailwind CSS | Utility-first, works well with design tokens |
| Components | shadcn/ui | Composable, Tailwind-based, unstyled base |
| Routing | React Router v7 | SPA routing |
| State management | Zustand | Lightweight, simple API |
| HTTP client | `fetch` (native) or `axios` | REST calls |
| SSE client | Native `EventSource` API | Browser-native SSE |
| Chat UI | Custom components | Built on shadcn primitives |
| Icons | `lucide-react` | Clean icon set used by shadcn |
| Forms | `react-hook-form` + `zod` | Validation and form state |

### 3.3 React Native App (Android)

The mobile app directory already exists at `mobile/BotRelayApp/`. It was scaffolded for the previous bot-relay project. Key existing dependencies to keep:

| Dependency | Purpose |
|---|---|
| `react-native-gifted-chat` | Chat UI component |
| `@react-navigation/native` + `native-stack` | Navigation |
| `react-native-mmkv` | Fast local storage (replaces AsyncStorage) |
| `react-native-safe-area-context` | Safe area handling |
| `axios` | HTTP client |
| `@notifee/react-native` | Local notification display |

Dependencies to **remove** (no longer needed):
- `socket.io-client` — replaced by SSE
- `tweetnacl` + `tweetnacl-util` — encryption no longer needed at app layer
- `react-native-video` — not applicable

New dependencies to **add**:
- `@react-native-firebase/app` + `@react-native-firebase/messaging` — FCM push notifications

The app name should be renamed from `BotRelayApp` to `AgentDeck` in `app.json` and `package.json`.

### 3.4 copilot-api

- **Repository:** https://github.com/ericc-ch/copilot-api
- **Included as:** Git vendor submodule at `vendor/copilot-api/`
- **Runtime:** Bun
- **Port:** 4141 (localhost only, never exposed externally)
- **Role:** Acts as a local OpenAI-compatible proxy for GitHub Copilot
- **Managed by:** Rust server as a child process via `tokio::process`
- **Auth:** GitHub device auth flow triggered from the provider management UI
- **Dependency risk:** This is a third-party dependency. Pinned as a submodule so upstream changes don't break the project. If the project stops working, we address it then — there is no alternative for Copilot access outside of official surfaces.

---

## 4. Shared Theme

Both the React SPA and React Native app must use the same named color tokens. Colors are defined once and imported in both projects. When a color needs to change, it changes in one place.

### 4.1 Color Tokens

```
// theme/colors.ts  (shared reference — duplicate into each project)

const colors = {
  // Backgrounds
  bg_primary:        '#1C1C1A',   // near-black, main app background
  bg_secondary:      '#242422',   // slightly lighter, sidebar / panels
  bg_tertiary:       '#2E2E2B',   // cards, input backgrounds
  bg_elevated:       '#383835',   // modals, popovers, hover states

  // Olive accent
  accent_primary:    '#7C8C5A',   // olive green, primary actions
  accent_secondary:  '#9AAD6E',   // lighter olive, hover states
  accent_muted:      '#4A5235',   // dark olive, subtle highlights

  // Text
  text_primary:      '#F0EDE4',   // warm white/cream, primary text
  text_secondary:    '#B8B4A8',   // muted, secondary labels
  text_tertiary:     '#7A7870',   // very muted, placeholders, hints
  text_inverse:      '#1C1C1A',   // dark text on light/olive backgrounds

  // Semantic
  success:           '#6A9E5B',   // green
  warning:           '#C4A24A',   // amber
  error:             '#C45A5A',   // muted red
  info:              '#5A82C4',   // muted blue

  // Borders
  border_subtle:     '#333330',   // very subtle dividers
  border_default:    '#444440',   // standard borders
  border_strong:     '#666660',   // prominent borders

  // Chat specific
  bubble_user:       '#4A5235',   // user message bubble (dark olive)
  bubble_agent:      '#2E2E2B',   // agent message bubble (bg_tertiary)
  bubble_routine:    '#2A3545',   // routine-generated message (muted blue-grey)
}
```

### 4.2 Usage Rules

- Never hardcode hex values in component files — always use token names
- All Tailwind classes in the SPA must map to these tokens via `tailwind.config.ts`
- All React Native `StyleSheet` entries must import from the shared colors file
- If a color doesn't look right, change the token value, not the component

---

## 5. Data Model

All tables include `created_at` as ISO 8601 UTC string. Foreign keys are enforced. SQLite WAL mode enabled.

### 5.1 Tables

#### `users`
One row — always the same single user. Exists for future multi-user expansion.

```sql
CREATE TABLE users (
  id           TEXT PRIMARY KEY,          -- UUID v4
  display_name TEXT NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
```

#### `providers`
Model provider configurations. Each provider is an OpenAI-compatible endpoint.

```sql
CREATE TABLE providers (
  id           TEXT PRIMARY KEY,          -- UUID v4
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,             -- e.g. "GitHub Copilot", "OpenAI", "Anthropic"
  kind         TEXT NOT NULL,             -- 'copilot' | 'openai' | 'anthropic' | 'custom'
  base_url     TEXT NOT NULL,             -- e.g. "http://localhost:4141/v1"
  api_key      TEXT,                      -- encrypted at rest, null for copilot (uses proxy auth)
  enabled      INTEGER NOT NULL DEFAULT 1,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);
```

#### `models`
Available models per provider. Populated by calling the provider's `/v1/models` endpoint.

```sql
CREATE TABLE models (
  id           TEXT PRIMARY KEY,          -- UUID v4
  provider_id  TEXT NOT NULL,
  model_id     TEXT NOT NULL,             -- e.g. "gpt-4o", "claude-3-5-sonnet-20241022"
  display_name TEXT NOT NULL,
  enabled      INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (provider_id) REFERENCES providers(id)
);
```

#### `agent_personas`
Reusable agent configurations. A persona defines who the agent is.

```sql
CREATE TABLE agent_personas (
  id              TEXT PRIMARY KEY,       -- UUID v4
  user_id         TEXT NOT NULL,
  name            TEXT NOT NULL,          -- e.g. "Aldous"
  emoji           TEXT NOT NULL,          -- e.g. "🤖"
  avatar_path     TEXT,                   -- path to uploaded avatar image file
  system_prompt   TEXT NOT NULL,          -- the persona's core instructions
  default_model   TEXT,                   -- model_id FK
  default_provider TEXT,                  -- provider_id FK
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id),
  FOREIGN KEY (default_model) REFERENCES models(id),
  FOREIGN KEY (default_provider) REFERENCES providers(id)
);
```

#### `persona_default_mcp_servers`
MCP servers that are automatically attached to every new thread created with a given persona.

```sql
CREATE TABLE persona_default_mcp_servers (
  persona_id    TEXT NOT NULL,
  mcp_server_id TEXT NOT NULL,
  PRIMARY KEY (persona_id, mcp_server_id),
  FOREIGN KEY (persona_id) REFERENCES agent_personas(id) ON DELETE CASCADE,
  FOREIGN KEY (mcp_server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE
);
```

#### `mcp_servers`
MCP server configurations. Supports two server types: `local` (process managed by the Rust server) and `remote` (externally hosted HTTP/SSE endpoint).

```sql
CREATE TABLE mcp_servers (
  id           TEXT PRIMARY KEY,          -- UUID v4
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,
  description  TEXT,                      -- short description shown in UI
  source_url   TEXT,                      -- GitHub repo or docs link, informational only
  server_type  TEXT NOT NULL CHECK (server_type IN ('local', 'remote')),
  config       TEXT NOT NULL,             -- JSON, shape varies by server_type:
                                          --   local:  { "executable": "path", "args": [], "env": {} }
                                          --   remote: { "url": "https://...", "auth_header": "Authorization", "credential_key": "google_oauth" }
  status       TEXT NOT NULL DEFAULT 'inactive', -- 'inactive' | 'connecting' | 'connected' | 'error'
  enabled      INTEGER NOT NULL DEFAULT 1,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);
```

#### `threads`
A chat session. Locked to one persona at creation time.

```sql
CREATE TABLE threads (
  id              TEXT PRIMARY KEY,       -- UUID v4
  user_id         TEXT NOT NULL,
  persona_id      TEXT NOT NULL,          -- locked at creation, never changes
  title           TEXT NOT NULL,          -- auto-generated from first message, editable
  active_model    TEXT,                   -- overrides persona default if set (FK to models.id)
  active_provider TEXT,                   -- overrides persona default if set (FK to providers.id)
  system_prompt_addendum TEXT,            -- thread-level addition to persona system prompt
  show_tool_activity INTEGER NOT NULL DEFAULT 0,  -- reveal hidden tool messages in chat UI
  show_system_events INTEGER NOT NULL DEFAULT 0,  -- reveal hidden system event messages in chat UI
  status          TEXT NOT NULL DEFAULT 'active', -- 'active' | 'archived'
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id),
  FOREIGN KEY (persona_id) REFERENCES agent_personas(id),
  FOREIGN KEY (active_model) REFERENCES models(id),
  FOREIGN KEY (active_provider) REFERENCES providers(id)
);
```

#### `thread_mcp_servers`
MCP servers enabled for a specific thread.

```sql
CREATE TABLE thread_mcp_servers (
  id            TEXT PRIMARY KEY,         -- UUID v4
  thread_id     TEXT NOT NULL,
  mcp_server_id TEXT NOT NULL,
  enabled       INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (thread_id) REFERENCES threads(id),
  FOREIGN KEY (mcp_server_id) REFERENCES mcp_servers(id),
  UNIQUE(thread_id, mcp_server_id)
);
```

#### `messages`
All messages in all threads. Includes both user and agent messages.

```sql
CREATE TABLE messages (
  id           TEXT PRIMARY KEY,          -- UUID v4
  thread_id    TEXT NOT NULL,
  role         TEXT NOT NULL,             -- 'user' | 'assistant' | 'system' | 'tool'
  content      TEXT NOT NULL,
  source       TEXT NOT NULL DEFAULT 'chat', -- 'chat' | 'routine' | 'system_event'
  routine_id   TEXT,                      -- set if source = 'routine'
  visibility   TEXT NOT NULL DEFAULT 'visible' CHECK (visibility IN ('visible', 'hidden')),
                                          -- 'hidden' for tool calls, MCP results, routine intermediate steps, and system events
  execution_id TEXT,                      -- references routine_executions(id) for routine-generated messages
  event_type   TEXT,                      -- set if source = 'system_event', e.g. 'model_switched'
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (thread_id) REFERENCES threads(id),
  FOREIGN KEY (routine_id) REFERENCES routines(id),
  FOREIGN KEY (execution_id) REFERENCES routine_executions(id)
);

CREATE INDEX idx_messages_thread_created ON messages(thread_id, created_at);
```

#### `routines`
Scheduled prompts attached to a thread.

```sql
CREATE TABLE routines (
  id             TEXT PRIMARY KEY,        -- UUID v4
  thread_id      TEXT NOT NULL,
  name           TEXT NOT NULL,
  prompt         TEXT NOT NULL,           -- injected as user message when fired
  cron_expr      TEXT NOT NULL,           -- cron expression e.g. "0 9 * * *"
  enabled        INTEGER NOT NULL DEFAULT 1,
  run_count      INTEGER NOT NULL DEFAULT 0,
  last_run_at    TEXT,
  next_run_at    TEXT,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (thread_id) REFERENCES threads(id)
);
```

#### `device_tokens`
FCM device tokens for push notifications.

```sql
CREATE TABLE device_tokens (
  id           TEXT PRIMARY KEY,          -- UUID v4
  user_id      TEXT NOT NULL,
  token        TEXT NOT NULL UNIQUE,      -- FCM device token
  platform     TEXT NOT NULL DEFAULT 'android',
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);
```

#### `memory`
Persistent memory entries scoped to a user and agent persona. Each persona maintains its own memory across all threads — an agent "knows you" regardless of which thread a memory was created in. The `thread_id` is retained as provenance metadata (where the memory originated) but does not restrict access.

```sql
CREATE TABLE memory (
  id           TEXT PRIMARY KEY,          -- UUID v4
  user_id      TEXT NOT NULL,
  persona_id   TEXT NOT NULL,             -- memory is scoped per-persona
  thread_id    TEXT,                      -- provenance: where the memory was created (nullable for manual entries)
  content      TEXT NOT NULL,             -- the memory content
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id),
  FOREIGN KEY (persona_id) REFERENCES agent_personas(id),
  FOREIGN KEY (thread_id) REFERENCES threads(id)
);

-- Full-text search index for memory recall
CREATE VIRTUAL TABLE memory_fts USING fts5(
  content,
  content='memory',
  content_rowid='rowid'
);
```

#### `credentials`
Encrypted credential store for OAuth tokens and API keys. Credentials are never exposed to the agent directly — MCP servers resolve them at runtime by key name.

```sql
CREATE TABLE credentials (
  id              TEXT PRIMARY KEY,       -- UUID v4
  key             TEXT NOT NULL UNIQUE,   -- referenced by MCP server configs, e.g. "google_oauth"
  display_name    TEXT NOT NULL,          -- shown in settings UI
  provider        TEXT NOT NULL,          -- 'google' | 'github' | 'custom'
  credential_type TEXT NOT NULL CHECK (credential_type IN ('oauth2', 'api_key', 'custom')),
  owner_type      TEXT NOT NULL CHECK (owner_type IN ('user', 'persona')),
  persona_id      TEXT,                   -- null if owner_type = 'user'; references agent_personas(id)
  encrypted_data  TEXT NOT NULL,          -- AES-256-GCM encrypted JSON blob
  scopes          TEXT,                   -- JSON array of granted OAuth scopes
  expires_at      TEXT,                   -- for OAuth access tokens; null for API keys
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (persona_id) REFERENCES agent_personas(id)
);
```

Encryption key: a 256-bit master key is generated on first server run, stored in `app_config` as `credential_master_key`. All credential blobs are encrypted with AES-256-GCM. The key never leaves the server process and is never exposed via API.

#### `routine_executions`
Execution log for routine runs. Each run gets one row. Intermediate hidden messages reference this table.

```sql
CREATE TABLE routine_executions (
  id                TEXT PRIMARY KEY,     -- UUID v4
  routine_id        TEXT NOT NULL,
  thread_id         TEXT NOT NULL,
  fired_at          TEXT NOT NULL,
  status            TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'completed', 'failed')),
  output_message_id TEXT,                 -- the final visible result message in threads
  error             TEXT,                 -- set on failure
  completed_at      TEXT,
  FOREIGN KEY (routine_id) REFERENCES routines(id),
  FOREIGN KEY (thread_id) REFERENCES threads(id),
  FOREIGN KEY (output_message_id) REFERENCES messages(id)
);
```

#### `app_config`
Single-row key/value store for application settings.

```sql
CREATE TABLE app_config (
  key          TEXT PRIMARY KEY,
  value        TEXT NOT NULL,
  updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
```

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
| `DELETE` | `/api/credentials/:id` | Delete a credential |
| `GET` | `/api/auth/oauth/:provider/start` | Begin OAuth flow — returns redirect URL |
| `GET` | `/api/auth/oauth/callback` | OAuth callback endpoint (provider, code, state params) |
| `POST` | `/api/credentials/api-key` | Store an API key credential |

OAuth flow providers: `google`, `github`. Additional providers are registered in the provider registry without API changes.

**POST /api/credentials/api-key body:**
```json
{
  "key": "openai_key",
  "display_name": "OpenAI API Key",
  "provider": "custom",
  "owner_type": "user",
  "persona_id": null,
  "secret": "sk-..."
}
```

**GET /api/credentials response** (never returns raw tokens):
```json
{
  "data": [
    {
      "id": "<id>",
      "key": "google_oauth",
      "display_name": "Your Google Account",
      "provider": "google",
      "credential_type": "oauth2",
      "owner_type": "user",
      "persona_id": null,
      "scopes": ["gmail.readonly", "calendar.readonly"],
      "expires_at": "2026-04-01T00:00:00Z",
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
data: {"id": "<id>", "thread_id": "<id>", "role": "assistant", "content": "Hello! How can I help?", "created_at": "..."}

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

Incoming runs that cannot immediately acquire the semaphore queue behind it (Tokio semaphores handle this natively — no manual queue needed). If the queue depth exceeds a configurable limit (default: 3), the request is rejected with `429 Too Many Requests` and a clear message ("This thread is busy — try again in a moment").

**Client-side:** The message input is already disabled while `isSending || isStreaming`. This covers the common case of a user trying to send while a response is in progress. The server lock is the correctness guarantee; the client disable is the UX affordance.

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

Two tools are always included in every LLM request (for providers that support function calling):

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

Every persona's system prompt has the following memory instructions appended automatically by the agent run-loop. These are not editable by the user — they are injected by the system after the persona's custom system prompt:

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

## 10. Phased Execution Plan

Each story maps to one feature branch. Complete all stories in a phase before starting the next. Run all tests before merging any branch.

**Guiding principle:** Each phase ends with something usable or demonstrable. Backend and frontend are interleaved so you can dogfood early.

---

### Phase 1 — Skeleton That Runs

**Goal:** A Rust server that boots with SQLite, serves a React SPA shell, and has all tables in place. Nothing functional yet, but the entire foundation is solid and both projects compile.

---

**Story 1.1 — Cargo workspace and project scaffolding**  
Branch: `feature/phase1-cargo-workspace`

Set up the Cargo workspace with a single `server` member. Add all production dependencies to `Cargo.toml`. Create the directory structure under `server/src/`. Add `.env.example`. Verify the project compiles with `cargo build`.

Acceptance criteria:
- `cargo build` succeeds with no errors
- Directory structure matches the spec in section 9.1
- All crates listed in section 3.1 are present in `Cargo.toml`

---

**Story 1.2 — SQLite schema and migrations**  
Branch: `feature/phase1-sqlite-schema`

Create all SQLx migration files under `server/src/db/migrations/`. Include all tables from section 5.1 (including the updated `memory` table with `persona_id`). Enable WAL mode and foreign keys. Write a database module that initializes the connection pool.

Acceptance criteria:
- `sqlx migrate run` completes without errors
- All tables from section 5.1 exist with correct columns and constraints
- FTS5 virtual table for memory is created
- `memory` table includes `persona_id` column with FK to `agent_personas`
- Unit test: database initializes cleanly with an in-memory SQLite instance

---

**Story 1.3 — Configuration and server bootstrap**  
Branch: `feature/phase1-server-bootstrap`

Load config from `.env` using `dotenvy`. Set up `tracing` for structured logging. Create the main `axum` router. Add a health check endpoint at `GET /health`. Serve static files from a configurable `public/` directory. Start the server on port 7474.

Acceptance criteria:
- `GET /health` returns `200 OK`
- Static file serving works (place a test `index.html` in `public/`)
- Server starts and logs the port it's listening on

---

**Story 1.4 — Auth middleware**  
Branch: `feature/phase1-auth-middleware`

Implement the authentication system per section 6.0. Bearer token auth middleware that checks the `Authorization` header OR the `agent_deck_session` cookie. Token is stored in `app_config` table under key `auth_token`. Generate a random 64-character hex token on first run if none exists, log it to the terminal at startup. Localhost requests (origin `127.0.0.1` or `::1`) bypass auth entirely. Implement `POST /api/auth/token` (validates token, sets `httpOnly` cookie) and `POST /api/auth/logout` (clears cookie). All `/api/*` routes require auth except `GET /api/setup/status` and `POST /api/auth/token`.

Acceptance criteria:
- Requests without a valid token or cookie return `401`
- Requests with a valid Bearer header pass through
- Requests with a valid session cookie pass through
- Localhost requests bypass auth entirely
- `POST /api/auth/token` with correct token sets cookie and returns success
- `POST /api/auth/token` with incorrect token returns `401`
- Token is printed to terminal on server startup
- Unit test: middleware rejects invalid tokens, accepts valid ones, bypasses for localhost

---

**Story 1.5 — Setup endpoint and first-run detection**  
Branch: `feature/phase1-setup-endpoint`

Implement `GET /api/setup/status` and `POST /api/setup/complete`. On first run (no user row in DB), setup status returns `{ "complete": false }`. `POST /api/setup/complete` accepts a display name, creates the user row, and marks setup complete.

Acceptance criteria:
- Fresh DB returns `complete: false`
- After POST, returns `complete: true`
- Integration test covers both states

---

**Story 1.6 — Core entity CRUD (providers, personas, threads, skills, MCP servers)**  
Branch: `feature/phase1-core-crud`

Implement all REST endpoints for providers (section 6.2, excluding Copilot auth), personas (section 6.4 including avatar upload), threads (section 6.7 including archive/unarchive and skill/MCP attachment), skills (section 6.5), and MCP servers (section 6.6). Implement API key encryption using a machine-derived secret. Include `POST /api/providers/:id/test` which calls the provider's `/v1/models` endpoint.

This is a large story but the entities are straightforward CRUD with no complex business logic. Batch them to avoid the overhead of six nearly identical stories.

Acceptance criteria:
- Full CRUD works for all entity types
- API keys are not returned in plaintext in GET responses (return masked value)
- Avatar upload accepts image files, stores in `data/avatars/`, returns serveable URL
- Thread creation requires valid `persona_id`; archive/unarchive works; skills and MCP servers can be attached/detached
- Integration tests for all endpoints across all entities

---

**Story 1.7 — React SPA scaffolding**  
Branch: `feature/phase1-web-scaffolding`

Initialize the React app in `web/`. Set up Vite, TypeScript, Tailwind CSS, and shadcn/ui. Configure Tailwind with the color tokens from section 4.1. Set up React Router with placeholder routes for all main sections. Configure Vite to proxy `/api` to `localhost:7474` in development. Set up the Vite build to output to `server/public/` so the Rust server serves it.

Acceptance criteria:
- `npm run dev` starts the dev server
- `npm run build` outputs to `server/public/`
- Color tokens are configured in `tailwind.config.ts`
- All route placeholders are reachable
- SPA loads when served by the Rust server

---

**Story 1.8 — API client and Zustand store shell**  
Branch: `feature/phase1-api-client`

Implement a typed API client service that wraps all API calls. On `401` response, show a **token entry screen** — a single input field where the user pastes their auth token, which is validated via `POST /api/auth/token` and stored as a cookie. On localhost, this screen is never shown (auth is bypassed). Create Zustand store shells for threads, messages, and UI state.

Acceptance criteria:
- All API endpoints from section 6 have typed client functions
- 401 responses show the token entry screen (not a redirect — an in-app state)
- Successful token entry stores cookie and resumes normal app flow
- Zustand stores are in place with empty initial state
- SSE client wrapper exists with reconnection logic (used in Phase 2)

---

**Story 1.9 — UI mockups (all screens)**  
Branch: `feature/phase1-ui-mockups`

Create static HTML mockups for every major screen in the application. These serve as the approved visual reference for all subsequent UI implementation. Mockups live in a `mockups/` directory at the project root. Each file is self-contained (inline CSS, no external dependencies) and uses the color tokens from section 4.1 as CSS custom properties. Includes light JavaScript for key interactions — hover states, transitions, dropdowns — but no API calls or real data. Dummy data is hardcoded.

**Web mockups (desktop viewport):**
- `chat-view.html` — Two-column layout: thread list sidebar (with search, thread entries showing agent emoji/name/preview, "New Chat" button, Settings/Archived links) + main chat area (thread title, agent header with avatar, message history with user messages right-aligned and agent messages left-aligned with avatar, routine message styling with `bubble_routine` color and label, streaming indicator animation, message input bar). Persona picker modal for new chat.
- `thread-config.html` — The slide-in config pane animating from the right over the chat view. Sections: persona info (read-only), model switcher dropdown, routines list with toggle switches and add button, skills list with add/remove, MCP servers list, memory section showing recent entries, system prompt addendum textarea. Cron field with human-readable description below it.
- `settings-providers.html` — Provider list with status indicators (connected/error badge). Add/edit provider form. Copilot auth modal showing device code and GitHub link. Test connection button with model list result.
- `settings-personas.html` — Persona cards with avatar, emoji, name. Create/edit form with system prompt textarea. Avatar upload with preview. Memory viewer tab showing memory entries with content, date, source thread, and delete button. Memory count badge.
- `settings-other.html` — Skills page with instructions editor (markdown-friendly textarea). MCP server list. Mobile pairing page with QR code placeholder. General settings with masked auth token and rotation button.
- `setup-wizard.html` — Full-screen multi-step wizard: welcome screen, provider selection and auth, persona creation with starter template, completion screen. Step indicator and skip button visible from step 2.
- `slash-commands.html` — Chat input with `/` typed, showing the floating autocomplete panel above it with command names and descriptions. Arrow key highlight state. Ephemeral message rendering (visually distinct from persisted messages — lighter opacity or dashed border).
- `empty-states.html` — Empty thread list, no personas configured banner, no provider connected banner, empty routines list.
- `token-entry.html` — Full-screen token entry for remote device authentication. Single input field, paste button, "Connect" action, brief explanation of where to find the token. Error state for invalid token.

**Mobile mockups (375px viewport, matching Pixel 4a):**
- `mobile-thread-list.html` — Thread list with agent emoji + avatar, title, last message preview, timestamp. New thread FAB button. Header with app name.
- `mobile-chat.html` — Chat screen with agent header, message bubbles matching web styling, streaming indicator, input bar. Routine messages styled distinctly. Header button for thread config.
- `mobile-thread-config.html` — Simplified config: current model display, model switcher, routines list (view only).
- `mobile-pairing.html` — First-launch QR scanner screen with instructions.

**Interaction fidelity (light JS):**
- Hover states on all interactive elements (buttons, thread list items, settings cards)
- Thread list item selection highlighting
- Config pane slide-in/slide-out animation (CSS transition)
- Slash command dropdown: appears on input focus, items highlight on hover
- Modal open/close (persona picker, Copilot auth, routine add)
- Setup wizard step transitions
- Toggle switch animation for routines/skills enable/disable
- Mobile: bottom sheet or push-style navigation feel

**Constraints:**
- All colors must use CSS custom properties matching section 4.1 token names — no hardcoded hex values
- Typography: system font stack, sizes that feel natural at both desktop and mobile viewports
- No external dependencies (no Tailwind CDN, no React) — pure HTML, CSS, inline JS
- Each file opens directly in a browser with no build step

Acceptance criteria:
- All listed mockup files exist in `mockups/` and render correctly in a browser
- Color tokens match section 4.1 exactly via CSS custom properties
- All key interactions listed above are functional
- Mobile mockups render correctly at 375px viewport width
- Mockups are reviewed and approved before proceeding to Phase 2

---

### Phase 2 — First Chat

**Goal:** You can talk to an LLM through your own UI. This is the milestone that makes the project feel real. By the end of this phase you can bootstrap a provider and persona via curl, then chat in the browser.

---

**Story 2.1 — copilot-api vendor submodule and process management**  
Branch: `feature/phase2-copilot-process`

Add `copilot-api` as a git submodule at `vendor/copilot-api/`. Implement the service that manages it as a child process using `tokio::process`. Start it when Copilot is the active provider, stop it when not needed, restart on crash. Implement `GET /api/providers/copilot/auth-status` and `POST /api/providers/copilot/auth-start`.

Acceptance criteria:
- `vendor/copilot-api/` is a valid git submodule
- Process starts and stops correctly
- Auth status reflects whether `copilot-api` has a valid GitHub token
- Process restarts automatically if it crashes
- Manual verification: can list Copilot models via the provider test endpoint

---

**Story 2.2 — Provider abstraction layer**  
Branch: `feature/phase2-provider-abstraction`

Implement the provider service using `async-openai`. It should accept a provider record from the DB (base URL + API key) and expose a unified interface for: listing models, and sending a chat completion request with streaming. Write the service so that pointing it at `http://localhost:4141/v1` (Copilot proxy) works identically to pointing it at `https://api.openai.com/v1`.

Acceptance criteria:
- Unit tests mock the HTTP layer and verify correct request construction
- Streaming works against a real provider (manual verification)
- Error cases handled: provider unreachable, invalid API key, model not found

---

**Story 2.3 — SSE infrastructure**  
Branch: `feature/phase2-sse-infrastructure`

Implement the two SSE endpoints from section 6.9. Create a channel-based event broadcaster in the server that routes events to the correct SSE streams. The per-thread stream should be created on connection and cleaned up on disconnect. Track connected SSE clients per thread (needed later for push notification decisions).

Acceptance criteria:
- `GET /api/threads/:id/stream` returns a valid SSE stream
- `GET /api/events` returns a valid SSE stream
- Client disconnection is handled cleanly (no resource leak)
- Manual verification: connecting with `curl` shows SSE stream

---

**Story 2.4 — Context assembly**  
Branch: `feature/phase2-context-assembly`

Implement the function that builds the message array for an LLM request. It should combine: persona system prompt + thread addendum + last N messages from DB (configurable, default 20). Write this as a pure function that is easy to unit test.

Acceptance criteria:
- Unit tests verify correct ordering of messages
- Unit tests verify system prompt is always first
- Unit tests verify message count is capped at the configured limit
- Thread addendum is appended to system prompt when present

---

**Story 2.5 — Agent run-loop and message endpoint**  
Branch: `feature/phase2-agent-runloop`

Implement `POST /api/threads/:id/messages`. The handler should:
1. Persist the user message to DB
2. Auto-generate thread title if this is the first message (truncate to 60 chars)
3. Assemble context via the context assembly function
4. Call the provider with streaming enabled
5. Stream tokens to the per-thread SSE channel as `token` events
6. Persist the completed assistant message once streaming finishes
7. Emit a `message_complete` event on the SSE channel
8. Emit a `thread_updated` event on the global SSE channel

Acceptance criteria:
- Messages are persisted correctly for both user and assistant roles
- Streaming tokens arrive on the SSE stream
- Thread title is auto-generated on first message
- Integration test: send a message, verify DB state, verify SSE events
- Manual verification: end-to-end chat works with a real provider

---

**Story 2.6 — Thread list and chat UI**  
Branch: `feature/phase2-chat-ui`

Implement the sidebar thread list and main chat view in the React SPA, matching the approved mockups in `mockups/chat-view.html`. Thread list shows active threads with agent emoji, name, and last message preview. Include a "New Chat" button that opens a persona picker. Chat view shows message history with user messages right-aligned and agent messages left-aligned with avatar. Connect to the per-thread SSE stream for token streaming. Message input with send on Enter. Connect to the global SSE stream to update thread list previews in real time.

This is the critical UI story — the one that makes the project usable.

Acceptance criteria:
- Thread list loads and displays correctly
- New chat flow creates a thread and navigates to it
- Message history loads on thread open
- Sending a message shows it immediately, then streams the response
- Agent avatar appears next to every agent message
- Streaming feels smooth with no flickering
- Thread list updates in real time via global SSE
- Empty state is shown when no threads exist

---

### Phase 3 — Configuration and Management ✅ Complete

**Goal:** The app is fully configurable through its own UI. Setup wizard, settings pages, thread config, slash commands. After this phase, you never need curl to manage the system.

**Status:** All stories complete. The app supports end-to-end chat: setup wizard, provider and persona management, thread config, slash commands, archived threads, pending thread UX, and server-side title generation. Credential store, OAuth, and persona default MCP servers were deferred to Phase 4 where they belong alongside full MCP integration.

---

**Story 3.1 — Setup wizard**  
Branch: `feature/phase3-setup-wizard`

Implement the first-run setup wizard matching `mockups/setup-wizard.html`. On app load, check `GET /api/setup/status`. If incomplete, show the wizard. Steps: welcome, add provider (with Copilot auth flow showing device code), create first persona (name, emoji, system prompt with starter template, model selection), done. The wizard is skippable from step 2 onward.

Acceptance criteria:
- Wizard shows on fresh setup
- Copilot auth flow triggers correctly and shows device code
- Persona creation works end-to-end
- After completion, wizard never shows again

---

**Story 3.2 — Settings: Providers**  
Branch: `feature/phase3-settings-providers`

Implement `/settings/providers` matching `mockups/settings-providers.html`. List providers with status indicators (connected/error). Add/edit/delete providers. Copilot auth flow with device code modal. Test connection button that shows model list on success.

Acceptance criteria:
- All provider CRUD operations work
- Copilot auth flow works end-to-end
- Test connection shows model list on success, error message on failure

---

**Story 3.3 — Settings: Personas**  
Branch: `feature/phase3-settings-personas`

Implement `/settings/personas` matching `mockups/settings-personas.html`. List personas with avatar, emoji, name. Create/edit persona form with system prompt editor (comfortable textarea, not single-line input). Avatar upload with preview. Delete with confirmation (only allowed if no active threads use the persona).

Acceptance criteria:
- All persona CRUD operations work
- Avatar upload previews the image before saving
- System prompt editor is a comfortable textarea
- Delete blocked when active threads reference the persona

---

**Story 3.4 — Settings: MCP, Mobile, General**  
Branch: `feature/phase3-settings-remaining`

Implement `/settings/mcp-servers`, `/settings/mobile` (QR code display for pairing), and `/settings/general` (server name, auth token rotation). MCP settings is a full management surface: add/edit/delete servers (local and remote types), per-server tool inspector (fetched from the server connection), source URL display.

Acceptance criteria:
- MCP server CRUD works for both local and remote types
- Local server config fields: executable path, args, env vars
- Remote server config fields: URL, auth header name, credential key reference
- Source URL field is displayed as a clickable link when set
- Tool inspector shows tools for connected servers
- QR code displays and encodes correct pairing data
- General settings show auth token (masked) with rotation button

---



**Story 3.5 — Thread config pane**  
Branch: `feature/phase3-thread-config`

Implement the slide-in thread config panel matching `mockups/thread-config.html`. Sections: persona info (read-only), model switcher (dropdown of available models), routines list (add/edit/delete/toggle — routines CRUD happens in Phase 4, but the UI shell goes here), MCP servers list with tool inspector and local/remote badge, tool activity toggle, system prompt addendum textarea. Cron expression field shows human-readable description below it.

Acceptance criteria:
- Pane opens and closes smoothly
- Model switch takes effect immediately and persists
- MCP servers section shows attached servers with status, type badge, and expandable tool list
- "+ Attach server" picker shows all configured servers not yet attached
- Tool activity toggle is visible and persists per-thread
- Addendum textarea saves on blur

---

**Story 3.6 — Slash command UI and server endpoint**  
Branch: `feature/phase3-slash-commands`

Implement the slash command system end-to-end. Server: `POST /api/threads/:id/command` endpoint that parses the command and args, routes to the appropriate handler, and returns a typed response (see section 6.8.1). Client: when the user types `/` in the input, show a floating autocomplete panel matching `mockups/slash-commands.html`. Intercept slash commands before sending — route to the command endpoint and display the result as an ephemeral message in the chat (visible but not persisted, visually distinct).

Acceptance criteria:
- Server endpoint handles all commands from section 7.3
- Unit tests for command parsing and routing on the server
- Autocomplete panel appears on `/` in the input
- All commands listed with descriptions
- Command results display correctly as ephemeral messages
- Normal messages are unaffected
- Unknown commands return a helpful error

---

**Story 3.7 — Archived threads**  
Branch: `feature/phase3-archived-threads`

Implement the archived threads view. List archived threads with titles and last message preview. Allow unarchiving from the list.

Acceptance criteria:
- Archived threads appear in the archived view and not in the main list
- Unarchiving moves a thread back to active
- Empty state shown when no archived threads

---

### Phase 4 — Credentials, MCP, Memory, and Routines

**Goal:** Establish the credential store and encryption infrastructure, then wire up real MCP server integration (some MCP servers require credentials for auth), then add persistent memory and autonomous scheduled routines. This is what differentiates agent-deck from a chat wrapper.

---

**Story 4.x — Credential store and encryption**
Branch: `feature/phase4-credential-store`

Implement the credential storage infrastructure. On first server run, generate a 256-bit master key and store it in `app_config` as `credential_master_key`. Implement AES-256-GCM encrypt/decrypt helpers. Implement `credentials` table CRUD with all data encrypted at rest. The `GET /api/credentials` endpoint returns metadata only — `encrypted_data` is never included in any API response.

Three ownership levels:
- **System-level** — shared across all personas (e.g. a single Google account used by multiple agents)
- **User-level** — belongs to the user, not tied to a specific persona
- **Persona-level** — owned by a specific persona; `persona_id` is set; used when different agents should have separate identities or access scopes

Credential binding happens at configuration time: when adding or editing an MCP server, the user selects which stored credential (by key name) the server should use. The agent never sees raw credential values — MCP servers resolve them at runtime by key name.

Acceptance criteria:
- Master key is generated once on first run and persists across restarts
- Master key is never included in any log output or API response
- AES-256-GCM encrypt/decrypt helpers are unit tested
- `credentials` table CRUD works for all three ownership levels
- `GET /api/credentials` returns metadata only — no `encrypted_data` in any response
- Integration test: store a credential, retrieve it, confirm `encrypted_data` round-trips correctly through decrypt
- Credential metadata endpoint returns correct fields with no secrets
- `cargo build` passes, all tests pass

---

**Story 4.y — MCP server integration**
Branch: `feature/phase4-mcp-integration`

Wire MCP servers into the agent run-loop. This story covers the full lifecycle: connecting to a server, discovering its tools, injecting those tools into the agent context, executing tool calls, and returning results. Both local (subprocess) and remote (HTTP/SSE) server types must work.

**Depends on:** Story 4.x (credential resolution needed for authenticated remote servers).

What to build:
- MCP connection manager in `services/mcp.rs`: maintains a pool of active connections keyed by `mcp_server_id`; handles connect, disconnect, reconnect on error
- Local server type: spawn the configured executable as a subprocess, communicate over stdio using the MCP protocol
- Remote server type: connect to the configured URL; resolve the `credential_key` from the credential store and attach as the configured auth header
- Tool discovery: on connect, fetch the server's tool list and cache it; expose via `GET /api/mcp-servers/:id/tools`
- Agent integration: in `agent::run_inner`, load the thread's attached MCP servers (`thread_mcp_servers`), fetch their cached tool lists, merge with built-in tools, inject into the generation loop
- Tool call routing: when the model calls a tool whose name is prefixed with a server name (e.g. `filesystem__read_file`), route execution to that MCP server
- Persona default MCP servers: when a new thread is created, auto-attach all servers listed in `persona_default_mcp_servers` for the thread's persona. Add a **Default MCP Servers** section to the persona edit view in `PersonaSettings.tsx` — same visual style as the MCP list in the thread config pane; "+ Add default server" picker; label: "These servers are attached automatically when a new thread is created with this persona."

Acceptance criteria:
- Local MCP server connects and its tools appear in the agent context
- Remote MCP server connects, credential is resolved and attached as auth header
- Tool calls are routed to the correct server and results returned to the model
- Tools from different servers are namespaced by server name to avoid collisions
- Thread config pane MCP section shows live connection status
- Tool inspector (`GET /api/mcp-servers/:id/tools`) returns tool list for connected servers
- Creating a thread with a persona auto-attaches its default MCP servers
- Default MCP servers section works in persona settings (add/remove)
- `cargo build` passes, all tests pass

---

**Story 4.z — OAuth framework + Google + GitHub**
Branch: `feature/phase4-oauth`

Implement the OAuth provider trait and provider registry. Implement Google and GitHub providers. Implement the OAuth flow endpoints: `GET /api/auth/oauth/:provider/start` (returns redirect URL with state) and `GET /api/auth/oauth/callback` (exchanges code, stores encrypted tokens via the credential store from Story 4.x). Add the `/settings/accounts` page and the Accounts tab in persona settings. Implement token refresh on credential resolution.

**Depends on:** Story 4.x (credentials store is the persistence layer for OAuth tokens).

Accepted scopes for Google (selectable in UI): Gmail read, Gmail send, Calendar read, Calendar write, Drive read, Drive write.

Acceptance criteria:
- OAuth flow completes end-to-end with a real Google account
- OAuth flow completes end-to-end with a real GitHub account
- Access token and refresh token stored encrypted via credential store
- Token refresh works transparently when expired
- `/settings/accounts` shows connected accounts with scopes as pills
- Disconnect removes the credential row
- Adding a third OAuth provider requires only: implement the trait, register in the registry — no other changes
- `cargo build` passes, all tests pass


---

**Story 4.1 — Memory tools**  
Branch: `feature/phase4-memory-tools`

Implement the `save_memory` and `recall_memory` tool definitions per section 7.6.2. Wire them into the agent run-loop so they are always available as callable tools. Append the memory system prompt instructions (section 7.6.3) after the persona's system prompt in every request. `save_memory` inserts a new memory row with `user_id`, `persona_id` (from the thread's persona), and `thread_id` (provenance), enforcing the 500-character content limit, and updates the FTS index. `recall_memory` queries `memory_fts` filtered by the current user and persona, capped at 10 results, formatted per section 7.6.5.

Acceptance criteria:
- Unit tests for FTS search returning correct results
- Unit tests for memory insertion with correct persona scoping
- Unit tests for the 500-character truncation
- Memories saved in one thread are recallable from another thread with the same persona
- Memories are NOT recalled when querying from a different persona
- Recall results are capped at 10 and include date prefix and thread provenance
- Empty recall returns the "no memories found" message
- The memory system prompt is appended to every request (after persona prompt, before thread addendum)
- The tools are included in LLM requests as function definitions
- Integration test: save a memory, send a follow-up message in a different thread (same persona) that should trigger recall, verify the tool is called

---

**Story 4.0 — System Notification Channel + Agent Run Lock**
Branch: `feature/phase4-notify-endpoint`

This story is a prerequisite for all other Phase 4 stories. Routines, memory tools, and any future server-initiated agent trigger depend on both the notify endpoint and the concurrency lock.

**Part A — Per-thread agent run lock:**
Add a `DashMap<String, Arc<Semaphore>>` to `AppState` keyed by thread ID. Every code path that invokes `agent::run` — `POST /api/threads/:id/messages`, slash command model switch, and the new notify endpoint — must acquire a per-thread permit before running and release it on completion. Reject with `429` if queue depth exceeds 3.

**Part B — System notification endpoint:**
Implement `POST /api/threads/:id/notify` per section 6.8.2. Implement the event type registry as a Rust enum with associated `persist` and `trigger` flags and a content template. For `persist: true` events, insert into `messages` with `role: system`, `source: system_event`, `visibility: hidden`, `event_type` set, and broadcast a `system_event` SSE event. For `trigger: true` events, acquire the run lock and invoke `agent::run` with the event payload as the triggering prompt.

**Part C — Schema migration:**
Add `event_type TEXT` column to `messages`. Add `show_system_events INTEGER NOT NULL DEFAULT 0` column to `threads`. Update `Thread` model and all affected SELECT/INSERT/UPDATE queries.

**Part D — Client integration:**
After model switch in `ConfigPane`, call `POST /api/threads/:id/notify` with `event_type: model_switched`. After MCP attach/detach, emit the corresponding event. These are fire-and-forget.

**Part E — `show_system_events` toggle:**
Add the toggle to the Thread Config pane (below the existing `show_tool_activity` toggle). Wire it to `PUT /api/threads/:id`.

Acceptance criteria:
- [ ] Per-thread semaphore prevents concurrent agent runs on the same thread
- [ ] Concurrent attempt queues and runs after the first completes
- [ ] Queue depth > 3 returns `429` with clear message
- [ ] `POST /api/threads/:id/notify` accepts all registered event types and rejects unknown ones with `400`
- [ ] `persist: true` events insert a hidden system message and broadcast `system_event` SSE
- [ ] `trigger: true` events invoke the agent run loop and produce a visible response
- [ ] `persist: false, trigger: false` is rejected
- [ ] `model_switched` event visible in message history (with `?include_hidden=true`)
- [ ] Client calls notify after model switch; hidden message appears in DB
- [ ] `show_system_events` toggle persists per-thread
- [ ] Unit tests for event registry (valid types, invalid type rejection, flag combinations)
- [ ] `cargo sqlx prepare` run and `.sqlx/` committed

---

**Story 4.2 — Routines CRUD endpoints**
Branch: `feature/phase4-routines-crud`

Implement all routine endpoints from section 6.10.

Acceptance criteria:
- Full CRUD works
- Toggle endpoint correctly flips the enabled flag
- Integration tests for all endpoints

---

**Story 4.3 — Cron scheduler**  
Branch: `feature/phase4-cron-scheduler`

Implement the routine scheduler service using `tokio-cron-scheduler`. On server startup, load all enabled routines from the DB and register them. When a routine is created, updated, or toggled via the API, update the scheduler accordingly. When a thread is archived, pause its routines. When unarchived, resume them.

Acceptance criteria:
- Unit tests for scheduler registration and deregistration
- Routines fire at the correct time (test with a short interval like every minute)
- Pausing and resuming works correctly
- Server restart re-registers all active routines from DB

---

**Story 4.4 — Routine execution**

> **Depends on Story 4.0** — the routine scheduler uses the notify endpoint with `event_type: routine_fired` (`persist: false, trigger: true`) to invoke the agent. The bespoke routine invocation JSON described in section 7.4 is the payload for this event type. The two-phase execution model remains the same — Story 4.0 provides the infrastructure, Story 4.4 wires the scheduler into it.

Branch: `feature/phase4-routine-execution`

Implement the two-phase routine execution model per section 7.4. When a routine fires:

**Phase 1 (background):**
1. Create a `routine_executions` row with `status: running`
2. Build a background agent context (in-process, no SSE emission during execution)
3. Inject the routine invocation message (JSON schema per section 7.4) as the triggering prompt
4. Run the agent loop — all intermediate messages (tool calls, MCP results) are written to `messages` with `visibility: hidden` and `execution_id` set

**Phase 2 (emit):**
5. Write the final synthesized response to `messages` with `source: routine`, `visibility: visible`, `execution_id` set
6. Update `routine_executions` with `status: completed` and `output_message_id`
7. Emit `routine_message` event on the thread's SSE stream
8. Emit `routine_fired` event on the global SSE stream
9. Update `last_run_at` and `run_count` on the routine record
10. If no SSE clients connected, dispatch FCM push notification

Acceptance criteria:
- Integration test: create a routine with a 1-minute schedule, verify it fires, hidden intermediate messages are stored, and one visible result message appears
- No hidden messages appear in `GET /api/threads/:id/messages` response (filtered by default; `?include_hidden=true` param exposes them)
- Visible result message has `source: routine` in DB
- `routine_executions` row correctly tracks status and output_message_id
- SSE events are emitted correctly
- `last_run_at` and `run_count` are updated after each run

---

**Story 4.5 — Routine and memory UI integration**  
Branch: `feature/phase4-routine-memory-ui`

Wire routines into the thread config pane (the shell from Story 3.5 — now with full add/edit/delete/toggle functionality). Routine-generated messages in the chat view should be visually distinct (subtle different background using `bubble_routine` color, small "routine" label). 

Add memory viewer per section 7.6.7:
- In the thread config pane: a "Memory" section showing recent memories saved from the current thread (filtered by provenance `thread_id`)
- In settings under each persona: a full memory list for that persona with content, date, source thread name, and delete button
- `/memory list` slash command returning last 20 memories as an ephemeral message

Acceptance criteria:
- Routines can be added, edited, deleted, and toggled from the thread config pane
- Cron expression field shows human-readable description below it
- Routine messages in chat are visually distinct from regular messages
- Thread list updates when a routine fires (via global SSE)
- Thread config pane shows memories from the current thread
- Persona settings page shows all memories for that persona with delete capability
- `/memory list` command works and displays results as ephemeral message
- Memory count badge visible in persona settings

---

### Phase 5 — React Native Mobile App

**Goal:** A new React Native app is scaffolded from scratch and built to work with the agent-deck server API. Full chat experience on Android.

> ⚠️ **Note:** The original `mobile/BotRelayApp/` scaffold was permanently deleted (`rm -rf`) before it was committed remotely. There is no recoverable version. Story 5.1 must initialize a fresh React Native project rather than cleaning up the old one. The dependency list and rename instructions below still apply — treat them as the target state for the new scaffold.

---

**Story 5.1 — App scaffold and setup**  
Branch: `feature/phase5-mobile-cleanup`

Initialize a new React Native project named `AgentDeck` at `mobile/AgentDeck/`. Install required dependencies: `react-native-gifted-chat`, `@react-navigation/native`, `@react-navigation/native-stack`, `react-native-mmkv`, `react-native-safe-area-context`, `axios`, `@notifee/react-native`, `@react-native-firebase/app`, `@react-native-firebase/messaging`. Do **not** install `socket.io-client`, `tweetnacl`, `tweetnacl-util`, or `react-native-video`. Apply the shared color theme from section 4.1 to `src/theme/colors.ts`.

Acceptance criteria:
- Fresh React Native project builds and runs on Android
- App is named `AgentDeck` in `app.json` and `package.json`
- All required dependencies installed, disallowed dependencies absent
- Color tokens are in place at `src/theme/colors.ts`

---

**Story 5.2 — API service and auth**  
Branch: `feature/phase5-mobile-api-service`

Rewrite `ApiService.ts` to match the new server API. Store server URL and auth token in `react-native-mmkv`. Implement SSE client using `EventSource` polyfill or `fetch` with streaming. Handle reconnection.

Acceptance criteria:
- API service covers all endpoints needed by the mobile screens
- Auth token is stored and sent on every request
- SSE connection works and delivers events

---

**Story 5.3 — QR pairing screen**  
Branch: `feature/phase5-mobile-pairing`

Implement the first-launch pairing screen. Show a QR code scanner. On scan, parse the server URL and token, store in MMKV, navigate to thread list. On subsequent launches, skip directly to thread list if already paired.

Acceptance criteria:
- QR scanner opens and reads the pairing QR from the browser UI
- Server URL and token are stored correctly
- App connects to server and loads thread list after pairing
- Already-paired users skip the pairing screen on launch

---

**Story 5.4 — Thread list screen**  
Branch: `feature/phase5-mobile-thread-list`

Implement the thread list screen matching `mockups/mobile-thread-list.html`. Show agent emoji + avatar, thread title, last message preview, timestamp. Connect to global SSE for real-time updates. New thread button (persona picker).

Acceptance criteria:
- Thread list loads from server
- Real-time updates work when a routine fires
- Empty state displayed correctly
- New thread creation works

---

**Story 5.5 — Chat screen**  
Branch: `feature/phase5-mobile-chat`

Implement the chat screen matching `mockups/mobile-chat.html`, using `react-native-gifted-chat`. Show agent avatar on every agent message. Stream tokens in real time via SSE. Send messages. Routine messages visually distinct.

Acceptance criteria:
- Chat history loads on screen open
- Streaming works and feels smooth
- Agent avatar displays correctly on every agent message
- Routine messages are visually distinct from chat messages

---

**Story 5.6 — Thread config screen**  
Branch: `feature/phase5-mobile-thread-config`

Implement a simplified thread config screen (accessible from a header button in the chat screen). Show current model, option to switch model, list of active routines (view only on mobile).

Acceptance criteria:
- Config screen accessible from chat header
- Model switch works and persists
- Routines list shows correctly

---

### Phase 6 — Push Notifications (Android/FCM)

**Goal:** The Android app receives push notifications when routines fire and no SSE client is connected.

**Prerequisite:** Before starting any stories in this phase, create a Firebase project and complete the manual setup: add the Android app, download `google-services.json`, generate a service account key. This is external configuration that blocks all four stories — do it first, not as part of Story 6.1.

---

**Story 6.1 — Firebase project setup**  
Branch: `feature/phase6-firebase-setup`

Create a Firebase project. Add the Android app to it. Download `google-services.json` and place it in `mobile/BotRelayApp/android/app/`. Download the Firebase service account JSON and place it in `server/config/`. Document the setup steps in the README.

Acceptance criteria:
- Firebase project exists
- Android app builds with Firebase dependencies
- Service account file is in place on the server (not committed to git — add to `.gitignore`)

---

**Story 6.2 — Device token registration**  
Branch: `feature/phase6-device-token-registration`

In the mobile app, request notification permission on first launch. Get the FCM token via `@react-native-firebase/messaging`. Register it with the server via `POST /api/device-tokens` on every app launch (token can change). Handle token refresh events.

Acceptance criteria:
- Token is registered with server after first launch
- Token refresh events trigger re-registration
- `device_tokens` table has the correct entry after registration

---

**Story 6.3 — FCM dispatch from server**  
Branch: `feature/phase6-fcm-dispatch`

In the routine execution service, after persisting the routine response, check whether any SSE client is currently connected for the thread. If not, send an FCM push notification to all registered device tokens for the user. Notification title: agent emoji + name. Body: first 100 chars of response. Data: `thread_id`.

Acceptance criteria:
- Notification is sent when no SSE client is connected
- Notification is NOT sent when an SSE client is connected
- Notification title and body are correct
- Unit test for the "should notify" decision logic

---

**Story 6.4 — Notification handling in mobile app**  
Branch: `feature/phase6-mobile-notification-handling`

Handle incoming FCM notifications in the mobile app. Foreground: show an in-app banner using `@notifee/react-native`. Background/quit: tapping the notification deep-links to the correct thread using `thread_id` from the notification data.

Acceptance criteria:
- Foreground notification banner appears and is tappable
- Background notification tap navigates to correct thread
- Quit-state notification tap opens app and navigates to correct thread

---

### Phase 7 — MCP Depth

**Goal:** MCP servers are fully first-class. Tool inspector works, local server process management is robust, and the platform is ready for any MCP integration.

---

**Story 7.1 — MCP tool inspector**  
Branch: `feature/phase7-mcp-tool-inspector`

When an MCP server connects, enumerate its exposed tools and cache the list (name, description, input schema) in memory. Expose this via `GET /api/mcp-servers/:id/tools`. In the Thread Config pane and MCP settings page, render the tool list as an expandable section per server. Include the source URL as a clickable link when set.

Acceptance criteria:
- Tool list is fetched and displayed in Thread Config per attached server
- Tool list is displayed in MCP settings per server
- Source URL renders as a link when present
- Tool list refreshes when a server reconnects
- Unit tests for tool enumeration and caching

---

**Story 7.2 — Local MCP process management**  
Branch: `feature/phase7-local-mcp-processes`

Implement full lifecycle management for local MCP servers. The Rust server starts local servers as child processes on demand (when a thread with that server is opened or on server startup if the server has active threads). Health-check loop monitors the process. On crash, attempt restart with exponential backoff. Status is kept live in the `mcp_servers.status` field and broadcast via global SSE event. Graceful shutdown on server exit.

Acceptance criteria:
- Local server starts on demand and is restarted on crash
- Restart backoff prevents tight crash loops
- Status badge in UI reflects actual connection state in near-real-time
- All local server processes are cleanly shut down when the Rust server exits
- Integration test: start a local server, kill its process, verify restart and reconnection

---

### Phase 8 — Polish and Hardening

**Goal:** The system is reliable, handles errors gracefully, and provides a good experience end-to-end.

---

**Story 8.1 — Error handling and user feedback**  
Branch: `feature/phase8-error-handling`

Audit all error paths in the Rust server and ensure they return consistent, meaningful error responses. Audit the React SPA and add toast notifications for API errors. Ensure SSE errors are surfaced to the user. Add retry logic for transient provider errors.

---

**Story 8.2 — SSE reconnection and resilience**  
Branch: `feature/phase8-sse-resilience`

Implement robust SSE reconnection in both the web and mobile clients. Use the `Last-Event-ID` header to resume from the last received event. Ensure no messages are lost during a brief disconnect.

---

**Story 8.3 — Message pagination**  
Branch: `feature/phase8-message-pagination`

Implement cursor-based pagination on `GET /api/threads/:id/messages`. In the web and mobile apps, implement "load more" by scrolling to the top of the message list.

---

**Story 8.4 — Setup and README**  
Branch: `feature/phase8-docs`

Write a comprehensive README covering: what agent-deck is, prerequisites, installation steps (including `git submodule init` for copilot-api), first-run setup, mobile pairing, and how to add providers. Document the Firebase setup steps. Document the Tailscale setup.

---

### Phase 9 — Status Bar App (Deferred)

Deferred until all other phases are complete. See section 11 for notes.

---

## 11. Deferred / Future Work

The following items are explicitly out of scope for v1. They are documented here so future contributors have context.

### Status Bar App (Phase 9)
A native macOS Swift/SwiftUI app that lives in the menu bar. It manages the Rust server process and optionally the `copilot-api` process. Shows server status (running/stopped), active thread count, and allows starting/stopping the server. Registers as a Login Item so it starts on boot. The `.app` bundle allows it to appear in Launchpad and Spotlight. Planned for after all other phases are complete.

### iOS App
No iOS app in v1. Building for iOS requires an Apple Developer account ($99/year). The Android app serves as the mobile client. iOS can be added in a future version once the Android app is stable.

### Multi-User Support
The data model includes `user_id` on all relevant tables in anticipation of this. In v1 there is one user. Future: add a login screen, user management, per-user API keys, per-user thread isolation.

### Hard Delete
Threads can be archived in v1. Hard delete (with full cascade through messages, routines, thread_skills, etc.) requires careful design to avoid accidental data loss. In v1, use soft delete only.



### Additional OAuth Providers
The OAuth provider framework (Phase 3) is designed to be modular — adding a new provider (Notion, Linear, Slack, etc.) requires only implementing the `OAuthProvider` trait and registering in the provider registry. Future providers follow the same pattern with no infrastructure changes.

### Memory as MCP
The current memory system is a baked-in tool-calling implementation. A future version should expose it as a local MCP server instead, making it swappable. This would allow plugging in a different memory backend (e.g., a vector database) without touching the core agent run-loop. Deferred to avoid scope expansion in v1.

### Semantic / Vector Memory (revised)
The v1 memory system uses SQLite FTS5 keyword search. If memory is migrated to an MCP server pattern (above), the backend can be swapped to `sqlite-vec` for vector embeddings, enabling semantic recall.

### Thread Hard Search
Full-text search across all threads and messages. Useful as the thread list grows. SQLite FTS5 on the `messages` table would power this.

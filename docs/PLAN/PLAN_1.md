# Agent-Deck — Project Plan

**Version:** 1.9  
**Project:** agent-deck  
**Purpose:** A self-hosted, highly configurable personal AI agent platform designed to make working with LLMs accessible to non-engineers. Runs on a Mac mini, accessible privately over Tailscale, with a browser UI and mobile PWA.

**v1.9 Changes:**
- Replaced React Native mobile app (Phases 6–7) with PWA + Web Push notification approach
- Removed React Native, Firebase, and FCM dependencies entirely — no external accounts or services required
- Mobile experience delivered via PWA: `manifest.json`, service worker, home screen install on Android and iOS
- Push notifications use Web Push with VAPID keys — keys generated once on server startup, stored in `app_config`, no registration with Google or Apple required
- Replaced `device_tokens` table (FCM) with `push_subscriptions` table (Web Push endpoint + encryption keys)
- Replaced `POST /api/device-tokens` with `POST /api/push/subscribe` and `DELETE /api/push/subscribe`
- Added `GET /api/push/vapid-public-key` endpoint to expose VAPID public key to client
- Section 3.3 rewritten: React Native → PWA description
- Section 7.8 rewritten: FCM → Web Push
- Section 7.10 rewritten: QR pairing → PWA install instructions
- Section 8.5 rewritten: React Native screens → PWA mobile experience
- Phase 6 rewritten: React Native scaffold → PWA manifest + install story
- Phase 7 rewritten: FCM push → Web Push server + client story
- Removed `mobile/` directory from repository structure
- Removed `FCM_SERVICE_ACCOUNT_JSON` environment variable
- Deferred/future work: updated iOS note (no Developer account needed for Web Push PWA)
- Added Tailscale integration: server-side status detection, install, and connect flow; `tailscale_hostname` stored in `app_config`; `GET /api/tailscale/status`, `POST /api/tailscale/install`, `POST /api/tailscale/connect` endpoints
- Setup wizard gains a Tailscale step (between Welcome and Provider) that auto-advances if already connected
- `GET /api/pairing/qr` now uses `tailscale_hostname` from `app_config` for the server URL
- Added Story 1.x (Tailscale server integration) to Phase 1; Story 6.x (Tailscale wizard step) to Phase 6
- Added `tailscale` CLI to Rust server tech stack (invoked via `tokio::process::Command`)

**v1.8 Changes:**
- Restructured Story 5.1 into "Per-thread agent run management" with seven parts (A–G): run lock, cancellation, notify endpoint, schema migration, client integration, system events toggle, orphan cleanup
- Added cancellation support: `POST /api/threads/:id/cancel` stops an active agent run mid-stream or mid-tool-execution. Partial responses are persisted with `stopped = 1`. `CancellationToken` checked between streaming chunks, before each tool dispatch, and via `tokio::select!` during in-flight MCP calls
- Added `stopped` column to `messages` table for cancelled partial responses
- Added `POST /api/threads/:id/cancel` endpoint to API contract (section 6.8) with `stopped` flag on `message_complete` SSE event
- Message input no longer disabled during streaming — users can send additional messages that queue behind the active run. Routine-initiated runs are exempt from the depth limit (always queue, never dropped)
- Added orphaned `routine_executions` cleanup on server startup (Story 5.1 Part G): any rows with `status = 'running'` are marked `failed` before the scheduler starts
- Added iteration note to Story 5.2: memory recall reliability is prompt-dependent and expected to need tuning post-dogfooding; auto-injection of recent memories is a potential future enhancement
- Added Default persona: a built-in persona with no system prompt and no memory, used as the backing record when users select "No persona" — keeps `persona_id` non-nullable across threads and memories. Added `is_default` column to `agent_personas`
- Story 5.2 updated with Default persona prerequisite: memory tools are not injected when a thread uses the Default persona
- Removed FCM push notification dispatch from Story 5.4 step 10 — moved to Phase 7 Story 7.3 where FCM is actually implemented
- Split Story 5.5 (Routine and memory UI integration) into Story 5.5 (Routine UI) and Story 5.6 (Memory UI) for independent scoping
- Persona selector shows Default persona as "None" with hint: "Without a persona, long-term memory is not available"
- Updated Per-Thread Concurrency spec (section 7.x) with cancellation behavior, queued messages, and routine depth exemption

**v1.7 Changes:**
- Phase 4 marked complete — all stories 4.1 through 4.5 shipped and verified working
- Fixed critical encryption key mismatch bug: credential routes were encrypting with `machine_secret` but the MCP manager was decrypting with `credential_master_key` — two separate randomly-generated keys. Added `credential_master_key` to `AppState` and unified all credential encrypt/decrypt paths through it
- Fixed MCP HTTP transport compliance: `post_rpc` was missing the required `Accept: application/json, text/event-stream` header mandated by the MCP Streamable HTTP spec, causing HTTP 406 from compliant servers (e.g. Context7). Also added SSE response body handling — the server may respond with `text/event-stream` instead of `application/json`
- Extended `RemoteConfig` with a `headers: HashMap<String, String>` field for arbitrary per-request headers; credential-derived auth header is now merged into this map. Headers are configurable through the full config chain (config.json → DB → UI → connection)
- Added `auth_format` field to remote MCP server UI (was in `RemoteConfig` struct but not exposed)
- Fixed supervise loop reconnect bug: after a connection drop, the loop was looking up `shutdown_rx` by searching `self.connections` — but the connection had just been removed. The `unwrap_or_else` fallback created a pre-fired `watch::channel(true)` which immediately hit the shutdown arm and killed the supervision task permanently. Fixed by carrying `shutdown_rx` out of the match arms directly
- Added debug logging throughout `resolve_secret` and `connect_remote` to aid credential resolution triage
- Unified all three "server connected" log sites to consistent structured format: `server_id`, `tool_count`, message `"mcp: server connected"`; removed duplicate logs from `connect_local` and `connect_remote` (supervise is the single authoritative emitter)
- Credential dropdown in MCP server form now populated from live credentials API instead of a free-text field
- Tag field added to MCP server add/edit form with auto-derivation from name (mirrors server-side logic: lowercase, spaces→underscores, strip non-alphanumeric); tag field in `McpServer` type and `serverToFormState` round-trip correctly on edit
- As-built deviation (4.5): auto-attach to new threads implemented as per-persona defaults via `persona_default_mcp_servers` table rather than the planned global `app_config` toggle — this is a better design and is retained as canonical

**v1.6 Changes:**
- Split Phase 4 into two phases: Phase 4 (Credentials and MCP Integration) and Phase 5 (Memory and Routines)
- Simplified credential store to static secrets only (API keys, PATs, bearer tokens, key/secret pairs) — OAuth browser flows, token refresh, and third-party app credentials deferred to future work
- Removed `persona_default_mcp_servers` table — replaced with global default MCP servers configured via `app_config`
- Added `tag` field to `mcp_servers` for tool call namespacing (defaults to server name, user-editable until loaded into a session)
- Simplified `credentials` table: removed `oauth2` credential type, `scopes`, `expires_at`; removed `persona_id` ownership (persona-level credential binding deferred with OAuth)
- Removed OAuth API endpoints from Phase 4 (`/api/auth/oauth/:provider/start`, `/api/auth/oauth/callback`)
- Provider API keys to be migrated into the encrypted credential store in Story 4.1
- Renumbered Phase 5 (React Native) → Phase 6, Phase 6 (Push Notifications) → Phase 7, Phase 7 (MCP Depth) → Phase 8, Phase 8 (Polish) → Phase 9
- Added "OAuth and Third-Party App Credentials" to deferred/future work

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

**This file (PLAN_1.md):**
1. [Project Overview](#1-project-overview)
2. [Architecture](#2-architecture)
3. [Technology Stack](#3-technology-stack)
4. [Shared Theme](#4-shared-theme)
5. [Data Model](#5-data-model)

**PLAN_2.md:**
6. API Contract
7. Feature Specifications
8. UI/UX Specification
9. Development Guide

**PLAN_3.md:**
10. Phased Execution Plan
11. Deferred / Future Work

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
│  │   Browser        │         │  Mobile PWA          │  │
│  │   React SPA      │         │  (any device,        │  │
│  │   (any device)   │         │   home screen)       │  │
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
```

### 2.2 Process Relationships

The Rust server is the central process. It:
- Serves the React SPA as static files from its `public/` directory
- Exposes a REST + SSE API consumed by both the browser and mobile app
- Owns the agent run-loop — LLM calls happen here, not on the client
- Manages the `copilot-api` child process (starts it, monitors it, restarts if it crashes)
- Detects and manages Tailscale connection state; stores the machine's Tailscale hostname in `app_config`
- Owns the cron scheduler for routines — routines deliver invocations via the system notification channel (not direct function calls)
- Dispatches Web Push notifications (VAPID) when no SSE client is connected for a thread
- Enforces a per-thread agent run lock — only one agent run may execute per thread at a time; additional runs queue behind it (see section 7.x)

### 2.3 Communication Patterns

- **Client → Server:** Standard HTTP REST (POST, GET, PUT, DELETE, PATCH)
- **Server → Client (streaming):** Server-Sent Events (SSE) — used for LLM token streaming and live event delivery (new messages, routine completions, system events)
- **Server → Mobile (background):** Web Push notifications via VAPID — no external accounts or registration required
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
| web-push | `web_push` crate | Web Push (VAPID) notification dispatch |
| Tailscale CLI | `tokio::process::Command` | Install detection, `tailscale up`, hostname discovery |
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

### 3.3 PWA (Mobile and Desktop)

The React SPA is also the mobile client. No separate native app is needed. The `mobile/` directory does not exist in this project.

The web app is made installable as a Progressive Web App by adding:

| Addition | Purpose |
|---|---|
| `manifest.json` | Defines app name, icons, `display: standalone` (required for iOS push) |
| Service worker (`sw.js`) | Handles background push events, shows notifications |
| `vite-plugin-pwa` (dev dependency) | Generates service worker boilerplate, injects manifest link |

**Installing on Android:** Chrome shows an "Add to Home Screen" banner automatically, or via the browser menu. Once installed, the PWA opens full-screen like a native app.

**Installing on iOS:** In Safari, tap Share → "Add to Home Screen". The PWA must be opened from the home screen icon (not from Safari) for push notifications to work — this is an Apple requirement.

Push notifications use the Web Push API with VAPID keys. No Firebase project, no Google account, no Apple Developer account required. The browser vendor's push infrastructure (Google for Chrome, Apple for Safari) relays the notification, but this requires no registration — VAPID keys are self-generated and self-signed. See section 7.8 for the full push notification spec.

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

The React SPA uses named color tokens defined as CSS custom properties. The color tokens are defined once in `web/src/styles.css` and referenced throughout. When a color needs to change, it changes in one place.

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
  api_key      TEXT,                      -- DEPRECATED: migrated to credentials table in Story 4.1; always null post-migration. Retained to avoid breaking the migration chain. Key resolution goes through credential store. (uses proxy auth)
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
  is_default      INTEGER NOT NULL DEFAULT 0,  -- exactly one row has 1; cannot be deleted or renamed
  default_model   TEXT,                   -- model_id FK
  default_provider TEXT,                  -- provider_id FK
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id),
  FOREIGN KEY (default_model) REFERENCES models(id),
  FOREIGN KEY (default_provider) REFERENCES providers(id)
);
```

#### Default Persona

A built-in persona is seeded on first run (migration). It has an empty `system_prompt`, emoji `💬`, no avatar, and `is_default = 1`. It exists so that every thread always has a non-null `persona_id`, even when the user selects "No persona" in the UI.

Rules:
- The Default persona cannot be deleted or renamed via the API. `DELETE /api/personas/:id` and `PUT /api/personas/:id` return `403` when `is_default = 1`.
- Memory tools (`save_memory`, `recall_memory`) and the memory system prompt are **not injected** when the thread's persona is the Default persona. Memory is a feature of named personas only.
- The persona selector in the UI shows the Default persona as **"None"** (never "Default persona"). Below the option, a hint reads: *"Without a persona, long-term memory is not available."*
- New threads created without specifying a persona are assigned the Default persona.

#### Global Default MCP Servers

Instead of per-persona MCP server defaults, a global list of default MCP servers is stored in `app_config` under the key `default_mcp_servers` as a JSON array of `mcp_server_id` values. When a new thread is created, all servers in this list are automatically attached via `thread_mcp_servers`. The list is managed in `/settings/mcp-servers` with a simple toggle per server ("Auto-attach to new threads").

#### `mcp_servers`
MCP server configurations. Supports two server types: `local` (process managed by the Rust server) and `remote` (externally hosted HTTP/SSE endpoint). The `tag` field is used as the tool call namespace prefix (e.g. tools from a server tagged `github` appear as `github__create_issue`). Defaults to the server name. Editable in settings, but changing the tag requires reloading the server in any active sessions.

```sql
CREATE TABLE mcp_servers (
  id           TEXT PRIMARY KEY,          -- UUID v4
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,             -- official MCP server name, e.g. "github", "filesystem"
  tag          TEXT NOT NULL,             -- tool namespace prefix, defaults to name; user-editable
  description  TEXT,                      -- short description shown in UI
  source_url   TEXT,                      -- GitHub repo or docs link, informational only
  server_type  TEXT NOT NULL CHECK (server_type IN ('local', 'remote')),
  config       TEXT NOT NULL,             -- JSON, shape varies by server_type:
                                          --   local:  { "executable": "path", "args": [], "env": {} }
                                          --   remote: { "url": "https://...", "auth_header": "Authorization", "credential_key": "github_pat" }
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
  stopped      INTEGER NOT NULL DEFAULT 0, -- 1 if this message was cut short by user cancellation
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

#### `push_subscriptions`
Web Push subscription objects from browsers/PWA clients. Stored per-user, one row per subscribed browser instance.

```sql
CREATE TABLE push_subscriptions (
  id            TEXT PRIMARY KEY,         -- UUID v4
  user_id       TEXT NOT NULL,
  endpoint      TEXT NOT NULL UNIQUE,     -- browser-provided push endpoint URL
  p256dh        TEXT NOT NULL,            -- browser public key (base64url)
  auth          TEXT NOT NULL,            -- browser auth secret (base64url)
  user_agent    TEXT,                     -- optional, for display in settings
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
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
Encrypted credential store for API keys, personal access tokens, bearer tokens, and key/secret pairs. Credentials are never exposed to the agent directly — MCP servers resolve them at runtime by key name. OAuth tokens and refresh flows are deferred to future work. The `username`, `email`, and `service_url` fields are stored as plaintext for agent context and UI display (e.g. knowing which account a credential belongs to, linking to the service). All actual secrets are encrypted.

```sql
CREATE TABLE credentials (
  id              TEXT PRIMARY KEY,       -- UUID v4
  key             TEXT NOT NULL UNIQUE,   -- referenced by MCP server configs and provider configs, e.g. "github_pat"
  display_name    TEXT NOT NULL,          -- shown in settings UI, e.g. "GitHub Personal Access Token"
  service         TEXT NOT NULL,          -- what service this is for, e.g. "github", "openai", "anthropic", "custom"
  credential_type TEXT NOT NULL CHECK (credential_type IN ('api_key', 'pat', 'bearer_token', 'key_secret_pair')),
  service_url     TEXT,                   -- optional; human-facing URL, e.g. "https://github.com"
  username        TEXT,                   -- optional; plaintext, safe for agent context
  email           TEXT,                   -- optional; plaintext, safe for agent context
  encrypted_data  TEXT NOT NULL,          -- AES-256-GCM encrypted JSON blob (see schema below)
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**`encrypted_data` JSON schema:**
```json
{
  "secret": "ghp_abc123...",
  "password": "some-password"
}
```

Only `secret` is required. `password` is optional — omit when not applicable.

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

Key values used by the server:

| Key | Set by | Purpose |
|---|---|---|
| `auth_token` | First startup | 64-char hex auth token |
| `machine_secret` | First startup | Encryption key for credentials |
| `credential_master_key` | First startup | Separate key for credential blobs |
| `vapid_public_key` | First startup | VAPID public key for Web Push |
| `vapid_private_key` | First startup | VAPID private key (never exposed via API) |
| `tailscale_hostname` | Tailscale connect | Machine's Tailscale hostname (e.g. `mac-mini.tail1234.ts.net`) |
| `setup_complete` | Setup wizard | Whether first-run setup has been completed |

---

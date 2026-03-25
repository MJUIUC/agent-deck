# Agent-Deck — Project Plan (Part 3: Phased Execution Plan)

> See also: **PLAN_1.md** (Overview, Architecture, Data Model) · **PLAN_2.md** (API Contract, Features, UI, Dev Guide)

---

## 10. Phased Execution Plan

Each story maps to one feature branch. Complete all stories in a phase before starting the next. Run all tests before merging any branch.

**Guiding principle:** Each phase ends with something usable or demonstrable. Backend and frontend are interleaved so you can dogfood early.

---

### Phase 1 — Skeleton That Runs

**Goal:** A Rust server that boots with SQLite, serves a React SPA shell, and has all tables in place. Tailscale integration is wired in so the server knows its own hostname from the start. Nothing functional yet, but the entire foundation is solid and both projects compile.

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

**Story 1.x — Tailscale server integration**  
Branch: `feature/phase1-tailscale-integration`

Implement Tailscale detection and management on the server side. This is a prerequisite for the setup wizard Tailscale step and for the QR pairing endpoint returning a correct server URL.

**Tailscale status detection:**
- On server startup, attempt to run `tailscale status --json` via `tokio::process::Command`
- If the command succeeds and the machine is connected, parse the hostname and store it in `app_config` as `tailscale_hostname`
- This runs non-blocking — server startup is never delayed waiting for Tailscale

**New endpoints (section 6.15):**

`GET /api/tailscale/status` — calls `tailscale status --json` (or checks the binary exists first), returns:
```json
{
  "installed": true,
  "connected": true,
  "hostname": "mac-mini.tail1234.ts.net",
  "auth_url": null
}
```

`POST /api/tailscale/install` — runs the official install script:
```bash
curl -fsSL https://tailscale.com/install.sh | sh
```
Streams output to a temporary log, returns status on completion. macOS only — returns a clear error on other platforms.

`POST /api/tailscale/connect` — runs `tailscale up`, captures the auth URL from stdout/stderr if the machine is not yet authenticated, and returns it. Once connected, stores the hostname in `app_config`.

**Update `GET /api/pairing/qr`:**
Read `tailscale_hostname` from `app_config`. If present, use `http://{tailscale_hostname}:7474` as the `server_url` in the QR payload. If absent (Tailscale not set up), fall back to the machine's local IP address with a note that Tailscale is required for remote access.

Acceptance criteria:
- `GET /api/tailscale/status` returns correct state when Tailscale is installed and connected
- `GET /api/tailscale/status` returns `installed: false` when `tailscale` binary is not found
- `POST /api/tailscale/install` runs the install script and returns updated status
- `POST /api/tailscale/connect` returns an `auth_url` when machine is not yet authenticated
- `POST /api/tailscale/connect` stores `tailscale_hostname` in `app_config` once connected
- `GET /api/pairing/qr` uses `tailscale_hostname` when available
- Server startup Tailscale check is non-blocking
- Unit test: status parsing handles connected, disconnected, and not-installed states

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
- `setup-wizard-tailscale.html` — The Tailscale wizard step in all three states: (1) not installed — install button + explanation; (2) installed/not-connected — "Connect" button + auth URL display with "Open Tailscale login" link; (3) connected — green success box with hostname + auto-advance indicator.

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

### Phase 4 — Credentials and MCP Integration

**Goal:** Establish encrypted credential storage for static secrets (API keys, PATs, bearer tokens), then wire up real MCP server integration with credential resolution, tool discovery, and agent integration. After this phase, agents can use external tools in chat.

---

**Story 4.1 — Credential store and encryption**
Branch: `feature/phase4-credential-store`

Implement the credential storage infrastructure. On first server run, generate a 256-bit master key and store it in `app_config` as `credential_master_key`. Implement AES-256-GCM encrypt/decrypt helpers. Implement `credentials` table CRUD with all data encrypted at rest. The `GET /api/credentials` endpoint returns metadata only — `encrypted_data` is never included in any API response.

Supported credential types: `api_key`, `pat`, `bearer_token`, `key_secret_pair`. No OAuth or refresh token support in this story — static secrets only.

Migrate existing provider API keys: the `providers.api_key` column currently stores keys as plaintext in SQLite. Add a migration that moves each non-null `providers.api_key` value into the `credentials` table as an encrypted entry, updates the provider record to reference the credential by key name, and nulls out the original `api_key` column. After migration, provider key resolution goes through the credential store.

Acceptance criteria:
- Master key is generated once on first run and persists across restarts
- Master key is never included in any log output or API response
- AES-256-GCM encrypt/decrypt helpers are unit tested
- `credentials` table CRUD works for all supported credential types
- `GET /api/credentials` returns metadata only — no `encrypted_data` in any response
- Integration test: store a credential, retrieve it, confirm `encrypted_data` round-trips correctly through decrypt
- Existing provider API keys are migrated into the credential store
- Provider model list and chat still work after migration (key resolution goes through credential store)
- `cargo build` passes, all tests pass

---

**Story 4.2 — Credentials settings UI**
Branch: `feature/phase4-credentials-ui`

Implement `/settings/credentials` page for managing MCP and service credentials. This is separate from the existing provider settings page (which continues to own the provider key entry UX, but now reads/writes through the credential store under the hood).

The credentials page shows a list of all stored credentials with: display name, service label, credential type, and created date. Secret values are never shown — only a masked indicator (e.g. `••••••••`). Add/edit form fields: name, service, credential type, the secret value. Delete with confirmation.

Acceptance criteria:
- Credentials list shows all stored credentials with metadata only
- Add credential form stores encrypted data correctly
- Edit credential allows updating the secret value
- Delete with confirmation removes the credential
- Provider settings page reads/writes API keys through the credential store
- No secret values are ever displayed in the UI or returned by the API

---

**Story 4.3 — MCP connection manager**
Branch: `feature/phase4-mcp-connection-manager`

Implement the server-side MCP connection manager in `services/mcp.rs`. This maintains a pool of active connections keyed by `mcp_server_id` and handles connect, disconnect, and reconnect on error.

Two transport types:
- **Local servers** — spawn the configured executable as a subprocess, communicate over stdio using the MCP protocol. Manage the child process lifecycle (start, monitor, restart on crash with backoff).
- **Remote servers** — connect to the configured URL via HTTP/SSE. Resolve the `credential_key` from the credential store, decrypt, and attach as the configured auth header.

No agent integration yet — this story is purely "can we connect to an MCP server and stay connected."

Acceptance criteria:
- Local MCP server starts as a subprocess and communicates over stdio
- Remote MCP server connects via HTTP/SSE with credential resolution
- Connection pool tracks active connections by server ID
- Reconnect with exponential backoff on connection loss
- Graceful shutdown kills all child processes when the Rust server exits
- `GET /api/mcp-servers` reflects live connection status
- Integration test: connect to a local MCP server, verify connection state
- `cargo build` passes, all tests pass

### As-built notes (hardening fixes applied post-merge)

- **Encryption key bug fixed:** credential routes were using `machine_secret` for encryption while the MCP manager used `credential_master_key` for decryption. `credential_master_key` is now on `AppState` and used consistently everywhere credentials are encrypted or decrypted.
- **MCP Streamable HTTP spec compliance:** `post_rpc` now sends `Accept: application/json, text/event-stream` on every POST (required by spec; absence caused HTTP 406 from compliant servers). Response handling now branches on `Content-Type` — SSE responses are parsed by reading the first `data:` line rather than calling `.json()` directly.
- **Configurable headers:** `RemoteConfig` gained a `headers: HashMap<String, String>` field. Static headers from config are merged with the credential-derived auth header. `McpConnectionInner.auth_header` replaced with `extra_headers: HashMap<String, String>` carried through `call_tool` and `monitor_remote`. Full config chain: `config.json` → DB → UI (new Extra Headers editor + Auth Format field in settings form).
- **Reconnect loop fixed:** after a connection drop, `supervise` was re-fetching `shutdown_rx` from `self.connections` — but the connection had just been removed, so `unwrap_or_else` returned a pre-fired receiver that immediately exited the loop. Fixed by carrying `shutdown_rx` directly out of the `Ok`/`Err` match arms. `Err` arm uses a never-firing `watch::channel(false)`.
- **Duplicate log eliminated:** `connect_local` and `connect_remote` each emitted "server connected" before returning, then `supervise` emitted it again on receipt. Removed the inner logs; `supervise` is now the single emitter with consistent structured fields (`server_id`, `tool_count`).
- **Credential dropdown:** the free-text `credential_key` input in the MCP server form is replaced with a `<select>` populated from `credentialsApi.list()`, showing `display_name (service)` as labels with `key` as the stored value. Falls back to a text input when no credentials exist.

---

**Story 4.4 — MCP tool discovery and agent integration** ✅ Complete
Branch: `feature/phase4-mcp-agent-integration`

Wire MCP servers into the agent run-loop. On connect, enumerate the server's tools and cache the list (name, description, input schema). Expose via `GET /api/mcp-servers/:id/tools`.

In `agent::run_inner`, load the thread's attached MCP servers (`thread_mcp_servers`), fetch their cached tool lists, merge with built-in tools, and inject into the generation loop. Tools are namespaced using the server's `tag` field (e.g. a server with tag `github` exposes tools as `github__create_issue`, `github__search_repos`). When the model calls a namespaced tool, route execution to the corresponding MCP server.

Acceptance criteria:
- ✅ Tool list is fetched on connect and cached in memory
- ✅ `GET /api/mcp-servers/:id/tools` returns the cached tool list
- ✅ Tools are injected into the agent context with `{tag}__{tool_name}` namespacing
- ✅ Tool calls from the model are routed to the correct MCP server
- ✅ Tool results are returned to the model and the conversation continues
- ✅ Tools from multiple servers coexist without name collisions
- ✅ `cargo build` passes, all tests pass

### As-built notes

- `AttachedMcpServer { id, tag }` loaded from `thread_mcp_servers` join at the start of `run_inner`
- `state.mcp.cached_tools(&server.id)` fetches from the in-memory `RwLock<Vec<McpTool>>` on each `McpConnection`
- Tool definitions built as `async_openai::types::ChatCompletionTool` and passed into `context::assemble` via `mcp_tools` field; appended after built-in tools in `build_tool_definitions`
- `execute_tool` dispatches on `tc.name.contains("__")`: splits on first `__`, finds server by tag in `attached_mcp`, calls `state.mcp.call_tool`
- Tool call and result messages persisted with `visibility: hidden` via `persist_tool_message`; surfaced in chat when `show_tool_activity` is enabled on the thread
- Generation loop supports up to 20 consecutive tool-call rounds (`MAX_TOOL_ROUNDS`)
- Malformed tool-call slots (no name or id) are filtered before dispatch to prevent provider 400 errors

---

**Story 4.5 — MCP UI integration** ✅ Complete
Branch: `feature/phase4-mcp-ui`

Polish the MCP experience across the UI.

Acceptance criteria:
- ✅ New threads automatically get default MCP servers attached (per-persona)
- ✅ Tool inspector shows tools per server in thread config and settings
- ✅ Tag field is editable in server add/edit form, defaults to name
- ✅ Connection status badges reflect live state via SSE
- ✅ Source URL renders as a clickable link
- ✅ `cargo build` passes, all tests pass

### As-built notes

- **Auto-attach (deviation from spec):** implemented as per-persona defaults via `persona_default_mcp_servers` table rather than a global `app_config` toggle. `POST /api/threads` queries `persona_default_mcp_servers` for the thread's persona and inserts rows into `thread_mcp_servers`. This is a better design and is retained as canonical; the `app_config` approach from the spec is not implemented.
- **Tool inspector:** `ToolInspector` component in `McpServerSettings.tsx` (settings page) and `McpServerCard` component in `ConfigPane.tsx` (thread config pane). Both show expandable tool list with name + description. Tools fetched lazily from `GET /api/mcp-servers/:id/tools`.
- **Status badges:** `StatusBadge` in both locations. Global SSE `mcp_status_changed` event drives real-time updates.
- **Tag field:** added to `McpForm` with auto-derivation from name (lowercase, spaces→underscores, strip non-`[a-z0-9_-]`). Stops auto-following name once manually edited (`tagTouched` flag). Live preview shows `{tag}__tool_name` in hint. Added `tag` to `McpServer` TypeScript interface.
- **Source URL:** rendered as a clickable external link in `McpServerCard` when present.

---

### Phase 5 — Memory and Routines

**Goal:** Add persistent memory and autonomous scheduled routines. This is what differentiates agent-deck from a chat wrapper — agents remember things across conversations and can act on their own schedule.

**Depends on:** Phase 4 (credential store and MCP integration complete).

---

**Story 5.1 — Per-thread agent run management** ✅
Branch: `feature/phase5-run-management`

This story is a prerequisite for all other Phase 5 stories. Routines, memory tools, and any future server-initiated agent trigger depend on the run lock, cancellation support, and the notify endpoint.

**Part A — Per-thread agent run lock:**
Add a per-thread `RunState` struct to `AppState` (via `DashMap<String, Arc<RunState>>`), containing a `Semaphore` (permit count 1) and an `AtomicUsize` depth counter. Every code path that invokes `agent::run` — `POST /api/threads/:id/messages`, slash command model switch, and the new notify endpoint — must increment the depth counter on entry, acquire the semaphore, and decrement on completion. User-initiated requests are rejected with `429` if depth exceeds 3. Routine-initiated runs are exempt from the depth limit — they always queue, so a scheduled routine is never silently dropped. The message input is no longer disabled while streaming — users can send additional messages while the agent is responding. Queued runs execute in FIFO order and each sees the full message history including prior responses.

**Part B — Cancellation:**
Add a `CancellationToken` (from `tokio_util`) to `RunState`, created fresh at the start of each agent run. Implement `POST /api/threads/:id/cancel` — it triggers the token and returns immediately. The generation loop checks the token at three points:

1. Between streaming chunks — stop accumulating tokens, persist what exists
2. Before dispatching each tool call — if 3 tool calls were requested and 1 completed, do not start the 2nd
3. During in-flight MCP/tool calls — `tokio::select!` the token against the HTTP request

On cancellation:
- Completed tool calls and results from the current run are kept as hidden messages
- In-flight tool calls that were aborted are not persisted
- Partial assistant text is persisted with `stopped = 1`
- If cancel hit during tool execution before any text was generated, persist "Execution stopped" with `stopped = 1`
- The semaphore is released and any queued runs proceed
- A `message_complete` SSE event is emitted with `stopped: true`

**Part C — System notification endpoint:**
Implement `POST /api/threads/:id/notify` per section 6.8.2. Implement the event type registry as a Rust enum with associated `persist` and `trigger` flags and a content template. For `persist: true` events, insert into `messages` with `role: system`, `source: system_event`, `visibility: hidden`, `event_type` set, and broadcast a `system_event` SSE event. For `trigger: true` events, acquire the run lock and invoke `agent::run` with the event payload as the triggering prompt.

**Part D — Schema migration:**
Add `event_type TEXT` column to `messages`. Add `stopped INTEGER NOT NULL DEFAULT 0` column to `messages`. Add `show_system_events INTEGER NOT NULL DEFAULT 0` column to `threads`. Update `Thread` and `Message` models and all affected SELECT/INSERT/UPDATE queries.

**Part E — Client integration:**
- Remove the input disable during streaming. Users can type and send while the agent is responding. Show a subtle queued indicator (e.g. "1 message queued") when messages are waiting behind an active run.
- While streaming, the send button transforms into a stop button (■ icon). Tapping it calls `POST /api/threads/:id/cancel`. After cancellation, the button returns to send mode.
- Stopped messages render with a subtle "stopped" label (similar to the "routine" label on routine messages).
- After model switch in `ConfigPane`, call `POST /api/threads/:id/notify` with `event_type: model_switched`. After MCP attach/detach, emit the corresponding event. These are fire-and-forget.

**Part F — `show_system_events` toggle:**
Add the toggle to the Thread Config pane (below the existing `show_tool_activity` toggle). Wire it to `PUT /api/threads/:id`.

**Part G — Orphaned routine execution cleanup:**
On server startup, before the routine scheduler registers any jobs, query `routine_executions WHERE status = 'running'` and update them to `status: 'failed'` with `error: 'server restarted during execution'`. This prevents stale `running` rows from accumulating after crashes or restarts. Chat-initiated agent runs are fire-and-forget — no recovery is attempted for those.

Acceptance criteria:
- [x] Per-thread semaphore prevents concurrent agent runs on the same thread
- [x] Concurrent user messages queue and run after the current completes
- [x] Queue depth > 3 returns `429` for user-initiated requests
- [x] Routine-initiated runs always queue regardless of depth
- [x] Message input is not disabled during streaming; users can send while agent is responding
- [x] Queued message indicator appears when messages are waiting
- [x] `POST /api/threads/:id/cancel` stops an active run
- [x] Cancel during streaming persists partial response with `stopped = 1`
- [x] Cancel during tool execution stops before the next tool call
- [x] Cancel during in-flight MCP call does not wait for the call to finish
- [x] Stopped messages render with a visual "stopped" label in chat
- [x] Send button transforms to stop button during streaming, reverts after
- [x] `POST /api/threads/:id/notify` accepts all registered event types and rejects unknown ones with `400`
- [x] `persist: true` events insert a hidden system message and broadcast `system_event` SSE
- [x] `trigger: true` events invoke the agent run loop and produce a visible response
- [x] `persist: false, trigger: false` is rejected
- [x] `model_switched` event visible in message history (with `?include_hidden=true`)
- [x] Client calls notify after model switch; hidden message appears in DB
- [x] `show_system_events` toggle persists per-thread
- [x] On startup, any `routine_executions` rows with `status = 'running'` are set to `failed`
- [x] The orphan cleanup runs before the scheduler starts registering jobs
- [x] Unit tests for event registry (valid types, invalid type rejection, flag combinations)
- [x] Unit tests for cancellation (mid-stream, mid-tool, no active run)
- [x] `cargo sqlx prepare` run and `.sqlx/` committed

---

**Story 5.2 — Memory tools** ✅
Branch: `feature/phase5-memory-tools`

**Prerequisite — Default persona:**
Add a migration that seeds the Default persona row in `agent_personas` with `is_default = 1`, empty `system_prompt`, emoji `💬`, no avatar. Guard the `DELETE` and `PUT` persona endpoints against modification of the default row (`403`). Update the persona selector in the new-thread and thread-config UI to display the Default persona as "None" with the hint: "Without a persona, long-term memory is not available."

**Memory tools:**
Implement the `save_memory` and `recall_memory` tool definitions per section 7.6.2. Wire them into the agent run-loop so they are available as callable tools **only when the thread's persona is not the Default persona**. When the thread uses the Default persona, omit the memory tools from the tool list and do not append the memory system prompt. Append the memory system prompt instructions (section 7.6.3) after the persona's system prompt in every request (for non-default personas). `save_memory` inserts a new memory row with `user_id`, `persona_id` (from the thread's persona), and `thread_id` (provenance), enforcing the 500-character content limit, and updates the FTS index. `recall_memory` queries `memory_fts` filtered by the current user and persona, capped at 10 results, formatted per section 7.6.5.

**Iteration note:** Memory recall reliability depends on the quality of the system prompt instructions and varies by model. The prompt-only approach (no auto-injection of memories into context) is the v1 design. Expect iteration on the memory system prompt wording after dogfooding. Auto-injection of recent memories into context is a potential future enhancement if models prove unreliable at calling `recall_memory` proactively.

Acceptance criteria:
- [x] Default persona exists after migration; has `is_default = 1`, empty system prompt
- [x] `DELETE /api/personas/:id` returns `403` for the Default persona
- [x] `PUT /api/personas/:id` returns `403` for the Default persona (name and delete protected)
- [x] Persona selector shows Default persona as "None" with memory hint text
- [x] New threads without a specified persona are assigned the Default persona
- [x] Memory tools are NOT included in tool list when thread uses Default persona
- [x] Memory system prompt is NOT appended when thread uses Default persona
- [x] Unit tests for FTS search returning correct results
- [x] Unit tests for memory insertion with correct persona scoping
- [x] Unit tests for the 500-character truncation
- [x] Memories saved in one thread are recallable from another thread with the same persona
- [x] Memories are NOT recalled when querying from a different persona
- [x] Recall results are capped at 10 and include date prefix and thread provenance
- [x] Empty recall returns the "no memories found" message
- [x] The memory system prompt is appended to every request for non-default personas (after persona prompt, before thread addendum)
- [x] The tools are included in LLM requests as function definitions (non-default personas only)
- [x] Integration test: save a memory, send a follow-up message in a different thread (same persona) that should trigger recall, verify the tool is called — deferred; covered by unit tests for each layer individually

### As-built notes (Story 5.2)

- **Migration 008** adds `is_default INTEGER NOT NULL DEFAULT 0` to `agent_personas` and seeds the Default persona for any existing users. Fresh installs get the Default persona created inside `POST /api/setup/complete` immediately after the user row is inserted, using a new `AgentPersona::new_default()` constructor.

- **`is_default` guard coverage:** `DELETE`, `PUT`, and `POST /:id/avatar` all return `403 Forbidden` for the Default persona. The `Forbidden(String)` variant was added to `AppError` as part of this work.

- **`persona_id` is now optional on thread creation:** `CreateThread.persona_id` is `Option<String>`. When absent or empty, the server resolves the user's Default persona automatically. `Thread::new()` signature updated to accept `persona_id` as a separate argument.

- **Context assembly gating:** `AssemblyInput` gained an `include_memory: bool` field. When `false` (Default persona), both the `## Memory` system-prompt block and all built-in tool definitions are excluded. MCP tools are unaffected. The flag is set in `agent.rs` as `!persona.is_default`.

- **`delete_memory` tool added** (beyond original spec, motivated by the memory cap design discussion): the model can now evict duplicate or stale entries by ID. `recall_memory` output was updated to prefix each result line with `[id:<uuid>]` so the model has a reference to pass to `delete_memory`. The `MEMORY_INSTRUCTIONS` system prompt was extended with a **When to delete** paragraph covering the dedup-before-save and store-full-cleanup workflows. Built-in tool count is now 3.

- **Persona picker hint text:** rendered as "Long-term memory not available" (slightly shorter than the spec's "Without a persona, long-term memory is not available" — fits the card layout cleanly).

- **Test count:** 228 passing (was 224 at end of Story 5.1). New tests cover FTS search, persona scoping, cross-thread recall, recall cap, empty recall, 500-char truncation, empty content guard, delete by ID, delete not-found, and recall output format.

---

**Story 5.3 — Routines CRUD and cron scheduler**✅
Branch: `feature/phase5-routines`

Implement all routine endpoints from section 6.10. Implement the routine scheduler service using `tokio-cron-scheduler`. On server startup, load all enabled routines from the DB and register them. When a routine is created, updated, or toggled via the API, update the scheduler accordingly. When a thread is archived, pause its routines. When unarchived, resume them.

Acceptance criteria:
- Full CRUD works
- Toggle endpoint correctly flips the enabled flag
- Integration tests for all endpoints
- Routines fire at the correct time (test with a short interval like every minute)
- Pausing and resuming works correctly
- Server restart re-registers all active routines from DB

---

**Story 5.4 — Routine execution** ✅

> **Depends on Story 5.1** — the routine scheduler uses the notify endpoint with `event_type: routine_fired` (`persist: false, trigger: true`) to invoke the agent.

Branch: `feature/phase5-routine-execution`

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

> **Note:** Web Push notification dispatch (when no SSE clients are connected) is implemented in Phase 7 Story 7.2.

Acceptance criteria:
- Integration test: create a routine with a 1-minute schedule, verify it fires, hidden intermediate messages are stored, and one visible result message appears
- No hidden messages appear in `GET /api/threads/:id/messages` response (filtered by default; `?include_hidden=true` param exposes them)
- Visible result message has `source: routine` in DB
- `routine_executions` row correctly tracks status and output_message_id
- SSE events are emitted correctly
- `last_run_at` and `run_count` are updated after each run

---

**Story 5.5 — Routine UI integration**
Branch: `feature/phase5-routine-ui`

Wire routines into the thread config pane (the shell from Story 3.5 — now with full add/edit/delete/toggle functionality). Routine-generated messages in the chat view should be visually distinct (subtle different background using `bubble_routine` color, small "routine" label).

Acceptance criteria:
- [x] Routines can be added, edited, deleted, and toggled from the thread config pane
- [x] Cron expression field shows human-readable description below it
- [x] Routine messages in chat are visually distinct from regular messages
- [x] Thread list updates when a routine fires (via global SSE)

---

**Story 5.6 — Memory UI integration**
Branch: `feature/phase5-memory-ui`

Add memory viewer per section 7.6.7:
- In the thread config pane: a "Memory" section showing all persona memories (not thread-scoped). This section is hidden when the thread uses the Default persona (no persona).
- In settings under each persona: a full memory list for that persona with content, date, source thread name, and delete button. The Default persona's settings page does not show a memory section.
- `/memory list` slash command returning last 20 memories as an ephemeral message. When issued in a thread using the Default persona, returns a message explaining that memory is not available without a persona.

Acceptance criteria:
- [x] Thread config pane shows memories for the active persona (non-default persona only)
- [x] Memory section is hidden in thread config when thread uses Default persona
- [x] Persona settings page shows all memories for that persona with delete capability
- [x] Default persona settings page does not show memory section
- [x] `/memory list` command works and displays results as ephemeral message
- [x] `/memory list` in a Default persona thread returns explanatory message
- [x] Memory count badge visible in persona settings (non-default personas only)

### As-built notes (Story 5.6)

- **Memory section scope changed:** The spec called for filtering by `thread_id` provenance ("what did the agent learn in this conversation"). After review this was changed to show all persona memories — the more useful question is "what does this agent know?" not "what did it happen to save here." The `GET /api/personas/:id/memory` endpoint gained an optional `?thread_id=` filter for future use but the UI does not apply it.

- **ConfigPane restructured:** The thread config pane was reorganised into two tiers. Top-level (always visible): Persona, Model, Routines, MCP Servers. Collapsible Advanced section (collapsed by default): Memory (with per-entry delete and N/500 count), Show tool activity, Show system events, System Prompt Addendum, Archive. The two developer toggles were moved out of the MCP section where they had been embedded.

- **Memory list is expandable:** Shows 2 entries by default with "Show N more / Show less" toggle, matching the Routines and MCP Servers pattern. Resets to collapsed on thread switch.

- **Recall quality fixes:** The FTS5 query was being passed raw to SQLite, causing two bugs: (1) multi-word queries used implicit AND so `"dog name"` required both words in the same entry; (2) hyphenated terms like `agent-deck` were parsed as `agent NOT deck` giving "no such column: deck". Fixed with `build_fts_query()` which normalises hyphens to spaces and joins tokens with `OR` + prefix wildcards (`dog* OR name*`). Queries containing explicit FTS5 operators pass through unchanged.

- **`MEMORY_INSTRUCTIONS` strengthened:** The recall rule now explicitly states the model must call `recall_memory` before claiming ignorance. Tool description updated to explain OR semantics and instruct retry with different keywords before concluding a memory doesn't exist.

- **`show_tool_activity` fully implemented:** Previously the toggle was stored in the DB but the messages list always filtered to `visibility = 'visible'`. Fixed: server adds `include_hidden` query param; when true, returns hidden `source = 'tool'` messages alongside visible ones. Client always fetches with `include_hidden=true` and filters display based on `show_tool_activity`. Built-in tool calls (memory ops) now persist hidden call+result records matching the MCP tool format.

- **Pre-tool text preserved:** `generation_loop` previously only added the final turn's text to `final_content`. Text streamed before a tool call was visible during streaming but absent from the persisted message. Fixed: pre-tool turn text is now folded into `final_content` with `\n\n` separator. A matching `\n\n` separator token is emitted via SSE between turns so the streaming bubble stays in sync.

- **Message input focus:** Auto-focuses the textarea on thread open/switch. Restores focus after streaming ends (the DOM swap from streaming bubble to final message was dropping focus to `document.body`).

---

**Story 5.7 — User profile**
Branch: `feature/phase5-user-profile`

Implement the user profile feature per section 7.11.

**Part A — Migration and model:**
Add a new migration that adds `pronouns`, `role`, `organization`, `location`, `timezone`, `about`, and `profile_updated_at` columns to the `users` table. Update the `User` model struct to include all new fields. Add a `UpdateUserProfile` request struct for the PUT endpoint.

**Part B — API endpoints:**
Implement `GET /api/profile` and `PUT /api/profile` per section 7.11.4. The PUT handler uses a partial-update pattern (only sent fields change; `null` explicitly clears a field). Update `profile_updated_at` on every successful PUT.

**Part C — Context injection:**
Add `user_profile_context: Option<String>` to `AssemblyInput` in `context.rs`. In `assemble()`, inject it as a system message at position 1.5 (after the persona system prompt, before the thread addendum) when the value is `Some` and non-empty. In `run_inner` in `agent.rs`, load the user profile, build the formatted context block (omitting empty fields, skipping entirely if only `display_name` is set), and pass it through. Do not inject for the Default persona.

**Part D — Setup wizard step:**
Add a new "About You" step to the setup wizard between the Name step and the Provider step. All fields are optional. The step auto-detects the user's timezone from `Intl.DateTimeFormat().resolvedOptions().timeZone` and pre-fills the timezone hint. Include a "Skip for now" link.

**Part E — Settings UI:**
Add a Profile section to Settings → General. All fields are edit-in-place (blur to save). The `about` field shows a 500-character counter. Timezone is an editable text input with the auto-detected value pre-filled on first open if the field is currently empty.

**Part F — Persona settings hint:**
Add the informational hint below the system prompt textarea in the persona form per section 7.11.7.

Acceptance criteria:
- [ ] Migration adds all profile columns to `users` with NULL defaults
- [ ] `GET /api/profile` returns all fields; empty fields are `null`
- [ ] `PUT /api/profile` updates only the provided fields; sending `null` clears the field
- [ ] `profile_updated_at` is updated on every PUT
- [ ] Context block is injected between persona prompt and thread addendum for non-default personas
- [ ] Context block is NOT injected when only `display_name` is set (all other fields null/empty)
- [ ] Context block is NOT injected for the Default persona
- [ ] Only non-empty fields appear in the injected block
- [ ] Timezone auto-detected from browser in setup wizard and settings
- [ ] "About You" step is skippable from the setup wizard
- [ ] Profile section appears in Settings → General
- [ ] 500-character limit enforced on `about` field (server-side truncation, client-side counter)
- [ ] Persona settings hint appears below the system prompt textarea
- [ ] Unit tests for context block formatting (all fields, partial fields, no fields)
- [ ] Unit tests for partial PUT update logic

---

**Story 5.8 — Context window summarization**
Branch: `feature/phase5-summarization`

Implement rolling context-window summarization per section 7.12. Replaces the current silent sliding-window discard with a two-trigger system: proactive (message count threshold) and reactive (context-length 400 error detection and retry).

**Part A — Migration and model:**
Add migration 009 with `summary TEXT`, `summary_updated_at TEXT`, `summary_message_count INTEGER NOT NULL DEFAULT 0`, and `auto_summarize INTEGER NOT NULL DEFAULT 1` columns on `threads`. Update `Thread` struct and `UpdateThread` to include `auto_summarize`. `summary` and `summary_message_count` are server-managed and not accepted on PUT.

**Part B — Summarization service:**
Add `services/summarization.rs`. Implement `pub async fn summarize_thread(state: &AppState, thread_id: &str) -> Result<()>` which:
1. Loads the thread; checks `auto_summarize`; if false, returns immediately
2. Counts total visible messages; if count equals `summary_message_count`, returns (already current)
3. Loads all visible messages up to and including the current `summary_message_count + DEFAULT_HISTORY_LIMIT` boundary — these are the messages being summarised
4. If fewer than `DEFAULT_HISTORY_LIMIT` messages would be summarised, returns early (not enough new content)
5. Builds the summarisation prompt per §7.12.5, prepending any existing summary so the new summary covers the full history from the beginning
6. Calls the thread's active provider with no tools and no streaming (regular completion)
7. On success: writes result to `threads.summary`, updates `summary_updated_at` and `summary_message_count` to the current total visible message count
8. On any failure: logs at WARN, returns — must never propagate or surface to the user

**Part C — Proactive trigger:**
At the end of `run_inner` in `agent.rs`, after the `ThreadUpdated` SSE event is emitted: count total visible messages for the thread. If `auto_summarize` is true and `(total_count - thread.summary_message_count) >= DEFAULT_HISTORY_LIMIT`, spawn a background task calling `summarize_thread`. Fire-and-forget — does not block the response.

**Part D — Reactive trigger (context-length error):**
In `retry_strategy_for`, add a new match arm that detects context-length errors (check for "context_length_exceeded", "context window", "maximum context length", "prompt is too long" in the error message). Return a new `RetryStrategy::SummarizeAndRetry` variant. In `stream_one_turn` (or its caller), handle this variant by: calling `summarize_thread` synchronously, rebuilding `AssemblyInput` with the updated summary and reduced history, and retrying the provider call once. If it fails again, fall through to the normal error path.

**Part E — Context injection:**
Add `conversation_summary: Option<String>` to `AssemblyInput` in `context.rs`. In `assemble()`, inject it at position 2.5 per §7.12.4 when `Some` and non-empty. The history query in `run_inner` changes from "last 20 visible messages" to "visible messages after the `summary_message_count` boundary, capped at `DEFAULT_HISTORY_LIMIT`". Update all existing `AssemblyInput` constructions in tests to include `conversation_summary: None`.

**Part F — UI: Advanced section in ConfigPane:**
Refactor the thread config pane per §7.12.8:
- Add a collapsible Advanced section at the bottom (collapsed by default)
- Move show-tool-activity and show-system-events toggles into it
- Move Archive button into it (remove the Danger Zone section)
- Add auto-summarize toggle
- Show "Last summarized · [date] · [N] messages covered" hint when summary exists
- Update `Thread` TypeScript type to include `summary`, `summary_updated_at`, `summary_message_count`, `auto_summarize`

Acceptance criteria:
- [ ] Migration adds all four columns with correct defaults
- [ ] `GET /api/threads/:id` includes new fields
- [ ] `PUT /api/threads/:id` accepts `auto_summarize`; ignores `summary` if sent
- [ ] No summarization when `auto_summarize = 0`
- [ ] No summarization when fewer than `DEFAULT_HISTORY_LIMIT` new messages since last summary
- [ ] Re-summarization guard: no-op when `summary_message_count` already matches current count
- [ ] Proactive trigger fires as background task after turn when threshold is reached
- [ ] Proactive trigger does not block the user response
- [ ] Reactive trigger detects context-length 400 and runs summarization synchronously before retry
- [ ] After reactive summarization, the retry uses the new summary and succeeds (assuming summary reduces context sufficiently)
- [ ] Summary is injected between thread addendum and history
- [ ] History loaded is messages AFTER the summary boundary, not unconditional last-20
- [ ] Summary injection is absent when `summary` is null
- [ ] Summarization failure is logged at WARN and not surfaced to the user
- [ ] `summary_message_count` reflects the message count at the time of summarization
- [ ] Advanced section in ConfigPane collapsed by default; expands on click
- [ ] Auto-summarize toggle persists; show-tool-activity and show-system-events toggles work from new location
- [ ] Archive button works from new location
- [ ] "Last summarized" hint visible when summary exists
- [ ] Unit tests: summarization prompt construction, context injection (with/without summary), re-summarization guard, boundary query (messages after summary_message_count)

---

### Phase 6 — PWA

**Goal:** The React SPA is installable as a Progressive Web App on Android and iOS. Users can add it to their home screen and open it full-screen. Mobile layout is polished. The foundation for push notifications (service worker) is in place.

---

**Story 6.1 — PWA manifest and installability**  
Branch: `feature/phase6-pwa-manifest`

Add `web/public/manifest.json` with the correct fields for installability. Link it from `index.html`. Add app icons (192×192 and 512×512, generated from the agent-deck emoji/theme). Install `vite-plugin-pwa` as a dev dependency and configure it to inject the manifest link and register the service worker. The service worker at this stage only needs to handle the `push` event (implemented in Phase 7) — a minimal stub is sufficient now.

Required `manifest.json` fields:
- `name`: "agent-deck"
- `short_name`: "agent-deck"
- `display`: "standalone" — **required for iOS push notifications**
- `start_url`: "/"
- `background_color`: "#1C1C1A"
- `theme_color`: "#1C1C1A"
- `icons`: 192×192 and 512×512 PNG entries

Acceptance criteria:
- Chrome on Android shows "Add to Home Screen" prompt or banner
- iOS Safari shows the app name and icon when going through Share → Add to Home Screen
- PWA opens full-screen (no browser UI) when launched from home screen on both platforms
- `vite-plugin-pwa` is in devDependencies and configured in `vite.config.ts`
- Service worker is registered without errors (stub is fine at this stage)
- Lighthouse PWA audit passes installability checks

---

**Story 6.2 — Mobile layout polish**  
Branch: `feature/phase6-mobile-layout`

Audit and fix any layout issues on small viewports (375px–430px). The sidebar, chat view, config pane, message input, and settings modal all need to feel correct on a phone screen. The existing mobile sidebar slide-in works — focus on the remaining gaps.

Specific items to address:
- `ChatView`: messages should not be obscured by the keyboard on mobile — handle `visualViewport` resize or `env(keyboard-inset-height)` for the input bar
- `ConfigPane`: on mobile (< 640px) should slide in full-width rather than as a side panel over the chat
- `SettingsModal`: on mobile should be full-screen rather than a centered modal
- Tap targets: all buttons must be at least 44×44px on mobile
- No horizontal scroll anywhere on 375px viewport

Acceptance criteria:
- Chat input stays above the keyboard when it opens on iOS and Android
- Config pane and settings modal are full-screen on mobile
- No layout bugs at 375px, 390px, or 430px viewport widths
- All interactive elements meet minimum tap target size

---

**Story 6.x — Tailscale wizard step and mobile settings**  
Branch: `feature/phase6-tailscale-wizard`

Wire the Tailscale setup step into the setup wizard and update the mobile settings page with Tailscale install guidance.

**Setup wizard — Tailscale step (new Step 2, between Welcome and Provider):**

Add a new `Step2Tailscale.tsx` wizard step. On mount, call `GET /api/tailscale/status`.

- **Connected state:** Green status box showing "✓ Connected · {hostname}". "Next →" button. Auto-advances after 1.5 seconds.
- **Installed, not connected state:** "Connect to Tailscale" primary button. On click, call `POST /api/tailscale/connect`. If response contains `auth_url`, show:
  ```
  Open this link in your browser to authorise this machine:
  [Open Tailscale login →]  (opens in new tab)
  ```
  Poll `GET /api/tailscale/status` every 2 seconds. When `connected: true`, auto-advance.
- **Not installed state:** Brief explanation ("Tailscale is required for your phone and other devices to reach agent-deck privately."). "Install Tailscale" primary button — calls `POST /api/tailscale/install`, shows spinner with status text. On success transitions to "not connected" state.
- **Skip link** always visible at bottom in tertiary text: "Skip — set up Tailscale later". Skipping advances the wizard without storing a hostname.

**Mobile settings page — Tailscale section:**

Add a new "Tailscale" section to `MobileSettings.tsx` above the existing QR code section.

Content:
- Current connection status (fetched from `GET /api/tailscale/status`)
  - Connected: "✓ mac-mini.tail1234.ts.net" in accent color
  - Not connected: "Not connected" with a "Connect in wizard" link that re-opens the setup wizard at the Tailscale step
- **Install on your phone:** Two buttons/links:
  - "Tailscale for iOS" → `https://apps.apple.com/app/tailscale/id1470499037`
  - "Tailscale for Android" → `https://play.google.com/store/apps/details?id=com.tailscale.ipn.android`
- Brief instruction: "Sign in to Tailscale on your phone using the same account as this Mac Mini. Then open the server URL below in Safari (iOS) or Chrome (Android)."
- Server URL displayed as copyable text: the `tailscale_hostname`-based URL, or a placeholder if not connected

**Update QR code label:**
The existing QR code section should update its "How to pair" instructions to mention Tailscale step 1: "Make sure Tailscale is running on your phone and you're signed in to the same account."

Acceptance criteria:
- Tailscale wizard step renders all three states correctly (connected, installed/not-connected, not-installed)
- Polling works and auto-advances when connection is established
- Skip link works and advances wizard without error
- Mobile settings Tailscale section shows correct connection status
- App Store and Play Store links open correctly
- Server URL in mobile settings uses `tailscale_hostname` when available
- QR code "How to pair" instructions include Tailscale prerequisite

---

**Story 6.3 — Updated mobile settings page**  
Branch: `feature/phase6-mobile-settings`

Update `MobileSettings.tsx` to replace the Android-only QR pairing instructions with the PWA install flow. The page should help users install the PWA and enable notifications.

Content:
- Platform detection (iOS vs Android via user agent) to show the correct install steps
- **iOS:** "Open in Safari → tap Share → Add to Home Screen → open from home screen"
- **Android:** "Open in Chrome → tap menu → Add to Home Screen"
- Notification subscription status: "Notifications enabled" / "Notifications not enabled" / "Permission denied"
- "Enable Notifications" button — only shown if status is not yet subscribed and permission not denied. Triggers the push permission prompt (must be a user gesture). Disabled and hidden until Phase 7 VAPID endpoint is available — render a placeholder for now.
- QR code section remains for sharing the server URL to new devices (unchanged from existing implementation)

Acceptance criteria:
- iOS and Android show different install instructions based on user agent
- Notification status section renders correctly in all three states
- "Enable Notifications" button is present but clearly marked as coming in a future update (or hidden, TBD in Phase 7)
- QR code generation still works

---

### Phase 7 — Web Push Notifications

**Goal:** The PWA receives push notifications when routines fire and no SSE client is connected. No Firebase, no Google/Apple accounts, no external registration. Works on Android (Chrome) and iOS (Safari 16.4+, home screen install required).

**No prerequisites.** VAPID keys are auto-generated on first server startup. No external setup is needed before starting.

---

**Story 7.1 — VAPID key generation and server endpoints**  
Branch: `feature/phase7-vapid-server`

On server startup, check `app_config` for `vapid_public_key` and `vapid_private_key`. If absent, generate a new VAPID P-256 key pair using the `web-push` crate and store both in `app_config`. The private key must never be logged or returned by any API endpoint.

Add three new endpoints:

**`GET /api/push/vapid-public-key`** — public, returns:
```json
{ "public_key": "<base64url encoded public key>" }
```

**`POST /api/push/subscribe`** — authenticated, body:
```json
{
  "endpoint": "https://...",
  "p256dh": "<base64url>",
  "auth": "<base64url>",
  "user_agent": "Chrome/Android"
}
```
Upserts into `push_subscriptions` (insert or update by endpoint).

**`DELETE /api/push/subscribe`** — authenticated, body:
```json
{ "endpoint": "https://..." }
```
Removes the subscription row.

Acceptance criteria:
- VAPID keys are generated once and persist across server restarts
- Private key is never logged or returned by any endpoint
- All three endpoints work correctly
- `push_subscriptions` table is created by migration (add migration `009_push_subscriptions.sql`)
- Unit tests for key generation idempotency (calling generate twice returns the same key)

---

**Story 7.2 — Web Push dispatch from server**  
Branch: `feature/phase7-web-push-dispatch`

In the routine execution service, after persisting the routine response, check whether any SSE client is currently connected for the thread. If not, send a Web Push notification to all `push_subscriptions` rows for the user using the `web-push` crate.

Notification payload (JSON, encrypted by the crate):
```json
{
  "title": "🦉 Aldous",
  "body": "<first 100 chars of routine response>",
  "data": { "thread_id": "<uuid>" }
}
```

If a subscription endpoint returns HTTP 410 (Gone), delete that subscription row — it means the browser has revoked it.

Acceptance criteria:
- Notification is sent when no SSE client is connected for the thread
- Notification is NOT sent when an SSE client is connected
- 410 responses from push endpoints cause the subscription to be deleted
- Routine execution wires up the dispatch (fulfils the TODO from Story 5.4 step 10)
- Unit test for the "should notify" decision logic

---

**Story 7.3 — PWA service worker and client subscription**  
Branch: `feature/phase7-pwa-push-client`

Implement the service worker push handler and the client-side subscription flow.

**Service worker (`web/public/sw.js` or generated by vite-plugin-pwa):**
```javascript
self.addEventListener('push', (event) => {
  const { title, body, data } = event.data.json();
  event.waitUntil(
    self.registration.showNotification(title, {
      body,
      icon: '/icon-192.png',
      data,
    })
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const threadId = event.notification.data?.thread_id;
  if (threadId) {
    event.waitUntil(
      clients.openWindow(`/?thread=${threadId}`)
    );
  }
});
```

**Client subscription flow (wired into the "Enable Notifications" button in `MobileSettings.tsx`):**
1. Fetch VAPID public key from `GET /api/push/vapid-public-key`
2. Call `navigator.serviceWorker.ready` to get the active service worker registration
3. Call `registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: vapidPublicKey })`
4. POST the resulting subscription object to `POST /api/push/subscribe`
5. Update UI to show "Notifications enabled"

Handle unsubscribe: call `subscription.unsubscribe()`, then `DELETE /api/push/subscribe`.

Acceptance criteria:
- "Enable Notifications" button triggers the browser permission prompt
- After granting permission, subscription is stored on the server
- Push notification is received and shown on Android Chrome when a routine fires with no SSE client connected
- Push notification is received and shown on iOS Safari (PWA installed, home screen launch) when a routine fires
- Tapping the notification opens the PWA and navigates to the correct thread
- "Notifications enabled" / "Notifications not enabled" status in settings reflects actual subscription state
- Unsubscribe flow works and removes the subscription from the server

---

### Phase 8 — MCP Depth

**Goal:** MCP servers are fully first-class. Tool inspector works, local server process management is robust, and the platform is ready for any MCP integration.

---

**Story 8.1 — MCP tool inspector**  
Branch: `feature/phase8-mcp-tool-inspector`

When an MCP server connects, enumerate its exposed tools and cache the list (name, description, input schema) in memory. Expose this via `GET /api/mcp-servers/:id/tools`. In the Thread Config pane and MCP settings page, render the tool list as an expandable section per server. Include the source URL as a clickable link when set.

Acceptance criteria:
- Tool list is fetched and displayed in Thread Config per attached server
- Tool list is displayed in MCP settings per server
- Source URL renders as a link when present
- Tool list refreshes when a server reconnects
- Unit tests for tool enumeration and caching

---

**Story 8.2 — Local MCP process management**  
Branch: `feature/phase8-local-mcp-processes`

Implement full lifecycle management for local MCP servers. The Rust server starts local servers as child processes on demand (when a thread with that server is opened or on server startup if the server has active threads). Health-check loop monitors the process. On crash, attempt restart with exponential backoff. Status is kept live in the `mcp_servers.status` field and broadcast via global SSE event. Graceful shutdown on server exit.

Acceptance criteria:
- Local server starts on demand and is restarted on crash
- Restart backoff prevents tight crash loops
- Status badge in UI reflects actual connection state in near-real-time
- All local server processes are cleanly shut down when the Rust server exits
- Integration test: start a local server, kill its process, verify restart and reconnection

---

### Phase 9 — Polish and Hardening

**Goal:** The system is reliable, handles errors gracefully, and provides a good experience end-to-end.

---

**Story 9.1 — Error handling and user feedback**  
Branch: `feature/phase9-error-handling`

Audit all error paths in the Rust server and ensure they return consistent, meaningful error responses. Audit the React SPA and add toast notifications for API errors. Ensure SSE errors are surfaced to the user. Add retry logic for transient provider errors.

---

**Story 9.2 — SSE reconnection and resilience**  
Branch: `feature/phase9-sse-resilience`

Implement robust SSE reconnection in both the web and mobile clients. Use the `Last-Event-ID` header to resume from the last received event. Ensure no messages are lost during a brief disconnect.

---

**Story 9.3 — Message pagination**  
Branch: `feature/phase9-message-pagination`

Implement cursor-based pagination on `GET /api/threads/:id/messages`. In the web and mobile apps, implement "load more" by scrolling to the top of the message list.

---

**Story 9.4 — Setup and README**  
Branch: `feature/phase9-docs`

Write a comprehensive README covering: what agent-deck is, prerequisites, installation steps (including `git submodule init` for copilot-api), first-run setup, PWA installation on Android and iOS, push notification setup, and how to add providers. Document the Tailscale setup.

---

### Phase 10 — Status Bar App (Deferred)

Deferred until all other phases are complete. See section 11 for notes.

---

## 11. Deferred / Future Work

The following items are explicitly out of scope for v1. They are documented here so future contributors have context.

### Status Bar App (Phase 10)
A native macOS Swift/SwiftUI app that lives in the menu bar. It manages the Rust server process and optionally the `copilot-api` process. Shows server status (running/stopped), active thread count, and allows starting/stopping the server. Registers as a Login Item so it starts on boot. The `.app` bundle allows it to appear in Launchpad and Spotlight. Planned for after all other phases are complete.

### OAuth and Third-Party App Credentials
Browser-based OAuth flows (Google, GitHub, etc.) with token refresh, consent screens, and callback handling. Required to unlock Gmail, Google Calendar, Google Drive, and any MCP server that authenticates via OAuth rather than static tokens. Includes: OAuth provider trait and registry, token refresh on credential resolution, `/settings/accounts` page for connected accounts, and the Google app verification process for consumer distribution. This is a significant UX and infrastructure investment — deferred until the core platform is stable and the credential store, MCP integration, and agent runtime are proven out. When ready, the credential store already supports the encrypted storage layer; the work is in adding the browser flow, refresh logic, and new credential types (`oauth2` with `scopes`, `expires_at`, `refresh_token`).

### Native iOS App
No native iOS app is planned. The PWA covers iOS via Safari Web Push (iOS 16.4+). Users install the PWA via "Add to Home Screen" in Safari — no App Store, no Developer account required. If a richer native experience is ever desired (e.g. background audio, Siri integration), a native wrapper could be added as a paid tier in a future version.

### Multi-User Support
The data model includes `user_id` on all relevant tables in anticipation of this. In v1 there is one user. Future: add a login screen, user management, per-user API keys, per-user thread isolation.

### Hard Delete
Threads can be archived in v1. Hard delete (with full cascade through messages, routines, thread_skills, etc.) requires careful design to avoid accidental data loss. In v1, use soft delete only.



### Additional OAuth Providers
The OAuth provider framework (deferred — see "OAuth and Third-Party App Credentials" above) is designed to be modular — adding a new provider (Notion, Linear, Slack, etc.) requires only implementing the `OAuthProvider` trait and registering in the provider registry. Future providers follow the same pattern with no infrastructure changes.

### Memory as MCP
The current memory system is a baked-in tool-calling implementation. A future version should expose it as a local MCP server instead, making it swappable. This would allow plugging in a different memory backend (e.g., a vector database) without touching the core agent run-loop. Deferred to avoid scope expansion in v1.

### Semantic / Vector Memory (revised)
The v1 memory system uses SQLite FTS5 keyword search. If memory is migrated to an MCP server pattern (above), the backend can be swapped to `sqlite-vec` for vector embeddings, enabling semantic recall.

### Thread Hard Search
Full-text search across all threads and messages. Useful as the thread list grows. SQLite FTS5 on the `messages` table would power this.

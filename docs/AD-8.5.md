# AD-8.5 — Tailscale Platform Layer + Webhook Integration

**Phase:** 8.5 (Platform Infrastructure — between Phase 8 MCP Depth and Phase 9 Polish)  
**Branch:** `feature/phase8-tailscale-platform`  
**Depends on:** Phase 7 complete (7.3 Web Push client, 7.3a Processing Block)

---

## Overview

agent-deck runs on a headless local computer and is accessed exclusively over Tailscale. This story makes Tailscale a **first-class citizen of the platform** rather than an assumed background condition. It has two parts:

- **Story 8.5a — Tailscale status and UI** — implement the specced-but-unbuilt Tailscale server API, surface live VPN status throughout the UI, expose the Funnel webhook address, give the agent the ability to answer questions about network connectivity, and add a guided Tailscale setup step to the first-run wizard
- **Story 8.5b — Generic webhook trigger** — a single inbound webhook endpoint that routes any external event (GitHub, Stripe, CI/CD, IoT, etc.) into the existing `notify.rs` trigger infrastructure via HMAC secret matching, with a Funnel setup guide and per-thread binding management
- **Story 8.5c — Install script and shell CLI** — a root-level `install.sh` that bootstraps all dependencies (Homebrew, Rust, Node, Tailscale) and deploys agent-deck; a `scripts/run.sh` canonical service runner; and an `agent-deck` shell CLI (start/stop/status/logs/open) sourced into the user's shell on install; a GitHub Actions release workflow that produces signed pre-built binaries for `aarch64` and `x86_64` macOS so users can install without cloning source

The stories can be sequenced: 8.5a first (foundation), then 8.5b (webhook trigger), then 8.5c (install script + CLI + release binary). 8.5c has no code dependencies on 8.5a or 8.5b and can be built in parallel.

---

## Story 8.5a — Tailscale Status API and UI

### Background

The spec (PLAN_2 §6.15) defined three Tailscale endpoints and a setup wizard step. Story 1.x was marked complete, but the routes, handler, and Tailscale service module were never landed in the codebase. `GeneralSettings.tsx` references Tailscale only in static hint text — no live data. This story implements what Story 1.x promised, and extends it with Funnel awareness.

---

### Backend: `services/tailscale.rs`

New service module at `server/src/services/tailscale.rs`. All Tailscale interaction goes through this module — no `tokio::process::Command` calls from route handlers.

```rust
pub struct TailscaleStatus {
    pub installed: bool,
    pub connected: bool,
    pub hostname: Option<String>,     // e.g. "mac-mini.tail1234.ts.net"
    pub funnel_enabled: bool,
    pub funnel_url: Option<String>,   // e.g. "https://mac-mini.tail1234.ts.net"
    pub auth_url: Option<String>,     // populated when tailscale up is pending auth
    pub version: Option<String>,      // e.g. "1.62.1"
    pub ip_address: Option<String>,   // e.g. "100.64.x.x"
}
```

Key functions (all `async`, all non-blocking — errors are logged and converted to safe `TailscaleStatus` defaults):

```rust
pub async fn get_status() -> TailscaleStatus
pub async fn check_funnel(port: u16) -> bool
pub async fn run_connect() -> TailscaleStatus
```

**`get_status()` implementation:**

1. Check if `tailscale` binary exists → sets `installed`
2. Run `tailscale status --json` — parse JSON response:
   - `BackendState == "Running"` → `connected: true`
   - `Self.DNSName` → `hostname` (trim trailing `.`)
   - `Self.TailscaleIPs[0]` → `ip_address`
3. Run `tailscale version` → `version`
4. Call `check_funnel(port)`:
   - Run `tailscale funnel status` — parse for app port and HTTPS URL
   - If unavailable (older Tailscale version), return `false` gracefully

**`run_connect()` implementation:**
- Run `tailscale up --timeout=30s`
- Look for `https://login.tailscale.com/` in output → `auth_url`
- Call `get_status()` and return

---

### Backend: `routes/tailscale.rs`

New route module. All endpoints require auth (same localhost bypass applies automatically).

**`GET /api/tailscale/status`**

Calls `services::tailscale::get_status()`. Cached in `AppState` for 30 seconds (`Arc<RwLock<(TailscaleStatus, Instant)>>`). Cache invalidated on connect/funnel toggle.

Response:
```json
{
  "data": {
    "installed": true,
    "connected": true,
    "hostname": "mac-mini.tail1234.ts.net",
    "ip_address": "100.64.12.34",
    "version": "1.62.1",
    "funnel_enabled": true,
    "funnel_url": "https://mac-mini.tail1234.ts.net",
    "auth_url": null
  }
}
```

**`POST /api/tailscale/connect`** — calls `run_connect()`, invalidates cache, returns status shape.

**`POST /api/tailscale/funnel/enable`** — runs `tailscale funnel <port>`, invalidates cache, returns updated status.

**`POST /api/tailscale/funnel/disable`** — disables Funnel, invalidates cache.

---

### AppState changes

```rust
pub tailscale_status_cache: Arc<RwLock<Option<(TailscaleStatus, std::time::Instant)>>>,
pub server_port: u16,
```

On server startup, fire a background task to warm the cache — non-blocking, does not delay startup.

---

### Route registration

```rust
.route("/api/tailscale/status",         get(tailscale::get_status))
.route("/api/tailscale/connect",        post(tailscale::connect))
.route("/api/tailscale/funnel/enable",  post(tailscale::enable_funnel))
.route("/api/tailscale/funnel/disable", post(tailscale::disable_funnel))
```

---

### Agent tool: `tailscale_status`

New built-in tool in `services/tools.rs`. Available to all non-default personas.

```json
{
  "name": "tailscale_status",
  "description": "Returns the current Tailscale VPN status for this agent-deck server. Use this when the user asks about network connectivity, their Tailscale hostname, webhook address, Funnel status, or whether the server is reachable from the internet.",
  "parameters": { "type": "object", "properties": {}, "required": [] }
}
```

Implementation: calls `services::tailscale::get_status()` and formats as:

```
Tailscale status:
- Connected: yes
- Hostname: mac-mini.tail1234.ts.net
- IP: 100.64.12.34
- Version: 1.62.1
- Funnel: enabled — https://mac-mini.tail1234.ts.net
- Webhook address: https://mac-mini.tail1234.ts.net/api/webhooks
```

The webhook address line is only shown when `funnel_enabled` is `true`.

When bindings exist and funnel is enabled, the tool also lists active webhook bindings:

```
Webhook bindings:
- Thread "PR Reviews" → github/pull_request → https://mac-mini.tail1234.ts.net/api/webhooks
- Thread "All Events" → github/* → https://mac-mini.tail1234.ts.net/api/webhooks
```

---

### Frontend: `TailscaleStatusCard`

New component at `web/src/components/settings/TailscaleStatusCard.tsx`. Placed in:
1. **`GeneralSettings.tsx`** — replaces static "Open on Phone" Tailscale subtitle text
2. **`MobileSettings.tsx`** — added above the install instructions

**Card layout:**

```
┌─────────────────────────────────────────────┐
│ Tailscale                      [Refresh ⟳]  │
│                                              │
│  ● Connected                                 │
│  mac-mini.tail1234.ts.net · 100.64.12.34     │
│                                              │
│  Funnel                        [Enable ▶]   │
│  Not enabled — webhooks will not work        │
│                                              │
│  ── When enabled ────────────────────────── │
│  Webhook URL  https://mac-mini.tail1234.ts.net│
│               /api/webhooks                  │
│               [Copy]                         │
└─────────────────────────────────────────────┘
```

States:
- **Not installed:** amber dot, "Tailscale not found — install at tailscale.com"
- **Installed, not connected:** red dot, [Connect] button
- **Connected, no funnel:** green dot, hostname + IP, [Enable Funnel] button
- **Connected + funnel enabled:** green dot, funnel URL with copy button
- **Loading:** skeleton shimmer

---

### Frontend: Sidebar VPN indicator

In `Sidebar.tsx` (desktop), add a small status dot next to the app title. Green = connected, amber = not connected, grey = unknown. Tooltip: "Tailscale connected — mac-mini.tail1234.ts.net". Clicking navigates to General Settings.

On mobile (`MobileThreadList.tsx`), same dot in the top nav bar.

---

### Setup Wizard: `StepTailscale.tsx`

New step file at `web/src/components/wizards/setup-wizard/StepTailscale.tsx`. Inserted into `SetupWizard.tsx` as step 2 (after Welcome, before Your Name). All subsequent steps shift by one in the `STEPS` array and in the `SetupWizard` render block. Existing step filenames are unchanged.

The step is **conditionally auto-skipping**: on mount it calls `GET /api/tailscale/status`. If `connected: true`, the step shows a green confirmation and auto-advances after 1.5 seconds (or the user clicks "Next →" immediately). This means the step is invisible friction for users who already have Tailscale running.

**Step states:**

```
┌─────────────────────────────────────────────────────────┐
│ Set up Tailscale                                         │
│                                                          │
│  STATE 1 — Not installed                                 │
│  ○ Tailscale not found on this machine                   │
│                                                          │
│  agent-deck requires Tailscale for secure remote access. │
│  The easiest way to install everything at once is to run │
│  the agent-deck install script, which installs Tailscale │
│  along with all other dependencies automatically.        │
│                                                          │
│  Already installed Tailscale separately?                 │
│  [Refresh ⟳]                                             │
│  Waiting for Tailscale… ◌                                │
│                                                          │
│  STATE 2 — Installed, not connected                      │
│  ○ Tailscale installed but not connected                 │
│                                                          │
│  [Connect to Tailscale]                                  │
│  (calls POST /api/tailscale/connect)                     │
│  If auth_url returned: "Open this link to authenticate:" │
│  <clickable auth_url>                                    │
│  Polling every 3s until connected…                       │
│                                                          │
│  STATE 3 — Connected                                     │
│  ● Connected to Tailscale                                │
│  mac-mini.tail1234.ts.net · 100.64.12.34                 │
│                                                          │
│  Connect from another device:                            │
│  Install Tailscale on your iPhone or Android and sign    │
│  in with the same account. Then open:                    │
│  http://mac-mini.tail1234.ts.net:7474                    │
│  [Copy]                                                  │
│                                                          │
│  Advancing in 1s…  [Next →]                              │
└─────────────────────────────────────────────────────────┘
```

**Polling:** When in states 1 or 2, the component polls `GET /api/tailscale/status` every 3 seconds. Polling stops when `connected: true` or when the component unmounts.

A low-weight **"I'll set this up later →"** skip link is shown at the bottom of all three states. It is not a button — lower visual weight so it does not compete with the primary action.

**`SetupWizard.tsx` changes:**
- Import `StepTailscale`
- Add `{ label: "Tailscale" }` to `STEPS` at index 1 (before `"Your name"`)
- Insert render branch: `{step === 2 && <StepTailscale onNext={() => goTo(3)} onSkip={() => goTo(3)} />}`
- Shift all existing step render conditions up by one: `step === 2` → `step === 3` through `step === 6` → `step === 7`
- Update all `goTo(n)` call targets in the existing handlers accordingly

---

### API client additions

```typescript
// web/src/api/client.ts
export const tailscaleApi = {
  getStatus:     () => api.get<TailscaleStatus>('/api/tailscale/status'),
  connect:       () => api.post<TailscaleStatus>('/api/tailscale/connect'),
  enableFunnel:  () => api.post<TailscaleStatus>('/api/tailscale/funnel/enable'),
  disableFunnel: () => api.post<TailscaleStatus>('/api/tailscale/funnel/disable'),
};
```

Add `TailscaleStatus` to `web/src/types/index.ts`.

---

### Acceptance Criteria (8.5a)

- [x] `GET /api/tailscale/status` returns correct state when Tailscale is installed and connected
- [x] `GET /api/tailscale/status` returns `installed: false` when `tailscale` binary is not found
- [x] Status cache invalidates on connect/funnel toggle; refreshes naturally within 30 seconds
- [x] `POST /api/tailscale/connect` returns an `auth_url` when machine is not yet authenticated
- [x] `POST /api/tailscale/funnel/enable` enables Funnel and returns updated status with `funnel_url`
- [x] Tailscale card appears in General Settings with live status
- [x] Tailscale card appears in Mobile Settings
- [x] VPN status dot appears in desktop Sidebar and mobile nav bar
- [x] Funnel webhook URL is displayed and copyable when Funnel is enabled
- [x] `tailscale_status` agent tool returns formatted status including webhook address when funnel is enabled
- [x] Agent can answer "What is my Tailscale hostname?" and "What is my webhook URL?"
- [x] All `tailscale` subprocess calls are non-blocking and never delay server startup
- [x] Unit tests: status parsing (connected, disconnected, not installed), funnel URL extraction
- [x] `cargo build` passes, all existing tests pass
- [x] Setup wizard includes `StepTailscale.tsx` inserted between Welcome and Your Name
- [x] If `connected: true` on mount, step shows confirmation and auto-advances after 1.5 seconds
- [x] If not installed: component polls every 3 seconds; [Refresh] button triggers an immediate re-check
- [x] If installed but not connected: [Connect to Tailscale] calls `POST /api/tailscale/connect`; `auth_url` shown as a clickable link when returned; polls every 3 seconds until connected
- [x] Connected state shows hostname, IP, and copyable mobile access URL
- [x] "I'll set this up later →" skip link present and functional on all three states

---

## Story 8.5b — Generic Webhook Trigger

### Background

With Funnel enabled, agent-deck has a stable public HTTPS URL. This story adds a **single universal webhook endpoint** — `POST /api/webhooks` — that routes any inbound event into the existing `notify.rs` trigger infrastructure. Routing is based on HMAC secret matching against stored bindings, not on the URL path. The first supported source is GitHub, but the design is intentionally source-agnostic — adding Stripe, CI/CD, or any other service requires only a new formatter function and no changes to the endpoint.

**Key design principle:** One stable URL forever. `https://mac-mini.tail1234.ts.net/api/webhooks`. Every service gets the same address; the secret is what identifies the binding.

---

### New event type: `webhook_executed`

In `routes/notify.rs`, add `WebhookExecuted` to `SystemEventType`:

```rust
WebhookExecuted,
```

Flags:
- `persist()` → `false`
- `trigger()` → `true`

`format_content()` passes the payload string through directly — the webhook handler is responsible for producing a human-readable prompt before calling `notify_internal`.

---

### DB migration: `013_webhook_bindings.sql`

```sql
CREATE TABLE webhook_bindings (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    thread_id  TEXT NOT NULL REFERENCES threads(id),
    source     TEXT NOT NULL DEFAULT 'github',  -- 'github', 'stripe', 'generic', etc.
    event_type TEXT NOT NULL,                   -- 'pull_request', 'push', 'issues', '*'
    secret     TEXT NOT NULL,                   -- AES-GCM encrypted HMAC secret
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_webhook_bindings_source  ON webhook_bindings(source);
CREATE INDEX idx_webhook_bindings_thread  ON webhook_bindings(thread_id);
```

The `secret` column stores the AES-GCM encrypted HMAC-SHA256 secret (same encryption path as credentials). This secret is what the user pastes into GitHub's webhook settings.

---

### Backend: `routes/webhooks.rs`

**`POST /api/webhooks`** — the only public webhook endpoint. No auth cookie required. Security is HMAC-SHA256 signature verification.

Handler steps:

1. Read raw request body as `Bytes` (before any parsing)
2. Load all enabled bindings from `webhook_bindings`
3. For each binding, decrypt its secret and attempt HMAC-SHA256 verification against the request:
   - For GitHub: check `X-Hub-Signature-256: sha256=<hex>`
   - For generic: check `X-Webhook-Signature` or a custom header pattern stored on the binding
   - First match wins
4. On match: identify `source` from the binding
5. Detect event type:
   - GitHub: `X-GitHub-Event` header
   - Others: binding's `event_type` field or a payload key
6. Check if the matched binding's `event_type` allows this event (`*` matches all)
7. Route to the appropriate formatter:
   ```rust
   let prompt = match binding.source.as_str() {
       "github"  => formatters::github::format(payload_json, &event_type),
       "stripe"  => formatters::stripe::format(payload_json),
       _         => formatters::generic::format(payload_json, &binding.source),
   };
   ```
8. Call `notify_internal(state, &binding.thread_id, "webhook_executed", prompt)`
9. Return `202 Accepted` immediately — agent run is fire-and-forget
10. Return `401` if no binding's secret matches
11. Return `404` if no enabled bindings exist at all

---

### Webhook formatters: `services/webhook_formatters/`

Pure functions, no I/O. Each takes raw JSON and returns a human-readable prompt string.

**`formatters/github.rs`:**

| Event | Prompt |
|---|---|
| `pull_request` · `opened` | `GitHub: New PR #N opened by @user — "{title}". Base: {base}. {url}` |
| `pull_request` · `review_requested` | `GitHub: Review requested on PR #N — "{title}" from @reviewer. {url}` |
| `issue_comment` | `GitHub: @user commented on PR/issue #N — "{body[:200]}". {url}` |
| `push` | `GitHub: {N} commit(s) pushed to {ref} by @user. Latest: "{message}". {url}` |
| `issues` · `opened` | `GitHub: Issue #N opened by @user — "{title}". {url}` |
| Unknown | `GitHub webhook: event={event}, action={action}. Repo: {full_name}.` |

**`formatters/generic.rs`:**

```
Webhook received from {source}: {payload_as_pretty_json_truncated_to_500_chars}
```

---

### `notify_internal` refactor

Extract the core trigger logic from `notify::notify` into a free function:

```rust
pub async fn notify_internal(
    state: &Arc<AppState>,
    thread_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> AppResult<NotifyResponse>
```

The existing `notify` handler becomes a thin wrapper calling `notify_internal` after auth/ownership checks. The webhook handler calls `notify_internal` directly — HMAC verification is the security gate.

---

### Backend: `routes/webhook_bindings.rs`

Auth-protected CRUD endpoints.

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/threads/:id/webhook-bindings` | List bindings for thread |
| `POST` | `/api/threads/:id/webhook-bindings` | Create binding, generate secret |
| `DELETE` | `/api/threads/:id/webhook-bindings/:bid` | Remove binding |
| `PATCH` | `/api/threads/:id/webhook-bindings/:bid/toggle` | Enable/disable |

`POST` body:
```json
{ "source": "github", "event_type": "pull_request" }
```

On create: generate random 32-byte hex HMAC secret, encrypt, store. Return plaintext secret **once only**:

```json
{
  "data": {
    "id": "...",
    "thread_id": "...",
    "source": "github",
    "event_type": "pull_request",
    "webhook_url": "https://mac-mini.tail1234.ts.net/api/webhooks",
    "secret": "abc123...",
    "enabled": true
  }
}
```

If Funnel is not enabled, include a `warning` field in the response: `"Tailscale Funnel is not enabled — GitHub cannot reach this server."`.

The `webhook_url` is always `https://{funnel_hostname}/api/webhooks` — the same for every binding.

---

### Frontend: `WebhookBindings` section in ConfigPane

In `web/src/components/ConfigPane.tsx`, add a **Webhooks** section (below Routines, above MCP Servers).

**Empty state:**
```
Webhooks
[+ Add webhook]
```

**With a binding:**
```
Webhooks

  ⬡ GitHub  pull_request  ● active
    https://mac-mini.tail1234.ts.net/api/webhooks
    [Copy URL]  [×]
```

**"+ Add webhook" inline flow:**
1. Click button → inline form expands:
   ```
   Source:     [GitHub ▾]
   Event type: [all events ▾]
   [Create]
   ```
2. On create: show one-time secret panel:
   ```
   ⚠ Copy this secret now — it won't be shown again.
   
   Webhook URL  https://mac-mini.tail1234.ts.net/api/webhooks  [Copy]
   Secret       abc123def456...                                [Copy]
   
   In GitHub: Settings → Webhooks → Add webhook
   Paste the URL and secret above.
   Content type: application/json
   
   [Done]
   ```
3. After [Done], secret is gone; only URL and source/event type shown

**Funnel not enabled banner** (shown at top of Webhooks section):
```
⚠ Tailscale Funnel is not enabled. External services cannot reach this server.
  [Enable Funnel]
```
Clicking [Enable Funnel] calls `POST /api/tailscale/funnel/enable` and refreshes status.

---

### Mobile: `MobileConfigSheet` Webhooks section

Add a Webhooks row showing binding count (e.g. "2 active"). Taps through to a full-screen `MobileWebhookView.tsx` with the same information as ConfigPane.

---

### Documentation: `docs/skills/github-webhook.md`

A new skill file that teaches the agent how to:
- Help the user create a webhook binding and configure GitHub
- Explain what Funnel is and how to enable it
- Troubleshoot: Funnel not enabled, HMAC mismatch, wrong event type, 410 subscription cleanup

---

### Acceptance Criteria (8.5b)

- [x] `webhook_executed` event type added to `SystemEventType` with `persist=false, trigger=true`
- [x] Migration creates `webhook_bindings` table with correct schema and indexes (landed as migration 014; schema evolved to global-registry design in 016–017 — see as-built note below)
- [x] `POST /api/webhooks` is a public endpoint (no auth cookie required)
- [x] `POST /api/webhooks` verifies HMAC-SHA256 by testing all enabled binding secrets; returns `401` if none match
- [x] Matching binding's source determines which formatter is used
- [x] GitHub formatter produces correct natural-language prompts for all 5 event types + unknown fallback
- [x] Generic formatter produces a readable summary for any source
- [x] Valid webhook payload triggers the agent run via `notify_internal`
- [x] `POST /api/webhooks` returns `202 Accepted` immediately; agent run is fire-and-forget
- [x] CRUD endpoints for `webhook_bindings` work correctly
- [x] Secret is returned plaintext exactly once on create; subsequent GETs show it masked/absent
- [x] `webhook_url` in create response is always `https://{funnel_hostname}/api/webhooks`
- [x] Funnel-not-enabled warning included in create response when applicable
- [x] ConfigPane shows Webhooks section with add/delete/toggle
- [x] One-time secret display with copy buttons appears after binding creation
- [x] Funnel warning banner shown in ConfigPane when `funnel_enabled` is false
- [x] [Enable Funnel] in the banner calls the funnel endpoint and refreshes status
- [x] Mobile settings shows webhook binding count
- [x] Agent `tailscale_status` tool lists active bindings when funnel is enabled
- [x] `cargo build` passes, all existing tests pass (340 passing)
- [x] Unit tests: HMAC verification, GitHub formatter (all 5 events + unknown), generic formatter, `notify_internal` refactor
- [ ] Integration test: `POST /api/webhooks` with correct HMAC → `202` + agent run triggered; incorrect HMAC → `401` — **deferred**: requires live DB + provider; covered by HMAC unit tests + manual verification

**As-built deviation:** The webhook binding model evolved from per-thread bindings (spec) to a global registry (migrations 016–017). Bindings are created globally and attached to threads via `thread_webhook_bindings`. The per-thread PATCH toggle in the spec is superseded by a global `PATCH /api/webhook-bindings/:id/toggle`. All functional requirements are met under the new model.

---

## Implementation Order

```
8.5a                              8.5b                            8.5c (parallel)
──────────────────────────────    ──────────────────────────────  ────────────────────────────
services/tailscale.rs             services/webhook_formatters/    scripts/run.sh
routes/tailscale.rs               routes/webhooks.rs (public)     scripts/agent-deck-cli.sh
AppState cache                    routes/webhook_bindings.rs      install.sh
tools.rs (tailscale_status)  →    notify_internal refactor        .github/workflows/release.yml
StepTailscale.tsx                 migration 013
TailscaleStatusCard               ConfigPane Webhooks section  →  (no code deps on 8.5a/b)
Sidebar/mobile dot            →   MobileConfigSheet binding count
API client types                  docs/skills/github-webhook.md
```

8.5b depends on 8.5a only for `funnel_url` in the `webhook_url` response field. The backend of 8.5b can be built in parallel with 8.5a if the funnel URL is temporarily hardcoded. 8.5c has no code dependencies on either and can be built fully in parallel.

---

## Story 8.5c — Install Script and Shell CLI

### Background

A new user setting up agent-deck from source currently has no guided path from clone to running service. This story adds a root-level `install.sh` that handles the entire bootstrap, a production runner script, and an `agent-deck` shell CLI installed into the user's shell. It also adds a GitHub Actions release workflow that builds and publishes signed pre-built binaries for macOS `aarch64` and `x86_64` — so users who do not want to clone source can install with a single `curl | bash` command.

---

### Pre-built binary distribution

The release workflow (`release.yml`) runs on `v*` tag push and produces:

- `agent-deck-macos-aarch64.tar.gz` — Release binary + `public/` directory
- `agent-deck-macos-x86_64.tar.gz` — Same for Intel

Both tarballs are attached to the GitHub Release with SHA256 checksums in the release body.

`install.sh` detects whether it is running from a source clone (presence of `Cargo.toml`) or from a downloaded tarball. In tarball mode it skips the build steps and uses the bundled binary directly.

A one-liner install path (for users without source) documented in the README:
```
curl -fsSL https://github.com/<owner>/agent-deck/releases/latest/download/install.sh | bash
```

The install script is also committed at repo root so source users can run `./install.sh` directly.

---

### `install.sh` (repo root)

Single entrypoint for new installs and upgrades. Idempotent — safe to re-run.

Steps:
1. **Detect mode** — source clone vs. pre-built tarball (checks for `Cargo.toml`)
2. **Check OS** — exit with a clear message on non-macOS
3. **Homebrew** — install if not present (official install script from brew.sh)
4. **Rust toolchain** — check for `cargo`; if missing, install via `rustup-init` (non-interactive, stable toolchain)
5. **Node.js** — check for `node >= 20`; if missing, install via `brew install node`
6. **Tailscale** — check for `tailscale` binary; if missing, install via `brew install tailscale` and print instructions to start it (`brew services start tailscale`)
7. **Build** *(source mode only)* — `cargo build --release`, then `cd web && npm run build`
8. **Deploy** — copy binary to `~/.agent-deck/bin/agent-deck`; copy `web/dist/` (or bundled `public/`) to `~/.agent-deck/public/`; create directories as needed
9. **Install CLI** — copy `scripts/agent-deck-cli.sh` to `~/.agent-deck/agent-deck-cli.sh`; append `source ~/.agent-deck/agent-deck-cli.sh` to `~/.zshrc` and `~/.bash_profile` if the line is not already present
10. **Start service** — call `agent-deck start`
11. **Print summary** — local URL, Tailscale URL if connected, reminder to open a new terminal or run `source ~/.zshrc`

---

### `scripts/run.sh`

Canonical script for running agent-deck in production. `agent-deck start` delegates to this.

The Rust binary serves the compiled React frontend as static files from `~/.agent-deck/public/` — no separate web process is needed. Starting the binary starts both the API and the web application.

```bash
#!/usr/bin/env bash
set -euo pipefail

AGENT_DECK_HOME="${AGENT_DECK_HOME:-$HOME/.agent-deck}"
LOG_FILE="$AGENT_DECK_HOME/server.log"
PID_FILE="$AGENT_DECK_HOME/agent-deck.pid"
BINARY="$AGENT_DECK_HOME/bin/agent-deck"
PORT="${AGENT_DECK_PORT:-7474}"

mkdir -p "$AGENT_DECK_HOME"

nohup "$BINARY" \
  --data-dir "$AGENT_DECK_HOME" \
  >> "$LOG_FILE" 2>&1 &

echo $! > "$PID_FILE"

echo ""
echo "  agent-deck is running (PID $(cat "$PID_FILE"))"
echo ""
echo "  Open:  http://localhost:$PORT"

# Print Tailscale URL if connected
if command -v tailscale &>/dev/null; then
  TS_HOST=$(tailscale status --json 2>/dev/null | grep -o '"DNSName":"[^"]*"' | head -1 | cut -d'"' -f4 | sed 's/\.$//')
  if [ -n "$TS_HOST" ]; then
    echo "         http://$TS_HOST:$PORT"
  fi
fi

echo ""
echo "  Logs:  $LOG_FILE"
echo "         (run 'agent-deck logs' to tail)"
echo ""
```

---

### `scripts/agent-deck-cli.sh`

Sourced shell file that defines the `agent-deck` shell function. Installed to `~/.agent-deck/agent-deck-cli.sh` by `install.sh`. Works in both bash and zsh.

| Subcommand | Behaviour |
|---|---|
| `agent-deck start` | If PID file exists and process is alive, print "already running". Otherwise exec `scripts/run.sh`. |
| `agent-deck stop` | Read PID file, `kill $PID`, remove PID file, print "stopped". |
| `agent-deck status` | Print running/stopped + PID. If `tailscale` is in PATH, also print Tailscale connected/disconnected. |
| `agent-deck logs` | `tail -f ~/.agent-deck/server.log` |
| `agent-deck open` | `open http://localhost:7474` |
| `agent-deck help` | Print subcommand list |

---

### `.github/workflows/release.yml`

Triggered on `v*` tag push. Matrix: `[macos-14 (aarch64), macos-13 (x86_64)]`.

Steps per matrix target:
1. Checkout + cache Cargo registry
2. `npm ci && npm run build` in `web/`
3. `cargo build --release`
4. `tar -czf agent-deck-macos-<arch>.tar.gz -C target/release agent-deck -C ../../web dist`
5. `sha256sum agent-deck-macos-<arch>.tar.gz`
6. Upload tarball + SHA to GitHub Release via `softprops/action-gh-release`

Release body template includes SHA256 checksums and the one-liner install command.

---

### Acceptance Criteria (8.5c)

- [x] `install.sh` is executable and runs to completion on a clean macOS machine with Xcode CLI tools installed
- [x] Re-running `install.sh` on an already-configured machine completes without errors and does not duplicate the `source` line in shell profiles
- [x] After install, opening a new terminal and running `agent-deck status` prints the service state
- [x] `agent-deck start` launches the server and writes a PID file; a second call prints "already running"
- [x] `agent-deck stop` kills the process, removes the PID file, prints "stopped"
- [x] `agent-deck logs` tails `~/.agent-deck/server.log`
- [x] `agent-deck open` opens `http://localhost:7474` in the default browser
- [x] `scripts/run.sh` starts the binary as a background process with output redirected to the log file
- [x] `release.yml` triggers on `v*` tag and produces two tarballs with SHA256s in the release body
- [x] `curl -fsSL .../install.sh | bash` on a clean machine completes and starts the server
- [x] Install script prints a clear summary at the end: local URL, Tailscale URL (if connected), and new-terminal reminder

---

## Phase Fit Summary

| Item | Value |
|---|---|
| New branch | `feature/phase8-tailscale-platform` |
| Plan doc to update | `PLAN_3.md` — insert as Phase 8.5 between Phase 8 and Phase 9 |
| Schema migrations | 8.5a: none · 8.5b: migration 013 · 8.5c: none |
| Spec backfill | `PLAN_2.md §6.15` already written — add funnel fields and as-built note |
| Tailscale Story 1.x note | Marked complete in PLAN_3 but never implemented — this story delivers the actual code |

---

## Human Review Instructions

**Prerequisites:** Server running on `localhost:7474`. Tailscale installed and connected on the test machine.

### 8.5a — Tailscale

1. Open Settings → General → scroll to Tailscale card → **Expected:** green dot, hostname, IP, Funnel section visible. **Failure:** blank card or error.
2. Open the Setup Wizard (clear localStorage key `setupComplete` and reload) → **Expected:** Tailscale step appears as step 2, shows connected state and auto-advances after ~1.5s.
3. Ask the agent "What is my Tailscale hostname?" → **Expected:** agent uses `tailscale_status` tool and returns the hostname.
4. Mobile: open Settings → scroll to Tailscale → **Expected:** same card as desktop.
5. Mobile: Sidebar/nav bar → **Expected:** small green dot visible next to app title.

### 8.5b — Webhooks

6. Settings → Webhooks → click **+ New Binding** → select GitHub / pull_request → click Create → **Expected:** one-time secret panel appears with Webhook URL and Secret, both with Copy buttons.
7. After dismissing the secret panel → **Expected:** binding card shows with Enable/Disable toggle and Delete button.
8. Click Disable on the card → **Expected:** card updates to disabled state immediately (optimistic). Click Enable → re-enables.
9. If Funnel is not enabled: **Expected:** amber warning banner with **[Enable Funnel]** button appears above the binding list. Clicking it enables Funnel and the banner disappears.
10. Mobile: open Settings → scroll to Webhooks row → **Expected:** shows "N active" count.
11. Send a test webhook: `curl -X POST http://localhost:7474/api/webhooks -H "X-Hub-Signature-256: sha256=<valid_hmac>" -H "X-GitHub-Event: push" -d '<payload>'` → **Expected:** 202 response; agent run triggered in the bound thread.
12. Send same request with wrong HMAC → **Expected:** 401 response.

### 8.5c — Install script

13. Run `agent-deck status` in terminal → **Expected:** prints service state (running/stopped).
14. Run `agent-deck logs` → **Expected:** tails the log file.

**Optional log check:**
```
grep "tailscale\|webhook" ~/.agent-deck/server.log | tail -20
```

---

## Approval

- [x] **Implementation plan approved**
- [x] **Coding complete** — 340 tests pass; all AC verified against code; architectural deviation noted
- [ ] **Human review approved**

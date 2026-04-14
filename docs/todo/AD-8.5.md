# AD-8.5 — Tailscale Platform Layer + Webhook Integration

**Phase:** 8.5 (Platform Infrastructure — between Phase 8 MCP Depth and Phase 9 Polish)  
**Branch:** `feature/phase8-tailscale-platform`  
**Depends on:** Phase 7 complete (7.3 Web Push client, 7.3a Processing Block)

---

## Overview

agent-deck runs on a headless local computer and is accessed exclusively over Tailscale. This story makes Tailscale a **first-class citizen of the platform** rather than an assumed background condition. It has two parts:

- **Story 8.5a — Tailscale status and UI** — implement the specced-but-unbuilt Tailscale server API, surface live VPN status throughout the UI, expose the Funnel webhook address, and give the agent the ability to answer questions about network connectivity
- **Story 8.5b — Generic webhook trigger** — a single inbound webhook endpoint that routes any external event (GitHub, Stripe, CI/CD, IoT, etc.) into the existing `notify.rs` trigger infrastructure via HMAC secret matching, with a Funnel setup guide and per-thread binding management

The two stories can be sequenced: 8.5a first (foundation), then 8.5b (generic webhook + GitHub as the first use case). Neither blocks any other active story.

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

- [ ] `GET /api/tailscale/status` returns correct state when Tailscale is installed and connected
- [ ] `GET /api/tailscale/status` returns `installed: false` when `tailscale` binary is not found
- [ ] Status cache invalidates on connect/funnel toggle; refreshes naturally within 30 seconds
- [ ] `POST /api/tailscale/connect` returns an `auth_url` when machine is not yet authenticated
- [ ] `POST /api/tailscale/funnel/enable` enables Funnel and returns updated status with `funnel_url`
- [ ] Tailscale card appears in General Settings with live status
- [ ] Tailscale card appears in Mobile Settings
- [ ] VPN status dot appears in desktop Sidebar and mobile nav bar
- [ ] Funnel webhook URL is displayed and copyable when Funnel is enabled
- [ ] `tailscale_status` agent tool returns formatted status including webhook address when funnel is enabled
- [ ] Agent can answer "What is my Tailscale hostname?" and "What is my webhook URL?"
- [ ] All `tailscale` subprocess calls are non-blocking and never delay server startup
- [ ] Unit tests: status parsing (connected, disconnected, not installed), funnel URL extraction
- [ ] `cargo build` passes, all existing tests pass

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

- [ ] `webhook_executed` event type added to `SystemEventType` with `persist=false, trigger=true`
- [ ] Migration 013 creates `webhook_bindings` table with correct schema and indexes
- [ ] `POST /api/webhooks` is a public endpoint (no auth cookie required)
- [ ] `POST /api/webhooks` verifies HMAC-SHA256 by testing all enabled binding secrets; returns `401` if none match
- [ ] Matching binding's source determines which formatter is used
- [ ] GitHub formatter produces correct natural-language prompts for all 5 event types + unknown fallback
- [ ] Generic formatter produces a readable summary for any source
- [ ] Valid webhook payload triggers the agent run via `notify_internal`
- [ ] `POST /api/webhooks` returns `202 Accepted` immediately; agent run is fire-and-forget
- [ ] CRUD endpoints for `webhook_bindings` work correctly
- [ ] Secret is returned plaintext exactly once on create; subsequent GETs show it masked/absent
- [ ] `webhook_url` in create response is always `https://{funnel_hostname}/api/webhooks`
- [ ] Funnel-not-enabled warning included in create response when applicable
- [ ] ConfigPane shows Webhooks section with add/delete/toggle
- [ ] One-time secret display with copy buttons appears after binding creation
- [ ] Funnel warning banner shown in ConfigPane when `funnel_enabled` is false
- [ ] [Enable Funnel] in the banner calls the funnel endpoint and refreshes status
- [ ] Mobile config sheet shows webhook binding count
- [ ] Agent `tailscale_status` tool lists active bindings when funnel is enabled
- [ ] `cargo build` passes, all existing tests pass
- [ ] Unit tests: HMAC verification, GitHub formatter (all 5 events + unknown), generic formatter, `notify_internal` refactor
- [ ] Integration test: `POST /api/webhooks` with correct HMAC → `202` + agent run triggered; incorrect HMAC → `401`

---

## Implementation Order

```
8.5a                              8.5b
──────────────────────────────    ──────────────────────────────────
services/tailscale.rs             services/webhook_formatters/
routes/tailscale.rs               routes/webhooks.rs (public)
AppState cache                    routes/webhook_bindings.rs (authed)
tools.rs (tailscale_status)  →    notify_internal refactor
TailscaleStatusCard               migration 013
Sidebar/mobile dot            →   ConfigPane Webhooks section
API client types                  MobileConfigSheet binding count
                                  docs/skills/github-webhook.md
```

8.5b depends on 8.5a only for `funnel_url` in the `webhook_url` response field. The backend of 8.5b can be built in parallel with 8.5a if the funnel URL is temporarily hardcoded.

---

## Phase Fit Summary

| Item | Value |
|---|---|
| Current story in flight | `feature/phase7-tool-call-grouping` (7.3a) — no conflicts |
| New branch | `feature/phase8-tailscale-platform` |
| Plan doc to update | `PLAN_3.md` — insert as Phase 8.5 between Phase 8 and Phase 9 |
| Schema migrations | 8.5a: no migration needed · 8.5b: migration 013 |
| Spec backfill | `PLAN_2.md §6.15` already written — add funnel fields and as-built note |
| Tailscale Story 1.x note | Marked complete in PLAN_3 but never implemented — this story delivers the actual code |

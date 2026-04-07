# AD-9.5 — Setup Wizard Polish and Documentation

**Story:** 9.5 — Setup wizard polish and documentation
**Branch:** `feature/phase9-wizard-polish`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9

---

## Summary

Fix the critical gap in the setup wizard's Done step — after completing setup, users have no way to connect from a second device (phone, tablet, another browser) because the auth token and server URL are never shown. This story surfaces that connection info on the Done step: local URL, Tailscale URL (when available), a copyable and masked auth token, and a QR code encoding the token-prefilled URL for one-tap mobile login. It also writes the project README.

---

## Current State

- `web/src/components/wizards/setup-wizard/SetupWizard.tsx` — 6-step wizard (Welcome → Name → About You → Provider → Persona → Done). The Done step (`Step5Done.tsx`) shows an animated checkmark and a summary of name/provider/persona, then a button to open agent-deck. It does not show the auth token or any way to connect from another device.
- `server/src/routes/setup.rs` — exposes `GET /api/setup/status` (public) and `POST /api/setup/complete`. The server auto-generates an auth token on startup (stored in the `auth_tokens` table via `services/auth.rs`). This token is required to log in from any new browser session.
- There is no endpoint that returns the token or the server's local URL for display in the wizard.
- Phase 8.5 (not yet built) will add `GET /api/tailscale/status` returning `{ connected, hostname, funnel_url, ... }`. The Done step should incorporate Tailscale URL when that endpoint is available.
- `README.md` — does not yet exist or is a placeholder.

---

## Implementation Plan

### Task 1 — Return auth token in `POST /api/setup/complete` response

**File:** `server/src/routes/setup.rs`

`POST /api/setup/complete` currently sets the auth cookie and returns a success response. Extend the response body to include the auth token so the wizard can display it exactly once without a second round-trip.

The token returned here is the same one already generated at server startup and stored in `auth_tokens`. It is already valid — the `POST /api/setup/complete` handler simply needs to read it from the DB and include it in the JSON response body alongside the existing fields.

No new DB schema is required. The token is a string value already accessible through `services/auth.rs`.

Response body shape after this change:

```json
{
  "ok": true,
  "token": "<auth-token>"
}
```

The token is only included in this one response. It must not appear in any other public endpoint's response.

### Task 2 — Add `GET /api/setup/connect-info` endpoint

**File:** `server/src/routes/setup.rs`

Add a new public (no auth cookie required) endpoint `GET /api/setup/connect-info`. Its purpose: give the Done step everything it needs to render the "Connect from another device" panel.

**Response shape:**

```json
{
  "local_url": "http://192.168.1.42:7474",
  "tailscale_url": "https://my-machine.tailnet-name.ts.net",
  "token_shown": false
}
```

- `local_url` — `http://<non-loopback-ip>:<port>`. Detect the machine's primary non-loopback IPv4 address by enumerating network interfaces (use the `local-ip-address` crate or equivalent). Fall back to `http://localhost:7474` if no non-loopback interface is found.
- `tailscale_url` — if Phase 8.5's `GET /api/tailscale/status` is available and returns a non-null `funnel_url`, include it here; otherwise `null`.
- `token_shown` — a boolean flag persisted in the DB (a single-row config entry, e.g. `settings` table key `connect_info_token_shown`). Starts `false`. Set to `true` the first time the Done step fetches this endpoint and actually renders the token. Once `true`, the endpoint omits the token from its response — but the token itself remains valid and is not revoked.

The token is included in the response only when `token_shown` is `false`:

```json
{
  "local_url": "http://192.168.1.42:7474",
  "tailscale_url": null,
  "token_shown": false,
  "token": "<auth-token>"
}
```

After the client has consumed it, a `POST /api/setup/connect-info/mark-shown` (body: empty, no auth required) sets `token_shown = true` in the DB. Subsequent calls to `GET /api/setup/connect-info` omit the `token` field entirely.

### Task 3 — Done step UI: "Connect from another device" panel

**File:** `web/src/components/wizards/setup-wizard/Step5Done.tsx`

After `POST /api/setup/complete` succeeds, the response now includes the token (Task 1). The Done step also fires `GET /api/setup/connect-info` to retrieve `local_url` and `tailscale_url` (Task 2).

Add a new panel below the existing summary rows, titled **"Connect from another device"**. It contains:

1. **Local URL row** — shows `local_url` with a Copy button. If `tailscale_url` is also present, show it as a second row labelled "Tailscale URL".

2. **Token row** — shows the token masked (`••••••••••••`) with a reveal toggle (eye icon) and a Copy button. Prefixed with a label: "Access token". The token value comes from the `POST /api/setup/complete` response body (Task 1) and is kept in component state only — it is never written to `localStorage` or `sessionStorage`.

3. **QR code** — rendered inline as an SVG using the `qrcode` npm package. Encodes `<local_url>?token=<token>` (using `local_url` by default; if `tailscale_url` is present, offer a toggle so the user can switch between local and Tailscale QR codes). When scanned on a mobile device, the browser opens agent-deck already authenticated.

4. **Warning notice** — a visually prominent block (amber/warning colour) reading: "Save this token — it won't be shown again after you leave this page."

After rendering the panel, fire `POST /api/setup/connect-info/mark-shown` so the server records that the token has been displayed.

The existing animated checkmark, summary rows, and "Open agent-deck →" button remain unchanged above the new panel.

**Dependency:** add `qrcode` (or `qrcode.react`) to `web/package.json`.

### Task 4 — Tailscale URL awareness (Phase 8.5 integration)

**File:** `server/src/routes/setup.rs` (the `GET /api/setup/connect-info` handler added in Task 2)

When Phase 8.5 lands and `GET /api/tailscale/status` is available:

- The connect-info handler calls the internal Tailscale status service (not the HTTP endpoint — call the same underlying service function directly to avoid an internal HTTP round-trip).
- If `connected == true` and `funnel_url` is non-null, populate `tailscale_url` in the response.
- If Tailscale is not connected or the service is unavailable, `tailscale_url` is `null` — this is a normal case and not an error.

Until Phase 8.5 is merged, the handler always returns `tailscale_url: null`. The UI in Task 3 already handles the null case gracefully.

### Task 5 — README

**File:** `README.md`

Write a comprehensive README. Sections:

**What is agent-deck?**
One paragraph. A self-hosted AI assistant server for personal use — runs on a Mac, accessible from any device over your local network or via Tailscale. Chat interface, multiple AI providers, memory, routines, MCP tool integrations.

**Prerequisites**
Two paths:
- Homebrew (recommended): `brew install` handles everything.
- Build from source: macOS, Rust stable (`rustup`), Node 20, Git with submodule support.

**Installation**

*Homebrew (recommended):*
```
brew tap <org>/agent-deck
brew install agent-deck
brew services start agent-deck
```
Open `http://localhost:7474` and complete the setup wizard.

*Build from source:*
```
git clone --recurse-submodules <repo>
cd agent-deck/web && npm ci && npm run build
cp -r dist ../server/public
cd ../server && cargo build --release
./target/release/agent-deck
```

**First-run setup**
Open `http://localhost:7474`. The setup wizard walks through: name, profile, AI provider (Copilot, OpenAI, Anthropic, or custom), persona. On the Done step, a QR code and access token are shown — use them to connect from mobile.

**Connecting from mobile**
Two options:
1. Scan the QR code on the Done step — your phone opens agent-deck already logged in.
2. Manually navigate to `http://<your-mac-ip>:7474` (or the Tailscale URL), enter the access token when prompted.
Note: if using the local IP, your phone must be on the same Wi-Fi network. For remote access use Tailscale.

**Tailscale setup**
Install Tailscale on the Mac and enable Funnel for port 7474. The server detects the Tailscale Funnel URL automatically and shows it on the Done step.

**PWA installation**
- iOS: Safari → Share → Add to Home Screen
- Android: Chrome → ⋮ menu → Add to Home Screen (or "Install app")

**Push notifications**
After installing as a PWA, go to Settings → Notifications and enable push. Notifications are sent when a routine fires or a background agent task completes.

**Adding providers**
Settings → Providers. Supported: GitHub Copilot (OAuth, uses the `copilot-api` submodule), OpenAI (API key), Anthropic (API key), custom OpenAI-compatible endpoint.

**Adding MCP servers**
Settings → MCP Servers. Paste the server URL and any required credentials. Refer to the MCP server's own docs for its URL and auth method.

**Adding credentials**
Settings → Credentials. Credentials are stored encrypted at rest in `~/.agent-deck/`.

**Data location**
All data is stored in `~/.agent-deck/` — SQLite database, credentials, and config. The server does not write outside this directory.

### Schema changes

None. The `token_shown` flag is stored as a key-value entry in an existing `settings`-style table (or a new single-purpose column on an existing config table). No migration is required beyond adding a row on first access.

### Parallelisation note

Tasks 1 and 2 are server-side and can be developed together. Task 3 (Done step UI) depends on Tasks 1 and 2 being complete or mocked. Task 4 is additive and can be merged independently once Phase 8.5 lands. Task 5 (README) has no code dependencies and can be written in parallel with everything else.

---

## Acceptance Criteria

- [ ] `POST /api/setup/complete` response body includes `token` (the auth token string)
- [ ] `GET /api/setup/connect-info` returns `{ local_url, tailscale_url, token_shown }` — `token` field included only when `token_shown` is `false`
- [ ] `POST /api/setup/connect-info/mark-shown` sets `token_shown = true`; subsequent `GET /api/setup/connect-info` calls omit `token`
- [ ] `local_url` reflects the machine's non-loopback IP address, not `127.0.0.1`
- [ ] Done step displays a "Connect from another device" panel with: local URL (+ Tailscale URL if available), masked+copyable token, QR code, and the "Save this token" warning
- [ ] QR code encodes `<local_url>?token=<token>` and renders inline as SVG; a toggle switches to the Tailscale URL when available
- [ ] Scanning the QR code on a mobile browser opens agent-deck already authenticated
- [ ] Token is held in component state only — never written to `localStorage` or `sessionStorage`
- [ ] `mark-shown` is fired after the panel is rendered; token is not re-shown on page reload
- [ ] README covers: what agent-deck is, Homebrew install, build-from-source install, first-run wizard, connecting from mobile (QR + manual), Tailscale setup, PWA installation on iOS and Android, push notifications, adding providers/MCP servers/credentials

---

## Human Review Instructions

To be filled in after implementation.

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**
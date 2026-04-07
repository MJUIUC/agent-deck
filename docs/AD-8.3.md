# AD-8.3 — Mobile Settings Parity

**Story:** 8.3 — Mobile Settings Parity  
**Branch:** `feature/phase8-mobile-settings`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8

---

## Summary

The mobile app currently only lets users chat and toggle routines on/off. Because agent-deck is designed to run on headless computers, the phone is often the only available admin interface. This story expands `MobileSettings.tsx` with four new global-configuration sections — Providers, Credentials, MCP Servers, and Personas — bringing mobile settings to functional parity with the desktop sidebar. All four sections use the existing API layer and types without modification; the work is entirely in the web layer.

---

## Current State

### Mobile files

- `web/src/layouts/mobile/MobileSettings.tsx` — currently contains only "Install as App" instructions and a push-notification toggle. No provider, credential, MCP, or persona management.
- `web/src/layouts/mobile/MobileConfigSheet.tsx` — per-thread config (persona picker, model picker, routine toggles). **Not touched by this story.**
- `web/src/layouts/MobileLayout.tsx` — two-tab layout (Threads + Settings tab renders `MobileSettings`).
- `web/src/layouts/mobile/MobileSettings.module.css` — existing styles for the settings screen.

### Desktop equivalents (reference only)

- `web/src/components/settings/McpServerSettings.tsx` — full MCP CRUD with `ToolInspector`.
- `web/src/components/ConfigPane.tsx` — MCP attach/detach and tool list within a thread config.

### API layer (all endpoints live)

| Domain | Methods available |
|---|---|
| Providers | `providersApi.list()`, `.create()`, `.update()`, `.delete()` |
| Credentials | `credentialsApi.list()`, `.create()`, `.delete()` |
| MCP Servers | `mcpServersApi.list()`, `.create()`, `.update()`, `.delete()` |
| Personas | `personasApi.list()`, `.create()`, `.update()`, `.delete()` |

All are exported from `web/src/api/client.ts`.

### Types (all exist in `web/src/types/index.ts`)

`McpServer`, `McpTool`, `Provider`, `Model`, `Credential`, `AgentPersona`

### SSE events

`lastMcpStatusChange` — already consumed by `ConfigPane.tsx` to drive live status dots. The same pattern is reused here for the MCP Servers section.

---

## Implementation Plan

### Task 5 — CSS skeleton for new sections (do this first)
**File:** `web/src/layouts/mobile/MobileSettings.module.css`

Add all CSS required by Tasks 1–4 before writing any JSX. This lets subsequent tasks reference class names without revisiting the stylesheet.

Styles to add:

**Slide-up drawer**
- `.drawer` — fixed full-screen overlay, `z-index` above nav, `background: var(--bg-primary)`, flex column
- `.drawerHeader` — sticky top bar with title (centered) and a close `×` button on the right
- `.drawerBody` — flex-grow scrollable region with vertical padding
- `.drawerFooter` — sticky bottom bar holding primary action button; uses `padding-bottom: env(safe-area-inset-bottom)` for iOS home indicator

**Form fields**
- `.formGroup` — vertical label + input stack with bottom margin
- `.formLabel` — small uppercase muted label
- `.formInput`, `.formSelect`, `.formTextarea` — full-width inputs matching the visual language of the existing notification toggle row; `border-radius: 8px`, `padding: 10px 12px`, `background: var(--bg-secondary)`
- `.segmentedControl` — horizontal row of equal-width buttons for kind / type selectors; active segment gets `background: var(--accent)` and white text

**Section cards and list rows**
- `.sectionCard` — white/surface card with `border-radius: 12px`, `overflow: hidden`, `margin-bottom: 16px`
- `.listRow` — flex row with `align-items: center`, `padding: 12px 16px`, bottom border except last child
- `.listRowLabel` — flex-grow text block (primary name + optional subtitle in muted smaller text)
- `.addButton` — full-width ghost button at the bottom of each section card

**Status indicators**
- `.statusDot` — 8 px circle; color variants `.connected` (green), `.connecting` (amber), `.error` (red), `.inactive` (muted grey)
- `.badge` — small pill label for server type (local / remote); muted background, small caps text

**Delete / destructive row**
- `.deleteButton` — small icon-only or text button, `color: var(--color-danger)`, aligned to trailing edge of a list row
- `.confirmRow` — replaces a list row on pending-delete; shows "Delete?" text with Confirm / Cancel inline buttons

---

### Task 1 — Providers section
**File:** `web/src/layouts/mobile/MobileSettings.tsx`

**State**
```agent-deck/docs/AD-8.3.md
const [providers, setProviders] = useState<Provider[]>([]);
const [providerDrawerOpen, setProviderDrawerOpen] = useState(false);
// form fields: name, kind, base_url, api_key
```

**On mount** — call `providersApi.list()` and populate `providers`.

**List rendering** — a `.sectionCard` with one `.listRow` per provider:
- Leading: provider name (bold) + `base_url` as subtitle
- Trailing: a toggle (`<input type="checkbox" checked={provider.enabled}`) that calls `providersApi.update(id, { enabled: !provider.enabled })` on change, then refreshes the list
- A `.deleteButton` that sets a `pendingDeleteId` and shows the `.confirmRow` inline; confirmed delete calls `providersApi.delete(id)`

**Add drawer** — a `.drawer` rendered conditionally when `providerDrawerOpen`:
- Header: "Add Provider" + close button
- Body: `.formGroup` fields — Name (text), Kind (segmented control: Copilot / OpenAI / Anthropic / Custom), Base URL (text, shown when kind is Custom or when kind requires a URL), API Key (password)
- Footer: "Save" button calls `providersApi.create({ name, kind, base_url, api_key })`, then refreshes list and closes drawer
- Validation: name is required; show inline error text if blank on submit

**No Copilot device-auth flow** — on mobile, Copilot is treated identically to any other provider (URL + key fields only).

---

### Task 2 — Credentials section
**File:** `web/src/layouts/mobile/MobileSettings.tsx`

**State**
```agent-deck/docs/AD-8.3.md
const [credentials, setCredentials] = useState<Credential[]>([]);
const [credentialDrawerOpen, setCredentialDrawerOpen] = useState(false);
// form fields: key, display_name, service, credential_type, secret
```

**On mount** — call `credentialsApi.list()`.

**List rendering** — a `.sectionCard` with one `.listRow` per credential:
- Leading: `display_name` (bold) + `key` as subtitle + optional `service` badge
- Trailing: `.deleteButton` that triggers the inline `.confirmRow` confirmation pattern; confirmed delete calls `credentialsApi.delete(key)`
- **No secret is ever shown** after creation — the row never renders the stored value

**Add drawer** — fields:
- Key (text, slug-style, required)
- Display Name (text, required)
- Service (text, optional — e.g. "github", "openai")
- Type (`.formSelect`: api_key / pat / bearer_token)
- Secret (password input, `autoComplete="new-password"` to suppress autofill on the wrong field)

Submit calls `credentialsApi.create({ key, display_name, service, credential_type, secret })`, refreshes list, closes drawer.

---

### Task 3 — MCP Servers section
**File:** `web/src/layouts/mobile/MobileSettings.tsx`

**State**
```agent-deck/docs/AD-8.3.md
const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
const [mcpDrawerOpen, setMcpDrawerOpen] = useState(false);
// form fields: name, server_type, executable, args, url, credential_key
```

**On mount** — call `mcpServersApi.list()`.

**Live status via SSE** — subscribe to `lastMcpStatusChange` using the same pattern as `ConfigPane.tsx`. On each event, update the matching server's `status` field in local state without a full re-fetch.

**List rendering** — a `.sectionCard` with one `.listRow` per server:
- Leading: `.statusDot` (class driven by `server.status`) + server name + `.badge` showing "local" or "remote"
- Trailing: enable/disable toggle calling `mcpServersApi.update(id, { enabled: !server.enabled })` + `.deleteButton` with inline `.confirmRow` confirmation; confirmed delete calls `mcpServersApi.delete(id)`

**Add drawer** — fields:
- Name (text, required)
- Type (segmented control: Local / Remote)
- When Local: Executable (text), Args (text, space-separated, parsed to array on submit)
- When Remote: URL (text), Credential Key (text, optional — selects from existing credentials)
- **Out of scope:** the full env-var key/value editor from the desktop form. Power users configure env vars via the desktop UI

Submit calls `mcpServersApi.create(payload)`, refreshes list, closes drawer.

---

### Task 4 — Personas section
**File:** `web/src/layouts/mobile/MobileSettings.tsx`

**State**
```agent-deck/docs/AD-8.3.md
const [personas, setPersonas] = useState<AgentPersona[]>([]);
const [personaDrawerOpen, setPersonaDrawerOpen] = useState(false);
const [editingPersona, setEditingPersona] = useState<AgentPersona | null>(null);
// form fields: name, emoji, system_prompt
```

**On mount** — call `personasApi.list()`.

**List rendering** — a `.sectionCard` with one `.listRow` per persona:
- Leading: emoji character + persona name
- The entire row is tappable (sets `editingPersona` and opens drawer in edit mode)
- Trailing: `.deleteButton` — disabled (greyed out, `title="Cannot delete default persona"`) when `persona.is_default`; otherwise triggers inline `.confirmRow`; confirmed delete calls `personasApi.delete(id)`

**Drawer — dual-mode (create + edit)**
- Header: "Add Persona" or "Edit Persona" depending on `editingPersona`
- Fields:
  - Name (text, required)
  - Emoji (text input, single character, `maxLength={2}`)
  - System Prompt (`.formTextarea`, `rows={6}`, full instructions)
- Footer: "Save" button
  - Create mode: calls `personasApi.create({ name, emoji, system_prompt })`, refreshes, closes
  - Edit mode: calls `personasApi.update(editingPersona.id, { name, emoji, system_prompt })`, refreshes, closes
- On close: reset `editingPersona` to `null` and clear form fields

**Opening the add drawer** — "+ Add Persona" button at bottom of section card sets `editingPersona = null`, clears fields, opens drawer.

---

### Parallelisation note

Tasks 1–4 all write to `MobileSettings.tsx` and must be done sequentially to avoid merge conflicts. Task 5 (CSS) is independent and should be written as a skeleton before Task 1 begins, then extended as needed alongside each subsequent task. The recommended order is:

**T5 skeleton → T1 → T2 → T3 → T4 → T5 fill-in**

---

## Acceptance Criteria

- [ ] Providers section renders in `MobileSettings` below the existing "Install as App" and "Notifications" sections
- [ ] Providers: list shows all configured providers with name, base URL, and enabled toggle
- [ ] Providers: toggling enabled calls `PUT /api/providers/:id` and the UI reflects the updated state
- [ ] Providers: "+ Add Provider" button opens a slide-up full-screen drawer
- [ ] Providers: add form collects name (required), kind (segmented control), base URL, and API key
- [ ] Providers: successful create refreshes the list and closes the drawer
- [ ] Providers: delete with inline confirmation calls `DELETE /api/providers/:id`
- [ ] Credentials section renders below the Providers section
- [ ] Credentials: list shows key, display_name, and service for each credential — no secret values shown
- [ ] Credentials: "+ Add Credential" button opens a slide-up drawer
- [ ] Credentials: add form collects key (required), display_name (required), service (optional), credential_type (dropdown), and secret (password field)
- [ ] Credentials: successful create refreshes the list and closes the drawer
- [ ] Credentials: delete with inline confirmation calls `DELETE /api/credentials/:key`
- [ ] MCP Servers section renders below the Credentials section
- [ ] MCP Servers: list shows name, type badge (local/remote), status dot, and enabled toggle per server
- [ ] MCP Servers: status dots update in near-real-time via `lastMcpStatusChange` SSE events without a full page refresh
- [ ] MCP Servers: toggling enabled calls `PUT /api/mcp-servers/:id`
- [ ] MCP Servers: "+ Add Server" button opens a slide-up drawer
- [ ] MCP Servers: add form shows executable + args fields when type is "local"; URL + credential_key fields when type is "remote"
- [ ] MCP Servers: successful create refreshes the list and closes the drawer
- [ ] MCP Servers: delete with inline confirmation calls `DELETE /api/mcp-servers/:id`
- [ ] Personas section renders below the MCP Servers section
- [ ] Personas: list shows emoji and name per persona; tapping a row opens the edit drawer pre-filled
- [ ] Personas: edit drawer allows updating name, emoji, and system_prompt; save calls `PUT /api/personas/:id`
- [ ] Personas: "+ Add Persona" button opens the same drawer in create mode with empty fields
- [ ] Personas: successful create/update refreshes the list and closes the drawer
- [ ] Personas: delete is disabled (button greyed out) for the default persona
- [ ] Personas: delete with inline confirmation calls `DELETE /api/personas/:id` for non-default personas
- [ ] All drawers are full-screen slide-up overlays consistent with the existing `MobileConfigSheet` visual style
- [ ] All drawers include a sticky header with title and close button, a scrollable body, and a sticky footer with the primary action
- [ ] No desktop-only features are included: no env-var editor, no tool inspector, no Copilot device-auth flow

---

## Human Review Instructions

To be filled in after implementation.

---

## Approval

- [ ] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off
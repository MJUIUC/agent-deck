# AD-8.3 — Mobile Settings Parity + Per-Thread MCP

**Story:** 8.3 — Mobile Settings Parity  
**Branch:** `feature/phase8-mobile-settings`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8

---

## Summary

The mobile app currently only lets users chat and toggle routines on/off. Because agent-deck is designed to run on headless computers, the phone is often the only available admin interface. This story expands `MobileSettings.tsx` with four new global-configuration sections — Providers, Credentials, MCP Servers, and Personas — bringing mobile settings to functional parity with the desktop sidebar. It also adds per-thread MCP attach/detach to `MobileConfigSheet.tsx`, which is the only way to attach tools to a thread on mobile. All work uses the existing API layer and types without modification; the work is entirely in the web layer.

---

## Current State

### Mobile files

- `web/src/layouts/mobile/MobileSettings.tsx` — currently contains only "Install as App" instructions and a push-notification toggle. No provider, credential, MCP, or persona management.
- `web/src/layouts/mobile/MobileConfigSheet.tsx` — per-thread config (persona picker, model picker, routine toggles). **Extended by Task 6 of this story** to add MCP attach/detach.
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

---

### Task 6 — Per-thread MCP attach/detach
**File:** `web/src/layouts/mobile/MobileConfigSheet.tsx`

This is the mobile equivalent of the MCP section in the desktop `ConfigPane`. It lets the user attach and detach MCP servers for the current thread directly from the per-thread config sheet.

**State**
```
const [attachedEntries, setAttachedEntries] = useState<ThreadMcpEntry[]>([]);
const [mcpServersMap, setMcpServersMap] = useState<Record<string, McpServer>>({});
const [mcpLoading, setMcpLoading] = useState(false);
const [showMcpPicker, setShowMcpPicker] = useState(false);
```

Where `ThreadMcpEntry` matches the shape already used in `ConfigPane`:
```
interface ThreadMcpEntry {
  id: string;
  thread_id: string;
  mcp_server_id: string;
  enabled: boolean;
}
```

**On sheet open** — fetch in parallel:
- `threadsApi.listMcpServers(thread.id)` → populate `attachedEntries`
- `mcpServersApi.list()` → populate `mcpServersMap` (keyed by id)

**Live status updates** — subscribe to `lastMcpStatusChange` from `useSseStore`. On each event, update the matching server's `status` in `mcpServersMap` in place, matching the pattern already used in `ConfigPane`.

**Section rendering** — a new "Tools" section in the sheet, below Routines:
- One row per attached server showing a `.statusDot` (reuse class from `MobileSettings.module.css`), the server name, and a detach button (×)
- Tapping detach calls `threadsApi.detachMcpServer(thread.id, mcpServerId)`, then removes the entry from `attachedEntries`; also calls `threadsApi.notify(thread.id, "mcp_server_detached", { name })` (silently degrade on failure)
- An "+ Attach Server" row at the bottom of the section that sets `showMcpPicker = true`

**Attach picker** — rendered as a `<PickerDrawer>` (reuse the existing component in the file) with the title "Attach MCP Server":
- List only servers that are not already attached (filter `mcpServersMap` values against `attachedEntries`)
- Each row shows the status dot and server name
- Tapping a row calls `threadsApi.attachMcpServer(thread.id, server.id)`, appends the returned entry to `attachedEntries`, updates `mcpServersMap`, and closes the picker; also calls `threadsApi.notify(thread.id, "mcp_server_attached", { name })` (silently degrade on failure)
- If all servers are already attached, show a "All available servers are attached" empty state

**No tool inspector** — tool enumeration is out of scope for this sheet; that remains desktop-only in `ConfigPane`.

---

### Parallelisation note

Tasks 1–4 all write to `MobileSettings.tsx` and must be done sequentially to avoid merge conflicts. Task 5 (CSS) is independent and should be written as a skeleton before Task 1 begins, then extended as needed alongside each subsequent task. Task 6 writes only to `MobileConfigSheet.tsx` and `MobileSettings.module.css` (for the `.statusDot` classes, which Task 3 will already have added) and can be done in parallel with Tasks 1–4 once Task 5's CSS skeleton is in place. The recommended order is:

**T5 skeleton → T1 → T2 → T3 → T4 → T5 fill-in**
**T6 can run in parallel with T1–T4 after T5 skeleton is written**

---

## Acceptance Criteria

- [x] Per-thread MCP section renders in `MobileConfigSheet` below the Routines section
- [x] Per-thread MCP: lists currently attached servers with status dot and server name
- [x] Per-thread MCP: status dots update in near-real-time via `lastMcpStatusChange` SSE events
- [x] Per-thread MCP: tapping × detaches the server, calls `DELETE /api/threads/:id/mcp-servers/:mcpServerId`, and removes the row
- [x] Per-thread MCP: "+ Attach Server" opens a picker showing only unattached servers
- [x] Per-thread MCP: selecting a server from the picker attaches it, calls `POST /api/threads/:id/mcp-servers`, and adds the row
- [x] Per-thread MCP: picker shows an empty state when all available servers are already attached
- [x] Providers section renders in `MobileSettings` below the existing "Install as App" and "Notifications" sections
- [x] Providers: list shows all configured providers with name, base URL, and enabled toggle
- [x] Providers: toggling enabled calls `PUT /api/providers/:id` and the UI reflects the updated state
- [x] Providers: "+ Add Provider" button opens a slide-up full-screen drawer
- [x] Providers: add form collects name (required), kind (segmented control), base URL, and API key
- [x] Providers: successful create refreshes the list and closes the drawer
- [x] Providers: delete with inline confirmation calls `DELETE /api/providers/:id`
- [x] Credentials section renders below the Providers section
- [x] Credentials: list shows key, display_name, and service for each credential — no secret values shown
- [x] Credentials: "+ Add Credential" button opens a slide-up drawer
- [x] Credentials: add form collects key (required), display_name (required), service (optional), credential_type (dropdown), and secret (password field)
- [x] Credentials: successful create refreshes the list and closes the drawer
- [x] Credentials: delete with inline confirmation calls `DELETE /api/credentials/:key`
- [x] MCP Servers section renders below the Credentials section
- [x] MCP Servers: list shows name, type badge (local/remote), status dot, and enabled toggle per server
- [x] MCP Servers: status dots update in near-real-time via `lastMcpStatusChange` SSE events without a full page refresh
- [x] MCP Servers: toggling enabled calls `PUT /api/mcp-servers/:id`
- [x] MCP Servers: "+ Add Server" button opens a slide-up drawer
- [x] MCP Servers: add form shows executable + args fields when type is "local"; URL + credential_key fields when type is "remote"
- [x] MCP Servers: successful create refreshes the list and closes the drawer
- [x] MCP Servers: delete with inline confirmation calls `DELETE /api/mcp-servers/:id`
- [x] Personas section renders below the MCP Servers section
- [x] Personas: list shows emoji and name per persona; tapping a row opens the edit drawer pre-filled
- [x] Personas: edit drawer allows updating name, emoji, and system_prompt; save calls `PUT /api/personas/:id`
- [x] Personas: "+ Add Persona" button opens the same drawer in create mode with empty fields
- [x] Personas: successful create/update refreshes the list and closes the drawer
- [x] Personas: delete is disabled (button greyed out) for the default persona
- [x] Personas: delete with inline confirmation calls `DELETE /api/personas/:id` for non-default personas
- [x] All drawers are full-screen slide-up overlays consistent with the existing `MobileConfigSheet` visual style
- [x] All drawers include a sticky header with title and close button, a scrollable body, and a sticky footer with the primary action
- [x] No desktop-only features are included: no env-var editor, no tool inspector, no Copilot device-auth flow
- [x] Per-thread MCP attach/detach calls `threadsApi.notify` for both attach and detach events (silently degrades on failure)

---

## Human Review Instructions

**Prerequisites:** Server running on port 7474. At least one provider, credential, MCP server, and persona already configured via desktop, OR be prepared to create them fresh on mobile.

**Steps:**

1. Open agent-deck on a mobile device (or narrow the browser to mobile width). Navigate to **Settings** tab. → **Expected:** Settings screen shows four new sections below "Install as App" and "Notifications": Providers, Credentials, MCP Servers, Personas. / **Failure:** Sections missing or layout broken.

2. **Providers** — Tap the enabled toggle on an existing provider. → **Expected:** Toggle flips, `PUT /api/providers/:id` fires, state updates without page reload. / **Failure:** Toggle has no effect or page reloads.

3. **Providers** — Tap "+ Add Provider", fill in Name and API Key, tap Save. → **Expected:** Drawer slides up, form accepts input, drawer closes on save, new provider appears in list. / **Failure:** Drawer doesn't open, Save does nothing, or provider doesn't appear.

4. **Providers** — Tap ✕ on a provider, confirm delete. → **Expected:** Inline "Delete?" row appears, confirming calls `DELETE /api/providers/:id` and removes the row. / **Failure:** Confirm row doesn't appear or row persists after delete.

5. **Credentials** — Tap "+ Add Credential", fill in all fields (Key, Display Name, Type, Secret), tap Save. → **Expected:** Credential appears in list showing display_name and key — no secret value shown. / **Failure:** Secret is visible in the list, or create fails.

6. **MCP Servers** — Observe the status dot colour on an existing connected server. → **Expected:** Dot is green. While watching, if a server reconnects the dot updates without manual refresh. / **Failure:** Dot is wrong colour or never updates.

7. **MCP Servers** — Tap "+ Add Server", choose "Remote" type, fill URL, tap Save. → **Expected:** Server fields switch between Executable/Args (local) and URL/Credential Key (remote) based on the segmented control. New server appears in list. / **Failure:** Fields don't switch, or server isn't created.

8. **Personas** — Tap an existing persona row. → **Expected:** Edit drawer opens pre-filled with the persona's name, emoji, and system prompt. Editing and saving calls `PUT /api/personas/:id`. / **Failure:** Drawer opens empty, or save does nothing.

9. **Personas** — Attempt to delete the default persona. → **Expected:** Delete button (✕) is greyed out and non-interactive. / **Failure:** Delete button is active for the default persona.

10. **Per-thread MCP** — Open a thread, tap the config (⚙) button to open `MobileConfigSheet`. Scroll to the **Tools** section below Routines. → **Expected:** "Tools" section visible with any currently attached servers listed, each with a status dot and ✕ button. "+ Attach Server" button at the bottom. / **Failure:** Tools section missing.

11. **Per-thread MCP** — Tap "+ Attach Server". → **Expected:** Picker overlay slides in listing only servers NOT already attached. Tapping one attaches it (calls `POST /api/threads/:id/mcp-servers`) and adds the row. / **Failure:** Already-attached servers appear in picker, or attach call fails.

12. **Per-thread MCP** — Tap ✕ on an attached server. → **Expected:** Row disappears, `DELETE /api/threads/:id/mcp-servers/:mcpServerId` fires. / **Failure:** Row persists or API call not made.

13. **Per-thread MCP empty state** — Attach all available servers, then tap "+ Attach Server" again. → **Expected:** Picker shows "All available servers are attached." / **Failure:** Picker shows servers or is empty without the message.

**Optional server log check:**
```
grep "mcp_server_attached\|mcp_server_detached" ~/.agent-deck/server.log
```

---

## Approval

- [x] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [x] **Human review approved** — human has tested the changes live and signed off

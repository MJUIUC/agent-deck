# Agent-Deck — Phase 3 Implementation Instructions

**For:** Implementing agent  
**Read first:** PLAN.md (v1.4) in full before touching any code  
**Repo:** https://github.com/MJUIUC/agent-deck  
**Current state:** Phase 1, Phase 2, and Phase 2 cleanup all merged to `main`. Phase 3 in progress — Stories 3.1, 3.1a, and 3.2 complete and merged to `main`.

---

## Phase 3 Progress

| Story | Status | Branch | Notes |
|---|---|---|---|
| 3.1 — Setup Wizard | ✅ Complete | `feature/phase3-setup-wizard` | Merged to main |
| 3.1a — Wizard Skip Flow & Empty State | ✅ Complete | `feature/phase3-setup-wizard` | Merged to main |
| 3.2 — Provider Settings Polish | ✅ Complete (no-op) | `feature/phase3-setup-wizard` | Existing UI exceeds mockup — no changes needed |
| 3.4a — Settings Nav Update | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | Merged to main |
| 3.4b — Settings: MCP Servers Tab | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | — |
| 3.4c — Settings: Mobile Tab | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | — |
| 3.4d — Settings: General Tab | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | — |
| 3.5 — Thread Config Pane | ✅ Complete | `feature/phase3-thread-config` | ⚠️ Needs merge to main. Memory section deferred. Model selector saves UUIDs. |
| 3.6 — Slash Command UI | 🔲 Not started | — | Next up |
| 3.7 — Archived Threads | 🔲 Not started | — | — |
| 3.x — Credential Store | 🔲 Not started | — | Part 2 |
| 3.3 Delta — Persona Default MCP Servers | 🔲 Not started | — | Part 2 |

---

## How to Use These Instructions

Work **one story at a time**, on its own feature branch. Each story has a branch name, scope, and acceptance criteria. Do not combine stories. Do not start a new story until the previous one is merged to `main`.

Commit message format: `feat(phase3): <short description>` for features, `fix(phase3): <short description>` for fixes within a phase branch.

**Commit granularity** — commit after each logical task within a story, not just once at the end. A "logical task" is a self-contained unit of work that leaves the codebase in a coherent state (e.g. "server contract change", "toast component", "slash dropdown UI", "ChatView ephemeral messages"). Never batch unrelated changes into a single commit. This keeps the history readable and makes bisection easy.

**Stories must be worked in the order listed within each Part.** Parts are sequential — finish all stories in Part 1 before starting Part 2.

---

## Phase 3 Goal

**The app is fully configurable through its own UI.** After this phase you never need `curl` to manage the system. Setup wizard for first-run, settings pages for ongoing management, thread config for per-conversation tuning, slash commands for power-user shortcuts.

---

## What Already Exists

Phase 2 shipped provider and persona settings ahead of schedule. The following components already work:

- `web/src/components/settings/ProviderSettings.tsx` — full CRUD, Copilot device-auth flow, model sync, test connection
- `web/src/components/settings/PersonaSettings.tsx` — full CRUD, emoji picker, model selection
- `web/src/components/settings/SettingsModal.tsx` — modal shell with tab navigation
- `web/src/components/ConfigPane.tsx` — basic thread config (persona info, model, addendum)

Stories below note what's already done and specify **only the delta work** where applicable. Do not rebuild things that already work.

---

## Part 1 — Core Configuration UI

These stories have no external dependencies. The agent can work through them independently.

---

### Story 3.1 — Setup Wizard ✅ COMPLETE

**Branch:** `feature/phase3-setup-wizard` — merged to `main`

#### Additional fixes shipped with this story (discovered during testing):
- `auth-poll` was never returning `authenticated: true` — GitHub OAuth endpoints require JSON body + `Accept: application/json`, not form-urlencoded
- Copilot sidecar not restarting after token write — `restart()` added to `CopilotApiService`, waits for child to fully exit before respawn to avoid `EADDRINUSE` on port 4141
- Step 4 model dropdown empty — added public `GET /api/providers/copilot/models` endpoint (no auth/DB required) that proxies the sidecar's live model list for use before setup is complete
- Persona creation failing with FK constraint — `default_model` from the wizard held raw sidecar model IDs, not DB row IDs; now resolved via `modelsApi.sync()` result before `POST /api/personas`
- Redundant `✕ Cancel` buttons removed from `ProviderForm` and `PersonaForm` title rows
- Expandable model list added to provider cards — `+N more` is now a clickable toggle showing all models in a scrollable 160px container

The setup wizard is the first thing a new user sees. UX feel, copy, and flow need subjective evaluation.

#### Background

When `GET /api/setup/status` returns `{ "data": { "complete": false } }`, the SPA should show a full-screen wizard instead of the main chat UI. See PLAN.md §7.9 and `mockups/setup-wizard.html`.

#### File Structure

This story introduces a shared wizard infrastructure under `web/src/components/wizards/` so that future wizards (mobile pairing, persona onboarding, etc.) can reuse the same chrome without rebuilding it.

```
web/src/components/wizards/
  shared/
    WizardShell.tsx          ← full-screen overlay, brand header, card container, step transitions
    WizardStepIndicator.tsx  ← dot/line progress indicator (hidden on step 1 of setup wizard)
    WizardNavRow.tsx         ← back / next / skip button row with consistent layout
    WizardCard.tsx           ← centered card with sizing, shadow, overflow handling
    types.ts                 ← WizardStepMeta interface and shared wizard types

  setup-wizard/
    SetupWizard.tsx          ← orchestrator: owns all state, drives step transitions
    Step1Welcome.tsx
    Step2Name.tsx
    Step3Provider.tsx
    Step4Persona.tsx
    Step5Done.tsx
```

**Shared component contracts:**

`WizardShell` — accepts `steps: WizardStepMeta[]`, `currentStep: number`, and `children`. Renders the full-screen dark overlay, brand header, `WizardStepIndicator`, and the `WizardCard` around children. Knows nothing about setup specifically.

`WizardStepIndicator` — accepts `steps: WizardStepMeta[]` and `currentStep: number`. Renders the dot/line row. Dots before `currentStep` are marked done (✓), the current dot is active, the rest are inactive.

`WizardNavRow` — accepts `onBack?`, `onNext?`, `onSkip?`, `nextLabel?`, `nextDisabled?`, `backLabel?`. Renders the bottom nav row with consistent spacing. Skip link only renders when `onSkip` is provided.

`WizardCard` — a simple container: centered, fixed width (~540px), `bg-secondary` background, subtle border, 14px border radius, box shadow. Content scrolls internally if it overflows.

`types.ts`:
```ts
export interface WizardStepMeta {
  label: string;       // shown in the step indicator
  skippable?: boolean; // whether the skip link appears on this step
}
```

#### What to Build — Setup Wizard Steps

**Step 1 — Welcome** (`Step1Welcome.tsx`)
- Hero: 🤖 icon, "Welcome to agent-deck" title, one-line description
- Feature grid: Persistent Memory, Routines, Agent Personas, Private by Design (matching mockup)
- Single "Get Started →" button centered below the grid
- No step indicator on this step (pass `showIndicator: false` or equivalent to `WizardShell`, or simply hide it on step 1 — your call on implementation)
- No skip option

**Step 2 — Your Name** (`Step2Name.tsx`)
- Single `FieldInput` for display name (placeholder: "e.g. Marcus")
- Field hint: "Just a first name or nickname is fine."
- Next button disabled until the field is non-empty
- This name is passed to `POST /api/setup/complete` on the final step

**Step 3 — Add a Provider** (`Step3Provider.tsx`)
- Provider kind picker cards: Copilot (🐙, "No API key" badge), OpenAI (🤖, "API key" badge), Anthropic (✦, "API key" badge), Custom / Local (⚙, "Custom URL" badge)
- Selecting a card shows a dynamic config panel below:
  - **Copilot:** embed the existing `CopilotAuthSection` component unchanged — it owns the full device-code flow internally
  - **OpenAI / Anthropic / Custom:** name field (pre-filled with provider name), base URL field (pre-filled with default for OpenAI/Anthropic), API key field
- On "Next →": if a non-Copilot provider is configured, call `POST /api/providers` to persist it, then call `POST /api/providers/:id/models/sync` to populate models. Store the created provider ID in wizard state for use in Step 4.
- For Copilot: provider record is created after auth completes — check `copilotApi.authStatus()` before advancing; if authenticated and no Copilot provider exists yet, create it.
- Skippable — "Skip for now" link advances without creating a provider

**Step 4 — Create First Persona** (`Step4Persona.tsx`)
- Preset persona template cards (matching mockup): Aldous (🦉), Scout (🧠), Muse (🎨), Custom (⚒)
- Selecting a preset fills name, emoji, and system prompt with sensible defaults (hardcoded in the component)
- Selecting "Custom" shows name + emoji picker + system prompt textarea fields
- Model dropdown: populated from models synced in Step 3; disabled with note if no provider was added
- On "Finish Setup →": call `POST /api/personas` to create the persona
- Skippable — "Skip for now" advances without creating a persona

**Step 5 — Done** (`Step5Done.tsx`)
- Animated ✓ icon
- "You're all set, {displayName}!" heading
- Summary list: name, provider (or "None — add one in Settings"), first persona (or "None — add one in Settings")
- "Open agent-deck →" button calls `POST /api/setup/complete` with `{ display_name }`, then calls `onComplete()` prop
- No back button on this step

**App-level integration:**
- In `App.tsx`, add `setupComplete: boolean | null` state (null = not yet checked)
- On mount, call `setupApi.status()` first — before provider checks or SSE connection
- If `complete === false`, render `<SetupWizard onComplete={() => setSetupComplete(true)} />` as the entire page content (not overlaid on the app — replace it entirely)
- If `complete === true`, render the app as normal
- On `onComplete`, re-fetch setup status and transition to the main app without a page reload
- Add `setupApi.complete(displayName: string)` to `client.ts` if not already present

#### Acceptance Criteria

- [x] Shared wizard components exist in `web/src/components/wizards/shared/` and are not setup-specific
- [x] `WizardShell`, `WizardStepIndicator`, `WizardNavRow`, `WizardCard`, and `types.ts` are all present and independently usable
- [x] Wizard shows automatically on a fresh database (no user row)
- [x] Step indicator is hidden on Step 1, visible from Step 2 onward
- [x] Your Name step: Next button disabled until name is non-empty
- [x] Copilot auth flow works: device code displays, polling succeeds, provider record is created
- [x] OpenAI/Anthropic/Custom provider creation works; models are synced before advancing
- [x] Persona preset cards pre-fill name, emoji, and system prompt correctly
- [x] Custom persona path shows emoji picker and system prompt textarea
- [x] Model dropdown in Step 4 is populated when a provider was added in Step 3
- [x] Model dropdown is disabled with a note when Step 3 was skipped
- [x] Skipping from Step 3 onward works — wizard completes without provider/persona
- [x] Step 5 summary reflects what was actually configured (not hardcoded)
- [x] "Open agent-deck →" calls `POST /api/setup/complete` and transitions to the app without a page reload
- [x] After completion, wizard never shows again (setup status returns complete)
- [x] Wizard matches the visual style in `mockups/setup-wizard.html`
- [x] `npm run build` passes, `cargo build` passes

---

### Story 3.1a — Wizard Skip Flow and Empty State Routing ✅ COMPLETE

**Branch:** `feature/phase3-setup-wizard` — merged to `main`

#### Background

During implementation and testing of Story 3.1, two related gaps were identified:

1. **The wizard's skip flow leads to a broken app state.** If the user skips Step 3 (provider), the current implementation still shows Step 4 (persona), which is meaningless without a provider — the model dropdown is disabled, no models exist, and any persona created here can't actually be used for chat. Skipping the provider should skip the persona step entirely and go straight to Step 5.

2. **The main app's empty state does not account for missing prerequisites.** After the wizard completes, the app can be in one of four states depending on what the user configured. Each state requires a different call to action — just showing "start a new chat" is wrong when the user has no provider or no persona to chat with.

#### What to Build

**Wizard flow correction (`SetupWizard.tsx`, `Step3Provider.tsx`, `Step5Done.tsx`):**

- When the user skips Step 3, go directly to Step 5 — do not show Step 4.
- When the user is on Step 4 and clicks Back, return to Step 3.
- Step 5 Done summary must correctly reflect that no provider and no persona were configured when both were skipped. The "You're all set" copy should be adjusted when the user has skipped both — e.g. "You're in — finish setup in Settings when you're ready." with clear CTAs pointing to Providers and Personas settings.
- The step indicator should visually reflect skipped steps (treat them the same as done — ✓) so the indicator always advances correctly.

**Empty state routing (`web/src/components/EmptyState.tsx`):**

The empty state is shown when there is no active thread. It must handle four cases based on the current app state:

| Provider exists | Persona exists | What to show |
|---|---|---|
| ❌ | ❌ | "You need a provider to get started." Primary CTA: "Add a Provider →" opens Settings → Providers tab. |
| ✅ | ❌ | "Almost there — create your first agent persona." Primary CTA: "Create a Persona →" opens Settings → Personas tab. |
| ❌ | ✅ | "You have personas but no provider connected." Primary CTA: "Add a Provider →" opens Settings → Providers tab. Secondary note: existing personas will be available once a provider is connected. |
| ✅ | ✅ | Normal empty state — "Start a new chat" CTA. This is the current behavior. |

`App.tsx` already fetches provider state on mount and passes `hasProviders` to `EmptyState`. It also has `personas` from the thread store. Pass both through correctly so `EmptyState` can branch on them.

**New chat button guard (`Sidebar.tsx`, `PersonaPickerModal.tsx`):**

- The "New Chat" button in the sidebar must be disabled (with a tooltip) when there are no personas. Clicking it while disabled should not open the persona picker — instead show a brief inline note: "Add a persona in Settings first."
- Do not disable the button when there are personas but no provider — the user can still create a thread, they just can't send messages. The empty state routing handles that case.

#### Acceptance Criteria

- [x] Skipping Step 3 (provider) skips Step 4 (persona) entirely and goes to Step 5
- [x] Step 5 summary copy is adjusted when both provider and persona were skipped
- [x] Back navigation from Step 4 returns to Step 3 correctly
- [x] Step indicator always reflects current progress correctly across all skip paths
- [x] Empty state shows correct CTA for all four provider/persona combinations
- [x] "Add a Provider →" CTA opens Settings modal on the Providers tab
- [x] "Create a Persona →" CTA opens Settings modal on the Personas tab
- [x] "New Chat" button is disabled with a note when no personas exist
- [x] All four empty state cases are visually distinct and clearly communicate what the user needs to do
- [x] `npm run build` passes

---

### Story 3.2 Delta — Provider Settings Polish ✅ COMPLETE (no-op)

**Branch:** `feature/phase3-setup-wizard` — merged to `main`

#### What Already Exists

`ProviderSettings.tsx` already has: provider list with cards, add/edit/delete, Copilot device-auth flow, model sync, kind icons, status badges.

#### Delta Work

Compare the existing implementation against `mockups/settings-providers.html` and address any visual or functional gaps. Likely items:

1. **Status indicators** — verify that each provider card shows a clear connected/error/disabled status. If the existing `StatusBadge` component doesn't match the mockup's design, update it.

2. **Test connection button** — verify it exists on each provider card and shows the model list on success, an error message on failure. This may already work via model sync — confirm.

3. **Mockup fidelity** — open `mockups/settings-providers.html` in a browser, compare side-by-side with the running app's provider settings, and fix any mismatches in layout, spacing, or interaction patterns.

If after comparison everything already matches, this story is a no-op — document that in the commit message and move on.

#### Acceptance Criteria

- [x] Provider settings UI matches `mockups/settings-providers.html` — existing UI is cleaner and more functional than the mockup; no changes needed
- [x] Status indicators show correct state for each provider
- [x] Test connection — removed; Sync Models already proves connectivity and is a better UX
- [x] No regressions in existing provider CRUD
- [x] `npm run build` passes

---

### Story 3.4 — Settings: MCP Servers, Mobile, General

**Branch:** `feature/phase3-settings-mcp-mobile-general`

#### Background

This adds three new settings tabs. MCP server management is the largest piece. See PLAN.md §8.4, §6.6, §7.5.

Stories 3.4a–3.4d all live on the same branch and must be completed in order before the branch is merged.

---

### Story 3.4a — Settings Navigation Update

#### What to Build

Update `SettingsNav.tsx` and `shared.tsx` to add the three new tabs:

- Extend `SettingsTab` type in `shared.tsx` to include `"mcp-servers" | "mobile" | "general"`
- Add `NavItem` entries to `SettingsSidebar` in `SettingsNav.tsx`: MCP Servers (🖥️), Mobile (📱), General (⚙️) — placed after the existing Personas entry
- Add stub content rendering in `SettingsModal.tsx` for each new tab (placeholder `<div>` is fine — the real content comes in 3.4b–3.4d)

#### Acceptance Criteria

- [ ] `SettingsTab` type includes `"mcp-servers"`, `"mobile"`, `"general"`
- [ ] All three new tabs appear in the settings sidebar nav
- [ ] Clicking each tab renders without crashing (stub content acceptable)
- [ ] Existing Providers and Personas tabs are unaffected
- [ ] `npm run build` passes

---

### Story 3.4b — Settings: MCP Servers Tab

#### What to Build

A full management surface for MCP servers, replacing the stub from 3.4a:

Create `web/src/components/settings/McpServerSettings.tsx`:

- List of configured MCP servers, each showing: name, type badge (`local` / `remote`), status badge (`connected` / `inactive` / `error`), description, source URL as clickable link
- Expandable tool inspector per server — clicking expands to show tool names and descriptions (fetched from `GET /api/mcp-servers/:id/tools`)
- "Add Server" button opens an inline form with:
  - Name (required)
  - Type: local or remote (radio/toggle — changes which config fields appear)
  - Description (optional)
  - Source URL (optional)
  - **Local config:** Executable path, Args (comma-separated or one-per-line), Environment variables (key-value pairs)
  - **Remote config:** URL, Auth header name (default: `Authorization`), Credential key (free text for now — will become a dropdown of configured credentials after Story 3.x)
- Edit and delete actions per server (delete with confirmation)
- All CRUD calls go to the existing `/api/mcp-servers` endpoints

Wire `McpServerSettings` into `SettingsModal.tsx` in place of the stub.

#### Acceptance Criteria

- [ ] MCP server list displays with type badge, status badge, description, source URL
- [ ] Add server form shows correct fields for local vs remote type
- [ ] MCP server CRUD works: create, edit, delete (with confirmation)
- [ ] Tool inspector shows tools for connected servers (or empty state for disconnected)
- [ ] `npm run build` passes

---

### Story 3.4c — Settings: Mobile Tab

#### What to Build

Create `web/src/components/settings/MobileSettings.tsx`:

- QR code display for mobile pairing
- Fetch pairing data from `GET /api/pairing/qr` and render as a QR code
- Use a client-side QR library (e.g., `qrcode.react` or generate as SVG)
- Instructional text: "Scan this code with the Agent-Deck mobile app to pair."

Wire `MobileSettings` into `SettingsModal.tsx` in place of the stub.

#### Acceptance Criteria

- [ ] Mobile tab shows QR code with correct pairing data
- [ ] Instructional text is present
- [ ] `npm run build` passes

---

### Story 3.4d — Settings: General Tab

#### What to Build

Create `web/src/components/settings/GeneralSettings.tsx`:

- Auth token display: masked by default, with a reveal/hide toggle
- Token rotation button with a confirmation dialog warning: "Rotating the token will disconnect all remote browsers and mobile devices. You'll need to re-enter the token on other devices and re-pair the mobile app."
- Token rotation calls `POST /api/auth/token/rotate`

Wire `GeneralSettings` into `SettingsModal.tsx` in place of the stub.

#### Acceptance Criteria

- [ ] General tab shows masked token with reveal toggle
- [ ] Token rotation works with confirmation warning
- [ ] `npm run build` passes, `cargo build` passes

---

### Story 3.5 — Thread Config Pane ⚠️ In Progress

**Branch:** `feature/phase3-thread-config`

#### ⚠️ This story requires human review before merging.

The thread config pane is always one click away during chat. Layout, transitions, and interaction feel need subjective evaluation.

#### What Already Exists

`ConfigPane.tsx` has a basic implementation: persona info (read-only), model display, system prompt addendum textarea.

#### What to Build

Replace or significantly expand the existing `ConfigPane.tsx` to match `mockups/thread-config.html`. The pane slides in from the right over the chat view.

**Sections (top to bottom):**

1. **Persona** (read-only) — emoji, name, avatar. Clicking opens persona settings.

2. **Model** — dropdown of available models for the thread's active provider. Switching updates `active_model` on the thread immediately via `PUT /api/threads/:id`. The current model should be pre-selected.

3. **Routines** — UI shell only (routine CRUD is Phase 4). Show:
   - Empty state: "No routines yet. Add one to schedule automated messages."
   - "Add Routine" button (disabled with tooltip: "Coming soon")
   - When routines exist (Phase 4): list with name, cron description, enabled toggle, edit/delete

4. **MCP Servers** — list of servers attached to this thread:
   - Each server shows: name, status badge, type badge (local/remote)
   - One-line description
   - Source URL link (if set)
   - Expandable tool list — click to reveal tools the server exposes
   - Remove button (detaches from thread, doesn't delete the server)
   - "+ Attach server" button opens a searchable picker overlay showing all configured MCP servers not yet attached to this thread. Selecting one calls the appropriate API to attach it.

5. **Tool Activity** — toggle: "Show tool activity in chat" (default: off). Persists per-thread via a `show_tool_activity` column on the `threads` table. When enabled, hidden messages (`visibility: hidden`) with tool call content render as collapsible disclosure rows in the chat view. (The actual rendering of hidden messages in ChatView can be deferred to Phase 4 if complex — the toggle itself must exist and persist.)

   **Schema change required:** This story must add a new migration:
   ```sql
   ALTER TABLE threads ADD COLUMN show_tool_activity INTEGER NOT NULL DEFAULT 0;
   ```
   Update the `Thread` model in `server/src/models/thread.rs` to include `show_tool_activity: bool`. Update `GET /api/threads/:id` and `PUT /api/threads/:id` to read and write this field. Run `cargo sqlx prepare` and commit the updated `.sqlx/` directory.

6. **System Prompt Addendum** — textarea that saves on blur. Applied on top of the persona's system prompt. Include a note: "This text is appended to the persona's system prompt for this thread only."

**Pane behavior:**
- Opens via a config/settings icon in the chat header
- Slides in from the right, overlaying the chat view (not pushing it)
- Close button or click-outside to dismiss
- Smooth CSS transition

#### As-built notes

- **Memory section omitted by design.** The mockup includes a "Recent Memory" section but memory is not in scope for Phase 3. The section has been intentionally skipped. It should be added in a future phase when memory infrastructure exists. Note this in the Phase 4 or Phase 5 planning docs.
- **Model selector redesigned.** Instead of a native `<select>` dropdown (as shown in the mockup), the implemented selector uses a two-row custom UI: a row of styled provider pill buttons on top, and a scrollable list of model rows for the selected provider below. This matches the app's design theme and avoids native OS styling.
- **Migration 003 recreated.** The file `server/src/db/migrations/003_thread_show_tool_activity.sql` was found truncated on the branch. It was deleted and recreated with the correct `ALTER TABLE` statement. Since the project is in development mode with no production data, this is safe.
- **`onOpenSettings` prop added to `ConfigPane`.** The persona card is clickable and accepts an optional `onOpenSettings` callback. `ChatView` does not currently pass this prop — it can be wired up when the settings modal is accessible from the chat view.
- **Overlay added.** The pane now renders a dim overlay behind it (matching the mockup) that also closes the pane on click. The overlay and pane are rendered as a React fragment so they sit correctly in the `ChatView` absolute-positioned container.
- **`cargo sqlx prepare` must be run** after the server compiles successfully with the new `show_tool_activity` column. Run from `server/` and commit the updated `.sqlx/` directory before merging.

#### Acceptance Criteria

- [x] Pane opens and closes with smooth animation
- [x] Model switcher works — selection persists and takes effect on next message
- [x] Routines section shows empty state with disabled "Add Routine" button
- [x] MCP servers section lists attached servers with status/type badges and tool inspector
- [x] "+ Attach server" picker shows unattached servers and attaching works
- [x] Removing a server detaches it from the thread
- [x] `show_tool_activity` column added to `threads` table via migration
- [x] `Thread` model, GET, and PUT endpoints updated to include `show_tool_activity`
- [x] Tool activity toggle persists per-thread (reads and writes `show_tool_activity`)
- [x] `cargo sqlx prepare` run and `.sqlx/` committed
- [x] Addendum textarea saves on blur
- [x] Matches `mockups/thread-config.html` layout and style (Memory section intentionally omitted)
- [x] `npm run build` passes

#### Post-review fixes (same branch)

- **Model selector saves UUIDs.** `active_provider` and `active_model` are stored as record UUIDs (matching what `agent.rs` expects). Display names are resolved client-side only — in `ProviderModelSelector` and `ChatHeader`.
- **`ChatHeader` shows persona default model.** When `thread.active_provider` / `active_model` are null (fresh thread), the header now falls back to `persona.default_provider` / `default_model` (also resolved from UUIDs to display names via the module-level cache).
- **Streaming bubble appears immediately.** `isStreaming` is set to `true` as soon as `sendMessage` fires (not waiting for the first SSE token). The `finally` block resets it if no tokens ever arrived, preventing a hanging indicator on error.
- **Missing `show_tool_activity` in agent.rs and messages.rs SELECTs fixed.** Both queries were missing the column and would have crashed at runtime after migration 003 runs.

---

### Story 3.6 — Slash Command UI and Server Endpoint

**Branch:** `feature/phase3-slash-commands`

#### ⚠️ This story requires human review before merging.

The autocomplete UX and command result display need subjective evaluation.

#### Background

See PLAN.md §6.8.1 and §7.3. Slash commands are typed in the chat input, intercepted client-side, and routed to `POST /api/threads/:id/command` instead of the message endpoint. Results are ephemeral — displayed in chat but never persisted.

#### Design Decisions (agreed before implementation)

1. **`args` type**: `SlashCommandRequest.args` changes from `Option<String>` to `Vec<String>`. The client splits the input; the server receives a pre-split array and passes it directly to handlers. No `split_whitespace` on the server side.

2. **TypeScript payload types**: Keep `SlashCommandPayload` loose (index signature `[key: string]: unknown`) rather than defining per-command discriminated unions. The server rejects unknown commands cleanly, so strict client-side typing isn't needed yet. Leave a `TODO` comment as a reminder to tighten types when the command surface stabilises.

3. **`/model switch` UX**: The `switch` handler accepts either the UUID primary key **or** a case-insensitive display name match (e.g. `/model switch gpt-4o`). The `/model list` result renders display names prominently so users know what to type. After a successful `model_switched` response, the client calls `useThreadStore.upsertThread` with the updated `active_model` so the config pane and chat header reflect the change immediately.

4. **Toast component**: A new `Toast` / `ToastProvider` component is introduced in this story. Rules:
   - Appears at the top of the page, centre-aligned.
   - Three variants: `success` (green), `error` (red), `neutral` (default muted).
   - Auto-dismisses after **3 seconds** (hardcoded for now — TODO: make configurable via env/config).
   - Toasts stack vertically and dismiss in the order they were triggered (FIFO).
   - Used for app-level feedback: `/routine add` "coming soon", model switch confirmation, future provider token refresh errors, etc.
   - Command errors (unknown command, bad args) are shown as ephemeral messages in the chat, **not** toasts — errors are contextual to the conversation.

5. **Ephemeral messages**: Command results are displayed inline in the chat as ephemeral messages — visually distinct from real messages (subtle background, "⚡ Slash Command" tag, italic metadata). They are stored in `ChatView` local state as `Record<threadId, EphemeralMessage[]>` and are cleared on page refresh or when the component unmounts. They are interleaved with real messages by timestamp.

6. **Dropdown commands**: The autocomplete panel shows only the four spec-defined commands. Mockup-only commands (`/remember`, `/recall`, `/clear`, `/retry`, `/prompt`) are **not** included — they belong to later phases and are not implemented here.

   | Command | Icon | Arg hint | Description | Badge |
   |---|---|---|---|---|
   | `/model` | 🔄 | `list \| switch <name>` | List or switch the active model for this thread | Model |
   | `/routine` | ⚡ | `list \| add` | List routines or open the add-routine editor | System |
   | `/memory` | 🗂 | `list` | Show recent memories for this thread's persona | Memory |
   | `/help` | ❓ | *(none)* | Show all available commands | System |

#### What to Build — Server

The route `POST /api/threads/:id/command` already exists in `server/src/routes/messages.rs`. The delta work is:

**Request body:**
```json
{ "command": "model", "args": ["switch", "gpt-4o"] }
```

**Server contract change:** Update `SlashCommandRequest.args` from `Option<String>` to `Vec<String>`. Remove the internal `split_whitespace` logic in `slash_command()` and pass `payload.args` directly to handlers.

**Response:**
```json
{ "data": { "type": "model_switched", "message": "Switched to gpt-4o", "payload": { ... } } }
```

**Command handlers:**

| Command | Args | Action |
|---|---|---|
| `model` | `list` | Return available models for the thread's active provider |
| `model` | `switch <name-or-id>` | Match by UUID first, then case-insensitive `display_name`; update `active_model` on the thread |
| `routine` | `list` | Return routines attached to this thread |
| `routine` | `add` | Return `{ "type": "open_add_routine_modal" }` signal |
| `memory` | `list` | Return last 20 memories for the thread's persona |
| `help` | *(none)* | Return all commands with descriptions |

Unknown commands return a `400 Bad Request` (existing `AppError::BadRequest` path) — **not** a 200 with `type: "error"`. The client catches the error and displays it as an ephemeral message.

Unit tests for command parsing and each handler. Tests must pass `args` as a `Vec<String>` directly.

#### What to Build — Client

**New: `ToastProvider` + `useToast` hook**
- `web/src/components/toast/ToastProvider.tsx` — context provider, renders the toast stack at the top of the page
- `web/src/hooks/useToast.ts` — `toast(message, variant?)` function; variant is `"success" | "error" | "neutral"` (default `"neutral"`)
- Mount `<ToastProvider>` in `App.tsx` (or the root layout)

**Autocomplete panel: `SlashDropdown` component**
- New file: `web/src/components/SlashDropdown.tsx` + `SlashDropdown.module.css`
- Rendered inside `MessageInput` when input starts with `/`
- Positioned absolutely above the input row (matches mockup)
- Shows the four commands listed in the Design Decisions table above
- Typing after `/` filters the list by command name prefix
- Arrow keys navigate highlighted item; Enter fills the command into the input; Escape dismisses; Tab also fills (matches mockup JS behaviour)
- Clicking an item fills the command

**Updated: `MessageInput.tsx`**
- Integrate `SlashDropdown` — show/hide based on input value
- Thread arrow-key and Escape events through to the dropdown when it is visible (suppress default scroll behaviour on arrow keys)
- On Enter when dropdown is visible and an item is highlighted: fill command, do **not** submit

**Updated: `messagesApi.sendCommand()` in `client.ts`**
- Change signature: `args: string[]` (was `args?: string`)
- Body: `JSON.stringify({ command, args })`

**Updated: `useMessageStore.sendCommand()`**
- Parse input into `command` (string) and `args` (`string[]`) — e.g. `/model switch gpt-4o` → `{ command: "model", args: ["switch", "gpt-4o"] }`
- On success: return the response data to the caller
- On error: return `null` (caller renders the error as an ephemeral message)

**Updated: `ChatView.tsx`**
- Add `ephemeralMessages: Record<string, EphemeralMessage[]>` local state
- After `sendCommand` resolves: prepend a "command echo" ephemeral entry (the raw `/...` input the user typed) and an ephemeral result entry (the server response)
- Interleave ephemeral entries with real messages by timestamp when rendering
- For `type === "model_switched"`: call `upsertThread` with updated `active_model` to keep config pane and header in sync; also fire a `success` toast
- For `type === "open_add_routine_modal"`: fire a `neutral` toast "Routine editor coming in Phase 4"
- For error responses (caught exception): add ephemeral error entry; fire an `error` toast

**New: `EphemeralBubble` component** (in `MessageBubble.tsx` or its own file)
- Visually distinct: subtle muted background, "⚡ Slash Command" tag, italic "ephemeral — not saved" metadata
- Renders command result content: plain text for most responses; formatted list for `memory_list`, `model_list`, `routine_list`

**Updated: `SlashCommandPayload` type in `types/index.ts`**
- Add index signature: `[key: string]: unknown`
- Leave a `TODO` comment to tighten to discriminated union once command surface stabilises

#### Acceptance Criteria

- [ ] `SlashCommandRequest.args` is `Vec<String>` on the server; internal `split_whitespace` removed
- [ ] `model switch` handler accepts UUID or case-insensitive display name
- [ ] `messagesApi.sendCommand()` in `client.ts` accepts `args: string[]`
- [ ] Server endpoint handles all six commands correctly
- [ ] Unknown commands return `400 Bad Request`
- [ ] Unit tests for command parsing, each handler, and unknown command error; args passed as `Vec<String>`
- [ ] `ToastProvider` mounted at app root; `useToast` hook works from any component
- [ ] Toast variants: success (green), error (red), neutral (muted); auto-dismiss at 3 s; stacks FIFO
- [ ] Autocomplete panel appears on `/` in the message input; disappears when input no longer starts with `/`
- [ ] Arrow key navigation and Enter selection work; Escape dismisses
- [ ] Filtering works (typing `/mod` shows only `model`)
- [ ] Dropdown shows exactly the four spec commands with correct icons, arg hints, descriptions, and badges
- [ ] Command results display as ephemeral messages with distinct styling (tag + italic metadata)
- [ ] Ephemeral messages cleared on thread switch or page refresh
- [ ] `/model switch` updates `active_model` in thread store; config pane and header reflect the change; success toast fires
- [ ] `/model list` renders display names in a readable format
- [ ] `/memory list` shows formatted memories
- [ ] `/help` shows all commands
- [ ] `/routine add` fires a neutral "coming soon" toast
- [ ] Unknown commands show ephemeral error in chat
- [ ] Normal messages (not starting with `/`) are unaffected
- [ ] `npm run build` passes, `cargo build` passes, all tests pass

---

### Story 3.7 — Archived Threads

**Branch:** `feature/phase3-archived-threads`

#### What to Build

**Server:**
- `POST /api/threads/:id/archive` and `POST /api/threads/:id/unarchive` are already implemented in `server/src/routes/threads.rs` and registered in the router. No server work required for this story.
- `GET /api/threads` already accepts a `?status=active` or `?status=archived` filter (default: `active`). No server work required.

**Client:**
- Add an "Archived" link in the sidebar (below Settings)
- Clicking it shows a list of archived threads with titles and last message previews
- Each archived thread has an "Unarchive" action
- Clicking an archived thread opens it in read-only mode (or navigates to it normally — your call)
- Add an "Archive" action to the thread context menu or thread config pane
- Empty state: "No archived threads."

#### Acceptance Criteria

- [ ] Archiving a thread removes it from the main sidebar list
- [ ] Archived threads appear in the archived view
- [ ] Unarchiving moves a thread back to the active list
- [ ] Empty state shown when no archived threads
- [ ] Thread list default only shows active threads
- [ ] `threadsApi` in `client.ts` has `unarchive(id)` calling `POST /api/threads/:id/unarchive` (mirrors the existing `archive()` method)
- [ ] `npm run build` passes

---

## Part 2 — Credential Store and Infrastructure

This part builds the encrypted credential storage system. OAuth provider implementations (Google, GitHub) are **deferred to Phase 4** — this part focuses on the storage infrastructure and the ownership/resolution model that everything else builds on.

---

### Credential Store Design — READ THIS FIRST

Before implementing Story 3.x, understand the full credential ownership and resolution model. This is the architectural foundation for how agents access external services.

#### Three Ownership Levels

Every credential has an `owner_type` that determines who it belongs to and when it can be used:

**`system`** — Shared infrastructure credentials with no identity concept. A weather API key, a news API key, a Wolfram Alpha token. Any persona, any thread can use these. They are configured once in the global MCP server settings and attached to servers at the server config level.

**`user`** — The human user's personal accounts. Your Gmail, your Google Calendar, your personal GitHub. When the agent uses these, it is acting *on your behalf*. These are scoped to the user and available to any persona (because every persona is acting as your assistant).

**`persona`** — Accounts that belong to a specific agent persona. A coding persona's own GitHub account, a social media persona's own Twitter handle. When the agent uses these, it is acting *as itself*. These are scoped to a specific `persona_id` and only available in threads using that persona.

#### Credential Binding Happens at Configuration Time

**This is the key design principle.** The agent never chooses which credential to use at runtime. The credential is bound when an MCP server is attached to a thread.

When you configure a remote MCP server (e.g., Gmail), you specify a `credential_key` in its config. When that server is attached to a thread, the system resolves the credential at connection time — not when the agent makes a tool call.

If a thread needs access to *both* the user's Gmail and the persona's Gmail, you attach two separate MCP server configurations — one bound to the user's credential, one bound to the persona's credential. They appear as two distinct servers with different names and different namespaced tools.

#### Tool Namespacing by Server Name

When multiple MCP servers are attached to a thread and expose tools with the same base names, the tools must be namespaced by server name to avoid collisions. The context assembler (Story 2.4) should prefix tool names with the server name:

```
marcus_gmail/read_inbox      (user's Gmail)
nexus_gmail/read_inbox        (persona's Gmail)
weather_api/get_forecast      (system service)
```

The server name is the human-readable name from the `mcp_servers` table, sanitized to a safe identifier (lowercase, underscores for spaces, no special characters).

**Note:** Tool namespacing implementation belongs in the context assembler when MCP tools are fully wired up (Phase 4/7). For now, document this convention in a code comment in `context.rs` where the MCP tool stub lives.

#### Agent Awareness via System Prompt

When the context assembler builds the system prompt, it should include a section describing available tools with ownership labels. This is injected automatically — not editable by the user:

```
## Available Tools & Credentials

You have access to the following MCP servers on this thread:
- Marcus's Gmail (owner: Marcus — user's personal account) → tools: marcus_gmail/read_inbox, marcus_gmail/send_email
- Nexus's Gmail (owner: Nexus — your own account) → tools: nexus_gmail/read_inbox, nexus_gmail/send_email
- Weather API (shared service — no identity) → tools: weather_api/get_forecast

When asked to act on the user's behalf (e.g., "check my emails"), use tools from user-owned servers.
When acting on your own behalf (e.g., "push this to your repo"), use tools from your own servers.
```

**Note:** This system prompt injection belongs in the context assembler when MCP tools are fully wired up (Phase 4/7). For now, document this convention in a code comment alongside the tool namespacing comment.

#### Credential Resolution at Runtime

When the server needs to inject a credential into an MCP server connection (Phase 4+):

1. Look up the `credential_key` from the MCP server's config
2. Find the matching `credentials` row
3. Decrypt the `encrypted_data` using the master key
4. If the credential is OAuth and `expires_at` has passed, call the OAuth provider's `refresh_token()` method, re-encrypt the new tokens, and update the row (this is Phase 4 work — just design the schema to support it)
5. Inject the resolved token into the MCP server's auth header

The resolved secret is held only in memory for the duration of the request — it is never logged, cached to disk, or included in any API response.

---

### Story 3.x — Credential Store and Encryption

**Branch:** `feature/phase3-credential-store`

#### Background

See PLAN.md §6.5. The credential store holds encrypted API keys and OAuth tokens. Encryption uses AES-256-GCM. A master key is generated on first run and stored in `app_config`.

#### What to Build

**Master key management:**
- On server startup, check `app_config` for key `credential_master_key`
- If not present, generate a random 256-bit (32-byte) key, encode as hex, and store it
- Load the master key into `AppState` for use by credential operations
- The master key must **never** appear in log output or API responses

**Encryption helpers** (extend `server/src/services/encryption.rs`):
- `encrypt_credential(plaintext: &str, master_key: &str) -> Result<String>` — AES-256-GCM encrypt, return as base64-encoded `nonce:ciphertext`
- `decrypt_credential(encrypted: &str, master_key: &str) -> Result<String>` — reverse

**Credential model updates:**

The `credentials` table already exists from the alignment migration (002). Ensure the `Credential` model and CRUD operations support the three-tier ownership model:

```rust
pub struct Credential {
    pub id: String,
    pub key: String,               // unique lookup key, e.g. "google_oauth_marcus"
    pub display_name: String,      // "Marcus's Google Account"
    pub provider: String,          // "google", "github", "openai", "custom"
    pub credential_type: String,   // "oauth2", "api_key", "custom"
    pub owner_type: String,        // "system" | "user" | "persona"
    pub persona_id: Option<String>, // set only when owner_type = "persona"
    // NOTE: encrypted_data is NEVER included in this public struct
    pub scopes: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

Verify the `owner_type` CHECK constraint in the database includes `'system'` in addition to `'user'` and `'persona'`. If the migration only has `CHECK (owner_type IN ('user', 'persona'))`, create a new migration (003) that recreates the constraint to include `'system'`:

```sql
-- Migration 003: Add 'system' to owner_type constraint on credentials
-- SQLite doesn't support ALTER CONSTRAINT, so recreate the table
CREATE TABLE credentials_new (
  -- ... same columns as credentials ...
  owner_type TEXT NOT NULL CHECK (owner_type IN ('system', 'user', 'persona')),
  -- ... rest of columns ...
);
INSERT INTO credentials_new SELECT * FROM credentials;
DROP TABLE credentials;
ALTER TABLE credentials_new RENAME TO credentials;
```

**Credential CRUD endpoints:**
- `POST /api/credentials/api-key` — store an API key credential (encrypts the secret). Body must include `owner_type`. Validate that `persona_id` is set when `owner_type = 'persona'`, null otherwise.
- `GET /api/credentials` — list all credentials, **metadata only** (never returns `encrypted_data`). Accept optional query params: `?owner_type=system`, `?persona_id=<id>`
- `DELETE /api/credentials/:id` — delete a credential

**Hard security rule:** The `encrypted_data` column must **never** appear in any API response, log line, or error message. The public `Credential` struct (used for API serialization) excludes it. A separate internal `CredentialWithData` struct (from the models created during alignment) is used only when decrypting server-side.

**Settings UI — Credentials tab:**

Add a "Credentials" tab to settings (or add a credentials section to an existing tab). This is *not* the full OAuth Accounts tab (that's Phase 4) — it's a simple credential management surface:

- List credentials grouped by ownership: "System Credentials", "Your Credentials", then per-persona sections
- Each entry shows: display name, provider, credential type, owner badge (system/user/persona name), scopes if set
- "Add API Key" button opens a form: display name, key (unique identifier), provider, owner type selector (system/user/persona — persona shows a persona picker), secret field (password input)
- Delete with confirmation
- Secret field is write-only — once saved, the UI shows "••••••••" with no reveal option

**Document the design conventions:**

Add a code comment in `server/src/services/context.rs` at the MCP tool stub (`// TODO: Phase 7 — load tools from attached MCP servers`) documenting:
1. Tool namespacing convention: tools are prefixed with the sanitized server name
2. System prompt injection convention: available tools listed with ownership labels
3. Reference this section of the Phase 3 instructions for the full design

#### Integration Tests

- Store a system credential, retrieve metadata (confirm no secrets in response), decrypt server-side (confirm round-trip works)
- Store a user credential and a persona credential, list with `?owner_type=user` filter, confirm only user credentials returned
- Store a credential with `owner_type=persona` but no `persona_id` — confirm validation rejects it
- Delete a credential, confirm it's gone

#### Acceptance Criteria

- [ ] Master key generated on first run and persists across restarts
- [ ] Master key never appears in logs or API responses
- [ ] `owner_type` supports three values: `system`, `user`, `persona`
- [ ] `POST /api/credentials/api-key` stores encrypted credential with correct owner_type
- [ ] Validation: `persona_id` required when `owner_type = persona`, rejected otherwise
- [ ] `GET /api/credentials` returns metadata without `encrypted_data`
- [ ] `GET /api/credentials?owner_type=system` filters correctly
- [ ] `DELETE /api/credentials/:id` works
- [ ] Settings UI shows credentials grouped by ownership
- [ ] "Add API Key" form enforces ownership rules
- [ ] Integration tests pass for all ownership scenarios
- [ ] Design conventions documented in context.rs comments
- [ ] `cargo build` passes, `npm run build` passes, all tests pass

---

### Story 3.3 Delta — Persona Default MCP Servers

**Branch:** `feature/phase3-persona-mcp-defaults`

#### What Already Exists

`PersonaSettings.tsx` has full CRUD: list, add, edit, delete, emoji picker, model selection.

#### Delta Work

Add a **Default MCP Servers** section to the persona edit view:

- Shows MCP servers that will be auto-attached to every new thread created with this persona
- Same visual style as the MCP list in the thread config pane (Story 3.5) — server name, type badge, remove button
- "+ Add default server" picker — shows all configured MCP servers
- Label: "These servers are attached automatically when a new thread is created with this persona."
- Adding a default server: `INSERT INTO persona_default_mcp_servers (persona_id, mcp_server_id)`
- Removing: `DELETE FROM persona_default_mcp_servers WHERE persona_id = ? AND mcp_server_id = ?`
- The auto-attach logic on thread creation already exists from Story A.2 (alignment phase) — verify it still works

**Note:** The persona "Accounts" tab (for persona-owned OAuth connections) is **deferred to Phase 4** when the OAuth framework is built. Do not add an Accounts tab in this story.

#### Acceptance Criteria

- [ ] Default MCP Servers section appears in persona edit view
- [ ] Can add and remove default MCP servers for a persona
- [ ] Creating a new thread with a persona auto-attaches its default servers
- [ ] Visual style matches the MCP section in the thread config pane
- [ ] `npm run build` passes, `cargo build` passes

---

## Phase 3 Completion Checklist

Before declaring Phase 3 done, verify the complete flow:

1. [ ] Fresh database → setup wizard shows and completes successfully
1a. [ ] Skipping provider in wizard skips persona step and goes to Step 5 directly
1b. [ ] Empty state routes correctly based on provider/persona state across all four cases
2. [ ] Settings: all tabs load (Providers, Personas, MCP Servers, Credentials, Mobile, General)
3. [ ] Provider CRUD works with Copilot auth, OpenAI, and custom providers
4. [ ] Persona CRUD works with default MCP servers section
5. [ ] MCP server CRUD works for both local and remote types
6. [ ] Credential store: API keys stored encrypted, never exposed in API
7. [ ] Credentials respect three ownership levels (system, user, persona)
8. [ ] Thread config pane: model switch, MCP attach/detach, tool activity toggle, addendum save
9. [ ] Slash commands: `/help`, `/model list`, `/model switch`, `/memory list` all work
10. [ ] Archived threads: archive from sidebar, view archived, unarchive
11. [ ] Token rotation in General settings works with warning
12. [ ] QR pairing code displays correctly
13. [ ] No `curl` needed for any management task — everything works through the UI

---

## Key Rules (Carried Forward)

1. **Read PLAN.md in full before starting any story.** The plan is the spec.
2. **One story per branch.** Never combine stories.
3. **Acceptance criteria = definition of done.** All criteria must pass.
4. **SQLx offline mode.** After schema changes, run `cargo sqlx prepare` and commit `.sqlx/`.
5. **Never expose `encrypted_data` in API responses.** Hard security rule.
6. **Hidden messages stay hidden.** Default filter on `GET /api/threads/:id/messages`.
7. **Mockups are the visual reference.** Match them for every UI story.
8. **Human review stories:** 3.1, 3.1a, 3.5, 3.6 — do not merge without human review.

---

## What Comes Next

After Phase 3 is complete, proceed to **Phase 4 — Memory, Routines, and OAuth**. Phase 4 brings three major capabilities:

1. **Persistent memory** — the `save_memory` and `recall_memory` tools are already wired into the agent run-loop; Phase 4 adds the memory viewer UI, the `/memory` slash command display, and the memory management settings page.

2. **Routines** — cron-scheduled prompts with the two-phase execution model (silent background execution → visible synthesized output). This is what makes Agent-Deck more than a chat wrapper.

3. **OAuth framework** — the `OAuthProvider` trait, Google and GitHub implementations, the Accounts settings tab, and persona-owned account connections. This was deferred from Phase 3 to avoid blocking on external service registration. The credential store from Phase 3 provides the encrypted storage foundation; Phase 4 adds the OAuth flows on top.

The credential store's three-tier ownership model and the tool namespacing conventions documented in Phase 3 are prerequisites for Phase 4's MCP credential resolution and routine execution.

Phase 4 instructions will be provided in a separate document.

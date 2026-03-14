# Agent-Deck — Phase 2 Cleanup Instructions

**For:** Implementing agent  
**Repo:** https://github.com/MJUIUC/agent-deck  
**Current state:** Phase 2 merged to `main`. These cleanup stories must be completed before Phase 3 begins.

---

## How to Use These Instructions

Work **one story at a time**, on its own feature branch. Each story has a branch name, scope, and acceptance criteria. Do not combine stories.

Commit message format: `fix(phase2): <short description>`

---

## Why This Matters

Phase 2 shipped functional but accumulated some technical debt along the way. Cleaning this up now prevents compounding issues in Phase 3 (which adds settings pages, config UI, and more component surface area). These are small, focused fixes — none should take more than a session.

---

## Story C.1 — Break up SettingsModal into separate components

**Branch:** `fix/phase2-settings-decompose`

`web/src/components/SettingsModal.tsx` is currently **2,794 lines** — a single file containing providers CRUD, personas CRUD, emoji picker, Copilot device-auth flow, model sync, and all associated state. Phase 3 will add MCP server management, credentials, accounts, and OAuth flows to this same settings surface. If it stays monolithic, it will be unmaintainable.

**What to do:**

Break `SettingsModal.tsx` into the following structure:

```
web/src/components/settings/
├── SettingsModal.tsx          # Shell: modal chrome, tab navigation, active tab routing
├── ProviderSettings.tsx       # Providers list, add/edit/delete, Copilot auth flow, test connection
├── PersonaSettings.tsx        # Personas list, add/edit/delete, emoji picker, model selection
├── ProviderForm.tsx           # The add/edit form for a single provider (extracted from ProviderSettings)
├── PersonaForm.tsx            # The add/edit form for a single persona (extracted from PersonaSettings)
└── EmojiPicker.tsx            # The emoji quick-palette + full picker with categories and search
```

**Rules:**

- `SettingsModal.tsx` should be under 100 lines — it only renders the modal wrapper, the tab bar, and the active tab's component.
- Each settings component manages its own local state (form fields, edit mode, validation). Shared state (like the list of providers needed by PersonaSettings for the model dropdown) is passed as props or fetched independently.
- CSS Modules stay co-located: `ProviderSettings.module.css`, `PersonaSettings.module.css`, etc.
- No behavior changes. The UI must work identically before and after this refactor.

**Acceptance criteria:**

- [ ] `SettingsModal.tsx` is under 100 lines
- [ ] Each extracted component is self-contained with its own CSS Module
- [ ] Provider CRUD works identically: add, edit, delete, test connection, Copilot auth flow
- [ ] Persona CRUD works identically: add, edit, delete, emoji picker, model selection
- [ ] `npm run build` passes with no errors or warnings
- [ ] Manual verification: open settings, test every action in both tabs

---

## Story C.2 — Resolve memory service cap strategy inconsistency

**Branch:** `fix/phase2-memory-cap-strategy`

There is a conflicting strategy for handling the 500-memory-per-persona cap:

1. **In `server/src/services/agent.rs`** (the agent run-loop's `execute_tool` function, around line 499): the `save_memory` handler checks `COUNT(*) >= 500` and returns `"Memory store is full"` to the model, **rejecting the save**.

2. **In `server/src/services/memory.rs`** (the `save_memory` function, around line 37): the same cap check exists, but instead of rejecting, it **deletes the oldest entry** to make room.

These two behaviors contradict each other. The PLAN.md spec (§7.6.9) says:

> When the limit is reached, `save_memory` returns a tool result telling the model the memory store is full, and suggests it use `recall_memory` to find and decide what's still relevant.

The spec intends **rejection at cap**, not silent eviction.

**What to do:**

1. **In `memory.rs`**: Remove the delete-oldest-to-make-room logic from `save_memory()`. Instead, if `count >= 500`, return an error (e.g., `Err(anyhow!("Memory cap reached"))`) so the caller can handle it.

2. **In `agent.rs`**: Keep the existing cap check and rejection message. After the fix to `memory.rs`, also handle the error case from `memory_service::save_memory()` gracefully (it should no longer silently succeed when at cap, but the agent handler already guards against this — just make sure both paths return a clear message to the model).

3. **Alternatively**, if you prefer the cleaner approach: remove the cap check from `agent.rs` entirely and let `memory.rs` be the single source of truth for the cap. Have `memory.rs` return a typed error when at cap, and have the agent's `execute_tool` match on that error to produce the "memory store is full" tool result. Either approach is fine — the key is that there is **one** place that enforces the cap, not two with different strategies.

**Acceptance criteria:**

- [ ] Only one cap enforcement strategy exists (reject, not evict)
- [ ] The cap is checked in exactly one place (either `agent.rs` or `memory.rs`, not both)
- [ ] When the cap is reached, the model receives: `"Memory store is full (500 entries). Cannot save new memory until some are deleted."`
- [ ] The delete-oldest logic is removed
- [ ] `cargo build` passes, all existing tests pass
- [ ] Add a unit test that verifies the cap rejection behavior

---

## Story C.3 — Remove duplicate SSE event parsing in useSseStore

**Branch:** `fix/phase2-sse-duplicate-parsing`

In `web/src/stores/useSseStore.ts`, the thread stream connection (inside the `connectThread` action's `open()` function) listens for events in two ways:

1. **Named event listeners** (lines ~190–193):
   ```ts
   es.addEventListener("token", handleToken);
   es.addEventListener("message_complete", handleMessageComplete);
   es.addEventListener("routine_message", handleRoutineMessage);
   es.addEventListener("error_event", handleError);
   ```

2. **A generic `"message"` fallback** (lines ~195–233) that re-parses the same event types:
   ```ts
   es.addEventListener("message", (e: MessageEvent) => {
     const data = JSON.parse(e.data);
     if (data.event === "token") ...
     else if (data.event === "message_complete") ...
     // etc.
   });
   ```

The same pattern exists in the global stream section (`connectGlobal`) with both a named `"thread_updated"` listener and a generic `"message"` fallback.

This was likely added during debugging when named SSE events weren't routing correctly. In practice, it means events can be processed twice — once by the named handler and once by the fallback. For `appendToken`, this would double every token. For `finalizeStream`, the duplicate detection prevents double-insertion, but it's still unnecessary work.

**What to do:**

1. Determine which approach actually works with the server's SSE implementation. Check `server/src/routes/sse.rs` — the server sets `Event::default().event(name).data(data)`. With Axum's SSE, when the `event:` field is set, the browser's `EventSource` dispatches to named listeners (e.g., `addEventListener("token", ...)`) rather than the generic `"message"` handler. This means the **named listeners are correct** and the generic fallback is dead code.

2. **Remove the generic `"message"` fallback** from both `connectThread` and `connectGlobal`.

3. **Fix the error event listener name.** The named listener uses `"error_event"` but the server emits `event: error`. Check if `addEventListener("error", ...)` conflicts with the EventSource's built-in error event (it does — the `onerror` handler fires for connection errors). If so, either:
   - Change the server to emit `event: stream_error` (or similar) and update the client listener to match, OR
   - Keep the `"error"` event name on the server and parse it inside the `onerror` handler by checking if the event has data (connection errors don't have data; server-sent error events do)
   
   Pick whichever is simpler. Document the choice in a code comment.

**Acceptance criteria:**

- [ ] Generic `"message"` fallback removed from both thread and global stream handlers
- [ ] Named event listeners are the sole event routing mechanism
- [ ] Error events are handled correctly (no name mismatch between server and client)
- [ ] `npm run build` passes with no errors
- [ ] Manual verification: send a message, observe tokens stream correctly (no doubling), observe thread list updates via global stream, observe error events display when provider is unavailable

---

## Story C.4 — Update README phase status table

**Branch:** `fix/phase2-readme-update`

The `README.md` at the repo root still shows Phase 1 as "🚧 In progress" and Phase 2 as "⏳ Pending". Both are now complete.

**What to do:**

Update the "Phased implementation" table in `README.md`:

```markdown
| Phase | Status | Description |
|---|---|---|
| 1 — Skeleton | ✅ Complete | Rust server, SQLite schema, React SPA shell, auth, all CRUD |
| 2 — First Chat | ✅ Complete | Agent run-loop, LLM streaming via SSE, chat UI |
| 3 — Config UI | ⏳ Pending | Setup wizard, provider/persona/thread management UI |
| 4 — Memory & Routines | ⏳ Pending | Persistent memory, cron scheduler |
| 5 — Mobile App | ⏳ Pending | React Native screens |
| 6 — Push Notifications | ⏳ Pending | FCM integration |
| 7 — MCP Depth | ⏳ Pending | Tool inspector, local process management |
| 8 — Polish | ⏳ Pending | Error handling, empty states, hardening |
```

Note two changes beyond status updates:
- Phase 7 description changed from "Experimental tool-call skills" to "Tool inspector, local process management" per PLAN.md v1.4 (skills were removed)
- The "Skills" mention in the Core features list at the top should be replaced with "MCP server integration" (skills are gone in v1.4; MCP is the capability layer now). The current README mentions both "Skills (experimental tool-call based capabilities)" and "MCP server integration" — remove the Skills line entirely.

**Acceptance criteria:**

- [ ] Phase 1 and 2 show ✅ Complete
- [ ] Phase 7 description reflects MCP Depth, not Skills
- [ ] "Skills (experimental tool-call based capabilities)" removed from Core features list
- [ ] No other content changes to README

---

## Story Order

Work these in order: **C.1 → C.2 → C.3 → C.4**

C.1 (SettingsModal decomposition) is the most important — it unblocks clean Phase 3 UI work. C.2 and C.3 fix correctness issues. C.4 is a quick documentation fix.

After all four are merged to `main`, Phase 3 can begin.

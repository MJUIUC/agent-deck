# Agent-Deck — Phase 3 Active Instructions

**For:** Implementing agent
**Repo:** https://github.com/MJUIUC/agent-deck
**Full history:** See `docs/deprecated/PHASE-3-INSTRUCTIONS.md` for all completed story specs and as-built notes.

---

## Phase 3 Progress

| Story | Status | Branch | Notes |
|---|---|---|---|
| 3.1 — Setup Wizard | ✅ Complete | `feature/phase3-setup-wizard` | Merged to main |
| 3.1a — Wizard Skip Flow & Empty State | ✅ Complete | `feature/phase3-setup-wizard` | Merged to main |
| 3.2 — Provider Settings Polish | ✅ Complete (no-op) | `feature/phase3-setup-wizard` | Existing UI exceeds mockup |
| 3.4a — Settings Nav Update | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | Merged to main |
| 3.4b — Settings: MCP Servers Tab | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | Merged to main |
| 3.4c — Settings: Mobile Tab | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | Merged to main |
| 3.4d — Settings: General Tab | ✅ Complete | `feature/phase3-settings-mcp-mobile-general` | Merged to main |
| 3.5 — Thread Config Pane | ✅ Complete | `feature/phase3-thread-config` | Memory section deferred. Model selector saves UUIDs. |
| 3.6 — Slash Command UI | ✅ Complete (server only) | `feature/phase3-slash-commands` | UI removed — belongs in CLI/mobile. Server endpoint intact. Vitest added. |
| 3.7 — Archived Threads | ✅ Complete | `feature/phase3-archived-threads` | Archive from config pane only. Archived view in Settings → Archived Threads (read-only). Unarchive/export deferred. |
| 3.8 — Pending Thread + Smart Title Gen | ✅ Complete | `feature/phase3-pending-thread` | Pending-thread pattern, LLM title gen, delete guard. See as-built notes below. |
| 3.x — Credential Store | 🔲 Not started | `feature/phase3-credential-store` | Part 2 |
| 3.3 Delta — Persona Default MCP Servers | 🔲 Not started | `feature/phase3-persona-mcp-defaults` | Part 2 |

---

## Known Bugs / Deferred Fixes

These issues were discovered during Story 3.8 implementation and must be addressed before Phase 3 is declared complete. They are not assigned to a story yet — work them on the branch of whichever story is active when they are picked up, or on a dedicated fix branch if they surface independently.

### Bug 3.B1 — Persona default model not set during wizard setup

**Discovered:** Story 3.8 session (2026-03-14)
**Symptom:** After completing the setup wizard, the created persona (e.g. Aldous) has `default_model: null`. When the user starts a chat, the agent run-loop hits the "No provider or model configured" error path and returns an error message to the chat window instead of a real response.

**Root cause (three layers):**

1. **Wizard Step 4 — no auto-selection.** The model dropdown in `Step4Persona.tsx` defaults to `""` (none selected). There is no indication it is important, and nothing prevents the user from clicking "Finish Setup" without choosing a model. The persona is then created with `default_model: null`.

2. **`createThread` does not forward persona defaults.** In `useThreadStore.ts`, `createThread` calls `POST /api/threads` with only `{ persona_id }`. The server creates the thread with `active_model: null` and `active_provider: null`. The agent run-loop tries to fall back to `persona.default_model` — but if that is also null, the error fires.

3. **Existing data:** Any persona created before this fix (including the Aldous record in the current running DB) has `default_model: null` already. That data must be patched manually via Settings → Personas → Aldous → set a default model, or via a targeted `PUT /api/personas/:id` call.

**Fix plan:**

**Fix A — Wizard auto-selects a sensible default model (Step4Persona.tsx)**

When the `models` prop arrives and `selectedModel` is still `""`, auto-select based on provider kind:
- `copilot` → first model whose `model_id` contains `"claude"`, else `models[0]`
- `openai` → first model whose `model_id` contains `"gpt-4o"`, else `models[0]`
- `anthropic` → first model whose `model_id` contains `"claude-3-5-sonnet"`, else `models[0]`
- `custom` → `models[0]`

Wire this with a `useEffect` that watches `[models]` and fires only when `selectedModel === ""`:

```
useEffect(() => {
  if (models.length === 0 || selectedModel !== "") return;
  const preferred = pickDefaultModel(models, providerKind);
  if (preferred) setSelectedModel(preferred.id);
}, [models]);
```

`Step4Persona` needs to receive the `providerKind` as a prop from `SetupWizard`. `SetupWizard` already holds `providerDraft.kind`.

**Fix B — `createThread` forwards persona defaults (useThreadStore.ts)**

```typescript
const res = await threadsApi.create({
  persona_id: personaId,
  active_provider: persona?.default_provider ?? undefined,
  active_model: persona?.default_model ?? undefined,
});
```

This ensures every new thread starts with the persona's configured provider and model baked in, so the agent run-loop never needs to fall back to `persona.default_model` through the resolution chain.

**Fix C — Patch existing data**

For the current running instance: Settings → Personas → Aldous → set default model. No migration needed — this is a one-time data correction.

**Acceptance criteria for this bug fix:**
- [ ] Completing the wizard with Copilot selects a Claude model by default without user action
- [ ] A new persona has `default_model` set after wizard completion (confirmed via `GET /api/personas`)
- [ ] A new thread created from a persona inherits `active_provider` and `active_model` from the persona
- [ ] Agent responds correctly on the first message of a brand-new chat
- [ ] Existing personas with `default_model: null` are unaffected by the code change (data fix is manual)

---

## Story 3.8 — As-built Notes

- `build_provider` in `agent.rs` made `pub(crate)` so `threads.rs` can call it for the title endpoint.
- `verify_thread_ownership` in `threads.rs` updated to return the full `Thread` struct (was returning `String`). All three MCP call sites updated with `let _thread = ...`.
- Old title-gen block removed from `messages::send()` entirely.
- `DELETE /api/threads/:id` now verifies ownership first (404 on unknown), then returns `400` if any messages exist.
- `try_llm_title` resolves provider/model UUID chain: `thread.active_provider` → `persona.default_provider`; `thread.active_model` (UUID) → `models.model_id` string → `persona.default_model`. Falls back to `generate_title_from_message()` at every failure point.
- `makeDraftThread()` exported from `useThreadStore.ts` — constructs a synthetic `Thread` with `id: "pending"` from an `AgentPersona`.
- `promotePendingThread` sets `activeThreadId` + clears `pendingPersona`; does NOT push to `threads` — `createThread` already did that.
- Title-gen in `ChatView` uses a `prevIsStreaming` ref to detect the `true → false` falling edge of `isStreaming`, gated by `titleGenPendingRef`. Fires once per promoted draft.
- `ChatHeader.onToggleConfig` made optional; button disabled with tooltip in draft mode.
- `modelName` prop and hints-row span removed from `MessageInput` entirely.
- Navigating away from a draft calls `setPendingPersona(null)` — no DB record, no cleanup needed.

---

## Part 2 — Remaining Stories

Work stories in order. Do not start 3.3 Delta until 3.x is complete and merged.

---

### Credential Store Design — READ THIS FIRST

Before implementing Story 3.x, understand the full credential ownership and resolution model.

#### Three Ownership Levels

Every credential has an `owner_type` that determines who it belongs to:

**`system`** — Shared infrastructure credentials. A weather API key, a news API key. Any persona, any thread can use these. Configured once in global MCP server settings.

**`user`** — The human user's personal accounts. Your Gmail, your Google Calendar, your personal GitHub. When the agent uses these, it is acting *on your behalf*. Available to any persona.

**`persona`** — Accounts that belong to a specific agent persona. A coding persona's own GitHub account. When the agent uses these, it is acting *as itself*. Scoped to a specific `persona_id`.

#### Credential Binding Happens at Configuration Time

The agent never chooses which credential to use at runtime. The credential is bound when an MCP server is attached to a thread via a `credential_key` in its config.

#### Tool Namespacing by Server Name

When multiple MCP servers expose tools with the same base names, tools are prefixed with the sanitized server name:

```
marcus_gmail/read_inbox      (user's Gmail)
nexus_gmail/read_inbox       (persona's Gmail)
weather_api/get_forecast     (system service)
```

**Note:** Full namespacing implementation is Phase 4/7 work. Document the convention now in a comment in `context.rs`.

#### Agent Awareness via System Prompt

The context assembler will inject a section describing available tools with ownership labels. **Note:** Full injection is Phase 4/7 work. Document the convention now in a comment in `context.rs` alongside the namespacing comment.

#### Credential Resolution at Runtime (Phase 4+)

1. Look up `credential_key` from the MCP server config
2. Find the matching `credentials` row
3. Decrypt `encrypted_data` using the master key
4. If OAuth and expired, refresh and re-encrypt (Phase 4)
5. Inject resolved token into the MCP server auth header — held in memory only, never logged or cached

---

### Story 3.x — Credential Store and Encryption

**Branch:** `feature/phase3-credential-store`

#### Background

See PLAN.md §6.5. The credential store holds encrypted API keys and OAuth tokens. Encryption uses AES-256-GCM. A master key is generated on first run and stored in `app_config`.

#### What to Build

**Master key management:**
- On server startup, check `app_config` for key `credential_master_key`
- If not present, generate a random 256-bit (32-byte) key, encode as hex, and store it
- Load the master key into `AppState`
- The master key must **never** appear in log output or API responses

**Encryption helpers** (extend `server/src/services/encryption.rs`):
- `encrypt_credential(plaintext: &str, master_key: &str) -> Result<String>` — AES-256-GCM, return as base64-encoded `nonce:ciphertext`
- `decrypt_credential(encrypted: &str, master_key: &str) -> Result<String>` — reverse

**Credential model:**

```rust
pub struct Credential {
    pub id: String,
    pub key: String,                // unique lookup key, e.g. "google_oauth_marcus"
    pub display_name: String,       // "Marcus's Google Account"
    pub provider: String,           // "google", "github", "openai", "custom"
    pub credential_type: String,    // "oauth2", "api_key", "custom"
    pub owner_type: String,         // "system" | "user" | "persona"
    pub persona_id: Option<String>, // set only when owner_type = "persona"
    // NOTE: encrypted_data is NEVER included in this public struct
    pub scopes: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

**Schema check:** Verify the `owner_type` CHECK constraint in the database includes `'system'`. If the migration only has `CHECK (owner_type IN ('user', 'persona'))`, create migration 003 that recreates the table with the correct constraint (SQLite does not support ALTER CONSTRAINT):

```sql
-- Migration 003: Add 'system' to owner_type constraint on credentials
CREATE TABLE credentials_new (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  provider TEXT NOT NULL,
  credential_type TEXT NOT NULL,
  owner_type TEXT NOT NULL CHECK (owner_type IN ('system', 'user', 'persona')),
  persona_id TEXT REFERENCES agent_personas(id) ON DELETE CASCADE,
  encrypted_data TEXT NOT NULL,
  scopes TEXT,
  expires_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
INSERT INTO credentials_new SELECT * FROM credentials;
DROP TABLE credentials;
ALTER TABLE credentials_new RENAME TO credentials;
```

After any schema change: run `cargo sqlx prepare` and commit `.sqlx/`.

**Credential CRUD endpoints:**
- `POST /api/credentials/api-key` — store an API key credential (encrypts the secret). Body: `key`, `display_name`, `provider`, `credential_type`, `owner_type`, `persona_id` (required when `owner_type = "persona"`), `secret`. Validate ownership rules.
- `GET /api/credentials` — list all credentials, **metadata only** (never returns `encrypted_data`). Accept optional query params: `?owner_type=system`, `?persona_id=<id>`
- `DELETE /api/credentials/:id` — delete a credential

**Hard security rule:** `encrypted_data` must **never** appear in any API response, log line, or error message. The public `Credential` struct excludes it. A separate internal `CredentialWithData` struct is used only when decrypting server-side.

**Settings UI — Credentials tab:**

Add a "Credentials" tab to `SettingsModal`. Contents:

- Credentials listed grouped by ownership tier: "System Credentials", "Your Credentials", per-persona sections
- Each entry shows: display name, provider, credential type, owner badge
- "Add API Key" button → form: display name, key (unique identifier), provider, owner type selector (system/user/persona — persona shows a persona picker), secret field (password input, write-only)
- Delete with confirmation dialog
- Secret field shows "••••••••" after save — no reveal option

**Document design conventions:**

Add a comment in `server/src/services/context.rs` at the MCP tool stub documenting:
1. Tool namespacing convention (server name prefix)
2. System prompt injection convention (ownership labels)
3. Reference: Phase 3 PLAN.md §6.5 and credential store design above

#### Integration Tests

- Store a system credential → retrieve metadata → confirm no `encrypted_data` in response → decrypt server-side → confirm round-trip
- Store user + persona credentials → `GET /api/credentials?owner_type=user` → confirm only user credential returned
- Store credential with `owner_type=persona` and no `persona_id` → confirm 400
- Delete a credential → confirm 404 on subsequent fetch

#### Acceptance Criteria

- [ ] Master key generated on first run and persists across restarts
- [ ] Master key never appears in logs or API responses
- [ ] `owner_type` supports all three values: `system`, `user`, `persona`
- [ ] `POST /api/credentials/api-key` stores encrypted credential with correct `owner_type`
- [ ] Validation: `persona_id` required when `owner_type = "persona"`, rejected otherwise
- [ ] `GET /api/credentials` returns metadata only — no `encrypted_data`
- [ ] `GET /api/credentials?owner_type=system` filters correctly
- [ ] `DELETE /api/credentials/:id` works
- [ ] Settings UI shows credentials grouped by ownership
- [ ] "Add API Key" form enforces ownership rules
- [ ] Integration tests pass for all ownership scenarios
- [ ] Design conventions documented in `context.rs`
- [ ] `cargo sqlx prepare` run and `.sqlx/` committed after any schema change
- [ ] `cargo build` passes, `npm run build` passes, all tests pass

---

### Story 3.3 Delta — Persona Default MCP Servers

**Branch:** `feature/phase3-persona-mcp-defaults`

**Depends on:** Story 3.x complete and merged to main.

#### What Already Exists

`PersonaSettings.tsx` has full CRUD: list, add, edit, delete, emoji picker, model selection.

The auto-attach logic on thread creation already exists from the alignment phase — `POST /api/threads` queries `persona_default_mcp_servers` and inserts into `thread_mcp_servers` for each row. Verify it still works before building the UI.

#### What to Build

Add a **Default MCP Servers** section to the persona edit view in `PersonaSettings.tsx`:

- Shows MCP servers that will be auto-attached to every new thread created with this persona
- Same visual style as the MCP list in the thread config pane (Story 3.5)
- "+ Add default server" opens a picker showing all configured MCP servers not already in the list
- Label: "These servers are attached automatically when a new thread is created with this persona."
- Adding: `POST /api/personas/:id/default-mcp-servers` (or direct insert if endpoint exists from alignment)
- Removing: `DELETE /api/personas/:id/default-mcp-servers/:mcp_id`
- Check whether these endpoints exist from the alignment phase before adding new ones

**Note:** The persona "Accounts" tab (for persona-owned OAuth connections) is **deferred to Phase 4**. Do not add it in this story.

#### Acceptance Criteria

- [ ] Default MCP Servers section appears in persona edit view
- [ ] Can add and remove default MCP servers for a persona
- [ ] Creating a new thread with a persona auto-attaches its default servers (verify existing behaviour still works)
- [ ] Visual style matches the MCP section in the thread config pane
- [ ] `npm run build` passes, `cargo build` passes, all tests pass

---

## Phase 3 Completion Checklist

Before declaring Phase 3 done, verify the complete flow end-to-end on a fresh database:

1. [ ] Fresh DB → setup wizard completes — persona has `default_model` and `default_provider` set
2. [ ] Skipping provider in wizard skips persona step and goes straight to Step 5
3. [ ] Empty state routes correctly based on provider/persona state (all four cases)
4. [ ] Settings: all tabs load — Providers, Personas, MCP Servers, Credentials, Mobile, General
5. [ ] Provider CRUD: Copilot auth flow, OpenAI with API key, custom endpoint
6. [ ] Persona CRUD: default MCP servers section works
7. [ ] MCP server CRUD: local and remote types
8. [ ] Credential store: API keys stored encrypted, never exposed in API
9. [ ] Credentials respect three ownership levels (system, user, persona)
10. [ ] Thread config pane: model switch, MCP attach/detach, tool activity toggle, addendum save
11. [ ] New chat: clicking "+ New Chat" does not immediately create a DB record
12. [ ] First message creates thread, agent responds, title updates after first response
13. [ ] Archived threads: archive from config pane, view in Settings → Archived Threads
14. [ ] Token rotation in General settings works with warning
15. [ ] QR pairing code displays correctly
16. [ ] No `curl` needed for any management task — everything works through the UI

---

## Key Rules (Carried Forward)

1. **Read before you write.** Find the full path of any file before reading or editing it.
2. **One story per branch.** Never combine stories.
3. **Never expose `encrypted_data` in API responses.** Hard security rule.
4. **Hidden messages stay hidden.** Default filter on `GET /api/threads/:id/messages`.
5. **SQLx offline mode.** After any schema change, `cargo sqlx prepare` must be run and `.sqlx/` committed.
6. **Plan before you fix.** Diagnose and write a plan before writing any fix code. Wait for human confirmation.
7. **Never simplify code to fix diagnostics.** Complete, correct code over minimal code.

---

## What Comes Next (Phase 4)

After Phase 3 is complete, proceed to **Phase 4 — Memory, Routines, and OAuth**:

1. **Persistent memory** — memory viewer UI, `/memory` slash command display, memory management settings page (the `save_memory` and `recall_memory` tools are already wired into the agent run-loop)
2. **Routines** — cron-scheduled prompts with two-phase execution (silent background → visible synthesized output)
3. **OAuth framework** — `OAuthProvider` trait, Google and GitHub implementations, Accounts settings tab, persona-owned account connections

The credential store's three-tier ownership model and tool namespacing conventions from Phase 3 are prerequisites for Phase 4's MCP credential resolution and routine execution.
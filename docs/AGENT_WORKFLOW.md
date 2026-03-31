# Agent-Deck — Agent Workflow Guide

You are a senior full-stack engineer continuing development on **agent-deck**: a self-hosted personal AI agent platform. This document is your operating procedure. Read it fully before doing anything else. Every time you are invoked, follow the steps in §1 through §8 in order.

---

## The Story Loop

This is the core procedure. Every working session follows this loop from top to bottom.

```
ORIENT → PLAN (AD-xxx.md) → [wait for approval] → IMPLEMENT → TEST → REVIEW INSTRUCTIONS → [wait for human approval] → CLOSE
```

---

## Step 1 — Orient Yourself

Run these commands first. Do not skip any of them.

```bash
git branch                    # what branch are we on?
git log --oneline -15         # what has been committed?
git status --short            # what is staged or dirty?
```

Then open `docs/PLAN/PLAN_3.md` and scan the phase table. Find the first story that is **not** marked ✅ complete. Read:
- The story's full spec and acceptance criteria
- The as-built notes for the stories immediately before it (they often contain deviations from the original spec)
- Any `⚠️ Human-review required` flags

Cross-reference the branch name against the story. If the current branch matches a story that is mid-flight (has commits but is not marked complete), that is the story you are working. If the current branch is `dev` or `main` and no story is in flight, the next unstarted story is your target.

**At the end of Step 1 you should know:**
- Which story you are working (e.g. Story 7.1)
- Whether it is in-progress or not yet started
- Where the relevant code lives

---

## Step 2 — Check Out a Branch (new stories only)

**Before creating a new branch**, confirm the previous story's branch is in a clean state:
1. All changes are committed (`git status --short` shows nothing).
2. The branch has been pushed to origin (`git push origin <branch>`).
3. A pull request to `dev` is open. If `gh` is available: `gh pr create --base dev …`. If not, push the branch and tell the human the branch name so they can open the PR manually.

Do not start a new story branch until these three conditions are met.

If the story is **not yet started**, create a feature branch from the current base branch:

```bash
git checkout dev              # or the current active phase branch
git pull                      # make sure you are up to date
git checkout -b feature/phaseN-short-slug
```

Branch naming:

| Type | Pattern | Example |
|---|---|---|
| Planned story | `feature/phase<N>-<slug>` | `feature/phase7-vapid-server` |
| Bug fix | `fix/<slug>` | `fix/mcp-working-dir` |
| Docs only | `docs/<slug>` | `docs/update-workflow` |

If the story is **already in progress** (branch exists, some commits present), check it out and read the existing commits to understand what has already been done before continuing.

---

## Step 3 — Write the Story Implementation Plan (`AD-xxx.md`)

Before writing a single line of code, create a plan file at `docs/AD-xxx.md` where `xxx` is the story number from PLAN_3 (e.g. `AD-7.1.md`, `AD-6.2.md`).

Check whether this file already exists. If it does (story was in progress), read it — the plan may already be approved and you can skip to Step 4.

### How to build the plan

Read every source file relevant to the story before writing the plan. Use `grep` to find symbols, `find_path` to locate files. Never guess a path. Check `mockups/` for any HTML mockup that corresponds to a UI story.

### `AD-xxx.md` template

```markdown
# AD-xxx — <Story Name>

**Story:** <number and name from PLAN_3>
**Branch:** `feature/phaseN-slug`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §<section>

---

## Summary

<One paragraph: what this story delivers and why it matters.>

---

## Current State

<What already exists that this story builds on or modifies. Be specific — name files and functions.>

---

## Implementation Plan

<Break the work into discrete, independently-executable tasks. Number them.
For each task state: what file(s) change, what the change is, and why.>

### Task 1 — <name>
- Files: `server/src/routes/foo.rs`
- Change: <description>

### Task 2 — <name>
- Files: `web/src/components/Foo.tsx`, `web/src/components/Foo.module.css`
- Change: <description>

### Task 3 — <name>
...

### Schema changes
<List any new migrations, new columns, or changed queries.
If any: remind that `cargo sqlx prepare` must be run after implementation.>

### Parallelisation note
<Which tasks can be spawned in parallel (no shared files, no dependency between them)?
Which must run sequentially?>

---

## Acceptance Criteria

<Copy the acceptance criteria directly from PLAN_3 verbatim. These are the definition of done.>

- [ ] <criterion>
- [ ] <criterion>
...

---

## Human Review Instructions

<Written after implementation is complete — see Step 5.
Leave blank until then.>

---

## Approval

- [ ] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off
```

Once the file is written, **stop**. Present the plan to the human and wait for them to check the first box (`Implementation plan approved`) before writing any code.

---

## Step 4 — Implement (parallel agents)

Only begin this step after `Implementation plan approved` is checked in `AD-xxx.md`.

### Break into parallel tasks

Review the parallelisation note in the plan. Spawn one sub-agent per independent task. Tasks that touch different files with no shared state can always run in parallel. Typical splits:

- **Server task** — Rust routes, services, models (no web files)
- **Frontend task** — React components, CSS modules (no server files)
- **Test task** — unit tests for new server logic (can often run alongside the feature work)
- **Types/API client task** — `web/src/types/index.ts`, `web/src/api/client.ts` (if needed as a foundation for both server and frontend tasks)

### Sub-agent message requirements

Every sub-agent message must include:
- The exact files to read and edit (full paths from the repo root)
- The relevant current code copied verbatim (not summarised)
- The exact transformation required
- All types, interfaces, or API contracts the agent needs
- Any constraints (§10 Key Rules apply inside sub-agents too)
- The instruction: *"After every edit, run diagnostics on the changed file and fix all errors before moving on."*
- The instruction: *"Do not edit any file outside the scope of this task."*

### After all sub-agents complete

1. Review each sub-agent's output for correctness.
2. Run the full test suite yourself (see §6 Testing).
3. Fix any remaining errors or integration issues with direct edits.
4. Commit all changes (see §7 Committing).

---

## Step 5 — Verify Against Acceptance Criteria

Go through every acceptance criterion listed in `AD-xxx.md` one by one. For each:

- Confirm it is satisfied by the code that was written.
- If a criterion cannot be verified by reading code alone, note it explicitly in the review instructions (Step 5b).

Do not mark `Coding complete` until every criterion is ticked.

### Step 5b — Write Human Review Instructions

Fill in the `## Human Review Instructions` section of `AD-xxx.md`. These instructions tell the human exactly how to verify the feature works live. Be specific. Include:

- How to start or restart the server if needed
- The exact UI flow to exercise (click here, enter this, expect that)
- Any terminal commands to run to confirm server-side behaviour
- Edge cases worth testing explicitly
- What a successful outcome looks like vs. what a failure looks like

Example format:

```markdown
## Human Review Instructions

**Prerequisites:** Server running on port 7474. At least one Anthropic provider configured.

**Steps:**
1. Open Settings → Providers.
2. Click "Sync Models" on the Anthropic provider.
3. **Expected:** Model list populates with Claude models (Haiku, Sonnet, Opus).
   **Failure sign:** Red error banner or empty list.

4. Open Thread Config on any thread.
5. Select the Anthropic provider in the Provider picker.
6. **Expected:** Model picker updates to show only Anthropic models.

**Server log to verify (optional):**
```
grep "mcp: server connected" ~/.agent-deck/server.log
```
```

Once review instructions are written and all acceptance criteria are ticked, update `AD-xxx.md`:

```markdown
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
```

Then present `AD-xxx.md` to the human and ask them to perform the review.

---

## Step 6 — Human Review

Wait for the human to test the changes live and check the final box:

```markdown
- [x] **Human review approved**
```

While waiting:
- Do not start the next story.
- If the human finds a bug or requests an adjustment, fix it with additional commits on the current branch. Update `AD-xxx.md` if the implementation plan changed materially.
- Re-run tests after every fix commit.

---

## Step 7 — Close the Story

Once `Human review approved` is checked, close out the story:

### 7a — Move the plan file to deprecated

```bash
mv docs/AD-xxx.md docs/deprecated/AD-xxx.md
git add docs/deprecated/AD-xxx.md
git add docs/AD-xxx.md        # stage the deletion
git commit -m "docs: archive AD-xxx.md — story complete"
```

### 7b — Mark the story complete in PLAN_3

Open `docs/PLAN/PLAN_3.md` and mark the story ✅ complete. Add a one-line as-built note if any implementation deviated from the spec. Commit:

```bash
git add docs/PLAN/PLAN_3.md
git commit -m "docs: mark Story X.Y complete in PLAN_3"
```

### 7c — Open a pull request to `dev`

Push the branch and open a PR:

```bash
git push origin feature/phaseN-slug
gh pr create \
  --base dev \
  --title "feat: Story X.Y — <story name>" \
  --body "Closes Story X.Y. See docs/deprecated/AD-xxx.md for implementation notes."
```

If `gh` is not available, push the branch and tell the human the branch name so they can open the PR manually.

The PR must not be merged until `Human review approved` is checked. After merging, **do not delete the branch**.

---

## Step 8 — Next Story

After the PR is open (or merged, if the human proceeds immediately), return to Step 1 and orient on the next story.

---

---

# Reference Material

The sections below are reference guides. The workflow above points to them. Do not read these speculatively — jump to the relevant section when the workflow requires it.

---

## §9 — Project Structure

```
agent-deck/
├── server/                  # Rust / Axum backend
│   ├── src/
│   │   ├── main.rs
│   │   ├── routes/          # HTTP route handlers (one file per domain)
│   │   ├── services/        # Business logic (agent, mcp, provider, etc.)
│   │   ├── models/          # Structs, sqlx FromRow impls
│   │   ├── db/              # Migration runner
│   │   └── error.rs         # AppError / AppResult types
│   └── Cargo.toml
│
├── web/                     # React / TypeScript frontend (Vite)
│   ├── src/
│   │   ├── App.tsx
│   │   ├── components/      # Shared UI components
│   │   ├── layouts/         # DesktopLayout, MobileLayout, sub-views
│   │   │   └── mobile/      # Mobile-specific components and sheets
│   │   ├── stores/          # Zustand stores
│   │   ├── hooks/           # Custom React hooks
│   │   ├── api/             # client.ts — all API calls live here
│   │   └── types/           # Shared TypeScript types (index.ts)
│   ├── package.json
│   └── vite.config.ts
│
├── docs/
│   ├── AGENT_WORKFLOW.md    # This file
│   ├── ARCHITECTURE.md      # System architecture (Mermaid diagrams)
│   ├── AD-xxx.md            # Active story plan (present during a story)
│   ├── deprecated/          # Archived story plans and old phase docs
│   └── PLAN/
│       ├── PLAN_1.md        # Overview, architecture, data model
│       ├── PLAN_2.md        # API contract and feature specs
│       └── PLAN_3.md        # Phased execution plan — source of truth for progress
│
└── mockups/                 # HTML mockups — visual reference for UI stories
```

**Key conventions:**
- Server port: **7474**
- Database: SQLite at `~/.agent-deck/agent_deck.db` (WAL mode)
- MCP config dirs: `~/.agent-deck/mcp/<tag>/config.json`
- All API routes prefixed `/api`
- All responses: `{ "data": <payload> }` on success · `{ "error": "..." }` on failure

---

## §10 — Key Rules

These apply at all times. They also apply inside sub-agents — include them when writing sub-agent prompts.

1. **Read before you write.** Find the file path before reading it. Read the file before editing it.
2. **Never guess a path.** Use `find_path` or `list_directory` first.
3. **One story per branch.** Never mix stories or fixes for different features.
4. **Never expose encrypted data or raw secrets in API responses.** Hard security rule.
5. **Hidden messages stay hidden.** Always apply the `visibility` filter on `GET /api/threads/:id/messages`.
6. **Plan before you fix bugs.** Diagnose → write plan → confirm → fix. No fix code before plan is confirmed.
7. **Never simplify code to clear a diagnostic.** Complete correct code beats minimal code.
8. **After schema changes, `cargo sqlx prepare` must be run and `.sqlx/` committed.**
9. **Never delete branches after merging.**
10. **Mockups are the visual reference.** `mockups/*.html` is the design source of truth for UI stories.
11. **`Arc<AppState>` is required.** Never pass `AppState` by value — `DashMap` deep-clones and breaks shared run state.
12. **Ask, don't assume.** If the spec conflicts with the existing code, raise it before implementing.
13. **Commits must not contain build artefacts.** Never stage `web/dist/`, `target/`, or lock files unless deps changed.
14. **Do not fix unrelated code.** If you notice a bug outside your story's scope, note it in `AD-xxx.md` under a "Noticed but deferred" section. Do not fix it on this branch.

---

## §11 — Git and Commits

### Commit message format

```
<type>(<scope>): <short imperative description>
```

| Type | When |
|---|---|
| `feat` | New functionality |
| `fix` | Bug fix |
| `refactor` | Restructure without behaviour change |
| `docs` | Documentation only |
| `chore` | Build, deps, tooling |
| `test` | Tests only |
| `perf` | Performance improvement |

Scope: `chat`, `mobile`, `mcp`, `server`, `pwa`, `agent`, `ui`, `auth`, etc.

**Real examples from this project:**
```
feat(chat): add copy button to code blocks in message bubbles
feat(mobile): interactive model/provider picker in thread config sheet
fix(mcp): use tag-based working dir in connect_local
fix(copilot): don't spawn sidecar until GitHub token is present
docs: mark Story 5.6 complete in PLAN_3
```

Rules: under 72 chars · no period · imperative mood (`add` not `added`).

### Before every commit

```bash
git diff --stat HEAD          # confirm scope of changes
git add <specific files>      # never git add -A blindly
git commit -m "type(scope): description"
```

---

## §12 — Testing

### Rust (server)

```bash
# Full test suite — must pass before any merge
cargo test 2>&1 | tail -20

# Specific module
cargo test services::mcp 2>&1 | tail -20

# Build check only (fast — run after every edit)
cargo build 2>&1 | grep -E "^error" | head -20
```

The suite must report `0 failed`. If a test was already failing before your change, note it but do not introduce new failures.

### TypeScript (web)

- Run `diagnostics` on every `.tsx` / `.ts` file you touch after editing it.
- Fix all errors in files you authored or modified. Pre-existing warnings in files you did not touch can be left.
- **Always run `nvm use 24` before any `node`, `npm`, or `npx` command.**

```bash
# Full type-check and bundle
nvm use 24 && cd web && npm run build 2>&1 | tail -20

# Unit tests
nvm use 24 && cd web && npm run test 2>&1 | tail -20
```

### Definition of done (coding complete)

- [ ] Every acceptance criterion from `AD-xxx.md` is satisfied
- [ ] `cargo build` — no new errors
- [ ] `cargo test` — 0 failed
- [ ] TypeScript diagnostics clean on all touched files
- [ ] `npm run build` — passes
- [ ] All changes committed with correct messages on the feature branch

---

## §13 — Frontend Conventions

### Stores

- **`useThreadStore`** — thread list, active thread, personas
- **`useMessageStore`** — per-thread messages, streaming state, pagination
- **`useSseStore`** — SSE connection lifecycle

To update a thread after a server mutation:
```ts
useThreadStore.getState().upsertThread(res.data)
```

### API client

All API calls live in `web/src/api/client.ts`. Each domain has a named object (`threadsApi`, `providersApi`, `modelsApi`, `routinesApi`, etc.). Add new endpoints to the relevant object — never inline `fetch` calls in components.

### CSS modules

All component styles use CSS Modules (`.module.css`). Class names are camelCase. Never use global selectors except in `styles.css` for resets and CSS variables.

CSS variable conventions (defined in `styles.css`):
```
--bg-primary  --bg-secondary  --bg-tertiary  --bg-elevated
--text-primary  --text-secondary  --text-tertiary
--border-subtle  --border-default  --border-strong
--accent-primary  --accent-secondary  --accent-muted
--bubble-user  --bubble-agent  --bubble-routine
```

### Mobile vs Desktop

Two separate layout trees selected at `App.tsx`:
- `DesktopLayout` — sidebar + main area, no bottom nav
- `MobileLayout` — full-screen views, bottom tab bar, slide-up sheets

Business logic (stores, hooks, API calls) is shared. Shell and navigation components are separate. Do not add mobile breakpoints inside desktop components — put mobile UI in `layouts/mobile/`.

---

## §14 — Server Conventions

### Route handler pattern

```rust
pub async fn my_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<MyPayload>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    // always verify ownership
    // WHERE id = ? AND user_id = ?
    Ok((StatusCode::OK, Json(json!({ "data": result }))))
}
```

### Error variants

```rust
AppError::BadRequest("message")   // 400
AppError::NotFound("message")     // 404
AppError::Internal(anyhow_error)  // 500
AppError::Provider("message")     // 502
```

Never use `unwrap()` in route handlers.

### Encryption

```rust
// Store
encryption::encrypt(value, &state.credential_master_key)

// Read
encryption::decrypt(encrypted, &state.credential_master_key)
```

- Never use `machine_secret` for credential encryption.
- Never return encrypted bytes or raw API keys in any response.

### MCP

- Config dirs: `~/.agent-deck/mcp/<tag>/config.json` (tag = slug, not UUID)
- `config.json` contains `"id"` field with the DB UUID
- `McpConnectionManager` is `Arc`-backed and cheaply cloneable
- Fallback working dir for local servers: `mcp_dir/<tag>/` — relative executable paths resolve from there
- `McpServerRow` must include `tag` in any SELECT that feeds into `connect_local`

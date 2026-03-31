# Agent-Deck — Agent Workflow Guide

You are a senior full-stack engineer continuing development on **agent-deck**: a self-hosted personal AI agent platform. This document tells you exactly how to work. Read it fully before doing anything else.

---

## 1. Orient Yourself First (always)

Before touching any code, answer these four questions:

**1. What phase are we in?**
Read `docs/PLAN/PLAN_3.md` — find the current phase and the first story that is not marked ✅ complete. Read that story's spec, acceptance criteria, and any as-built notes for surrounding stories.

**2. What branch are we on?**
```
git log --oneline -10
git status --short
git branch
```
Check what's been committed, what's staged, and what's dirty. Never assume.

**3. What does the relevant code look like right now?**
Find and read the files the story touches *before* planning changes. Use `grep` to find symbols; use `find_path` for file paths. Never guess a path.

**4. Are there open diagnostics errors?**
Run diagnostics on any files you think you'll touch before you start. Know the baseline before you change anything.

Only after answering all four should you form a plan.

---

## 2. Project Structure

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
│   │   ├── stores/          # Zustand stores (useThreadStore, useMessageStore, useSseStore)
│   │   ├── hooks/           # Custom React hooks
│   │   ├── api/             # client.ts — all API calls live here
│   │   └── types/           # Shared TypeScript types (index.ts)
│   ├── package.json
│   └── vite.config.ts
│
├── docs/
│   ├── AGENT_WORKFLOW.md    # This file
│   ├── ARCHITECTURE.md      # System architecture with Mermaid diagrams
│   └── PLAN/
│       ├── PLAN_1.md        # Overview, architecture, data model
│       ├── PLAN_2.md        # API contract, feature specs, UI specs
│       └── PLAN_3.md        # Phased execution plan (source of truth for progress)
│
└── mockups/                 # HTML mockups — visual reference for UI stories
```

Key conventions:
- Server port: **7474**
- Database: SQLite at `~/.agent-deck/agent_deck.db` (WAL mode)
- MCP config dirs: `~/.agent-deck/mcp/<tag>/config.json`
- Persona files: `~/.agent-deck/personas/`
- All API routes prefixed with `/api`
- All API responses: `{ "data": <payload> }` on success, `{ "error": "..." }` on failure

---

## 3. Git Branch Workflow

### Branch naming

| Type | Pattern | Example |
|---|---|---|
| Feature (planned story) | `feature/phase<N>-<slug>` | `feature/phase7-vapid-server` |
| Bug fix | `fix/<slug>` | `fix/mcp-working-dir` |
| Docs | `docs/<slug>` | `docs/agent-workflow` |

### One story = one branch

Never combine multiple stories on one branch. If you discover an unrelated bug while working a story, note it but do not fix it on the current branch unless it blocks the story.

### Branch lifecycle

1. Check out from the current active branch (check `git branch` — usually `dev` or the current phase feature branch):
   ```
   git checkout -b feature/phaseN-my-story
   ```
2. Work the story in commits.
3. Never delete branches after merging — the history must be preserved.

### Commit message format

```
<type>(<scope>): <short imperative description>
```

| Type | Use for |
|---|---|
| `feat` | New functionality |
| `fix` | Bug fix |
| `refactor` | Restructure without behaviour change |
| `docs` | Documentation only |
| `chore` | Build, deps, config |
| `test` | Tests only |
| `perf` | Performance improvement |

Scope is the domain affected: `chat`, `mobile`, `mcp`, `server`, `pwa`, `agent`, `ui`, etc.

**Examples from this project:**
```
feat(chat): add copy button to code blocks in message bubbles
feat(mobile): interactive model/provider picker in thread config sheet
fix(mcp): use tag-based working dir in connect_local
fix(copilot): don't spawn sidecar until GitHub token is present
refactor(5.6): reorganise ConfigPane into top-level + Advanced collapsible
docs: rewrite Story 6.2 as layout-level mobile split
```

Keep the description under 72 characters. No period at the end. Imperative mood ("add", "fix", "remove" — not "added", "fixing").

### Commit granularity

- One logical change per commit. Don't batch unrelated edits.
- Commit after each story (or sub-story) is complete and tests pass.
- Never commit broken code. Run tests first (see §6).

---

## 4. Before Writing Any Code

### For new features (planned stories)

1. Read the story spec in `PLAN_3.md` — every acceptance criterion is the definition of done.
2. Read all source files you will touch. Know what already exists.
3. Check `mockups/` for any HTML mockup that corresponds to the UI story.
4. Write a short plan:
   - Which files change and why
   - What new files (if any) are created
   - Any schema changes required
   - Risks or ambiguities
5. **Stop and confirm with the human before writing code** if anything is unclear or the plan has significant risk.

### For bug fixes

1. Diagnose first. Read logs, grep the codebase, find the exact line causing the issue.
2. Form a theory of the root cause — not just the symptom.
3. State your plan:
   - Root cause
   - Exact change(s) to fix it
   - Any side effects or related code that also needs updating
4. Make the fix. Do not refactor unrelated code at the same time.
5. Verify the fix builds and tests pass.

---

## 5. Making Changes

### The edit loop

1. **Read** the file (or the relevant section by line range) before editing.
2. **Edit** with a precise description of what you're changing and why.
3. **Check diagnostics** after every edit:
   - TypeScript: check diagnostics on the edited file
   - Rust: `cargo build` (see §6)
4. **Fix errors immediately** — don't accumulate broken state across multiple files.
5. **Clean up** — remove unused imports, dead CSS classes, and orphaned variables created by your changes.

### Targeted edits only

- Change only what the story or bug fix requires.
- Do not "improve" surrounding code while you're in a file.
- Do not remove code you didn't write unless it is directly causing the bug you are fixing.

### Spawning sub-agents

Use sub-agents for self-contained changes that are clearly scoped:
- When a task touches only one or two files and has no ambiguity
- When you need multiple independent tasks done in parallel
- When the change is mechanical (find/replace a pattern across files, update CSS)

**Include in the sub-agent message:**
- The exact files to edit
- The current relevant code (copy the actual lines, not a summary)
- The exact desired output or transformation
- Any types, imports, or constraints the agent needs to know
- "After editing, run diagnostics and fix any errors."

**Do not spawn sub-agents for:**
- Anything requiring diagnosis or judgement
- Changes that depend on reading the output of a previous change
- Anything touching the agent run-loop, auth, or encryption

### Schema changes (Rust / SQLx)

After any change to a SQL query or schema:
1. Remind the human to run: `cargo sqlx prepare`
2. The generated `.sqlx/` directory must be committed alongside the schema change.
3. Do not skip this — offline mode will fail in CI without it.

---

## 6. Testing

### Rust (server)

Run the full test suite from the project root:
```
cargo test 2>&1 | tail -20
```

Run a specific test module:
```
cargo test services::mcp 2>&1 | tail -20
```

Build check only (faster — catches compile errors without running tests):
```
cargo build 2>&1 | grep -E "^error" | head -20
```

**After any server change, at minimum do a `cargo build` before committing.** Run the full test suite before merging.

The test suite should report `0 failed` before you commit. If a test was already failing before your change, note it but do not break additional tests.

### TypeScript (web)

Check diagnostics on each edited file:
- Use the `diagnostics` tool on every `.tsx` / `.ts` file you touch.
- Fix all errors. Warnings from pre-existing issues (e.g. fast-refresh warnings in `shared.tsx`) can be left if they predate your change — confirm this by checking git blame mentally.

Type-check and build:
```
cd web && npm run build 2>&1 | tail -20
```

Run Vitest unit tests:
```
cd web && npm run test 2>&1 | tail -20
```

**Do not commit TypeScript with errors in files you authored or modified.**

### What "done" means

A story is done when:
- [ ] All acceptance criteria in `PLAN_3.md` are satisfied
- [ ] `cargo build` passes with no new errors
- [ ] `cargo test` passes (0 failed)
- [ ] TypeScript diagnostics are clean on all touched files
- [ ] `npm run build` passes
- [ ] Changes are committed with a correct message on the correct branch

---

## 7. Committing Work

Before committing:
```
git diff --stat HEAD          # see what changed
git diff HEAD                 # see exact diffs
```

Stage and commit:
```
git add <specific files>      # never use git add -A blindly
git commit -m "feat(scope): description"
```

Do not stage:
- Build artifacts (`web/dist/`, `target/`)
- Lock files unless dependencies actually changed
- Unrelated files that happened to be open

After committing, tell the human:
- What was committed and on which branch
- Whether tests pass
- What the next logical step is

---

## 8. Working with the Frontend

### State management

- **`useThreadStore`** — thread list, active thread, personas
- **`useMessageStore`** — per-thread messages, streaming state, pagination
- **`useSseStore`** — SSE connection lifecycle

To update a thread in the store after a server mutation:
```ts
useThreadStore.getState().upsertThread(res.data)
```

### API calls

All API functions live in `web/src/api/client.ts`. Add new endpoints there. Follow the existing pattern — each domain has a named API object (`threadsApi`, `providersApi`, `modelsApi`, etc.).

### CSS modules

All component styles use CSS Modules (`.module.css`). Class names are camelCase. Never use global selectors except in `styles.css` for resets and CSS variables.

CSS variable naming conventions (defined in `styles.css`):
- `--bg-primary`, `--bg-secondary`, `--bg-tertiary`, `--bg-elevated`
- `--text-primary`, `--text-secondary`, `--text-tertiary`
- `--border-subtle`, `--border-default`, `--border-strong`
- `--accent-primary`, `--accent-secondary`, `--accent-muted`
- `--bubble-user`, `--bubble-agent`, `--bubble-routine`

### Mobile vs Desktop

The app has two entirely separate layout trees selected at `App.tsx` level:
- `DesktopLayout` — sidebar + main area, no bottom nav
- `MobileLayout` — full-screen views, bottom tab bar, slide-up sheets

Business logic (stores, hooks, API calls) is shared. Only shell and navigation components differ. Do not add mobile-specific breakpoints inside desktop components — put mobile UI in `layouts/mobile/`.

---

## 9. Working with the Server

### Route handler pattern

```rust
pub async fn my_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<MyPayload>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    // ... query, validate, respond
    Ok((StatusCode::OK, Json(json!({ "data": result }))))
}
```

Always verify resource ownership: `WHERE id = ? AND user_id = ?`

### Error handling

Use `AppError` variants — never `unwrap()` in route handlers:
- `AppError::BadRequest(msg)` → 400
- `AppError::NotFound(msg)` → 404
- `AppError::Internal(anyhow_error)` → 500
- `AppError::Provider(msg)` → 502

### Encryption

- Credentials and API keys are encrypted at rest with AES-256-GCM.
- Always use `encryption::encrypt(value, &state.credential_master_key)` to store.
- Always use `encryption::decrypt(encrypted, &state.credential_master_key)` to read.
- Never use `machine_secret` for credential encryption — that's for a different purpose.
- **Never return encrypted bytes or raw secrets in API responses.**

### MCP notes

- MCP config dirs live at `~/.agent-deck/mcp/<tag>/` (tag = slug, not UUID)
- `config.json` includes an `"id"` field with the DB UUID
- `McpConnectionManager` is cloneable — all heavy state is behind `Arc`
- Working directory for local servers defaults to `mcp_dir/<tag>/` — relative executable paths resolve from there

---

## 10. Key Rules

These apply at all times, no exceptions.

1. **Read before you write.** Find the file path before reading. Read the file before editing.
2. **Never guess a path.** Use `find_path` or `list_directory` first.
3. **One story per branch.** Never mix stories.
4. **Never expose encrypted data or raw secrets in API responses.** Hard security rule.
5. **Hidden messages stay hidden.** The `visibility` filter on `GET /api/threads/:id/messages` is always applied.
6. **Plan before you fix.** Diagnose → write plan → confirm → fix. No fix code before the plan is confirmed.
7. **Never simplify code to clear a diagnostic.** Complete, correct code is more valuable than minimal code that compiles.
8. **After schema changes, `cargo sqlx prepare` must be run and `.sqlx/` committed.**
9. **Never delete branches after merging.** History is preserved.
10. **Mockups are the visual reference.** For UI stories, `mockups/*.html` is the design source of truth.
11. **`Arc<AppState>` is required.** Never pass `AppState` by value — `DashMap::clone()` deep-copies and breaks shared state across requests.
12. **Ask, don't assume.** If the spec and existing code conflict, raise it before implementing.
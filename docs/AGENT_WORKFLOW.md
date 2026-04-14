# Agent-Deck — Agent Skill

You are a senior full-stack engineer on **agent-deck**. When invoked, execute Steps 1–8 in order. Do not skip steps. Do not write code outside of sub-agents.

> **HARD RULE:** Never commit to `dev` or `main`. Every change goes on a feature branch and reaches `dev` only via pull request.

---

## Step 1 — Orient

Run immediately:

```bash
git branch
git log --oneline -15
git status --short
```

Open `docs/PLAN/PLAN_3.md`. Find the first story **not** marked ✅. Read its spec and the as-built notes for the story before it.

- If the current branch matches an in-progress story → that is your story.
- If you are on `dev` or `main` → check out the correct feature branch before doing anything else.

**You must know before continuing:**
- Which story you are working (e.g. Story 7.1)
- Whether it is in-progress or new
- Which feature branch you are on

> **`docs/todo/`** is the staging area for upcoming AD docs. Do not touch files there unless you are explicitly moving one to active (Step 3). Do not auto-promote a queued story — always wait for the human to instruct you.

---

## Step 2 — Branch (new stories only)

Before creating a branch, verify the previous story is clean:
1. `git status --short` is empty
2. Branch is pushed to origin
3. A PR to `dev` is open (use `gh pr create --base dev …` or tell the human)

Then:

```bash
git checkout dev && git pull
git checkout -b feature/phaseN-short-slug
```

| Type | Pattern | Example |
|---|---|---|
| Story | `feature/phase<N>-<slug>` | `feature/phase7-vapid-server` |
| Bug fix | `fix/<slug>` | `fix/mcp-working-dir` |
| Docs | `docs/<slug>` | `docs/update-workflow` |

If the branch already exists, check it out and read existing commits before continuing.

---

## Step 3 — Plan

Check whether `docs/AD-xxx.md` exists for this story.

- **Exists at `docs/AD-xxx.md`** (active) and first box checked → plan is approved, go to Step 4.
- **Exists at `docs/AD-xxx.md`** (active) and first box unchecked → present it to the human, wait for approval, do not write code.
- **Exists at `docs/todo/AD-xxx.md`** (queued) → move it to `docs/AD-xxx.md`, present it to the human, wait for approval.
- **Does not exist anywhere** → write it using the template below, save it to `docs/AD-xxx.md`, then stop and present it to the human.

Read every relevant source file before writing the plan. Use `grep` for symbols, `find_path` for files. Check `mockups/` for UI stories. Never guess a path.

### `AD-xxx.md` template

```markdown
# AD-xxx — <Story Name>

**Story:** <number and name from PLAN_3>
**Branch:** `feature/phaseN-slug`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §<section>

---

## Summary

<One paragraph: what this story delivers.>

---

## Current State

<What already exists. Name specific files and functions.>

---

## Implementation Plan

### Task 1 — <name>
- Files: `...`
- Change: <description>

### Task 2 — <name>
- Files: `...`
- Change: <description>

### Schema changes
<List migrations, new columns, changed queries.>

### Parallelisation note
<Which tasks can run in parallel, which must be sequential.>

---

## Acceptance Criteria

<Copy verbatim from PLAN_3.>

- [ ] <criterion>

---

## Human Review Instructions

<Leave blank until Step 5.>

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**
```

**Stop after writing the plan. Wait for the human to check `Implementation plan approved`.**

---

## Step 4 — Implement

Only begin after `Implementation plan approved` is checked.

**Never write code in this thread.** All implementation goes through sub-agents.

Spawn one sub-agent per independent task. Common splits:
- **Server task** — Rust routes, services, models
- **Frontend task** — React components, CSS modules
- **Test task** — unit tests for new server logic
- **Types/API task** — `web/src/types/index.ts`, `web/src/api/client.ts`

Every sub-agent message must include:
- Exact file paths (full, from repo root)
- Relevant existing code copied verbatim
- The exact transformation required
- All types, interfaces, and API contracts needed
- The rules in §10
- *"Run diagnostics on every changed file and fix all errors before moving on."*
- *"Do not edit any file outside the scope of this task."*

After all sub-agents finish:
1. Review each output for correctness.
2. Run the full test suite (§12).
3. Fix integration issues with direct edits.
4. Commit all changes (§11).

---

## Step 5 — Verify

Check every acceptance criterion in `AD-xxx.md` against the code. Tick each one. Do not mark coding complete until all are ticked.

Then fill in `## Human Review Instructions` in `AD-xxx.md`:

```markdown
## Human Review Instructions

**Prerequisites:** <server state, config needed>

**Steps:**
1. <action> → **Expected:** <result> / **Failure:** <sign>
2. ...

**Optional server log check:**
grep "..." ~/.agent-deck/server.log
```

Then update the plan:

```markdown
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
```

Present `AD-xxx.md` to the human and ask them to review.

---

## Step 6 — Human Review

Wait for the human to check `Human review approved`.

While waiting:
- Do not start the next story.
- Fix bugs with new commits on the current branch. Re-run tests after each fix.

---

## Step 7 — Close

Once `Human review approved` is checked:

**7a — One commit: archive plan + mark PLAN_3 complete**

```bash
mv docs/AD-xxx.md docs/deprecated/AD-xxx.md
# Edit PLAN_3.md: mark story ✅, add one-line as-built note if anything deviated
git add docs/deprecated/AD-xxx.md docs/AD-xxx.md docs/PLAN/PLAN_3.md
git commit -m "docs: close Story X.Y — archive plan, mark complete in PLAN_3"
```

**7b — Check `docs/todo/` for the next queued story**

After archiving, list `docs/todo/`. If a queued AD doc is present, tell the human what's available — do **not** auto-promote. Wait for the human to instruct you before moving any file from `docs/todo/` to `docs/`.

**7c — Open PR to `dev`**

```bash
git push origin feature/phaseN-slug
gh pr create --base dev \
  --title "feat: Story X.Y — <story name>" \
  --body "Closes Story X.Y. See docs/deprecated/AD-xxx.md for implementation notes."
```

If `gh` is unavailable, push the branch and tell the human.

Do not delete the branch after merging.

---

## Step 8 — Loop

Return to Step 1 and orient on the next story.

---

---

# Reference Material

Jump to the relevant section when the workflow requires it. Do not read speculatively.

---

## §9 — Project Structure

```
agent-deck/
├── server/
│   └── src/
│       ├── main.rs
│       ├── routes/       # HTTP handlers (one file per domain)
│       ├── services/     # Business logic
│       ├── models/       # Structs, sqlx FromRow
│       ├── db/           # Migration runner
│       └── error.rs      # AppError / AppResult
├── web/
│   └── src/
│       ├── App.tsx
│       ├── components/
│       ├── layouts/
│       │   └── mobile/
│       ├── stores/       # Zustand stores
│       ├── hooks/
│       ├── api/          # client.ts — all API calls
│       └── types/        # index.ts — shared TS types
├── docs/
│   ├── AGENT_WORKFLOW.md
│   ├── ARCHITECTURE.md
│   ├── AD-xxx.md         # Active plan (present during a story)
│   ├── todo/             # Queued AD docs — approved but not yet started
│   ├── deprecated/       # Completed stories — archived after close
│   └── PLAN/
│       ├── PLAN_1.md     # Architecture, data model
│       ├── PLAN_2.md     # API contract, feature specs
│       └── PLAN_3.md     # Execution plan — source of truth
└── mockups/              # HTML mockups — visual reference for UI stories
```

- Server port: **7474**
- Database: SQLite at `~/.agent-deck/.database/agent-deck.db` (WAL mode)
- MCP config: `~/.agent-deck/mcp/<tag>/config.json`
- All API routes prefixed `/api`
- Responses: `{ "data": <payload> }` on success · `{ "error": "..." }` on failure

---

## §10 — Key Rules

Apply at all times. Include in every sub-agent prompt.

1. Read before you write. Find the path before reading. Read before editing.
2. Never guess a path. Use `find_path` or `list_directory`.
3. One story per branch.
4. Never commit to `dev` or `main`. If you are on `dev`, create a branch first.
5. Never expose encrypted data or raw secrets in API responses.
6. Always apply the `visibility` filter on `GET /api/threads/:id/messages`.
7. Plan before fixing bugs. Diagnose → plan → confirm → fix.
8. Never simplify code to clear a diagnostic.
9. After schema changes, write the migration file and verify `cargo build` still compiles clean.
10. Never delete branches after merging.
11. Mockups are the design source of truth for UI stories.
12. Always use `Arc<AppState>`. Never pass `AppState` by value.
13. If spec conflicts with existing code, raise it before implementing.
14. Never stage build artefacts (`web/dist/`, `target/`).
15. Do not fix unrelated bugs. Note them in `AD-xxx.md` under "Noticed but deferred".

---

## §11 — Git and Commits

### Format

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

Scopes: `chat`, `mobile`, `mcp`, `server`, `pwa`, `agent`, `ui`, `auth`

Rules: under 72 chars · no period · imperative mood (`add` not `added`)

### Before every commit

```bash
git diff --stat HEAD
git add <specific files>      # never git add -A blindly
git commit -m "type(scope): description"
```

---

## §12 — Testing

### Rust

```bash
cargo build 2>&1 | grep -E "^error" | head -20   # after every edit
cargo test 2>&1 | tail -20                         # must show 0 failed
```

### TypeScript

Run `diagnostics` on every `.tsx`/`.ts` file you touch. Fix all errors in files you authored or modified.

```bash
nvm use 24 && cd web && npm run build 2>&1 | tail -20
nvm use 24 && cd web && npm run test 2>&1 | tail -20
```

Always run `nvm use 24` before any `node`, `npm`, or `npx` command.

### Definition of done

- [ ] All acceptance criteria from `AD-xxx.md` satisfied
- [ ] `cargo build` — no new errors
- [ ] `cargo test` — 0 failed
- [ ] TypeScript diagnostics clean on all touched files
- [ ] `npm run build` passes
- [ ] All changes committed on the feature branch

---

## §13 — Frontend Conventions

**Stores:** `useThreadStore`, `useMessageStore`, `useSseStore`

To update a thread after a server mutation:
```ts
useThreadStore.getState().upsertThread(res.data)
```

**API client:** All calls live in `web/src/api/client.ts` in named domain objects (`threadsApi`, `providersApi`, etc.). Never inline `fetch` in components.

**CSS Modules:** All styles use `.module.css`. Class names are camelCase. No global selectors except resets and CSS variables in `styles.css`.

CSS variable conventions: `--bg-primary/secondary/tertiary/elevated` · `--text-primary/secondary/tertiary` · `--border-subtle/default/strong` · `--accent-primary/secondary/muted` · `--bubble-user/agent/routine`

**Mobile vs Desktop:** Two separate layout trees in `App.tsx` — `DesktopLayout` and `MobileLayout`. Stores and hooks are shared. Never add mobile breakpoints inside desktop components — put mobile UI in `layouts/mobile/`.

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
    // verify ownership: WHERE id = ? AND user_id = ?
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
encryption::encrypt(value, &state.credential_master_key)   // store
encryption::decrypt(encrypted, &state.credential_master_key) // read
```

Never use `machine_secret` for credential encryption. Never return encrypted bytes or raw API keys in any response.

### MCP

- Config: `~/.agent-deck/mcp/<tag>/config.json` (`tag` = slug, not UUID; file contains `"id"` with DB UUID)
- `McpConnectionManager` is `Arc`-backed and cheaply cloneable
- Fallback working dir for local servers: `mcp_dir/<tag>/`
- Any `SELECT` feeding `connect_local` must include `tag` in `McpServerRow`

---

## §15 — Dev Docker Builds (Mobile Sessions)

### When to use this section

When development commands arrive **from a mobile device** (i.e. via the agent-deck mobile UI), the agent must **never run `cargo run`, `npm run dev`, or any command that modifies the live server process**. Instead, all build-and-test work goes through the isolated Docker dev instance.

### The `.agent-machine` file

Every repo that supports Docker dev builds ships with `.agent-machine` listed in `.gitignore`. The file must be created **manually on each authorised machine** — it is never committed.

The file contains a single keyword identifying the machine. The keyword for the Mac mini is:

```
mac-mini
```

To set up a new machine:

```bash
echo 'mac-mini' > .agent-machine
```

Requires Colima + Docker CLI installed (`brew install colima docker`).

### Machine guard

Before running any build command, check the machine identity:

```bash
cat .agent-machine
```

- If the file **does not exist** → stop. Do not run build commands. Tell the human this machine is not configured for dev Docker builds and point them to §15 setup.
- If the file does **not contain** `mac-mini` → stop. Same message.
- If the file **contains** `mac-mini` → proceed.

> The check is a substring match — `mac-mini`, `mac-mini-primary`, `mac-mini-studio` all pass.

### Setup (one-time, per machine)

```bash
echo 'mac-mini' > .agent-machine
```

Requires Colima + Docker CLI installed (`brew install colima docker`).

### The `agent-dev.sh` script

All dev Docker operations go through `scripts/agent-dev.sh`:

| Command | Effect |
|---|---|
| `./scripts/agent-dev.sh` | Stop old container → build image → start fresh (default) |
| `./scripts/agent-dev.sh build` | Build image only |
| `./scripts/agent-dev.sh start` | Start existing image |
| `./scripts/agent-dev.sh stop` | Stop and remove container |
| `./scripts/agent-dev.sh logs` | Tail container logs |
| `./scripts/agent-dev.sh status` | Check if container is running |
| `./scripts/agent-dev.sh clean` | Remove container + image |

The script **self-guards** — it reads `.agent-machine` and exits with an error if the file is missing or does not contain `mac-mini`.

### Dev instance details

| Property | Value |
|---|---|
| Port | **7475** |
| Data dir | `~/.agent-deck-dev/` |
| Container name | `agent-deck-dev` |
| Live instance | Port **7474** — never touched |

### Typical mobile dev loop

1. Make code changes on the feature branch (via filesystem/terminal MCP).
2. Verify `.agent-machine` is present.
3. Run `./scripts/agent-dev.sh` — builds and starts the dev container.
4. Tell the human: *"Dev instance rebuilt and running at http://localhost:7475. Live instance on 7474 is untouched."*
5. Human tests on `localhost:7475`.
6. Iterate: repeat from step 1 as needed.
7. When approved, follow Steps 5–7 of the normal workflow to commit and open a PR.

### Rules

- **Never** restart or kill the process on port 7474.
- **Never** run `cargo run` or `cargo watch` during a mobile session.
- **Never** run `npm run dev` against the live web directory during a mobile session.
- Always run `./scripts/agent-dev.sh status` before building to avoid orphaned containers.
- If Colima is not running, the script will attempt `colima start` automatically.

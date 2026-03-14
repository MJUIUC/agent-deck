# Story 4.3a — Handoff: `~/.agent-deck` Data Directory & MCP Filesystem Sync

**Branch:** `feature/phase4.3a-mcp-data-dir`
**Base branch:** `feature/phase4-mcp-connection-manager`
**Merge target:** `dev`

---

## What this story is

A focused infrastructure story that does two things:

1. Gives the server a stable, user-visible home directory at `~/.agent-deck` so it works correctly when run headlessly as a service, from any working directory, after a binary update, or from a pre-built release.
2. Makes `~/.agent-deck/mcp/<id>/config.json` the authoritative source of truth for MCP server configuration, with the database acting as an index and runtime state cache. Every API call that creates, updates, or deletes an MCP server must stay in sync with the filesystem.

This is **not** a feature story — no new user-facing behaviour. It is a foundation that Stories 4.4 and beyond depend on.

---

## Repository layout to know

```
agent-deck/
├── Cargo.toml                        # workspace — add `dirs = "5"` here
├── server/
│   ├── Cargo.toml                    # add `dirs = { workspace = true }`
│   └── src/
│       ├── config.rs                 # ← primary change
│       ├── main.rs                   # ← startup sequence change
│       ├── db/
│       │   └── mod.rs                # ← dev-path migration hint
│       ├── services/
│       │   └── mcp.rs                # ← startup_sync + working_dir + config.json writes
│       └── routes/
│           ├── personas.rs           # ← persona directory mirror
│           └── tokens.rs             # ← MCP API calls write/delete config.json
```

---

## Target directory structure

```
~/.agent-deck/
├── .database/
│   └── agent-deck.db          # DB — hidden from casual browsing, not directly edited
├── mcp/
│   └── <server-id>/
│       ├── config.json        # AUTHORITATIVE config for this MCP server
│       └── ...                # working files — venv, repo, logs — unmanaged by agent-deck
└── personas/
    ├── meta.json              # root index: [{ id, name, emoji }]
    └── <persona-id>/
        ├── meta.json          # { id, name, emoji, default_model, default_provider }
        └── instructions.md   # system_prompt content
```

---

## Source of truth table

| Data | Authoritative | DB role |
|---|---|---|
| MCP server config (name, tag, executable, args, env) | `mcp/<id>/config.json` | Cache — synced from file on startup, overwritten on API write |
| MCP runtime state (status, enabled, tool cache) | DB | Only home |
| Persona metadata (name, emoji, model links) | DB | Primary |
| Persona instructions (system_prompt) | DB | Primary — file is mirror |
| Credentials | DB (encrypted) | Never written to disk |

---

## Step-by-step implementation

### 1. Add `dirs` crate

`Cargo.toml` (workspace):
```toml
dirs = "5"
```

`server/Cargo.toml`:
```toml
dirs = { workspace = true }
```

---

### 2. `config.rs` — introduce `data_dir`, derive all paths from it

Replace the current `database_url` default logic. New fields on `Config`:

```rust
pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,          // NEW — everything is derived from this
    pub database_url: String,       // derived: {data_dir}/.database/agent-deck.db
    pub mcp_dir: PathBuf,           // derived: {data_dir}/mcp
    pub personas_dir: PathBuf,      // derived: {data_dir}/personas
    pub public_dir: String,         // unchanged
    pub fcm_service_account_json: Option<String>,
}
```

Resolution order for `data_dir`:
1. `AGENT_DECK_DATA_DIR` env var (absolute path)
2. `~/.agent-deck` via `dirs::home_dir()`
3. Panic with a clear message if neither resolves (no home dir is an unusual edge case)

All three derived paths (`database_url`, `mcp_dir`, `personas_dir`) are computed in `Config::from_env()` and never configured separately. No new env vars needed for them.

---

### 3. `main.rs` — startup sequence

```
1. Config::from_env()  — resolves data_dir and derived paths
2. Ensure directory tree exists:
     tokio::fs::create_dir_all(data_dir/.database/).await
     tokio::fs::create_dir_all(data_dir/mcp/).await
     tokio::fs::create_dir_all(data_dir/personas/).await
3. dev-path migration hint (see below)
4. db::init(&config.database_url).await
5. routes::build_router(pool, config).await
   — McpConnectionManager::start() calls startup_sync() internally
6. bind + serve with graceful shutdown (unchanged)
```

Pass `config.mcp_dir` and `config.personas_dir` into the places that need them. The cleanest way is to keep them on `Config` (already in `AppState`) so handlers can reach them via `state.config.mcp_dir`.

---

### 4. `db/mod.rs` — dev-path migration hint

After resolving the target database path but before calling `init()`, check:

```rust
let legacy = PathBuf::from("./data/agent-deck.db");
if !config.data_dir.join(".database/agent-deck.db").exists() && legacy.exists() {
    warn!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    warn!("  Found existing database at ./data/agent-deck.db");
    warn!("  Default location is now ~/.agent-deck/.database/agent-deck.db");
    warn!("  To migrate, run:");
    warn!("    mv ./data/agent-deck.db ~/.agent-deck/.database/agent-deck.db");
    warn!("  Using ./data/agent-deck.db for this session.");
    warn!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    // override database_url for this session
    config.database_url = "sqlite:./data/agent-deck.db".to_string();
}
```

Do **not** auto-migrate. Let the user move the file manually. This keeps the session working and makes the user aware of the change without data loss risk.

---

### 5. `services/mcp.rs` — three changes

#### 5a. `LocalConfig` gains `working_dir`

```rust
struct LocalConfig {
    executable: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    working_dir: Option<String>,   // NEW
}
```

In `connect_local`, before spawning:
```rust
let resolved_working_dir = cfg.working_dir
    .as_deref()
    .map(PathBuf::from)
    .unwrap_or_else(|| mcp_dir.join(&row.id));

tokio::fs::create_dir_all(&resolved_working_dir).await?;
cmd.current_dir(&resolved_working_dir);
```

`McpConnectionManager` needs to hold `mcp_dir: PathBuf`. Add it alongside `pool` and `master_key`. Pass it in from `build_router` via `config.mcp_dir`.

#### 5b. `startup_sync()`

New public async method on `McpConnectionManager`. Called from `start()` before connecting servers.

```
1. Read all entries in mcp_dir/
2. For each <id>/ subdirectory containing a config.json:
   a. Parse config.json as McpServerConfig (name, tag, server_type, config blob)
   b. Upsert into mcp_servers:
      - If row exists: update name, tag, server_type, config WHERE id = ?
      - If row missing: INSERT with enabled = 1, status = 'inactive'
   c. Log: info!("mcp: synced config for server {}", id)
3. Query all mcp_servers rows WHERE enabled = 1
4. For each, check if mcp_dir/<id>/config.json exists
   - If not: set enabled = 0, status = 'inactive', log a warning
```

`config.json` format (what gets written and read):
```json
{
  "name": "github",
  "tag": "github",
  "server_type": "local",
  "config": {
    "executable": "docker",
    "args": ["run", "-i", "--rm", "ghcr.io/github/github-mcp-server"],
    "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "{credential:github_pat}" }
  }
}
```

This is a superset of the DB `config` column — the DB `config` column stores only the inner `config` object. The outer `name`, `tag`, `server_type` fields are written to the file for human readability but the DB columns remain the indexed values.

#### 5c. Config file writes — called from `tokens.rs` handlers

Add three public methods to `McpConnectionManager`:

```rust
pub async fn write_config_file(&self, server: &McpServer) -> Result<()>
pub async fn delete_config_dir(&self, server_id: &str) -> Result<()>
```

`write_config_file` creates `mcp_dir/<id>/` if needed, then writes `config.json`.
`delete_config_dir` removes the entire `mcp_dir/<id>/` directory.

In `tokens.rs`:
- After `create_mcp` DB insert → `state.mcp.write_config_file(&server).await`
- After `update_mcp` DB update → `state.mcp.write_config_file(&updated).await`
- After `delete_mcp` DB delete → `state.mcp.delete_config_dir(&id).await`

Both methods log warnings on failure but do **not** return HTTP errors — a filesystem write failure should not fail the API call. The DB remains the operational source of truth; the file is best-effort for power users.

---

### 6. `routes/personas.rs` — persona directory mirror

Add a `PersonaFiles` helper (free functions or a small struct, your choice) in a new `services/personas.rs` or inline in the route file:

```rust
async fn write_persona_files(personas_dir: &Path, persona: &AgentPersona) -> Result<()>
async fn delete_persona_dir(personas_dir: &Path, persona_id: &str) -> Result<()>
async fn update_root_index(personas_dir: &Path, pool: &SqlitePool) -> Result<()>
```

`write_persona_files`:
1. `create_dir_all(personas_dir/<id>/)`
2. Write `personas/<id>/meta.json`:
   ```json
   { "id": "...", "name": "...", "emoji": "...", "default_model": "...", "default_provider": "..." }
   ```
3. Write `personas/<id>/instructions.md`:
   ```markdown
   <!-- Agent-deck persona instructions for: {name} -->
   <!-- Edit this file to update the system prompt. Reload not yet automatic — use the UI to save. -->

   {system_prompt content}
   ```

`delete_persona_dir`: `tokio::fs::remove_dir_all(personas_dir/<id>/)` — log warning on failure, don't propagate.

`update_root_index`: re-query all personas from DB, write `personas/meta.json` as a JSON array of `{ id, name, emoji }`. Called on create, update, and delete.

Wire into handlers:
- `create` → `write_persona_files` + `update_root_index` after DB insert
- `update` → `write_persona_files` + `update_root_index` after DB update
- `delete` → `delete_persona_dir` + `update_root_index` after DB delete
- `upload_avatar` → write to `personas_dir/<id>/avatar.<ext>` instead of `data/avatars/`

Same pattern as MCP: filesystem writes are best-effort, logged on failure, never fail the HTTP response.

---

## Things to keep in mind

**`build_router` return type is already `(Router, McpConnectionManager)`** — that came out of 4.3. The `McpConnectionManager` is also on `AppState` as `state.mcp`. Both are available in handlers and in `main.rs`.

**`Config` is already on `AppState`** — handlers reach `personas_dir` and `mcp_dir` via `state.config.personas_dir` etc. No new state fields needed.

**Test helpers that construct `AppState` directly** — there are several (`sse.rs`, `mod.rs`). They will need `config.mcp_dir` and `config.personas_dir` populated. Use `tempdir()` from the `tempfile` crate (already a common pattern in the test suite) or just point them at a temp path. Check `make_state()` in `sse.rs` — it already constructs a full `Config`.

**`dirs` crate** — use `dirs::home_dir()` which returns `Option<PathBuf>`. Handle the `None` case with a clear `anyhow::bail!` message.

**Don't touch the credential store** — it lives entirely in the DB and must never be written to disk in any form.

**Avatar upload** — the route exists at `POST /api/personas/:id/avatar` in `routes/personas.rs` but there is no UI for it. Move the write target to `personas_dir/<id>/avatar.<ext>` as part of this story, but don't build any new UI. Leave a `// TODO: avatar UI` comment.

---

## Tests to write

- `Config::from_env()` uses `~/.agent-deck` when no env var set
- `Config::from_env()` respects `AGENT_DECK_DATA_DIR`
- `Config` derived paths are correct relative to `data_dir`
- `startup_sync` upserts a `config.json` into DB when row is missing
- `startup_sync` updates DB config column when `config.json` changes
- `startup_sync` disables a DB row when its directory is gone
- `write_config_file` creates the directory and writes valid JSON
- `delete_config_dir` removes the directory
- `write_persona_files` creates `meta.json` and `instructions.md`
- `delete_persona_dir` removes the directory
- Root `personas/meta.json` index contains all current personas after create/delete

Use `tempfile::tempdir()` for all filesystem tests — never write to the real `~/.agent-deck` in tests.

---

## Acceptance criteria

- [ ] `cargo test` — all existing 152 tests still pass, new tests added
- [ ] First run: `~/.agent-deck/.database/`, `mcp/`, `personas/` all created automatically
- [ ] `AGENT_DECK_DATA_DIR=/tmp/test-deck cargo run` uses that directory
- [ ] Dev-path hint logged when `./data/agent-deck.db` exists and new path is absent
- [ ] `POST /api/mcp-servers` → `config.json` written to `mcp/<id>/`
- [ ] `PUT /api/mcp-servers/:id` → `config.json` updated
- [ ] `DELETE /api/mcp-servers/:id` → `mcp/<id>/` directory removed
- [ ] Drop a `config.json` into `mcp/<new-id>/`, restart server → server appears in `GET /api/mcp-servers`
- [ ] Remove `mcp/<id>/` directory, restart server → server disabled in `GET /api/mcp-servers`
- [ ] `POST /api/personas` → `personas/<id>/meta.json` and `instructions.md` written
- [ ] `PUT /api/personas/:id` → files updated
- [ ] `DELETE /api/personas/:id` → `personas/<id>/` removed
- [ ] Root `personas/meta.json` stays accurate after every persona operation
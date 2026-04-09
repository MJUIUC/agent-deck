# AD-9.6 — Configurable Ports

**Story:** 9.6 — Configurable Ports  
**Branch:** `feature/phase9-configurable-ports`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9  
**Related story:** `docs/AD-9.4.md` (macOS distribution — Homebrew formula and LaunchAgent both reference the server port)

---

## Summary

The agent-deck server is hardcoded to port 7474. Users who have a port conflict or a preference cannot change it without recompiling. This story adds a `--port` CLI flag and an `AGENT_DECK_PORT` environment variable so the port is configurable at startup. The Homebrew formula (`AD-9.4`) and LaunchAgent plist are updated to pass the port through. The web app's Vite proxy is updated to use the configured port in development.

---

## Current State

### What already exists

- `server/src/main.rs` — server entry point; `tokio::net::TcpListener::bind("0.0.0.0:7474")` hardcoded; no CLI argument parsing
- `server/src/state.rs` (or equivalent) — `AppState` struct; `server_port: u16` field was added in AD-8.5 for the Tailscale Funnel URL builder — it is set to `7474` at startup
- `server/Cargo.toml` — no CLI argument parsing crate; `dotenvy` is present for `.env` loading
- `web/vite.config.ts` — dev proxy: `target: 'http://localhost:7474'` hardcoded
- `scripts/com.agent-deck.server.plist` — LaunchAgent; `ProgramArguments` array references the binary with no port flag
- Homebrew formula (to be created in AD-9.4) — `brew services` uses the plist above

### What does not exist yet

- `--port` CLI flag in `main.rs`
- `AGENT_DECK_PORT` environment variable fallback
- `vite.config.ts` reading the port from an env var in development
- LaunchAgent plist updated to pass `--port` (or read `AGENT_DECK_PORT`)
- Documentation of the port configuration mechanism

---

## Implementation Plan

### Task 1 — CLI flag and env var in `main.rs`

**Modified file:** `server/src/main.rs`  
**Modified file:** `Cargo.toml` (workspace or server member)

Add `clap` as a dependency for CLI parsing:
```toml
clap = { version = "4", features = ["derive"] }
```

Define a minimal `Args` struct:
```rust
#[derive(clap::Parser)]
#[command(name = "agent-deck", about = "agent-deck server")]
struct Args {
    /// Port to listen on (default: 7474, env: AGENT_DECK_PORT)
    #[arg(long, env = "AGENT_DECK_PORT", default_value_t = 7474)]
    port: u16,
}
```

`clap`'s `env` attribute automatically falls back to the environment variable when the flag is not passed — no manual `std::env::var` call needed.

In `main()`:
```rust
let args = Args::parse();
let port = args.port;
let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
tracing::info!("agent-deck listening on port {}", port);
```

Pass `port` into `AppState` via the existing `server_port: u16` field (already there from AD-8.5). If AD-8.5 has not yet landed, add `server_port: u16` to `AppState` at this point.

On startup, log the resolved port prominently:
```
agent-deck listening on 0.0.0.0:7474
Auth token: <token>
```

### Task 2 — Vite dev proxy

**Modified file:** `web/vite.config.ts`

The dev proxy currently hardcodes port 7474. Read it from `process.env.AGENT_DECK_PORT` with a fallback:

```ts
const serverPort = process.env.AGENT_DECK_PORT ?? '7474';

export default defineConfig({
  server: {
    proxy: {
      '/api': {
        target: `http://localhost:${serverPort}`,
        changeOrigin: true,
      },
    },
  },
  // ...
});
```

Developers running a non-default port set `AGENT_DECK_PORT=8080 npm run dev` — no code changes needed.

### Task 3 — LaunchAgent plist

**Modified file:** `scripts/com.agent-deck.server.plist`

Add an `EnvironmentVariables` key to the plist so the port can be overridden without editing `ProgramArguments`:

```xml
<key>EnvironmentVariables</key>
<dict>
    <key>AGENT_DECK_PORT</key>
    <string>7474</string>
</dict>
```

Users who want a different port edit this value and run `launchctl unload` + `launchctl load` to restart the service. The default `7474` means no behavioral change for standard installs.

Note: When AD-9.4 lands, the Homebrew formula's generated plist should follow the same pattern.

### Task 4 — `.env.example` update

**Modified file:** `.env.example`

Add the port variable with a comment:
```
# Port for the agent-deck server to listen on (default: 7474)
# AGENT_DECK_PORT=7474
```

### Task 5 — Startup log and health check

**Modified file:** `server/src/main.rs` (already touched in Task 1)

The existing startup log line prints the auth token. Extend it to also print the port clearly:

```
╔══════════════════════════════════════════╗
║  agent-deck is running                   ║
║  http://localhost:7474                   ║
║  Auth token: <token>                     ║
╚══════════════════════════════════════════╝
```

This matches the existing startup banner style (if one exists) or establishes a consistent format. The URL uses the resolved `port` value, not a hardcoded 7474.

### Parallelisation note

All tasks are small and touch different files. Tasks 1 and 2 can be done in parallel. Tasks 3, 4, and 5 are one-liners and can be done alongside either. No sequential dependencies.

---

## Acceptance Criteria

- [ ] `cargo run -- --port 8080` starts the server on port 8080
- [ ] `AGENT_DECK_PORT=8080 cargo run` starts the server on port 8080 (no `--port` flag required)
- [ ] Running with no arguments or env var starts the server on port 7474 (no behavioral regression)
- [ ] `--help` output shows the `--port` flag and the `AGENT_DECK_PORT` env var
- [ ] `AppState.server_port` reflects the resolved port value at runtime
- [ ] `vite.config.ts` proxy uses `AGENT_DECK_PORT` env var when set, falls back to 7474
- [ ] `AGENT_DECK_PORT=8080 npm run dev` proxies `/api` to `http://localhost:8080`
- [ ] LaunchAgent plist has an `EnvironmentVariables` block with `AGENT_DECK_PORT = 7474`
- [ ] `.env.example` documents `AGENT_DECK_PORT`
- [ ] Startup log prints the resolved port (not a hardcoded 7474)
- [ ] `cargo build` passes, all existing tests pass

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**

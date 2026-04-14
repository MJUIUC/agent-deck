# AD-9.1 — Structured Application Logging

**Story:** 9.1 — Structured application logging with rolling file retention
**Branch:** `feature/phase9-structured-logging`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9

---

## Summary

Wires up the existing `tracing` + `tracing-subscriber` dependencies (already declared in `Cargo.toml` but only used for stdout output) into a dual-sink logging system: structured output continues to go to stdout, and a parallel rolling file appender writes daily log files to `~/.agent-deck/.logs/`. Files are named `agent-deck-YYYY-MM-DD.log`. On server startup, any log file older than 30 days is automatically purged. Log level is controlled at runtime via the `RUST_LOG` environment variable (defaulting to `info`). No new dependencies are required — `tracing-appender` ships as part of the `tracing-subscriber` ecosystem and is already available via the workspace.

---

## Current State

- `tracing` and `tracing-subscriber` are declared in the workspace `Cargo.toml` and in `server/Cargo.toml`.
- `server/src/main.rs` initialises `tracing_subscriber::fmt()` targeting stdout only, with `EnvFilter` from `RUST_LOG` (defaulting to `info`). This is the only tracing initialisation point.
- `~/.agent-deck/` directory is created on startup (`config.data_dir`). A `.logs/` subdirectory does not yet exist and is not created anywhere.
- `server/src/config.rs` exposes `Config::from_env()` which populates `config.data_dir`. This is the correct source for the log directory path.
- No log file rotation, retention, or purge logic exists anywhere in the codebase.

---

## Implementation Plan

### Task 1 — Add `tracing-appender` to workspace dependencies

- Files: `Cargo.toml` (workspace root), `server/Cargo.toml`
- Add to `[workspace.dependencies]`:
  ```toml
  tracing-appender = "0.2"
  ```
- Add to `server/Cargo.toml` under `[dependencies]`:
  ```toml
  tracing-appender = { workspace = true }
  ```

### Task 2 — Create log directory and purge stale files on startup

- Files: `server/src/main.rs`
- After the existing directory creation block (the one that creates `db_dir`, `mcp_dir`, `personas_dir`), add:
  ```rust
  let logs_dir = config.data_dir.join(".logs");
  tokio::fs::create_dir_all(&logs_dir).await?;
  purge_old_logs(&logs_dir, 30).await;
  ```
- Add a new private async function `purge_old_logs` in `main.rs`:
  ```rust
  async fn purge_old_logs(logs_dir: &std::path::Path, retain_days: u64) {
      let cutoff = std::time::SystemTime::now()
          - std::time::Duration::from_secs(retain_days * 86_400);
      let mut read_dir = match tokio::fs::read_dir(logs_dir).await {
          Ok(rd) => rd,
          Err(_) => return,
      };
      while let Ok(Some(entry)) = read_dir.next_entry().await {
          let path = entry.path();
          if path.extension().and_then(|e| e.to_str()) != Some("log") {
              continue;
          }
          if let Ok(meta) = tokio::fs::metadata(&path).await {
              if let Ok(modified) = meta.modified() {
                  if modified < cutoff {
                      let _ = tokio::fs::remove_file(&path).await;
                      // stdout only — tracing not yet initialised at this point
                      eprintln!("[agent-deck] purged old log: {}", path.display());
                  }
              }
          }
      }
  }
  ```
  > **Note:** `purge_old_logs` uses `eprintln!` rather than `tracing::info!` because it runs before the tracing subscriber is initialised.

### Task 3 — Replace single-sink tracing init with dual-sink (stdout + rolling file)

- Files: `server/src/main.rs`
- Replace the existing `tracing_subscriber::fmt().with_env_filter(...).init()` block with:
  ```rust
  use tracing_subscriber::layer::SubscriberExt;
  use tracing_subscriber::util::SubscriberInitExt;

  let file_appender = tracing_appender::rolling::daily(&logs_dir, "agent-deck.log");
  let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

  let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
      .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

  tracing_subscriber::registry()
      .with(env_filter)
      .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
      .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
      .init();
  ```
- Bind `_guard` to a local variable that lives for the duration of `main` (name it `_appender_guard` to make the intentional lifetime explicit):
  ```rust
  let (non_blocking, _appender_guard) = tracing_appender::non_blocking(file_appender);
  ```
  > **Critical:** `_appender_guard` must be kept alive until the end of `main`. Dropping it early will silently stop the file sink mid-run.

### Schema changes

None — no database migrations required.

### Parallelisation note

- **Task 1** (Cargo.toml) must complete before **Tasks 2 and 3** can be compiled and tested.
- **Tasks 2 and 3** both edit `main.rs` and must be done sequentially (or in a single sub-agent pass) to avoid merge conflicts.

Recommended agent split:
- **Sub-agent A**: Task 1 (Cargo.toml workspace + server/Cargo.toml)
- After A completes → **Sub-agent B**: Tasks 2 + 3 together (`main.rs` changes — directory creation, purge function, dual-sink init)

---

## Acceptance Criteria

- [ ] On server startup, `~/.agent-deck/.logs/` is created if it does not exist
- [ ] A daily log file named `agent-deck-YYYY-MM-DD.log` is written to `~/.agent-deck/.logs/` and contains structured log output
- [ ] All existing `tracing::info!`, `tracing::warn!`, and `tracing::error!` call sites write to the file sink without any changes to those call sites
- [ ] Log output continues to appear on stdout (dual sink — no regression)
- [ ] Log files older than 30 days are deleted on server startup
- [ ] `RUST_LOG=debug cargo run` produces debug-level output in both sinks
- [ ] `cargo build` produces no new errors or warnings
- [ ] `cargo test` shows 0 failed

---

## Human Review Instructions

*Leave blank until coding is complete.*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved**

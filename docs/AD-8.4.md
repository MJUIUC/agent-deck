# AD-8.4 — MCP Keepalive Ping

**Story:** 8.4 — MCP Keepalive Ping
**Branch:** `feature/phase8-mcp-keepalive`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8

---

## Summary

Local MCP servers (spawned as child processes) may exit after a period of inactivity due to their own internal idle timers. This causes a cold-start penalty on the next tool call — the process must be re-spawned, the MCP handshake re-run, and only then can the tool execute. This story adds a background keepalive for **local (stdio) servers only**: a `tools/list` ping sent every 45 seconds while the server is connected. Remote (HTTP) servers are excluded — they are stateless request/response and are already monitored by the existing `monitor_remote` health-check loop.

---

## Current State

- All MCP connection logic lives in `server/src/services/mcp.rs`
- `McpConnection` holds `inner: Mutex<McpConnectionInner>` (contains `child`, `stdin`) and `stdout_reader: Option<Arc<Mutex<BufReader<ChildStdout>>>>`
- `call_tool` (local path) acquires `inner` briefly to write a request to stdin, releases it, then acquires `stdout_reader` to read the response — these are two separate lock acquisitions with a gap between them
- The gap means a concurrent writer could interleave on stdin and steal the response from stdout
- `supervise` manages the connect → monitor → backoff loop; `monitor_connection` blocks until the connection is considered dead
- `shutdown_tx`/`shutdown_rx` (`watch::channel`) on `McpConnection` signal intentional teardown; dropping the `Arc<McpConnection>` closes `shutdown_tx` causing `shutdown_rx.changed()` to return `Err`
- No keepalive mechanism exists

---

## Design

### Why a `request_lock` is required

The local stdio transport is a single sequential pipe. `call_tool` writes a request to stdin and reads the matching response from stdout. If a keepalive fires between the write and the read, two requests are queued on stdin, and the responses arrive interleaved on stdout — the wrong reader consumes the wrong response. A `Mutex<()>` named `request_lock` added to `McpConnection` serializes the full write-then-read cycle, making concurrent writers impossible.

### `Mutex<()>` vs `Semaphore`

A `Semaphore(1)` and `Mutex<()>` are equivalent for mutual exclusion. The semaphore's distinguishing features (`add_permits`, `close`) are not needed here. `Mutex<()>` communicates the intent (mutual exclusion) more clearly and prevents accidentally widening the invariant.

### Keepalive uses `try_lock()`, not `.lock().await`

The agent run loop's `RunState::semaphore` uses `.acquire().await` because queuing messages is the correct behavior — no message should be silently dropped. The keepalive has the opposite requirement: a ping that queues behind a 3-minute tool call and fires immediately after it finishes provides no keepalive benefit. `try_lock()` returns `Err` instantly if the lock is held; the keepalive simply skips that tick and waits for the next 45-second interval.

### Scope: local servers only

Remote servers communicate over HTTP — each request is an independent POST with no shared pipe state. They do not have the idle-exit problem (no child process) and are already monitored by `monitor_remote`. No keepalive is spawned for remote connections.

### Cancellation

The keepalive task selects on `shutdown_rx.changed()`. When `disconnect_server` calls `shutdown_tx.send(true)`, or when the `Arc<McpConnection>` is dropped from the pool (closing `shutdown_tx`), the keepalive exits cleanly without needing a separate `CancellationToken`.

---

## Implementation Plan

### Task 1 — Add `request_lock` to `McpConnection`

**File:** `server/src/services/mcp.rs`

Add one field to `McpConnection`:

```rust
struct McpConnection {
    server_id: String,
    inner: Mutex<McpConnectionInner>,
    request_lock: tokio::sync::Mutex<()>,   // ← new
    status: RwLock<McpStatus>,
    tools: RwLock<Vec<McpTool>>,
    stdout_reader: Option<Arc<Mutex<BufReader<tokio::process::ChildStdout>>>>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}
```

Initialise it in both `connect_local` and `connect_remote` struct literals:

```rust
request_lock: tokio::sync::Mutex::new(()),
```

### Task 2 — Hold `request_lock` across the full stdio round-trip in `call_tool`

**File:** `server/src/services/mcp.rs`, `call_tool`, local branch only

Wrap the existing stdin-write + stdout-read sequence with a `request_lock` guard:

```rust
if is_local {
    let _request_guard = conn.request_lock.lock().await;

    // existing: write req to stdin via inner lock
    {
        let mut inner = conn.inner.lock().await;
        write_line_to_stdin(inner.stdin.as_mut().unwrap(), &req).await?;
    }

    // existing: read response from stdout_reader
    let stdout_reader = conn.stdout_reader.as_ref()
        .ok_or_else(|| anyhow!("MCP server '{}' has no stdout reader", server_id))?;
    let resp = {
        let mut reader = stdout_reader.lock().await;
        read_json_rpc_response(&mut *reader).await?
    };

    // _request_guard drops here, releasing the lock
    // ... rest of response handling unchanged
}
```

The `_request_guard` must remain in scope (not prefixed with `_` alone — use a named binding to make the intent clear and prevent an accidental immediate drop).

### Task 3 — Keepalive function and lifecycle integration

**File:** `server/src/services/mcp.rs`

Add a private async function:

```rust
async fn run_keepalive_local(conn: Arc<McpConnection>) {
    const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(45);
    let mut ticker = tokio::time::interval(KEEPALIVE_INTERVAL);
    ticker.tick().await; // consume the immediate first tick

    let mut shutdown_rx = conn.shutdown_rx.clone();

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match conn.request_lock.try_lock() {
                    Err(_) => {
                        // A tool call is in flight; skip this tick.
                        tracing::trace!(
                            server_id = %conn.server_id,
                            "mcp keepalive: skipping tick, request_lock held"
                        );
                    }
                    Ok(_guard) => {
                        // Send tools/list and read the response while holding the lock.
                        let ping_result = async {
                            let id = {
                                let mut inner = conn.inner.lock().await;
                                let id = inner.next_id();
                                let req = JsonRpcRequest::request(id, "tools/list", None);
                                write_line_to_stdin(inner.stdin.as_mut().unwrap(), &req).await?;
                                id
                            };
                            let stdout_reader = conn.stdout_reader.as_ref()
                                .ok_or_else(|| anyhow!("no stdout reader"))?;
                            let mut reader = stdout_reader.lock().await;
                            let resp = read_json_rpc_response(&mut *reader).await?;
                            if let Some(err) = resp.error {
                                return Err(anyhow!("tools/list error: {}", err));
                            }
                            let _ = id; // suppress unused warning
                            Ok(())
                        }.await;

                        match ping_result {
                            Ok(()) => tracing::trace!(
                                server_id = %conn.server_id,
                                "mcp keepalive: ping ok"
                            ),
                            Err(e) => {
                                tracing::warn!(
                                    server_id = %conn.server_id,
                                    error = %e,
                                    "mcp keepalive: ping failed, stopping"
                                );
                                return;
                            }
                        }
                    }
                }
            }
            result = shutdown_rx.changed() => {
                // shutdown_tx sent true (disconnect_server) or was dropped (connection removed).
                tracing::debug!(
                    server_id = %conn.server_id,
                    "mcp keepalive: shutdown signal received"
                );
                let _ = result;
                return;
            }
        }
    }
}
```

Spawn it inside `supervise`, in the `Ok(conn)` arm, immediately after the connection is inserted into the pool:

```rust
Ok(conn) => {
    let conn = Arc::new(conn);
    self.connections.insert(server_id.to_string(), conn.clone());

    // Spawn keepalive for local connections only.
    let is_local = conn.inner.lock().await.child.is_some();
    if is_local {
        let conn_for_keepalive = conn.clone();
        tokio::spawn(async move {
            run_keepalive_local(conn_for_keepalive).await;
        });
    }

    // existing: monitor_connection, backoff_shutdown_rx, etc.
    self.monitor_connection(server_id, &conn).await;
    // ...
}
```

The `tokio::spawn` handle is intentionally not stored — the keepalive exits on its own via `shutdown_rx` when the connection is removed from the pool. Detaching is correct here because the keepalive must outlive the `supervise` arm's local scope but must not outlive the `McpConnection`.

### Task 4 — Tracing levels

- Successful ping: `tracing::trace!` — invisible in normal operation
- Skipped tick: `tracing::trace!`
- Shutdown signal: `tracing::debug!`
- Ping failure: `tracing::warn!` — only meaningful signal

No new files are needed. All changes are in `server/src/services/mcp.rs`.

### Parallelisation note

Tasks 1 and 2 are sequential (Task 2 depends on the field added in Task 1). Task 3 depends on Task 1 (uses `request_lock`). All three can be done in a single pass through `services/mcp.rs`.

---

## Acceptance Criteria

- [ ] A `tools/list` ping is sent to each connected **local** MCP server every 45 seconds
- [ ] If `request_lock` is held when the tick fires, the ping is skipped for that interval — no queuing
- [ ] Keepalive task starts when a local MCP server connects (inside `supervise`)
- [ ] Keepalive task stops when `shutdown_rx` signals teardown (disconnect or connection drop)
- [ ] A failed ping logs a `WARN` and exits — does not retry or crash
- [ ] Successful pings and skipped ticks log at `TRACE` only
- [ ] `call_tool` (local path) holds `request_lock` for the full write-then-read cycle
- [ ] Remote (HTTP) servers: no keepalive spawned, `call_tool` unchanged
- [ ] No user-visible UI changes
- [ ] `cargo build` passes, all existing tests pass

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [x] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**

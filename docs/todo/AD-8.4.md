# AD-8.4 — MCP Keepalive Ping

**Story:** 8.4 — MCP Keepalive Ping  
**Branch:** `feature/phase8-mcp-keepalive`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8  
**Related stories:** `docs/AD-8.2.md` (local MCP process management), `docs/AD-8.3.md` (per-thread MCP)

---

## Summary

MCP servers are spawned as child processes and may exit after a period of inactivity. This causes a cold-start penalty on the next tool call — the process must be re-spawned, the MCP handshake re-run, and only then can the tool execute. This latency is noticeable to the user and undermines the responsiveness of tool-heavy threads.

This story adds a background keepalive mechanism: while a thread is active and has MCP servers enabled, agent-deck sends a lightweight periodic ping to each connected MCP server. This keeps the process warm so that all tool calls are fast.

---

## Current State

### What already exists

- `server/src/mcp/` — MCP client management, process spawning, tool call dispatch
- Per-thread MCP enable/disable (AD-8.3) — MCP servers are scoped to threads
- `AppState` — holds active MCP connections keyed by thread or globally
- No keepalive mechanism exists — MCP processes are spawned on demand and may idle-exit

### What does not exist yet

- Background keepalive task per active MCP server
- Ping/noop call logic (using `tools/list` or MCP `ping` if supported)
- Lifecycle tie-in: start keepalive when MCP enabled for thread, stop when disabled or thread closed

---

## Design

### Ping mechanism

The MCP protocol does not define a dedicated `ping` method in all implementations. The safest universal noop is **`tools/list`** — it is always supported, returns a known response, and produces no side effects. The response can be discarded.

If a future MCP spec formalizes a `ping` method, the implementation can be swapped without changing the keepalive lifecycle logic.

### Keepalive interval

**45 seconds.** This is:
- Short enough to prevent most MCP server idle timeouts (typically 60–120s)
- Long enough to be negligible overhead
- Configurable via a compile-time constant initially; can be promoted to a config field later

### Lifecycle

| Event | Keepalive action |
|---|---|
| Thread opened, MCP server connected | Spawn keepalive task for that server |
| Thread closed | Cancel keepalive task |
| MCP server disabled for thread | Cancel keepalive task |
| MCP server re-enabled | Spawn new keepalive task |
| MCP server errors on ping | Log warning, cancel task (server is gone) |

Each keepalive task is a `tokio::task` holding a `tokio::time::interval`. It is cancelled via a `CancellationToken` or by dropping the task handle when the thread/server scope ends.

### Error handling

- If a ping fails, log a `WARN` and stop the keepalive for that server — don't loop-crash
- Do not attempt to restart the MCP server from within the keepalive task; that responsibility belongs to the MCP connection manager
- Ping errors are silent to the user — no UI impact

---

## Implementation Plan

### Task 1 — Ping helper function

**Modified file:** `server/src/mcp/client.rs` (or equivalent MCP client module)

Add a `ping_server(server_id: &str) -> Result<()>` function that calls `tools/list` on the given MCP server and discards the response. This function will be reused by the keepalive task and can also be used for connection health checks in the future.

```rust
pub async fn ping_server(client: &McpClient) -> anyhow::Result<()> {
    let _ = client.list_tools().await?;
    Ok(())
}
```

### Task 2 — Keepalive task

**New file:** `server/src/mcp/keepalive.rs`

```rust
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

const KEEPALIVE_INTERVAL_SECS: u64 = 45;

pub async fn run_keepalive(client: McpClientHandle, cancel: CancellationToken) {
    let mut ticker = interval(Duration::from_secs(KEEPALIVE_INTERVAL_SECS));
    ticker.tick().await; // consume immediate first tick

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e) = ping_server(&client).await {
                    tracing::warn!("MCP keepalive ping failed for {}: {}", client.id(), e);
                    break;
                }
                tracing::trace!("MCP keepalive ping ok: {}", client.id());
            }
            _ = cancel.cancelled() => {
                tracing::debug!("MCP keepalive cancelled for {}", client.id());
                break;
            }
        }
    }
}
```

### Task 3 — Lifecycle integration

**Modified file:** `server/src/mcp/manager.rs` (or wherever MCP connections are tracked)

When a MCP server connection is established for a thread:
1. Create a `CancellationToken`
2. Spawn `run_keepalive(client_handle, cancel_token.clone())`
3. Store the `CancellationToken` alongside the connection handle

When the connection is torn down (thread closed, MCP disabled):
1. Call `cancel_token.cancel()` — the keepalive task exits cleanly on next loop

### Task 4 — Tracing / observability

Keepalive pings log at `TRACE` level (not `DEBUG`) — they should be invisible in normal operation but visible when deep-debugging MCP issues. Failures log at `WARN`.

This integrates cleanly with AD-9.1 (structured logging with `tracing-appender`).

### Parallelisation note

Task 1 and Task 2 can be written in parallel. Task 3 depends on both. Task 4 is woven into Tasks 2 and 3.

---

## Acceptance Criteria

- [ ] A `tools/list` ping is sent to each active MCP server every 45 seconds while the thread is open
- [ ] Keepalive tasks start when an MCP server is connected to a thread
- [ ] Keepalive tasks stop when the thread is closed or the MCP server is disabled
- [ ] A failed ping logs a `WARN` and stops the keepalive for that server — does not crash or retry infinitely
- [ ] Successful pings log at `TRACE` level only
- [ ] No user-visible UI changes — this is entirely backend
- [ ] `cargo build` passes, all existing tests pass
- [ ] Manual test: open a thread with an MCP server, wait 2+ minutes, make a tool call — response is fast (no cold-start delay)

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**

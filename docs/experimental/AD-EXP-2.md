# AD-EXP-2 — Multi-Agent Subtasks (Orchestrator/Subagent Architecture)

**Story:** EXP-2 — Multi-Agent Subtasks  
**Status:** 🧪 EXPERIMENTAL — Not on active roadmap. Keeping for reference and future consideration.  
**Branch:** `feature/exp-multi-agent` *(not yet created)*  
**Phase doc reference:** N/A  
**Related stories:** AD-9.2 (SSE Reconnection & Resilience), AD-9.7 (Progress Indicators)

---

## Summary

An orchestrator/subagent system allowing a parent agent thread to spawn independent child agent tasks, route them to different models, and aggregate results — all within agent-deck's existing Rust + SSE architecture. Subagents are first-class SSE streams with their own DB-persisted sessions, making them resumable across crashes and reconnects.

This document is **experimental**. The ideas here are worth preserving and may inform future development, but this feature is not scheduled for implementation. Revisit when the core roadmap (Phase 8–9) stabilizes.

---

## Motivation

- Complex agent tasks (research, code review, multi-step analysis) benefit from parallel subtask execution
- Routing subtasks to cheaper/faster models (e.g. Sonnet for search, Opus for synthesis) reduces cost and latency
- Zed, IronClaw, and other production agent systems have converged on this pattern
- agent-deck's existing `RunState` + semaphore + SSE infrastructure maps cleanly onto a subagent model
- Shipping per-subagent model routing from day one addresses a known gap in Zed's implementation (open issue #52042)

---

## Prior Art

### Zed Editor

Zed's subagent system ships as part of Zed AI (April 2025). Key characteristics:

```
ThreadEnvironment trait
  ├── create_subagent(label, cx) → SubagentHandle
  └── resume_subagent(session_id, cx) → SubagentHandle

NativeSubagentHandle {
  session_id,           // DB-persisted, resumable
  subagent_thread,      // Entity<Thread> — full agent loop
  acp_thread,           // ACP protocol thread
  parent_thread_entity  // WeakEntity<Thread> — parent can die
}

ThreadEvents (streamed back to parent):
  SubagentSpawned(session_id)
  ToolCall / ToolCallUpdate
  Plan(plan)
  Retry(status)
```

Each subagent in Zed is a full `Entity<Thread>` — a complete independent agent loop with its own GPUI entity lifecycle. Results flow back to the parent via `ThreadEvent` variants on a shared event bus.

**Key gap in Zed:** Per-subagent model routing is not implemented (upstream GitHub issue #52042). All subagents inherit the parent thread's model.

**Notable strength:** `resume_subagent(session_id)` — subagents persist across crashes and can be picked back up. Requires storing `session_id` in the DB.

### IronClaw

IronClaw uses Tokio-native primitives:

```
ThreadTree
  ├── spawn_subtask(brief) → oneshot::Receiver<SubtaskResult>
  ├── spawn_batch(briefs) → Vec<oneshot::Receiver<SubtaskResult>>
  └── PolicyEngine (concurrency limits, priority queues)

DefaultSelfRepair — automatic retry on subagent failure
```

Each subtask gets a "brief" (task description + context budget). Results flow back via `oneshot::channel`. Supports parallel batch spawning. Has self-repair/retry logic that Zed lacks.

**Key gap in IronClaw:** Not resumable — no persistent `session_id`. Subagent state lost on crash.

---

## Proposed Architecture for agent-deck

### Design Goals

1. **Subagents as first-class SSE streams** — each subagent is fully visible in the UI
2. **Per-subagent model routing** — parent can specify a different model per subtask
3. **Resumable subagents** — `session_id` stored in DB, survives crashes (like Zed)
4. **Parallel batch execution** — multiple subagents can run concurrently (like IronClaw)
5. **Self-repair** — configurable retry on subtask failure (like IronClaw)
6. **Minimal new infrastructure** — built on top of existing `RunState`, semaphore, and SSE machinery

---

### Data Model

#### New table: `subagent_sessions`

```sql
CREATE TABLE subagent_sessions (
  id           TEXT PRIMARY KEY,      -- UUID session_id
  parent_turn_id TEXT NOT NULL,       -- turn_id of parent that spawned this
  thread_id    TEXT NOT NULL,         -- which thread owns this subagent
  label        TEXT NOT NULL,         -- human-readable task label
  model        TEXT,                  -- override model (NULL = inherit parent)
  brief        TEXT NOT NULL,         -- task description / system prompt injection
  status       TEXT NOT NULL,         -- 'pending' | 'running' | 'complete' | 'failed'
  result       TEXT,                  -- JSON result payload on completion
  created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

#### `RunState` extension

```rust
pub struct RunState {
    // existing fields ...
    pub parent_session_id: Option<String>,   // set if this run IS a subagent
    pub subagent_sessions: Vec<String>,      // session IDs of spawned children
}
```

---

### Tool Interface

The orchestrator agent spawns subagents via built-in tools (like `save_memory` / `recall_memory`). These are injected into the system prompt automatically when the feature is enabled for a persona.

#### `spawn_subtask`

```
spawn_subtask(label, brief, model?)
```

- Creates a `subagent_sessions` row with `status = 'pending'`
- Spawns a Tokio task running a full agent loop (same `run_agent_loop()`)
- Returns `session_id` immediately — orchestrator can continue
- Subagent streams its own SSE events tagged with `session_id`

#### `spawn_batch`

```
spawn_batch(tasks: [{label, brief, model?}])
```

- Spawns multiple subagents in parallel
- Returns `[{label, session_id}]` immediately
- All run concurrently, each with its own SSE stream

#### `await_subtask`

```
await_subtask(session_id, timeout_seconds?)
```

- Blocks the orchestrator's current tool-call round until `status = 'complete' | 'failed'`
- Returns `{status, result}` when done
- Orchestrator can then synthesize results

#### `list_subtasks`

```
list_subtasks()
```

- Returns all subagents for the current parent `turn_id` and their statuses
- Allows orchestrator to check progress before deciding to `await_subtask`

---

### SSE Events

New event types emitted on the **per-thread SSE stream**, tagged with `session_id`:

```
SubagentSpawned  { session_id, label, model }
SubagentToken    { session_id, token }           // subagent's token stream
SubagentComplete { session_id, result_summary }
SubagentFailed   { session_id, error, retry_count }
SubagentRetrying { session_id, attempt }
```

The UI can render subagent activity inline in the parent thread — collapsed by default, expandable to show the full subagent token stream.

---

### Concurrency & Safety

- Each subagent acquires its own **per-thread semaphore slot** (or a dedicated subagent semaphore with configurable capacity, e.g. 4 concurrent subagents per thread)
- Parent `turn_id` race guard still applies — if the parent thread is cancelled, all child subagents for that `turn_id` are cancelled
- Subagent `RunState` carries `parent_session_id` for lineage tracking
- Max subagent depth: 2 (subagents cannot spawn sub-subagents) to prevent runaway trees

---

### Resumability

Following Zed's pattern:

- Every subagent has a DB-persisted `session_id`
- On server restart, `status = 'running'` rows are detected on startup
- Options:
  - **Auto-resume**: restart the subagent loop from last DB checkpoint
  - **Mark-failed**: set `status = 'failed'` and let orchestrator retry on next reconnect
- Initial implementation: **mark-failed** (simpler). Auto-resume deferred.

This pairs naturally with **AD-9.2** (SSE Reconnection & Resilience) — the parent thread's reconnect handler can check `list_subtasks()` and re-await any that were running.

---

### Self-Repair

Configurable per subagent or globally in persona settings:

```toml
[subtasks]
max_retries = 2
retry_delay_seconds = 5
```

On `SubagentFailed`:
1. Increment `retry_count` on the `subagent_sessions` row
2. If `retry_count < max_retries`: re-spawn with same `session_id`, emit `SubagentRetrying`
3. If exhausted: emit `SubagentFailed` final, set `status = 'failed'`

---

### Model Routing

The `model` field on `spawn_subtask` accepts any model identifier supported by the existing model picker. This means:

- Orchestrator: `claude-opus-4` (reasoning, synthesis)
- Search subtasks: `claude-sonnet-4` (fast, cheap)
- Coding subtasks: `claude-sonnet-4` or `gpt-4.1`
- Local subtasks: `llama3.3:70b` via Ollama

Model routing config can also be set at the **persona level** as a subtask model default, so the orchestrator doesn't need to specify a model every time.

---

### Example: Research + Synthesize

```
User: "Write a comprehensive analysis of Redwire's competitive position in the space manufacturing market."

Orchestrator (Opus):
  1. spawn_batch([
       { label: "search_competitors",   brief: "Find Redwire's main competitors in space manufacturing...", model: "claude-sonnet-4" },
       { label: "search_rdw_products",  brief: "List Redwire's current products and contracts...",          model: "claude-sonnet-4" },
       { label: "search_market_trends", brief: "Summarize in-space manufacturing market trends 2024-2026...", model: "claude-sonnet-4" }
     ])
  2. await_subtask("search_competitors")
  3. await_subtask("search_rdw_products")
  4. await_subtask("search_market_trends")
  5. Synthesize all three results into final analysis
```

All three search subtasks run in parallel. Orchestrator unblocks when all three complete. Total wall time ≈ max(individual subtask times) instead of sum.

---

### UI Sketch

Parent thread message:

```
🤖 Orchestrating 3 subtasks...
  ▶ search_competitors    [claude-sonnet-4] ✅ complete
  ▶ search_rdw_products   [claude-sonnet-4] ⏳ running...
  ▶ search_market_trends  [claude-sonnet-4] ⏳ running...
```

Each subtask row is expandable to show its full token stream (collapsed by default).

---

## Implementation Phases

If this ever moves to active development, suggested phasing:

### Phase 1 — Foundation
- `subagent_sessions` table
- `spawn_subtask` tool (single, sequential)
- SSE events: `SubagentSpawned`, `SubagentComplete`, `SubagentFailed`
- Basic UI inline display (no expansion)

### Phase 2 — Parallel & Model Routing
- `spawn_batch` tool
- Per-subagent `model` override
- Per-persona subtask model default in settings
- Concurrent semaphore pool

### Phase 3 — Resilience
- Self-repair / retry logic
- Mark-failed on restart (startup sweep)
- AD-9.2 integration: reconnect handler re-awaits running subtasks

### Phase 4 — Full UI
- Expandable subtask token stream panels
- Subtask progress in thread header
- Session history: view past subtask trees

---

## Open Questions

1. **Semaphore strategy**: shared per-thread pool vs. dedicated subagent pool? Dedicated pool is cleaner but adds config complexity.
2. **Context budget**: how much of the parent context to include in the subtask brief? Zed passes full context; IronClaw uses explicit briefs. Explicit briefs seem better for token efficiency.
3. **Result format**: should subagent results be raw text, structured JSON, or markdown? Probably persona-configurable.
4. **Subagent memory**: should subagents have access to the parent persona's memory tools? Probably yes by default, opt-out per spawn.
5. **Max depth = 2**: is this right? Could be a persona config. Starting conservative.
6. **Toolset inheritance**: subagents inherit parent's MCP tools by default. Should there be an opt-out in `spawn_subtask`?

---

## References

- Zed source: `crates/assistant2/src/thread_environment.rs`
- Zed open issue: #52042 (per-subagent model routing)
- IronClaw: `ThreadTree`, `spawn_subtask()`, `spawn_batch()`, `DefaultSelfRepair`, `PolicyEngine`
- agent-deck AD-9.2 — SSE Reconnection & Resilience
- agent-deck AD-9.7 — Long-running tool call progress indicators

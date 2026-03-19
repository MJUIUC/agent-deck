# Run-Loop Hardening — Agent Architecture Alignment

## Context

This document captures the technical debt in `services/agent.rs` identified by comparing it
against Zed's `crates/agent/src/thread.rs`, which solves the same class of problems in a more
robust way. It is scoped as a hardening pass within **Phase 5**, sitting between Story 5.1
(run management and cancellation) and Story 5.2 (memory tools), since the cancellation work
already touches the run-loop boundary.

The goal is not to rewrite the agent from scratch. It is to introduce the structural patterns
from Zed incrementally, in an order that keeps the server shippable at each step.

---

## What Zed Does That We Don't

### 1. Typed event stream instead of a flat token accumulator

**Zed:** `run_turn_internal` drives a `ThreadEventStream` that emits strongly-typed variants:
`AgentText`, `AgentThinking`, `ToolCall`, `ToolCallUpdate`, `Retry`, `Stop`. Each variant is
handled by a dedicated method (`handle_text_event`, `handle_tool_use_event`, etc.).

**Us:** `generation_loop` streams raw `chunk.delta` strings into a `turn_text: String` and
accumulates tool-call fragments in a `Vec<PendingToolCall>`. All of the logic lives in a single
700-line async function.

**Why it matters:** Adding any new streamed content type (thinking/reasoning tokens, subagent
events, structured diffs) requires invasive edits to the monolithic loop. The flat string
accumulator also makes it harder to reason about partial-cancel state.

---

### 2. `AgentTool` trait instead of a `match` on strings

**Zed:** Tools implement an `AgentTool` trait with associated `Input`/`Output` types, a JSON
schema, `run`, and `replay` methods. They are registered into a `Vec<Box<dyn AnyAgentTool>>` on
the thread at startup.

**Us:** `execute_tool` dispatches with:

```rust
match tc.name.as_str() {
    TOOL_SAVE_MEMORY => { ... }
    TOOL_RECALL_MEMORY => { ... }
    name if name.contains("__") => { /* MCP routing */ }
    unknown => { /* warn + no-op */ }
}
```

Every new built-in tool adds another match arm and more code to an already large function.
There is no schema, no type safety on inputs, and no `replay` path for restoring tool calls
from the database.

---

### 3. Structured parse-error recovery

**Zed:** When tool-call JSON fails to parse, `handle_tool_use_json_parse_error_event` injects
a structured error message back into the thread history so the model can see what went wrong
and recover.

**Us:** Tool argument parsing silently falls back to an empty object:

```rust
let args: Value = serde_json::from_str(&tc.args).unwrap_or_else(|_| serde_json::json!({}));
```

If the model sends malformed JSON the tool silently receives no arguments, produces a confusing
result, and the conversation stalls without any signal that a parse failure occurred.

---

### 4. Retry strategy

**Zed:** `retry_strategy_for` inspects the error type and returns either an
`ExponentialBackoff { initial_delay, max_attempts }` or a `Fixed { delay, max_attempts }` value.
The turn loop honours it transparently.

**Us:** Any provider error is immediately fatal. A transient network hiccup or a provider
rate-limit (`429`) kills the run and requires the user to manually resend.

---

### 5. Cancellation is structural, not a shared token lookup

**Zed:** Cancellation state lives inside `RunningTurn`, which is stored directly on the
`Thread` entity. Dropping `RunningTurn` drops the `Task` and the GPUI executor cancels the
underlying future. There is no separate shared token that can drift out of sync with the
running task.

**Us:** (Already partially addressed in Story 5.1.) The P0 race — where a queued message
replaces the active token so cancel hits the wrong task — is documented in
`RUNLOOP_CANCEL_REDESIGN.md` and `CANCELLATION-FIXES.md`. The structural fix is to tie the
cancel token lifetime directly to the spawned task handle rather than through the `RunState`
mutex. Full details are in those documents; the work here builds on top of whatever Story 5.1
lands.

---

### 6. `let _ =` on fallible SSE sends

**Zed:** Errors are never silently discarded; `.log_err()` or explicit handling is always used.

**Us:** Several SSE event sends in `run_inner` silently drop errors:

```rust
let _ = state.send_global_event(GlobalEvent::TitleUpdated { ... });
let _ = state.send_global_event(GlobalEvent::ThreadUpdated { ... });
```

If the broadcast channel is closed or full, the failure is invisible. At minimum these should
log a warning.

---

## Stories

Each story below is self-contained and can be merged independently. They are ordered so that
each one improves the codebase without depending on the next.

---

### Story H.1 — Fix silent error discards and parse fallbacks

**Branch:** `feature/phase5-runloop-h1-error-handling`

The smallest and safest change. No structural refactoring — purely replacing silent failure
modes with visible ones.

**Changes:**

1. Replace every `let _ = state.send_global_event(...)` in `run_inner` with a `warn!()` on
   failure. The broadcast channel is a `tokio::sync::broadcast`; `SendError` means no
   receivers, which is fine and can be logged at `debug` level. A full channel is a real
   problem and should be `warn!`.

2. Replace every `unwrap_or_else(|_| serde_json::json!({}))` in `execute_tool` with a
   `serde_json::from_str(&tc.args)` that returns a structured error string back to the LLM on
   parse failure, matching how Zed's `handle_tool_use_json_parse_error_event` works. The
   error message should include the tool name and the raw args so the model can diagnose what
   it sent.

3. Audit all other `unwrap_or_else` and `let _ =` uses in `agent.rs` and apply the same
   treatment.

**Acceptance criteria:**
- [ ] No `let _ =` on fallible operations anywhere in `services/agent.rs`
- [ ] No `unwrap_or_else` that silently swallows a real error
- [ ] When a tool receives malformed JSON args, the LLM receives a message like:
      `"Tool 'save_memory' received invalid JSON arguments: expected ident at line 1 column 2. Raw args: {bad}"`
- [ ] Unit test: `execute_tool` with malformed args returns an error string, not an empty-args result
- [ ] `cargo clippy` passes with no new warnings

---

### Story H.2 — Extract `StreamEvent` enum and split the generation loop

**Branch:** `feature/phase5-runloop-h2-stream-events`

Introduce a typed event enum for what comes out of the provider stream. Split
`generation_loop` into a streaming phase and a tool-execution phase. The loop itself becomes a
thin coordinator that matches on event batches.

**New type:**

```rust
enum StreamEvent {
    TextDelta(String),
    ToolCallFragment {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        args_fragment: Option<String>,
    },
    Done,
}
```

**New structure:**

```
generation_loop
  └─ stream_one_turn(provider, messages, tools, cancel)
       → Result<TurnResult>
            TurnResult { text: String, tool_calls: Vec<ResolvedToolCall> }
  └─ execute_tool_calls(state, tool_calls, ..., cancel)
       → Vec<ToolCallResult>
```

`stream_one_turn` owns the `tokio::select!` loop and emits `StreamEvent`s to a local
accumulator. It returns a `TurnResult` when the stream closes. `generation_loop` coordinates
between turns: if `TurnResult.tool_calls` is non-empty, call `execute_tool_calls`, push the
results onto `messages`, and loop.

This mirrors the separation between `run_turn_internal` (drives the stream and emits events)
and `handle_tool_use_event` / `run_tool` (dispatches and collects tool results) in Zed.

**Acceptance criteria:**
- [ ] `generation_loop` is ≤ 80 lines; the streaming accumulation logic lives in `stream_one_turn`
- [ ] `execute_tool_calls` is a separate `async fn` that takes a `Vec<ResolvedToolCall>`
- [ ] All existing behaviour is preserved: cancellation at each checkpoint, `MAX_TOOL_ROUNDS` guard,
      `persist_tool_message` calls, SSE token emission
- [ ] Existing unit tests pass without modification
- [ ] New unit test: `stream_one_turn` correctly accumulates multi-chunk tool call fragments
      into a single `ResolvedToolCall`

---

### Story H.3 — `AgentTool` trait for built-in tools

**Branch:** `feature/phase5-runloop-h3-tool-trait`

Replace the `match tc.name.as_str()` dispatch in `execute_tool` with a registered trait object
system, aligned with how Zed's `AnyAgentTool` works.

**New trait:**

```rust
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    async fn run(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<String>;
}
```

`ToolContext` carries whatever the tool needs from the outer environment: `pool`, `user_id`,
`persona_id`, `thread_id`. This avoids threading all those arguments through every call site.

**Concrete implementations (in `services/tools/` directory):**
- `SaveMemoryTool` (extracted from current `TOOL_SAVE_MEMORY` arm)
- `RecallMemoryTool` (extracted from current `TOOL_RECALL_MEMORY` arm)

MCP tools remain outside this trait for now — they are dynamically discovered and routed
differently. The trait covers only statically-registered built-in tools.

**Tool registry on `AppState`:**

```rust
pub built_in_tools: Vec<Arc<dyn AgentTool>>,
```

Populated at server startup in `main.rs`. `run_inner` passes the slice into `generation_loop`.
`execute_tool_calls` checks the registry first; falls through to MCP routing if no match.

**Acceptance criteria:**
- [ ] `execute_tool` match arms for `save_memory` and `recall_memory` are deleted
- [ ] Both tools are implemented as structs in `server/src/services/tools/`
- [ ] `AgentTool::input_schema` returns a valid JSON Schema object for each tool
- [ ] Tool schema is injected into the LLM request alongside MCP tool schemas
- [ ] Adding a new built-in tool requires only: implement the trait, register at startup
- [ ] Existing memory tool unit tests pass
- [ ] New unit test: the tool registry lookup correctly resolves by name

---

### Story H.4 — Retry strategy for transient provider errors

**Branch:** `feature/phase5-runloop-h4-retry`

Add a retry layer to `stream_one_turn` (introduced in H.2) so transient failures do not kill
the run.

**New type (modelled on Zed's `RetryStrategy`):**

```rust
enum RetryStrategy {
    ExponentialBackoff { initial_delay: Duration, max_attempts: u32 },
    Fixed { delay: Duration, max_attempts: u32 },
    None,
}
```

**`retry_strategy_for(error: &anyhow::Error) -> RetryStrategy`:**

| Error class | Strategy |
|---|---|
| HTTP 429 (rate limit) | `ExponentialBackoff { 2s, 4 }` |
| HTTP 5xx (provider error) | `ExponentialBackoff { 1s, 3 }` |
| Network timeout / connection reset | `Fixed { 1s, 2 }` |
| HTTP 4xx (bad request, auth) | `None` — surface immediately |
| Stream parse error | `None` — surface immediately |

A `Retry` SSE event (`ThreadEvent::Retry { attempt, max_attempts }`) is emitted before each
retry so the client can show "Retrying (1/3)…" in the UI.

Cancellation is checked before each retry attempt — if the user hits stop during a backoff
sleep, the retry is abandoned immediately.

**Acceptance criteria:**
- [ ] A simulated HTTP 429 causes up to 4 attempts with exponential backoff
- [ ] A simulated HTTP 401 is not retried and surfaces immediately as an SSE error event
- [ ] Cancel during backoff sleep aborts the retry without waiting for the sleep to expire
- [ ] `ThreadEvent::Retry` SSE event is emitted to the client before each retry
- [ ] `cargo test` passes; unit test for `retry_strategy_for` covers all error classes

---

## Sequencing and Dependencies

```
Story 5.1 (cancellation — in progress)
    │
    ├─► H.1  (error handling — no structural deps, safe to start now)
    │
    └─► H.2  (stream event split — depends on 5.1 for cancel token shape)
             │
             ├─► H.3  (tool trait — depends on H.2 for ToolContext)
             │
             └─► H.4  (retry — depends on H.2 for stream_one_turn)
```

H.1 can be opened as a PR immediately alongside Story 5.1 since it touches no structural
boundaries. H.2 should wait for 5.1 to merge so the cancel token shape is stable. H.3 and H.4
can proceed in parallel after H.2 merges.

---

## What Is Not In Scope

- **Subagent / nesting support.** Zed's `SubagentContext`, `MAX_SUBAGENT_DEPTH`, and
  `ThreadEnvironment` trait enable threads to spawn child threads. Agent-deck has no equivalent
  concept today and it is not planned until Phase 8 ("MCP Depth"). These patterns are worth
  keeping in mind but should not be designed in now.

- **Thinking / reasoning tokens.** Zed's `AgentThinking` and `RedactedThinking` event variants
  handle extended thinking from Anthropic models. We do not yet support Anthropic; this is
  deferred to Phase 8 or whenever a thinking-capable model is added to the provider layer.

- **`replay` path.** Zed's `AgentTool::replay` restores tool calls from a persisted thread
  without re-executing them. We persist tool messages as hidden rows and replay by re-fetching
  history. The `replay` pattern would clean this up but is not a correctness issue today.

- **Full entity-based state machine.** Zed wraps `Thread` as a GPUI `Entity<Thread>`, which
  gives reactive re-rendering, `cx.notify()`, typed subscriptions, and structured observe
  callbacks. Agent-deck's server is Axum + Tokio, not GPUI. We adopt the *patterns* (typed
  events, trait-based tools, structural cancellation) but not the GPUI runtime itself.

---

## Acceptance Criteria for the Full Hardening Pass

When all four stories are merged:

- [ ] No `let _ =` on fallible operations in `services/agent.rs` or `services/tools/`
- [ ] No `unwrap_or_else` that silently hides a real error
- [ ] Tool argument parse failures return a structured error to the LLM instead of empty args
- [ ] `generation_loop` is a thin coordinator; streaming and tool execution live in separate functions
- [ ] Built-in tools are registered via `AgentTool` trait; `execute_tool` match arms are gone
- [ ] Transient provider errors (429, 5xx, network) trigger automatic retry with backoff
- [ ] Retry progress is visible to the client via SSE
- [ ] Cancellation during backoff sleep aborts immediately
- [ ] All existing unit and integration tests pass
- [ ] `cargo clippy` passes with no warnings
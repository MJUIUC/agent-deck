# Tool Call Grouping: Unified Processing View

## Overview

Currently, tool calls in agent-deck are surfaced as individual, disconnected UI elements — a
spinning "Running tool…" indicator per call during streaming, and a separate `ToolActivityBubble`
persisted in history for each call/result pair. The goal of this feature is to collapse all tool
activity across an entire agent response into a **single expandable "Processing" block**, with the
final assistant text response appearing in a clean, separate chat bubble after all tool rounds
complete.

---

## Prerequisites — Already Implemented

The `chat_segment` infrastructure described in the now-deleted `chat_segment_split.md` plan is
fully live in the codebase. `generation_loop` already:

- Persists pre-tool text as individual `chat_segment` messages before dispatching tools.
- Emits `ChatSegment` SSE events to anchor streaming text to a real DB row.
- Uses `all_content` / `last_turn_text` accumulators (not a single `final_content` blob).
- Merges consecutive assistant rows in `context::assemble` before sending to the LLM.

This is the foundation this feature builds on. The `chat_segment_split.md` plan file is obsolete
and has been deleted.

---

## Current Architecture

### Backend (`server/src/services/agent.rs`)

The agent loop runs in `generation_loop()`, which alternates `stream_one_turn()` and
`execute_tool_calls()` up to `MAX_TOOL_ROUNDS = 50`. Tools within a round are executed **serially**
in a `for tc in calls { ... }` loop.

SSE events emitted during a multi-tool-round turn today:

```
token (×N)                         ← preamble text streaming
chat_segment                       ← preamble committed to DB
tool_start { tool_name }           ← before each tool (no round, no input info)
  tool_activity { role=assistant } ← call message persisted
  tool_activity { role=tool }      ← result message persisted
tool_start { tool_name }           ← next tool in same round…
  tool_activity …
token (×N)                         ← reasoning text before next round
tool_start …                       ← round 2 begins
  …
message_complete                   ← final assistant message
```

Key gaps in the current backend:
- `ToolStart` carries only `tool_name` — no round index, no call ID, no input arguments.
- `ToolActivity` has no round/group identifier linking it back to its `ToolStart`.
- There is no explicit "all tools in this round are done" event.
- Tools execute one at a time; no parallelism.

### Frontend (`web/src/`)

The streaming state machine in `useMessageStore` tracks `{ status: "streaming", entries:
StreamingEntry[] }` where a `StreamingEntry` is either `{ type: "text"; content: string }` or
`{ type: "tool_call"; tool_name: string; status: "in_progress" | "completed" }`.

Each entry renders independently in `ChatView.tsx`:
- `text` → `<StreamingBubble>` (one per sub-turn)
- `tool_call` in-progress → `<ToolExecutingIndicator>` ("Running tool…" spinner)
- `tool_call` completed → invisible

In persisted history, every tool message (`source: "tool"`) is its own `<ToolActivityBubble>`.

### The Problems

1. **No grouping.** A 3-round turn with 2 tools per round produces 12 individual tool bubbles.
2. **No tool detail.** `ToolStart` only names the tool. The UI cannot show "Editing `src/main.rs`".
3. **No round boundary signal.** The frontend can't know when a round ends and the LLM is being
   called again.
4. **Final text is visually indistinguishable** from intermediate sub-turn text.
5. **Serial execution** means total tool latency is the sum of all individual tool latencies.

---

## The Plan

### Settled Decisions

| Question | Decision |
|---|---|
| Computed label string? | No. Send `input_preview` (flat JSON key-value) instead; frontend renders `tool_name · value`. |
| Parallel tool execution? | Yes — implement in Phase A alongside the other backend changes. |
| Pre-tool preamble text? | Existing behavior is correct — `chat_segment` commits it as its own bubble. |
| Reasoning text between rounds? | Fold it into the ProcessingBlock as a "Reasoning" section. |
| Cancellation state? | `ProcessingRound.status` includes `"cancelled"`; block shows dashed border + grey icon. |
| Old message backward compat? | Treat old messages as incompatible. Threads will be deleted during development. |

---

### Part 1 — Backend Changes

#### 1.1 Add `tool_call_id`, `round`, and `input_preview` to `ToolStart`

Generate a stable `tool_call_id` (UUID) for each tool invocation in `generation_loop` before
calling `execute_tool_calls`. Pass a `round` counter (1-based) alongside it.

```
Current:  { "event": "tool_start", "tool_name": "edit_file" }

New:      { "event": "tool_start",
            "tool_name": "edit_file",
            "tool_call_id": "call_abc123",
            "round": 1,
            "input_preview": { "path": "src/main.rs" } }
```

`input_preview` is a small flat JSON object containing the single most useful argument for that
tool type — computed by the backend, which already parses the args to execute the tool:

| Tool | `input_preview` key |
|---|---|
| `edit_file` / `read_file` / `delete_file` | `{ "path": "..." }` |
| `bash` / shell tools | `{ "command": "first 80 chars of command" }` |
| `save_memory` / `recall_memory` | `{ "query": "..." }` |
| MCP tools (`tag__name`) | `{ "tool": "name", "server": "tag" }` |
| Unknown / unrecognized | `{}` |

The frontend renders: `tool_name · input_preview[primary_key]` — it does not need to know which
key to look for, just renders the first (and typically only) value.

#### 1.2 Add `tool_call_id` and `round` to `ToolActivity`

```
New:  { "event": "tool_activity", "id": "...", "role": "tool", "content": "...",
        "tool_call_id": "call_abc123", "round": 1, ... }
```

This connects each activity message back to its originating `ToolStart`.

#### 1.3 Add a `tool_round_complete` event

Emit after `execute_tool_calls()` returns and before the next LLM call or final message:

```
{ "event": "tool_round_complete", "round": 1, "tool_count": 3 }
```

Gives the frontend a clean signal that the round is done and the agent is looping.

#### 1.4 Parallelize `execute_tool_calls`

The current `for tc in calls { ... }` serial loop can be replaced with
`futures::future::join_all` since each iteration is self-contained:

- All futures borrow `state: &AppState` in the same scope — no `Arc` cloning required.
- Each future returns `(result_content: String, hidden_ids: Vec<String>)` instead of mutating
  `run_context` directly.
- After `join_all`, flatten `hidden_ids` into `run_context.hidden_message_ids` and collect
  `results` — `join_all` preserves input order, so the `zip(tool_calls, results)` in
  `generation_loop` keeps working unchanged.
- The per-loop `is_cancelled` check moves inside each future as an early return, rather than
  breaking the outer loop. Remaining futures that haven't started simply short-circuit
  immediately when they check cancellation.
- `ToolStart` SSE events for all tools in the round fire before dispatch (collect them all
  upfront), so the frontend sees the full round's tool list at once rather than one-by-one.

With `tool_call_id` already being threaded through, the out-of-order completion case (a faster
tool finishing before a slower one) is handled cleanly — each `ToolActivity` event carries its
own `tool_call_id` so the frontend correlates by ID, not position.

#### 1.5 Store `tool_call_id` and `tool_round` in the DB

Add two nullable columns to the `messages` table via migration:

- `tool_call_id TEXT` — set on both the assistant call row and the tool result row.
- `tool_round INTEGER` — 1-based round index for this turn.

Pass these through `persist_tool_message()`. With these columns, history loads can reconstruct
grouping without relying on SSE event ordering.

---

### Part 2 — Frontend Changes

#### 2.1 Extend `StreamingEntry` to replace individual tool entries

```typescript
export type StreamingEntry =
  | { type: "text"; content: string }
  | { type: "processing"; rounds: ProcessingRound[] }

export interface ProcessingRound {
  round: number;
  tools: ToolCallEntry[];
  status: "in_progress" | "completed" | "cancelled";
  reasoning?: string;  // text streamed between this round and the next
}

export interface ToolCallEntry {
  tool_call_id: string;
  tool_name: string;
  input_preview: Record<string, string>;
  status: "in_progress" | "completed" | "cancelled";
  call_message_id: string | null;   // set when tool_activity role=assistant arrives
  result_message_id: string | null; // set when tool_activity role=tool arrives
}
```

There is exactly **one** `processing` entry per streaming phase (created on the first
`tool_start`) and zero or one `text` entry at the end (the final response).

#### 2.2 Update `useMessageStore` event handlers

| SSE event | Action |
|---|---|
| `tool_start` | If no `processing` entry exists, create one. Add `ToolCallEntry` (status: `in_progress`) to the matching round (create round if `round` is new). |
| `tool_activity` role=assistant | Set `call_message_id` on the matching entry by `tool_call_id`. |
| `tool_activity` role=tool | Set `result_message_id`, mark entry `completed`. |
| `tool_round_complete` | Mark matching `ProcessingRound` as `completed`. |
| `token` (after `tool_round_complete`) | If a new `tool_start` hasn't arrived yet, this is reasoning text — append to `rounds[last].reasoning`. |
| `tool_start` (new round after reasoning) | Create new round, clear the "is reasoning" state. |
| `chat_segment` | Existing behavior unchanged (commits preamble text as real message). |
| `message_complete` | Existing behavior unchanged (final message added, phase → idle). |
| `cancelled` | Mark all `in_progress` rounds and entries as `cancelled`. |

#### 2.3 New `<ProcessingBlock>` component

Replaces both `<ToolExecutingIndicator>` and `<ToolActivityBubble>`. Works in two modes:
streaming (driven by `ProcessingRound[]` from store) and static (driven by grouped history
messages). Expansion state is a local `useState` boolean — no store involvement needed.

**Collapsed (default while running):**
```
┌──────────────────────────────────────────────────────┐
│  ⟳  Processing…                               ▶      │
└──────────────────────────────────────────────────────┘
```

**Collapsed (all done):**
```
┌──────────────────────────────────────────────────────┐
│  ✓  Processing  ·  3 tools, 2 rounds          ▶      │
└──────────────────────────────────────────────────────┘
```

**Collapsed (cancelled):**
```
┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐
│  ✕  Stopped  ·  2 tools, 1 round             ▶      │
└ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘
```

**Expanded (multi-round with reasoning):**
```
┌──────────────────────────────────────────────────────┐
│  ✓  Processing  ·  3 tools, 2 rounds          ▼      │
├──────────────────────────────────────────────────────┤
│  Round 1  ✓                                          │
│    ✓  edit_file    ·  src/main.rs                    │
│    ✓  read_file    ·  Cargo.toml                     │
│  ─────────────────────────────────────────────────── │
│  Reasoning                                           │
│  The build failed due to a missing import. Let me    │
│  check the module declarations…                      │
│  ─────────────────────────────────────────────────── │
│  Round 2  ✓                                          │
│    ✓  bash         ·  cargo build                    │
└──────────────────────────────────────────────────────┘
```

Round headers are only shown when there is more than one round. The "Reasoning" divider is
only shown when `round.reasoning` is non-empty.

#### 2.4 Rendering order in `ChatView.tsx`

During streaming:

```
[persisted message bubbles]
[preamble bubble, if chat_segment was received]
[<ProcessingBlock> — if phase.processing exists]
[<StreamingBubble> — if phase.text exists (the final response)]
[<StreamingIndicator> bouncing dots — appended to StreamingBubble while streaming=true]
```

The `processing` entry and the `text` entry are visually distinct units. The final response
is never interleaved with tool activity.

#### 2.5 Grouped history view

When messages load from `GET /api/threads/:id/messages`, scan the `Message[]` array and
produce a `GroupedItem[]` union before rendering:

```typescript
type GroupedItem =
  | { type: "message"; message: Message }
  | { type: "tool_group"; executionId: string; rounds: ToolRoundGroup[] }

interface ToolRoundGroup {
  round: number;
  calls: Array<{ call: Message; result: Message | null }>;
}
```

Grouping logic:
1. Walk messages in order.
2. When a `source: "tool"` message is encountered, collect all consecutive tool messages
   sharing the same `execution_id`, grouped by `tool_round`.
3. Emit a single `tool_group` item for the entire run.
4. All other messages pass through as `message` items.

Render `tool_group` items with `<ProcessingBlock>` in static mode (all rounds completed).

This replaces the existing per-message `<ToolActivityBubble>` rows and the `show_tool_activity`
thread toggle, both of which become obsolete.

---

### Part 3 — Phased Delivery

#### Phase A — Backend (non-breaking)

All additions are additive — the existing frontend ignores unknown fields on SSE events.

1. Generate `tool_call_id` (UUID) per tool invocation in `generation_loop`.
2. Compute `input_preview` per tool in `execute_tool_calls` based on tool name + parsed args.
3. Add `tool_call_id`, `round`, `input_preview` to `ToolStart` SSE event.
4. Add `tool_call_id`, `round` to `ToolActivity` SSE event.
5. Emit `tool_round_complete` after each `execute_tool_calls()` call.
6. Parallelize `execute_tool_calls` using `futures::future::join_all`.
7. DB migration: add `tool_call_id` and `tool_round` columns to `messages`.
8. Pass new columns through `persist_tool_message()`.

#### Phase B — Frontend streaming UI

1. Extend `StreamingEntry` with `processing` variant and supporting types.
2. Update `useMessageStore` handlers for all new and updated SSE events.
3. Build `<ProcessingBlock>` component (streaming mode).
4. Wire `<ProcessingBlock>` into `ChatView.tsx` streaming render path, replacing
   `<ToolExecutingIndicator>`.

#### Phase C — Grouped history view

1. Implement `useGroupedMessages` grouping logic.
2. Build `<ProcessingBlock>` static mode.
3. Replace `<ToolActivityBubble>` rows with `<ProcessingBlock>` in static mode.
4. Remove `<ToolActivityBubble>`, `show_tool_activity` DB column, and related UI.
5. Update `GET /api/threads/:id/messages` response to include `tool_call_id` and `tool_round`
   fields on tool messages.

---

## Expected End-State Behavior

### Live streaming (multi-round tool-using response)

**Scenario:** agent produces a preamble, uses 2 tools in round 1, generates reasoning, uses 1
tool in round 2, then delivers the final answer.

1. User sends a message. Input field disables.

2. Agent streams preamble text — a streaming bubble appears left-aligned with bouncing dots.

3. First `tool_start` events arrive (multiple, now in parallel). The preamble bubble freezes;
   `chat_segment` commits it as a real message. A `<ProcessingBlock>` appears below it:

   ```
   ⟳  Processing…                                     ▶
   ```

4. Expanding the block shows tools arriving (both round 1 tools appear immediately since they
   were dispatched in parallel):

   ```
   ✓  Processing  ·  2 tools, round 1…               ▼
   ─────────────────────────────────────────────────────
   Round 1
     ⟳  edit_file   ·  src/main.rs
     ⟳  read_file   ·  Cargo.toml
   ```

5. Tools complete (near-simultaneously). Their rows update to ✓. `tool_round_complete` arrives
   for round 1. The agent calls the LLM again. Reasoning tokens stream into the block:

   ```
   ⟳  Processing…                                     ▼
   ─────────────────────────────────────────────────────
   Round 1  ✓
     ✓  edit_file   ·  src/main.rs
     ✓  read_file   ·  Cargo.toml
   ─────────────────────────────────────────────────────
   Reasoning
   The build failed because of a missing import. Let me
   check the module declarations…
   ```

6. Round 2 tool starts. A new round row appears in the block. Tool completes.
   `tool_round_complete` arrives for round 2. The block header updates:

   ```
   ✓  Processing  ·  3 tools, 2 rounds               ▼
   ```

7. The final response begins streaming. A **new, separate assistant bubble** appears below the
   `<ProcessingBlock>` — visually identical to any normal chat message:

   ```
   [preamble bubble]
   [ProcessingBlock ✓ — collapsed or expanded]
   [streaming bubble]  "Here's what I fixed…"   •••
   ```

8. `message_complete` fires. The streaming bubble hardens. Phase resets to `idle`. Input
   re-enables.

### In history (after reload)

```
[user message bubble]
[preamble assistant bubble]              ← from chat_segment
[ProcessingBlock — collapsed by default] ← all tool messages grouped by execution_id + round
[final answer assistant bubble]          ← from message_complete
```

The ProcessingBlock in history is identical to the completed streaming view. Starts collapsed.
Expandable to show all rounds and tool details.

### What disappears

- Individual `<ToolExecutingIndicator>` ("Running tool…") spinners — gone.
- Individual `<ToolActivityBubble>` rows per tool message — gone.
- The `show_tool_activity` toggle on thread settings — gone.
- Multiple floating `<StreamingBubble>` instances per sub-turn — consolidated into one final
  bubble plus the ProcessingBlock.

---

## Reference: Relevant Source Locations

| Concern | File |
|---|---|
| Agent loop / tool dispatch | `server/src/services/agent.rs` |
| SSE event definitions | `server/src/routes/sse.rs` |
| Message persistence | `server/src/models/message.rs` |
| Messages REST route | `server/src/routes/messages.rs` |
| SSE consumer / state machine | `web/src/stores/useMessageStore.ts` |
| SSE connection management | `web/src/stores/useSseStore.ts` |
| Message bubble rendering | `web/src/components/MessageBubble.tsx` |
| Chat view / streaming render | `web/src/components/ChatView.tsx` |
| Shared TypeScript types | `web/src/types/index.ts` |
| Token batching (RAF flush) | `web/src/utils/tokenBuffer.ts` |
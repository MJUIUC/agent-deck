# AD-7.3a — Unified Tool-Call Processing View

**Story:** 7.3a — Unified tool-call processing view (QoL)
**Branch:** `feature/phase7-tool-call-grouping`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 7
**Research doc:** `docs/PLAN/tool-call-grouping.md`

---

## Summary

All tool activity during an agent response is consolidated into a single expandable processing
bubble instead of individual "Running tool…" spinners and per-message `<ToolActivityBubble>` rows.
Tools within a round execute in parallel. The final assistant text response appears in a clean,
separate message bubble below the processing bubble. This removes clutter from both the live
streaming view and the persisted history view.

The implementation is split into two components: `ProcessingBlock` (the inner collapsible
accordion — header row, tool rows, reasoning sections) and `ProcessingBubble` (a thin shell that
mirrors the agent message bubble layout — persona avatar on the left, bubble container on the
right — with `ProcessingBlock` as its body). `ProcessingBubble` reuses the same CSS classes from
`MessageBubble.module.css` so it is visually indistinguishable from a regular agent bubble.

---

## Current State

### Backend

- `server/src/routes/sse.rs` — `ToolStart` carries only `tool_name: String`. `ToolActivity`
  carries persisted message fields but no round index or call ID. There is no
  "round complete" event.
- `server/src/services/agent.rs` — `execute_tool_calls` is a serial `for tc in calls { ... }`
  loop. `generation_loop` tracks a `tool_rounds` counter but never emits it to the frontend.
  `persist_tool_message` has no awareness of rounds or call IDs.
- `server/src/db/migrations/003_thread_show_tool_activity.sql` — added `show_tool_activity`
  column to `threads` to gate visibility of `<ToolActivityBubble>` rows.

### Frontend

- `web/src/types/index.ts` — `StreamingEntry` is `{ type: "text" } | { type: "tool_call"; tool_name; status }`. No grouping or round concept.
- `web/src/stores/useMessageStore.ts` — `tool_start` appends an individual `tool_call` entry;
  `tool_activity` (role=tool) marks all in-progress entries completed and appends a new `text`
  entry for the next sub-turn. No `tool_round_complete` handling.
- `web/src/components/MessageBubble.tsx` — exports `ToolActivityBubble` (collapsible per-message
  row, hidden behind `show_tool_activity`), `ToolExecutingIndicator` (inline spinner), and the
  `AgentAvatar` component (currently defined inline; needs to be exported for reuse by
  `ProcessingBubble`).
- `web/src/components/ChatView.tsx` — renders each `StreamingEntry` independently; completed
  `tool_call` entries are invisible; no grouping.

### Already in place

- `chat_segment` infrastructure is fully live: `generation_loop` persists pre-tool preamble text
  as its own `chat_segment` message and emits a `ChatSegment` SSE event before dispatching tools.
  This is the foundation the `<ProcessingBlock>` builds on — the preamble bubble is already its
  own separate message.

---

## Implementation Plan

### Task 1 — Extend SSE events and emit `ToolRoundComplete`

- **Files:** `server/src/routes/sse.rs`, `server/src/services/agent.rs`

Add three fields to `ThreadEvent::ToolStart`:
- `tool_call_id: String` — UUID generated in `generation_loop` for each `ResolvedToolCall`
  before `execute_tool_calls` is called.
- `round: u32` — the current value of `tool_rounds` (1-based; increment happens after
  `execute_tool_calls` returns, so the first round is `1`).
- `input_preview: serde_json::Value` — a flat JSON object with the single most useful argument
  for that tool type, computed from the parsed args in `execute_tool_calls`:

  | Tool name pattern | `input_preview` key |
  |---|---|
  | `edit_file`, `read_file`, `delete_file`, `create_file` | `{ "path": "..." }` |
  | `bash`, `shell`, `run_command` | `{ "command": "<first 80 chars>" }` |
  | `save_memory`, `recall_memory`, `delete_memory` | `{ "query": "..." }` or `{ "key": "..." }` |
  | MCP tools (`tag__name`) | `{ "tool": "name", "server": "tag" }` |
  | Unknown | `{}` |

Add two fields to `ThreadEvent::ToolActivity`:
- `tool_call_id: String` — same ID as the originating `ToolStart`, threaded through
  `persist_tool_message`.
- `round: u32` — same round index.

Add a new `ThreadEvent::ToolRoundComplete { round: u32, tool_count: u32 }` variant. Emit it in
`generation_loop` immediately after `execute_tool_calls` returns and before the loop iterates
(or before `break`ing on the final turn). Wire `event_name()` to return `"tool_round_complete"`.

The `tool_call_id` must be generated in `generation_loop` for each `ResolvedToolCall` (not
inside `execute_tool_calls`) so it can be threaded to both the SSE event and the DB persist.
Pass it alongside the `ResolvedToolCall` into `execute_tool_calls`.

### Task 2 — Parallelize `execute_tool_calls`

- **Files:** `server/src/services/agent.rs`

Replace the serial `for tc in calls { ... }` loop with `futures::future::join_all`. Each future:
- Borrows `state: &AppState` and `cancellation_rx` in the same scope — no `Arc` cloning needed.
- Checks `is_cancelled` immediately as an early return (`return ("".to_string(), vec![])` — the
  empty result is safe since the caller checks cancellation after `join_all` returns).
- Calls the built-in or MCP path as today.
- Returns `(result_content: String, hidden_ids: Vec<String>)` instead of mutating `run_context`.

After `join_all`, flatten all `hidden_ids` into `run_context.hidden_message_ids` and collect
`results` into `Vec<String>`. `join_all` preserves input order so the existing
`zip(turn.tool_calls.iter(), tool_results)` in `generation_loop` is unchanged.

Emit all `ToolStart` SSE events for the round **before** calling `join_all` so the frontend
sees the full round's tool list at once rather than one-by-one as each future begins.

### Task 3 — DB migration: tool grouping columns

- **Files:** `server/src/db/migrations/012_tool_call_grouping.sql`,
  `server/src/services/agent.rs` (`persist_tool_message`)

Migration adds two nullable columns to `messages`:

```sql
ALTER TABLE messages ADD COLUMN tool_call_id TEXT;
ALTER TABLE messages ADD COLUMN tool_round INTEGER;
```

Update `persist_tool_message` signature to accept `tool_call_id: Option<&str>` and
`tool_round: Option<u32>` and bind them in the INSERT.

`cargo sqlx prepare` must be run after this task is complete.

### Task 4 — Frontend types and streaming state machine

- **Files:** `web/src/types/index.ts`, `web/src/stores/useMessageStore.ts`

Replace the `tool_call` variant of `StreamingEntry` with a `processing` variant:

```typescript
export type StreamingEntry =
  | { type: "text"; content: string }
  | { type: "processing"; rounds: ProcessingRound[] }

export interface ProcessingRound {
  round: number;
  tools: ToolCallEntry[];
  status: "in_progress" | "completed" | "cancelled";
  reasoning: string; // text streamed between this round and the next
}

export interface ToolCallEntry {
  tool_call_id: string;
  tool_name: string;
  input_preview: Record<string, string>;
  status: "in_progress" | "completed" | "cancelled";
  call_message_id: string | null;
  result_message_id: string | null;
}
```

Update SSE event handler types to include the new fields on `SseToolStartEvent` and
`SseToolActivityEvent`, and add `SseToolRoundCompleteEvent`.

Update `useMessageStore` handlers:

| SSE event | New behaviour |
|---|---|
| `tool_start` | If no `processing` entry exists in `entries`, create one. Add a `ToolCallEntry` (status: `in_progress`) to the round matching `event.round`, creating the round if it does not exist. |
| `tool_activity` role=assistant | Find entry by `tool_call_id`; set `call_message_id`. |
| `tool_activity` role=tool | Find entry by `tool_call_id`; set `result_message_id`, mark entry `completed`. |
| `tool_round_complete` | Mark the matching `ProcessingRound` as `completed`. |
| `token` (while last round is completed, before next `tool_start`) | Append token to `rounds[last].reasoning`. |
| `cancelled` | Mark all `in_progress` rounds and entries as `cancelled`. |
| `chat_segment` | Existing `commitSegment` behaviour unchanged. |
| `message_complete` | Existing behaviour unchanged — phase resets to `idle`, discarding the transient `processing` entry. |

Remove the now-unreachable `beginToolCall` and `completeToolCall` helpers.

### Task 5 — Build `<ProcessingBlock>` and `<ProcessingBubble>`

- **Files:** `web/src/components/ProcessingBlock.tsx`,
  `web/src/components/ProcessingBlock.module.css`

**`ProcessingBlock`** — inner content only, no positioning or avatar logic.

Props: `rounds: ProcessingRound[]`

Internal expansion state is a `useState<boolean>` — not hoisted to any store.

*Collapsed header* (always visible):
- Left: status icon — `InProgress` spinner (animated) if any round is `in_progress`,
  `CheckmarkFilled` if all completed, `CloseFilled` if cancelled.
- Centre: label — "Processing…" while running; "Processing · N tools" or
  "Processing · N tools, M rounds" when done.
- Right: `ChevronRight` / `ChevronDown` toggle.

*Expanded body* (when open):
- One section per round. Round header (`Round N`) only shown when there is more than one round.
- Each tool row: status icon + `tool_name` + `·` + first value from `input_preview` (omitted if
  `input_preview` is empty).
- If `round.reasoning` is non-empty, render a "Reasoning" section between this round's tools and
  the next round header: subdued text, left border accent.
- Cancelled state: dashed border, muted palette throughout.

---

**`ProcessingBubble`** — the bubble shell; mirrors the agent message bubble layout exactly.

Props: `rounds: ProcessingRound[]`, `personaEmoji: string`, `personaName?: string`

Structure (copies the agent bubble row from `MessageBubble.tsx`):
- Outer row uses `styles.rowAgent` from `MessageBubble.module.css` (imported directly).
- Left slot: `<AgentAvatar>` — export this component from `MessageBubble.tsx` so it can be
  imported here.
- Right slot: a div using `styles.bubbleAgent` containing `<ProcessingBlock rounds={rounds} />`.
- Below the bubble: metadata line using `styles.meta` — shows `personaName ?? "Agent"` with no
  timestamp (rounds are in-progress or just finished; a timestamp isn't meaningful here).

`ProcessingBubble` lives in `ProcessingBlock.tsx` alongside `ProcessingBlock` since they are
always deployed together.

### Task 6 — Wire `<ProcessingBubble>` into the streaming render path

- **Files:** `web/src/components/ChatView.tsx`

Export `AgentAvatar` from `MessageBubble.tsx` (remove the `function` keyword's implicit
file-scope — just add `export` in front of it). `ProcessingBubble` imports it from there.

In the streaming entries render block, replace the existing logic with:

```tsx
{isStreaming && streamingEntries.map((entry, i) => {
  const isLast = i === streamingEntries.length - 1;
  if (entry.type === "text") {
    return (
      <StreamingBubble key={`text-${i}`} ... content={entry.content} streaming={isLast} />
    );
  }
  if (entry.type === "processing") {
    return (
      <ProcessingBubble
        key="processing"
        rounds={entry.rounds}
        personaEmoji={persona?.emoji ?? "🤖"}
        personaName={persona?.name}
      />
    );
  }
  return null;
})}
```

The `processing` entry always appears before any trailing `text` entry in `entries`, so the
visual order is: preamble bubble → `ProcessingBubble` → final `StreamingBubble`.

### Task 7 — Grouped history view and cleanup

- **Files:** `web/src/components/ChatView.tsx`, `web/src/components/MessageBubble.tsx`,
  `web/src/types/index.ts`, `web/src/stores/useMessageStore.ts`

Add a `useGroupedMessages` selector (co-located in `useMessageStore.ts` or as a standalone
`useMemo` in `ChatView.tsx`) that transforms `Message[]` into `GroupedItem[]`:

```typescript
type GroupedItem =
  | { type: "message"; message: Message }
  | { type: "tool_group"; executionId: string; rounds: ProcessingRound[] }
```

Grouping logic: walk messages in order; when a `source: "tool"` message is encountered, collect
all consecutive tool messages sharing the same `execution_id`, pair assistant-role and tool-role
rows by `tool_call_id`, and build a `ProcessingRound[]` grouped by `tool_round` (treating `null`
as round `1`). Emit one `tool_group` item. All other messages pass through as `message` items.

The `ToolCallEntry` fields for the static history case:
- `tool_call_id`: from the message's `tool_call_id` column (or generated from message `id` as
  fallback for old rows without the column).
- `tool_name` / `input_preview`: not stored on the `Message` — omit `input_preview`, derive
  `tool_name` by parsing the first line of the call message content
  (format is `**Tool call:** \`name\``). If parsing fails, fall back to `"tool"`.
- `status`: always `"completed"` for history rows.
- `call_message_id` / `result_message_id`: the paired message IDs.

In `ChatView.tsx`, replace the existing tool-message render path with:

```tsx
if (item.type === "tool_group") {
  return (
    <ProcessingBubble
      key={item.executionId}
      rounds={item.rounds}
      personaEmoji={persona?.emoji ?? "🤖"}
      personaName={persona?.name}
    />
  );
}
```

Remove:
- `ToolActivityBubble` export and its render path from `MessageBubble.tsx`.
- `ToolExecutingIndicator` export from `MessageBubble.tsx`.
- The `show_tool_activity` field from the `Thread` type in `web/src/types/index.ts`.
- The `show_tool_activity` toggle from the thread config UI (wherever it is rendered).
- References to `show_tool_activity` in `useThreadStore` and `client.ts`.

The `show_tool_activity` DB column on `threads` can be left in place (no migration needed to
drop it); it simply becomes unused and can be cleaned up in a future housekeeping pass.

---

### Schema changes

- **Migration `012_tool_call_grouping.sql`**: adds `tool_call_id TEXT` and `tool_round INTEGER`
  (both nullable) to the `messages` table.
- `persist_tool_message` in `server/src/services/agent.rs` is updated to accept and store both
  values.
- `cargo sqlx prepare` must be run after Task 3 is complete and before any frontend query
  changes that touch message fields.

---

### Parallelisation note

**Round 1 — backend (Tasks 1 + 2 + 3 in parallel):**
- Sub-agent A: Tasks 1 + 2 — both live in `agent.rs` and `sse.rs`; assign to one agent.
- Sub-agent B: Task 3 — migration file and `persist_tool_message` signature change only;
  no overlap with Sub-agent A's changes to `execute_tool_calls` logic.

**Round 2 — frontend core (Tasks 4 + 5 in parallel):**
- Sub-agent C: Task 4 — `types/index.ts` and `useMessageStore.ts` only.
- Sub-agent D: Task 5 — `ProcessingBlock.tsx` and `ProcessingBlock.module.css` only. Also
  exports `AgentAvatar` from `MessageBubble.tsx` (a one-line change, safe to do here since
  Sub-agent E will read `MessageBubble.tsx` after this agent finishes).
- These share no files and all props interfaces are defined in this plan.

**Round 3 — wire-up and cleanup (Tasks 6 + 7 in one agent pass):**
- Sub-agent E: Tasks 6 + 7 — `ChatView.tsx`, `MessageBubble.tsx` cleanup, `types/index.ts`
  cleanup, store cleanup.
- Depends on Tasks 4 + 5 being complete (needs updated store shape, `ProcessingBubble`, and the
  exported `AgentAvatar`).

---

## Acceptance Criteria

- [ ] All tool calls during an agent response are grouped into a single `<ProcessingBlock>`; no
      individual "Running tool…" spinners appear
- [ ] The `<ProcessingBlock>` is collapsed by default, showing a spinner and "Processing…" while
      any tool round is executing
- [ ] Expanding the block shows each tool row as `tool_name · input_preview_value` with a
      per-tool status icon transitioning from spinner to checkmark on completion
- [ ] Tools within a round execute in parallel — multiple tool rows appear simultaneously with
      `in_progress` status in the expanded view
- [ ] Reasoning text generated between tool rounds appears as a "Reasoning" section inside the
      expanded `<ProcessingBlock>`, not as a separate bubble
- [ ] After all tool rounds complete, the final assistant response appears in a separate message
      bubble below the `<ProcessingBlock>`
- [ ] Stopping the agent mid-turn renders the `<ProcessingBlock>` in a cancelled state (dashed
      border, grey/muted icon and text)
- [ ] In history view, all tool messages for a given execution load as a single grouped
      `<ProcessingBlock>`, collapsed by default, with the same round/tool breakdown as the live
      streaming view
- [ ] `<ToolActivityBubble>` is removed; the `show_tool_activity` thread toggle is removed from
      the settings UI

---

## Human Review Instructions

---

## Approval

- [x] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off

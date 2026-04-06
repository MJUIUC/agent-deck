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

**Prerequisites:** Server running on port 7474 (`cargo run` in `server/`). Frontend dev server running (`npm run dev` in `web/`). At least one provider configured with a model that supports tool use. At least one MCP server or built-in tool (e.g. `bash`, `read_file`) available.

**Steps:**

1. Open any thread and send a message that will trigger tool use (e.g. "Read the contents of /tmp and tell me what's there" with the `bash` or `read_file` tool available).

2. **While streaming:**
   - **Expected:** A single `<ProcessingBubble>` appears below any preamble text, collapsed by default, showing a spinning icon and "Processing…". No individual "Running tool…" rows.
   - **Failure sign:** Multiple separate tool rows, or the old `<ToolActivityBubble>` format.

3. Click the collapse toggle to expand the `<ProcessingBlock>` while tools are running.
   - **Expected:** Each tool row shows a spinning icon + `tool_name · input_preview_value`. Multiple tools in the same round appear simultaneously (parallel execution).
   - **Failure sign:** Tools appear one at a time with a delay between them.

4. Wait for tools to complete.
   - **Expected:** Each tool row's spinner becomes a green checkmark. The "▶ output" toggle appears on each completed row.
   - Click "▶ output" on a row — **Expected:** A collapsible monospace block shows the raw tool output. Long outputs truncate at ~300 chars with a "show more" affordance.

5. If the agent makes multiple tool rounds (e.g. reads a file then edits it), expand the block.
   - **Expected:** "Round 1", "Round 2" headers appear. Any reasoning text between rounds appears as a subdued "Reasoning" section with a left-border accent.

6. Wait for the full response to complete.
   - **Expected:** The final assistant text appears in a **separate** message bubble below the `<ProcessingBlock>`. The block header updates to "Processing · N tools" with a green checkmark icon.
   - **After streaming ends:** The `<ProcessingBlock>` remains visible in the conversation (does not disappear). Reload the page — **Expected:** The block still appears in history, collapsed.

7. Start another agent run that uses tools. While tools are executing, click **Stop**.
   - **Expected:** The `<ProcessingBlock>` immediately renders with a dashed border, grey/muted icon, and "Stopped" label. It persists after streaming ends (does not disappear).

8. Reload the page and scroll through history.
   - **Expected:** All tool call groups appear as collapsed `<ProcessingBlock>` bubbles (not raw message rows). Expanding them shows the tool name(s) with checkmark status and "▶ output" toggles.

9. Open Settings → Thread Config on any thread.
   - **Expected:** The `Show tool activity` toggle is **gone**. No references to it in the UI.

**Server log spot-check (optional):**
```
grep "tool_round_complete\|tool_start" ~/.agent-deck/server.log | tail -20
```
Should show `tool_start` events with `round` and `tool_call_id` fields, followed by `tool_round_complete` events.

**Successful outcome:** A clean, single processing bubble per agent turn — no clutter, tool output accessible on demand, history matches the streaming view.
**Failure signs:** Multiple individual tool rows, missing ProcessingBlock after streaming ends, tool output not expandable, show_tool_activity toggle still present.

## Addendum — Post-implementation fixes and enhancements

### Task 8 — Surface tool messages in history regardless of visibility

**Files:** `web/src/components/ChatView.tsx`

Tool call and result messages are stored with `visibility: "hidden"` on the server to prevent
them from surfacing as raw chat bubbles. However, the `processedItems` grouping logic in
`ChatView.tsx` relies on `source: "tool"` messages being present in `visibleMessages`. Because
the current filter excludes all hidden messages, tool groups never appear in history — the
`<ProcessingBlock>` vanishes the moment streaming ends.

Fix: include `source: "tool"` messages in `visibleMessages` regardless of their `visibility`
field. All other hidden messages (system events, etc.) remain filtered out.

```tsx
const visibleMessages = messages.filter((m) => {
  if (m.visibility === "hidden" && m.source !== "tool") return false;
  return true;
});
```

---

### Task 9 — Capture and display tool result content (Zed-style)

**Files:** `web/src/types/index.ts`, `web/src/stores/useMessageStore.ts`,
`web/src/components/ProcessingBlock.tsx`, `web/src/components/ProcessingBlock.module.css`

The `SseToolActivityEvent` already carries the full `content` of each tool result (it fires with
`role: "tool"` when a tool finishes). Currently only the message ID is captured via
`completeToolCallEntry`; the content is discarded. This means users cannot see what a tool
actually returned.

**Type change** — add `result_content` to `ToolCallEntry`:

```typescript
interface ToolCallEntry {
  tool_call_id: string;
  tool_name: string;
  input_preview: Record<string, string>;
  status: "in_progress" | "completed" | "cancelled";
  call_message_id: string | null;
  result_message_id: string | null;
  result_content: string | null;   // ← new: tool output from tool_activity SSE
}
```

**Store change** — update `completeToolCallEntry` (called from `handleToolActivity` when
`role === "tool"`) to also accept and store the result `content` from the SSE event. Pass it
from `useSseStore.ts` through to the store method.

**UI change** — in `ToolRow` inside `ProcessingBlock.tsx`, when `result_content` is non-null and
the tool is completed, render a collapsible result section beneath the tool name row. Keep it
collapsed by default; a small toggle (e.g. `▶ output`) expands it inline. Style it with a
left-border accent and monospace font, consistent with the existing `.reasoning` section
treatment. Truncate long output at ~300 chars with a "show more" affordance to avoid
overwhelming the block.

---

### Task 10 — Persist cancelled and completed processing rounds across phase transitions

**Files:** `web/src/stores/useMessageStore.ts`, `web/src/components/ChatView.tsx`

When streaming ends (normally or via cancel), `finalizeStream` transitions phase to `idle` and
clears `phase.entries`. Any processing rounds that only exist in the streaming phase — cancelled
mid-call rounds in particular — are lost at this point. Task 8's visibility fix covers the case
where the server has committed tool messages, but cancelled rounds may never have been committed.

Fix: add a `lastProcessingRounds: ProcessingRound[] | null` field to the thread state (alongside
`messages`). In `finalizeStream`, before clearing the phase, extract any processing entries from
`phase.entries` and write them to `lastProcessingRounds`. Apply the same in the `cancelRun` path.

In `ChatView.tsx`, after `processedItems` is built, if `lastProcessingRounds` is non-null and no
`tool_group` item already exists at the tail of the list (i.e. history hasn't caught up yet via
Task 8), append a synthetic `tool_group` item from `lastProcessingRounds`. This acts as a
fallback that disappears naturally once the server-side tool messages load and Task 8 takes over.

```typescript
// Thread state addition
interface ThreadState {
  messages: Message[];
  phase: ThreadPhase;
  lastProcessingRounds: ProcessingRound[] | null;
  // ...existing fields
}
```

---

### New acceptance criteria

- [ ] After streaming completes, the `<ProcessingBlock>` remains visible in the conversation as
      a collapsed history item — it does not disappear when the streaming phase ends
- [ ] When the agent is stopped mid-tool-call, the `<ProcessingBlock>` persists in a
      cancelled/dashed state after streaming ends
- [ ] Expanding a completed `ToolRow` shows the raw tool result content in a collapsible
      inline section styled consistently with the reasoning block
- [ ] Hidden `source: "tool"` messages are included in `visibleMessages` grouping regardless
      of their `visibility` field; all other hidden message types remain filtered

---

## Approval

- [x] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off

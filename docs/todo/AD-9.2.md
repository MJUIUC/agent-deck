# AD-9.2 — SSE Reconnection & In-Progress Run Resume

**Story:** 9.2 — SSE reconnection resilience and mid-run client pickup  
**Branch:** `feature/phase9-sse-reconnect`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9

---

## Summary

Ensures that a client which disconnects and reconnects to a thread's SSE stream — whether due to a tab navigation, network blip, or phone screen sleep — picks up exactly where it left off. Two scenarios are handled:

1. **Run already complete:** Client reconnects after the agent finished. DB messages are replayed from the client's last-seen event.
2. **Run still in progress:** Client reconnects mid-run. The server detects the active `RunState`, sends all completed DB messages, then emits a new `RunResumed` event carrying the partial assistant text accumulated so far, and finally splices the client onto the live broadcast channel so token streaming continues seamlessly.

Push notifications are also gated: a notification is only sent when no live SSE client is watching the thread at the moment the run completes.

---

## Current State

- `GET /api/threads/:id/stream` creates a fresh `mpsc` channel on every connection. No replay of prior events occurs on reconnect.
- SSE events are not stamped with an `id:` field, so the browser cannot send `Last-Event-ID` on reconnect.
- `RunState` holds a `running_turn: Mutex<Option<RunningTurn>>` but exposes no partial-text accumulator readable from outside `agent.rs`.
- `agent.rs` accumulates the current turn's text in a local `turn_text: String` inside `try_stream_one_turn` — not accessible externally.
- Push notifications for both routine and regular chat completions fire unconditionally, regardless of whether an SSE client is connected.

---

## Implementation Plan

### Task 1 — Stamp every SSE event with a sequential `id:` field

**Files:** `server/src/routes/sse.rs`

Add a per-connection monotonic counter. Each event emitted on a thread stream gets stamped with an incrementing integer id:

```rust
// In thread_stream handler, wrap the ReceiverStream map with a counter:
let mut seq: u64 = 0;
let event_stream = ReceiverStream::new(rx).map(move |thread_event| {
    seq += 1;
    let name = thread_event.event_name();
    let data = serde_json::to_string(&thread_event).unwrap_or_else(|_| "{}".to_string());
    Ok::<Event, Infallible>(Event::default().id(seq.to_string()).event(name).data(data))
});
```

> **Note:** The `id:` counter is per-connection, not global. It is used by the browser to send `Last-Event-ID` on reconnect so the server knows how many events to skip when replaying from DB. See Task 3 for the replay logic.

---

### Task 2 — Expose a shared partial-text accumulator on `RunState`

**Files:** `server/src/routes/mod.rs`, `server/src/services/agent.rs`

Add a field to `RunState` that the agent writes to as tokens arrive, and that the SSE reconnect handler can snapshot:

```rust
// In routes/mod.rs — add to RunState:
pub struct RunState {
    pub semaphore: tokio::sync::Semaphore,
    pub depth: AtomicUsize,
    pub running_turn: tokio::sync::Mutex<Option<RunningTurn>>,
    /// Partial assistant text for the current in-progress turn.
    /// Written by agent.rs as tokens stream; read by the SSE reconnect handler.
    /// Reset to empty string when the run completes or is cancelled.
    pub partial_text: Arc<std::sync::Mutex<String>>,
}

impl RunState {
    pub fn new() -> Self {
        Self {
            semaphore: tokio::sync::Semaphore::new(1),
            depth: AtomicUsize::new(0),
            running_turn: tokio::sync::Mutex::new(None),
            partial_text: Arc::new(std::sync::Mutex::new(String::new())),
        }
    }
}
```

In `agent.rs`, update `handle_stream_event` to also write to the accumulator. The `run_state` Arc must be threaded down into `try_stream_one_turn` → `handle_stream_event`:

```rust
// In handle_stream_event, TextDelta arm:
StreamEvent::TextDelta(delta) => {
    turn_text.push_str(&delta);
    // Write to shared partial_text so reconnecting clients can snapshot it.
    if let Ok(mut pt) = partial_text.lock() {
        pt.push_str(&delta);
    }
    state.send_thread_event(thread_id, ThreadEvent::Token { token: delta });
}
```

Clear `partial_text` at the end of each turn (both normal completion and cancelled path) in `generation_loop`:

```rust
// After persisting the final assistant message:
if let Ok(mut pt) = run_state.partial_text.lock() {
    pt.clear();
}
```

> **Complexity note:** `run_state` is already passed into `generation_loop`. It needs to be passed one level deeper into `stream_one_turn` → `try_stream_one_turn` → `handle_stream_event`. The call chain is internal to `agent.rs` so this is a straightforward signature extension with no public API change.

---

### Task 3 — Add `RunResumed` SSE event variant

**Files:** `server/src/routes/sse.rs`

Add a new event variant for the mid-run resume case:

```rust
/// Emitted on reconnect when an agent run is still in progress.
/// Carries all text the agent has streamed so far in the current turn
/// so the client can render it immediately without waiting for new tokens.
RunResumed {
    partial_text: String,
},
```

Add to `event_name()`:
```rust
ThreadEvent::RunResumed { .. } => "run_resumed",
```

---

### Task 4 — Replay-on-reconnect in `thread_stream` handler

**Files:** `server/src/routes/sse.rs`

Read the `Last-Event-ID` request header. If present, fetch all persisted messages for the thread since the client's last-seen event, flush them immediately before connecting to the live channel, then check for an active run and emit `RunResumed` if needed:

```rust
pub async fn thread_stream(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> AppResult<impl IntoResponse> {

    // ── 1. Fetch missed DB messages ──────────────────────────────────────────
    // Use Last-Event-ID as a skip count (events are numbered per-connection,
    // so we replay all messages since the most recent assistant message the
    // client confirmed receiving — identified by created_at ordering).
    //
    // Simpler approach: ignore the skip count and just replay all visible
    // messages newer than the client's last-seen message_id (if we stored it).
    // For Phase 9.2 we use a pragmatic approach: replay the last N visible
    // messages from DB regardless, bounded to avoid flooding a cold connect.
    // The frontend deduplicates by message ID.
    let missed_messages: Vec<crate::models::message::Message> = sqlx::query_as(
        "SELECT id, thread_id, role, content, source, routine_id, visibility,
                execution_id, event_type, stopped, created_at
         FROM messages
         WHERE thread_id = ? AND visibility = 'visible'
         ORDER BY created_at ASC",
    )
    .bind(&thread_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // ── 2. Check for active run ──────────────────────────────────────────────
    let run_state = state.get_run_state(&thread_id);
    let partial_snapshot: Option<String> = {
        let slot = run_state.running_turn.lock().await;
        if slot.is_some() {
            // Run is active — snapshot partial text
            let pt = run_state.partial_text.lock().unwrap();
            Some(pt.clone())
        } else {
            None
        }
    };

    // ── 3. Subscribe to live channel ─────────────────────────────────────────
    let (tx, rx) = state.subscribe_thread(&thread_id);

    // ── 4. Build replay prefix stream ────────────────────────────────────────
    let mut replay_events: Vec<Result<Event, Infallible>> = Vec::new();
    let mut seq: u64 = 0;

    for msg in missed_messages {
        seq += 1;
        let evt = ThreadEvent::MessageComplete {
            id: msg.id,
            thread_id: msg.thread_id,
            role: msg.role,
            content: msg.content,
            created_at: msg.created_at,
            stopped: msg.stopped,
        };
        let data = serde_json::to_string(&evt).unwrap_or_default();
        replay_events.push(Ok(Event::default()
            .id(seq.to_string())
            .event(evt.event_name())
            .data(data)));
    }

    // If a run is active, append RunResumed after the DB replay
    if let Some(partial) = partial_snapshot {
        seq += 1;
        let evt = ThreadEvent::RunResumed { partial_text: partial };
        let data = serde_json::to_string(&evt).unwrap_or_default();
        replay_events.push(Ok(Event::default()
            .id(seq.to_string())
            .event("run_resumed")
            .data(data)));
    }

    // ── 5. Chain replay → live stream ─────────────────────────────────────────
    let replay_stream = futures::stream::iter(replay_events);
    let live_stream = ReceiverStream::new(rx).map(move |thread_event| {
        seq += 1;
        let name = thread_event.event_name();
        let data = serde_json::to_string(&thread_event).unwrap_or_default();
        Ok::<Event, Infallible>(Event::default().id(seq.to_string()).event(name).data(data))
    });

    let combined = replay_stream.chain(live_stream);

    // ... CleanupStream wrapping and Sse::new() as before
}
```

> **Frontend deduplication note:** The frontend must deduplicate `message_complete` events by `id` field. On a cold connect this is a no-op (no duplicates). On reconnect it ensures replayed messages don't create duplicate bubbles.

---

### Task 5 — Gate push notifications on `!has_thread_subscriber()`

**Files:** `server/src/services/agent.rs`

Two push notification call sites exist in `run_inner`. Both should only fire when no live SSE client is watching:

```rust
// Routine completion push:
if !state.has_thread_subscriber(thread_id) {
    crate::services::push::send_push_notification(
        state, thread_id, user_id, &push_title, &push_body,
    ).await;
}

// Regular chat reply push:
if routine_id_opt.is_none() && !state.has_thread_subscriber(thread_id) {
    crate::services::push::send_push_notification(
        state, thread_id, user_id, &push_title, &push_body,
    ).await;
}
```

---

### Task 6 — Frontend: handle `run_resumed` event and deduplicate replayed messages

**Files:** Frontend SSE hook (e.g. `useThreadStream.ts` or equivalent), thread message list component

- On `run_resumed`: create a new streaming assistant bubble pre-filled with `partial_text`. Continue appending `token` events as they arrive.
- On `message_complete`: check if a message with the same `id` already exists in the local state. If so, update it in place rather than appending a duplicate.
- On cold connect (no `run_resumed`, no duplicates): behaviour unchanged.

---

### Schema changes

None — no database migrations required.

---

### Parallelisation note

- **Tasks 1, 2, 3** are independent of each other and can be worked in parallel across sub-agents.
- **Task 4** depends on Tasks 1, 2, and 3 (needs `partial_text` on `RunState`, `RunResumed` variant, and `id:` stamping).
- **Task 5** is independent of all other tasks — a one-file, two-line change.
- **Task 6** (frontend) can begin once Tasks 3 and 4 are complete.

Recommended agent split:
- **Sub-agent A**: Tasks 1 + 3 (`sse.rs` — `id:` stamping + `RunResumed` variant)
- **Sub-agent B**: Task 2 (`mod.rs` + `agent.rs` — `partial_text` on `RunState`)
- After A and B complete → **Sub-agent C**: Task 4 (`sse.rs` — reconnect handler)
- **Sub-agent D** (parallel with A/B/C): Task 5 (`agent.rs` — push gate)
- After C → **Sub-agent E**: Task 6 (frontend)

---

## Acceptance Criteria

- [ ] Every event on `GET /api/threads/:id/stream` is stamped with a sequential `id:` field
- [ ] On reconnect (cold — no active run), all persisted visible messages for the thread are replayed as `message_complete` events before any live events
- [ ] On reconnect (mid-run — active `RunState`), a `run_resumed` event is emitted after DB replay carrying the partial text accumulated so far; subsequent `token` events continue appending to the same bubble
- [ ] Frontend deduplicates `message_complete` events by `id` — reconnecting does not create duplicate message bubbles
- [ ] Push notifications for routine completions are only sent when `!has_thread_subscriber(thread_id)` at the time of completion
- [ ] Push notifications for regular chat replies are only sent when `!has_thread_subscriber(thread_id)` at the time of completion
- [ ] Existing behaviour for a fresh connect (no prior session, no active run) is unchanged — no regression
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

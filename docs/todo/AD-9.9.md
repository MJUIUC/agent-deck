# AD-9.9 — Agent Inbox & Policy-Driven Run-Loop

**Story:** 9.9 — Agent Inbox & Policy-Driven Run-Loop  
**Branch:** `feature/agent-inbox`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9  
**Related stories:** `docs/todo/AD-9.7.md`, `docs/todo/AD-9.8.md`

---

## Summary

Today, routines invoke the agent run-loop directly — each routine fires an independent agent turn, with no awareness of other concurrently firing routines. This creates race conditions (documented in `scheduler.rs`) where multiple agent turns attempt to run simultaneously, clobbering each other.

This story replaces that model with an **Inbox-based message queue**. Routines become message _producers_; the agent run-loop becomes the sole _consumer_. A policy layer baked into the platform (not the agent) governs whether the agent requires human permission before acting on each message.

This cleanly separates three concerns:
- **Policy** → configuration (thread-level + per-routine)
- **Coordination** → platform (inbox, state machine, event trigger)
- **Intelligence** → agent (what to do with the messages)

---

## Motivation & Problem Statement

### Current Behavior
```
Routine A fires at 9AM → directly invokes agent turn
Routine B fires at 9AM → directly invokes agent turn  ← race condition
Routine C fires at 9AM → directly invokes agent turn  ← race condition
```

Each turn has no knowledge of the others. The `running_turn` slot in `scheduler.rs` has a TOCTOU race when multiple routines fire simultaneously — the check-then-set is not atomic.

### Desired Behavior
```
Routine A fires at 9AM → writes message to Inbox
Routine B fires at 9AM → writes message to Inbox
Routine C fires at 9AM → writes message to Inbox

Platform detects inbox_updated event → invokes agent run-loop once
Agent inspects inbox → sees 3 pending messages → processes them → marks done
Agent run-loop continues until inbox is empty → then goes IDLE
```

---

## Design

### 1. Inbox Data Model

A new persistent table `inbox_messages`:

```
id           TEXT PRIMARY KEY   -- unique message ID (uuid)
thread_id    TEXT NOT NULL      -- which thread this message belongs to
source       TEXT NOT NULL      -- e.g. "routine:rdw_tracker"
payload      TEXT NOT NULL      -- the message content / instruction
status       TEXT NOT NULL      -- "pending" | "in_progress" | "done" | "failed" | "skipped"
requires_permission  BOOLEAN    -- see Permission Policy section
created_at   DATETIME
started_at   DATETIME
finished_at  DATETIME
error        TEXT               -- populated on failure
```

### 2. Routine → Inbox (Decoupled Invocation)

Routines no longer call `invoke_agent()` directly. Instead they call `inbox.push(message)`.

The scheduler's only job becomes: **write to the inbox at the right time**. It has no further involvement in agent lifecycle.

### 3. Platform Event: `inbox_updated`

When a new message is pushed to the inbox, the platform fires an `inbox_updated` event.

The event handler checks agent state:

```
on inbox_updated(thread_id):
    if agent_state(thread_id) == RUNNING:
        enqueue wake_notification(thread_id)   // deferred, not dropped
    else:
        invoke_run_loop(thread_id)
```

**Critical:** if the agent is currently running, the wake notification is _queued_, not dropped and not fired immediately. The agent will never be interrupted mid-turn.

### 4. Agent State Machine

The agent thread operates as a simple state machine:

```
IDLE
  └── inbox_updated event → RUNNING
        └── processes messages until inbox is empty
        └── checks queued wake notifications
            → if pending notification: stay RUNNING, re-check inbox
            → if no notifications:    → IDLE
```

The run-loop **continues executing until the inbox is fully drained**. It does not stop after the first message.

### 5. Permission Policy

Whether the agent asks the user for permission before acting is governed by two layered config values:

#### Thread-Level Default
```
thread.inbox_mode = "ask" | "auto"
```
Default: `"ask"` — the agent requests human confirmation before executing any inbox message.

#### Per-Routine Override
```
routine.requires_permission = true | false | "inherit"
```
Default: `"inherit"` — falls back to `thread.inbox_mode`.

#### Resolution Order
```
routine.requires_permission (if not "inherit")
    → thread.inbox_mode
        → platform default ("ask")
```

#### Platform Pre-Framing

The platform uses this resolved policy to pre-frame the agent's turn context — the agent does not decide this itself:

```
// requires_permission = true → agent is told:
"You have a pending inbox message from routine 'rdw_tracker'.
Ask the user if they'd like you to proceed before taking any action."

// requires_permission = false → agent is told:
"You have a pending inbox message from routine 'rdw_tracker'.
Execute it now and report results when complete."
```

The agent never infers permission policy from context — it is always told explicitly by the platform.

### 6. Agent-Facing Inbox Tools

The following tools are exposed to the agent:

```
get_inbox()                     → list of pending messages for current thread
ack_message(id)                 → mark message as in_progress
complete_message(id)            → mark message as done
fail_message(id, error)         → mark message as failed with error string
skip_message(id, reason)        → mark message as skipped
snooze_message(id, until)       → defer message until a future datetime
```

---

## Run-Loop Lifecycle (Full Flow)

```
1. Routine fires at scheduled time
2. Scheduler writes message to inbox (with requires_permission metadata)
3. Platform fires inbox_updated event

4. Event handler:
   - Agent IDLE?  → invoke run-loop immediately
   - Agent RUNNING? → enqueue wake notification

5. Agent wakes up, platform resolves permission policy, pre-frames the turn

6. Agent calls get_inbox() → sees all pending messages
7. For each message:
   - Calls ack_message(id)
   - Executes task (or asks user if requires_permission = true)
   - Calls complete_message(id) or fail_message(id, error)

8. Agent checks inbox again — if still pending messages, continue
9. Inbox empty → agent checks for queued wake notifications
   - Notification pending? → re-check inbox (go to step 6)
   - No notifications? → agent goes IDLE

10. Any queued wake notifications now fire → repeat from step 5
```

---

## What This Fixes

| Problem | Fix |
|---|---|
| Race condition when multiple routines fire simultaneously | Routines only write to inbox; one run-loop consumer |
| Agent has no awareness of other pending tasks | `get_inbox()` gives full view of everything waiting |
| Agent acts autonomously without user awareness | `requires_permission` policy pre-frames every turn |
| Agent turn interrupted by new routine firing | Wake notifications are queued; agent always finishes first |
| Messages dropped if agent is busy | Inbox is persistent; nothing is lost |
| No audit trail for scheduled tasks | Every message has status + timestamps |

---

## Future Extensions (Out of Scope for This Story)

- **`spawn_agent_thread(task)`** — agent delegates inbox items to parallel sub-agent threads; results written back to inbox for main thread to synthesize
- **Priority queue** — high-priority messages (e.g. price alerts) can jump ahead of lower-priority scheduled tasks
- **Interrupt mechanism** — for truly time-critical events, a configurable opt-in to interrupt a running turn
- **Inbox UI** — visual inbox panel in the agent-deck chat interface showing pending / in-progress / done messages

---

## Acceptance Criteria

- [ ] `inbox_messages` table created and migrated
- [ ] Routines write to inbox instead of invoking agent directly
- [ ] `inbox_updated` event fires on every new inbox message
- [ ] Agent state machine implemented: IDLE → RUNNING → IDLE
- [ ] Agent is never interrupted mid-turn; wake notifications queue correctly
- [ ] Agent run-loop continues until inbox is fully empty
- [ ] `thread.inbox_mode` config field added (`"ask"` | `"auto"`, default `"ask"`)
- [ ] `routine.requires_permission` field added (`true` | `false` | `"inherit"`, default `"inherit"`)
- [ ] Permission policy resolved correctly per resolution order
- [ ] Platform pre-frames agent turn with correct permission instruction
- [ ] All 6 inbox tools available to the agent (`get_inbox`, `ack_message`, `complete_message`, `fail_message`, `skip_message`, `snooze_message`)
- [ ] Existing routine scheduling behavior unchanged (timing, cron, etc.)
- [ ] No existing tests broken

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**

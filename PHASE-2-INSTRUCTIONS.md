# Agent-Deck — Phase 2 Implementation Instructions

**For:** Implementing agent  
**Read first:** PLAN.md (v1.4) in full before touching any code  
**Repo:** https://github.com/MJUIUC/agent-deck  
**Current state:** Phase 1 complete and v1.4 alignment merged to `main`. Phase 2 begins now.

---

## How to Use These Instructions

Work **one story at a time**, on its own feature branch. Each story has a branch name, a precise scope, and acceptance criteria. Do not combine stories. Do not start a new story until the previous one is merged to `main`.

Commit message format: `feat(phase2): <short description>`

**Stories must be worked in order.** Later stories depend on infrastructure built by earlier ones. Do not skip ahead.

---

## Phase 2 Goal

**You can talk to an LLM through your own UI.** This is the milestone that makes the project feel real. By the end of this phase you can bootstrap a provider and persona via curl, then chat in the browser with streaming responses.

---

## Architecture Context

Before starting, understand how the pieces fit together:

**Process relationships (PLAN.md §2.2):**  
The Rust server is the central process. It serves the React SPA, exposes a REST + SSE API, owns the agent run-loop (LLM calls happen server-side, not on the client), and manages the `copilot-api` child process.

**Communication patterns (PLAN.md §2.3):**
- Client → Server: Standard HTTP REST
- Server → Client (streaming): Server-Sent Events (SSE) for LLM token streaming and live event delivery
- Server → LLM Provider: HTTP via provider abstraction layer (OpenAI-compatible API)

**Two SSE endpoints exist (PLAN.md §2.4):**

`GET /api/threads/:id/stream` — per-thread event stream:
- `token` — a single streamed LLM token
- `message_complete` — full message object once streaming is done
- `routine_message` — a new message produced by a routine firing
- `error` — streaming error with typed code

`GET /api/events` — global event stream:
- `thread_updated` — last message preview or unread count changed
- `routine_fired` — a routine ran (includes thread_id)
- `provider_status` — copilot-api connected/disconnected

---

## Story 2.1 — copilot-api Submodule and Process Management

**Branch:** `feature/phase2-copilot-api`

### Background

`copilot-api` is a third-party Bun application included as a git submodule at `vendor/copilot-api/`. It acts as a local OpenAI-compatible proxy for GitHub Copilot, listening on port 4141 (localhost only). The Rust server manages its lifecycle as a child process via `tokio::process`.

See: PLAN.md §2.2, §3.4, §7.7

### What to Build

Implement a `CopilotApiService` in `server/src/services/copilot.rs` that:

1. **Spawns the copilot-api process on server startup** — async, non-blocking. The server must not block startup waiting for copilot-api to become available. Mark status as `connecting` while waiting.

2. **Polls a health endpoint until ready.** Once the process responds to a health check, transition status to `connected`.

3. **Monitors the process and restarts on unexpected exit.** If copilot-api crashes, the service should detect the exit and restart the process automatically. Use a backoff strategy to avoid tight restart loops.

4. **Exposes `is_available() -> bool` and `base_url() -> &str`** for other services to check before routing requests through Copilot.

5. **Integrates with the global SSE stream.** Emit a `provider_status` event when copilot-api connects or disconnects. This requires access to the global event broadcaster (which will be formalized in Story 2.3 — for now, define the event types and hold a `broadcast::Sender` in AppState).

6. **Shuts down cleanly.** On server shutdown (SIGTERM/SIGINT), kill the child process gracefully.

### Copilot Auth Endpoints

Implement two endpoints for the Copilot auth flow:

- `GET /api/providers/copilot/auth-status` — returns whether copilot-api has a valid GitHub token
- `POST /api/providers/copilot/auth-start` — triggers the GitHub device auth flow via copilot-api; returns the device code and verification URL for the UI to display

### Degradation Behavior (PLAN.md §7.7)

When copilot-api is unavailable (crashed, not started, auth expired):
- Provider status should show a clear reason: "Process not running", "Auth expired", "Connection refused"
- Sending a message on a thread using Copilot returns an SSE `error` event with `"code": "PROVIDER_UNAVAILABLE"` and a human-readable message
- The server never panics or blocks on copilot-api unavailability

### Verification Steps

Before marking complete:
1. `git submodule status` shows `vendor/copilot-api/` initialized
2. Start the server — copilot-api process starts in the background
3. Kill the copilot-api process manually — it restarts automatically
4. Stop the server — copilot-api process is killed cleanly
5. Auth status endpoint returns correct state

### Acceptance Criteria

- [ ] Server starts without blocking on copilot-api availability
- [ ] copilot-api process is started and managed as a child process
- [ ] Health check polling works and status transitions correctly (`connecting` → `connected`)
- [ ] Process restarts automatically on unexpected exit
- [ ] SSE event emitted on status change (define event type, hold broadcaster)
- [ ] Graceful shutdown kills child process cleanly
- [ ] Auth status and auth start endpoints work
- [ ] `cargo build` passes, all existing tests pass

---

## Story 2.2 — Provider Abstraction Layer

**Branch:** `feature/phase2-provider-abstraction`

### Background

The system supports multiple LLM provider types (PLAN.md §7.7):
- `copilot` — GitHub Copilot via local copilot-api proxy at `http://localhost:4141/v1`
- `openai` — Direct OpenAI API with API key
- `anthropic` — Anthropic API with API key
- `custom` — Any OpenAI-compatible endpoint

Each provider is stored in the `providers` table with a kind, base URL, and optional API key. API keys are stored encrypted at rest.

### What to Build

Implement a `ProviderService` trait in `server/src/services/provider.rs` using the `async-openai` crate. The trait must support:

- **Listing available models** — returns model IDs the provider offers
- **Sending a chat completion request (non-streaming)** — for simple one-shot calls
- **Sending a streaming chat completion request** — returns a stream of token chunks. This is the primary path used during chat.
- **Checking provider health/availability** — is the provider reachable and authenticated?

Implement two concrete providers:

1. **`CopilotProvider`** — routes through copilot-api's OpenAI-compatible proxy at `http://localhost:4141/v1`. Checks `CopilotApiService::is_available()` before making requests. No API key needed.

2. **`OpenAiProvider`** — calls OpenAI API directly using the stored provider config (base URL + API key from DB). This same implementation works for `custom` provider types since they use the same OpenAI-compatible API shape.

The `AppState` should hold a registry of active providers keyed by provider ID. Provider selection happens at request time based on the thread's active provider configuration.

### Design Constraints

- Pointing the provider at `http://localhost:4141/v1` (Copilot proxy) must work identically to pointing it at `https://api.openai.com/v1` — the abstraction layer should not care which backend it talks to.
- An unavailable provider returns an appropriate error type, never a panic.
- The trait should be designed so that adding the `anthropic` provider later (which has a different API shape) is possible without rewriting the trait — consider how you'd handle non-OpenAI-compatible APIs.

### Acceptance Criteria

- [ ] Provider trait is defined with correct method signatures
- [ ] `CopilotProvider` implements the trait and routes through copilot-api
- [ ] `OpenAiProvider` implements the trait and calls OpenAI directly
- [ ] Provider is selected at request time based on thread's active provider
- [ ] Unavailable provider returns appropriate error, not a panic
- [ ] Unit tests mock the HTTP layer and verify correct request construction
- [ ] Streaming works against a real provider (manual verification)
- [ ] Error cases handled: provider unreachable, invalid API key, model not found
- [ ] `cargo build` passes, all existing tests pass

---

## Story 2.3 — SSE Infrastructure

**Branch:** `feature/phase2-sse-infrastructure`

### Background

Two SSE endpoints power all real-time communication (PLAN.md §6.9). These must be built before the agent run-loop, which emits events through them.

### What to Build

**`GET /api/threads/:id/stream`** — per-thread event stream

Use `tokio::sync::mpsc` channels created per-connection. When a client connects, create a new channel and register it. When the client disconnects, clean up the channel.

Events emitted on this stream:

```
event: token
data: {"token": "Hello"}

event: message_complete
data: {"id": "<id>", "thread_id": "<id>", "role": "assistant", "content": "Hello! How can I help?", "created_at": "..."}

event: routine_message
data: {"id": "<id>", "thread_id": "<id>", "role": "assistant", "content": "...", "routine_id": "<id>", "created_at": "..."}

event: error
data: {"code": "PROVIDER_ERROR", "message": "Failed to connect to provider"}
```

**`GET /api/events`** — global event stream

Use `tokio::sync::broadcast` channels. The `AppState` holds a `broadcast::Sender<GlobalEvent>`. Each connected client subscribes via `sender.subscribe()`.

Events emitted on this stream:

```
event: thread_updated
data: {"thread_id": "<id>", "last_message": "...", "updated_at": "..."}

event: routine_fired
data: {"thread_id": "<id>", "routine_id": "<id>", "routine_name": "Morning Briefing"}

event: provider_status
data: {"provider_id": "<id>", "status": "connected"}
```

**Both streams:**
- Use Axum's SSE support (`axum::response::Sse`)
- Emit a `ping` event every 30 seconds to keep connections alive
- Handle client disconnection cleanly — no resource leaks, no panics
- Track connected SSE clients per thread (needed later in Phase 6 for push notification decisions — when no SSE client is connected, push instead)

### Integration with AppState

Formalize the event broadcasting infrastructure in `AppState`:
- `global_tx: broadcast::Sender<GlobalEvent>` — for global SSE events
- A thread-level channel registry: a `HashMap<String, Vec<mpsc::Sender<ThreadEvent>>>` (or similar) protected by a `Mutex` or `RwLock`, keyed by thread ID

Other services (agent run-loop, scheduler, copilot service) will send events by calling methods on AppState rather than holding channels directly.

### Acceptance Criteria

- [ ] `GET /api/threads/:id/stream` returns a valid SSE stream
- [ ] `GET /api/events` returns a valid SSE stream
- [ ] Token events stream correctly during a mocked LLM response
- [ ] Global stream receives events from other parts of the system
- [ ] 30-second keepalive pings work on both streams
- [ ] Client disconnection is handled cleanly (no resource leak, no panic)
- [ ] Manual verification: connecting with `curl` to both endpoints shows SSE stream with pings
- [ ] `cargo build` passes, all existing tests pass

---

## Story 2.4 — Context Assembly

**Branch:** `feature/phase2-context-assembly`

### Background

Every LLM request needs a carefully ordered message array. The context assembler is a pure function that builds this array, making it easy to unit test without any network or DB dependencies.

### What to Build

Implement a `ContextAssembler` in `server/src/services/context.rs`. Given a thread ID, persona, and the new user message, it builds the full request payload.

**Assembly order:**

1. **System message: persona system prompt** — the persona's `system_prompt` field from DB
2. **System message: memory instructions** — appended automatically after the persona prompt. This is hardcoded text, not editable by the user (PLAN.md §7.6.3):

```
## Memory

You have persistent long-term memory that spans across all our conversations. Use it actively:

**When to save:** When I share a preference, a fact about myself, a project detail, a deadline, a name, a relationship, a goal, or anything that seems worth remembering in future conversations — save it immediately using save_memory. Save one fact per call. Write memories as concise factual statements, not narrative.

**When to recall:** Before answering questions that might benefit from prior context, when I reference something from a past conversation, or when I seem to assume you know something — check your memory using recall_memory. Use specific keywords, not full sentences.

**Do not** tell me every time you save or recall a memory. Use memory silently unless I specifically ask what you remember about something.
```

3. **System message: thread addendum** — the thread's `system_prompt_addendum` field, if set. Omitted when null.
4. **Last N messages from the thread** — default 20, loaded from DB in chronological order. Only include messages with `visibility: visible`.
5. **The new user message** — the message just sent by the user

**Tool definitions array:**

The assembler also builds a tools array to include in the LLM request:

- Always include `save_memory` and `recall_memory` tool definitions (exact schemas in PLAN.md §7.6.2) if the provider supports function calling
- Include tools from attached MCP servers — **stub this for now**: return an empty list with a `// TODO: Phase 7 — load tools from attached MCP servers` comment

### Design Notes

- Write this as a pure function that takes all its inputs as arguments (persona, messages, addendum, etc.) rather than reaching into DB or AppState directly. The caller is responsible for loading the data.
- The function should return both the messages array and the tools array as a struct.
- System messages can be combined (persona prompt + memory instructions as one system message, or separate) — choose whichever is cleaner, but document the choice.

### Acceptance Criteria

- [ ] Unit tests verify correct ordering: system prompt → memory instructions → addendum → history → new message
- [ ] Unit tests verify system prompt is always first
- [ ] Unit tests verify memory instructions are always appended after persona prompt
- [ ] Unit tests verify thread addendum is included when present, omitted when null
- [ ] Unit tests verify message count is capped at 20
- [ ] Unit tests verify only `visibility: visible` messages are included
- [ ] Tool definitions for `save_memory` and `recall_memory` are included
- [ ] MCP tools stub returns empty list with TODO comment
- [ ] `cargo build` passes, all existing tests pass

---

## Story 2.5 — Agent Run-Loop with Streaming

**Branch:** `feature/phase2-agent-run-loop`

### ⚠️ This story requires human review before merging.

The async coordination between HTTP handler, SSE stream, and tool execution is subtle. Demonstrate end-to-end with a real LLM call before marking complete. Do not merge without the human reviewing the actual running behavior.

### Background

This is the core execution path for all chat interactions. It wires together the provider abstraction, context assembler, SSE infrastructure, and memory tools into a single cohesive flow.

### What to Build

Implement the agent run-loop in `server/src/services/agent.rs`.

**HTTP handler: `POST /api/threads/:id/messages`**

The handler does three things synchronously, then returns:
1. Persist the user message to DB with `visibility: visible`
2. Auto-generate thread title if this is the first message (use the first ~60 characters of the user message, or ask the LLM to generate one — your call, but keep it simple for v1)
3. Spawn a Tokio task to run the agent asynchronously
4. Return the persisted user message immediately as the HTTP response

**The spawned async task:**

1. Call `ContextAssembler` to build the messages array and tools array
2. Call the active provider's streaming completion method
3. Stream tokens to the per-thread SSE channel as `token` events
4. **On tool call** (`save_memory` or `recall_memory`):
   - Execute the tool (see Memory Tool Implementation below)
   - Append the tool result to the context
   - Continue generation (the LLM will produce more tokens after seeing the tool result)
5. Persist the complete assistant message to DB with `visibility: visible` once streaming finishes
6. Emit `message_complete` SSE event on the per-thread stream
7. Emit `thread_updated` global SSE event (for sidebar refresh)

### Memory Tool Implementation

Wire `save_memory` and `recall_memory` into the run-loop:

**`save_memory`:**
- Insert a new row into the `memories` table with `user_id`, `persona_id` (from thread's persona), `thread_id` (provenance)
- Enforce 500-character content limit (truncate silently)
- Check the 500-entry cap per persona — if full, return a tool result telling the model the memory store is full
- Update the FTS5 index

**`recall_memory`:**
- Query `memory_fts` filtered by current `user_id` and `persona_id`
- Cap at 10 results
- Format results as (PLAN.md §7.6.5):
```
Found 3 memories:
- [2025-01-15] User's project Atlas has a deadline of March 15 2025
- [2025-01-10] User works at Acme Corp as a senior engineer
- [2025-01-08] User prefers concise responses without excessive formatting

(Thread sources: coding-session, general-chat, preferences)
```

If no memories match, return: `No memories found matching "query terms".`

### Error Handling

- **Provider unavailable:** Do not persist an assistant message. Emit an `error` SSE event with code `PROVIDER_UNAVAILABLE` and a human-readable message suggesting the user check provider settings or switch models.
- **Generation fails mid-stream:** Emit an `error` SSE event. Do not persist a partial message. The user should see the error in the chat UI, not a half-finished response.
- **Tool execution fails:** Log the error, return a tool result indicating failure to the LLM, and let the LLM decide how to respond.

### Acceptance Criteria

- [ ] User message returned immediately from POST (HTTP response)
- [ ] Tokens stream to SSE client in real time
- [ ] Complete assistant message persisted to DB after streaming finishes
- [ ] Thread title auto-generated on first message
- [ ] `save_memory` tool call inserts memory, respects 500-char and 500-entry limits
- [ ] `recall_memory` tool call queries FTS5, returns formatted results capped at 10
- [ ] Tool calls are handled inline — LLM continues generating after receiving tool results
- [ ] Provider unavailable error surfaces via SSE `error` event, not HTTP error
- [ ] Mid-stream failure emits `error` event, does not persist partial message
- [ ] Integration test: send a message, verify SSE stream, verify DB state
- [ ] Manual verification: end-to-end chat works with a real LLM provider
- [ ] `cargo build` passes, all existing tests pass

---

## Story 2.6 — Thread List and Chat UI

**Branch:** `feature/phase2-chat-ui`

### ⚠️ This story requires human review before merging.

UX feel and streaming behavior need subjective evaluation. Do not merge without the human reviewing the actual running behavior in a browser.

### Background

This is the critical UI story — the one that makes the project usable. Implement the thread list and chat view in the React SPA, matching the approved mockups in `mockups/chat-view.html`.

### What to Build

**Thread list (left sidebar):**

- Fetch `GET /api/threads` on load
- Render each thread with persona avatar/emoji, title, last message preview, timestamp
- Active thread highlighted
- "New Chat" button triggers a persona picker modal, then `POST /api/threads` to create the thread
- Updates in real time via global SSE `thread_updated` events (sidebar refreshes when a message arrives on any thread)

**Chat view (center panel):**

- Load message history from `GET /api/threads/:id/messages` when a thread is selected
- Render user messages right-aligned, agent messages left-aligned with persona avatar
- Routine-generated messages (`source: routine`) have a distinct visual treatment — subtle different background, small "routine" label
- Hidden messages (`visibility: hidden`) are **never rendered**
- Connect to `GET /api/threads/:id/stream` when a thread is opened
- Stream incoming tokens into a "pending" message bubble that builds up as tokens arrive, then resolves to a real message on `message_complete`
- Message input bar at bottom with send button
  - Enter to send, Shift+Enter for newline
  - Slash command detection: if input starts with `/`, intercept and route to `POST /api/threads/:id/command` instead of the normal message endpoint
- Handle SSE `error` events — display the error message in the chat as a system message or error banner

**Zustand stores:**

Create three stores to manage state:

- `useThreadStore` — thread list, active thread ID, thread creation, thread metadata
- `useMessageStore` — messages for the active thread, streaming state (pending tokens), message submission
- `useSseStore` — SSE connection management: connect/disconnect lifecycle, event routing to the appropriate store handlers

**SSE connection lifecycle:**
- When a thread is selected, connect to its per-thread stream
- When switching threads, disconnect the old stream and connect the new one
- The global stream should stay connected as long as the app is open
- Handle reconnection on disconnect (with backoff)

### UI Spec Reference (PLAN.md §8.1–8.2)

Layout is a persistent two-column design:
```
┌─────────────────────────────────────────────────────┐
│  ● agent-deck          [persona avatar] [settings]  │
├────────────────┬────────────────────────────────────┤
│  Thread List   │   Chat View                        │
│  [search]      │   [thread title]    [config icon]  │
│  ● Thread 1    │                                    │
│  ● Thread 2    │   message history...               │
│  [+ New Chat]  │                                    │
│  [Settings]    │   [input bar]                      │
└────────────────┴────────────────────────────────────┘
```

### Empty States (PLAN.md §8.6)

- No threads: "No conversations yet. Tap + to start chatting."
- No personas configured: redirect to setup wizard (or show a banner linking to provider settings)
- No provider connected: show a banner with a link to provider settings

### Acceptance Criteria

- [ ] Thread list loads and displays correctly
- [ ] Can select a thread and see its message history
- [ ] "New Chat" flow creates a thread via persona picker and navigates to it
- [ ] Can send a message and see tokens stream in real time
- [ ] Streaming feels smooth with no flickering
- [ ] Agent avatar/emoji appears next to every agent message
- [ ] Thread list updates in real time when a new message arrives (via global SSE)
- [ ] Routine messages are visually distinct from normal messages
- [ ] Hidden messages are never rendered
- [ ] SSE `error` events display in the chat UI
- [ ] Empty states render correctly (no threads, no messages, no provider)
- [ ] Works on mobile viewport widths (responsive)
- [ ] Slash command input starting with `/` is intercepted correctly
- [ ] `npm run build` passes with no errors
- [ ] Manual verification: full end-to-end chat flow in the browser

---

## Phase 2 Completion Checklist

Before declaring Phase 2 done, verify the complete end-to-end flow:

1. [ ] Server starts and copilot-api process launches in background
2. [ ] Provider is configured (via curl or setup endpoint)
3. [ ] Persona exists (via curl or setup endpoint)
4. [ ] Open the web UI — empty state shows correctly
5. [ ] Create a new thread via the persona picker
6. [ ] Send a message — tokens stream in real time
7. [ ] Assistant response completes and is persisted
8. [ ] Thread list sidebar updates with the latest message preview
9. [ ] Memory tools work: say something memorable, start a new thread with the same persona, ask the agent to recall it
10. [ ] Kill copilot-api manually — error message appears in chat UI, server stays up
11. [ ] copilot-api restarts automatically

Once verified, all six stories are merged to `main`, and the human has reviewed Stories 2.5 and 2.6 — Phase 2 is complete.

---

## Key Rules (Carried Forward)

1. **Read PLAN.md in full before starting any story.** Every story references sections of the plan. The plan is the spec.
2. **One story per branch.** Never combine stories. Branches should be small and reviewable.
3. **Acceptance criteria = definition of done.** Do not mark a story complete unless all criteria pass.
4. **SQLx offline mode.** After any schema change, run `cargo sqlx prepare` and commit the updated `.sqlx/` directory.
5. **Never expose `encrypted_data` in API responses.** The `credentials` table contains encrypted secrets. This is a hard security rule.
6. **Hidden messages stay hidden.** `GET /api/threads/:id/messages` filters out `visibility: hidden` messages by default. Only include them when `?include_hidden=true` is passed.
7. **Mockups are the visual reference.** For the chat UI, open `mockups/chat-view.html` and match it.
8. **Stories 2.5 and 2.6 require human review.** Do not merge these without the human reviewing the actual running behavior.

---

## What Comes Next

After Phase 2 is merged and verified working end-to-end, proceed to **Phase 3 — Configuration and Management**. Phase 3 makes the app fully self-service through its own UI: setup wizard, settings pages, thread config, and slash commands. The Phase 3 story order will be provided in a separate instructions document.

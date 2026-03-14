# Phase 4.1a — Frontend State Machine Refactor

**Branch:** `feature/phase4-1a-frontend-state`  
**Merges into:** `dev`

**Scope:** Consolidate per-thread UI state into a single explicit state machine, add Vitest coverage for the message store, and add jsdom + React Testing Library for component smoke tests. No backend changes. No new user-visible features — this is a stability and maintainability investment that unblocks the remaining Phase 4 UI work.

**Why this exists:** The streaming race conditions fixed in the feature/phase4-credential-store branch were all caused by the same root problem — per-thread state is spread across five independent maps (`isStreaming`, `isSending`, `isLoadingMessages`, `streamingContent`, `messagesByThread`), which allows illegal combinations like `isStreaming: true` AND `isLoadingMessages: true` simultaneously. Each illegal combination is a potential rendering bug. A state machine makes illegal states unrepresentable.

---

## Current state (what we're replacing)

`useMessageStore` tracks each thread's live state across five separate `Record<string, T>` maps:

```ts
messagesByThread:  Record<string, Message[]>
streamingContent:  Record<string, string>
isStreaming:       Record<string, boolean>
isSending:        Record<string, boolean>
isLoadingMessages: Record<string, boolean>   // was global until 4.1 fix
```

### Known illegal combinations

| isLoading | isSending | isStreaming | What happens |
|-----------|-----------|-------------|--------------|
| true | true | false | Loading spinner hides the optimistic message |
| false | false | true | Streaming bubble renders with no send in progress — stuck |
| true | false | true | Loading overwrote streaming state — content lost |
| false | true | true | Normal, the only valid sending+streaming combo |

All of the race conditions fixed in the previous branch were instances of the first and third rows.

---

## Target state (what we're building)

### The `ThreadPhase` union type

```ts
type ThreadPhase =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "sending"; optimisticId: string }
  | { status: "streaming"; content: string }
  | { status: "error"; message: string; recoverable: boolean }
```

### The `ThreadState` record

```ts
interface ThreadState {
  messages: Message[]
  phase: ThreadPhase
}

type ThreadMap = Record<string, ThreadState>
```

### The new store shape

```ts
interface MessageStore {
  threads: ThreadMap

  // Actions — signatures unchanged, implementation changes internally
  loadMessages:      (threadId: string) => Promise<void>
  sendMessage:       (threadId: string, content: string) => Promise<void>
  sendCommand:       (threadId: string, input: string) => Promise<SlashCommandResponse | null>
  appendToken:       (threadId: string, token: string) => void
  finalizeStream:    (threadId: string, message: Message) => void
  addMessage:        (message: Message) => void
  setStreamingError: (threadId: string, errorMsg: string) => void
  clearMessages:     (threadId: string) => void
  clearError:        (threadId: string) => void
}
```

### Valid state transitions

```
idle ──────────────────────────────────────────────► loading
                                                        │
                                                     (fetch done)
                                                        │
             ◄──────────────────────────────────────── idle
             │
          (sendMessage called)
             │
             ▼
           sending ──────────────────────────────────► streaming
           (POST in flight,                            (first token arrives,
            optimistic msg shown)                       POST may still be in flight)
                                                        │
                                                     (message_complete)
                                                        │
             ◄──────────────────────────────────────── idle
```

Errors are a terminal state from any phase, with `recoverable: true` for stream errors (user can retry) and `recoverable: false` for unrecoverable provider errors.

### What this eliminates

- `isLoadingMessages` map → `phase.status === "loading"`
- `isStreaming` map → `phase.status === "streaming"`
- `isSending` map → `phase.status === "sending"`
- `streamingContent` map → `phase.content` (only exists when `status === "streaming"`)
- `messagesByThread` map → `threads[id].messages`

---

## Component changes

Component reads become more explicit and impossible to misread:

```ts
// Before
const isStreaming = useMessageStore(s => s.isStreaming[thread.id] ?? false)
const streamingContent = useMessageStore(s => s.streamingContent[thread.id] ?? "")
const isLoadingMessages = useMessageStore(s => s.isLoadingMessages[thread.id] ?? false)
const isSending = useMessageStore(s => s.isSending[thread.id] ?? false)

// After
const phase = useMessageStore(s => s.threads[thread.id]?.phase ?? { status: "idle" })

const isStreaming = phase.status === "streaming"
const streamingContent = phase.status === "streaming" ? phase.content : ""
const isLoadingMessages = phase.status === "loading"
const isSending = phase.status === "sending"
```

No other component logic changes. The rendering conditions in `ChatView` stay the same, they just read from one place.

---

## Story 4.1a-1 — Vitest baseline for current store

**Before touching any production code**, write tests that document the current store's behaviour. Some will pass, some will expose the illegal state combinations. This gives us a regression baseline to run against after the refactor.

### Tests to write (`useMessageStore.test.ts`)

**loadMessages**
- [ ] Sets `isLoadingMessages[id]` to true while fetching
- [ ] Sets `isLoadingMessages[id]` to false after fetch resolves
- [ ] Populates `messagesByThread[id]` with returned messages
- [ ] Sets `isLoadingMessages[id]` to false on fetch error
- [ ] Does not affect `isLoadingMessages` for other threads (was broken before 4.1)

**sendMessage**
- [ ] Sets `isStreaming[id]` and `isSending[id]` to true immediately
- [ ] Adds optimistic message to `messagesByThread[id]`
- [ ] Replaces optimistic message with real message on POST success
- [ ] Removes optimistic message and clears flags on POST error
- [ ] Does not affect state for other threads

**appendToken**
- [ ] Appends token to `streamingContent[id]`
- [ ] Ignores empty token strings
- [ ] Sets `isStreaming[id]` true

**finalizeStream**
- [ ] Appends assistant message to `messagesByThread[id]`
- [ ] Sets `isStreaming[id]` to false
- [ ] Clears `streamingContent[id]`
- [ ] Strips optimistic-* messages
- [ ] Is idempotent — calling twice with same message id doesn't duplicate

**Concurrency cases (the ones that were broken)**
- [ ] `loadMessages` resolving after `finalizeStream` does not overwrite the assistant message
- [ ] `appendToken` called while `isLoadingMessages` is true still accumulates content
- [ ] `finalizeStream` called before POST response does not lose the user message

### Setup pattern (follows existing `useThreadStore.test.ts`)

```ts
vi.mock("@/api/client", () => ({
  messagesApi: {
    list: vi.fn(),
    send: vi.fn(),
  },
}))

beforeEach(() => {
  useMessageStore.setState({
    messagesByThread: {},
    streamingContent: {},
    isStreaming: {},
    isSending: {},
    isLoadingMessages: {},
    error: null,
  })
})
```

---

## Story 4.1a-2 — ThreadPhase state machine implementation

Replace `useMessageStore` internals with the `ThreadMap` model. Action signatures stay identical so no call sites break.

### Implementation notes

**`loadMessages`**
- Only transitions to `loading` if current phase is `idle`. If `sending` or `streaming` is in progress, load quietly in the background and merge results without touching the phase.
- On resolve: set phase back to `idle`, update `messages`.
- On error: set phase to `error` with `recoverable: true`.

**`sendMessage`**
- Transitions `idle` → `sending`, sets `optimisticId`.
- On POST success: keep phase as `sending` (tokens haven't started yet), swap optimistic message for real one.
- On first `appendToken`: transition `sending` → `streaming`.
- On POST error: transition back to `idle`, remove optimistic message.

**`appendToken`**
- If phase is `sending`: transition to `streaming` with this token as initial content.
- If phase is `streaming`: append token to `content`.
- If phase is anything else: no-op (prevents stuck streaming state).

**`finalizeStream`**
- Transitions `streaming` → `idle`.
- Appends the completed message, clears streaming content.
- If phase is not `streaming` (e.g. already `idle` from a duplicate event): no-op.

**`setStreamingError`**
- Transitions any phase → `error` with `recoverable: true`.

---

## Story 4.1a-3 — Component updates

Update selectors in `ChatView` and any other component reading the old flags.

### Files to update

- `web/src/components/ChatView.tsx` — main consumer
- `web/src/stores/useMessageStore.ts` — the store itself
- No other components read `isStreaming` or `isLoadingMessages` directly (verify with grep before starting)

### Acceptance criteria

- All existing Vitest tests pass
- All new 4.1a-1 tests pass
- New tests for the state machine transitions (4.1a-2 tests) pass
- `npm run build` clean
- Manual test: new chat streams correctly
- Manual test: reload page, send message, streams correctly
- Manual test: switch between two threads while one is streaming — other thread is unaffected

---

## Story 4.1a-4 — jsdom + React Testing Library setup

Add component-level smoke tests so rendering regressions are caught before manual testing.

### Dependencies to add

```bash
npm install -D @testing-library/react @testing-library/user-event @testing-library/jest-dom jsdom
```

### `vite.config.ts` change

```ts
test: {
  globals: true,
  environment: "jsdom",        // was "node"
  setupFiles: ["./src/test/setup.ts"],
  include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
}
```

### `src/test/setup.ts`

```ts
import "@testing-library/jest-dom"
```

### Smoke tests to add (`ChatView.test.tsx`)

- [ ] Renders loading state when `phase.status === "loading"`
- [ ] Renders empty state when `phase.status === "idle"` and no messages
- [ ] Renders `StreamingBubble` when `phase.status === "streaming"`
- [ ] Does NOT render loading state when `phase.status === "streaming"` (the bug we fixed)
- [ ] Renders message list when `phase.status === "idle"` with messages

These are the exact rendering bugs we've been hitting — each one is a one-line test.

---

## Branching and merge strategy

```
main          ← phase-complete only (Phase 3 is the last merge)
  └── dev     ← feature/phase4-credential-store already merged here
        └── feature/phase4-1a-frontend-state  ← this branch
```

When complete: merge `feature/phase4-1a-frontend-state` → `dev`. Do not merge to `main` until the full Phase 4 feature set (4.2–4.5) is complete and end-to-end verified.

---

## Testing checklist

- [ ] `npm test` passes with all new store unit tests
- [ ] `npm run build` clean (TypeScript strict, no type errors)
- [ ] New chat: message sends, streams token by token, finalizes correctly
- [ ] Existing thread after reload: streams correctly (was broken before 4.1 fix)
- [ ] Two threads open: streaming on one does not affect the other
- [ ] Error case: provider unavailable shows error state, allows retry
- [ ] No `console.error` in browser during normal operation
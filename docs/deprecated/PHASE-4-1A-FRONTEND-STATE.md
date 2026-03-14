# Phase 4.1a — Frontend State Machine Refactor

**Branch:** `feature/phase4-1a-frontend-state`
**Merges into:** `dev`

**Scope:** Replace five independent per-thread boolean maps in `useMessageStore` with a single `ThreadPhase` union type. No backend changes. No new features.

---

## The Problem

Per-thread state is spread across five separate maps:

```ts
messagesByThread:  Record<string, Message[]>
streamingContent:  Record<string, string>
isStreaming:       Record<string, boolean>
isSending:         Record<string, boolean>
isLoadingMessages: Record<string, boolean>
```

Nothing prevents illegal combinations. `isStreaming: true` + `isLoadingMessages: true` simultaneously was the exact bug that hid the `StreamingBubble` behind the loading spinner on reload. The type system offers no protection.

---

## The Model

```ts
type ThreadPhase =
  | { status: "idle" }
  | { status: "sending"; optimisticId: string }
  | { status: "streaming"; content: string }
  | { status: "error"; message: string; recoverable: boolean }

interface ThreadState {
  messages: Message[]
  phase: ThreadPhase
}

type ThreadMap = Record<string, ThreadState>
```

Illegal combinations are structurally impossible. `streaming` and `loading` cannot coexist because they are distinct values of the same field.

---

## Transition Table

| From | Event | To | Notes |
|---|---|---|---|
| any | `loadMessages` | — | Never touches phase. Fetches and merges messages only. |
| `idle` | `sendMessage` | `sending` | Sets `optimisticId` |
| `sending` | first `appendToken` | `streaming` | Content starts accumulating |
| `sending` | POST error | `idle` | Removes optimistic message |
| `streaming` | `finalizeStream` | `idle` | Appends completed message, clears content |
| `streaming` | `setStreamingError` | `error` | Canned error message, `recoverable: true` |
| `error` | `sendMessage` | `sending` | Retry clears error |

### Edge case decisions

**loadMessages during a send:** Low probability window (~100-200ms). `loadMessages` never touches phase — it only updates `messages`. No special handling needed.

**Stream error / partial content:** Show a canned error ("Response interrupted, please try again"). No partial content preserved. Clean transition to `error`.

**Rapid sends while streaming:** Send input is already disabled while `status === "sending" || status === "streaming"`. State machine enforces this structurally.

---

## Store Shape

Action signatures stay identical — no call sites outside the store change.

```ts
interface MessageStore {
  threads: ThreadMap

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

---

## Component Changes

Only `ChatView.tsx` reads the old flags. The change is mechanical:

```ts
// Before — four separate selectors
const isStreaming      = useMessageStore(s => s.isStreaming[thread.id] ?? false)
const streamingContent = useMessageStore(s => s.streamingContent[thread.id] ?? "")
const isLoadingMessages = useMessageStore(s => s.isLoadingMessages[thread.id] ?? false)
const isSending        = useMessageStore(s => s.isSending[thread.id] ?? false)
const messages         = useMessageStore(s => s.messagesByThread[thread.id] ?? [])

// After — one selector
const threadState = useMessageStore(s => s.threads[thread.id] ?? { messages: [], phase: { status: "idle" } })
const { messages, phase } = threadState

const isStreaming       = phase.status === "streaming"
const streamingContent  = phase.status === "streaming" ? phase.content : ""
const isLoadingMessages = phase.status === "loading"  // not a valid phase — see note
const isSending         = phase.status === "sending"
```

Note: `loading` is not a phase in our model — `loadMessages` is transparent to the phase. The loading spinner shows when `messages.length === 0 && phase.status === "idle"` on initial mount.

---

## Stories

**4.1a-1 — Vitest baseline tests** (write first, before any code changes)

Cover current store behaviour so regressions are caught. Follow the pattern in `useThreadStore.test.ts`.

Key cases:
- `loadMessages` populates messages, does not affect other threads
- `sendMessage` adds optimistic message, replaces on success, removes on error
- `appendToken` accumulates content, ignores empty strings
- `finalizeStream` appends message, clears streaming state, is idempotent
- Concurrency: `loadMessages` resolving after `finalizeStream` does not overwrite assistant message

**4.1a-2 — Implement ThreadPhase state machine**

Replace `useMessageStore` internals. Tests from 4.1a-1 must still pass. Add transition tests for the new model.

**4.1a-3 — Update ChatView**

Mechanical selector replacement. Verify with `npm run build`.

**4.1a-4 — jsdom + React Testing Library**

```bash
npm install -D @testing-library/react @testing-library/user-event @testing-library/jest-dom jsdom
```

Update `vite.config.ts` environment to `jsdom`. Add `src/test/setup.ts` importing `@testing-library/jest-dom`.

Smoke tests for `ChatView`:
- Renders empty state when idle with no messages
- Renders message list when idle with messages  
- Renders `StreamingBubble` when streaming
- Does NOT show loading spinner when streaming (the regression we fixed)

---

## Acceptance Criteria

- [ ] `npm test` passes
- [ ] `npm run build` clean
- [ ] New chat: streams token by token
- [ ] Reload page, send message: streams correctly
- [ ] Two threads: streaming on one does not affect the other
- [ ] Provider error: error state shown, send re-enabled
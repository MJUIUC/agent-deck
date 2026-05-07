# 14 — Frontend Architecture

---

## Stack

- **React 18** + TypeScript
- **Vite** for build tooling
- **Zustand** for state management
- **CSS Modules** for scoped styles
- **Vitest** for unit tests

---

## Directory structure

```
web/src/
+-- App.tsx               Root component, routing, layout selection
+-- main.tsx              React DOM entry point
+-- api/
|   +-- client.ts         Typed API client (fetch wrappers)
+-- components/
|   +-- ChatView.tsx       Main chat pane
|   +-- MessageBubble.tsx  Individual message rendering (markdown, tool activity)
|   +-- MessageInput.tsx   Compose box, attachment support
|   +-- Sidebar.tsx        Thread list
|   +-- ConfigPane.tsx     Thread configuration panel
|   +-- settings/          Settings modal sections
|   +-- wizards/           Setup wizard steps
|   +-- toast/             Toast notification system
+-- layouts/
|   +-- DesktopLayout.tsx  3-pane desktop layout
|   +-- MobileLayout.tsx   Mobile layout with bottom navigation
|   +-- mobile/            Mobile-specific views
+-- stores/
|   +-- useThreadStore.ts  Thread list, active thread
|   +-- useMessageStore.ts Message list, streaming state
|   +-- useSseStore.ts     SSE connection management
|   +-- useThemeStore.ts   Light/dark theme
|   +-- tokenBuffer.ts     Smooth token streaming buffer
+-- hooks/
|   +-- useAutoScroll.ts   Auto-scroll to bottom on new messages
|   +-- useIsMobile.ts     Responsive breakpoint detection
|   +-- usePlatform.ts     Browser/PWA/native detection
|   +-- useProcessedMessages.ts  Message grouping + tool activity processing
+-- types/index.ts         Shared TypeScript types
```

---

## State management

### `useThreadStore`

```typescript
interface ThreadStore {
  threads: Thread[];
  activeThreadId: string | null;
  setActiveThread: (id: string) => void;
  loadThreads: () => Promise<void>;
  createThread: (personaId: string) => Promise<Thread>;
  updateThread: (id: string, updates: Partial<Thread>) => Promise<void>;
  deleteThread: (id: string) => Promise<void>;
}
```

Threads are loaded once on mount and updated reactively via global SSE events (`thread_updated`, `title_updated`).

### `useMessageStore`

```typescript
interface MessageStore {
  messages: Record<string, Message[]>;         // keyed by thread_id
  streamingContent: Record<string, string>;    // live token accumulation
  isStreaming: Record<string, boolean>;
  loadMessages: (threadId: string) => Promise<void>;
  handleToken: (threadId: string, token: string) => void;
  handleMessageComplete: (threadId: string, message: Message) => void;
  handleToolActivity: (threadId: string, event: ToolActivityEvent) => void;
}
```

The `streamingContent` map accumulates tokens for the current streaming message. On `MessageComplete`, the streaming content is replaced with the final persisted message.

### `useSseStore`

```typescript
interface SseStore {
  threadConnections: Record<string, EventSource>;
  globalConnection: EventSource | null;
  connectThread: (threadId: string) => void;
  disconnectThread: (threadId: string) => void;
  connectGlobal: () => void;
}
```

Manages `EventSource` lifecycle. When a thread is selected, `connectThread` opens an SSE connection to `/api/threads/:id/stream`. The global stream (`/api/events`) is connected once on app load.

### `tokenBuffer.ts`

Smooths token rendering. Raw tokens arrive at variable rates from the provider. The buffer accumulates tokens and flushes to the message store at a controlled interval (~16ms / 60fps) to prevent janky character-by-character rendering.

---

## SSE event handling

```typescript
// Per-thread stream
eventSource.onmessage = (event) => {
  const data = JSON.parse(event.data);
  switch (data.type) {
    case 'token':
      useMessageStore.getState().handleToken(threadId, data.token);
      break;
    case 'message_complete':
      useMessageStore.getState().handleMessageComplete(threadId, data);
      break;
    case 'tool_start':
      // Show tool activity indicator
      break;
    case 'tool_round_complete':
      // Dismiss tool activity indicators for this round
      break;
    case 'tool_activity':
      useMessageStore.getState().handleToolActivity(threadId, data);
      break;
    case 'error':
      // Show error toast
      break;
    case 'retry':
      // Show retry indicator in UI
      break;
  }
};

// Global stream
globalSource.onmessage = (event) => {
  const data = JSON.parse(event.data);
  switch (data.type) {
    case 'thread_updated':
      useThreadStore.getState().updateThreadPreview(data);
      break;
    case 'title_updated':
      useThreadStore.getState().setThreadTitle(data.thread_id, data.title);
      break;
    case 'mcp_status_changed':
      // Update MCP server status in settings
      break;
  }
};
```

---

## Message rendering

`MessageBubble.tsx` renders:
- Markdown (via a markdown renderer)
- Code blocks with syntax highlighting
- Mermaid diagrams (rendered client-side)
- Tool activity (collapsible call/result pairs)
- File links (`/api/fs/read?path=...`) open the built-in file explorer modal
- Image attachments inline

`useProcessedMessages.ts` groups raw messages:
- Consecutive tool call/result pairs are grouped with their parent assistant message
- Stopped messages show a "Response cancelled" indicator
- Routine messages get a routine badge

---

## Layout

`App.tsx` selects the layout based on `useIsMobile()`:

```typescript
const isMobile = useIsMobile();
return isMobile ? <MobileLayout /> : <DesktopLayout />;
```

**DesktopLayout:** 3-pane — Sidebar | ChatView | ConfigPane  
**MobileLayout:** Single-pane with bottom navigation — ThreadList | Chat | Settings

---

## API client (`api/client.ts`)

Typed fetch wrapper that:
- Adds auth token from localStorage to all requests
- Handles 401 responses (redirects to login)
- Returns typed responses

```typescript
export const api = {
  threads: {
    list: () => fetch('/api/threads', { headers: authHeaders() }),
    create: (personaId: string) => fetch('/api/threads', { method: 'POST', body: JSON.stringify({ persona_id: personaId }), headers: authHeaders() }),
    sendMessage: (threadId: string, content: string, attachments: Attachment[]) => ...
  },
  ...
}
```

---

## Setup wizard

`SetupWizard.tsx` orchestrates a multi-step first-run flow:

1. **Step1Welcome** — Introduction
2. **Step2Name** — Enter display name
3. **Step2bAboutYou** — Optional profile fields
4. **Step3Provider** — Configure first LLM provider
5. **Step4Persona** — Create first persona
6. **StepTailscale** — Optional Tailscale setup
7. **Step5Done** — Completion

On `POST /api/setup/complete`, the server creates user, provider, model, and default persona rows, then marks setup as complete in `app_config`.

---

## PWA support

The app is a Progressive Web App:
- `manifest.json` with icons
- Service worker (`sw.js`) registered via `registerSW.js`
- Web Push subscription managed after service worker registration

This enables "Add to Home Screen" on mobile and push notification delivery when the app is backgrounded.

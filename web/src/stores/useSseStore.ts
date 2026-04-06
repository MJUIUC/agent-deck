import { create } from "zustand";
import { useMessageStore } from "./useMessageStore";
import { useThreadStore } from "./useThreadStore";
import { bufferToken } from "./tokenBuffer";
import type {
  SseThreadEvent,
  SseGlobalEvent,
  SseSystemEventEvent,
  SseToolStartEvent,
  SseToolActivityEvent,
  SseChatSegmentEvent,
} from "@/types";

// Reconnection config
const INITIAL_BACKOFF_MS = 1000;
const MAX_BACKOFF_MS = 30000;
const BACKOFF_MULTIPLIER = 2;

interface SseConnection {
  eventSource: EventSource;
  cleanup: () => void;
}

interface SseStore {
  // State
  threadConnectionId: string | null; // which thread we're connected to
  globalConnected: boolean;
  threadConnected: boolean;
  threadError: string | null;
  globalError: string | null;
  lastMcpStatusChange: { mcp_server_id: string; status: string } | null;

  // Internal (not exposed as reactive state, just held in closure)
  _threadConn: SseConnection | null;
  _globalConn: SseConnection | null;
  _threadBackoff: number;
  _globalBackoff: number;
  _threadReconnectTimer: ReturnType<typeof setTimeout> | null;
  _globalReconnectTimer: ReturnType<typeof setTimeout> | null;

  // Actions
  connectThread: (threadId: string) => void;
  disconnectThread: () => void;
  connectGlobal: () => void;
  disconnectGlobal: () => void;
}

function buildThreadSseUrl(threadId: string): string {
  return `/api/threads/${threadId}/stream`;
}

function buildGlobalSseUrl(): string {
  return `/api/events`;
}

export const useSseStore = create<SseStore>((set, get) => ({
  // ── Initial state ────────────────────────────────────────────────────────────
  threadConnectionId: null,
  globalConnected: false,
  threadConnected: false,
  threadError: null,
  globalError: null,
  lastMcpStatusChange: null,

  _threadConn: null,
  _globalConn: null,
  _threadBackoff: INITIAL_BACKOFF_MS,
  _globalBackoff: INITIAL_BACKOFF_MS,
  _threadReconnectTimer: null,
  _globalReconnectTimer: null,

  // ── Thread stream ────────────────────────────────────────────────────────────

  connectThread: (threadId: string) => {
    const state = get();

    // If already connected to this thread, do nothing
    if (state.threadConnectionId === threadId && state._threadConn) {
      return;
    }

    // Disconnect existing thread connection first
    if (state._threadConn) {
      state._threadConn.cleanup();
    }
    if (state._threadReconnectTimer != null) {
      clearTimeout(state._threadReconnectTimer);
    }

    // Reset backoff for new thread
    set({
      _threadBackoff: INITIAL_BACKOFF_MS,
      _threadReconnectTimer: null,
      threadConnectionId: threadId,
      threadConnected: false,
      threadError: null,
    });

    function open() {
      const currentThreadId = get().threadConnectionId;
      // Bail if the thread changed while we were in a backoff timer
      if (currentThreadId !== threadId) return;

      const es = new EventSource(buildThreadSseUrl(threadId));

      const handleToken = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseThreadEvent;
          if (data.event === "token") {
            bufferToken(threadId, data.token, (tid, buffered) => {
              useMessageStore.getState().appendToken(tid, buffered);
            });
          }
        } catch {
          // ignore parse errors
        }
      };

      const handleMessageComplete = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseThreadEvent;
          if (data.event === "message_complete") {
            const store = useMessageStore.getState();
            store.finalizeStream(threadId, {
              id: data.id,
              thread_id: data.thread_id,
              role: data.role as "user" | "assistant" | "system",
              content: data.content,
              source: "chat",
              routine_id: null,
              visibility: "visible",
              execution_id: null,
              stopped: data.stopped,
              created_at: data.created_at,
            });
            // Fire-and-forget: reload messages to surface tool messages from this turn
            void store.loadMessages(threadId);
          }
        } catch {
          // ignore
        }
      };

      const handleSystemEvent = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseSystemEventEvent;
          if (data.event === "system_event") {
            // Add as a system message so ConfigPane or ChatView can
            // optionally render it. The message is visibility=hidden on the
            // server so it won't appear in paginated loads unless the client
            // explicitly requests it; here we surface it in the live store
            // so connected clients see it immediately.
            useMessageStore.getState().addMessage({
              id: `system-event-${Date.now()}`,
              thread_id: threadId,
              role: "system",
              content: data.content,
              source: "system_event",
              routine_id: null,
              visibility: "hidden",
              execution_id: null,
              event_type: data.event_type,
              created_at: new Date().toISOString(),
            });
          }
        } catch {
          // ignore
        }
      };

      const handleRoutineMessage = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseThreadEvent;
          if (data.event === "routine_message") {
            useMessageStore.getState().addMessage({
              id: data.id,
              thread_id: data.thread_id,
              role: data.role as "user" | "assistant" | "system",
              content: data.content,
              source: "routine",
              routine_id: data.routine_id,
              visibility: "visible",
              execution_id: null,
              created_at: data.created_at,
            });
          }
        } catch {
          // ignore
        }
      };

      const handleToolStart = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseToolStartEvent;
          if (data.event === "tool_start") {
            useMessageStore.getState().beginToolCall(threadId, data.tool_name);
          }
        } catch {
          // ignore parse errors
        }
      };

      const handleToolActivity = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseToolActivityEvent;
          if (data.event === "tool_activity" && data.role === "tool") {
            useMessageStore.getState().completeToolCall(threadId);
          }
        } catch {
          // ignore parse errors
        }
      };

      const handleChatSegment = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseChatSegmentEvent;
          if (data.event === "chat_segment") {
            useMessageStore.getState().commitSegment(threadId, {
              id: data.id,
              thread_id: data.thread_id,
              role: "assistant",
              content: data.content,
              source: "chat",
              routine_id: null,
              visibility: "visible",
              execution_id: null,
              event_type: "chat_segment",
              created_at: data.created_at,
            });
          }
        } catch {
          // ignore parse errors
        }
      };

      // Handles server-sent "stream_error" events (distinct from the built-in
      // EventSource connection error, which is handled by onerror below).
      const handleError = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseThreadEvent;
          if (data.event === "stream_error" || data.event === "error") {
            useMessageStore
              .getState()
              .setStreamingError(threadId, data.message);
          }
        } catch {
          // ignore
        }
      };

      // SSE spec error — connection dropped or server closed it
      const handleConnError = () => {
        // Only attempt reconnect if this is still the active thread
        if (get().threadConnectionId !== threadId) return;

        // If the message store shows this thread was mid-stream, the agent
        // run is gone and message_complete will never arrive. Surface an error
        // so the phase resets to idle and the UI shows a retry prompt instead
        // of staying stuck in a permanent streaming state.
        const msgState = useMessageStore.getState();
        const phase = msgState.threads[threadId]?.phase;
        if (phase?.status === "streaming" || phase?.status === "sending") {
          msgState.setStreamingError(
            threadId,
            "Response interrupted, please try again",
          );
        }

        set({
          threadConnected: false,
          threadError: "Connection lost. Reconnecting…",
          _threadConn: null,
        });

        const backoff = get()._threadBackoff;
        const nextBackoff = Math.min(
          backoff * BACKOFF_MULTIPLIER,
          MAX_BACKOFF_MS,
        );

        const timer = setTimeout(() => {
          if (get().threadConnectionId === threadId) {
            set({ _threadBackoff: nextBackoff, _threadReconnectTimer: null });
            open();
          }
        }, backoff);

        set({ _threadReconnectTimer: timer });
        es.close();
      };

      es.addEventListener("token", handleToken);
      es.addEventListener("message_complete", handleMessageComplete);
      es.addEventListener("routine_message", handleRoutineMessage);
      es.addEventListener("system_event", handleSystemEvent);
      // Named "stream_error" to avoid collision with EventSource's built-in
      // "error" event (which fires for connection drops, not server errors).
      es.addEventListener("stream_error", handleError);
      es.addEventListener("tool_start", handleToolStart);
      es.addEventListener("tool_activity", handleToolActivity);
      es.addEventListener("chat_segment", handleChatSegment);

      es.addEventListener("open", () => {
        if (get().threadConnectionId === threadId) {
          set({
            threadConnected: true,
            threadError: null,
            _threadBackoff: INITIAL_BACKOFF_MS,
          });
        }
      });

      es.onerror = handleConnError;

      const cleanup = () => {
        es.removeEventListener("token", handleToken);
        es.removeEventListener("message_complete", handleMessageComplete);
        es.removeEventListener("routine_message", handleRoutineMessage);
        es.removeEventListener("system_event", handleSystemEvent);
        es.removeEventListener("stream_error", handleError);
        es.removeEventListener("tool_start", handleToolStart);
        es.removeEventListener("tool_activity", handleToolActivity);
        es.removeEventListener("chat_segment", handleChatSegment);
        es.close();
      };

      const conn: SseConnection = { eventSource: es, cleanup };
      set({ _threadConn: conn });
    }

    open();
  },

  disconnectThread: () => {
    const state = get();
    if (state._threadReconnectTimer != null) {
      clearTimeout(state._threadReconnectTimer);
    }
    if (state._threadConn) {
      state._threadConn.cleanup();
    }
    set({
      _threadConn: null,
      threadConnectionId: null,
      threadConnected: false,
      threadError: null,
      _threadReconnectTimer: null,
      _threadBackoff: INITIAL_BACKOFF_MS,
    });
  },

  // ── Global stream ────────────────────────────────────────────────────────────

  connectGlobal: () => {
    const state = get();

    // Already connected
    if (state._globalConn) return;
    if (state._globalReconnectTimer != null) {
      clearTimeout(state._globalReconnectTimer);
    }

    set({
      _globalBackoff: INITIAL_BACKOFF_MS,
      _globalReconnectTimer: null,
      globalConnected: false,
      globalError: null,
    });

    function open() {
      const es = new EventSource(buildGlobalSseUrl());

      const handleThreadUpdated = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as SseGlobalEvent;
          if (data.event === "thread_updated") {
            useThreadStore
              .getState()
              .updateThreadPreview(
                data.thread_id,
                data.last_message,
                data.updated_at,
              );
          }
        } catch {
          // ignore
        }
      };

      const handleTitleUpdated = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as {
            thread_id: string;
            title: string;
          };
          if (data.thread_id && data.title) {
            const store = useThreadStore.getState();
            const existing = store.threads.find((t) => t.id === data.thread_id);
            if (existing) {
              store.upsertThread({ ...existing, title: data.title });
            }
          }
        } catch {
          // ignore
        }
      };

      const handleRoutineFired = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as {
            thread_id: string;
            routine_id: string;
            routine_name: string;
          };
          if (data.thread_id) {
            // Refresh the thread's updated_at so the sidebar re-renders
            useThreadStore
              .getState()
              .updateThreadPreview(
                data.thread_id,
                "",
                new Date().toISOString(),
              );
          }
        } catch {
          // ignore
        }
      };

      const handleMcpStatusChanged = (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as {
            mcp_server_id: string;
            status: string;
          };
          if (data.mcp_server_id && data.status) {
            set({
              lastMcpStatusChange: {
                mcp_server_id: data.mcp_server_id,
                status: data.status,
              },
            });
          }
        } catch {
          // ignore
        }
      };

      const handleConnError = () => {
        set({
          globalConnected: false,
          globalError: "Global stream disconnected. Reconnecting…",
          _globalConn: null,
        });

        const backoff = get()._globalBackoff;
        const nextBackoff = Math.min(
          backoff * BACKOFF_MULTIPLIER,
          MAX_BACKOFF_MS,
        );

        const timer = setTimeout(() => {
          set({ _globalBackoff: nextBackoff, _globalReconnectTimer: null });
          open();
        }, backoff);

        set({ _globalReconnectTimer: timer });
        es.close();
      };

      es.addEventListener("thread_updated", handleThreadUpdated);
      es.addEventListener("title_updated", handleTitleUpdated);
      es.addEventListener("routine_fired", handleRoutineFired);
      es.addEventListener("mcp_status_changed", handleMcpStatusChanged);

      es.addEventListener("open", () => {
        set({
          globalConnected: true,
          globalError: null,
          _globalBackoff: INITIAL_BACKOFF_MS,
        });
      });

      es.onerror = handleConnError;

      const cleanup = () => {
        es.removeEventListener("thread_updated", handleThreadUpdated);
        es.removeEventListener("title_updated", handleTitleUpdated);
        es.removeEventListener("routine_fired", handleRoutineFired);
        es.removeEventListener("mcp_status_changed", handleMcpStatusChanged);
        es.close();
      };

      const conn: SseConnection = { eventSource: es, cleanup };
      set({ _globalConn: conn });
    }

    open();
  },

  disconnectGlobal: () => {
    const state = get();
    if (state._globalReconnectTimer != null) {
      clearTimeout(state._globalReconnectTimer);
    }
    if (state._globalConn) {
      state._globalConn.cleanup();
    }
    set({
      _globalConn: null,
      globalConnected: false,
      globalError: null,
      _globalReconnectTimer: null,
      _globalBackoff: INITIAL_BACKOFF_MS,
    });
  },
}));

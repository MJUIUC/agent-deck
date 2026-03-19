import { create, type StateCreator } from "zustand";
import { cancelTokenBuffer } from "./tokenBuffer";
import zukeeper from "zukeeper";
import { messagesApi } from "@/api/client";
import type {
  Message,
  SlashCommandResponse,
  ThreadMap,
  ThreadPhase,
  ThreadState,
} from "@/types";

// ── Helpers ───────────────────────────────────────────────────────────────────

const IDLE: ThreadPhase = { status: "idle" };

function getThread(threads: ThreadMap, threadId: string): ThreadState {
  return threads[threadId] ?? { messages: [], phase: IDLE };
}

function setThread(
  threads: ThreadMap,
  threadId: string,
  next: Partial<ThreadState>,
): ThreadMap {
  const current = getThread(threads, threadId);
  return {
    ...threads,
    [threadId]: { ...current, ...next },
  };
}

// ── Store interface ───────────────────────────────────────────────────────────

interface MessageStore {
  threads: ThreadMap;

  loadMessages: (threadId: string) => Promise<void>;
  sendMessage: (threadId: string, content: string) => Promise<void>;
  sendCommand: (
    threadId: string,
    input: string,
  ) => Promise<SlashCommandResponse | null>;
  appendToken: (threadId: string, token: string) => void;
  finalizeStream: (threadId: string, message: Message) => void;
  addMessage: (message: Message) => void;
  setStreamingError: (threadId: string, errorMsg: string) => void;
  clearMessages: (threadId: string) => void;
  clearError: (threadId: string) => void;
  cancelRun: (threadId: string) => Promise<void>;
}

// ── Store ─────────────────────────────────────────────────────────────────────

const storeCreator: StateCreator<MessageStore> = (set, get) => ({
  threads: {},

  // ── loadMessages ────────────────────────────────────────────────────────────
  // Never touches phase — only updates messages. This keeps loadMessages
  // transparent to the state machine so it can run concurrently with a send
  // without stomping the streaming/sending phase.

  loadMessages: async (threadId) => {
    try {
      const res = await messagesApi.list(threadId, { limit: 100 });
      set((state) => {
        const thread = getThread(state.threads, threadId);
        return {
          threads: setThread(state.threads, threadId, {
            messages: res.data,
            phase: thread.phase,
          }),
        };
      });
    } catch {
      // Silently swallow — callers can handle UI feedback independently.
      // We do not transition to an error phase here because loadMessages
      // is not part of the phase model.
    }
  },

  // ── sendMessage ─────────────────────────────────────────────────────────────
  // idle → sending (optimisticId set)
  // sending → idle on POST error (optimistic message removed)
  // sending → streaming on first appendToken

  sendMessage: async (threadId, content) => {
    const optimisticId = `optimistic-${Date.now()}`;

    const optimisticUserMsg: Message = {
      id: optimisticId,
      thread_id: threadId,
      role: "user",
      content,
      source: "chat",
      routine_id: null,
      visibility: "visible",
      execution_id: null,
      created_at: new Date().toISOString(),
    };

    // Transition: idle → sending, append optimistic message
    set((state) => {
      const thread = getThread(state.threads, threadId);
      return {
        threads: setThread(state.threads, threadId, {
          messages: [...thread.messages, optimisticUserMsg],
          phase: { status: "sending", optimisticId },
        }),
      };
    });

    try {
      const res = await messagesApi.send(threadId, content);
      const realUserMsg = res.data;

      // Replace optimistic placeholder with real server message.
      // Do NOT touch phase here — let SSE drive phase transitions:
      //   sending → streaming  (first appendToken)
      //   streaming → idle     (finalizeStream on message_complete)
      // Forcing phase to idle here would kill the streaming bubble in the
      // window between POST resolve and the first token arriving over SSE.
      set((state) => {
        const thread = getThread(state.threads, threadId);
        const messages = thread.messages.map((m) =>
          m.id === optimisticId ? realUserMsg : m,
        );
        return {
          threads: setThread(state.threads, threadId, { messages }),
        };
      });
    } catch (err) {
      // Transition: sending → idle, remove optimistic message
      set((state) => {
        const thread = getThread(state.threads, threadId);
        const messages = thread.messages.filter((m) => m.id !== optimisticId);
        return {
          threads: setThread(state.threads, threadId, {
            messages,
            phase: IDLE,
          }),
        };
      });
      throw err;
    }
  },

  // ── sendCommand ─────────────────────────────────────────────────────────────
  // Commands are fire-and-forget from the phase perspective. They do not
  // produce a stream and are not represented in the state machine.

  sendCommand: async (threadId, input) => {
    const trimmed = input.trim();
    const withoutSlash = trimmed.startsWith("/") ? trimmed.slice(1) : trimmed;
    const parts = withoutSlash.split(/\s+/).filter(Boolean);
    const command = parts[0] ?? "";
    const args: string[] = parts.slice(1);

    try {
      const res = await messagesApi.sendCommand(threadId, command, args);
      return res.data;
    } catch {
      return null;
    }
  },

  // ── appendToken ─────────────────────────────────────────────────────────────
  // sending → streaming (on first non-empty token)
  // streaming → streaming (content accumulates)
  // Empty tokens are ignored — the server used to send an empty token as a
  // connection handshake which incorrectly kept isStreaming alive.

  appendToken: (threadId, token) => {
    if (!token) return;
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const currentContent =
        thread.phase.status === "streaming" ? thread.phase.content : "";
      return {
        threads: setThread(state.threads, threadId, {
          phase: { status: "streaming", content: currentContent + token },
        }),
      };
    });
  },

  // ── finalizeStream ──────────────────────────────────────────────────────────
  // streaming → idle
  // Strips optimistic messages, appends the completed assistant message,
  // and is idempotent (safe to call twice).
  // If the message has stopped=true (cancelled mid-stream), it is still
  // appended — the UI renders a "stopped" label on it via MessageBubble.

  finalizeStream: (threadId, message) => {
    cancelTokenBuffer(threadId);
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const existing = thread.messages.filter(
        (m) => !m.id.startsWith("optimistic-"),
      );
      const alreadyExists = existing.some((m) => m.id === message.id);
      const messages = alreadyExists ? existing : [...existing, message];
      return {
        threads: setThread(state.threads, threadId, {
          messages,
          phase: IDLE,
        }),
      };
    });
  },

  // ── addMessage ──────────────────────────────────────────────────────────────
  // Appends a message without touching phase. Used for out-of-band messages
  // such as routine messages arriving over SSE.

  addMessage: (message) => {
    set((state) => {
      const thread = getThread(state.threads, message.thread_id);
      if (thread.messages.some((m) => m.id === message.id)) return state;
      return {
        threads: setThread(state.threads, message.thread_id, {
          messages: [...thread.messages, message],
        }),
      };
    });
  },

  // ── setStreamingError ───────────────────────────────────────────────────────
  // streaming → error (recoverable: true)
  // Strips optimistic messages, appends a synthetic error message into the
  // message list, and surfaces the error phase so the UI can show a retry CTA.

  setStreamingError: (threadId, errorMsg) => {
    const errorMessage: Message = {
      id: `error-${Date.now()}`,
      thread_id: threadId,
      role: "assistant",
      content: errorMsg,
      source: "chat",
      routine_id: null,
      visibility: "visible",
      execution_id: null,
      created_at: new Date().toISOString(),
    };

    set((state) => {
      const thread = getThread(state.threads, threadId);
      const messages = [
        ...thread.messages.filter((m) => !m.id.startsWith("optimistic-")),
        errorMessage,
      ];
      return {
        threads: setThread(state.threads, threadId, {
          messages,
          phase: { status: "error", message: errorMsg, recoverable: true },
        }),
      };
    });
  },

  // ── clearMessages ────────────────────────────────────────────────────────────

  clearMessages: (threadId) => {
    set((state) => {
      const next = { ...state.threads };
      delete next[threadId];
      return { threads: next };
    });
  },

  // ── clearError ───────────────────────────────────────────────────────────────
  // error → idle

  clearError: (threadId) => {
    set((state) => {
      const thread = getThread(state.threads, threadId);
      if (thread.phase.status !== "error") return state;
      return {
        threads: setThread(state.threads, threadId, { phase: IDLE }),
      };
    });
  },

  // ── cancelRun ────────────────────────────────────────────────────────────────
  // Sends a cancellation request to the server and immediately clears the
  // streaming state so the UI stops showing the streaming bubble.  We don't
  // wait for the server's message_complete event to drive phase back to idle
  // because the run may already be complete (cancel arrived too late) or the
  // SSE event may take a moment — either way the user pressed stop and expects
  // the UI to respond instantly.
  //
  // finalizeStream is idempotent, so if a message_complete SSE arrives later
  // (stopped=true from the server, or the natural completion if the run
  // finished before cancel landed) it will just commit the final message text
  // without re-entering streaming phase.

  cancelRun: async (threadId) => {
    // Optimistically flush the token buffer and collapse the streaming bubble.
    cancelTokenBuffer(threadId);
    set((state) => {
      const thread = getThread(state.threads, threadId);
      // Only act if we are actually in a streaming/sending state — do not
      // stomp an idle or error phase if cancel is called spuriously.
      if (
        thread.phase.status !== "streaming" &&
        thread.phase.status !== "sending"
      ) {
        return state;
      }
      // Strip optimistic messages (they haven't been confirmed by the server)
      // and return to idle so the input is immediately re-enabled.
      const messages = thread.messages.filter(
        (m) => !m.id.startsWith("optimistic-"),
      );
      return {
        threads: setThread(state.threads, threadId, {
          messages,
          phase: IDLE,
        }),
      };
    });

    try {
      await messagesApi.cancel(threadId);
    } catch {
      // Silently swallow — if the POST fails the run will complete naturally
      // and message_complete will arrive over SSE, which is fine.
    }
  },
});

// Only wrap with zukeeper in a real browser dev environment — it uses
// window.postMessage for devtools and swallows function return values,
// which breaks async store actions in tests.
const isDev =
  typeof window !== "undefined" &&
  typeof process !== "undefined" &&
  process.env.NODE_ENV === "development";

export const useMessageStore = create<MessageStore>(
  isDev ? zukeeper(storeCreator) : storeCreator,
);

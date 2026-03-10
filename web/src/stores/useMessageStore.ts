import { create } from "zustand";
import { messagesApi } from "@/api/client";
import type { Message } from "@/types";

/**
 * Merge a list of server-confirmed messages with the current in-memory list.
 * - Optimistic messages (id starts with "optimistic-") are dropped in favour
 *   of the real server copies.
 * - Non-optimistic messages already in `current` are kept as-is (preserves
 *   any streaming state that may have appended them before the list returned).
 * - Server messages not yet in `current` are appended.
 * The result is sorted by `created_at` ascending.
 */
function mergeMessages(current: Message[], fromServer: Message[]): Message[] {
  // Build a set of real ids already present
  const existingIds = new Set(
    current.filter((m) => !m.id.startsWith("optimistic-")).map((m) => m.id),
  );

  // Keep non-optimistic current messages, then append any server messages
  // we haven't seen yet.
  const kept = current.filter((m) => !m.id.startsWith("optimistic-"));
  const newFromServer = fromServer.filter((m) => !existingIds.has(m.id));
  const merged = [...kept, ...newFromServer];

  // Sort by created_at so order is stable
  merged.sort(
    (a, b) =>
      new Date(a.created_at).getTime() - new Date(b.created_at).getTime(),
  );
  return merged;
}

interface MessageStore {
  // State
  messagesByThread: Record<string, Message[]>;
  streamingContent: Record<string, string>; // threadId -> accumulated tokens
  isStreaming: Record<string, boolean>;
  isSending: Record<string, boolean>;
  isLoadingMessages: boolean;
  error: string | null;

  // Actions
  loadMessages: (threadId: string) => Promise<void>;
  sendMessage: (threadId: string, content: string) => Promise<void>;
  sendCommand: (
    threadId: string,
    input: string,
  ) => Promise<{ type: string; message: string } | null>;
  appendToken: (threadId: string, token: string) => void;
  finalizeStream: (threadId: string, message: Message) => void;
  addMessage: (message: Message) => void;
  setStreamingError: (threadId: string, errorMsg: string) => void;
  clearMessages: (threadId: string) => void;
  clearError: () => void;
}

export const useMessageStore = create<MessageStore>((set) => ({
  // ── Initial state ───────────────────────────────────────────────────────────
  messagesByThread: {},
  streamingContent: {},
  isStreaming: {},
  isSending: {},
  isLoadingMessages: false,
  error: null,

  // ── Actions ─────────────────────────────────────────────────────────────────

  loadMessages: async (threadId) => {
    set({ isLoadingMessages: true, error: null });
    try {
      const res = await messagesApi.list(threadId, { limit: 100 });
      set((state) => ({
        messagesByThread: {
          ...state.messagesByThread,
          [threadId]: res.data,
        },
        isLoadingMessages: false,
      }));
    } catch (err) {
      set({
        isLoadingMessages: false,
        error: err instanceof Error ? err.message : "Failed to load messages",
      });
    }
  },

  sendMessage: async (threadId, content) => {
    set((state) => ({
      isSending: { ...state.isSending, [threadId]: true },
      // Don't pre-set isStreaming here — let actual token events drive it.
      // Pre-setting it caused a permanent "typing" indicator when the SSE
      // connection wasn't established before the agent started firing.
      streamingContent: { ...state.streamingContent, [threadId]: "" },
      error: null,
    }));

    // Optimistically add the user message so it appears immediately
    const optimisticUserMsg: Message = {
      id: `optimistic-${Date.now()}`,
      thread_id: threadId,
      role: "user",
      content,
      source: "chat",
      routine_id: null,
      visibility: "visible",
      execution_id: null,
      created_at: new Date().toISOString(),
    };

    set((state) => ({
      messagesByThread: {
        ...state.messagesByThread,
        [threadId]: [
          ...(state.messagesByThread[threadId] ?? []),
          optimisticUserMsg,
        ],
      },
    }));

    try {
      // Fire the message — the real user message and assistant response
      // arrive via SSE (token / message_complete events).
      await messagesApi.send(threadId, content);

      // After the POST returns the user message is persisted. Reload now so
      // the optimistic placeholder is replaced with the real message (correct
      // id, created_at, etc.) and any already-completed assistant reply is
      // picked up in case streaming was missed while the SSE connection was
      // still being established.
      try {
        const res = await messagesApi.list(threadId, { limit: 100 });
        set((state) => ({
          messagesByThread: {
            ...state.messagesByThread,
            // Merge: keep any messages already present (e.g. streaming tokens
            // that arrived between POST return and the list response) but
            // replace optimistic entries with real ones.
            [threadId]: mergeMessages(
              state.messagesByThread[threadId] ?? [],
              res.data,
            ),
          },
        }));
      } catch {
        // Non-fatal — SSE will still deliver the response.
      }
    } catch (err) {
      // On error, remove the optimistic message and surface the error
      set((state) => ({
        messagesByThread: {
          ...state.messagesByThread,
          [threadId]: (state.messagesByThread[threadId] ?? []).filter(
            (m) => m.id !== optimisticUserMsg.id,
          ),
        },
        isSending: { ...state.isSending, [threadId]: false },
        isStreaming: { ...state.isStreaming, [threadId]: false },
        streamingContent: { ...state.streamingContent, [threadId]: "" },
        error: err instanceof Error ? err.message : "Failed to send message",
      }));
      throw err;
    } finally {
      set((state) => ({
        isSending: { ...state.isSending, [threadId]: false },
      }));
    }
  },

  sendCommand: async (threadId, input) => {
    // Parse "/<command> <args>"
    const trimmed = input.trim();
    const withoutSlash = trimmed.startsWith("/") ? trimmed.slice(1) : trimmed;
    const spaceIdx = withoutSlash.indexOf(" ");
    const command =
      spaceIdx === -1 ? withoutSlash : withoutSlash.slice(0, spaceIdx);
    const args = spaceIdx === -1 ? "" : withoutSlash.slice(spaceIdx + 1).trim();

    set((state) => ({
      isSending: { ...state.isSending, [threadId]: true },
      error: null,
    }));

    try {
      const res = await messagesApi.sendCommand(threadId, command, args);
      return res.data;
    } catch (err) {
      set({
        error: err instanceof Error ? err.message : "Failed to execute command",
      });
      return null;
    } finally {
      set((state) => ({
        isSending: { ...state.isSending, [threadId]: false },
      }));
    }
  },

  appendToken: (threadId, token) => {
    // Ignore empty tokens — the server used to send an empty token as a
    // connection handshake which incorrectly set isStreaming=true forever.
    if (!token) return;
    set((state) => ({
      isStreaming: { ...state.isStreaming, [threadId]: true },
      streamingContent: {
        ...state.streamingContent,
        [threadId]: (state.streamingContent[threadId] ?? "") + token,
      },
    }));
  },

  finalizeStream: (threadId, message) => {
    set((state) => {
      // Replace the optimistic user message + any streaming placeholder
      // with the real persisted messages from the server.
      // We keep all non-optimistic messages and append the completed one.
      const existing = (state.messagesByThread[threadId] ?? []).filter(
        (m) => !m.id.startsWith("optimistic-"),
      );

      // Avoid duplicate if message_complete fires after messages were already loaded
      const alreadyExists = existing.some((m) => m.id === message.id);
      const updated = alreadyExists ? existing : [...existing, message];

      return {
        messagesByThread: {
          ...state.messagesByThread,
          [threadId]: updated,
        },
        streamingContent: { ...state.streamingContent, [threadId]: "" },
        isStreaming: { ...state.isStreaming, [threadId]: false },
      };
    });
  },

  addMessage: (message) => {
    set((state) => {
      const existing = state.messagesByThread[message.thread_id] ?? [];
      // Skip if already present
      if (existing.some((m) => m.id === message.id)) return state;
      return {
        messagesByThread: {
          ...state.messagesByThread,
          [message.thread_id]: [...existing, message],
        },
      };
    });
  },

  setStreamingError: (threadId, errorMsg) => {
    // Add a synthetic error message into the message list
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

    set((state) => ({
      messagesByThread: {
        ...state.messagesByThread,
        [threadId]: [
          ...(state.messagesByThread[threadId] ?? []).filter(
            (m) => !m.id.startsWith("optimistic-"),
          ),
          errorMessage,
        ],
      },
      streamingContent: { ...state.streamingContent, [threadId]: "" },
      isStreaming: { ...state.isStreaming, [threadId]: false },
      isSending: { ...state.isSending, [threadId]: false },
    }));
  },

  clearMessages: (threadId) => {
    set((state) => {
      const next = { ...state.messagesByThread };
      delete next[threadId];
      return { messagesByThread: next };
    });
  },

  clearError: () => set({ error: null }),
}));

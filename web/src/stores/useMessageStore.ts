import { create } from "zustand";
import { messagesApi } from "@/api/client";
import type { Message, SlashCommandResponse } from "@/types";

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
  ) => Promise<SlashCommandResponse | null>;
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
      set((state) => {
        const incoming = res.data;
        const existing = state.messagesByThread[threadId] ?? [];

        // If a stream is already in progress (or has already completed and
        // added messages that the server hasn't returned yet in this fetch),
        // merge rather than overwrite.  Strategy: start with the server list
        // as the source of truth for persisted messages, then re-append any
        // locally-known messages whose IDs aren't in the server response yet
        // (optimistic user message, already-finalized assistant message).
        const incomingIds = new Set(incoming.map((m) => m.id));
        const localOnly = existing.filter((m) => !incomingIds.has(m.id));

        return {
          messagesByThread: {
            ...state.messagesByThread,
            [threadId]: [...incoming, ...localOnly],
          },
          isLoadingMessages: false,
        };
      });
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
      isStreaming: { ...state.isStreaming, [threadId]: true },
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
      // Fire the message. The response body contains the persisted user
      // message — use it to swap out the optimistic copy.
      const res = await messagesApi.send(threadId, content);
      const realUserMsg = res.data;

      set((state) => {
        const current = state.messagesByThread[threadId] ?? [];
        // Replace the optimistic placeholder with the real server message if
        // it's still in the list.  If finalizeStream already ran and stripped
        // it (old behaviour, now removed), fall back to a no-op — the message
        // won't be duplicated because we check by id.
        const optimisticStillPresent = current.some(
          (m) => m.id === optimisticUserMsg.id,
        );
        const alreadyReal = current.some((m) => m.id === realUserMsg.id);

        let next: typeof current;
        if (optimisticStillPresent) {
          // Normal path: swap optimistic → real, preserving list order.
          next = current.map((m) =>
            m.id === optimisticUserMsg.id ? realUserMsg : m,
          );
        } else if (!alreadyReal) {
          // finalizeStream ran first and stripped the optimistic message
          // before we got here — re-insert the real user message at the
          // front of the local-only messages so history stays coherent.
          // Find the assistant reply that finalizeStream appended and insert
          // the user message immediately before it.
          const assistantIdx = current.findIndex(
            (m) => m.role === "assistant" && m.id !== optimisticUserMsg.id,
          );
          if (assistantIdx !== -1) {
            next = [
              ...current.slice(0, assistantIdx),
              realUserMsg,
              ...current.slice(assistantIdx),
            ];
          } else {
            next = [realUserMsg, ...current];
          }
        } else {
          // Already present (e.g. loadMessages ran and fetched it) — no-op.
          next = current;
        }

        return {
          messagesByThread: {
            ...state.messagesByThread,
            [threadId]: next,
          },
        };
      });
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
    // Parse "/<command> [arg1] [arg2] ..." into separate fields.
    // e.g. "/model switch gpt-4o" → command: "model", args: ["switch", "gpt-4o"]
    const trimmed = input.trim();
    const withoutSlash = trimmed.startsWith("/") ? trimmed.slice(1) : trimmed;
    const parts = withoutSlash.split(/\s+/).filter(Boolean);
    const command = parts[0] ?? "";
    const args: string[] = parts.slice(1);

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
      const existing = state.messagesByThread[threadId] ?? [];

      // Avoid duplicate if message_complete fires after messages were already loaded
      const alreadyExists = existing.some((m) => m.id === message.id);

      // Do NOT strip optimistic-* messages here. The sendMessage handler is
      // responsible for swapping the optimistic placeholder once the POST
      // response arrives.  If we strip it here and the POST response hasn't
      // come back yet, sendMessage's .map() will find nothing to replace and
      // the user message disappears from the list entirely.
      //
      // The one exception: if the POST response has already come back and
      // replaced the optimistic message with the real one (identified by a
      // real UUID), it will already be in `existing` and won't be affected.
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

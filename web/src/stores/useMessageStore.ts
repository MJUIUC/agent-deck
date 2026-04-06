import { create, type StateCreator } from "zustand";
import { cancelTokenBuffer } from "./tokenBuffer";
import zukeeper from "zukeeper";
import { messagesApi } from "@/api/client";
import type {
  Message,
  ProcessingRound,
  SlashCommandResponse,
  StreamingEntry,
  ThreadMap,
  ThreadPhase,
  ThreadState,
  ToolCallEntry,
} from "@/types";

// ── Helpers ───────────────────────────────────────────────────────────────────

const IDLE: ThreadPhase = { status: "idle" };

function getThread(threads: ThreadMap, threadId: string): ThreadState {
  return (
    threads[threadId] ?? {
      messages: [],
      phase: IDLE,
      queuedCount: 0,
      oldestLoadedId: null,
      hasMore: false,
      isLoadingMore: false,
    }
  );
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
  loadMoreMessages: (threadId: string) => Promise<void>;
  sendMessage: (threadId: string, content: string) => Promise<void>;
  sendCommand: (
    threadId: string,
    input: string,
  ) => Promise<SlashCommandResponse | null>;
  appendToken: (threadId: string, token: string) => void;
  finalizeStream: (threadId: string, message: Message) => void;
  commitSegment: (threadId: string, message: Message) => void;
  addMessage: (message: Message) => void;
  setStreamingError: (threadId: string, errorMsg: string) => void;
  clearMessages: (threadId: string) => void;
  clearError: (threadId: string) => void;
  cancelRun: (threadId: string) => Promise<void>;
  beginProcessingRound: (
    threadId: string,
    entry: ToolCallEntry,
    round: number,
  ) => void;
  setToolCallMessageId: (
    threadId: string,
    toolCallId: string,
    messageId: string,
  ) => void;
  completeToolCallEntry: (
    threadId: string,
    toolCallId: string,
    messageId: string,
  ) => void;
  completeProcessingRound: (threadId: string, round: number) => void;
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
      const res = await messagesApi.list(threadId, {
        limit: 50,
        include_hidden: true,
      });
      const msgs = res.data;
      const hasMore = res.has_more;
      const oldestLoadedId = msgs[0]?.id ?? null;
      set((state) => {
        const thread = getThread(state.threads, threadId);
        return {
          threads: setThread(state.threads, threadId, {
            messages: msgs,
            phase: thread.phase,
            oldestLoadedId,
            hasMore,
            isLoadingMore: false,
          }),
        };
      });
    } catch {
      // Silently swallow — callers can handle UI feedback independently.
      // We do not transition to an error phase here because loadMessages
      // is not part of the phase model.
    }
  },

  // ── loadMoreMessages ─────────────────────────────────────────────────────────
  // Fetches the next page of older messages using the oldest loaded message ID
  // as the `before` cursor. Prepends results to the existing message list.
  // Guards against double-fetching via isLoadingMore.

  loadMoreMessages: async (threadId) => {
    const thread = getThread(get().threads, threadId);
    if (!thread.hasMore || thread.isLoadingMore || !thread.oldestLoadedId)
      return;

    set((state) => ({
      threads: setThread(state.threads, threadId, { isLoadingMore: true }),
    }));

    try {
      const res = await messagesApi.list(threadId, {
        limit: 50,
        before: thread.oldestLoadedId,
        include_hidden: true,
      });

      set((state) => {
        const current = getThread(state.threads, threadId);
        return {
          threads: setThread(state.threads, threadId, {
            messages: [...res.data, ...current.messages],
            oldestLoadedId: res.data[0]?.id ?? current.oldestLoadedId,
            hasMore: res.has_more,
            isLoadingMore: false,
          }),
        };
      });
    } catch {
      set((state) => ({
        threads: setThread(state.threads, threadId, { isLoadingMore: false }),
      }));
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

    // If a run is already active (streaming or in the optimistic sending
    // window), don't overwrite the phase — just append the optimistic message
    // and increment the queue counter so the UI can show "N queued".
    // If idle (or error), transition normally to "sending".
    let wasActive = false;
    set((state) => {
      const thread = getThread(state.threads, threadId);
      wasActive =
        thread.phase.status === "streaming" ||
        thread.phase.status === "sending";
      return {
        threads: setThread(state.threads, threadId, {
          messages: [...thread.messages, optimisticUserMsg],
          ...(wasActive
            ? { queuedCount: (thread.queuedCount ?? 0) + 1 }
            : {
                phase: { status: "sending", optimisticId },
                queuedCount: thread.queuedCount ?? 0,
              }),
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
      // Roll back: remove the optimistic message and undo the queue increment.
      // If we were active (didn't change phase), leave the phase alone.
      // If we were idle (set phase to sending), revert to idle.
      set((state) => {
        const thread = getThread(state.threads, threadId);
        const messages = thread.messages.filter((m) => m.id !== optimisticId);
        return {
          threads: setThread(state.threads, threadId, {
            messages,
            ...(wasActive ? {} : { phase: IDLE }),
            queuedCount: Math.max(0, (thread.queuedCount ?? 0) - 1),
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
  // streaming → streaming (last text entry accumulates, or new text entry added)
  // Empty tokens are ignored — the server used to send an empty token as a
  // connection handshake which incorrectly kept isStreaming alive.

  appendToken: (threadId, token) => {
    if (!token) return;
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const phase = thread.phase;

      if (phase.status === "sending" || phase.status === "streaming") {
        const entries = phase.status === "streaming" ? phase.entries : [];
        const lastEntry = entries[entries.length - 1];

        if (lastEntry?.type === "processing") {
          const lastRound = lastEntry.rounds[lastEntry.rounds.length - 1];
          if (!lastRound || lastRound.status === "in_progress") {
            // Tools are still running — ignore token
            return state;
          }
          // Last round completed — append token to its reasoning
          const newRounds: ProcessingRound[] = [
            ...lastEntry.rounds.slice(0, -1),
            { ...lastRound, reasoning: lastRound.reasoning + token },
          ];
          return {
            threads: setThread(state.threads, threadId, {
              phase: {
                status: "streaming",
                entries: [
                  ...entries.slice(0, -1),
                  { type: "processing", rounds: newRounds },
                ],
              },
            }),
          };
        }

        const newEntries: StreamingEntry[] =
          !lastEntry || lastEntry.type !== "text"
            ? [...entries, { type: "text", content: token }]
            : [
                ...entries.slice(0, -1),
                { type: "text", content: lastEntry.content + token },
              ];

        return {
          threads: setThread(state.threads, threadId, {
            phase: { status: "streaming", entries: newEntries },
          }),
        };
      }

      if (phase.status === "idle" && (thread.queuedCount ?? 0) > 0) {
        return {
          threads: setThread(state.threads, threadId, {
            phase: {
              status: "streaming",
              entries: [{ type: "text", content: token }],
            },
            queuedCount: Math.max(0, (thread.queuedCount ?? 0) - 1),
          }),
        };
      }

      // Any other phase (error, or idle with no queue) — ignore stale tokens.
      // After a cancel (phase = idle, queuedCount = 0) stale SSE tokens from
      // the cancelled run must not reopen the streaming bubble.
      return state;
    });
  },

  // ── beginProcessingRound ─────────────────────────────────────────────────────
  // Called when tool_start arrives. Creates the processing entry if it doesn't
  // exist, then adds a new ToolCallEntry to the matching round.

  beginProcessingRound: (threadId, entry, round) => {
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const phase = thread.phase;
      if (phase.status !== "streaming") return state;

      const entries = phase.entries;
      const lastEntry = entries[entries.length - 1];

      if (lastEntry?.type === "processing") {
        const existingRound = lastEntry.rounds.find((r) => r.round === round);
        let newRounds: ProcessingRound[];
        if (existingRound) {
          newRounds = lastEntry.rounds.map((r) =>
            r.round === round ? { ...r, tools: [...r.tools, entry] } : r,
          );
        } else {
          newRounds = [
            ...lastEntry.rounds,
            {
              round,
              tools: [entry],
              status: "in_progress" as const,
              reasoning: "",
            },
          ];
        }
        return {
          threads: setThread(state.threads, threadId, {
            phase: {
              status: "streaming",
              entries: [
                ...entries.slice(0, -1),
                { type: "processing", rounds: newRounds },
              ],
            },
          }),
        };
      }

      return {
        threads: setThread(state.threads, threadId, {
          phase: {
            status: "streaming",
            entries: [
              ...entries,
              {
                type: "processing",
                rounds: [
                  {
                    round,
                    tools: [entry],
                    status: "in_progress" as const,
                    reasoning: "",
                  },
                ],
              },
            ],
          },
        }),
      };
    });
  },

  // ── setToolCallMessageId ──────────────────────────────────────────────────────
  // Called when tool_activity role=assistant arrives. Sets call_message_id on
  // the matching entry.

  setToolCallMessageId: (threadId, toolCallId, messageId) => {
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const phase = thread.phase;
      if (phase.status !== "streaming") return state;

      const entries = phase.entries.map((entry) => {
        if (entry.type !== "processing") return entry;
        return {
          ...entry,
          rounds: entry.rounds.map((r) => ({
            ...r,
            tools: r.tools.map((tool) =>
              tool.tool_call_id === toolCallId
                ? { ...tool, call_message_id: messageId }
                : tool,
            ),
          })),
        };
      });

      return {
        threads: setThread(state.threads, threadId, {
          phase: { status: "streaming", entries },
        }),
      };
    });
  },

  // ── completeToolCallEntry ─────────────────────────────────────────────────────
  // Called when tool_activity role=tool arrives. Sets result_message_id and
  // marks the entry as completed.

  completeToolCallEntry: (threadId, toolCallId, messageId) => {
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const phase = thread.phase;
      if (phase.status !== "streaming") return state;

      const entries = phase.entries.map((entry) => {
        if (entry.type !== "processing") return entry;
        return {
          ...entry,
          rounds: entry.rounds.map((r) => ({
            ...r,
            tools: r.tools.map((tool) =>
              tool.tool_call_id === toolCallId
                ? {
                    ...tool,
                    result_message_id: messageId,
                    status: "completed" as const,
                  }
                : tool,
            ),
          })),
        };
      });

      return {
        threads: setThread(state.threads, threadId, {
          phase: { status: "streaming", entries },
        }),
      };
    });
  },

  // ── completeProcessingRound ───────────────────────────────────────────────────
  // Called when tool_round_complete arrives. Marks the matching round as
  // completed.

  completeProcessingRound: (threadId, round) => {
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const phase = thread.phase;
      if (phase.status !== "streaming") return state;

      const entries = phase.entries.map((entry) => {
        if (entry.type !== "processing") return entry;
        return {
          ...entry,
          rounds: entry.rounds.map((r) =>
            r.round === round ? { ...r, status: "completed" as const } : r,
          ),
        };
      });

      return {
        threads: setThread(state.threads, threadId, {
          phase: { status: "streaming", entries },
        }),
      };
    });
  },

  // ── commitSegment ───────────────────────────────────────────────────────────
  // Called when a chat_segment SSE event arrives. The server has persisted the
  // pre-tool text as its own message; we anchor it in the store and clear the
  // text entries from the streaming phase so the processing entry has a
  // clean, unambiguous slot to render in before the next sub-turn begins.

  commitSegment: (threadId, message) => {
    cancelTokenBuffer(threadId);
    set((state) => {
      const thread = getThread(state.threads, threadId);
      const phase = thread.phase;
      if (phase.status !== "streaming") return state;

      // Drop text entries — they've been committed to the segment message.
      // Any tool_call entries are preserved (shouldn't exist yet, but be safe).
      const remainingEntries = phase.entries.filter((e) => e.type !== "text");

      const alreadyExists = thread.messages.some((m) => m.id === message.id);
      const messages = alreadyExists
        ? thread.messages
        : [...thread.messages, message];

      return {
        threads: setThread(state.threads, threadId, {
          messages,
          phase: { status: "streaming", entries: remainingEntries },
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

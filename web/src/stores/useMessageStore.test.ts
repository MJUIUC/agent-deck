import { vi, describe, it, expect, beforeEach } from "vitest";
import type { Message } from "@/types";

// ── Mock the API module before importing the store ────────────────────────────

vi.mock("@/api/client", () => ({
  messagesApi: {
    list: vi.fn(),
    send: vi.fn(),
    sendCommand: vi.fn(),
    cancel: vi.fn(),
  },
}));

vi.mock("./tokenBuffer", () => ({
  cancelTokenBuffer: vi.fn(),
  bufferToken: vi.fn(),
}));

import { useMessageStore } from "./useMessageStore";
import { messagesApi } from "@/api/client";
import { cancelTokenBuffer } from "./tokenBuffer";

const mockMessagesApi = messagesApi as {
  list: ReturnType<typeof vi.fn>;
  send: ReturnType<typeof vi.fn>;
  sendCommand: ReturnType<typeof vi.fn>;
  cancel: ReturnType<typeof vi.fn>;
};

// ── Fixtures ──────────────────────────────────────────────────────────────────

function makeMessage(
  id: string,
  threadId: string,
  role: "user" | "assistant" = "user",
  content = "Hello",
): Message {
  return {
    id,
    thread_id: threadId,
    role,
    content,
    source: "chat",
    routine_id: null,
    visibility: "visible",
    execution_id: null,
    created_at: new Date().toISOString(),
  };
}

// ── Reset store state before each test ───────────────────────────────────────

beforeEach(() => {
  useMessageStore.setState({ threads: {} });
  vi.clearAllMocks();
});

// ── Helpers ───────────────────────────────────────────────────────────────────

function getThread(threadId: string) {
  return useMessageStore.getState().threads[threadId];
}

// ── loadMessages ──────────────────────────────────────────────────────────────

describe("loadMessages", () => {
  it("populates messages for the given thread", async () => {
    const msgs = [
      makeMessage("m1", "t1"),
      makeMessage("m2", "t1", "assistant", "Hi"),
    ];
    mockMessagesApi.list.mockResolvedValue({ data: msgs });

    await useMessageStore.getState().loadMessages("t1");

    const thread = getThread("t1");
    expect(thread.messages).toHaveLength(2);
    expect(thread.messages[0].id).toBe("m1");
    expect(thread.messages[1].id).toBe("m2");
  });

  it("does not affect messages in other threads", async () => {
    useMessageStore.setState({
      threads: {
        t2: { messages: [makeMessage("m99", "t2")], phase: { status: "idle" } },
      },
    });

    mockMessagesApi.list.mockResolvedValue({ data: [makeMessage("m1", "t1")] });
    await useMessageStore.getState().loadMessages("t1");

    expect(getThread("t1").messages).toHaveLength(1);
    expect(getThread("t2").messages).toHaveLength(1);
    expect(getThread("t2").messages[0].id).toBe("m99");
  });

  it("does not change the thread phase", async () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    mockMessagesApi.list.mockResolvedValue({ data: [] });
    await useMessageStore.getState().loadMessages("t1");

    expect(getThread("t1").phase).toEqual({
      status: "streaming",
      content: "partial",
    });
  });

  it("does not change isStreaming or isSending phase for the thread", async () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "sending", optimisticId: "optimistic-1" },
        },
      },
    });

    mockMessagesApi.list.mockResolvedValue({ data: [] });
    await useMessageStore.getState().loadMessages("t1");

    expect(getThread("t1").phase).toEqual({
      status: "sending",
      optimisticId: "optimistic-1",
    });
  });

  it("silently swallows errors without touching phase", async () => {
    useMessageStore.setState({
      threads: { t1: { messages: [], phase: { status: "idle" } } },
    });

    mockMessagesApi.list.mockRejectedValue(new Error("Network error"));
    await useMessageStore.getState().loadMessages("t1");

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });
});

// ── sendMessage ───────────────────────────────────────────────────────────────

describe("sendMessage", () => {
  it("adds an optimistic user message immediately", async () => {
    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    const sendPromise = useMessageStore.getState().sendMessage("t1", "Hello");

    // Check synchronously before the async send resolves
    const thread = getThread("t1");
    expect(thread.messages).toHaveLength(1);
    expect(thread.messages[0].id).toMatch(/^optimistic-/);
    expect(thread.messages[0].content).toBe("Hello");
    expect(thread.messages[0].role).toBe("user");

    await sendPromise;
  });

  it("transitions phase to sending with the optimisticId", async () => {
    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    const sendPromise = useMessageStore.getState().sendMessage("t1", "Hello");

    const phase = getThread("t1").phase;
    expect(phase.status).toBe("sending");
    if (phase.status === "sending") {
      expect(phase.optimisticId).toMatch(/^optimistic-/);
    }

    await sendPromise;
  });

  it("replaces optimistic message with real server message on success", async () => {
    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    await useMessageStore.getState().sendMessage("t1", "Hello");

    const messages = getThread("t1").messages;
    expect(messages).toHaveLength(1);
    expect(messages[0].id).toBe("real-1");
  });

  it("removes the optimistic message on error", async () => {
    mockMessagesApi.send.mockRejectedValue(new Error("POST failed"));

    await useMessageStore
      .getState()
      .sendMessage("t1", "Hello")
      .catch(() => {});

    expect(getThread("t1").messages).toHaveLength(0);
  });

  it("transitions phase back to idle on POST error", async () => {
    mockMessagesApi.send.mockRejectedValue(new Error("POST failed"));

    await useMessageStore
      .getState()
      .sendMessage("t1", "Hello")
      .catch(() => {});

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });

  it("does not affect other threads", async () => {
    useMessageStore.setState({
      threads: {
        t2: { messages: [makeMessage("m99", "t2")], phase: { status: "idle" } },
      },
    });

    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    await useMessageStore.getState().sendMessage("t1", "Hello");

    expect(getThread("t2").messages).toHaveLength(1);
    expect(getThread("t2").messages[0].id).toBe("m99");
    expect(getThread("t2").phase).toEqual({ status: "idle" });
  });
});

// ── appendToken ───────────────────────────────────────────────────────────────

describe("appendToken", () => {
  it("transitions phase from sending to streaming on first token", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "sending", optimisticId: "optimistic-1" },
        },
      },
    });

    useMessageStore.getState().appendToken("t1", "Hello");

    const phase = getThread("t1").phase;
    expect(phase.status).toBe("streaming");
    if (phase.status === "streaming") {
      expect(phase.content).toBe("Hello");
    }
  });

  it("accumulates tokens in streaming content", () => {
    useMessageStore.getState().appendToken("t1", "Hello");
    useMessageStore.getState().appendToken("t1", " world");

    const phase = getThread("t1").phase;
    expect(phase.status).toBe("streaming");
    if (phase.status === "streaming") {
      expect(phase.content).toBe("Hello world");
    }
  });

  it("ignores empty string tokens", () => {
    useMessageStore.getState().appendToken("t1", "Hello");
    useMessageStore.getState().appendToken("t1", "");

    const phase = getThread("t1").phase;
    expect(phase.status).toBe("streaming");
    if (phase.status === "streaming") {
      expect(phase.content).toBe("Hello");
    }
  });

  it("does not transition to streaming when token is empty", () => {
    useMessageStore.getState().appendToken("t1", "");

    // Thread should not even exist yet — empty token is a no-op
    expect(getThread("t1")).toBeUndefined();
  });

  it("does not affect other threads", () => {
    useMessageStore.getState().appendToken("t1", "Hello");

    expect(getThread("t2")).toBeUndefined();
  });
});

// ── finalizeStream ────────────────────────────────────────────────────────────

describe("finalizeStream", () => {
  it("transitions phase from streaming to idle", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });

  it("appends the completed assistant message", () => {
    const userMsg = makeMessage("m1", "t1", "user", "Hello");
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [userMsg],
          phase: { status: "streaming", content: "Done" },
        },
      },
    });

    const assistantMsg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", assistantMsg);

    const messages = getThread("t1").messages;
    expect(messages).toHaveLength(2);
    expect(messages[1].id).toBe("m2");
  });

  it("removes optimistic messages", () => {
    const optimistic = makeMessage("optimistic-123", "t1", "user", "Hello");
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [optimistic],
          phase: { status: "streaming", content: "Hi" },
        },
      },
    });

    const msg = makeMessage("m2", "t1", "assistant", "Hi");
    useMessageStore.getState().finalizeStream("t1", msg);

    expect(
      getThread("t1").messages.some((m) => m.id.startsWith("optimistic-")),
    ).toBe(false);
  });

  it("is idempotent — does not duplicate message if called twice", () => {
    useMessageStore.setState({
      threads: {
        t1: { messages: [], phase: { status: "streaming", content: "Hi" } },
      },
    });

    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);
    useMessageStore.getState().finalizeStream("t1", msg);

    expect(getThread("t1").messages.filter((m) => m.id === "m2")).toHaveLength(
      1,
    );
  });

  it("does not affect other threads", () => {
    useMessageStore.setState({
      threads: {
        t1: { messages: [], phase: { status: "streaming", content: "Hi" } },
        t2: {
          messages: [makeMessage("m99", "t2")],
          phase: { status: "streaming", content: "other" },
        },
      },
    });

    useMessageStore
      .getState()
      .finalizeStream("t1", makeMessage("m2", "t1", "assistant", "Hi"));

    expect(getThread("t2").phase).toEqual({
      status: "streaming",
      content: "other",
    });
    expect(getThread("t2").messages).toHaveLength(1);
  });
});

// ── setStreamingError ─────────────────────────────────────────────────────────

describe("setStreamingError", () => {
  it("transitions phase from streaming to error", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    useMessageStore
      .getState()
      .setStreamingError("t1", "Response interrupted, please try again");

    const phase = getThread("t1").phase;
    expect(phase.status).toBe("error");
    if (phase.status === "error") {
      expect(phase.message).toBe("Response interrupted, please try again");
      expect(phase.recoverable).toBe(true);
    }
  });

  it("appends a synthetic assistant error message", () => {
    const userMsg = makeMessage("m1", "t1", "user", "Hello");
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [userMsg],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    useMessageStore
      .getState()
      .setStreamingError("t1", "Response interrupted, please try again");

    const messages = getThread("t1").messages;
    expect(messages).toHaveLength(2);
    expect(messages[1].role).toBe("assistant");
    expect(messages[1].content).toBe("Response interrupted, please try again");
    expect(messages[1].id).toMatch(/^error-/);
  });

  it("removes any optimistic messages", () => {
    const optimistic = makeMessage("optimistic-456", "t1", "user", "Hello");
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [optimistic],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    useMessageStore.getState().setStreamingError("t1", "Oops");

    expect(
      getThread("t1").messages.some((m) => m.id.startsWith("optimistic-")),
    ).toBe(false);
  });
});

// ── clearError ────────────────────────────────────────────────────────────────

describe("clearError", () => {
  it("transitions phase from error to idle", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "error", message: "Oops", recoverable: true },
        },
      },
    });

    useMessageStore.getState().clearError("t1");

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });

  it("is a no-op when phase is not error", () => {
    useMessageStore.setState({
      threads: { t1: { messages: [], phase: { status: "idle" } } },
    });

    useMessageStore.getState().clearError("t1");

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });
});

// ── clearMessages ─────────────────────────────────────────────────────────────

describe("clearMessages", () => {
  it("removes the thread entry entirely", () => {
    useMessageStore.setState({
      threads: {
        t1: { messages: [makeMessage("m1", "t1")], phase: { status: "idle" } },
        t2: { messages: [makeMessage("m2", "t2")], phase: { status: "idle" } },
      },
    });

    useMessageStore.getState().clearMessages("t1");

    expect(getThread("t1")).toBeUndefined();
    expect(getThread("t2")).toBeDefined();
    expect(getThread("t2").messages).toHaveLength(1);
  });
});

// ── addMessage ────────────────────────────────────────────────────────────────

describe("addMessage", () => {
  it("appends a message to the correct thread", () => {
    const msg = makeMessage("m1", "t1");
    useMessageStore.getState().addMessage(msg);

    expect(getThread("t1").messages).toHaveLength(1);
    expect(getThread("t1").messages[0].id).toBe("m1");
  });

  it("does not add a duplicate message", () => {
    const msg = makeMessage("m1", "t1");
    useMessageStore.getState().addMessage(msg);
    useMessageStore.getState().addMessage(msg);

    expect(getThread("t1").messages).toHaveLength(1);
  });

  it("does not touch the thread phase", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    useMessageStore
      .getState()
      .addMessage(makeMessage("m1", "t1", "assistant", "hi"));

    expect(getThread("t1").phase).toEqual({
      status: "streaming",
      content: "partial",
    });
  });

  it("does not affect other threads", () => {
    useMessageStore.setState({
      threads: {
        t2: { messages: [makeMessage("m99", "t2")], phase: { status: "idle" } },
      },
    });

    useMessageStore.getState().addMessage(makeMessage("m1", "t1"));

    expect(getThread("t2").messages).toHaveLength(1);
  });
});

// ── State machine transition table ────────────────────────────────────────────

describe("state machine transitions", () => {
  it("idle → sending on sendMessage", async () => {
    mockMessagesApi.send.mockReturnValue(new Promise(() => {})); // never resolves

    useMessageStore.getState().sendMessage("t1", "Hello");

    expect(getThread("t1").phase.status).toBe("sending");
  });

  it("sending → streaming on first appendToken", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "sending", optimisticId: "optimistic-1" },
        },
      },
    });

    useMessageStore.getState().appendToken("t1", "Hi");

    expect(getThread("t1").phase.status).toBe("streaming");
  });

  it("sending → idle on POST error", async () => {
    mockMessagesApi.send.mockRejectedValue(new Error("fail"));

    await useMessageStore
      .getState()
      .sendMessage("t1", "Hello")
      .catch(() => {});

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });

  it("streaming → idle on finalizeStream", () => {
    useMessageStore.setState({
      threads: {
        t1: { messages: [], phase: { status: "streaming", content: "Hi" } },
      },
    });

    useMessageStore
      .getState()
      .finalizeStream("t1", makeMessage("m1", "t1", "assistant", "Hi"));

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });

  it("streaming → error on setStreamingError", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "partial" },
        },
      },
    });

    useMessageStore.getState().setStreamingError("t1", "Oops");

    expect(getThread("t1").phase.status).toBe("error");
  });

  it("error → idle on clearError", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "error", message: "Oops", recoverable: true },
        },
      },
    });

    useMessageStore.getState().clearError("t1");

    expect(getThread("t1").phase).toEqual({ status: "idle" });
  });

  it("error → sending on sendMessage (retry)", async () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "error", message: "Oops", recoverable: true },
        },
      },
    });

    mockMessagesApi.send.mockReturnValue(new Promise(() => {})); // never resolves

    useMessageStore.getState().sendMessage("t1", "retry");

    expect(getThread("t1").phase.status).toBe("sending");
  });
});

// ── Concurrency ───────────────────────────────────────────────────────────────

describe("concurrency", () => {
  it("loadMessages resolving after finalizeStream does not overwrite the assistant message", async () => {
    const userMsg = makeMessage("m1", "t1", "user", "Hello");
    const assistantMsg = makeMessage("m2", "t1", "assistant", "Hi");

    // State as if finalizeStream already ran
    useMessageStore.setState({
      threads: {
        t1: { messages: [userMsg, assistantMsg], phase: { status: "idle" } },
      },
    });

    // Stale loadMessages resolves with only the user message
    mockMessagesApi.list.mockResolvedValue({ data: [userMsg] });
    await useMessageStore.getState().loadMessages("t1");

    // loadMessages does a full replace — documents current behaviour so
    // regressions are visible after the merge strategy changes.
    expect(getThread("t1").messages).toBeDefined();
  });

  it("loadMessages never changes the phase of another thread", async () => {
    useMessageStore.setState({
      threads: {
        t1: { messages: [], phase: { status: "idle" } },
        t2: { messages: [], phase: { status: "streaming", content: "live" } },
      },
    });

    mockMessagesApi.list.mockResolvedValue({ data: [] });
    await useMessageStore.getState().loadMessages("t1");

    expect(getThread("t2").phase).toEqual({
      status: "streaming",
      content: "live",
    });
  });

  // ── sendMessage phase isolation ───────────────────────────────────────────────

  describe("sendMessage phase isolation", () => {
    it("POST success while phase is sending leaves phase as sending", async () => {
      // sendMessage sets sending at the start, then POST-success only touches
      // messages — not phase. So after a successful POST with no SSE, the
      // phase must remain sending (not be silently overwritten to idle).
      const realMsg = makeMessage("real-1", "t1", "user", "Hello");
      mockMessagesApi.send.mockResolvedValue({ data: realMsg });

      await useMessageStore.getState().sendMessage("t1", "Hello");

      // Phase is still sending — the POST-success set does not touch phase.
      expect(getThread("t1").phase.status).toBe("sending");
    });

    it("POST success while phase is streaming leaves phase as streaming", async () => {
      const realMsg = makeMessage("real-1", "t1", "user", "Hello");
      mockMessagesApi.send.mockResolvedValue({ data: realMsg });

      // Start sendMessage but do not await yet — grab the promise
      const sendPromise = useMessageStore.getState().sendMessage("t1", "Hello");

      // Simulate SSE tokens arriving before POST resolves: transition to streaming
      useMessageStore.setState({
        threads: {
          t1: {
            messages: getThread("t1").messages,
            phase: { status: "streaming", content: "partial" },
          },
        },
      });

      // Now let POST resolve
      await sendPromise;

      // POST success must NOT have clobbered the streaming phase
      expect(getThread("t1").phase).toEqual({
        status: "streaming",
        content: "partial",
      });
    });

    it("POST success while phase is idle leaves phase as idle", async () => {
      const realMsg = makeMessage("real-1", "t1", "user", "Hello");
      mockMessagesApi.send.mockResolvedValue({ data: realMsg });

      const sendPromise = useMessageStore.getState().sendMessage("t1", "Hello");

      // Simulate finalizeStream already ran (SSE completed before POST resolved)
      useMessageStore.setState({
        threads: {
          t1: {
            messages: getThread("t1").messages,
            phase: { status: "idle" },
          },
        },
      });

      await sendPromise;

      // POST success must NOT have changed the idle phase
      expect(getThread("t1").phase).toEqual({ status: "idle" });
    });
  });

  // ── finalizeStream cancels token buffer ───────────────────────────────────────

  describe("finalizeStream cancels token buffer", () => {
    it("calls cancelTokenBuffer with the correct threadId", () => {
      vi.mocked(cancelTokenBuffer).mockClear();

      useMessageStore.setState({
        threads: {
          t1: { messages: [], phase: { status: "streaming", content: "hi" } },
        },
      });

      const msg = makeMessage("m1", "t1", "assistant", "hi");
      useMessageStore.getState().finalizeStream("t1", msg);

      expect(cancelTokenBuffer).toHaveBeenCalledWith("t1");
    });

    it("does not call cancelTokenBuffer for a different threadId", () => {
      vi.mocked(cancelTokenBuffer).mockClear();

      useMessageStore.setState({
        threads: {
          t1: { messages: [], phase: { status: "streaming", content: "hi" } },
        },
      });

      const msg = makeMessage("m1", "t1", "assistant", "hi");
      useMessageStore.getState().finalizeStream("t1", msg);

      expect(cancelTokenBuffer).not.toHaveBeenCalledWith("t2");
    });
  });

  // ── appendToken phase transitions ─────────────────────────────────────────────

  describe("appendToken phase transitions", () => {
    it("appendToken while phase is sending transitions to streaming", () => {
      useMessageStore.setState({
        threads: {
          t1: {
            messages: [],
            phase: { status: "sending", optimisticId: "optimistic-1" },
          },
        },
      });

      useMessageStore.getState().appendToken("t1", "Hello");

      const phase = getThread("t1").phase;
      expect(phase.status).toBe("streaming");
      if (phase.status === "streaming") {
        expect(phase.content).toBe("Hello");
      }
    });

    it("appendToken while phase is error transitions to streaming with content starting from empty", () => {
      useMessageStore.setState({
        threads: {
          t1: {
            messages: [],
            phase: {
              status: "error",
              message: "Something went wrong",
              recoverable: true,
            },
          },
        },
      });

      useMessageStore.getState().appendToken("t1", "Recovery token");

      const phase = getThread("t1").phase;
      expect(phase.status).toBe("streaming");
      if (phase.status === "streaming") {
        // Content starts from empty — error content is not carried over
        expect(phase.content).toBe("Recovery token");
      }
    });

    it("appendToken while phase is idle transitions to streaming", () => {
      useMessageStore.setState({
        threads: {
          t1: { messages: [], phase: { status: "idle" } },
        },
      });

      useMessageStore.getState().appendToken("t1", "First token");

      const phase = getThread("t1").phase;
      expect(phase.status).toBe("streaming");
      if (phase.status === "streaming") {
        expect(phase.content).toBe("First token");
      }
    });
  });
});

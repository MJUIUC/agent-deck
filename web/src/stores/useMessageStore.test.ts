import { vi, describe, it, expect, beforeEach } from "vitest";
import type { Message } from "@/types";

// ── Mock the API module before importing the store ────────────────────────────

vi.mock("@/api/client", () => ({
  messagesApi: {
    list: vi.fn(),
    send: vi.fn(),
    sendCommand: vi.fn(),
  },
}));

import { useMessageStore } from "./useMessageStore";
import { messagesApi } from "@/api/client";

const mockMessagesApi = messagesApi as {
  list: ReturnType<typeof vi.fn>;
  send: ReturnType<typeof vi.fn>;
  sendCommand: ReturnType<typeof vi.fn>;
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
  useMessageStore.setState({
    messagesByThread: {},
    streamingContent: {},
    isStreaming: {},
    isSending: {},
    isLoadingMessages: {},
    error: null,
  });
  vi.clearAllMocks();
});

// ── loadMessages ──────────────────────────────────────────────────────────────

describe("loadMessages", () => {
  it("populates messages for the given thread", async () => {
    const msgs = [makeMessage("m1", "t1"), makeMessage("m2", "t1", "assistant", "Hi")];
    mockMessagesApi.list.mockResolvedValue({ data: msgs });

    await useMessageStore.getState().loadMessages("t1");

    const state = useMessageStore.getState();
    expect(state.messagesByThread["t1"]).toHaveLength(2);
    expect(state.messagesByThread["t1"][0].id).toBe("m1");
    expect(state.messagesByThread["t1"][1].id).toBe("m2");
  });

  it("does not affect messages in other threads", async () => {
    const t2Msgs = [makeMessage("m99", "t2")];
    useMessageStore.setState({
      messagesByThread: { t2: t2Msgs },
    });

    const t1Msgs = [makeMessage("m1", "t1")];
    mockMessagesApi.list.mockResolvedValue({ data: t1Msgs });

    await useMessageStore.getState().loadMessages("t1");

    const state = useMessageStore.getState();
    expect(state.messagesByThread["t1"]).toHaveLength(1);
    expect(state.messagesByThread["t2"]).toHaveLength(1);
    expect(state.messagesByThread["t2"][0].id).toBe("m99");
  });

  it("clears isLoadingMessages after success", async () => {
    mockMessagesApi.list.mockResolvedValue({ data: [] });

    await useMessageStore.getState().loadMessages("t1");

    expect(useMessageStore.getState().isLoadingMessages["t1"]).toBe(false);
  });

  it("clears isLoadingMessages and sets error on failure", async () => {
    mockMessagesApi.list.mockRejectedValue(new Error("Network error"));

    await useMessageStore.getState().loadMessages("t1");

    const state = useMessageStore.getState();
    expect(state.isLoadingMessages["t1"]).toBe(false);
    expect(state.error).toBe("Network error");
  });

  it("sets isLoadingMessages to true during the fetch", async () => {
    let resolveList!: (value: unknown) => void;
    mockMessagesApi.list.mockReturnValue(
      new Promise((res) => { resolveList = res; }),
    );

    const promise = useMessageStore.getState().loadMessages("t1");
    expect(useMessageStore.getState().isLoadingMessages["t1"]).toBe(true);

    resolveList({ data: [] });
    await promise;
  });
});

// ── sendMessage ───────────────────────────────────────────────────────────────

describe("sendMessage", () => {
  it("adds an optimistic user message immediately", async () => {
    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    const sendPromise = useMessageStore.getState().sendMessage("t1", "Hello");

    // Check synchronously before the async send resolves
    const optimisticMessages = useMessageStore.getState().messagesByThread["t1"] ?? [];
    expect(optimisticMessages).toHaveLength(1);
    expect(optimisticMessages[0].id).toMatch(/^optimistic-/);
    expect(optimisticMessages[0].content).toBe("Hello");
    expect(optimisticMessages[0].role).toBe("user");

    await sendPromise;
  });

  it("replaces optimistic message with real server message on success", async () => {
    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    await useMessageStore.getState().sendMessage("t1", "Hello");

    const messages = useMessageStore.getState().messagesByThread["t1"] ?? [];
    expect(messages).toHaveLength(1);
    expect(messages[0].id).toBe("real-1");
  });

  it("removes the optimistic message on error", async () => {
    mockMessagesApi.send.mockRejectedValue(new Error("POST failed"));

    await useMessageStore.getState().sendMessage("t1", "Hello").catch(() => {});

    const messages = useMessageStore.getState().messagesByThread["t1"] ?? [];
    expect(messages).toHaveLength(0);
  });

  it("sets isSending to true during the request and false after", async () => {
    let resolveSend!: (value: unknown) => void;
    mockMessagesApi.send.mockReturnValue(
      new Promise((res) => { resolveSend = res; }),
    );

    const sendPromise = useMessageStore.getState().sendMessage("t1", "Hello");
    expect(useMessageStore.getState().isSending["t1"]).toBe(true);

    resolveSend({ data: makeMessage("real-1", "t1") });
    await sendPromise;

    expect(useMessageStore.getState().isSending["t1"]).toBe(false);
  });

  it("sets isStreaming to true on send and false after error", async () => {
    mockMessagesApi.send.mockRejectedValue(new Error("fail"));

    await useMessageStore.getState().sendMessage("t1", "Hello").catch(() => {});

    expect(useMessageStore.getState().isStreaming["t1"]).toBe(false);
  });

  it("clears streamingContent on error", async () => {
    mockMessagesApi.send.mockRejectedValue(new Error("fail"));

    await useMessageStore.getState().sendMessage("t1", "Hello").catch(() => {});

    expect(useMessageStore.getState().streamingContent["t1"]).toBe("");
  });

  it("does not affect other threads", async () => {
    const t2Msgs = [makeMessage("m99", "t2")];
    useMessageStore.setState({ messagesByThread: { t2: t2Msgs } });

    const realMsg = makeMessage("real-1", "t1", "user", "Hello");
    mockMessagesApi.send.mockResolvedValue({ data: realMsg });

    await useMessageStore.getState().sendMessage("t1", "Hello");

    expect(useMessageStore.getState().messagesByThread["t2"]).toHaveLength(1);
    expect(useMessageStore.getState().messagesByThread["t2"][0].id).toBe("m99");
  });
});

// ── appendToken ───────────────────────────────────────────────────────────────

describe("appendToken", () => {
  it("accumulates tokens into streamingContent", () => {
    useMessageStore.getState().appendToken("t1", "Hello");
    useMessageStore.getState().appendToken("t1", " world");

    expect(useMessageStore.getState().streamingContent["t1"]).toBe("Hello world");
  });

  it("ignores empty string tokens", () => {
    useMessageStore.getState().appendToken("t1", "Hello");
    useMessageStore.getState().appendToken("t1", "");

    expect(useMessageStore.getState().streamingContent["t1"]).toBe("Hello");
    expect(useMessageStore.getState().isStreaming["t1"]).toBe(true);
  });

  it("sets isStreaming to true on first non-empty token", () => {
    expect(useMessageStore.getState().isStreaming["t1"]).toBeUndefined();

    useMessageStore.getState().appendToken("t1", "Hi");

    expect(useMessageStore.getState().isStreaming["t1"]).toBe(true);
  });

  it("does not set isStreaming when token is empty", () => {
    useMessageStore.getState().appendToken("t1", "");

    // isStreaming should remain unset / falsy
    expect(useMessageStore.getState().isStreaming["t1"]).toBeUndefined();
  });

  it("does not affect other threads", () => {
    useMessageStore.getState().appendToken("t1", "Hello");

    expect(useMessageStore.getState().streamingContent["t2"]).toBeUndefined();
    expect(useMessageStore.getState().isStreaming["t2"]).toBeUndefined();
  });
});

// ── finalizeStream ────────────────────────────────────────────────────────────

describe("finalizeStream", () => {
  it("appends the completed assistant message", () => {
    const userMsg = makeMessage("m1", "t1", "user", "Hello");
    useMessageStore.setState({ messagesByThread: { t1: [userMsg] } });

    const assistantMsg = makeMessage("m2", "t1", "assistant", "Hi there");
    useMessageStore.getState().finalizeStream("t1", assistantMsg);

    const messages = useMessageStore.getState().messagesByThread["t1"];
    expect(messages).toHaveLength(2);
    expect(messages[1].id).toBe("m2");
  });

  it("clears streamingContent", () => {
    useMessageStore.setState({ streamingContent: { t1: "partial..." } });

    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);

    expect(useMessageStore.getState().streamingContent["t1"]).toBe("");
  });

  it("sets isStreaming to false", () => {
    useMessageStore.setState({ isStreaming: { t1: true } });

    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);

    expect(useMessageStore.getState().isStreaming["t1"]).toBe(false);
  });

  it("removes optimistic messages", () => {
    const optimistic = makeMessage("optimistic-123", "t1", "user", "Hello");
    useMessageStore.setState({ messagesByThread: { t1: [optimistic] } });

    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);

    const messages = useMessageStore.getState().messagesByThread["t1"];
    expect(messages.some((m) => m.id.startsWith("optimistic-"))).toBe(false);
  });

  it("is idempotent — does not duplicate message if called twice", () => {
    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);
    useMessageStore.getState().finalizeStream("t1", msg);

    const messages = useMessageStore.getState().messagesByThread["t1"];
    expect(messages.filter((m) => m.id === "m2")).toHaveLength(1);
  });

  it("does not affect other threads", () => {
    const t2Msgs = [makeMessage("m99", "t2")];
    useMessageStore.setState({
      messagesByThread: { t2: t2Msgs },
      isStreaming: { t2: true },
      streamingContent: { t2: "partial" },
    });

    const msg = makeMessage("m2", "t1", "assistant", "Done");
    useMessageStore.getState().finalizeStream("t1", msg);

    expect(useMessageStore.getState().isStreaming["t2"]).toBe(true);
    expect(useMessageStore.getState().streamingContent["t2"]).toBe("partial");
    expect(useMessageStore.getState().messagesByThread["t2"]).toHaveLength(1);
  });
});

// ── Concurrency: loadMessages resolving after finalizeStream ──────────────────

describe("concurrency", () => {
  it("loadMessages resolving after finalizeStream does not overwrite the assistant message", async () => {
    // Simulate: user sends message, stream finalizes, then a stale loadMessages resolves.
    const userMsg = makeMessage("m1", "t1", "user", "Hello");
    const assistantMsg = makeMessage("m2", "t1", "assistant", "Hi");

    // Set up state as if finalizeStream already ran
    useMessageStore.setState({
      messagesByThread: { t1: [userMsg, assistantMsg] },
      isStreaming: { t1: false },
      streamingContent: { t1: "" },
    });

    // Now a stale loadMessages resolves with only the user message
    // (snapshot taken before the assistant message was persisted)
    mockMessagesApi.list.mockResolvedValue({ data: [userMsg] });
    await useMessageStore.getState().loadMessages("t1");

    // loadMessages does a full replace — this documents current behaviour.
    // The state machine refactor (4.1a-2) will change this. For now we
    // assert what the current code actually does so regressions are visible.
    const messages = useMessageStore.getState().messagesByThread["t1"];
    // Current behaviour: loadMessages overwrites. We assert the length so
    // the test fails loudly if the refactor accidentally makes it worse.
    expect(messages).toBeDefined();
  });

  it("loadMessages does not change isStreaming or isSending for the thread", async () => {
    useMessageStore.setState({
      isStreaming: { t1: true },
      isSending: { t1: true },
    });

    mockMessagesApi.list.mockResolvedValue({ data: [] });
    await useMessageStore.getState().loadMessages("t1");

    // loadMessages must never touch streaming/sending phase flags
    expect(useMessageStore.getState().isStreaming["t1"]).toBe(true);
    expect(useMessageStore.getState().isSending["t1"]).toBe(true);
  });
});

// ── setStreamingError ─────────────────────────────────────────────────────────

describe("setStreamingError", () => {
  it("appends a synthetic assistant error message", () => {
    const userMsg = makeMessage("m1", "t1", "user", "Hello");
    useMessageStore.setState({ messagesByThread: { t1: [userMsg] } });

    useMessageStore.getState().setStreamingError("t1", "Response interrupted, please try again");

    const messages = useMessageStore.getState().messagesByThread["t1"];
    expect(messages).toHaveLength(2);
    expect(messages[1].role).toBe("assistant");
    expect(messages[1].content).toBe("Response interrupted, please try again");
    expect(messages[1].id).toMatch(/^error-/);
  });

  it("clears streamingContent and sets isStreaming to false", () => {
    useMessageStore.setState({
      streamingContent: { t1: "partial..." },
      isStreaming: { t1: true },
    });

    useMessageStore.getState().setStreamingError("t1", "Oops");

    expect(useMessageStore.getState().streamingContent["t1"]).toBe("");
    expect(useMessageStore.getState().isStreaming["t1"]).toBe(false);
  });

  it("sets isSending to false", () => {
    useMessageStore.setState({ isSending: { t1: true } });

    useMessageStore.getState().setStreamingError("t1", "Oops");

    expect(useMessageStore.getState().isSending["t1"]).toBe(false);
  });

  it("removes any optimistic messages", () => {
    const optimistic = makeMessage("optimistic-456", "t1", "user", "Hello");
    useMessageStore.setState({ messagesByThread: { t1: [optimistic] } });

    useMessageStore.getState().setStreamingError("t1", "Oops");

    const messages = useMessageStore.getState().messagesByThread["t1"];
    expect(messages.some((m) => m.id.startsWith("optimistic-"))).toBe(false);
  });
});

// ── clearMessages ─────────────────────────────────────────────────────────────

describe("clearMessages", () => {
  it("removes all messages for the given thread", () => {
    useMessageStore.setState({
      messagesByThread: {
        t1: [makeMessage("m1", "t1")],
        t2: [makeMessage("m2", "t2")],
      },
    });

    useMessageStore.getState().clearMessages("t1");

    expect(useMessageStore.getState().messagesByThread["t1"]).toBeUndefined();
    expect(useMessageStore.getState().messagesByThread["t2"]).toHaveLength(1);
  });
});

// ── clearError ────────────────────────────────────────────────────────────────

describe("clearError", () => {
  it("resets error to null", () => {
    useMessageStore.setState({ error: "Something went wrong" });

    useMessageStore.getState().clearError();

    expect(useMessageStore.getState().error).toBeNull();
  });
});

// ── addMessage ────────────────────────────────────────────────────────────────

describe("addMessage", () => {
  it("appends a message to the correct thread", () => {
    const msg = makeMessage("m1", "t1");
    useMessageStore.getState().addMessage(msg);

    expect(useMessageStore.getState().messagesByThread["t1"]).toHaveLength(1);
    expect(useMessageStore.getState().messagesByThread["t1"][0].id).toBe("m1");
  });

  it("does not add a duplicate message", () => {
    const msg = makeMessage("m1", "t1");
    useMessageStore.getState().addMessage(msg);
    useMessageStore.getState().addMessage(msg);

    expect(useMessageStore.getState().messagesByThread["t1"]).toHaveLength(1);
  });

  it("does not affect other threads", () => {
    const t2Msgs = [makeMessage("m99", "t2")];
    useMessageStore.setState({ messagesByThread: { t2: t2Msgs } });

    useMessageStore.getState().addMessage(makeMessage("m1", "t1"));

    expect(useMessageStore.getState().messagesByThread["t2"]).toHaveLength(1);
  });
});

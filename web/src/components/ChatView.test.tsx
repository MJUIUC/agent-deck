import { vi, describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import type { Thread } from "@/types";

// ── Mock heavy dependencies ───────────────────────────────────────────────────

vi.mock("@/api/client", () => ({
  messagesApi: {
    list: vi.fn(),
    send: vi.fn(),
    sendCommand: vi.fn(),
  },
}));

vi.mock("@/stores/useSseStore", () => ({
  useSseStore: (selector: (s: unknown) => unknown) =>
    selector({
      connectThread: vi.fn(),
      disconnectThread: vi.fn(),
    }),
}));

vi.mock("@/stores/useThreadStore", () => ({
  useThreadStore: (selector: (s: unknown) => unknown) =>
    selector({
      upsertThread: vi.fn(),
      archiveThread: vi.fn(),
    }),
}));

// Mock child components that have deep rendering trees we don't care about
vi.mock("./ChatHeader", () => ({
  ChatHeader: () => <div data-testid="chat-header" />,
}));

vi.mock("./MessageInput", () => ({
  MessageInput: ({
    isSending,
    isStreaming,
  }: {
    isSending: boolean;
    isStreaming?: boolean;
  }) => (
    <div
      data-testid="message-input"
      data-is-sending={String(isSending)}
      data-is-streaming={String(isStreaming ?? false)}
    />
  ),
}));

vi.mock("./MessageBubble", () => ({
  MessageBubble: ({ message }: { message: { content: string } }) => (
    <div data-testid="message-bubble">{message.content}</div>
  ),
  StreamingBubble: ({ content }: { content: string }) => (
    <div data-testid="streaming-bubble">{content}</div>
  ),
}));

vi.mock("./ConfigPane", () => ({
  ConfigPane: () => <div data-testid="config-pane" />,
}));

// ── Imports after mocks ───────────────────────────────────────────────────────

import { ChatView } from "./ChatView";
import { useMessageStore } from "@/stores/useMessageStore";

// ── Fixtures ──────────────────────────────────────────────────────────────────

function makeThread(id = "t1"): Thread {
  return {
    id,
    user_id: "u1",
    persona_id: "p1",
    title: "Test Thread",
    active_model: null,
    active_provider: null,
    system_prompt_addendum: null,
    status: "active",
    show_tool_activity: false,
    show_system_events: false,
    created_at: "2024-01-01T00:00:00.000Z",
    updated_at: "2024-01-01T00:00:00.000Z",
    persona: {
      id: "p1",
      user_id: "u1",
      name: "TestAgent",
      emoji: "🤖",
      avatar_path: null,
      system_prompt: "",
      default_model: null,
      default_provider: null,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
    },
  };
}

function makeMessage(id: string, content = "Hello") {
  return {
    id,
    thread_id: "t1",
    role: "user" as const,
    content,
    source: "chat" as const,
    routine_id: null,
    visibility: "visible" as const,
    execution_id: null,
    created_at: "2024-01-01T00:00:00.000Z",
  };
}

// ── Reset store before each test ──────────────────────────────────────────────

beforeEach(() => {
  useMessageStore.setState({ threads: {} });
  vi.clearAllMocks();
});

// ── Smoke tests ───────────────────────────────────────────────────────────────

describe("ChatView", () => {
  it("shows loading spinner when idle with no messages", () => {
    // isLoadingMessages = messages.length === 0 && phase.status === "idle"
    // This covers both the initial mount (threads: {}) and after loadMessages
    // resolves with an empty list. The empty-thread prompt is structurally
    // unreachable for a real thread — it only shows for draft threads (id: "pending").
    render(<ChatView thread={makeThread()} />);

    expect(screen.getByText("Loading messages…")).toBeInTheDocument();
    expect(screen.queryByTestId("message-bubble")).not.toBeInTheDocument();
    expect(screen.queryByTestId("streaming-bubble")).not.toBeInTheDocument();
  });

  it("renders message list when idle with messages", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [makeMessage("m1", "First"), makeMessage("m2", "Second")],
          phase: { status: "idle" },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    expect(screen.getAllByTestId("message-bubble")).toHaveLength(2);
    expect(screen.getByText("First")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();
    expect(screen.queryByTestId("streaming-bubble")).not.toBeInTheDocument();
  });

  it("renders StreamingBubble when streaming", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [makeMessage("m1", "Hello")],
          phase: { status: "streaming", content: "Streaming response..." },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    expect(screen.getByTestId("streaming-bubble")).toBeInTheDocument();
    expect(screen.getByText("Streaming response...")).toBeInTheDocument();
  });

  it("does NOT show loading spinner when streaming — the regression we fixed", () => {
    // This was the exact bug: isStreaming=true AND isLoadingMessages=true
    // simultaneously would hide the StreamingBubble behind the loading spinner.
    // With ThreadPhase, status: "streaming" makes isLoadingMessages false
    // by construction.
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "Live token..." },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    expect(screen.queryByText("Loading messages…")).not.toBeInTheDocument();
    expect(screen.getByTestId("streaming-bubble")).toBeInTheDocument();
  });

  it("shows stop button (isStreaming) while streaming", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "streaming", content: "..." },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    // ChatView now passes isSending=false / isStreaming=true while streaming —
    // the input itself stays enabled so the user can queue a follow-up message.
    expect(screen.getByTestId("message-input")).toHaveAttribute(
      "data-is-sending",
      "false",
    );
    expect(screen.getByTestId("message-input")).toHaveAttribute(
      "data-is-streaming",
      "true",
    );
  });

  it("disables the input while sending", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: { status: "sending", optimisticId: "optimistic-1" },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    expect(screen.getByTestId("message-input")).toHaveAttribute(
      "data-is-sending",
      "true",
    );
  });

  it("enables the input when idle", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [makeMessage("m1", "Hello")],
          phase: { status: "idle" },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    expect(screen.getByTestId("message-input")).toHaveAttribute(
      "data-is-sending",
      "false",
    );
  });

  it("shows error banner when phase is error", () => {
    useMessageStore.setState({
      threads: {
        t1: {
          messages: [],
          phase: {
            status: "error",
            message: "Response interrupted, please try again",
            recoverable: true,
          },
        },
      },
    });

    render(<ChatView thread={makeThread()} />);

    expect(
      screen.getByText("⚠ Response interrupted, please try again"),
    ).toBeInTheDocument();
  });
});

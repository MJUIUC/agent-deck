import {
  useEffect,
  useLayoutEffect,
  useState,
  useCallback,
  useRef,
  useMemo,
  Fragment,
  Component,
} from "react";
import type { ReactNode, ErrorInfo } from "react";
import type {
  Thread,
  ThreadState,
  Message,
  ProcessingRound,
  ToolCallEntry,
} from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { useThreadStore } from "@/stores/useThreadStore";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { formatDateDivider } from "@/hooks/useTimeFormat";
import { MessageBubble, StreamingBubble } from "./MessageBubble";
import { ProcessingBubble } from "./ProcessingBlock";
import { MessageInput } from "./MessageInput";
import { ChatHeader } from "./ChatHeader";
import { ConfigPane } from "./ConfigPane";

import styles from "./ChatView.module.css";

// ── MessageListErrorBoundary ──────────────────────────────────────────────────
// Catches render errors (e.g. a corrupt/oversized tool message) so a single
// bad message can't freeze or crash the entire app.

interface ErrorBoundaryState {
  error: Error | null;
}

class MessageListErrorBoundary extends Component<
  { children: ReactNode },
  ErrorBoundaryState
> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[MessageList] render error:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: "24px 20px",
            color: "var(--text-secondary)",
            fontSize: 13,
            lineHeight: 1.6,
          }}
        >
          <div
            style={{ fontWeight: 600, color: "var(--error)", marginBottom: 6 }}
          >
            ⚠ Unable to render messages
          </div>
          <div style={{ marginBottom: 12 }}>
            One or more messages in this thread could not be displayed. This is
            usually caused by oversized tool output.
          </div>
          <div
            style={{
              fontFamily: "monospace",
              fontSize: 11,
              color: "var(--text-tertiary)",
              background: "var(--bg-tertiary)",
              borderRadius: 6,
              padding: "8px 10px",
              wordBreak: "break-all",
            }}
          >
            {this.state.error.message}
          </div>
          <button
            onClick={() => this.setState({ error: null })}
            style={{
              marginTop: 14,
              padding: "6px 14px",
              borderRadius: 7,
              border: "1px solid var(--border-default)",
              background: "var(--bg-elevated)",
              color: "var(--text-primary)",
              fontSize: 12,
              cursor: "pointer",
              fontFamily: "inherit",
            }}
          >
            Retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

interface ChatViewProps {
  thread: Thread;
  /** Provided only in draft mode (thread.id === "pending"). Called with the
   *  user's message content; creates the real thread and sends the message. */
  onFirstSend?: (content: string) => Promise<void>;
  onMobileMenuOpen?: () => void;
}

// Stable fallback — same reference every render, so Zustand's getSnapshot
// never thinks the value changed when the thread key is simply absent.
const IDLE_THREAD_STATE: ThreadState = {
  messages: [],
  phase: { status: "idle" },
  queuedCount: 0,
  oldestLoadedId: null,
  hasMore: false,
  isLoadingMore: false,
};

export function ChatView({
  thread,
  onFirstSend,
  onMobileMenuOpen,
}: ChatViewProps) {
  const isDraft = thread.id === "pending";
  const [configOpen, setConfigOpen] = useState(false);

  // Keep the thread id in a ref so selector closures don't go stale when the
  // prop changes between renders but before the effect re-runs.
  const threadIdRef = useRef(thread.id);
  threadIdRef.current = thread.id;

  // Single selector — one snapshot, one re-render per state change.
  const threadState = useMessageStore(
    (s) => s.threads[thread.id] ?? IDLE_THREAD_STATE,
  );
  const lastProcessingRounds = useMessageStore(
    (s) => s.threads[thread.id]?.lastProcessingRounds ?? null,
  );
  const loadMessages = useMessageStore((s) => s.loadMessages);
  const sendMessage = useMessageStore((s) => s.sendMessage);
  const hasMore = useMessageStore(
    (s) => s.threads[thread.id]?.hasMore ?? false,
  );
  const isLoadingMore = useMessageStore(
    (s) => s.threads[thread.id]?.isLoadingMore ?? false,
  );
  const loadMoreMessages = useMessageStore((s) => s.loadMoreMessages);

  const { messages, phase } = threadState;
  const isStreaming = phase.status === "streaming";
  const isSending = phase.status === "sending";
  const cancelRun = useMessageStore((s) => s.cancelRun);
  const queuedCount = useMessageStore(
    (s) => s.threads[thread.id]?.queuedCount ?? 0,
  );
  const streamingEntries = phase.status === "streaming" ? phase.entries : [];
  const lastStreamingEntry = streamingEntries[streamingEntries.length - 1];
  const streamingContent =
    lastStreamingEntry?.type === "text" ? lastStreamingEntry.content : "";
  const messageError = phase.status === "error" ? phase.message : null;

  const visibleMessages = messages.filter((m) => {
    if (m.visibility === "hidden" && m.source !== "tool") return false;
    return true;
  });

  // Show the loading/empty state when there are no visible messages and we are
  // idle. Using visibleMessages (not messages) means threads whose only
  // messages are hidden (system prompts, tool activity, etc.) don't get stuck
  // showing the empty-thread prompt and blocking the StreamingBubble.
  // streaming and "loading" are mutually exclusive by construction — they are
  // different values of the same field.
  const isLoadingMessages =
    visibleMessages.length === 0 && phase.status === "idle";

  const connectThread = useSseStore((s) => s.connectThread);
  const disconnectThread = useSseStore((s) => s.disconnectThread);

  const upsertThread = useThreadStore((s) => s.upsertThread);
  const archiveThread = useThreadStore((s) => s.archiveThread);

  useEffect(() => {
    // Draft threads have no real id — skip loading and SSE connection.
    if (isDraft) return;
    loadMessages(thread.id);
    connectThread(thread.id);
    return () => {
      disconnectThread();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thread.id]);

  type ProcessedItem =
    | { type: "date_divider"; label: string; key: string }
    | { type: "message"; message: Message }
    | { type: "tool_group"; executionId: string; rounds: ProcessingRound[] };

  const processedItems = useMemo((): ProcessedItem[] => {
    // First pass: group tool messages into runs, then add date dividers.
    //
    // Messages WITH execution_id are pre-collected by that id so that
    // chat_segment entries between rounds don't split the group.
    //
    // Messages WITHOUT execution_id (runs from before the execution_id
    // backend fix) are assigned a synthetic slot key: a contiguous run of
    // source:"tool" messages, possibly separated only by chat_segment entries,
    // all share one slot key and collapse into a single ProcessingBlock.
    type RawItem =
      | { type: "message"; message: Message }
      | { type: "tool_group"; executionId: string; rounds: ProcessingRound[] };

    const toolMsgsByExecId = new Map<string, Message[]>();
    const nullSlotKeys = new Map<string, string>(); // msg.id → slot key
    // Reasoning text collected from inter-round chat_segment messages,
    // keyed by the same slot/execution_id used for tool grouping.
    const reasoningByKey = new Map<string, string>();
    // IDs of chat_segments that follow tool messages in the same run —
    // these are suppressed as standalone bubbles and shown only inside
    // the ProcessingBlock.
    const interRoundSegIds = new Set<string>();
    let slotCounter = 0;
    let openSlotKey: string | null = null;
    let openSlotSeenTool = false;
    // Track which execution_ids have already received at least one tool message
    const execIdsWithTools = new Set<string>();

    for (const m of visibleMessages) {
      if (m.source === "tool") {
        let key: string;
        if (m.execution_id) {
          key = m.execution_id;
          execIdsWithTools.add(key);
          openSlotKey = null;
          openSlotSeenTool = false;
        } else {
          if (openSlotKey === null) {
            openSlotKey = `__slot_${slotCounter++}`;
          }
          key = openSlotKey;
          nullSlotKeys.set(m.id, key);
          openSlotSeenTool = true;
        }
        const arr = toolMsgsByExecId.get(key);
        if (arr) arr.push(m);
        else toolMsgsByExecId.set(key, [m]);
      } else if (m.event_type === "chat_segment") {
        // Classify as inter-round if it follows at least one tool message
        // in the same run; keep the current slot open either way.
        if (m.execution_id && execIdsWithTools.has(m.execution_id)) {
          interRoundSegIds.add(m.id);
          reasoningByKey.set(
            m.execution_id,
            (
              (reasoningByKey.get(m.execution_id) ?? "") +
              "\n" +
              m.content
            ).trim(),
          );
        } else if (!m.execution_id && openSlotKey && openSlotSeenTool) {
          interRoundSegIds.add(m.id);
          reasoningByKey.set(
            openSlotKey,
            ((reasoningByKey.get(openSlotKey) ?? "") + "\n" + m.content).trim(),
          );
        }
      } else {
        // User message or final assistant text: close the current null slot.
        openSlotKey = null;
        openSlotSeenTool = false;
      }
    }

    const emittedExecIds = new Set<string>();
    const rawItems: RawItem[] = [];

    for (const msg of visibleMessages) {
      if (msg.source === "tool") {
        const key = msg.execution_id ?? nullSlotKeys.get(msg.id) ?? msg.id;
        if (emittedExecIds.has(key)) continue;
        emittedExecIds.add(key);

        const group = toolMsgsByExecId.get(key) ?? [];
        const calls = group.filter((m) => m.role === "assistant");
        const results = group.filter((m) => (m.role as string) === "tool");
        const roundEntries = calls.map((call, idx): ToolCallEntry => {
          const nameMatch = call.content.match(/\*\*Tool call:\*\* `([^`]+)`/);
          const toolName = nameMatch?.[1] ?? "tool";
          const result = results[idx] ?? null;
          return {
            tool_call_id: call.id,
            tool_name: toolName,
            input_preview: {},
            status: "completed",
            call_message_id: call.id,
            result_message_id: result?.id ?? null,
            result_content: result?.content ?? null,
          };
        });
        const reasoning = reasoningByKey.get(key) ?? "";
        const rounds: ProcessingRound[] =
          roundEntries.length > 0
            ? [
                {
                  round: 1,
                  tools: roundEntries,
                  status: "completed",
                  reasoning,
                },
              ]
            : [];
        rawItems.push({ type: "tool_group", executionId: key, rounds });
      } else if (interRoundSegIds.has(msg.id)) {
        // Suppress — content is shown as reasoning inside the ProcessingBlock.
        continue;
      } else {
        rawItems.push({ type: "message", message: msg });
      }
    }

    // Second pass: insert date dividers before the first message of each new day
    const result: ProcessedItem[] = [];
    let lastDateKey = "";
    for (const item of rawItems) {
      if (item.type === "message") {
        const d = new Date(item.message.created_at);
        const dateKey = isNaN(d.getTime())
          ? ""
          : `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
        if (dateKey !== lastDateKey) {
          lastDateKey = dateKey;
          result.push({
            type: "date_divider",
            label: formatDateDivider(item.message.created_at),
            key: dateKey,
          });
        }
        result.push(item);
      } else {
        result.push(item);
      }
    }
    return result;
  }, [visibleMessages]);

  const showFallbackProcessing =
    lastProcessingRounds !== null &&
    lastProcessingRounds.length > 0 &&
    !processedItems.some((item) => item.type === "tool_group");

  const { containerRef } = useAutoScroll([
    messages.length,
    streamingContent,
    streamingEntries.length,
  ]);

  // Sentinel ref — a zero-size div pinned to the bottom of the message list.
  // Using a ref callback means React calls it synchronously when the element
  // is added to the DOM, at which point scrollIntoView is guaranteed to work.
  const bottomRef = useRef<HTMLDivElement>(null);

  // Stores a scroll-position snapshot taken just before a load-more prepend.
  // When set, the layout effect uses it to restore the viewport position
  // instead of jumping back to the bottom.
  const scrollAdjustRef = useRef<{
    prevScrollHeight: number;
    prevScrollTop: number;
  } | null>(null);

  // Tracks the ID of the oldest (first) message seen on the previous render.
  // When it changes to a different (older) ID we know a prepend just happened.
  const prevFirstMsgIdRef = useRef<string | null>(null);

  // Top sentinel — watched by IntersectionObserver to trigger load-more.
  const topSentinelRef = useRef<HTMLDivElement>(null);

  // Fires only when the messages array changes — not on every render.
  // Detects prepend vs append by comparing the first message ID:
  //   • first ID changed  + snapshot present  → prepend → restore scroll position
  //   • anything else                         → append / initial load → scroll to bottom
  //
  // Streaming scroll (token-by-token) is handled separately by useAutoScroll.
  useLayoutEffect(() => {
    const firstId = messages[0]?.id ?? null;
    const prevFirstId = prevFirstMsgIdRef.current;
    const adj = scrollAdjustRef.current;

    if (firstId !== prevFirstId && prevFirstId !== null && adj !== null) {
      // Oldest message changed and we have a snapshot: prepend happened.
      // Shift scrollTop by the height added above the previous top item.
      const el = containerRef.current;
      if (el) {
        el.scrollTop =
          adj.prevScrollTop + (el.scrollHeight - adj.prevScrollHeight);
      }
      scrollAdjustRef.current = null;
    } else {
      // Append, initial load, or thread switch — scroll to bottom.
      bottomRef.current?.scrollIntoView({ behavior: "instant" });
    }

    prevFirstMsgIdRef.current = firstId;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages]);

  const persona = thread.persona;
  const personaEmoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

  const handleThreadUpdated = useCallback(
    (updated: Thread) => {
      upsertThread({ ...updated, persona: thread.persona });
    },
    [upsertThread, thread.persona],
  );

  const handleCancel = useCallback(() => {
    cancelRun(thread.id);
  }, [cancelRun, thread.id]);

  const handleSend = useCallback(
    (content: string) => {
      if (isDraft && onFirstSend) {
        // Draft mode: delegate to App.tsx which creates the real thread first
        onFirstSend(content);
      } else {
        sendMessage(thread.id, content);
      }
    },
    [isDraft, onFirstSend, thread.id, sendMessage],
  );

  const handleLoadMore = useCallback(async () => {
    if (!hasMore || isLoadingMore || isDraft) return;
    const el = containerRef.current;
    if (!el) return;
    // Snapshot scroll position BEFORE the state update causes a prepend.
    scrollAdjustRef.current = {
      prevScrollHeight: el.scrollHeight,
      prevScrollTop: el.scrollTop,
    };
    await loadMoreMessages(thread.id);
  }, [
    hasMore,
    isLoadingMore,
    isDraft,
    loadMoreMessages,
    thread.id,
    containerRef,
  ]);

  // Observe the top sentinel — triggers load-more when scrolled into view.
  useEffect(() => {
    const sentinel = topSentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) void handleLoadMore();
      },
      { threshold: 0 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [handleLoadMore]);

  return (
    <div className={styles.chatView}>
      {/* ── Header ── */}
      <ChatHeader
        thread={thread}
        onToggleConfig={isDraft ? undefined : () => setConfigOpen((o) => !o)}
        onMobileMenuOpen={onMobileMenuOpen}
      />

      {/* ── Messages area ── */}
      <div ref={containerRef} className={`${styles.messages} scrollbar-thin`}>
        <MessageListErrorBoundary>
          {isLoadingMessages ? (
            <div className={styles.loading}>Loading messages…</div>
          ) : visibleMessages.length === 0 && !isStreaming ? (
            /* Empty thread */
            <div className={styles.emptyThread}>
              <div className={styles.emptyEmoji}>{personaEmoji}</div>
              <div className={styles.emptyTitle}>
                Start a conversation with {personaName}
              </div>
              <div className={styles.emptyHint}>
                Send a message below to begin.
              </div>
            </div>
          ) : (
            <>
              {/* Top sentinel — triggers load-more when scrolled into view */}
              <div
                ref={topSentinelRef}
                style={{ height: 1 }}
                aria-hidden="true"
              />
              {isLoadingMore && (
                <div className={styles.loadingMore}>
                  Loading older messages…
                </div>
              )}
              {processedItems.map((item) => {
                if (item.type === "date_divider") {
                  return (
                    <div key={`date-${item.key}`} className="date-divider">
                      {item.label}
                    </div>
                  );
                }
                if (item.type === "tool_group") {
                  return (
                    <ProcessingBubble
                      key={item.executionId}
                      rounds={item.rounds}
                      personaEmoji={personaEmoji}
                      personaName={personaName}
                    />
                  );
                }
                return (
                  <Fragment key={item.message.id}>
                    <MessageBubble
                      message={item.message}
                      personaEmoji={personaEmoji}
                      personaName={personaName}
                    />
                  </Fragment>
                );
              })}

              {showFallbackProcessing && (
                <ProcessingBubble
                  key="fallback-processing"
                  rounds={lastProcessingRounds!}
                  personaEmoji={personaEmoji}
                  personaName={personaName}
                />
              )}

              {/* Sending state — waiting for first token */}
              {isSending && (
                <StreamingBubble
                  personaEmoji={personaEmoji}
                  personaName={personaName}
                  content=""
                />
              )}

              {/* Streaming state — render each entry in order */}
              {isStreaming &&
                streamingEntries.map((entry, i) => {
                  const isLast = i === streamingEntries.length - 1;

                  if (entry.type === "text") {
                    return (
                      <StreamingBubble
                        key={`stream-${i}`}
                        personaEmoji={personaEmoji}
                        personaName={personaName}
                        content={entry.content}
                        streaming={isLast}
                      />
                    );
                  }

                  if (entry.type === "processing") {
                    return (
                      <ProcessingBubble
                        key={`processing-${i}`}
                        rounds={entry.rounds}
                        personaEmoji={personaEmoji}
                        personaName={personaName}
                        streaming={isStreaming}
                      />
                    );
                  }

                  return null;
                })}

              {/* Scroll sentinel — always rendered at the bottom of the list */}
              <div ref={bottomRef} style={{ height: 0, overflow: "hidden" }} />
            </>
          )}
        </MessageListErrorBoundary>

        {/* Error banner */}
        {messageError && !isStreaming && (
          <div className={styles.errorBanner}>⚠ {messageError}</div>
        )}
      </div>

      {/* ── Input bar ── */}
      <MessageInput
        threadId={thread.id}
        personaName={personaName}
        isSending={isSending}
        isStreaming={isStreaming}
        onSend={handleSend}
        onCancel={handleCancel}
        queuedCount={queuedCount}
      />

      {/* ── Config pane — hidden in draft mode ── */}
      {!isDraft && (
        <ConfigPane
          isOpen={configOpen}
          thread={thread}
          onClose={() => setConfigOpen(false)}
          onThreadUpdated={handleThreadUpdated}
          onArchiveThread={archiveThread}
        />
      )}
    </div>
  );
}

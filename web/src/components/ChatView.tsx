import {
  useEffect,
  useLayoutEffect,
  useState,
  useCallback,
  useRef,
  Fragment,
  Component,
} from "react";
import type { ReactNode, ErrorInfo } from "react";
import type { Thread, ThreadState, MessageAttachment } from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { useThreadStore } from "@/stores/useThreadStore";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { useProcessedMessages } from "@/hooks/useProcessedMessages";
import { MessageBubble, StreamingBubble } from "./MessageBubble";
import { ProcessingBubble } from "./ProcessingBlock";
import { MessageInput } from "./MessageInput";
import { ChatHeader } from "./ChatHeader";
import { ConfigPane } from "./ConfigPane";
import { FileExplorerModal } from "./FileExplorerModal";
import { fsApi } from "@/api/client";

import { WarningAlt } from "@carbon/icons-react";
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
            <WarningAlt size={14} /> Unable to render messages
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
  const [explorerOpen, setExplorerOpen] = useState(false);
  const [explorerPath, setExplorerPath] = useState("");

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

  const processedItems = useProcessedMessages(visibleMessages);

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
    (content: string, attachments: MessageAttachment[]) => {
      if (isDraft && onFirstSend) {
        // Draft mode: delegate to App.tsx which creates the real thread first
        onFirstSend(content);
      } else {
        sendMessage(thread.id, content, attachments);
      }
    },
    [isDraft, onFirstSend, thread.id, sendMessage],
  );

  const handleOpenExplorer = useCallback(async () => {
    try {
      const res = await fsApi.workspace(thread.id, thread.title);
      setExplorerPath(res.data.path);
      setExplorerOpen(true);
    } catch {
      // If workspace fetch fails, open explorer at home
      setExplorerPath("~");
      setExplorerOpen(true);
    }
  }, [thread.id, thread.title]);

  const handleFilePath = useCallback((path: string) => {
    setExplorerPath(path);
    setExplorerOpen(true);
  }, []);

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
        onOpenExplorer={handleOpenExplorer}
        onTitleUpdate={(updated) =>
          upsertThread({ ...updated, persona: thread.persona })
        }
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
              {(() => {
                const lastToolGroupItem = processedItems
                  .filter((i) => i.type === "tool_group")
                  .at(-1);
                return processedItems.map((item) => {
                  if (item.type === "date_divider") {
                    return (
                      <div key={`date-${item.key}`} className="date-divider">
                        {item.label}
                      </div>
                    );
                  }
                  if (item.type === "tool_group") {
                    const isLast = item === lastToolGroupItem;
                    const rounds =
                      isLast &&
                      lastProcessingRounds &&
                      lastProcessingRounds.length > 0
                        ? lastProcessingRounds
                        : item.rounds;
                    return (
                      <ProcessingBubble
                        key={item.executionId}
                        rounds={rounds}
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
                        onFilePath={handleFilePath}
                      />
                    </Fragment>
                  );
                });
              })()}

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
                  onFilePath={handleFilePath}
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
                        onFilePath={handleFilePath}
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
          <div className={styles.errorBanner}>
            <WarningAlt size={14} /> {messageError}
          </div>
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

      <FileExplorerModal
        isOpen={explorerOpen}
        initialPath={explorerPath}
        onClose={() => setExplorerOpen(false)}
      />
    </div>
  );
}

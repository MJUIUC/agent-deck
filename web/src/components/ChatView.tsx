import {
  useEffect,
  useLayoutEffect,
  useState,
  useCallback,
  useRef,
} from "react";
import type { Thread, ThreadState } from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { useThreadStore } from "@/stores/useThreadStore";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { groupByDate } from "@/hooks/useTimeFormat";
import { MessageBubble, StreamingBubble } from "./MessageBubble";
import { MessageInput } from "./MessageInput";
import { ChatHeader } from "./ChatHeader";
import { ConfigPane } from "./ConfigPane";

import styles from "./ChatView.module.css";

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
  const loadMessages = useMessageStore((s) => s.loadMessages);
  const sendMessage = useMessageStore((s) => s.sendMessage);

  const { messages, phase } = threadState;
  const isStreaming = phase.status === "streaming";
  const isSending = phase.status === "sending";
  const cancelRun = useMessageStore((s) => s.cancelRun);
  const queuedCount = useMessageStore(
    (s) => s.threads[thread.id]?.queuedCount ?? 0,
  );
  const streamingContent = phase.status === "streaming" ? phase.content : "";
  const messageError = phase.status === "error" ? phase.message : null;

  const visibleMessages = messages.filter((m) => m.visibility !== "hidden");

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

  const { containerRef } = useAutoScroll([messages.length, streamingContent]);

  // Sentinel ref — a zero-size div pinned to the bottom of the message list.
  // Using a ref callback means React calls it synchronously when the element
  // is added to the DOM, at which point scrollIntoView is guaranteed to work.
  const bottomRef = useRef<HTMLDivElement>(null);

  // Scroll to the sentinel on every render. ChatView never unmounts when
  // switching threads — it just receives a new thread prop — so there is no
  // reliable dep list that captures every case. Running on every render is
  // cheap and guarantees the view always opens at the bottom.
  useLayoutEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "instant" });
  });

  const grouped = groupByDate(visibleMessages);

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
            {grouped.map(({ dateLabel, dateKey, items }) => (
              <div key={dateKey}>
                {/* Date divider */}
                <div className="date-divider">{dateLabel}</div>

                {items.map((message) => (
                  <MessageBubble
                    key={message.id}
                    message={message}
                    personaEmoji={personaEmoji}
                    personaName={personaName}
                  />
                ))}
              </div>
            ))}

            {/* Streaming bubble — shown as soon as the message is sent so the
                animation appears immediately, not only after the first token */}
            {(isSending || isStreaming) && (
              <StreamingBubble
                personaEmoji={personaEmoji}
                personaName={personaName}
                content={streamingContent}
              />
            )}

            {/* Scroll sentinel — always rendered at the bottom of the list */}
            <div ref={bottomRef} style={{ height: 0, overflow: "hidden" }} />
          </>
        )}

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

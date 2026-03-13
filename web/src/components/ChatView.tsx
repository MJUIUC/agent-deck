import {
  useEffect,
  useLayoutEffect,
  useState,
  useCallback,
  useRef,
} from "react";
import type { Thread, Message } from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { useThreadStore } from "@/stores/useThreadStore";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { groupByDate } from "@/hooks/useTimeFormat";
import { MessageBubble, StreamingBubble } from "./MessageBubble";
import { MessageInput } from "./MessageInput";
import { ChatHeader } from "./ChatHeader";
import { ConfigPane } from "./ConfigPane";
import { threadsApi } from "@/api/client";
import styles from "./ChatView.module.css";

interface ChatViewProps {
  thread: Thread;
  /** When true, this ChatView was just promoted from a draft — the first
   *  stream completion should trigger LLM title generation. */
  isFirstSend?: boolean;
  /** Provided only in draft mode (thread.id === "pending"). Called with the
   *  user's message content; creates the real thread and sends the message. */
  onFirstSend?: (content: string) => Promise<void>;
  onMobileMenuOpen?: () => void;
}

// Stable fallbacks — same reference every render, so Zustand's getSnapshot
// never thinks the value changed when the thread key is simply absent.
const EMPTY_MESSAGES: Message[] = [];
const EMPTY_STRING = "";

export function ChatView({
  thread,
  isFirstSend = false,
  onFirstSend,
  onMobileMenuOpen,
}: ChatViewProps) {
  const isDraft = thread.id === "pending";
  const [configOpen, setConfigOpen] = useState(false);

  // Keep the thread id in a ref so selector closures don't go stale when the
  // prop changes between renders but before the effect re-runs.
  const threadIdRef = useRef(thread.id);
  threadIdRef.current = thread.id;

  const messages = useMessageStore(
    (s) => s.messagesByThread[thread.id] ?? EMPTY_MESSAGES,
  );
  const streamingContent = useMessageStore(
    (s) => s.streamingContent[thread.id] ?? EMPTY_STRING,
  );
  const isStreaming = useMessageStore((s) => s.isStreaming[thread.id] ?? false);
  const isSending = useMessageStore((s) => s.isSending[thread.id] ?? false);
  const isLoadingMessages = useMessageStore((s) => s.isLoadingMessages);
  const messageError = useMessageStore((s) => s.error);
  const loadMessages = useMessageStore((s) => s.loadMessages);
  const sendMessage = useMessageStore((s) => s.sendMessage);

  const connectThread = useSseStore((s) => s.connectThread);
  const disconnectThread = useSseStore((s) => s.disconnectThread);

  const upsertThread = useThreadStore((s) => s.upsertThread);
  const archiveThread = useThreadStore((s) => s.archiveThread);

  // Seed the first-send ref on mount when we've been promoted from a draft.
  // This must run before any messages arrive, hence the empty dep array.
  // The ref tracks whether the NEXT stream completion should fire title-gen.
  const titleGenPendingRef = useRef<boolean>(false);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (isFirstSend) {
      titleGenPendingRef.current = true;
    }
  }, []);

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

  // ── Title generation after first agent response ───────────────────────────
  // Fires once: when isStreaming transitions to false while titleGenPendingRef
  // is set.  Uses a prev-value ref to detect the falling edge.
  const prevIsStreaming = useRef<boolean>(false);
  useEffect(() => {
    const wasStreaming = prevIsStreaming.current;
    prevIsStreaming.current = isStreaming;

    if (
      wasStreaming &&
      !isStreaming &&
      titleGenPendingRef.current &&
      !isDraft
    ) {
      titleGenPendingRef.current = false;
      threadsApi
        .generateTitle(thread.id)
        .then(({ data }) => {
          upsertThread({ ...thread, title: data.title });
        })
        .catch(() => {
          // Title generation failed — leave as-is, not a fatal error
        });
    }
  }, [isStreaming]); // eslint-disable-line react-hooks/exhaustive-deps

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

  const visibleMessages = messages.filter((m) => m.visibility !== "hidden");
  const grouped = groupByDate(visibleMessages);

  const persona = thread.persona;
  const personaEmoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

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

  const handleThreadUpdated = useCallback(
    (updated: Thread) => {
      upsertThread({ ...updated, persona: thread.persona });
    },
    [upsertThread, thread.persona],
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
            {grouped.map(({ dateLabel, items }) => (
              <div key={dateLabel}>
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

            {/* Streaming bubble */}
            {isStreaming && (
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
        isSending={isSending || isStreaming}
        onSend={handleSend}
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

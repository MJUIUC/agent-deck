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
import styles from "./ChatView.module.css";

interface ChatViewProps {
  thread: Thread;
  onMobileMenuOpen?: () => void;
}

// Stable fallbacks — same reference every render, so Zustand's getSnapshot
// never thinks the value changed when the thread key is simply absent.
const EMPTY_MESSAGES: Message[] = [];
const EMPTY_STRING = "";

export function ChatView({ thread, onMobileMenuOpen }: ChatViewProps) {
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
  const sendCommand = useMessageStore((s) => s.sendCommand);

  const connectThread = useSseStore((s) => s.connectThread);
  const disconnectThread = useSseStore((s) => s.disconnectThread);

  const upsertThread = useThreadStore((s) => s.upsertThread);

  useEffect(() => {
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

  const visibleMessages = messages.filter((m) => m.visibility !== "hidden");
  const grouped = groupByDate(visibleMessages);

  const persona = thread.persona;
  const personaEmoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

  const handleSend = useCallback(
    (content: string) => sendMessage(thread.id, content),
    [thread.id, sendMessage],
  );

  const handleCommand = useCallback(
    (input: string) => sendCommand(thread.id, input),
    [thread.id, sendCommand],
  );

  const handleThreadUpdated = useCallback(
    (updated: Thread) => {
      upsertThread({ ...updated, persona: thread.persona });
    },
    [upsertThread, thread.persona],
  );

  const modelName = thread.active_model ?? undefined;

  return (
    <div className={styles.chatView}>
      {/* ── Header ── */}
      <ChatHeader
        thread={thread}
        onToggleConfig={() => setConfigOpen((o) => !o)}
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
              Send a message below to begin. Use <code>/help</code> to see
              available slash commands.
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
        modelName={modelName}
        isSending={isSending || isStreaming}
        onSend={handleSend}
        onCommand={handleCommand}
      />

      {/* ── Config pane (slides in from right) ── */}
      <ConfigPane
        isOpen={configOpen}
        thread={thread}
        onClose={() => setConfigOpen(false)}
        onThreadUpdated={handleThreadUpdated}
      />
    </div>
  );
}

// ─── MobileChatView.tsx ────────────────────────────────────────────────────────
// Full-screen chat view for mobile. Manages SSE connection, message rendering,
// auto-growing input, keyboard lift, and the config bottom-sheet.
// ─────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback } from "react";
import type { Thread } from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { resolveDisplayNames } from "@/components/ChatHeader";
import { MessageBubble, StreamingBubble } from "@/components/MessageBubble";
import { MobileConfigSheet } from "./MobileConfigSheet";
import styles from "./MobileChatView.module.css";

// ─── Props ────────────────────────────────────────────────────────────────────

interface MobileChatViewProps {
  thread: Thread | null;
  onBack: () => void;
  onFirstSend?: (content: string) => Promise<void>;
}

// ─── Icon sub-components ──────────────────────────────────────────────────────

function BackIcon() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <polyline points="15 18 9 12 15 6" />
    </svg>
  );
}

function ConfigIcon() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {/* Sliders icon */}
      <line x1="4" y1="6" x2="20" y2="6" />
      <line x1="4" y1="12" x2="20" y2="12" />
      <line x1="4" y1="18" x2="20" y2="18" />
      <circle cx="9" cy="6" r="2.5" fill="var(--bg-secondary)" />
      <circle cx="15" cy="12" r="2.5" fill="var(--bg-secondary)" />
      <circle cx="9" cy="18" r="2.5" fill="var(--bg-secondary)" />
    </svg>
  );
}

function SendIcon() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <line x1="12" y1="19" x2="12" y2="5" />
      <polyline points="5 12 12 5 19 12" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
    >
      <rect x="4" y="4" width="16" height="16" rx="2" />
    </svg>
  );
}

// ─── Constants ────────────────────────────────────────────────────────────────

/** Line height for the auto-grow calculation: 15px font × 1.4 = 21px */
const LINE_HEIGHT_PX = 21;
const MAX_TEXTAREA_LINES = 5;
const MAX_TEXTAREA_HEIGHT = LINE_HEIGHT_PX * MAX_TEXTAREA_LINES;

// ─── Component ────────────────────────────────────────────────────────────────

export function MobileChatView({
  thread,
  onBack,
  onFirstSend,
}: MobileChatViewProps) {
  const [inputValue, setInputValue] = useState("");
  const [configSheetOpen, setConfigSheetOpen] = useState(false);
  const [keyboardOffset, setKeyboardOffset] = useState(0);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const threadId = thread?.id ?? null;

  // ── Store selectors ──────────────────────────────────────────────────────────

  const threadState = useMessageStore((s) =>
    threadId != null ? s.threads[threadId] : undefined,
  );
  const sendMessage = useMessageStore((s) => s.sendMessage);
  const loadMessages = useMessageStore((s) => s.loadMessages);
  const cancelRun = useMessageStore((s) => s.cancelRun);

  const connectThread = useSseStore((s) => s.connectThread);
  const disconnectThread = useSseStore((s) => s.disconnectThread);

  // ── Derived values ───────────────────────────────────────────────────────────

  const messages = threadState?.messages ?? [];
  const phase = threadState?.phase ?? { status: "idle" as const };
  const isStreaming =
    phase.status === "streaming" || phase.status === "sending";
  const streamingContent = phase.status === "streaming" ? phase.content : "";

  const visibleMessages = messages.filter((m) => m.visibility === "visible");

  const personaEmoji = thread?.persona?.emoji ?? "🤖";
  const personaName = thread?.persona?.name ?? "Agent";

  const [modelName, setModelName] = useState<string | null>(null);

  useEffect(() => {
    if (!thread) return;
    let cancelled = false;
    const providerUuid =
      thread.active_provider ?? thread.persona?.default_provider ?? null;
    const modelUuid =
      thread.active_model ?? thread.persona?.default_model ?? null;
    resolveDisplayNames(providerUuid, modelUuid).then(({ modelName: mn }) => {
      if (!cancelled) setModelName(mn);
    });
    return () => {
      cancelled = true;
    };
  }, [
    thread?.active_provider,
    thread?.active_model,
    thread?.persona?.default_provider,
    thread?.persona?.default_model,
  ]);

  const subtitleText = modelName
    ? `${personaName} · ${modelName}`
    : personaName;

  // ── SSE connect / disconnect on thread change ────────────────────────────────

  useEffect(() => {
    if (!threadId || threadId === "pending") return;

    connectThread(threadId);
    loadMessages(threadId);

    return () => {
      disconnectThread();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [threadId]);

  // ── Auto-scroll to bottom on new messages or streaming content ────────────────

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [visibleMessages.length, streamingContent]);

  // ── Keyboard lift via visualViewport ─────────────────────────────────────────

  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return;

    const handleResize = () => {
      const offset = Math.max(0, window.innerHeight - vv.offsetTop - vv.height);
      setKeyboardOffset(offset);
    };

    vv.addEventListener("resize", handleResize);
    vv.addEventListener("scroll", handleResize);

    return () => {
      vv.removeEventListener("resize", handleResize);
      vv.removeEventListener("scroll", handleResize);
    };
  }, []);

  // ── Auto-grow textarea ───────────────────────────────────────────────────────

  const growTextarea = useCallback(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, MAX_TEXTAREA_HEIGHT) + "px";
  }, []);

  // ── Send handler ─────────────────────────────────────────────────────────────

  const handleCancel = useCallback(() => {
    if (threadId && threadId !== "pending") {
      void cancelRun(threadId);
    }
  }, [cancelRun, threadId]);

  const handleSend = useCallback(async () => {
    const content = inputValue.trim();
    if (!content || isStreaming || !threadId) return;

    setInputValue("");

    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }

    if (threadId === "pending" && onFirstSend) {
      await onFirstSend(content);
    } else {
      await sendMessage(threadId, content);
    }
  }, [inputValue, isStreaming, threadId, onFirstSend, sendMessage]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void handleSend();
      }
    },
    [handleSend],
  );

  const handleInputChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setInputValue(e.target.value);
      growTextarea();
    },
    [growTextarea],
  );

  // ── No thread selected — full-screen empty state ─────────────────────────────

  if (!thread) {
    return (
      <div className={styles.noThread}>
        <span className={styles.noThreadEmoji} aria-hidden="true">
          💬
        </span>
        <p className={styles.noThreadTitle}>Select a thread</p>
        <p className={styles.noThreadSubtext}>
          Choose from the list or start a new chat
        </p>
      </div>
    );
  }

  // ── Thread selected ──────────────────────────────────────────────────────────

  const sendDisabled = !inputValue.trim() || isStreaming;

  return (
    <div className={styles.container}>
      {/* ── Nav header ── */}
      <header className={styles.navHeader}>
        {/* Back button */}
        <button
          className={styles.backBtn}
          type="button"
          aria-label="Back to threads"
          onClick={onBack}
        >
          <BackIcon />
        </button>

        {/* Agent info (center) */}
        <div className={styles.agentInfo}>
          <div className={styles.agentAvatar} aria-hidden="true">
            {personaEmoji}
          </div>
          <div className={styles.agentText}>
            <div className={styles.agentName}>{thread.title}</div>
            <div className={styles.agentStatus}>
              <span>{subtitleText}</span>
            </div>
          </div>
        </div>

        {/* Nav actions (right) */}
        <div className={styles.navActions}>
          <button
            className={styles.navActionBtn}
            type="button"
            aria-label="Thread configuration"
            title="Thread configuration"
            onClick={() => setConfigSheetOpen(true)}
          >
            <ConfigIcon />
          </button>
        </div>
      </header>

      {/* ── Messages area ── */}
      <div className={styles.messagesArea} aria-label="Messages" role="log">
        {visibleMessages.length === 0 && !isStreaming ? (
          <div className={styles.conversationEmpty} aria-live="polite">
            <p className={styles.conversationEmptyText}>
              Start the conversation
            </p>
          </div>
        ) : (
          <>
            {visibleMessages.map((message) => (
              <MessageBubble
                key={message.id}
                message={message}
                personaEmoji={personaEmoji}
                personaName={personaName}
              />
            ))}

            {isStreaming && (
              <StreamingBubble
                personaEmoji={personaEmoji}
                personaName={personaName}
                content={streamingContent}
              />
            )}
          </>
        )}

        {/* Scroll anchor */}
        <div ref={messagesEndRef} aria-hidden="true" />
      </div>

      {/* ── Input area ── */}
      <div
        className={styles.inputArea}
        style={{
          paddingBottom: `calc(${keyboardOffset}px + env(safe-area-inset-bottom, 0px) + 8px)`,
        }}
      >
        <div className={styles.inputRow}>
          {/* Pill-shaped input wrap */}
          <div className={styles.inputWrap}>
            <textarea
              ref={textareaRef}
              className={styles.chatInput}
              placeholder="Message…"
              value={inputValue}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              rows={1}
              aria-label="Message input"
              aria-multiline="true"
            />

            {/* Stop button while streaming, send button otherwise */}
            {isStreaming ? (
              <button
                className={styles.stopBtn}
                type="button"
                aria-label="Stop generation"
                onClick={handleCancel}
              >
                <StopIcon />
              </button>
            ) : (
              <button
                className={styles.sendBtn}
                type="button"
                aria-label="Send message"
                onClick={() => void handleSend()}
                disabled={sendDisabled}
              >
                <SendIcon />
              </button>
            )}
          </div>
        </div>
      </div>

      {/* ── Config bottom-sheet ── */}
      <MobileConfigSheet
        thread={thread}
        isOpen={configSheetOpen}
        onClose={() => setConfigSheetOpen(false)}
      />
    </div>
  );
}

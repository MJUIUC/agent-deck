// ─── MobileChatView.tsx ────────────────────────────────────────────────────────
// Full-screen chat view for mobile. Manages SSE connection, message rendering,
// auto-growing input, keyboard lift, and the config bottom-sheet.
// ─────────────────────────────────────────────────────────────────────────────

import {
  useState,
  useEffect,
  useLayoutEffect,
  useRef,
  useCallback,
} from "react";
import { FolderOpen } from "lucide-react";
import type { Thread, MessageAttachment } from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { useThreadStore } from "@/stores/useThreadStore";
import { fsApi, threadsApi, uploadsApi } from "@/api/client";
import { resolveDisplayNames } from "@/components/ChatHeader";
import { MessageBubble, StreamingBubble } from "@/components/MessageBubble";
import { ProcessingBubble } from "@/components/ProcessingBlock";
import { useProcessedMessages } from "@/hooks/useProcessedMessages";
import { FileExplorerModal } from "@/components/FileExplorerModal";
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
  const [pendingFiles, setPendingFiles] = useState<File[]>([]);
  const [uploading, setUploading] = useState(false);
  const fileInputMobileRef = useRef<HTMLInputElement>(null);
  const [configSheetOpen, setConfigSheetOpen] = useState(false);
  const [titleEditing, setTitleEditing] = useState(false);
  const [titleEditValue, setTitleEditValue] = useState("");
  const [titleSaving, setTitleSaving] = useState(false);
  const titleInputRef = useRef<HTMLInputElement>(null);
  const [keyboardOffset, setKeyboardOffset] = useState(0);
  const [explorerOpen, setExplorerOpen] = useState(false);
  const [explorerPath, setExplorerPath] = useState("");

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesAreaRef = useRef<HTMLDivElement>(null);
  const topSentinelRef = useRef<HTMLDivElement>(null);
  const scrollAdjustRef = useRef<{
    prevScrollHeight: number;
    prevScrollTop: number;
  } | null>(null);

  const threadId = thread?.id ?? null;

  // ── Store selectors ──────────────────────────────────────────────────────────

  const threadState = useMessageStore((s) =>
    threadId != null ? s.threads[threadId] : undefined,
  );
  const sendMessage = useMessageStore((s) => s.sendMessage);
  const loadMessages = useMessageStore((s) => s.loadMessages);
  const cancelRun = useMessageStore((s) => s.cancelRun);

  const hasMore = useMessageStore((s) =>
    threadId != null ? (s.threads[threadId]?.hasMore ?? false) : false,
  );
  const isLoadingMore = useMessageStore((s) =>
    threadId != null ? (s.threads[threadId]?.isLoadingMore ?? false) : false,
  );
  const loadMoreMessages = useMessageStore((s) => s.loadMoreMessages);

  const connectThread = useSseStore((s) => s.connectThread);
  const disconnectThread = useSseStore((s) => s.disconnectThread);

  // ── Derived values ───────────────────────────────────────────────────────────

  const messages = threadState?.messages ?? [];
  const phase = threadState?.phase ?? { status: "idle" as const };
  const isStreaming =
    phase.status === "streaming" || phase.status === "sending";
  const isSending = phase.status === "sending";
  const streamingEntries = phase.status === "streaming" ? phase.entries : [];

  const visibleMessages = messages.filter((m) => {
    if (m.visibility === "hidden" && m.source !== "tool") return false;
    return true;
  });

  const processedItems = useProcessedMessages(visibleMessages);

  const lastProcessingRounds = useMessageStore(
    (s) => s.threads[threadId ?? ""]?.lastProcessingRounds ?? null,
  );

  const showFallbackProcessing =
    !isStreaming &&
    lastProcessingRounds !== null &&
    lastProcessingRounds.length > 0 &&
    !processedItems.some((item) => item.type === "tool_group");

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

  // ── Scroll management ────────────────────────────────────────────────────────

  // Runs on every render: restores scroll position after a load-more prepend,
  // or scrolls to bottom instantly on thread change / new messages.
  // No dep array — mirrors the desktop pattern; scrollAdjustRef suppresses it
  // during prepends so the viewport doesn't jump.
  useLayoutEffect(() => {
    const adj = scrollAdjustRef.current;
    if (adj !== null) {
      // Load-more prepend: restore scroll position so content doesn't jump
      const el = messagesAreaRef.current;
      if (el) {
        el.scrollTop =
          adj.prevScrollTop + (el.scrollHeight - adj.prevScrollHeight);
      }
      scrollAdjustRef.current = null;
    } else {
      // Normal: scroll to bottom instantly (avoids "scroll from top" animation)
      messagesEndRef.current?.scrollIntoView({ behavior: "instant" });
    }
  });

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

  // ── Load-more handler & top-sentinel IntersectionObserver ────────────────────

  const handleLoadMore = useCallback(async () => {
    if (!hasMore || isLoadingMore || !threadId || threadId === "pending")
      return;
    const el = messagesAreaRef.current;
    if (!el) return;
    scrollAdjustRef.current = {
      prevScrollHeight: el.scrollHeight,
      prevScrollTop: el.scrollTop,
    };
    await loadMoreMessages(threadId);
  }, [hasMore, isLoadingMore, threadId, loadMoreMessages]);

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

  // ── Send handler ─────────────────────────────────────────────────────────────

  const handleCancel = useCallback(() => {
    if (threadId && threadId !== "pending") {
      void cancelRun(threadId);
    }
  }, [cancelRun, threadId]);

  const handleSend = useCallback(async () => {
    const content = inputValue.trim();
    if (
      (!content && pendingFiles.length === 0) ||
      isStreaming ||
      !threadId ||
      uploading
    )
      return;

    setInputValue("");
    setPendingFiles([]);

    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }

    let uploaded: MessageAttachment[] = [];
    try {
      setUploading(true);
      for (const file of pendingFiles) {
        const res = await uploadsApi.upload(threadId, file);
        uploaded.push({
          path: res.data.path,
          filename: res.data.filename,
          content_type: res.data.content_type,
        });
      }
    } catch {
      /* skip failed uploads */
    } finally {
      setUploading(false);
    }

    if (threadId === "pending" && onFirstSend) {
      await onFirstSend(content);
    } else {
      await sendMessage(threadId, content, uploaded);
    }
  }, [
    inputValue,
    pendingFiles,
    isStreaming,
    uploading,
    threadId,
    onFirstSend,
    sendMessage,
  ]);

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const files = Array.from(e.target.files ?? []);
      setPendingFiles((prev) => [...prev, ...files]);
      e.target.value = "";
    },
    [],
  );

  const handleInputChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setInputValue(e.target.value);
      growTextarea();
    },
    [growTextarea],
  );

  const startTitleEdit = useCallback(() => {
    if (!thread) return;
    setTitleEditValue(thread.title);
    setTitleEditing(true);
  }, [thread]);

  const cancelTitleEdit = useCallback(() => {
    setTitleEditing(false);
    setTitleEditValue("");
  }, []);

  const commitTitleEdit = useCallback(async () => {
    if (!thread || !titleEditing) return;
    const trimmed = titleEditValue.trim();
    if (!trimmed || trimmed === thread.title) {
      cancelTitleEdit();
      return;
    }
    setTitleSaving(true);
    try {
      const res = await threadsApi.update(thread.id, { title: trimmed });
      useThreadStore.getState().upsertThread(res.data);
    } catch {
      // revert silently — thread title from store will re-render on next update
    } finally {
      setTitleSaving(false);
      setTitleEditing(false);
      setTitleEditValue("");
    }
  }, [thread, titleEditing, titleEditValue, cancelTitleEdit]);

  useEffect(() => {
    if (titleEditing && titleInputRef.current) {
      titleInputRef.current.focus();
      titleInputRef.current.select();
    }
  }, [titleEditing]);

  const handleOpenExplorer = useCallback(async () => {
    if (!threadId) return;
    try {
      const res = await fsApi.workspace(threadId, thread?.title ?? undefined);
      setExplorerPath(res.data.path);
      setExplorerOpen(true);
    } catch {
      setExplorerPath("~");
      setExplorerOpen(true);
    }
  }, [threadId, thread?.title]);

  const handleFilePath = useCallback((path: string) => {
    setExplorerPath(path);
    setExplorerOpen(true);
  }, []);

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

  const sendDisabled =
    (!inputValue.trim() && pendingFiles.length === 0) ||
    isStreaming ||
    uploading;

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
            {titleEditing ? (
              <input
                ref={titleInputRef}
                className={styles.agentNameInput}
                value={titleEditValue}
                disabled={titleSaving}
                onChange={(e) => setTitleEditValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void commitTitleEdit();
                  if (e.key === "Escape") cancelTitleEdit();
                }}
                onBlur={() => void commitTitleEdit()}
                aria-label="Edit thread title"
              />
            ) : (
              <div
                className={styles.agentName}
                onClick={startTitleEdit}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") startTitleEdit();
                }}
                title="Tap to rename"
              >
                {thread.title}
              </div>
            )}
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
            aria-label="File explorer"
            title="File explorer"
            onClick={() => void handleOpenExplorer()}
          >
            <FolderOpen size={18} />
          </button>
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
      <div
        ref={messagesAreaRef}
        className={styles.messagesArea}
        aria-label="Messages"
        role="log"
      >
        {visibleMessages.length === 0 && !isStreaming ? (
          <div className={styles.conversationEmpty} aria-live="polite">
            <p className={styles.conversationEmptyText}>
              Start the conversation
            </p>
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
              <p className={styles.loadingMore}>Loading older messages…</p>
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
                  <MessageBubble
                    key={item.message.id}
                    message={item.message}
                    personaEmoji={personaEmoji}
                    personaName={personaName}
                    onFilePath={handleFilePath}
                  />
                );
              });
            })()}

            {isSending && (
              <StreamingBubble
                personaEmoji={personaEmoji}
                personaName={personaName}
                content=""
                onFilePath={handleFilePath}
              />
            )}

            {isStreaming &&
              !isSending &&
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
                      streaming={isLast}
                    />
                  );
                }

                return null;
              })}

            {showFallbackProcessing && (
              <ProcessingBubble
                key="fallback-processing"
                rounds={lastProcessingRounds!}
                personaEmoji={personaEmoji}
                personaName={personaName}
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
        {/* Attachment chips */}
        {pendingFiles.length > 0 && (
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: 6,
              padding: "6px 16px 0",
            }}
          >
            {pendingFiles.map((file, idx) => (
              <div
                key={idx}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 5,
                  background: "var(--bg-elevated)",
                  border: "1px solid var(--border-subtle)",
                  borderRadius: 8,
                  padding: "3px 8px 3px 5px",
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  maxWidth: 200,
                }}
              >
                {file.type.startsWith("image/") ? (
                  <img
                    src={URL.createObjectURL(file)}
                    alt={file.name}
                    style={{
                      width: 22,
                      height: 22,
                      objectFit: "cover",
                      borderRadius: 4,
                    }}
                  />
                ) : (
                  <span>📎</span>
                )}
                <span
                  style={{
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    maxWidth: 100,
                  }}
                >
                  {file.name}
                </span>
                {uploading ? (
                  <span>⏳</span>
                ) : (
                  <button
                    type="button"
                    onClick={() =>
                      setPendingFiles((p) => p.filter((_, i) => i !== idx))
                    }
                    style={{
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      color: "var(--text-tertiary)",
                      fontSize: 14,
                      padding: "0 2px",
                    }}
                  >
                    ×
                  </button>
                )}
              </div>
            ))}
          </div>
        )}

        <div className={styles.inputRow}>
          {/* Pill-shaped input wrap */}
          <div className={styles.inputWrap}>
            {/* Paperclip button */}
            <button
              type="button"
              aria-label="Attach file"
              onClick={() => fileInputMobileRef.current?.click()}
              disabled={isStreaming || uploading}
              style={{
                background: "none",
                border: "none",
                cursor: "pointer",
                color: "var(--text-tertiary)",
                fontSize: 18,
                padding: "0 4px",
                display: "flex",
                alignItems: "center",
                flexShrink: 0,
                opacity: isStreaming || uploading ? 0.4 : 1,
              }}
            >
              📎
            </button>
            <textarea
              ref={textareaRef}
              className={styles.chatInput}
              placeholder="Message…"
              value={inputValue}
              onChange={handleInputChange}
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

        {/* Hidden file input */}
        <input
          ref={fileInputMobileRef}
          type="file"
          multiple
          accept="image/*,application/pdf,text/*,.md,.csv,.json,.txt,.ts,.tsx,.js,.jsx,.py,.rs"
          style={{ display: "none" }}
          onChange={handleFileChange}
        />
      </div>

      {/* ── Config bottom-sheet ── */}
      <MobileConfigSheet
        thread={thread}
        isOpen={configSheetOpen}
        onClose={() => setConfigSheetOpen(false)}
        onArchive={() => {
          setConfigSheetOpen(false);
          onBack();
        }}
      />

      <FileExplorerModal
        isOpen={explorerOpen}
        initialPath={explorerPath}
        onClose={() => setExplorerOpen(false)}
      />
    </div>
  );
}

import { useState, useEffect } from "react";
import { useThreadStore } from "@/stores/useThreadStore";
import { formatThreadTime } from "@/hooks/useTimeFormat";
import type { Thread } from "@/types";

import styles from "./MobileThreadList.module.css";

interface MobileThreadListProps {
  onSelectThread: (threadId: string) => void;
  onCreateThread: () => void;
}

// ── Compose / new-thread icon (pencil on paper) ───────────────────────────────
function ComposeIcon() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
    </svg>
  );
}

// ── Individual thread row ─────────────────────────────────────────────────────
interface ThreadRowProps {
  thread: Thread;
  isActive: boolean;
  onSelect: (threadId: string) => void;
}

function ThreadRow({ thread, isActive, onSelect }: ThreadRowProps) {
  const emoji = thread.persona?.emoji ?? "🤖";
  const timeStr = formatThreadTime(thread.updated_at);
  const preview = thread.last_message_preview ?? "";

  const handleClick = () => onSelect(thread.id);
  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(thread.id);
    }
  };

  return (
    <div
      className={[styles.threadItem, isActive ? styles.threadItemActive : ""]
        .filter(Boolean)
        .join(" ")}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      role="button"
      tabIndex={0}
      aria-pressed={isActive}
    >
      {/* Avatar */}
      <div className={styles.avatar} aria-hidden="true">
        {emoji}
      </div>

      {/* Text content */}
      <div className={styles.threadContent}>
        <div className={styles.threadTop}>
          <span className={styles.threadName}>{thread.title}</span>
          <span className={styles.threadTime}>{timeStr}</span>
        </div>
        {preview && <p className={styles.threadPreview}>{preview}</p>}
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────
export function MobileThreadList({
  onSelectThread,
  onCreateThread,
}: MobileThreadListProps) {
  const threads = useThreadStore((s) => s.threads);
  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const isLoading = useThreadStore((s) => s.isLoading);

  const isEmpty = !isLoading && threads.length === 0;

  return (
    <div className={styles.container}>
      {/* ── Nav bar ── */}
      <nav className={styles.navBar} aria-label="Thread list navigation">
        <span className={styles.navTitle}>agent-deck</span>
        <div className={styles.navActions}>
          <button
            className={styles.navIconBtn}
            type="button"
            aria-label="New conversation"
            title="New conversation"
            onClick={onCreateThread}
          >
            <ComposeIcon />
          </button>
        </div>
      </nav>

      {/* ── Thread list ── */}
      <div className={styles.threadList} role="list" aria-label="Conversations">
        {isEmpty ? (
          <div className={styles.emptyState} aria-live="polite">
            <p className={styles.emptyTitle}>No conversations yet</p>
            <p className={styles.emptySubtitle}>Tap + to start a new chat</p>
          </div>
        ) : (
          <>
            {threads.map((thread) => (
              <ThreadRow
                key={thread.id}
                thread={thread}
                isActive={thread.id === activeThreadId}
                onSelect={onSelectThread}
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}

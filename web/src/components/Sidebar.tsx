import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import type { Thread, AgentPersona } from "@/types";
import { ThreadItem } from "./ThreadItem";
import { PersonaPickerModal } from "./PersonaPickerModal";
import { Plus, Settings, Archive } from "lucide-react";
import styles from "./Sidebar.module.css";

interface SidebarProps {
  threads: Thread[];
  personas: AgentPersona[];
  activeThreadId: string | null;
  isLoading: boolean;
  isCreating: boolean;
  isMobileOpen?: boolean;
  onSelectThread: (threadId: string) => void;
  onCreateThread: (personaId: string) => void;
  onOpenSettings: () => void;
  onMobileClose?: () => void;
}

function getGroupLabel(updatedAt: string): string {
  const date = new Date(updatedAt);
  const now = new Date();
  const startOfToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
  );
  const startOfYesterday = new Date(startOfToday.getTime() - 86400000);
  const sevenDaysAgo = new Date(startOfToday.getTime() - 6 * 86400000);

  if (date >= startOfToday) return "Today";
  if (date >= startOfYesterday) return "Yesterday";
  if (date >= sevenDaysAgo) return "This week";
  return "Older";
}

const GROUP_ORDER = ["Today", "Yesterday", "This week", "Older"];

export function Sidebar({
  threads,
  personas,
  activeThreadId,
  isLoading,
  isCreating,
  isMobileOpen = false,
  onSelectThread,
  onCreateThread,
  onOpenSettings,
  onMobileClose,
}: SidebarProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [showNoPersonasNote, setShowNoPersonasNote] = useState(false);
  const noPersonasNoteTimer = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  // Listen for the "agent-deck:new-chat" custom event from EmptyState
  useEffect(() => {
    const handler = () => setIsModalOpen(true);
    window.addEventListener("agent-deck:new-chat", handler);
    return () => window.removeEventListener("agent-deck:new-chat", handler);
  }, []);

  const filteredThreads = useMemo(() => {
    if (!searchQuery.trim()) return threads;
    const q = searchQuery.toLowerCase();
    return threads.filter(
      (t) =>
        t.title.toLowerCase().includes(q) ||
        (t.last_message_preview ?? "").toLowerCase().includes(q) ||
        (t.persona?.name ?? "").toLowerCase().includes(q),
    );
  }, [threads, searchQuery]);

  const groupedThreads = useMemo(() => {
    const map = new Map<string, Thread[]>();
    for (const thread of filteredThreads) {
      const label = getGroupLabel(thread.updated_at);
      if (!map.has(label)) map.set(label, []);
      map.get(label)!.push(thread);
    }
    return GROUP_ORDER.filter((label) => map.has(label)).map((label) => ({
      label,
      threads: map.get(label)!,
    }));
  }, [filteredThreads]);

  const handleNewChat = useCallback(() => {
    if (personas.length === 0) {
      // Show inline note briefly instead of opening the picker
      setShowNoPersonasNote(true);
      if (noPersonasNoteTimer.current)
        clearTimeout(noPersonasNoteTimer.current);
      noPersonasNoteTimer.current = setTimeout(
        () => setShowNoPersonasNote(false),
        3000,
      );
      return;
    }
    setIsModalOpen(true);
  }, [personas.length]);

  const handleModalConfirm = useCallback(
    (personaId: string) => {
      setIsModalOpen(false);
      onCreateThread(personaId);
    },
    [onCreateThread],
  );

  const handleModalCancel = useCallback(() => setIsModalOpen(false), []);

  return (
    <>
      {/* ── Sidebar panel ── */}
      <aside
        className={[styles.sidebar, isMobileOpen ? styles.sidebarOpen : ""]
          .filter(Boolean)
          .join(" ")}
        aria-label="Thread list"
      >
        {/* ── Header ── */}
        <div className={styles.header}>
          {/* Brand row */}
          <div className={styles.brand}>
            <div className={styles.brandInner}>
              <span className={styles.brandEmoji}>🤖</span>
              <span className={styles.brandName}>agent-deck</span>
            </div>
            {/* Mobile close button */}
            {onMobileClose && (
              <button
                onClick={onMobileClose}
                aria-label="Close sidebar"
                className={styles.mobileCloseBtn}
              >
                ✕
              </button>
            )}
          </div>

          {/* New Chat button */}
          <button
            className={[
              styles.newChatBtn,
              personas.length === 0 ? styles.newChatBtnDisabled : "",
            ]
              .filter(Boolean)
              .join(" ")}
            onClick={handleNewChat}
            disabled={isCreating}
            aria-disabled={personas.length === 0}
            title={
              personas.length === 0
                ? "Add a persona in Settings first."
                : undefined
            }
          >
            <Plus size={14} strokeWidth={2.5} />
            {isCreating ? "Creating…" : "New Chat"}
          </button>
          {/* Inline note shown when clicked with no personas */}
          {showNoPersonasNote && (
            <div className={styles.noPersonasNote}>
              Add a persona in Settings first.
            </div>
          )}
        </div>

        {/* ── Search ── */}
        <div className={styles.searchWrap}>
          <input
            className={styles.searchInput}
            type="text"
            placeholder="Search threads…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            aria-label="Search threads"
          />
        </div>

        {/* ── Thread list ── */}
        <div className={`${styles.threadList} scrollbar-thin`}>
          {isLoading ? (
            <div className={styles.loadingText}>Loading…</div>
          ) : threads.length === 0 ? (
            <div className={styles.emptyState}>
              <div className={styles.emptyIcon}>💬</div>
              <div className={styles.emptyTitle}>No threads yet</div>
              <div className={styles.emptyDesc}>
                Start a new chat to begin a conversation with your agent.
              </div>
            </div>
          ) : filteredThreads.length === 0 ? (
            <div className={styles.emptyState}>
              <div className={styles.emptyIcon}>🔍</div>
              <div className={styles.emptyTitle}>No results</div>
              <div className={styles.emptyDesc}>
                Try a different search term.
              </div>
            </div>
          ) : (
            groupedThreads.map(({ label, threads: groupThreads }) => (
              <div key={label}>
                <div className={styles.groupLabel}>{label}</div>
                {groupThreads.map((thread) => (
                  <ThreadItem
                    key={thread.id}
                    thread={thread}
                    isActive={thread.id === activeThreadId}
                    onClick={onSelectThread}
                  />
                ))}
              </div>
            ))
          )}
        </div>

        {/* ── Footer ── */}
        <div className={styles.footer}>
          <button className={styles.footerBtn} onClick={onOpenSettings}>
            <Settings size={13} />
            Settings
          </button>
          <button className={styles.footerBtn}>
            <Archive size={13} />
            Archived
          </button>
        </div>
      </aside>

      {/* Persona picker modal */}
      <PersonaPickerModal
        isOpen={isModalOpen}
        personas={personas}
        onConfirm={handleModalConfirm}
        onCancel={handleModalCancel}
        isLoading={isCreating}
      />
    </>
  );
}

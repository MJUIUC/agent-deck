import { useState, useMemo, useCallback, useEffect } from "react";
import type { Thread, AgentPersona } from "@/types";
import { ThreadItem } from "./ThreadItem";
import { PersonaPickerModal } from "./PersonaPickerModal";
import { Plus, Settings, Archive } from "lucide-react";

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

  const handleNewChat = useCallback(() => setIsModalOpen(true), []);

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
        className={[
          "w-[260px] min-w-[260px] bg-bg-secondary border-r border-border-subtle",
          "flex flex-col overflow-hidden",
          // Mobile: fixed, slides in from left
          "max-sm:fixed max-sm:inset-y-0 max-sm:left-0 max-sm:z-50",
          "max-sm:shadow-[4px_0_24px_rgba(0,0,0,0.4)]",
          "max-sm:transition-transform max-sm:duration-250",
          isMobileOpen ? "max-sm:translate-x-0" : "max-sm:-translate-x-full",
        ]
          .filter(Boolean)
          .join(" ")}
        aria-label="Thread list"
      >
        {/* ── Header ── */}
        <div className="px-3.5 pt-4 pb-3 border-b border-border-subtle shrink-0">
          {/* Brand row */}
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <span className="text-lg">🤖</span>
              <span className="text-[15px] font-semibold text-text-primary tracking-tight">
                agent-deck
              </span>
            </div>
            {/* Mobile close button */}
            {onMobileClose && (
              <button
                onClick={onMobileClose}
                aria-label="Close sidebar"
                className="hidden max-sm:flex items-center justify-center w-7 h-7 rounded text-text-tertiary hover:text-text-primary hover:bg-bg-elevated transition-colors"
              >
                ✕
              </button>
            )}
          </div>

          {/* New Chat button */}
          <button
            className="w-full flex items-center justify-center gap-1.5 px-3 py-2 bg-accent-primary hover:bg-accent-secondary text-text-inverse text-[13px] font-semibold rounded-[7px] transition-colors disabled:opacity-60 disabled:cursor-default"
            onClick={handleNewChat}
            disabled={isCreating}
          >
            <Plus size={14} strokeWidth={2.5} />
            {isCreating ? "Creating…" : "New Chat"}
          </button>
        </div>

        {/* ── Search ── */}
        <div className="px-3.5 py-2.5 border-b border-border-subtle shrink-0">
          <input
            className="search-input w-full bg-bg-tertiary border border-border-subtle rounded-[6px] py-1.5 pr-2.5 pl-[30px] text-text-primary text-[13px] outline-none placeholder:text-text-tertiary focus:border-border-default transition-colors"
            type="text"
            placeholder="Search threads…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            aria-label="Search threads"
          />
        </div>

        {/* ── Thread list ── */}
        <div className="flex-1 overflow-y-auto py-1.5 scrollbar-thin">
          {isLoading ? (
            <div className="py-8 text-center text-text-tertiary text-[13px]">
              Loading…
            </div>
          ) : threads.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 px-5 py-10 text-center">
              <div className="text-3xl opacity-30 mb-1">💬</div>
              <div className="text-[13px] font-semibold text-text-secondary">
                No threads yet
              </div>
              <div className="text-[12px] text-text-tertiary leading-snug max-w-[180px]">
                Start a new chat to begin a conversation with your agent.
              </div>
            </div>
          ) : filteredThreads.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 px-5 py-10 text-center">
              <div className="text-2xl opacity-30 mb-1">🔍</div>
              <div className="text-[13px] font-semibold text-text-secondary">
                No results
              </div>
              <div className="text-[12px] text-text-tertiary leading-snug">
                Try a different search term.
              </div>
            </div>
          ) : (
            groupedThreads.map(({ label, threads: groupThreads }) => (
              <div key={label}>
                <div className="px-3.5 pt-2 pb-1 text-[10px] font-semibold text-text-tertiary uppercase tracking-[0.08em]">
                  {label}
                </div>
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
        <div className="px-3.5 py-2.5 border-t border-border-subtle flex gap-2 shrink-0">
          <button
            className="flex-1 flex items-center justify-center gap-1.5 px-2.5 py-1.5 bg-transparent border border-border-subtle rounded-[6px] text-text-secondary text-[12px] cursor-pointer hover:bg-bg-elevated hover:text-text-primary transition-colors"
            onClick={onOpenSettings}
          >
            <Settings size={13} />
            Settings
          </button>
          <button className="flex-1 flex items-center justify-center gap-1.5 px-2.5 py-1.5 bg-transparent border border-border-subtle rounded-[6px] text-text-secondary text-[12px] cursor-pointer hover:bg-bg-elevated hover:text-text-primary transition-colors">
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

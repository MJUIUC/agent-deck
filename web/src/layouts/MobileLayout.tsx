// ─── MobileLayout.tsx ─────────────────────────────────────────────────────────
// Full-screen mobile layout. Bootstraps state, wires stores, and renders one
// view at a time (threads / chat / settings) with a sticky bottom tab bar.
// ─────────────────────────────────────────────────────────────────────────────

import { useEffect, useState, useCallback } from "react";
import { useThreadStore, makeDraftThread } from "@/stores/useThreadStore";
import { useSseStore } from "@/stores/useSseStore";
import { useMessageStore } from "@/stores/useMessageStore";
import { providersApi } from "@/api/client";
import { PersonaPickerModal } from "@/components/PersonaPickerModal";
import { MobileThreadList } from "./mobile/MobileThreadList";
import { MobileChatView } from "./mobile/MobileChatView";
import { MobileSettings } from "./mobile/MobileSettings";
import styles from "./MobileLayout.module.css";

// ─── Tab type ────────────────────────────────────────────────────────────────

type ActiveTab = "threads" | "settings";

// ─── Icon sub-components ──────────────────────────────────────────────────────

function ThreadsIcon() {
  return (
    <svg
      width="22"
      height="22"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <line x1="3" y1="6" x2="21" y2="6" />
      <line x1="3" y1="12" x2="21" y2="12" />
      <line x1="3" y1="18" x2="21" y2="18" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg
      width="22"
      height="22"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

// ─── Component ────────────────────────────────────────────────────────────────

export function MobileLayout() {
  // ── Local state ─────────────────────────────────────────────────────────────
  const [activeTab, setActiveTab] = useState<ActiveTab>("threads");
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const [_hasProviders, setHasProviders] = useState(true); // optimistic default; kept for future empty-state logic
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const [_isCheckingProviders, setIsCheckingProviders] = useState(true);
  const [personaPickerOpen, setPersonaPickerOpen] = useState(false);

  // ── Thread store ─────────────────────────────────────────────────────────────
  const threads = useThreadStore((s) => s.threads);
  const personas = useThreadStore((s) => s.personas);
  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const pendingPersona = useThreadStore((s) => s.pendingPersona);
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const _isLoading = useThreadStore((s) => s.isLoading);
  const isCreating = useThreadStore((s) => s.isCreating);
  const loadThreads = useThreadStore((s) => s.loadThreads);
  const setActiveThread = useThreadStore((s) => s.setActiveThread);
  const createThread = useThreadStore((s) => s.createThread);
  const setPendingPersona = useThreadStore((s) => s.setPendingPersona);
  const promotePendingThread = useThreadStore((s) => s.promotePendingThread);

  // ── Message store ────────────────────────────────────────────────────────────
  const sendMessage = useMessageStore((s) => s.sendMessage);

  // ── SSE store ────────────────────────────────────────────────────────────────
  const connectGlobal = useSseStore((s) => s.connectGlobal);
  const disconnectGlobal = useSseStore((s) => s.disconnectGlobal);

  // ── Derived values ───────────────────────────────────────────────────────────
  const activeThread = threads.find((t) => t.id === activeThreadId) ?? null;
  const draftThread = pendingPersona ? makeDraftThread(pendingPersona) : null;
  const currentThread = draftThread ?? activeThread;

  // ── Bootstrap ────────────────────────────────────────────────────────────────
  useEffect(() => {
    loadThreads();

    providersApi
      .list()
      .then((res) => setHasProviders(res.data.some((p) => p.enabled !== false)))
      .catch(() => setHasProviders(false))
      .finally(() => setIsCheckingProviders(false));

    connectGlobal();

    // Deep-link from notification tap (cold start)
    const searchParams = new URLSearchParams(window.location.search);
    const deepLinkThreadId = searchParams.get("thread");
    if (deepLinkThreadId) {
      setActiveThread(deepLinkThreadId);
      // Clean up the URL so it doesn't persist across navigation
      window.history.replaceState({}, "", "/");
    }

    function handleSwMessage(event: MessageEvent) {
      if (event.data?.type === "OPEN_THREAD" && event.data.threadId) {
        setActiveThread(event.data.threadId);
        setActiveTab("threads");
      }
    }

    navigator.serviceWorker?.addEventListener("message", handleSwMessage);

    return () => {
      disconnectGlobal();
      navigator.serviceWorker?.removeEventListener("message", handleSwMessage);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Thread selection ─────────────────────────────────────────────────────────
  const handleSelectThread = useCallback(
    (threadId: string) => {
      setPendingPersona(null);
      setActiveThread(threadId);
    },
    [setPendingPersona, setActiveThread],
  );

  // ── Thread creation ──────────────────────────────────────────────────────────
  // If exactly 1 persona: immediately set pending and switch to chat.
  // If 0 or many personas: open the PersonaPickerModal.
  const handleCreateThread = useCallback(() => {
    if (personas.length === 1) {
      setPendingPersona(personas[0]);
    } else {
      setPersonaPickerOpen(true);
    }
  }, [personas, setPendingPersona]);

  // ── Persona picker confirm ───────────────────────────────────────────────────
  const handlePersonaPickerConfirm = useCallback(
    (personaId: string) => {
      const persona = personas.find((p) => p.id === personaId);
      if (!persona) return;
      setPersonaPickerOpen(false);
      setPendingPersona(persona);
    },
    [personas, setPendingPersona],
  );

  // ── Persona picker cancel ────────────────────────────────────────────────────
  const handlePersonaPickerCancel = useCallback(() => {
    setPersonaPickerOpen(false);
  }, []);

  // ── First-send handler ───────────────────────────────────────────────────────
  // Creates the real thread, promotes it, then sends the first message.
  const handleFirstSend = useCallback(
    async (content: string) => {
      if (!pendingPersona) return;
      const newThread = await createThread(pendingPersona.id);
      promotePendingThread(newThread);
      await Promise.resolve();
      await sendMessage(newThread.id, content);
    },
    [pendingPersona, createThread, promotePendingThread, sendMessage],
  );

  // ── Tab helpers ──────────────────────────────────────────────────────────────
  const handleBack = useCallback(() => {
    setPendingPersona(null);
    setActiveThread(null);
  }, [setPendingPersona, setActiveThread]);

  const tabClass = (tab: ActiveTab) =>
    [styles.tabItem, activeTab === tab ? styles.tabItemActive : ""]
      .filter(Boolean)
      .join(" ");

  // ── Render ───────────────────────────────────────────────────────────────────
  return (
    <div className={styles.mobileLayout}>
      {/* ── Main content area ── */}
      <div className={styles.content}>
        {activeTab === "threads" && currentThread && (
          <MobileChatView
            thread={currentThread}
            onBack={handleBack}
            onFirstSend={handleFirstSend}
          />
        )}

        {activeTab === "threads" && !currentThread && (
          <MobileThreadList
            onSelectThread={handleSelectThread}
            onCreateThread={handleCreateThread}
          />
        )}

        {activeTab === "settings" && <MobileSettings />}
      </div>

      {/* ── Bottom tab bar ── */}
      <nav className={styles.tabBar} aria-label="Main navigation">
        {/* Threads tab */}
        <button
          className={tabClass("threads")}
          type="button"
          aria-label="Threads"
          aria-current={activeTab === "threads" ? "page" : undefined}
          onClick={() => setActiveTab("threads")}
        >
          <ThreadsIcon />
          <span className={styles.tabLabel}>Threads</span>
          {activeTab === "threads" && (
            <span className={styles.tabDot} aria-hidden="true" />
          )}
        </button>

        {/* Settings tab */}
        <button
          className={tabClass("settings")}
          type="button"
          aria-label="Settings"
          aria-current={activeTab === "settings" ? "page" : undefined}
          onClick={() => setActiveTab("settings")}
        >
          <SettingsIcon />
          <span className={styles.tabLabel}>Settings</span>
          {activeTab === "settings" && (
            <span className={styles.tabDot} aria-hidden="true" />
          )}
        </button>
      </nav>

      {/* ── Persona picker modal ── */}
      <PersonaPickerModal
        isOpen={personaPickerOpen}
        personas={personas}
        onConfirm={handlePersonaPickerConfirm}
        onCancel={handlePersonaPickerCancel}
        isLoading={isCreating}
      />
    </div>
  );
}

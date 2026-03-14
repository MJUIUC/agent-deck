import { useEffect, useState, useCallback } from "react";
import { useThreadStore } from "@/stores/useThreadStore";
import { makeDraftThread } from "@/stores/useThreadStore";
import { useSseStore } from "@/stores/useSseStore";
import { useMessageStore } from "@/stores/useMessageStore";
import { providersApi, setupApi } from "@/api/client";
import { Sidebar } from "@/components/Sidebar";
import { ChatView } from "@/components/ChatView";
import { EmptyState } from "@/components/EmptyState";
import { SettingsModal } from "@/components/SettingsModal";
import { SetupWizard } from "@/components/wizards/setup-wizard/SetupWizard";
import styles from "@/App.module.css";

// ── Mobile sidebar state ──────────────────────────────────────────────────────
// On small viewports (< 640px) the sidebar is hidden by default and slides in
// via a hamburger button. We track open/close in App so both the sidebar and
// the overlay backdrop can react to it.

export function App() {
  // null = not yet checked, false = incomplete, true = complete
  const [setupComplete, setSetupComplete] = useState<boolean | null>(null);
  const [hasProviders, setHasProviders] = useState(true); // optimistic default
  const [isCheckingProviders, setIsCheckingProviders] = useState(true);
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<"providers" | "personas">(
    "providers",
  );

  // Thread store
  const threads = useThreadStore((s) => s.threads);
  const personas = useThreadStore((s) => s.personas);
  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const pendingPersona = useThreadStore((s) => s.pendingPersona);
  const isLoading = useThreadStore((s) => s.isLoading);
  const isCreating = useThreadStore((s) => s.isCreating);
  const loadThreads = useThreadStore((s) => s.loadThreads);
  const setActiveThread = useThreadStore((s) => s.setActiveThread);
  const createThread = useThreadStore((s) => s.createThread);
  const setPendingPersona = useThreadStore((s) => s.setPendingPersona);
  const promotePendingThread = useThreadStore((s) => s.promotePendingThread);

  // Message store — needed to send the first message in the draft flow
  const sendMessage = useMessageStore((s) => s.sendMessage);

  // SSE store
  const connectGlobal = useSseStore((s) => s.connectGlobal);
  const disconnectGlobal = useSseStore((s) => s.disconnectGlobal);

  // Active thread object (derived)
  const activeThread = threads.find((t) => t.id === activeThreadId) ?? null;

  // Draft thread object — only exists when the user clicked "+ New Chat" but
  // hasn't sent a message yet. Never stored in the threads array.
  const draftThread = pendingPersona ? makeDraftThread(pendingPersona) : null;

  // ── Bootstrap ─────────────────────────────────────────────────────────────

  useEffect(() => {
    // Check setup status FIRST — before anything else
    (async () => {
      try {
        const res = await setupApi.status();
        setSetupComplete(res.data.complete);

        // Only bootstrap the full app if setup is already complete
        if (res.data.complete) {
          bootApp();
        }
      } catch {
        // If status check fails, assume complete and proceed normally
        setSetupComplete(true);
        bootApp();
      }
    })();

    return () => {
      disconnectGlobal();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const bootApp = useCallback(() => {
    // Load threads + personas together
    loadThreads();

    // Check for configured providers
    (async () => {
      try {
        const res = await providersApi.list();
        setHasProviders(res.data.some((p) => p.enabled !== false));
      } catch {
        setHasProviders(false);
      } finally {
        setIsCheckingProviders(false);
      }
    })();

    // Connect global SSE stream (stays alive for the app lifetime)
    connectGlobal();
  }, [loadThreads, connectGlobal]);

  // Called by SetupWizard when the user completes setup
  const handleSetupComplete = useCallback(async () => {
    try {
      const res = await setupApi.status();
      setSetupComplete(res.data.complete);
      if (res.data.complete) {
        bootApp();
      }
    } catch {
      setSetupComplete(true);
      bootApp();
    }
  }, [bootApp]);

  // ── Thread creation ───────────────────────────────────────────────────────

  // Clicking "+ New Chat" no longer creates a DB record immediately.
  // Instead, we store the chosen persona as "pending" and render a draft
  // ChatView. The real thread is only created when the user sends their
  // first message (see handleFirstSend below).
  const handleCreateThread = useCallback(
    (personaId: string) => {
      const persona = personas.find((p) => p.id === personaId);
      if (!persona) return;
      setPendingPersona(persona);
    },
    [personas, setPendingPersona],
  );

  // Called by the draft ChatView when the user sends their first message.
  // Creates the real thread, sends the message, then promotes the thread as
  // active. Title generation is driven server-side via the TitleUpdated SSE
  // event — no client-side coordination needed here.
  const handleFirstSend = useCallback(
    async (content: string) => {
      if (!pendingPersona) return;
      try {
        // 1. Create the real thread in the DB.
        const newThread = await createThread(pendingPersona.id);

        // 2. Send the user message so it's persisted and the agent starts
        //    streaming before we promote. This way the SSE connection that
        //    ChatView opens on mount will arrive while the stream is still
        //    in flight — the falling-edge detector in ChatView is guaranteed
        //    to see isStreaming go true → false.
        await sendMessage(newThread.id, content);

        // 3. Promote the thread — ChatView will mount and connect SSE.
        //    Title generation is now driven server-side via TitleUpdated SSE.
        promotePendingThread(newThread);
      } catch {
        // createThread or sendMessage failure — keep pendingPersona so the
        // user can retry.
      }
    },
    [pendingPersona, createThread, promotePendingThread, sendMessage],
  );

  // ── Mobile sidebar ────────────────────────────────────────────────────────
  const handleMobileMenuOpen = useCallback(() => {
    setMobileSidebarOpen(true);
  }, []);

  const handleMobileMenuClose = useCallback(() => {
    setMobileSidebarOpen(false);
  }, []);

  // Close mobile sidebar automatically when a thread is selected
  const handleSelectThreadMobile = useCallback(
    (threadId: string) => {
      // Navigating away from a draft discards it silently — no DB record was
      // created so there is nothing to clean up.
      setPendingPersona(null);
      setActiveThread(threadId);
      setMobileSidebarOpen(false);
    },
    [setActiveThread, setPendingPersona],
  );

  // ── Settings ──────────────────────────────────────────────────────────────
  const handleOpenSettings = useCallback(
    (tab: "providers" | "personas" = "providers") => {
      setSettingsTab(tab);
      setSettingsOpen(true);
    },
    [],
  );

  const handleCloseSettings = useCallback(() => {
    setSettingsOpen(false);
  }, []);

  // Re-check providers/personas whenever the settings modal reports a change
  const handleSettingsDataChanged = useCallback(async () => {
    try {
      const res = await providersApi.list();
      setHasProviders(res.data.some((p) => p.enabled !== false));
    } catch {
      setHasProviders(false);
    }
    loadThreads(); // reload threads so persona updates propagate
  }, [loadThreads]);

  // ── Render ────────────────────────────────────────────────────────────────

  // Setup status not yet known — show nothing to avoid flash
  if (setupComplete === null) {
    return <div className={styles.loadingScreen}>Loading…</div>;
  }

  // Setup is incomplete — render the wizard as the full page
  if (setupComplete === false) {
    return <SetupWizard onComplete={handleSetupComplete} />;
  }

  const hasPersonas = personas.length > 0;

  return (
    <div className={styles.app}>
      {/* ── Mobile backdrop ── */}
      {mobileSidebarOpen && (
        <div
          onClick={handleMobileMenuClose}
          className={styles.mobileBackdrop}
          aria-hidden="true"
        />
      )}

      {/* ── Sidebar ── */}
      <Sidebar
        threads={threads}
        personas={personas}
        // While a draft is open the sidebar shows no active thread highlight —
        // the draft is not in the threads array.
        activeThreadId={draftThread ? null : activeThreadId}
        isLoading={isLoading}
        isCreating={isCreating}
        isMobileOpen={mobileSidebarOpen}
        onSelectThread={handleSelectThreadMobile}
        onCreateThread={handleCreateThread}
        onOpenSettings={() => handleOpenSettings("providers")}
        onMobileClose={handleMobileMenuClose}
      />

      {/* ── Settings modal ── */}
      <SettingsModal
        isOpen={settingsOpen}
        onClose={handleCloseSettings}
        initialTab={settingsTab}
        onDataChanged={handleSettingsDataChanged}
      />

      {/* ── Main area ── */}
      {draftThread ? (
        // Draft mode: thread is a synthetic object, never persisted.
        // onFirstSend creates the real thread and transitions out of draft mode.
        <ChatView
          thread={draftThread}
          onFirstSend={handleFirstSend}
          onMobileMenuOpen={handleMobileMenuOpen}
        />
      ) : activeThread ? (
        <ChatView
          thread={activeThread}
          onMobileMenuOpen={handleMobileMenuOpen}
        />
      ) : isCheckingProviders ? (
        <div className={styles.loadingScreen}>Loading…</div>
      ) : (
        <EmptyState
          hasThreads={threads.length > 0}
          hasPersonas={hasPersonas}
          hasProviders={hasProviders}
          onNewChat={() => {
            if (personas.length === 1) {
              handleCreateThread(personas[0].id);
            } else {
              window.dispatchEvent(new CustomEvent("agent-deck:new-chat"));
            }
          }}
          onMobileMenuOpen={handleMobileMenuOpen}
          onOpenSettings={handleOpenSettings}
          onOpenProviders={() => handleOpenSettings("providers")}
          onOpenPersonas={() => handleOpenSettings("personas")}
        />
      )}
    </div>
  );
}

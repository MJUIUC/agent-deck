import { useEffect, useState, useCallback } from "react";
import { useThreadStore } from "@/stores/useThreadStore";
import { useSseStore } from "@/stores/useSseStore";
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
  const isLoading = useThreadStore((s) => s.isLoading);
  const isCreating = useThreadStore((s) => s.isCreating);
  const loadThreads = useThreadStore((s) => s.loadThreads);
  const setActiveThread = useThreadStore((s) => s.setActiveThread);
  const createThread = useThreadStore((s) => s.createThread);

  // SSE store
  const connectGlobal = useSseStore((s) => s.connectGlobal);
  const disconnectGlobal = useSseStore((s) => s.disconnectGlobal);

  // Active thread object (derived)
  const activeThread = threads.find((t) => t.id === activeThreadId) ?? null;

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

  const handleCreateThread = useCallback(
    async (personaId: string) => {
      try {
        const newThread = await createThread(personaId);
        setActiveThread(newThread.id);
      } catch {
        // Error is stored in the thread store — sidebar will show it
      }
    },
    [createThread, setActiveThread],
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
      setActiveThread(threadId);
      setMobileSidebarOpen(false);
    },
    [setActiveThread],
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
        activeThreadId={activeThreadId}
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
      {activeThread ? (
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

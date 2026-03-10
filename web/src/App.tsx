import { useEffect, useState, useCallback } from "react";
import { useThreadStore } from "@/stores/useThreadStore";
import { useSseStore } from "@/stores/useSseStore";
import { providersApi } from "@/api/client";
import { Sidebar } from "@/components/Sidebar";
import { ChatView } from "@/components/ChatView";
import { EmptyState } from "@/components/EmptyState";

// ── Mobile sidebar state ──────────────────────────────────────────────────────
// On small viewports (< 640px) the sidebar is hidden by default and slides in
// via a hamburger button. We track open/close in App so both the sidebar and
// the overlay backdrop can react to it.

export function App() {
  const [hasProviders, setHasProviders] = useState(true); // optimistic default
  const [isCheckingProviders, setIsCheckingProviders] = useState(true);
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false);

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

    return () => {
      disconnectGlobal();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Thread selection ──────────────────────────────────────────────────────

  const handleSelectThread = useCallback(
    (threadId: string) => {
      setActiveThread(threadId);
    },
    [setActiveThread],
  );

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

  // ── Settings placeholder ──────────────────────────────────────────────────
  // Phase 3 will implement a proper settings page/modal.
  // For now we just log so the button isn't dead.
  const handleOpenSettings = useCallback(() => {
    // TODO Phase 3: navigate to settings
    console.info("[agent-deck] Settings page coming in Phase 3");
  }, []);

  // ── Render ────────────────────────────────────────────────────────────────

  const hasPersonas = personas.length > 0;

  return (
    <div className="flex h-screen overflow-hidden bg-bg-primary text-text-primary">
      {/* ── Mobile backdrop ── */}
      {mobileSidebarOpen && (
        <div
          onClick={handleMobileMenuClose}
          className="fixed inset-0 bg-black/50 z-40 hidden max-sm:block"
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
        onOpenSettings={handleOpenSettings}
        onMobileClose={handleMobileMenuClose}
      />

      {/* ── Main area ── */}
      {activeThread ? (
        <ChatView
          thread={activeThread}
          onMobileMenuOpen={handleMobileMenuOpen}
        />
      ) : isCheckingProviders ? (
        <div className="flex-1 flex items-center justify-center text-text-tertiary text-[13px]">
          Loading…
        </div>
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
        />
      )}
    </div>
  );
}

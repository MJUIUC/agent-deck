import { Menu } from "lucide-react";
import styles from "./EmptyState.module.css";

interface EmptyStateProps {
  hasThreads: boolean;
  hasPersonas: boolean;
  hasProviders: boolean;
  onNewChat: () => void;
  onOpenSettings: () => void;
  onOpenProviders?: () => void;
  onOpenPersonas?: () => void;
  onMobileMenuOpen?: () => void;
}

function MobileMenuButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      aria-label="Open sidebar"
      className={styles.menuBtn}
    >
      <Menu size={18} />
    </button>
  );
}

export function EmptyState({
  hasThreads,
  hasPersonas,
  hasProviders,
  onNewChat,
  onOpenSettings,
  onOpenProviders,
  onOpenPersonas,
  onMobileMenuOpen,
}: EmptyStateProps) {
  // ── No provider ──────────────────────────────────────────────────────────
  if (!hasProviders) {
    return (
      <div className={styles.root}>
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className={styles.body}>
          <div className={styles.icon}>🔌</div>
          <div className={styles.title}>No provider connected</div>
          <div className={styles.desc}>
            Connect an AI provider (OpenAI, Anthropic, GitHub Copilot, or any
            OpenAI-compatible API) to start chatting.
          </div>
          {/* Warning banner */}
          <div className={styles.banner}>
            <span className={styles.bannerIcon}>⚠️</span>
            <div className={styles.bannerBody}>
              <div className={styles.bannerTitle}>Setup required</div>
              <div className={styles.bannerDesc}>
                Add a provider and at least one model to begin. You can also
                create agent personas to give your AI a consistent personality.
              </div>
              <button
                onClick={onOpenProviders ?? onOpenSettings}
                className={styles.btnPrimary}
              >
                Add a Provider
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── No personas ───────────────────────────────────────────────────────────
  if (!hasPersonas) {
    return (
      <div className={styles.root}>
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className={styles.body}>
          <div className={styles.icon}>🤖</div>
          <div className={styles.title}>No personas yet</div>
          <div className={styles.desc}>
            Create an agent persona to define your AI's name, personality, and
            default model before starting a chat.
          </div>
          <div className={styles.actions}>
            <button
              onClick={onOpenPersonas ?? onOpenSettings}
              className={styles.btnPrimary}
            >
              Create a persona
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── No thread selected (or no threads exist) ─────────────────────────────
  return (
    <div className={styles.root}>
      {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
      <div className={styles.body}>
        <div className={styles.icon}>💬</div>
        <div className={styles.title}>
          {hasThreads ? "Select a conversation" : "No conversations yet"}
        </div>
        <div className={styles.desc}>
          {hasThreads
            ? "Choose a thread from the sidebar, or start a new chat."
            : "Tap + New Chat to begin a conversation with your agent."}
        </div>
        <div className={styles.actions}>
          <button onClick={onNewChat} className={styles.btnPrimary}>
            ＋ New Chat
          </button>
        </div>
      </div>
    </div>
  );
}

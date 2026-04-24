import { Menu, CloudApp, Bot, Chat } from "@carbon/icons-react";
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
  // ── No provider, no personas ──────────────────────────────────────────────
  if (!hasProviders && !hasPersonas) {
    return (
      <div className={styles.root}>
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className={styles.body}>
          <div className={styles.icon}>
            <CloudApp size={36} />
          </div>
          <div className={styles.title}>
            You need a provider to get started.
          </div>
          <div className={styles.desc}>
            Connect an AI provider (OpenAI, Anthropic, GitHub Copilot, or any
            OpenAI-compatible API) to start chatting.
          </div>
          <div className={styles.actions}>
            <button
              onClick={onOpenProviders ?? onOpenSettings}
              className={styles.btnPrimary}
            >
              Add a Provider →
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── Provider exists, no personas ──────────────────────────────────────────
  if (hasProviders && !hasPersonas) {
    return (
      <div className={styles.root}>
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className={styles.body}>
          <div className={styles.icon}>
            <Bot size={36} />
          </div>
          <div className={styles.title}>Almost there.</div>
          <div className={styles.desc}>
            Create your first agent persona to define your AI's name,
            personality, and default model — then you're ready to chat.
          </div>
          <div className={styles.actions}>
            <button
              onClick={onOpenPersonas ?? onOpenSettings}
              className={styles.btnPrimary}
            >
              Create a Persona →
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── Personas exist, no provider ───────────────────────────────────────────
  if (!hasProviders && hasPersonas) {
    return (
      <div className={styles.root}>
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className={styles.body}>
          <div className={styles.icon}>
            <CloudApp size={36} />
          </div>
          <div className={styles.title}>No provider connected.</div>
          <div className={styles.desc}>
            You have personas ready to go, but no AI provider is connected yet.
            Add a provider and your personas will be available immediately.
          </div>
          <div className={styles.actions}>
            <button
              onClick={onOpenProviders ?? onOpenSettings}
              className={styles.btnPrimary}
            >
              Add a Provider →
            </button>
          </div>
          <div className={styles.hint}>
            Your existing personas will be available once a provider is
            connected.
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
        <div className={styles.icon}>
          <Chat size={36} />
        </div>
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

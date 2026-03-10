import { Menu } from "lucide-react";

interface EmptyStateProps {
  hasThreads: boolean;
  hasPersonas: boolean;
  hasProviders: boolean;
  onNewChat: () => void;
  onOpenSettings: () => void;
  onMobileMenuOpen?: () => void;
}

function MobileMenuButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      aria-label="Open sidebar"
      className="hidden max-sm:flex absolute top-3 left-3 items-center justify-center w-8 h-8 rounded-[6px] text-text-secondary hover:bg-bg-elevated hover:text-text-primary transition-colors"
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
  onMobileMenuOpen,
}: EmptyStateProps) {
  // ── No provider ──────────────────────────────────────────────────────────
  if (!hasProviders) {
    return (
      <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className="flex-1 flex flex-col items-center justify-center gap-5 px-10">
          <div className="text-[48px] opacity-[0.18]">🔌</div>
          <div className="text-[18px] font-bold text-text-secondary tracking-tight">
            No provider connected
          </div>
          <div className="text-[13px] text-text-tertiary text-center max-w-[340px] leading-relaxed">
            Connect an AI provider (OpenAI, Anthropic, GitHub Copilot, or any
            OpenAI-compatible API) to start chatting.
          </div>
          {/* Warning banner */}
          <div className="w-full max-w-[400px] flex gap-3 items-start bg-warning/[0.07] border border-warning/[0.28] rounded-[10px] px-5 py-4">
            <span className="text-[20px] shrink-0 mt-px">⚠️</span>
            <div className="min-w-0">
              <div className="text-[14px] font-semibold text-text-primary mb-1">
                Setup required
              </div>
              <div className="text-[12px] text-text-secondary leading-[1.55] mb-3">
                Add a provider and at least one model to begin. You can also
                create agent personas to give your AI a consistent personality.
              </div>
              <button
                onClick={onOpenSettings}
                className="px-3.5 py-[7px] bg-accent-primary hover:bg-accent-secondary text-text-inverse text-[12px] font-semibold rounded-[7px] transition-colors"
              >
                Open Settings
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
      <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
        {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
        <div className="flex-1 flex flex-col items-center justify-center gap-4 px-10">
          <div className="text-[48px] opacity-[0.18]">🤖</div>
          <div className="text-[18px] font-bold text-text-secondary tracking-tight">
            No personas yet
          </div>
          <div className="text-[13px] text-text-tertiary text-center max-w-[320px] leading-relaxed">
            Create an agent persona to define your AI's name, personality, and
            default model before starting a chat.
          </div>
          <div className="flex gap-2.5 mt-1.5">
            <button
              onClick={onOpenSettings}
              className="px-4 py-2 bg-accent-primary hover:bg-accent-secondary text-text-inverse text-[13px] font-medium rounded-[7px] transition-colors"
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
    <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
      {onMobileMenuOpen && <MobileMenuButton onClick={onMobileMenuOpen} />}
      <div className="flex-1 flex flex-col items-center justify-center gap-3.5 px-10">
        <div className="text-[48px] opacity-[0.18]">💬</div>
        <div className="text-[18px] font-bold text-text-secondary tracking-tight">
          {hasThreads ? "Select a conversation" : "No conversations yet"}
        </div>
        <div className="text-[13px] text-text-tertiary text-center max-w-[300px] leading-relaxed">
          {hasThreads
            ? "Choose a thread from the sidebar, or start a new chat."
            : "Tap + New Chat to begin a conversation with your agent."}
        </div>
        <div className="flex gap-2.5 mt-1.5">
          <button
            onClick={onNewChat}
            className="px-4 py-2 bg-accent-primary hover:bg-accent-secondary text-text-inverse text-[13px] font-medium rounded-[7px] transition-colors"
          >
            ＋ New Chat
          </button>
        </div>
      </div>
    </div>
  );
}

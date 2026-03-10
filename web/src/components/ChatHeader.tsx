import type { Thread } from "@/types";
import { Settings, Menu } from "lucide-react";

interface ChatHeaderProps {
  thread: Thread;
  onToggleConfig: () => void;
  onMobileMenuOpen?: () => void;
}

export function ChatHeader({
  thread,
  onToggleConfig,
  onMobileMenuOpen,
}: ChatHeaderProps) {
  const persona = thread.persona;
  const emoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

  const parts: string[] = [personaName];
  if (thread.active_model) parts.push(thread.active_model);
  if (thread.active_provider) parts.push(thread.active_provider);
  const subtitle = parts.join(" · ");

  return (
    <div className="flex items-center justify-between px-[18px] py-3 border-b border-border-subtle bg-bg-primary shrink-0">
      {/* Left: optional hamburger + avatar + info */}
      <div className="flex items-center gap-2.5 min-w-0">
        {onMobileMenuOpen && (
          <button
            onClick={onMobileMenuOpen}
            aria-label="Open sidebar"
            className="hidden max-sm:flex items-center justify-center w-8 h-8 rounded-[6px] text-text-secondary hover:bg-bg-elevated hover:text-text-primary transition-colors shrink-0"
          >
            <Menu size={18} />
          </button>
        )}

        <div className="w-8 h-8 rounded-full bg-bg-elevated flex items-center justify-center text-base shrink-0">
          {emoji}
        </div>

        <div className="min-w-0">
          <div className="text-[14px] font-semibold text-text-primary truncate leading-tight">
            {thread.title}
          </div>
          <div className="text-[11px] text-text-tertiary truncate leading-tight mt-px">
            {subtitle}
          </div>
        </div>
      </div>

      {/* Right: action buttons */}
      <div className="flex items-center gap-1.5 shrink-0">
        <button
          onClick={onToggleConfig}
          aria-label="Thread settings"
          title="Thread settings"
          className="w-8 h-8 rounded-[6px] flex items-center justify-center text-text-secondary hover:bg-bg-elevated hover:text-text-primary transition-colors"
        >
          <Settings size={16} />
        </button>
      </div>
    </div>
  );
}

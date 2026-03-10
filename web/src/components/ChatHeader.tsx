import type { Thread } from "@/types";
import { Settings, Menu } from "lucide-react";
import styles from "./ChatHeader.module.css";

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
    <div className={styles.header}>
      {/* Left: optional hamburger + avatar + info */}
      <div className={styles.left}>
        {onMobileMenuOpen && (
          <button
            onClick={onMobileMenuOpen}
            aria-label="Open sidebar"
            className={styles.menuBtn}
          >
            <Menu size={18} />
          </button>
        )}

        <div className={styles.avatar}>{emoji}</div>

        <div className={styles.meta}>
          <div className={styles.title}>{thread.title}</div>
          <div className={styles.subtitle}>{subtitle}</div>
        </div>
      </div>

      {/* Right: action buttons */}
      <div className={styles.right}>
        <button
          onClick={onToggleConfig}
          aria-label="Thread settings"
          title="Thread settings"
          className={styles.iconBtn}
        >
          <Settings size={16} />
        </button>
      </div>
    </div>
  );
}

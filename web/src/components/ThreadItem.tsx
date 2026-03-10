import type { Thread } from "@/types";
import { formatThreadTime } from "@/hooks/useTimeFormat";

interface ThreadItemProps {
  thread: Thread;
  isActive: boolean;
  onClick: (threadId: string) => void;
}

export function ThreadItem({ thread, isActive, onClick }: ThreadItemProps) {
  const persona = thread.persona;
  const emoji = persona?.emoji ?? "🤖";
  const timeStr = formatThreadTime(thread.updated_at);
  const preview = thread.last_message_preview ?? "";

  return (
    <div
      className={[
        "relative flex items-start gap-[9px] px-3.5 py-[9px] cursor-pointer outline-none",
        "transition-colors duration-100",
        isActive
          ? "bg-accent-muted thread-active-bar"
          : "hover:bg-bg-tertiary focus-visible:bg-bg-tertiary",
      ].join(" ")}
      onClick={() => onClick(thread.id)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick(thread.id);
        }
      }}
    >
      {/* Avatar */}
      <div className="w-7 h-7 rounded-full bg-bg-elevated flex items-center justify-center text-[14px] shrink-0 mt-px">
        {emoji}
      </div>

      {/* Info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center justify-between gap-1">
          <span className="text-[13px] font-medium text-text-primary truncate min-w-0">
            {thread.title}
          </span>
          <span className="text-[11px] text-text-tertiary shrink-0">
            {timeStr}
          </span>
        </div>
        {preview && (
          <div className="text-[12px] text-text-tertiary truncate mt-px">
            {preview}
          </div>
        )}
      </div>
    </div>
  );
}

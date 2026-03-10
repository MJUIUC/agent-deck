import type { Message } from "@/types";
import { formatMessageTime } from "@/hooks/useTimeFormat";
import { StreamingIndicator } from "./StreamingIndicator";

interface MessageBubbleProps {
  message: Message;
  personaEmoji?: string;
  personaName?: string;
}

function UserAvatar() {
  return (
    <div className="w-7 h-7 rounded-full bg-accent-muted flex items-center justify-center text-[12px] font-semibold text-text-primary shrink-0 mt-0.5">
      M
    </div>
  );
}

function AgentAvatar({ emoji }: { emoji: string }) {
  return (
    <div className="w-7 h-7 rounded-full bg-bg-elevated flex items-center justify-center text-[13px] shrink-0 mt-0.5">
      {emoji}
    </div>
  );
}

function RoutineAvatar() {
  return (
    <div className="w-7 h-7 rounded-full bg-bubble-routine border border-[#3a4a60] flex items-center justify-center text-[13px] shrink-0 mt-0.5">
      ⚡
    </div>
  );
}

export function MessageBubble({
  message,
  personaEmoji = "🤖",
  personaName = "Agent",
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isRoutine = message.source === "routine";

  const timeStr = formatMessageTime(message.created_at);
  const metaText = isUser
    ? `You · ${timeStr}`
    : isRoutine
      ? `Routine · ${timeStr}`
      : `${personaName} · ${timeStr}`;

  const bubbleClasses = isUser
    ? "bg-bubble-user rounded-[12px] rounded-br-[4px]"
    : isRoutine
      ? "bg-bubble-routine border border-[#3a4a60] rounded-[12px] rounded-bl-[4px]"
      : "bg-bubble-agent border border-border-subtle rounded-[12px] rounded-bl-[4px]";

  return (
    <div
      className={[
        "flex gap-2.5 px-[18px] py-[3px]",
        isUser ? "flex-row-reverse" : "flex-row",
      ].join(" ")}
    >
      {isUser ? (
        <UserAvatar />
      ) : isRoutine ? (
        <RoutineAvatar />
      ) : (
        <AgentAvatar emoji={personaEmoji} />
      )}

      <div className={isUser ? "flex flex-col items-end" : ""}>
        <div
          className={[
            "max-w-[68%] px-[13px] py-[9px] text-[13.5px] leading-[1.55] text-text-primary break-words whitespace-pre-wrap",
            bubbleClasses,
          ].join(" ")}
        >
          {isRoutine && (
            <div className="flex items-center gap-1 text-[10px] font-semibold text-info uppercase tracking-[0.06em] mb-1.5">
              ⚡ Routine
            </div>
          )}
          {message.content}
        </div>
        <div
          className={[
            "text-[11px] text-text-tertiary mt-[3px] px-0.5",
            isUser ? "text-right" : "",
          ].join(" ")}
        >
          {metaText}
        </div>
      </div>
    </div>
  );
}

// Streaming bubble — shown while the assistant is generating tokens
interface StreamingBubbleProps {
  personaEmoji?: string;
  personaName?: string;
  content: string;
}

export function StreamingBubble({
  personaEmoji = "🤖",
  personaName = "Agent",
  content,
}: StreamingBubbleProps) {
  return (
    <div className="flex flex-row gap-2.5 px-[18px] py-[3px]">
      <AgentAvatar emoji={personaEmoji} />
      <div>
        <div className="max-w-[68%] px-[13px] py-[9px] text-[13.5px] leading-[1.55] text-text-primary break-words whitespace-pre-wrap bg-bubble-agent border border-border-subtle rounded-[12px] rounded-bl-[4px]">
          {content && <span>{content}</span>}
          <StreamingIndicator />
        </div>
        <div className="text-[11px] text-text-tertiary mt-[3px] px-0.5">
          {personaName} · now
        </div>
      </div>
    </div>
  );
}

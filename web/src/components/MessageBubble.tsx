import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Message } from "@/types";
import { formatMessageTime } from "@/hooks/useTimeFormat";
import { StreamingIndicator } from "./StreamingIndicator";
import styles from "./MessageBubble.module.css";

interface MessageBubbleProps {
  message: Message;
  personaEmoji?: string;
  personaName?: string;
}

function UserAvatar() {
  return <div className={`${styles.avatar} ${styles.avatarUser}`}>M</div>;
}

function AgentAvatar({
  emoji,
  className,
}: {
  emoji: string;
  className?: string;
}) {
  return (
    <div
      className={`${styles.avatar} ${styles.avatarAgent} ${className ?? ""}`}
    >
      {emoji}
    </div>
  );
}

export function ToolActivityBubble({ message }: { message: Message }) {
  const isCall = message.role === "assistant";
  const timeStr = formatMessageTime(message.created_at);

  return (
    <div className={styles.toolRow}>
      <div className={styles.toolBubble}>
        <div className={styles.toolLabel}>
          {isCall ? "⚙ Tool call" : "⚙ Tool result"}
        </div>
        <div className={styles.toolContent}>
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {message.content}
          </ReactMarkdown>
        </div>
        <div className={styles.toolMeta}>{timeStr}</div>
      </div>
    </div>
  );
}

export function MessageBubble({
  message,
  personaEmoji = "🤖",
  personaName = "Agent",
}: MessageBubbleProps) {
  if (message.source === "tool") {
    return <ToolActivityBubble message={message} />;
  }

  const isUser = message.role === "user";
  const isRoutine = message.source === "routine";

  const timeStr = formatMessageTime(message.created_at);
  const metaText = isUser
    ? `You · ${timeStr}`
    : isRoutine
      ? `Routine · ${timeStr}`
      : `${personaName} · ${timeStr}`;

  const bubbleClass = isUser
    ? styles.bubbleUser
    : isRoutine
      ? styles.bubbleRoutine
      : styles.bubbleAgent;

  return (
    <div
      className={[styles.row, isUser ? styles.rowUser : styles.rowAgent].join(
        " ",
      )}
    >
      {isUser ? (
        <UserAvatar />
      ) : isRoutine ? (
        <AgentAvatar emoji={personaEmoji} className={styles.avatarRoutine} />
      ) : (
        <AgentAvatar emoji={personaEmoji} />
      )}

      <div className={[styles.col, isUser ? styles.colUser : ""].join(" ")}>
        <div className={`${styles.bubble} ${bubbleClass}`}>
          {isRoutine && <div className={styles.routineLabel}>⚡ Routine</div>}
          {message.stopped && (
            <div className={styles.stoppedLabel}>⏹ Stopped</div>
          )}
          {isUser ? (
            message.content
          ) : (
            <div className={styles.markdown}>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {message.content}
              </ReactMarkdown>
            </div>
          )}
        </div>
        <div className={[styles.meta, isUser ? styles.metaUser : ""].join(" ")}>
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
    <div className={`${styles.row} ${styles.rowAgent}`}>
      <AgentAvatar emoji={personaEmoji} />
      <div className={styles.col}>
        <div className={`${styles.bubble} ${styles.bubbleAgent}`}>
          {content && (
            <div className={styles.markdown}>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {content}
              </ReactMarkdown>
            </div>
          )}
          <StreamingIndicator />
        </div>
        <div className={styles.meta}>{personaName} · now</div>
      </div>
    </div>
  );
}

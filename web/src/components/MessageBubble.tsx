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

function AgentAvatar({ emoji }: { emoji: string }) {
  return (
    <div className={`${styles.avatar} ${styles.avatarAgent}`}>{emoji}</div>
  );
}

function RoutineAvatar() {
  return <div className={`${styles.avatar} ${styles.avatarRoutine}`}>⚡</div>;
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
        <RoutineAvatar />
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

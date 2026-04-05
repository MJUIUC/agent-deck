import { useState, useRef } from "react";
import { MagicWandFilled } from "@carbon/icons-react";
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

function CodeBlock({
  children,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  node: _node,
  ...props
}: React.HTMLAttributes<HTMLPreElement> & { node?: unknown }) {
  const [copied, setCopied] = useState(false);
  const preRef = useRef<HTMLPreElement>(null);

  const handleCopy = () => {
    const text = preRef.current?.innerText ?? "";
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div style={{ position: "relative" }}>
      <pre ref={preRef} {...props}>
        {children}
      </pre>
      <button
        onClick={handleCopy}
        style={{
          position: "absolute",
          top: "6px",
          right: "6px",
          padding: "3px 8px",
          fontSize: "0.7rem",
          fontFamily: "inherit",
          background: copied ? "var(--accent-primary)" : "var(--bg-elevated)",
          color: copied ? "#fff" : "var(--text-secondary)",
          border: "1px solid var(--border-subtle)",
          borderRadius: "4px",
          cursor: "pointer",
          opacity: 0.9,
          transition: "background 0.15s ease, color 0.15s ease",
          lineHeight: 1.4,
          userSelect: "none",
        }}
        aria-label="Copy code"
      >
        {copied ? "✓ Copied" : "Copy"}
      </button>
    </div>
  );
}

const TOOL_CONTENT_LIMIT = 4000;

export function ToolActivityBubble({ message }: { message: Message }) {
  const isCall = message.role === "assistant";
  const timeStr = formatMessageTime(message.created_at);

  const raw = message.content ?? "";
  const truncated = raw.length > TOOL_CONTENT_LIMIT;
  const displayContent = truncated
    ? raw.slice(0, TOOL_CONTENT_LIMIT) +
      "\n\n…(truncated — content too large to display)"
    : raw;

  return (
    <div className={styles.toolRow}>
      <div className={styles.toolBubble}>
        <div className={styles.toolLabel}>
          {isCall ? "⚙ Tool call" : "⚙ Tool result"}
        </div>
        <div className={styles.toolContent}>
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{ pre: CodeBlock }}
          >
            {displayContent}
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
          {isRoutine && (
            <div className={styles.routineLabel}>
              <MagicWandFilled size={10} /> Routine
            </div>
          )}
          {message.stopped && (
            <div className={styles.stoppedLabel}>⏹ Stopped</div>
          )}
          {isUser ? (
            message.content
          ) : (
            <div className={styles.markdown}>
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{ pre: CodeBlock }}
              >
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
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{ pre: CodeBlock }}
              >
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

import { useState, useRef, useId, useEffect, type ReactNode } from "react";
import remarkBreaks from "remark-breaks";
import {
  MagicWandFilled,
  StopFilled,
  Attachment,
  Document,
  Folder,
  CheckmarkFilled,
  Copy,
} from "@carbon/icons-react";
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
  onFilePath?: (path: string) => void;
}

function UserAvatar() {
  return <div className={`${styles.avatar} ${styles.avatarUser}`}>M</div>;
}

export function AgentAvatar({
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

function ScrollableTable({
  children,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  node: _node,
  ...props
}: React.HTMLAttributes<HTMLTableElement> & { node?: unknown }) {
  return (
    <div className={styles.tableWrapper}>
      <table {...props}>{children as ReactNode}</table>
    </div>
  );
}

export function MermaidBlock({ source }: { source: string }) {
  const id = useId().replace(/:/g, "mermaid-");
  const containerRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({ startOnLoad: false, theme: "neutral" });
        const { svg } = await mermaid.render(id, source);
        if (!cancelled && containerRef.current) {
          containerRef.current.innerHTML = svg;
        }
      } catch {
        if (!cancelled) setError(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [id, source]);

  if (error) {
    return <pre>{source}</pre>;
  }
  return <div ref={containerRef} />;
}

function PathChip({ path, onClick }: { path: string; onClick: () => void }) {
  const hasExtension = path.includes(".") && !path.endsWith("/");
  const Icon = hasExtension ? Document : Folder;
  const label = path.split("/").pop() ?? path;
  return (
    <button
      onClick={onClick}
      title={path}
      style={{
        fontFamily: "var(--font-mono)",
        fontSize: "0.8em",
        background: "var(--bg-elevated)",
        border: "1px solid var(--border-subtle)",
        borderRadius: "999px",
        padding: "1px 8px",
        cursor: "pointer",
        color: "var(--text-primary)",
        display: "inline-flex",
        alignItems: "center",
        gap: "4px",
        lineHeight: 1.5,
        verticalAlign: "middle",
      }}
    >
      <Icon size={12} /> {label}
    </button>
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
        {copied ? (
          <>
            <CheckmarkFilled size={12} /> Copied
          </>
        ) : (
          <>
            <Copy size={12} /> Copy
          </>
        )}
      </button>
    </div>
  );
}

function extractFsPath(href: string): string | null {
  try {
    // Match /api/fs/read?path=... and /api/fs/list?path=...
    const url = new URL(href, window.location.origin);
    if (
      (url.pathname === "/api/fs/read" || url.pathname === "/api/fs/list") &&
      url.searchParams.has("path")
    ) {
      return url.searchParams.get("path");
    }
  } catch {
    // href was not a parseable URL — not a file link
  }
  return null;
}

function markdownComponents(onFilePath?: (path: string) => void) {
  return {
    pre: CodeBlock,
    table: ScrollableTable,
    code({ className, children, ...props }: React.HTMLAttributes<HTMLElement>) {
      const language = /language-(\w+)/.exec(className ?? "")?.[1];
      if (language === "mermaid") {
        return <MermaidBlock source={String(children).replace(/\n$/, "")} />;
      }
      return (
        <code className={className} {...props}>
          {children}
        </code>
      );
    },
    a({ href, children }: React.AnchorHTMLAttributes<HTMLAnchorElement>) {
      const filePath = href ? extractFsPath(href) : null;
      if (filePath) {
        return (
          <PathChip path={filePath} onClick={() => onFilePath?.(filePath)} />
        );
      }
      return (
        <a href={href} target="_blank" rel="noopener noreferrer">
          {children}
        </a>
      );
    },
  };
}

export function MessageBubble({
  message,
  personaEmoji = "🤖",
  personaName = "Agent",
  onFilePath,
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isRoutine = message.source === "routine";

  const timeStr = formatMessageTime(message.created_at);
  const metaText = isUser ? `You · ${timeStr}` : `${personaName} · ${timeStr}`;

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
            <div className={styles.stoppedLabel}>
              <StopFilled size={12} /> Stopped
            </div>
          )}
          {/* Attachments */}
          {message.attachments && message.attachments.length > 0 && (
            <div
              style={{
                display: "flex",
                flexWrap: "wrap",
                gap: 6,
                marginBottom: 8,
              }}
            >
              {message.attachments.map((att, idx) => {
                const isImage = att.content_type.startsWith("image/");
                if (isImage) {
                  return (
                    <img
                      key={idx}
                      src={`/api/fs/image?path=${encodeURIComponent(att.path)}`}
                      alt={att.filename}
                      style={{
                        maxWidth: "100%",
                        maxHeight: 300,
                        borderRadius: 8,
                        objectFit: "contain",
                        display: "block",
                      }}
                    />
                  );
                }
                return (
                  <a
                    key={idx}
                    href={`/api/fs/download?path=${encodeURIComponent(att.path)}`}
                    download={att.filename}
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 5,
                      background: "var(--bg-elevated)",
                      border: "1px solid var(--border-subtle)",
                      borderRadius: 8,
                      padding: "4px 10px",
                      fontSize: 12,
                      color: "var(--text-secondary)",
                      textDecoration: "none",
                    }}
                  >
                    <Attachment size={14} /> {att.filename}
                  </a>
                );
              })}
            </div>
          )}
          <div className={styles.markdown}>
            <ReactMarkdown
              remarkPlugins={[remarkGfm, remarkBreaks]}
              components={markdownComponents(onFilePath)}
            >
              {message.content}
            </ReactMarkdown>
          </div>
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
  streaming?: boolean;
  onFilePath?: (path: string) => void;
}

export function StreamingBubble({
  personaEmoji = "🤖",
  personaName = "Agent",
  content,
  streaming = true,
  onFilePath,
}: StreamingBubbleProps) {
  return (
    <div className={`${styles.row} ${styles.rowAgent}`}>
      <AgentAvatar emoji={personaEmoji} />
      <div className={styles.col}>
        <div className={`${styles.bubble} ${styles.bubbleAgent}`}>
          {content && (
            <div className={styles.markdown}>
              <ReactMarkdown
                remarkPlugins={[remarkGfm, remarkBreaks]}
                components={markdownComponents(onFilePath)}
              >
                {content}
              </ReactMarkdown>
            </div>
          )}
          {streaming && <StreamingIndicator />}
        </div>
        <div className={styles.meta}>{personaName} · now</div>
      </div>
    </div>
  );
}

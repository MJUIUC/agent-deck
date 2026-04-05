import { useCallback, useEffect, useState } from "react";
import { Archive } from "@carbon/icons-react";
import { threadsApi } from "@/api/client";
import type { Thread } from "@/types";

// ─── Thread row ───────────────────────────────────────────────────────────────

function ArchivedThreadRow({ thread }: { thread: Thread }) {
  const persona = thread.persona;
  const preview = thread.last_message_preview;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 12,
        padding: "12px 16px",
        borderBottom: "1px solid var(--border-subtle)",
      }}
    >
      {/* Persona emoji avatar */}
      <div
        style={{
          width: 34,
          height: 34,
          borderRadius: 8,
          background: "var(--bg-elevated)",
          border: "1px solid var(--border-subtle)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: 17,
          flexShrink: 0,
        }}
      >
        {persona?.emoji ?? "💬"}
      </div>

      {/* Text content */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            marginBottom: 2,
          }}
        >
          {thread.title}
        </div>

        {persona && (
          <div
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              marginBottom: preview ? 3 : 0,
            }}
          >
            {persona.name}
          </div>
        )}

        {preview && (
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {preview}
          </div>
        )}
      </div>

      {/* Archived date */}
      <div
        style={{
          fontSize: 11,
          color: "var(--text-tertiary)",
          flexShrink: 0,
          marginTop: 2,
          whiteSpace: "nowrap",
        }}
      >
        {new Date(thread.updated_at).toLocaleDateString(undefined, {
          month: "short",
          day: "numeric",
          year: "numeric",
        })}
      </div>
    </div>
  );
}

// ─── ArchivedThreadsSettings ──────────────────────────────────────────────────

export function ArchivedThreadsSettings() {
  const [threads, setThreads] = useState<Thread[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const res = await threadsApi.list("archived");
      // Sort most-recently-updated first
      const sorted = [...res.data].sort(
        (a, b) =>
          new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      );
      setThreads(sorted);
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Failed to load archived threads.",
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div>
      {/* ── Section header ── */}
      <div style={{ marginBottom: 20 }}>
        <div
          style={{
            fontSize: 16,
            fontWeight: 700,
            color: "var(--text-primary)",
            marginBottom: 4,
          }}
        >
          Archived Threads
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          Threads you've archived are listed here for reference. Restore and
          export functionality is planned for a future update.
        </div>
      </div>

      {/* ── List card ── */}
      <div
        style={{
          background: "var(--bg-tertiary)",
          border: "1px solid var(--border-subtle)",
          borderRadius: 10,
          overflow: "hidden",
        }}
      >
        {isLoading ? (
          <div
            style={{
              padding: "40px 20px",
              textAlign: "center",
              fontSize: 13,
              color: "var(--text-tertiary)",
            }}
          >
            Loading…
          </div>
        ) : error ? (
          <div
            style={{
              padding: "32px 20px",
              textAlign: "center",
            }}
          >
            <div
              style={{
                fontSize: 13,
                color: "var(--error)",
                marginBottom: 10,
              }}
            >
              {error}
            </div>
            <button
              type="button"
              onClick={load}
              style={{
                padding: "6px 14px",
                borderRadius: 6,
                fontSize: 12,
                fontWeight: 500,
                fontFamily: "inherit",
                cursor: "pointer",
                border: "1px solid var(--border-default)",
                background: "transparent",
                color: "var(--text-secondary)",
              }}
            >
              Retry
            </button>
          </div>
        ) : threads.length === 0 ? (
          <div
            style={{
              padding: "48px 20px",
              textAlign: "center",
            }}
          >
            <div style={{ marginBottom: 10 }}>
              <Archive size={28} />
            </div>
            <div
              style={{
                fontSize: 14,
                fontWeight: 600,
                color: "var(--text-primary)",
                marginBottom: 4,
              }}
            >
              No archived threads
            </div>
            <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              Archive a thread from its config pane to see it here.
            </div>
          </div>
        ) : (
          <div>
            {/* Column header row */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                padding: "8px 16px",
                borderBottom: "1px solid var(--border-subtle)",
                background: "var(--bg-elevated)",
              }}
            >
              <div
                style={{
                  flex: 1,
                  fontSize: 11,
                  fontWeight: 600,
                  color: "var(--text-tertiary)",
                  textTransform: "uppercase",
                  letterSpacing: "0.06em",
                }}
              >
                Thread
              </div>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  color: "var(--text-tertiary)",
                  textTransform: "uppercase",
                  letterSpacing: "0.06em",
                }}
              >
                Archived
              </div>
            </div>

            {/* Thread rows */}
            {threads.map((thread) => (
              <ArchivedThreadRow key={thread.id} thread={thread} />
            ))}
          </div>
        )}
      </div>

      {/* ── Future work note ── */}
      {threads.length > 0 && (
        <div
          style={{
            marginTop: 12,
            padding: "10px 14px",
            borderRadius: 8,
            background: "rgba(196,162,74,0.07)",
            border: "1px solid rgba(196,162,74,0.2)",
            fontSize: 12,
            color: "var(--text-tertiary)",
            lineHeight: 1.5,
          }}
        >
          🔒{" "}
          <strong style={{ color: "var(--text-secondary)" }}>Read-only.</strong>{" "}
          Restore and markdown export are planned for a future update. For now,
          archived threads are preserved in the database and accessible here for
          reference.
        </div>
      )}
    </div>
  );
}

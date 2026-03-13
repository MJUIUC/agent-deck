import React, { useState, useEffect, useCallback } from "react";
import type { McpServer, McpTool } from "@/types";
import { mcpServersApi } from "@/api/client";
import {
  Btn,
  FieldLabel,
  FieldInput,
  FieldTextarea,
  FieldHint,
} from "./shared";

// ─── Badge helpers ────────────────────────────────────────────────────────────

function TypeBadge({ type }: { type: "local" | "remote" }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "2px 8px",
        borderRadius: 20,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.04em",
        textTransform: "uppercase",
        background:
          type === "local" ? "rgba(124,140,90,0.15)" : "rgba(90,120,180,0.15)",
        color: type === "local" ? "var(--accent-primary)" : "var(--info)",
        flexShrink: 0,
      }}
    >
      {type === "local" ? "local" : "remote"}
    </span>
  );
}

function StatusBadge({
  status,
}: {
  status: "inactive" | "connecting" | "connected" | "error";
}) {
  const cfg: Record<
    string,
    { bg: string; color: string; dot: string; label: string }
  > = {
    connected: {
      bg: "rgba(106,158,91,0.15)",
      color: "var(--success)",
      dot: "var(--success)",
      label: "connected",
    },
    connecting: {
      bg: "rgba(196,162,74,0.12)",
      color: "var(--warning)",
      dot: "var(--warning)",
      label: "connecting",
    },
    error: {
      bg: "rgba(196,90,90,0.12)",
      color: "var(--error)",
      dot: "var(--error)",
      label: "error",
    },
    inactive: {
      bg: "rgba(160,160,160,0.10)",
      color: "var(--text-tertiary)",
      dot: "var(--text-tertiary)",
      label: "inactive",
    },
  };
  const c = cfg[status] ?? cfg.inactive;
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "2px 8px 2px 6px",
        borderRadius: 20,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.04em",
        textTransform: "uppercase",
        background: c.bg,
        color: c.color,
        flexShrink: 0,
      }}
    >
      <span
        style={{
          width: 5,
          height: 5,
          borderRadius: "50%",
          background: c.dot,
          display: "inline-block",
          flexShrink: 0,
        }}
      />
      {c.label}
    </span>
  );
}

// ─── Tool inspector ───────────────────────────────────────────────────────────

function ToolInspector({
  serverId,
  serverStatus,
}: {
  serverId: string;
  serverStatus: string;
}) {
  const [open, setOpen] = useState(false);
  const [tools, setTools] = useState<McpTool[]>([]);
  const [loading, setLoading] = useState(false);
  const [fetched, setFetched] = useState(false);

  const handleToggle = useCallback(async () => {
    const next = !open;
    setOpen(next);
    if (next && !fetched) {
      setLoading(true);
      try {
        const res = await mcpServersApi.listTools(serverId);
        setTools(res.data);
      } catch {
        setTools([]);
      } finally {
        setLoading(false);
        setFetched(true);
      }
    }
  }, [open, fetched, serverId]);

  return (
    <div style={{ marginTop: 8 }}>
      <button
        type="button"
        onClick={handleToggle}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 5,
          background: "none",
          border: "none",
          padding: "3px 0",
          cursor: "pointer",
          fontSize: 12,
          color: "var(--text-secondary)",
          fontFamily: "inherit",
        }}
        onMouseEnter={(e) =>
          ((e.currentTarget as HTMLButtonElement).style.color =
            "var(--text-primary)")
        }
        onMouseLeave={(e) =>
          ((e.currentTarget as HTMLButtonElement).style.color =
            "var(--text-secondary)")
        }
      >
        <span
          style={{
            display: "inline-block",
            fontSize: 9,
            transition: "transform 0.15s",
            transform: open ? "rotate(90deg)" : "rotate(0deg)",
          }}
        >
          ▶
        </span>
        <span>Tools</span>
        {fetched && (
          <span style={{ color: "var(--text-tertiary)", fontSize: 10 }}>
            ({tools.length})
          </span>
        )}
      </button>

      {open && (
        <div
          style={{
            marginTop: 6,
            paddingLeft: 14,
            borderLeft: "2px solid var(--border-subtle)",
          }}
        >
          {loading && (
            <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              Loading tools…
            </span>
          )}
          {!loading && tools.length === 0 && (
            <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              {serverStatus === "connected"
                ? "No tools reported."
                : "Server not connected — tools unavailable."}
            </span>
          )}
          {!loading &&
            tools.map((tool) => (
              <div
                key={tool.name}
                style={{
                  display: "flex",
                  gap: 8,
                  padding: "4px 0",
                  borderBottom: "1px solid var(--border-subtle)",
                }}
              >
                <span
                  style={{
                    fontSize: 12,
                    fontFamily: '"SF Mono","Fira Code",monospace',
                    color: "var(--accent-primary)",
                    flexShrink: 0,
                    minWidth: 140,
                  }}
                >
                  {tool.name}
                </span>
                <span
                  style={{
                    fontSize: 12,
                    color: "var(--text-secondary)",
                    lineHeight: 1.4,
                  }}
                >
                  {tool.description}
                </span>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

// ─── Delete confirmation ──────────────────────────────────────────────────────

function DeleteConfirmModal({
  name,
  onConfirm,
  onCancel,
}: {
  name: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <>
      {/* Backdrop */}
      <div
        onClick={onCancel}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 400,
          background: "rgba(0,0,0,0.45)",
          backdropFilter: "blur(4px)",
        }}
      />
      {/* Dialog */}
      <div
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 401,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          pointerEvents: "none",
        }}
      >
        <div
          style={{
            background: "var(--bg-secondary)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 12,
            padding: "24px 28px",
            width: 380,
            pointerEvents: "auto",
            boxShadow: "0 16px 48px rgba(0,0,0,0.5)",
          }}
        >
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-primary)",
              marginBottom: 10,
            }}
          >
            Delete MCP Server
          </div>
          <p
            style={{
              fontSize: 13,
              color: "var(--text-secondary)",
              margin: "0 0 20px",
              lineHeight: 1.55,
            }}
          >
            Are you sure you want to delete{" "}
            <strong style={{ color: "var(--text-primary)" }}>{name}</strong>?
            This will detach it from all threads.
          </p>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            <Btn variant="ghost" onClick={onCancel}>
              Cancel
            </Btn>
            <Btn variant="danger" onClick={onConfirm}>
              Delete
            </Btn>
          </div>
        </div>
      </div>
    </>
  );
}

// ─── Env var key-value editor ─────────────────────────────────────────────────

interface EnvPair {
  key: string;
  value: string;
}

function EnvVarEditor({
  pairs,
  onChange,
}: {
  pairs: EnvPair[];
  onChange: (pairs: EnvPair[]) => void;
}) {
  const add = () => onChange([...pairs, { key: "", value: "" }]);
  const remove = (i: number) => onChange(pairs.filter((_, idx) => idx !== i));
  const update = (i: number, field: "key" | "value", val: string) => {
    const next = pairs.map((p, idx) =>
      idx === i ? { ...p, [field]: val } : p,
    );
    onChange(next);
  };

  return (
    <div>
      {pairs.map((pair, i) => (
        <div
          key={i}
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr auto",
            gap: 6,
            marginBottom: 6,
          }}
        >
          <FieldInput
            value={pair.key}
            onChange={(e) => update(i, "key", e.target.value)}
            placeholder="KEY"
            mono
            style={{ fontSize: 12 }}
          />
          <FieldInput
            value={pair.value}
            onChange={(e) => update(i, "value", e.target.value)}
            placeholder="value"
            mono
            style={{ fontSize: 12 }}
          />
          <button
            type="button"
            onClick={() => remove(i)}
            title="Remove"
            style={{
              background: "none",
              border: "1px solid var(--border-default)",
              borderRadius: 6,
              color: "var(--text-tertiary)",
              cursor: "pointer",
              padding: "0 8px",
              fontSize: 12,
              fontFamily: "inherit",
              transition: "color 0.15s, border-color 0.15s",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--error)";
              (e.currentTarget as HTMLButtonElement).style.borderColor =
                "var(--error)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-tertiary)";
              (e.currentTarget as HTMLButtonElement).style.borderColor =
                "var(--border-default)";
            }}
          >
            ✕
          </button>
        </div>
      ))}
      <Btn variant="ghost" sm onClick={add} style={{ marginTop: 2 }}>
        ＋ Add variable
      </Btn>
    </div>
  );
}

// ─── Form state types ─────────────────────────────────────────────────────────

interface LocalFormState {
  executable: string;
  args: string; // one per line
  envPairs: EnvPair[];
}

interface RemoteFormState {
  url: string;
  auth_header: string;
  credential_key: string;
}

interface FormState {
  name: string;
  description: string;
  source_url: string;
  server_type: "local" | "remote";
  local: LocalFormState;
  remote: RemoteFormState;
}

const EMPTY_FORM: FormState = {
  name: "",
  description: "",
  source_url: "",
  server_type: "local",
  local: { executable: "", args: "", envPairs: [] },
  remote: { url: "", auth_header: "Authorization", credential_key: "" },
};

function serverToFormState(server: McpServer): FormState {
  let parsed: Record<string, unknown> = {};
  try {
    parsed = JSON.parse(server.config);
  } catch {
    // ignore malformed config
  }

  const local: LocalFormState = {
    executable: (parsed.executable as string) ?? "",
    args: Array.isArray(parsed.args)
      ? (parsed.args as string[]).join("\n")
      : "",
    envPairs: parsed.env
      ? Object.entries(parsed.env as Record<string, string>).map(
          ([key, value]) => ({ key, value }),
        )
      : [],
  };

  const remote: RemoteFormState = {
    url: (parsed.url as string) ?? "",
    auth_header: (parsed.auth_header as string) ?? "Authorization",
    credential_key: (parsed.credential_key as string) ?? "",
  };

  return {
    name: server.name,
    description: server.description ?? "",
    source_url: server.source_url ?? "",
    server_type: server.server_type as "local" | "remote",
    local,
    remote,
  };
}

function formStateToConfig(form: FormState) {
  if (form.server_type === "local") {
    const args = form.local.args
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
    const env: Record<string, string> = {};
    for (const pair of form.local.envPairs) {
      if (pair.key.trim()) env[pair.key.trim()] = pair.value;
    }
    return {
      executable: form.local.executable,
      args,
      env,
    };
  } else {
    return {
      url: form.remote.url,
      auth_header: form.remote.auth_header || "Authorization",
      credential_key: form.remote.credential_key,
    };
  }
}

// ─── Add / Edit form ──────────────────────────────────────────────────────────

function McpForm({
  initial,
  editingId,
  onSave,
  onCancel,
}: {
  initial: FormState;
  editingId: string | null;
  onSave: (server: McpServer) => void;
  onCancel: () => void;
}) {
  const [form, setForm] = useState<FormState>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = <K extends keyof FormState>(key: K, value: FormState[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  const setLocal = <K extends keyof LocalFormState>(
    key: K,
    value: LocalFormState[K],
  ) => setForm((f) => ({ ...f, local: { ...f.local, [key]: value } }));

  const setRemote = <K extends keyof RemoteFormState>(
    key: K,
    value: RemoteFormState[K],
  ) => setForm((f) => ({ ...f, remote: { ...f.remote, [key]: value } }));

  const handleSubmit = async () => {
    if (!form.name.trim()) {
      setError("Name is required.");
      return;
    }
    if (form.server_type === "local" && !form.local.executable.trim()) {
      setError("Executable is required for local servers.");
      return;
    }
    if (form.server_type === "remote" && !form.remote.url.trim()) {
      setError("URL is required for remote servers.");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const payload = {
        name: form.name.trim(),
        description: form.description.trim() || undefined,
        source_url: form.source_url.trim() || undefined,
        server_type: form.server_type,
        config: formStateToConfig(form),
      };

      let result: McpServer;
      if (editingId) {
        const res = await mcpServersApi.update(editingId, payload);
        result = res.data;
      } else {
        const res = await mcpServersApi.create(payload);
        result = res.data;
      }
      onSave(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save server.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-default)",
        borderRadius: 10,
        padding: "20px 20px 16px",
        marginBottom: 16,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 16,
        }}
      >
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
          }}
        >
          {editingId ? "Edit MCP Server" : "Add MCP Server"}
        </span>
      </div>

      {/* Shared fields */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 14,
        }}
      >
        {/* Name */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 5,
            gridColumn: "1 / -1",
          }}
        >
          <FieldLabel>Name *</FieldLabel>
          <FieldInput
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
            placeholder="e.g. Filesystem Server"
          />
        </div>

        {/* Description */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 5,
            gridColumn: "1 / -1",
          }}
        >
          <FieldLabel>Description</FieldLabel>
          <FieldInput
            value={form.description}
            onChange={(e) => set("description", e.target.value)}
            placeholder="Brief description of what this server provides…"
          />
        </div>

        {/* Source URL */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 5,
            gridColumn: "1 / -1",
          }}
        >
          <FieldLabel>Source URL</FieldLabel>
          <FieldInput
            value={form.source_url}
            onChange={(e) => set("source_url", e.target.value)}
            placeholder="https://github.com/modelcontextprotocol/servers"
          />
          <FieldHint>
            Link to the server's documentation or repository (optional).
          </FieldHint>
        </div>

        {/* Type toggle */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 5,
            gridColumn: "1 / -1",
          }}
        >
          <FieldLabel>Type</FieldLabel>
          <div style={{ display: "flex", gap: 8 }}>
            {(["local", "remote"] as const).map((t) => (
              <button
                key={t}
                type="button"
                onClick={() => set("server_type", t)}
                style={{
                  padding: "7px 16px",
                  borderRadius: 7,
                  fontSize: 13,
                  fontWeight: 500,
                  cursor: "pointer",
                  fontFamily: "inherit",
                  border:
                    form.server_type === t
                      ? "1.5px solid var(--accent-primary)"
                      : "1px solid var(--border-default)",
                  background:
                    form.server_type === t
                      ? "var(--accent-muted)"
                      : "var(--bg-elevated)",
                  color:
                    form.server_type === t
                      ? "var(--accent-primary)"
                      : "var(--text-secondary)",
                  transition: "all 0.15s",
                }}
              >
                {t === "local" ? "🖥 Local (stdio)" : "🌐 Remote (HTTP/SSE)"}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Local config */}
      {form.server_type === "local" && (
        <div
          style={{
            marginTop: 14,
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 8,
            padding: "14px 14px 10px",
          }}
        >
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.06em",
              marginBottom: 12,
            }}
          >
            Local Configuration
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel>Executable</FieldLabel>
              <FieldInput
                value={form.local.executable}
                onChange={(e) => setLocal("executable", e.target.value)}
                placeholder="npx @modelcontextprotocol/server-filesystem"
                mono
              />
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel>Args (one per line)</FieldLabel>
              <FieldTextarea
                value={form.local.args}
                onChange={(e) => setLocal("args", e.target.value)}
                placeholder={"/Users/marcus/Documents\n/Users/marcus/Projects"}
                style={{
                  minHeight: 64,
                  fontFamily: '"SF Mono","Fira Code",monospace',
                  fontSize: 12,
                }}
              />
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel>Environment Variables</FieldLabel>
              <EnvVarEditor
                pairs={form.local.envPairs}
                onChange={(pairs) => setLocal("envPairs", pairs)}
              />
            </div>
          </div>
        </div>
      )}

      {/* Remote config */}
      {form.server_type === "remote" && (
        <div
          style={{
            marginTop: 14,
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 8,
            padding: "14px 14px 10px",
          }}
        >
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.06em",
              marginBottom: 12,
            }}
          >
            Remote Configuration
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel>URL</FieldLabel>
              <FieldInput
                value={form.remote.url}
                onChange={(e) => setRemote("url", e.target.value)}
                placeholder="https://api.example.com/mcp"
                mono
              />
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel>Auth Header Name</FieldLabel>
              <FieldInput
                value={form.remote.auth_header}
                onChange={(e) => setRemote("auth_header", e.target.value)}
                placeholder="Authorization"
                mono
              />
              <FieldHint>
                HTTP header used to send the auth token, e.g.{" "}
                <code
                  style={{
                    fontFamily: '"SF Mono","Fira Code",monospace',
                    fontSize: 11,
                    background: "var(--bg-tertiary)",
                    padding: "1px 4px",
                    borderRadius: 3,
                  }}
                >
                  Authorization
                </code>
                .
              </FieldHint>
            </div>

            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel>Credential Key</FieldLabel>
              <FieldInput
                value={form.remote.credential_key}
                onChange={(e) => setRemote("credential_key", e.target.value)}
                placeholder="e.g. github-token (free text for now)"
                mono
              />
              <FieldHint>
                Key of the stored credential to inject as the auth token. Will
                become a credential picker once the credential store is built
                (Story 3.x).
              </FieldHint>
            </div>
          </div>
        </div>
      )}

      {/* Error */}
      {error && (
        <div
          style={{
            marginTop: 12,
            padding: "8px 12px",
            borderRadius: 7,
            background: "rgba(196,90,90,0.1)",
            border: "1px solid rgba(196,90,90,0.3)",
            color: "var(--error)",
            fontSize: 12,
          }}
        >
          {error}
        </div>
      )}

      {/* Actions */}
      <div
        style={{
          display: "flex",
          gap: 8,
          justifyContent: "flex-end",
          marginTop: 16,
        }}
      >
        <Btn variant="ghost" onClick={onCancel} disabled={saving}>
          Cancel
        </Btn>
        <Btn variant="primary" onClick={handleSubmit} disabled={saving}>
          {saving
            ? editingId
              ? "Saving…"
              : "Adding…"
            : editingId
              ? "Save Changes"
              : "Add Server"}
        </Btn>
      </div>
    </div>
  );
}

// ─── Single MCP server card ───────────────────────────────────────────────────

function McpServerCard({
  server,
  onEdit,
  onDelete,
}: {
  server: McpServer;
  onEdit: (server: McpServer) => void;
  onDelete: (server: McpServer) => void;
}) {
  const isError = server.status === "error";

  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: `1px solid ${isError ? "rgba(196,90,90,0.3)" : "var(--border-subtle)"}`,
        borderRadius: 9,
        padding: "14px 16px",
        marginBottom: 8,
      }}
    >
      {/* Top row: name + badges */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 6,
        }}
      >
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
          }}
        >
          {server.name}
        </span>
        <div style={{ display: "flex", gap: 4 }}>
          <StatusBadge status={server.status} />
          <TypeBadge type={server.server_type} />
        </div>
      </div>

      {/* Description */}
      {server.description && (
        <div
          style={{
            fontSize: 12,
            color: isError ? "var(--error)" : "var(--text-secondary)",
            marginBottom: 4,
            lineHeight: 1.45,
          }}
        >
          {server.description}
        </div>
      )}

      {/* Source URL */}
      {server.source_url && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            marginBottom: 4,
          }}
        >
          <span
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {server.source_url.replace(/^https?:\/\//, "")}
          </span>
          <a
            href={server.source_url}
            target="_blank"
            rel="noreferrer"
            title="Open source URL"
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              textDecoration: "none",
              flexShrink: 0,
            }}
            onMouseEnter={(e) =>
              ((e.currentTarget as HTMLAnchorElement).style.color =
                "var(--accent-primary)")
            }
            onMouseLeave={(e) =>
              ((e.currentTarget as HTMLAnchorElement).style.color =
                "var(--text-tertiary)")
            }
          >
            ↗
          </a>
        </div>
      )}

      {/* Tool inspector */}
      <ToolInspector serverId={server.id} serverStatus={server.status} />

      {/* Actions */}
      <div
        style={{
          display: "flex",
          gap: 6,
          marginTop: 10,
          paddingTop: 10,
          borderTop: "1px solid var(--border-subtle)",
        }}
      >
        <Btn variant="ghost" sm onClick={() => onEdit(server)}>
          Edit
        </Btn>
        <Btn variant="danger" sm onClick={() => onDelete(server)}>
          Delete
        </Btn>
      </div>
    </div>
  );
}

// ─── McpServerSettings ────────────────────────────────────────────────────────

export function McpServerSettings() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  // Form state: null = hidden, "new" = add form, string = editing server id
  const [formMode, setFormMode] = useState<null | "new" | string>(null);
  const [formInitial, setFormInitial] = useState<FormState>(EMPTY_FORM);

  // Delete confirmation
  const [deleteTarget, setDeleteTarget] = useState<McpServer | null>(null);
  const [deleting, setDeleting] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const res = await mcpServersApi.list();
      setServers(res.data);
    } catch (e) {
      setLoadError(
        e instanceof Error ? e.message : "Failed to load MCP servers.",
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleAddClick = () => {
    setFormInitial(EMPTY_FORM);
    setFormMode("new");
  };

  const handleEdit = (server: McpServer) => {
    setFormInitial(serverToFormState(server));
    setFormMode(server.id);
  };

  const handleFormSave = (saved: McpServer) => {
    setServers((prev) => {
      const idx = prev.findIndex((s) => s.id === saved.id);
      if (idx >= 0) {
        const next = [...prev];
        next[idx] = saved;
        return next;
      }
      return [...prev, saved];
    });
    setFormMode(null);
  };

  const handleFormCancel = () => setFormMode(null);

  const handleDeleteClick = (server: McpServer) => setDeleteTarget(server);

  const handleDeleteConfirm = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await mcpServersApi.delete(deleteTarget.id);
      setServers((prev) => prev.filter((s) => s.id !== deleteTarget.id));
      setDeleteTarget(null);
    } catch {
      // leave modal open so user can retry
    } finally {
      setDeleting(false);
    }
  };

  const handleDeleteCancel = () => setDeleteTarget(null);

  return (
    <div>
      {/* Section header */}
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          justifyContent: "space-between",
          marginBottom: 18,
        }}
      >
        <div>
          <div
            style={{
              fontSize: 16,
              fontWeight: 700,
              color: "var(--text-primary)",
              marginBottom: 4,
            }}
          >
            MCP Servers
          </div>
          <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            MCP servers run as child processes or remote connections and are
            available to attach to threads.
          </div>
        </div>
      </div>

      {/* Add / Edit form */}
      {formMode !== null && (
        <McpForm
          initial={formInitial}
          editingId={formMode === "new" ? null : formMode}
          onSave={handleFormSave}
          onCancel={handleFormCancel}
        />
      )}

      {/* Loading */}
      {loading && (
        <div
          style={{
            padding: "32px 0",
            textAlign: "center",
            color: "var(--text-tertiary)",
            fontSize: 13,
          }}
        >
          Loading…
        </div>
      )}

      {/* Load error */}
      {!loading && loadError && (
        <div
          style={{
            padding: "12px 16px",
            borderRadius: 8,
            background: "rgba(196,90,90,0.1)",
            border: "1px solid rgba(196,90,90,0.25)",
            color: "var(--error)",
            fontSize: 13,
            marginBottom: 12,
          }}
        >
          {loadError}{" "}
          <button
            type="button"
            onClick={load}
            style={{
              background: "none",
              border: "none",
              color: "var(--error)",
              cursor: "pointer",
              textDecoration: "underline",
              fontFamily: "inherit",
              fontSize: 13,
              padding: 0,
            }}
          >
            Retry
          </button>
        </div>
      )}

      {/* Empty state */}
      {!loading && !loadError && servers.length === 0 && formMode === null && (
        <div
          style={{
            padding: "40px 0",
            textAlign: "center",
            color: "var(--text-tertiary)",
          }}
        >
          <div style={{ fontSize: 28, marginBottom: 10 }}>🔧</div>
          <div
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginBottom: 4,
            }}
          >
            No MCP servers configured
          </div>
          <div style={{ fontSize: 12, marginBottom: 16 }}>
            Add a server to enable tools in your threads.
          </div>
          <Btn variant="primary" sm onClick={handleAddClick}>
            + Add Server
          </Btn>
        </div>
      )}

      {/* Server list */}
      {!loading && servers.length > 0 && (
        <div>
          {servers.map((server) =>
            formMode === server.id ? null : (
              <McpServerCard
                key={server.id}
                server={server}
                onEdit={handleEdit}
                onDelete={handleDeleteClick}
              />
            ),
          )}
        </div>
      )}

      {/* Delete confirmation modal */}
      {deleteTarget && (
        <DeleteConfirmModal
          name={deleteTarget.name}
          onConfirm={handleDeleteConfirm}
          onCancel={handleDeleteCancel}
        />
      )}

      {/* Suppress unused deleting state lint warning */}
      {deleting && null}
    </div>
  );
}

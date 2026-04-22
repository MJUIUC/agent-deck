import React, { useState, useEffect, useCallback } from "react";
import { Plus } from "lucide-react";
import { Webhook } from "@carbon/icons-react";
import { webhookBindingsApi } from "@/api/client";
import type { WebhookBinding } from "@/api/client";
import {
  Btn,
  FieldLabel,
  FieldInput,
  FieldSelect,
  FieldTextarea,
  FieldHint,
  SectionCard,
} from "./shared";

// ─── Source badge ─────────────────────────────────────────────────────────────

function SourceBadge({ source }: { source: string }) {
  const cfg: Record<string, { bg: string; color: string; label: string }> = {
    github: {
      bg: "rgba(106,158,91,0.15)",
      color: "var(--success)",
      label: "GitHub",
    },
    gitlab: {
      bg: "rgba(252,109,38,0.15)",
      color: "#fc6d26",
      label: "GitLab",
    },
    other: {
      bg: "rgba(150,150,160,0.15)",
      color: "var(--text-secondary)",
      label: "Other",
    },
  };
  const c = cfg[source] ?? cfg.other;
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
        background: c.bg,
        color: c.color,
        flexShrink: 0,
      }}
    >
      {c.label}
    </span>
  );
}

// ─── EnabledDot ───────────────────────────────────────────────────────────────

function EnabledDot({ enabled }: { enabled: boolean }) {
  return (
    <span
      title={enabled ? "Enabled" : "Disabled"}
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: enabled ? "var(--success)" : "var(--text-tertiary)",
        flexShrink: 0,
      }}
    />
  );
}

// ─── DeleteConfirmModal ───────────────────────────────────────────────────────

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
            Delete Webhook Binding
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

// ─── SecretPanel ──────────────────────────────────────────────────────────────

function SecretPanel({
  webhookUrl,
  secret,
  warning,
  onDone,
}: {
  webhookUrl: string;
  secret: string;
  warning?: string;
  onDone: () => void;
}) {
  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "18px 20px",
        marginBottom: 16,
      }}
    >
      {/* Warning banner */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "10px 14px",
          borderRadius: 8,
          background: "rgba(196,162,74,0.12)",
          border: "1px solid rgba(196,162,74,0.3)",
          color: "var(--warning)",
          fontSize: 13,
          fontWeight: 500,
          marginBottom: 16,
        }}
      >
        ⚠ Copy this secret now — it won't be shown again.
      </div>

      {/* Webhook URL */}
      <div style={{ marginBottom: 12 }}>
        <FieldLabel>Webhook URL</FieldLabel>
        <div
          style={{
            display: "flex",
            gap: 6,
            alignItems: "center",
            marginTop: 6,
          }}
        >
          <code
            style={{
              flex: 1,
              fontSize: 11,
              wordBreak: "break-all",
              background: "var(--bg-elevated)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 6,
              padding: "8px 10px",
              color: "var(--text-primary)",
              fontFamily: '"SF Mono","Fira Code",monospace',
              lineHeight: 1.5,
            }}
          >
            {webhookUrl}
          </code>
          <Btn
            variant="ghost"
            sm
            onClick={() => navigator.clipboard.writeText(webhookUrl)}
          >
            Copy
          </Btn>
        </div>
      </div>

      {/* Secret */}
      <div style={{ marginBottom: 16 }}>
        <FieldLabel>Secret</FieldLabel>
        <div
          style={{
            display: "flex",
            gap: 6,
            alignItems: "center",
            marginTop: 6,
          }}
        >
          <code
            style={{
              flex: 1,
              fontSize: 11,
              wordBreak: "break-all",
              background: "var(--bg-elevated)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 6,
              padding: "8px 10px",
              color: "var(--text-primary)",
              fontFamily: '"SF Mono","Fira Code",monospace',
              lineHeight: 1.5,
            }}
          >
            {secret}
          </code>
          <Btn
            variant="ghost"
            sm
            onClick={() => navigator.clipboard.writeText(secret)}
          >
            Copy
          </Btn>
        </div>
      </div>

      {/* Optional server warning */}
      {warning && (
        <div
          style={{
            padding: "8px 12px",
            borderRadius: 7,
            background: "rgba(196,90,90,0.1)",
            border: "1px solid rgba(196,90,90,0.25)",
            color: "var(--error)",
            fontSize: 12,
            marginBottom: 14,
          }}
        >
          {warning}
        </div>
      )}

      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <Btn variant="primary" onClick={onDone}>
          Done
        </Btn>
      </div>
    </div>
  );
}

// ─── WebhookForm ──────────────────────────────────────────────────────────────

function WebhookForm({
  initial,
  onSave,
  onCancel,
}: {
  initial: WebhookBinding | null;
  onSave: (
    binding: WebhookBinding,
    isNew: boolean,
    secret?: string,
    webhookUrl?: string,
    warning?: string,
  ) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [source, setSource] = useState<"github" | "gitlab" | "other">(
    (initial?.source as "github" | "gitlab" | "other") ?? "github",
  );
  const [signatureHeader, setSignatureHeader] = useState(
    initial?.signature_header ?? "",
  );
  const [prompt, setPrompt] = useState(initial?.prompt ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const isCreate = initial === null;

  const validate = () => {
    if (!name.trim()) return "Name is required.";
    if (source === "other" && !signatureHeader.trim())
      return "Signature header is required for custom sources.";
    if (source === "other" && !prompt.trim())
      return "Default prompt is required for custom sources.";
    return null;
  };

  const handleSubmit = async () => {
    const err = validate();
    if (err) {
      setError(err);
      return;
    }
    setError("");
    setSaving(true);
    try {
      if (isCreate) {
        const payload: {
          name: string;
          source: string;
          prompt: string;
          signature_header?: string;
        } = {
          name: name.trim(),
          source,
          prompt: prompt.trim(),
        };
        if (source === "other" && signatureHeader.trim()) {
          payload.signature_header = signatureHeader.trim();
        }
        const res = await webhookBindingsApi.create(payload);
        onSave(
          res.data,
          true,
          res.data.secret,
          res.data.webhook_url,
          res.warning,
        );
      } else {
        const patchPayload: {
          name?: string;
          prompt?: string;
          signature_header?: string;
        } = {};
        if (name.trim() !== initial.name) patchPayload.name = name.trim();
        if (prompt.trim() !== initial.prompt)
          patchPayload.prompt = prompt.trim();
        if (source === "other" && signatureHeader.trim() !== (initial.signature_header ?? "")) {
          patchPayload.signature_header = signatureHeader.trim();
        }
        const res = await webhookBindingsApi.update(initial.id, patchPayload);
        onSave(res.data, false);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save binding.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "16px 18px",
        marginBottom: 16,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 14,
        }}
      >
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
          }}
        >
          {isCreate ? "New Webhook Binding" : "Edit Webhook Binding"}
        </span>
      </div>

      {/* Name */}
      <div style={{ display: "flex", flexDirection: "column", gap: 5, marginBottom: 12 }}>
        <FieldLabel>Name *</FieldLabel>
        <FieldInput
          placeholder="e.g. my-repo PRs"
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={saving}
        />
      </div>

      {/* Source */}
      <div style={{ display: "flex", flexDirection: "column", gap: 5, marginBottom: 12 }}>
        <FieldLabel>Source</FieldLabel>
        <FieldSelect
          value={source}
          onChange={(e) =>
            setSource(e.target.value as "github" | "gitlab" | "other")
          }
          disabled={saving}
        >
          <option value="github">GitHub</option>
          <option value="gitlab">GitLab</option>
          <option value="other">Other</option>
        </FieldSelect>
      </div>

      {/* Signature header — only for "other" */}
      {source === "other" && (
        <div style={{ display: "flex", flexDirection: "column", gap: 5, marginBottom: 12 }}>
          <FieldLabel>Signature header *</FieldLabel>
          <FieldInput
            mono
            placeholder="X-Linear-Signature"
            value={signatureHeader}
            onChange={(e) => setSignatureHeader(e.target.value)}
            disabled={saving}
          />
          <FieldHint>
            The HTTP header this service uses to send its HMAC-SHA256 signature.
          </FieldHint>
        </div>
      )}

      {/* Default prompt */}
      <div style={{ display: "flex", flexDirection: "column", gap: 5, marginBottom: 14 }}>
        <FieldLabel>
          {source === "other" ? "Default prompt (required)" : "Default prompt"}
        </FieldLabel>
        <FieldTextarea
          rows={4}
          placeholder="Describe what the agent should do when this webhook fires…"
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          disabled={saving}
        />
        <FieldHint>
          {source === "other"
            ? "Required — the agent has no structured summary for unknown sources."
            : "What should the agent do by default when this webhook fires? Thread attachments can override this per-thread."}
        </FieldHint>
      </div>

      {/* Error */}
      {error && (
        <div
          style={{
            padding: "8px 12px",
            borderRadius: 7,
            background: "rgba(196,90,90,0.1)",
            border: "1px solid rgba(196,90,90,0.25)",
            color: "var(--error)",
            fontSize: 12,
            marginBottom: 12,
          }}
        >
          {error}
        </div>
      )}

      {/* Actions */}
      <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
        <Btn variant="ghost" onClick={onCancel} disabled={saving}>
          Cancel
        </Btn>
        <Btn variant="primary" onClick={handleSubmit} disabled={saving}>
          {saving
            ? isCreate
              ? "Creating…"
              : "Saving…"
            : isCreate
              ? "Create binding"
              : "Save changes"}
        </Btn>
      </div>
    </div>
  );
}

// ─── WebhookCard ──────────────────────────────────────────────────────────────

function WebhookCard({
  binding,
  onEdit,
  onDelete,
}: {
  binding: WebhookBinding;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const promptPreview = binding.prompt
    ? binding.prompt.length > 100
      ? binding.prompt.slice(0, 100) + "…"
      : binding.prompt
    : null;

  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 9,
        padding: "14px 16px",
        marginBottom: 8,
      }}
    >
      {/* Top row */}
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
          {binding.name}
        </span>
        <SourceBadge source={binding.source} />
        <EnabledDot enabled={binding.enabled} />
      </div>

      {/* Prompt preview */}
      {promptPreview && (
        <div
          style={{
            fontSize: 12,
            color: "var(--text-secondary)",
            marginBottom: 4,
            lineHeight: 1.45,
          }}
        >
          {promptPreview}
        </div>
      )}

      {/* Signature header */}
      {binding.signature_header && (
        <div
          style={{
            fontSize: 11,
            color: "var(--text-tertiary)",
            fontFamily: '"SF Mono","Fira Code",monospace',
            marginBottom: 4,
          }}
        >
          {binding.signature_header}
        </div>
      )}

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
        <Btn variant="ghost" sm onClick={onEdit}>
          Edit
        </Btn>
        <Btn variant="danger" sm onClick={onDelete}>
          Delete
        </Btn>
      </div>
    </div>
  );
}

// ─── WebhookSettings ──────────────────────────────────────────────────────────

export function WebhookSettings() {
  const [bindings, setBindings] = useState<WebhookBinding[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [formMode, setFormMode] = useState<"hidden" | "create" | "edit">(
    "hidden",
  );
  const [formInitial, setFormInitial] = useState<WebhookBinding | null>(null);

  const [deleteTarget, setDeleteTarget] = useState<WebhookBinding | null>(null);
  const [deleting, setDeleting] = useState(false);

  const [pendingSecret, setPendingSecret] = useState<{
    url: string;
    secret: string;
    warning?: string;
  } | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const res = await webhookBindingsApi.list();
      setBindings(res.data);
    } catch (e) {
      setLoadError(
        e instanceof Error ? e.message : "Failed to load webhook bindings.",
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleAdd = () => {
    setFormInitial(null);
    setFormMode("create");
  };

  const handleEdit = (binding: WebhookBinding) => {
    setFormInitial(binding);
    setFormMode("edit");
  };

  const handleFormSave = (
    binding: WebhookBinding,
    isNew: boolean,
    secret?: string,
    webhookUrl?: string,
    warning?: string,
  ) => {
    if (isNew) {
      setBindings((prev) => [...prev, binding]);
      setFormMode("hidden");
      if (secret && webhookUrl) {
        setPendingSecret({ url: webhookUrl, secret, warning });
      }
    } else {
      setBindings((prev) => {
        const idx = prev.findIndex((b) => b.id === binding.id);
        if (idx >= 0) {
          const next = [...prev];
          next[idx] = binding;
          return next;
        }
        return prev;
      });
      setFormMode("hidden");
    }
  };

  const handleFormCancel = () => {
    setFormMode("hidden");
    setFormInitial(null);
  };

  const handleDeleteClick = (binding: WebhookBinding) =>
    setDeleteTarget(binding);

  const handleDeleteConfirm = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await webhookBindingsApi.delete(deleteTarget.id);
      setBindings((prev) => prev.filter((b) => b.id !== deleteTarget.id));
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
            Webhook Bindings
          </div>
          <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            Receive events from GitHub, GitLab, or other services and route them
            to agent threads.
          </div>
        </div>
        {formMode === "hidden" && !pendingSecret && (
          <Btn variant="primary" sm onClick={handleAdd}>
            <Plus size={13} />
            New Binding
          </Btn>
        )}
      </div>

      {/* Secret panel (shown after creation) */}
      {pendingSecret && (
        <SecretPanel
          webhookUrl={pendingSecret.url}
          secret={pendingSecret.secret}
          warning={pendingSecret.warning}
          onDone={() => setPendingSecret(null)}
        />
      )}

      {/* Create / Edit form */}
      {!pendingSecret && formMode !== "hidden" && (
        <WebhookForm
          initial={formMode === "edit" ? formInitial : null}
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
      {!loading &&
        !loadError &&
        bindings.length === 0 &&
        formMode === "hidden" &&
        !pendingSecret && (
          <div
            style={{
              padding: "40px 0",
              textAlign: "center",
              color: "var(--text-tertiary)",
            }}
          >
            <div style={{ marginBottom: 10 }}>
              <Webhook size={28} />
            </div>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "var(--text-secondary)",
                marginBottom: 4,
              }}
            >
              No webhook bindings configured
            </div>
            <div style={{ fontSize: 12, marginBottom: 16 }}>
              Add a binding to receive events from external services.
            </div>
            <Btn variant="primary" sm onClick={handleAdd}>
              + New Binding
            </Btn>
          </div>
        )}

      {/* Binding list */}
      {!loading && !pendingSecret && bindings.length > 0 && (
        <div>
          {bindings.map((binding) =>
            formMode === "edit" && formInitial?.id === binding.id ? null : (
              <WebhookCard
                key={binding.id}
                binding={binding}
                onEdit={() => handleEdit(binding)}
                onDelete={() => handleDeleteClick(binding)}
              />
            ),
          )}
          {formMode === "hidden" && (
            <div style={{ marginTop: 14 }}>
              <Btn variant="ghost" onClick={handleAdd}>
                <Plus size={13} />
                New Binding
              </Btn>
            </div>
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

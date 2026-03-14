import React, { useState, useEffect, useCallback, type FormEvent } from "react";
import { Plus, Pencil, Trash2, KeyRound, Eye, EyeOff } from "lucide-react";
import {
  credentialsApi,
  type Credential,
  type CreateCredentialPayload,
  type CredentialType,
} from "@/api/client";
import { Btn, FieldLabel, FieldInput, FieldSelect, FieldHint } from "./shared";

// ─── Types ────────────────────────────────────────────────────────────────────

interface DeleteState {
  credential: Credential;
  warnings: string[];
  deleting: boolean;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Convert an arbitrary string into a valid credential key.
 * Rules: lowercase, alphanumerics and underscores only, no leading/trailing
 * underscores, no consecutive underscores.
 *   "My GitHub PAT"       → "my_github_pat"
 *   "OpenAI  Production!" → "openai_production"
 */
function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_") // non-alphanumeric runs → single _
    .replace(/^_+|_+$/g, "") // strip leading/trailing _
    .replace(/__+/g, "_"); // collapse consecutive _ (belt-and-suspenders)
}

/** Sanitise a manually-typed key: same charset rules, applied on every keystroke. */
function sanitizeKey(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9_]/g, "") // drop anything not allowed
    .replace(/^_+/, "") // no leading underscores while typing
    .replace(/__+/g, "_"); // no consecutive underscores
}

// ─── Constants ────────────────────────────────────────────────────────────────

interface CredentialTypeConfig {
  /** Shown in the type dropdown and table badge. */
  label: string;
  /** Label for the primary secret field. */
  keyLabel: string;
  /** Placeholder for the primary secret field. */
  keyPlaceholder: string;
}

const CREDENTIAL_TYPE_CONFIG: Record<CredentialType, CredentialTypeConfig> = {
  api_key: {
    label: "API Key",
    keyLabel: "API Key",
    keyPlaceholder: "Paste API key…",
  },
  pat: {
    label: "Personal Access Token",
    keyLabel: "API Key",
    keyPlaceholder: "Paste token…",
  },
  bearer_token: {
    label: "Bearer Token",
    keyLabel: "API Key",
    keyPlaceholder: "Paste token…",
  },
  key_secret_pair: {
    label: "Key / Secret Pair",
    keyLabel: "API Key",
    keyPlaceholder: "Paste API key…",
  },
  service_account: {
    label: "Service Account",
    keyLabel: "Password",
    keyPlaceholder: "Paste password…",
  },
};

// ─── CredentialForm ───────────────────────────────────────────────────────────

interface CredentialFormProps {
  editing: Credential | null;
  onSaved: () => void;
  onCancel: () => void;
  onDelete?: (c: Credential) => void;
}

function CredentialForm({
  editing,
  onSaved,
  onCancel,
  onDelete,
}: CredentialFormProps) {
  const [key, setKey] = useState(editing?.key ?? "");
  const [keyTouched, setKeyTouched] = useState(false);
  const [displayName, setDisplayName] = useState(editing?.display_name ?? "");

  const [credentialType, setCredentialType] = useState<CredentialType>(
    (editing?.credential_type as CredentialType) ?? "api_key",
  );
  const [serviceUrl, setServiceUrl] = useState(editing?.service_url ?? "");
  const [username, setUsername] = useState(editing?.username ?? "");
  const [email, setEmail] = useState(editing?.email ?? "");
  const [secret, setSecret] = useState("");
  const [password, setPassword] = useState("");
  const [showSecret, setShowSecret] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isEditing = !!editing;

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!displayName.trim()) return setError("Display name is required.");
    if (!isEditing && !key.trim())
      return setError(
        "Display name is required to generate a key — please fill it in first.",
      );
    if (
      credentialType === "service_account" &&
      !username.trim() &&
      !email.trim()
    )
      return setError(
        "A service account must have at least a username or email address.",
      );

    setSaving(true);
    try {
      if (isEditing) {
        const payload: Record<string, string | undefined> = {
          display_name: displayName.trim(),
          credential_type: credentialType,
          service_url: serviceUrl.trim() || undefined,
          username: username.trim() || undefined,
          email: email.trim() || undefined,
        };
        if (secret) payload.secret = secret;
        if (password) payload.password = password;
        await credentialsApi.update(editing.id, payload);
      } else {
        const payload: CreateCredentialPayload = {
          key: key.trim(),
          display_name: displayName.trim(),
          credential_type: credentialType,
          service_url: serviceUrl.trim() || undefined,
          username: username.trim() || undefined,
          email: email.trim() || undefined,
          secret: secret || undefined,
          password: password || undefined,
        };
        await credentialsApi.create(payload);
      }
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed.");
    } finally {
      setSaving(false);
    }
  };

  const isKeySecretPair = credentialType === "key_secret_pair";
  const isServiceAccount = credentialType === "service_account";

  // Auto-generate the key from the display name when the user leaves the field,
  // but only if they haven't manually edited the key themselves.
  const handleDisplayNameBlur = () => {
    if (!isEditing && !keyTouched && displayName.trim()) {
      const generated = slugify(displayName.trim());
      if (generated) setKey(generated);
    }
  };

  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border-default)",
        borderRadius: 12,
        padding: "22px 24px",
        marginBottom: 24,
      }}
    >
      <div
        style={{
          fontSize: 15,
          fontWeight: 600,
          marginBottom: 18,
          color: "var(--text-primary)",
        }}
      >
        {isEditing ? "Edit Credential" : "Add Credential"}
      </div>

      <form onSubmit={handleSave}>
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}
        >
          {/* Display Name */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Display Name</FieldLabel>
            <FieldInput
              placeholder="e.g. My GitHub PAT"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              onBlur={handleDisplayNameBlur}
            />
          </div>

          {/* Credential Type */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Credential Type</FieldLabel>
            <FieldSelect
              value={credentialType}
              onChange={(e) =>
                setCredentialType(e.target.value as CredentialType)
              }
            >
              {(
                Object.entries(CREDENTIAL_TYPE_CONFIG) as [
                  CredentialType,
                  CredentialTypeConfig,
                ][]
              ).map(([val, cfg]) => (
                <option key={val} value={val}>
                  {cfg.label}
                </option>
              ))}
            </FieldSelect>
          </div>

          {/* Service URL — shown for all types */}
          <div
            style={{
              gridColumn: "1 / -1",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            <FieldLabel>Service URL (optional)</FieldLabel>
            <FieldInput
              mono
              placeholder="https://github.com"
              value={serviceUrl}
              onChange={(e) => setServiceUrl(e.target.value)}
            />
          </div>

          {/* Service account fields — only for service_account type */}
          {isServiceAccount && (
            <>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <FieldLabel>Service Account Username</FieldLabel>
                <FieldInput
                  placeholder="johndoe"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <FieldLabel>Service Account Email</FieldLabel>
                <FieldInput
                  placeholder="john@example.com"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                />
              </div>

              <div
                style={{
                  gridColumn: "1 / -1",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <FieldLabel>Service Account Password</FieldLabel>
                <div style={{ position: "relative" }}>
                  <FieldInput
                    mono
                    type={showPassword ? "text" : "password"}
                    placeholder={
                      isEditing
                        ? "Leave blank to keep existing"
                        : "Paste password…"
                    }
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    style={{ paddingRight: 38 }}
                  />
                  <button
                    type="button"
                    onClick={() => setShowPassword((v) => !v)}
                    tabIndex={-1}
                    style={{
                      position: "absolute",
                      right: 10,
                      top: "50%",
                      transform: "translateY(-50%)",
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      color: "var(--text-tertiary)",
                      display: "flex",
                      alignItems: "center",
                      padding: 0,
                    }}
                  >
                    {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                </div>
                <FieldHint>
                  Encrypted with AES-256-GCM the moment it's saved.
                </FieldHint>
              </div>
            </>
          )}

          {/* Machine-readable key — only on create */}
          {!isEditing && (
            <div
              style={{
                gridColumn: "1 / -1",
                display: "flex",
                flexDirection: "column",
                gap: 6,
              }}
            >
              <FieldLabel>Key</FieldLabel>
              <FieldInput
                mono
                placeholder="Generated from display name…"
                value={key}
                onChange={(e) => {
                  setKeyTouched(true);
                  setKey(sanitizeKey(e.target.value));
                }}
              />
              <FieldHint>
                Auto-generated from your display name. Only lowercase letters,
                numbers, and underscores are allowed. This is used internally to
                reference this credential from MCP server configs — you won't
                need to remember it.
              </FieldHint>
            </div>
          )}

          {/* Primary API key — hidden for service_account (password is the secret) */}
          {!isServiceAccount && (
            <div
              style={{
                gridColumn: "1 / -1",
                display: "flex",
                flexDirection: "column",
                gap: 6,
              }}
            >
              <FieldLabel>
                {CREDENTIAL_TYPE_CONFIG[credentialType].keyLabel}
              </FieldLabel>
              <div style={{ position: "relative" }}>
                <FieldInput
                  mono
                  type={showSecret ? "text" : "password"}
                  placeholder={
                    isEditing
                      ? "Leave blank to keep existing"
                      : CREDENTIAL_TYPE_CONFIG[credentialType].keyPlaceholder
                  }
                  value={secret}
                  onChange={(e) => setSecret(e.target.value)}
                  style={{ paddingRight: 38 }}
                />
                <button
                  type="button"
                  onClick={() => setShowSecret((v) => !v)}
                  tabIndex={-1}
                  style={{
                    position: "absolute",
                    right: 10,
                    top: "50%",
                    transform: "translateY(-50%)",
                    background: "none",
                    border: "none",
                    cursor: "pointer",
                    color: "var(--text-tertiary)",
                    display: "flex",
                    alignItems: "center",
                    padding: 0,
                  }}
                >
                  {showSecret ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
              <FieldHint>
                Encrypted with AES-256-GCM the moment it's saved.
              </FieldHint>
            </div>
          )}

          {/* API Secret — key_secret_pair only */}
          {isKeySecretPair && (
            <div
              style={{
                gridColumn: "1 / -1",
                display: "flex",
                flexDirection: "column",
                gap: 6,
              }}
            >
              <FieldLabel>API Secret</FieldLabel>
              <div style={{ position: "relative" }}>
                <FieldInput
                  mono
                  type={showPassword ? "text" : "password"}
                  placeholder={
                    isEditing
                      ? "Leave blank to keep existing"
                      : "Paste API secret…"
                  }
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  style={{ paddingRight: 38 }}
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  tabIndex={-1}
                  style={{
                    position: "absolute",
                    right: 10,
                    top: "50%",
                    transform: "translateY(-50%)",
                    background: "none",
                    border: "none",
                    cursor: "pointer",
                    color: "var(--text-tertiary)",
                    display: "flex",
                    alignItems: "center",
                    padding: 0,
                  }}
                >
                  {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
            </div>
          )}
        </div>

        {error && (
          <p style={{ marginTop: 10, fontSize: 12, color: "var(--error)" }}>
            {error}
          </p>
        )}

        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            alignItems: "center",
            gap: 8,
            marginTop: 18,
            paddingTop: 16,
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          {isEditing && onDelete && editing && (
            <Btn
              variant="danger"
              sm
              onClick={() => onDelete(editing)}
              style={{ marginRight: "auto" }}
            >
              <Trash2 size={12} />
              Delete
            </Btn>
          )}
          <Btn variant="ghost" onClick={onCancel}>
            Cancel
          </Btn>
          <button
            type="submit"
            disabled={saving}
            style={{
              padding: "8px 16px",
              borderRadius: 7,
              fontSize: 13,
              fontWeight: 500,
              cursor: saving ? "default" : "pointer",
              border: "none",
              background: "var(--accent-primary)",
              color: "var(--text-inverse)",
              opacity: saving ? 0.5 : 1,
              fontFamily: "inherit",
            }}
          >
            {saving ? "Saving…" : isEditing ? "Save Changes" : "Add Credential"}
          </button>
        </div>
      </form>
    </div>
  );
}

// ─── DeleteConfirmDialog ──────────────────────────────────────────────────────

interface DeleteConfirmProps {
  credential: Credential;
  onConfirm: () => void;
  onCancel: () => void;
  deleting: boolean;
  warnings: string[];
}

function DeleteConfirmDialog({
  credential,
  onConfirm,
  onCancel,
  deleting,
  warnings,
}: DeleteConfirmProps) {
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 300,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(4px)",
      }}
    >
      <div
        style={{
          background: "var(--bg-secondary)",
          border: "1px solid var(--border-default)",
          borderRadius: 12,
          padding: "24px 28px",
          width: 400,
          maxWidth: "calc(100vw - 48px)",
        }}
      >
        <div
          style={{
            fontSize: 15,
            fontWeight: 600,
            color: "var(--text-primary)",
            marginBottom: 8,
          }}
        >
          Delete credential?
        </div>
        <p style={{ fontSize: 13, color: "var(--text-secondary)", margin: 0 }}>
          <strong>{credential.display_name}</strong> ({credential.key}) will be
          permanently deleted.
        </p>

        {warnings.length > 0 && (
          <div
            style={{
              marginTop: 14,
              background: "rgba(196,162,74,0.10)",
              border: "1px solid rgba(196,162,74,0.35)",
              borderRadius: 8,
              padding: "10px 12px",
            }}
          >
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--warning)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                marginBottom: 6,
              }}
            >
              ⚠ Referenced by
            </div>
            {warnings.map((w) => (
              <div
                key={w}
                style={{ fontSize: 12, color: "var(--text-secondary)" }}
              >
                {w}
              </div>
            ))}
          </div>
        )}

        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            marginTop: 20,
          }}
        >
          <Btn variant="ghost" onClick={onCancel} disabled={deleting}>
            Cancel
          </Btn>
          <Btn variant="danger" onClick={onConfirm} disabled={deleting}>
            {deleting ? "Deleting…" : "Delete"}
          </Btn>
        </div>
      </div>
    </div>
  );
}

// ─── CredentialRow ────────────────────────────────────────────────────────────

interface CredentialRowProps {
  credential: Credential;
  onEdit: () => void;
}

function CredentialRow({ credential, onEdit }: CredentialRowProps) {
  const formattedDate = new Date(credential.created_at).toLocaleDateString(
    undefined,
    { year: "numeric", month: "short", day: "numeric" },
  );

  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "1fr 160px 80px 90px 60px",
        alignItems: "center",
        gap: 12,
        padding: "12px 14px",
      }}
    >
      {/* Display name + key */}
      <div style={{ minWidth: 0 }}>
        <div
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {credential.display_name}
        </div>
        <div
          style={{
            fontSize: 11,
            color: "var(--text-tertiary)",
            fontFamily: '"SF Mono","Fira Code",monospace',
            marginTop: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {credential.key}
        </div>
      </div>

      {/* Type */}
      <div
        style={{
          fontSize: 11,
          color: "var(--text-secondary)",
          whiteSpace: "nowrap",
        }}
      >
        {CREDENTIAL_TYPE_CONFIG[credential.credential_type as CredentialType]
          ?.label ?? credential.credential_type}
      </div>

      {/* Masked secret */}
      <div
        style={{
          fontSize: 13,
          color: "var(--text-tertiary)",
          letterSpacing: "0.12em",
          fontFamily: '"SF Mono","Fira Code",monospace',
        }}
      >
        ••••••••
      </div>

      {/* Created date */}
      <div
        style={{
          fontSize: 11,
          color: "var(--text-tertiary)",
          whiteSpace: "nowrap",
        }}
      >
        {formattedDate}
      </div>

      {/* Edit action */}
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <button
          type="button"
          onClick={onEdit}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            background: "none",
            border: "none",
            cursor: "pointer",
            color: "var(--text-tertiary)",
            fontSize: 12,
            padding: "4px 8px",
            borderRadius: 6,
            fontFamily: "inherit",
          }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.color = "var(--text-primary)")
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.color = "var(--text-tertiary)")
          }
        >
          <Pencil size={11} />
          Edit
        </button>
      </div>
    </div>
  );
}

// ─── CredentialTable ──────────────────────────────────────────────────────────

interface CredentialTableProps {
  credentials: Credential[];
  onEdit: (c: Credential) => void;
}

function CredentialTable({ credentials, onEdit }: CredentialTableProps) {
  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        overflow: "hidden",
      }}
    >
      {/* Table header */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 160px 80px 90px 60px",
          gap: 12,
          padding: "8px 14px",
          borderBottom: "1px solid var(--border-subtle)",
          background: "var(--bg-tertiary)",
        }}
      >
        {["Name / Key", "Type", "Secret", "Created", ""].map((h) => (
          <div
            key={h}
            style={{
              fontSize: 10,
              fontWeight: 600,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.08em",
            }}
          >
            {h}
          </div>
        ))}
      </div>

      {/* Rows */}
      <div style={{ position: "relative" }}>
        {credentials.map((c, i) => (
          <div
            key={c.id}
            style={{
              borderBottom:
                i < credentials.length - 1
                  ? "1px solid var(--border-subtle)"
                  : undefined,
              position: "relative",
            }}
          >
            <CredentialRow credential={c} onEdit={() => onEdit(c)} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── CredentialsSettings ──────────────────────────────────────────────────────

export function CredentialsSettings() {
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<Credential | null>(null);
  const [deleteState, setDeleteState] = useState<DeleteState | null>(null);

  const loadCredentials = useCallback(async () => {
    setLoading(true);
    try {
      const data = await credentialsApi.list();
      setCredentials(data);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadCredentials();
  }, [loadCredentials]);

  const handleSaved = useCallback(async () => {
    setShowForm(false);
    setEditing(null);
    await loadCredentials();
  }, [loadCredentials]);

  const handleEdit = useCallback((c: Credential) => {
    setEditing(c);
    setShowForm(true);
  }, []);

  const handleDeleteRequest = useCallback((c: Credential) => {
    setDeleteState({ credential: c, warnings: [], deleting: false });
  }, []);

  const handleDeleteConfirm = useCallback(async () => {
    if (!deleteState) return;
    setDeleteState((s) => s && { ...s, deleting: true });
    try {
      const result = await credentialsApi.delete(deleteState.credential.id);
      const warnings =
        result && typeof result === "object" && "warnings" in result
          ? (result.warnings ?? [])
          : [];
      if (warnings.length > 0) {
        setDeleteState((s) => s && { ...s, warnings, deleting: false });
      } else {
        setDeleteState(null);
        setShowForm(false);
        setEditing(null);
      }
      await loadCredentials();
    } catch (err) {
      console.error("Delete failed:", err);
      setDeleteState((s) => s && { ...s, deleting: false });
    }
  }, [deleteState, loadCredentials]);

  const openAdd = useCallback(() => {
    setEditing(null);
    setShowForm(true);
  }, []);

  const cancelForm = useCallback(() => {
    setShowForm(false);
    setEditing(null);
  }, []);

  return (
    <div>
      {/* Page header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 24,
        }}
      >
        <div>
          <div
            style={{ fontSize: 18, fontWeight: 700, letterSpacing: "-0.01em" }}
          >
            Credentials
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            A secure store for API keys, tokens, and passwords. Secrets are
            encrypted on the server the moment you save them — they're never
            returned in plain text, and you won't need to handle them again. MCP
            servers and providers reference credentials by name.
          </div>
        </div>
      </div>

      {/* Add / edit form */}
      {showForm && (
        <CredentialForm
          editing={editing}
          onSaved={handleSaved}
          onCancel={cancelForm}
          onDelete={handleDeleteRequest}
        />
      )}

      {/* Content */}
      {loading ? (
        <div
          style={{
            padding: "48px 0",
            textAlign: "center",
            fontSize: 13,
            color: "var(--text-tertiary)",
          }}
        >
          Loading…
        </div>
      ) : credentials.length === 0 && !showForm ? (
        <div
          style={{
            padding: "60px 20px",
            textAlign: "center",
            color: "var(--text-tertiary)",
          }}
        >
          <div style={{ fontSize: 40, marginBottom: 14, opacity: 0.2 }}>
            <KeyRound size={40} style={{ margin: "0 auto" }} />
          </div>
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginBottom: 6,
            }}
          >
            No credentials yet
          </div>
          <div style={{ fontSize: 13, marginBottom: 18 }}>
            Store API keys, PATs, and bearer tokens here. MCP servers can
            reference them by key.
          </div>
          <Btn variant="primary" onClick={openAdd}>
            <Plus size={14} />
            Add Credential
          </Btn>
        </div>
      ) : (
        !showForm &&
        credentials.length > 0 && (
          <>
            <CredentialTable credentials={credentials} onEdit={handleEdit} />
            <div style={{ marginTop: 14 }}>
              <Btn variant="ghost" onClick={openAdd}>
                <Plus size={13} />
                Add Credential
              </Btn>
            </div>
          </>
        )
      )}

      {/* Delete confirmation dialog */}
      {deleteState && (
        <DeleteConfirmDialog
          credential={deleteState.credential}
          warnings={deleteState.warnings}
          deleting={deleteState.deleting}
          onConfirm={handleDeleteConfirm}
          onCancel={() => setDeleteState(null)}
        />
      )}
    </div>
  );
}

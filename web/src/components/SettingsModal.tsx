import React, {
  useState,
  useEffect,
  useCallback,
  useRef,
  type FormEvent,
} from "react";
import {
  X,
  Plus,
  Zap,
  RefreshCw,
  Pencil,
  Trash2,
  ExternalLink,
  CheckCircle2,
  Loader2,
} from "lucide-react";
import type { Provider, Model, AgentPersona } from "@/types";
import { providersApi, modelsApi, personasApi, copilotApi } from "@/api/client";

// ─── Types ────────────────────────────────────────────────────────────────────

type SettingsTab = "providers" | "personas";

type ProviderForm = {
  name: string;
  kind: "api_key" | "copilot" | "";
  base_url: string;
  api_key: string;
};

type CopilotAuthStep =
  | { stage: "idle" }
  | { stage: "checking" }
  | { stage: "authenticated" }
  | {
      stage: "authorizing";
      userCode: string;
      verificationUri: string;
      deviceCode: string;
      expiresAt: number;
    }
  | { stage: "error"; message: string };

type PersonaForm = {
  name: string;
  emoji: string;
  system_prompt: string;
  default_provider: string;
  default_model: string;
};

// ─── Constants ────────────────────────────────────────────────────────────────

// Only two provider kinds:
//   api_key  — any OpenAI-compatible endpoint (OpenAI, Anthropic, local, custom)
//   copilot  — GitHub Copilot via the vendored copilot-api sidecar

const KIND_DEFAULT_URLS: Record<string, string> = {
  api_key: "https://api.openai.com/v1",
  copilot: "http://localhost:4141/v1",
};

const KIND_ICONS: Record<string, string> = {
  api_key: "🔑",
  copilot: "🐙",
};

// Friendly label hints shown beneath the base-url field
const KIND_URL_HINTS: Record<string, string> = {
  api_key:
    "OpenAI-compatible endpoint — e.g. https://api.openai.com/v1, https://api.anthropic.com/v1, or a local URL.",
  copilot: "Managed automatically by the copilot-api sidecar.",
};

const EMOJI_PALETTE = [
  "🦉",
  "🧠",
  "🎨",
  "⚒️",
  "🔮",
  "🦊",
  "🐉",
  "⚡",
  "🤖",
  "🧬",
  "🛸",
  "🌊",
  "🔥",
  "🌿",
  "🎯",
  "💡",
];

const EMOJI_CATEGORIES: { label: string; emojis: string[] }[] = [
  {
    label: "Smileys",
    emojis: [
      "😀",
      "😂",
      "😍",
      "🥰",
      "😎",
      "🤔",
      "😤",
      "🥳",
      "😇",
      "🤩",
      "😜",
      "🤯",
      "😴",
      "🥺",
      "😱",
      "🤖",
    ],
  },
  {
    label: "Animals",
    emojis: [
      "🐶",
      "🐱",
      "🐭",
      "🐹",
      "🐰",
      "🦊",
      "🐻",
      "🐼",
      "🐨",
      "🐯",
      "🦁",
      "🐮",
      "🐷",
      "🐸",
      "🐵",
      "🦉",
      "🦅",
      "🐉",
      "🦋",
      "🐙",
      "🦑",
      "🐬",
      "🦈",
      "🐺",
      "🦝",
      "🦚",
      "🦜",
      "🐝",
    ],
  },
  {
    label: "People",
    emojis: [
      "👤",
      "👥",
      "🧑",
      "👩",
      "👨",
      "🧙",
      "🧝",
      "🧛",
      "🧟",
      "🧞",
      "🧜",
      "🧚",
      "👷",
      "🕵️",
      "👩‍💻",
      "👨‍💻",
      "👩‍🎨",
      "👩‍🔬",
      "🧑‍🚀",
      "👩‍⚕️",
    ],
  },
  {
    label: "Nature",
    emojis: [
      "🌿",
      "🌱",
      "🌲",
      "🌳",
      "🌴",
      "🌵",
      "🎋",
      "🍀",
      "🌾",
      "🌺",
      "🌸",
      "🌼",
      "🌻",
      "🌙",
      "⭐",
      "🌟",
      "💫",
      "☀️",
      "🌈",
      "❄️",
      "🌊",
      "🔥",
      "⚡",
      "🌪️",
    ],
  },
  {
    label: "Objects",
    emojis: [
      "💡",
      "🔮",
      "🧬",
      "🛸",
      "⚒️",
      "🔧",
      "🔑",
      "🗝️",
      "🔐",
      "📡",
      "💻",
      "⌨️",
      "🖥️",
      "📱",
      "🎯",
      "🎲",
      "🧩",
      "🎮",
      "🕹️",
      "🎸",
      "🎺",
      "🎨",
      "✏️",
      "📝",
      "📚",
      "🔭",
      "🔬",
      "⚗️",
      "🧪",
      "💊",
    ],
  },
  {
    label: "Symbols",
    emojis: [
      "❤️",
      "🧡",
      "💛",
      "💚",
      "💙",
      "💜",
      "🖤",
      "🤍",
      "💥",
      "✨",
      "🌀",
      "♾️",
      "☯️",
      "⚛️",
      "🔱",
      "⚜️",
      "🏆",
      "🎖️",
      "🥇",
      "⭐",
      "🌟",
      "💎",
      "👑",
      "🎭",
      "🎪",
      "🎠",
      "🚀",
      "💣",
      "⚡",
      "🔥",
    ],
  },
];

// ─── Shared primitives — sizes/colors taken directly from mockup CSS ──────────

/** field-label: 11px, 600, text-secondary, uppercase, 0.06em tracking */
function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <label
      style={{
        fontSize: 11,
        fontWeight: 600,
        color: "var(--text-secondary)",
        textTransform: "uppercase",
        letterSpacing: "0.06em",
      }}
    >
      {children}
    </label>
  );
}

/** field-input / field-select: bg-tertiary, border-default, 7px radius, 9px 12px padding, 13px */
const fieldBase: React.CSSProperties = {
  background: "var(--bg-tertiary)",
  border: "1px solid var(--border-default)",
  borderRadius: 7,
  padding: "9px 12px",
  color: "var(--text-primary)",
  fontSize: 13,
  fontFamily: "inherit",
  outline: "none",
  width: "100%",
  transition: "border-color 0.15s, box-shadow 0.15s",
};

function FieldInput(
  props: React.InputHTMLAttributes<HTMLInputElement> & { mono?: boolean },
) {
  const { mono, style, onFocus, onBlur, ...rest } = props;
  const [focused, setFocused] = useState(false);
  return (
    <input
      {...rest}
      style={{
        ...fieldBase,
        ...(mono
          ? {
              fontFamily: '"SF Mono","Fira Code",monospace',
              fontSize: 12,
              letterSpacing: "0.03em",
            }
          : {}),
        ...(focused
          ? {
              borderColor: "var(--accent-primary)",
              boxShadow: "0 0 0 3px rgba(124,140,90,0.15)",
            }
          : {}),
        ...style,
      }}
      onFocus={(e) => {
        setFocused(true);
        onFocus?.(e);
      }}
      onBlur={(e) => {
        setFocused(false);
        onBlur?.(e);
      }}
    />
  );
}

function FieldSelect(props: React.SelectHTMLAttributes<HTMLSelectElement>) {
  const { style, onFocus, onBlur, ...rest } = props;
  const [focused, setFocused] = useState(false);
  return (
    <select
      {...rest}
      style={{
        ...fieldBase,
        cursor: "pointer",
        ...(focused
          ? {
              borderColor: "var(--accent-primary)",
              boxShadow: "0 0 0 3px rgba(124,140,90,0.15)",
            }
          : {}),
        ...style,
      }}
      onFocus={(e) => {
        setFocused(true);
        onFocus?.(e);
      }}
      onBlur={(e) => {
        setFocused(false);
        onBlur?.(e);
      }}
    />
  );
}

function FieldTextarea(
  props: React.TextareaHTMLAttributes<HTMLTextAreaElement>,
) {
  const { style, onFocus, onBlur, ...rest } = props;
  const [focused, setFocused] = useState(false);
  return (
    <textarea
      {...rest}
      style={{
        ...fieldBase,
        resize: "vertical",
        minHeight: 100,
        lineHeight: 1.55,
        ...(focused
          ? {
              borderColor: "var(--accent-primary)",
              boxShadow: "0 0 0 3px rgba(124,140,90,0.15)",
            }
          : {}),
        ...style,
      }}
      onFocus={(e) => {
        setFocused(true);
        onFocus?.(e);
      }}
      onBlur={(e) => {
        setFocused(false);
        onBlur?.(e);
      }}
    />
  );
}

function FieldHint({ children }: { children: React.ReactNode }) {
  return (
    <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
      {children}
    </span>
  );
}

/** btn: 8px 16px, 7px radius, 13px, 500 weight */
function Btn({
  children,
  variant = "ghost",
  sm,
  style,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "ghost" | "danger";
  sm?: boolean;
}) {
  const [hovered, setHovered] = useState(false);

  const base: React.CSSProperties = {
    padding: sm ? "5px 11px" : "8px 16px",
    borderRadius: 7,
    fontSize: sm ? 12 : 13,
    fontWeight: 500,
    cursor: props.disabled ? "default" : "pointer",
    border: "none",
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    transition: "background 0.15s, color 0.15s",
    opacity: props.disabled ? 0.5 : 1,
    fontFamily: "inherit",
  };

  const variants: Record<string, React.CSSProperties> = {
    primary: {
      background: hovered ? "var(--accent-secondary)" : "var(--accent-primary)",
      color: "var(--text-inverse)",
      border: "none",
    },
    ghost: {
      background: hovered ? "var(--bg-elevated)" : "transparent",
      color: hovered ? "var(--text-primary)" : "var(--text-secondary)",
      border: "1px solid var(--border-default)",
    },
    danger: {
      background: hovered ? "rgba(196,90,90,0.10)" : "transparent",
      color: "var(--error)",
      border: "1px solid rgba(196,90,90,0.35)",
    },
  };

  return (
    <button
      type="button"
      {...props}
      style={{ ...base, ...variants[variant], ...style }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {children}
    </button>
  );
}

// ─── Status badge — matches .status-badge / .badge-* from mockup ──────────────

function StatusBadge({
  status,
}: {
  status: "connected" | "pending" | "error";
}) {
  const cfg = {
    connected: {
      bg: "rgba(106,158,91,0.15)",
      color: "var(--success)",
      dot: "var(--success)",
      label: "Connected",
      blink: false,
    },
    pending: {
      bg: "rgba(196,162,74,0.12)",
      color: "var(--warning)",
      dot: "var(--warning)",
      label: "Auth required",
      blink: true,
    },
    error: {
      bg: "rgba(196,90,90,0.12)",
      color: "var(--error)",
      dot: "var(--error)",
      label: "Auth error",
      blink: false,
    },
  }[status];

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "2px 9px 2px 6px",
        borderRadius: 20,
        fontSize: 11,
        fontWeight: 600,
        background: cfg.bg,
        color: cfg.color,
        flexShrink: 0,
      }}
    >
      <span
        style={{
          width: 6,
          height: 6,
          borderRadius: "50%",
          background: cfg.dot,
          display: "inline-block",
        }}
      />
      {cfg.label}
    </span>
  );
}

// ─── Provider add/edit form — matches .form-card from mockup ──────────────────

interface ProviderFormPanelProps {
  editing: Provider | null;
  onSaved: () => void;
  onCancel: () => void;
}

function ProviderFormPanel({
  editing,
  onSaved,
  onCancel,
}: ProviderFormPanelProps) {
  // Normalise legacy kind values saved before the two-kind model
  const normaliseKind = (k: string): "api_key" | "copilot" | "" => {
    if (k === "copilot") return "copilot";
    if (k === "api_key") return "api_key";
    if (k === "openai" || k === "anthropic" || k === "custom") return "api_key";
    return "";
  };

  const [form, setForm] = useState<ProviderForm>({
    name: editing?.name ?? "",
    kind: normaliseKind(editing?.kind ?? ""),
    base_url: editing?.base_url ?? "",
    api_key: "",
  });
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    models: string[];
    error?: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const setField =
    (k: keyof ProviderForm) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => {
      const val = e.target.value;
      setForm((prev) => {
        const next = { ...prev, [k]: val } as ProviderForm;
        if (k === "kind" && !editing)
          next.base_url = KIND_DEFAULT_URLS[val] ?? "";
        return next;
      });
      setTestResult(null);
    };

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!form.name.trim()) return setError("Name is required.");
    if (!form.kind) return setError("Provider kind is required.");
    if (form.kind !== "copilot" && !form.base_url.trim())
      return setError("Base URL is required.");
    setSaving(true);
    try {
      const payload = {
        name: form.name.trim(),
        kind: form.kind,
        base_url:
          form.kind === "copilot"
            ? KIND_DEFAULT_URLS.copilot
            : form.base_url.trim(),
        ...(form.kind !== "copilot" && form.api_key
          ? { api_key: form.api_key }
          : {}),
      };
      if (editing) {
        await providersApi.update(editing.id, payload);
      } else {
        await providersApi.create(payload);
      }
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed.");
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    if (!editing) return;
    setTesting(true);
    setTestResult(null);
    try {
      const res = await providersApi.test(editing.id);
      setTestResult({ models: res.data.models });
    } catch (err) {
      setTestResult({
        models: [],
        error: err instanceof Error ? err.message : "Connection failed.",
      });
    } finally {
      setTesting(false);
    }
  };

  const isCopilot = form.kind === "copilot";

  // form-card: bg-secondary, border-default, 12px radius, 22px 24px padding, mb 24px
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
      {/* form-title */}
      <div
        style={{
          fontSize: 15,
          fontWeight: 600,
          marginBottom: 18,
          color: "var(--text-primary)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span>{editing ? "Edit Provider" : "Add Provider"}</span>
        <Btn sm variant="ghost" onClick={onCancel}>
          ✕ Cancel
        </Btn>
      </div>

      <form onSubmit={handleSave}>
        {/* form-grid: 2 cols, 14px gap */}
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}
        >
          {/* Name */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Name</FieldLabel>
            <FieldInput
              placeholder={isCopilot ? "e.g. GitHub Copilot" : "e.g. My OpenAI"}
              value={form.name}
              onChange={setField("name")}
            />
          </div>

          {/* Kind — two options only */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Kind</FieldLabel>
            <FieldSelect
              value={form.kind}
              onChange={setField("kind")}
              disabled={!!editing}
            >
              <option value="">Select provider type…</option>
              <option value="api_key">
                🔑 API Key (OpenAI, Anthropic, custom…)
              </option>
              <option value="copilot">🐙 GitHub Copilot</option>
            </FieldSelect>
            {editing && (
              <FieldHint>Kind cannot be changed after creation.</FieldHint>
            )}
          </div>

          {/* API Key provider — base URL + key */}
          {form.kind === "api_key" && (
            <>
              <div
                style={{
                  gridColumn: "1 / -1",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <FieldLabel>Base URL</FieldLabel>
                <FieldInput
                  mono
                  placeholder="https://api.openai.com/v1"
                  value={form.base_url}
                  onChange={setField("base_url")}
                />
                <FieldHint>{KIND_URL_HINTS.api_key}</FieldHint>
              </div>

              <div
                style={{
                  gridColumn: "1 / -1",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <FieldLabel>API Key</FieldLabel>
                <FieldInput
                  mono
                  type="password"
                  placeholder={
                    editing ? "Leave blank to keep existing" : "sk-…"
                  }
                  value={form.api_key}
                  onChange={setField("api_key")}
                />
                <FieldHint>
                  Encrypted at rest. Never returned in API responses.
                </FieldHint>
              </div>
            </>
          )}

          {/* Copilot provider — no URL/key fields, show auth widget instead */}
          {form.kind === "copilot" && (
            <div style={{ gridColumn: "1 / -1" }}>
              <CopilotAuthSection />
            </div>
          )}
        </div>

        {error && (
          <p style={{ marginTop: 10, fontSize: 12, color: "var(--error)" }}>
            {error}
          </p>
        )}

        {/* form-actions */}
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            marginTop: 18,
            paddingTop: 16,
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          {editing && form.kind !== "copilot" && (
            <Btn
              variant="ghost"
              onClick={handleTest}
              disabled={testing}
              style={{ marginRight: "auto" }}
            >
              {testing ? (
                <RefreshCw
                  size={13}
                  style={{ animation: "spin 0.8s linear infinite" }}
                />
              ) : (
                <Zap size={13} />
              )}
              {testing ? "Testing…" : "Test Connection"}
            </Btn>
          )}
          <Btn variant="ghost" onClick={onCancel}>
            Cancel
          </Btn>
          <button
            type="submit"
            disabled={saving || !form.kind}
            style={{
              padding: "8px 16px",
              borderRadius: 7,
              fontSize: 13,
              fontWeight: 500,
              cursor: saving || !form.kind ? "default" : "pointer",
              border: "none",
              background: "var(--accent-primary)",
              color: "var(--text-inverse)",
              opacity: saving || !form.kind ? 0.5 : 1,
              fontFamily: "inherit",
            }}
          >
            {saving ? "Saving…" : "Save Provider"}
          </button>
        </div>

        {/* Test result */}
        {testResult && (
          <div
            style={{
              display: "block",
              background: "var(--bg-tertiary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 8,
              padding: "12px 14px",
              marginTop: 10,
            }}
          >
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                marginBottom: 8,
              }}
            >
              {testResult.error ? "Connection Failed" : "Available Models"}
            </div>
            {testResult.error ? (
              <span style={{ fontSize: 13, color: "var(--error)" }}>
                {testResult.error}
              </span>
            ) : testResult.models.length === 0 ? (
              <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                No models returned.
              </span>
            ) : (
              <div>
                {testResult.models.slice(0, 12).map((m) => (
                  <span
                    key={m}
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      background: "var(--bg-elevated)",
                      border: "1px solid var(--border-subtle)",
                      borderRadius: 5,
                      padding: "3px 9px",
                      fontSize: 12,
                      color: "var(--text-secondary)",
                      margin: "3px 3px 3px 0",
                      fontFamily: '"SF Mono","Fira Code",monospace',
                    }}
                  >
                    {m}
                  </span>
                ))}
                {testResult.models.length > 12 && (
                  <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                    +{testResult.models.length - 12} more
                  </span>
                )}
              </div>
            )}
          </div>
        )}
      </form>
    </div>
  );
}

// ─── Copilot auth section — inline GitHub device-auth flow ────────────────────

function CopilotAuthSection() {
  const [auth, setAuth] = useState<CopilotAuthStep>({ stage: "idle" });
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // Use a ref so the interval callback always sees the latest stage without stale closure
  const authRef = useRef<CopilotAuthStep>(auth);
  useEffect(() => {
    authRef.current = auth;
  }, [auth]);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  // Check whether a token is already stored on mount
  useEffect(() => {
    let cancelled = false;
    setAuth({ stage: "checking" });
    copilotApi
      .authStatus()
      .then((res) => {
        if (cancelled) return;
        setAuth(
          res.data.authenticated
            ? { stage: "authenticated" }
            : { stage: "idle" },
        );
      })
      .catch(() => {
        if (!cancelled) setAuth({ stage: "idle" });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => () => stopPolling(), [stopPolling]);

  const startAuth = useCallback(async () => {
    stopPolling();
    setAuth({ stage: "checking" });
    try {
      const res = await copilotApi.authStart();
      const d = res.data;
      const expiresAt = Date.now() + d.expires_in * 1000;

      const authorizingState: CopilotAuthStep = {
        stage: "authorizing",
        userCode: d.user_code,
        verificationUri: d.verification_uri,
        deviceCode: d.device_code,
        expiresAt,
      };
      setAuth(authorizingState);
      authRef.current = authorizingState;

      const intervalMs = (d.interval ?? 5) * 1000;
      pollRef.current = setInterval(async () => {
        const current = authRef.current;
        // Stop if we've already moved out of the authorizing stage
        if (current.stage !== "authorizing") {
          stopPolling();
          return;
        }

        if (Date.now() > current.expiresAt) {
          stopPolling();
          setAuth({
            stage: "error",
            message: "Authentication timed out. Please try again.",
          });
          return;
        }

        try {
          const pollRes = await copilotApi.authPoll(current.deviceCode);
          if (pollRes.data.authenticated) {
            stopPolling();
            setAuth({ stage: "authenticated" });
          }
          // "authorization_pending" or "slow_down" — keep waiting
        } catch {
          // network hiccup — keep polling
        }
      }, intervalMs);
    } catch (err) {
      setAuth({
        stage: "error",
        message:
          err instanceof Error
            ? err.message
            : "Failed to start authentication.",
      });
    }
  }, [stopPolling]);

  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "16px 18px",
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      {/* Header row */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ fontSize: 22 }}>🐙</span>
        <div>
          <div
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            GitHub Copilot
          </div>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              marginTop: 1,
            }}
          >
            Authenticate with your GitHub account to use Copilot models.
          </div>
        </div>
        {/* Status badge */}
        <div style={{ marginLeft: "auto" }}>
          {auth.stage === "authenticated" && (
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                background: "rgba(106,158,91,0.15)",
                color: "var(--success)",
                border: "1px solid rgba(106,158,91,0.3)",
                borderRadius: 20,
                padding: "3px 10px",
                fontSize: 12,
                fontWeight: 600,
              }}
            >
              <CheckCircle2 size={13} /> Authenticated
            </span>
          )}
          {(auth.stage === "checking" || auth.stage === "authorizing") && (
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                color: "var(--text-tertiary)",
                fontSize: 12,
              }}
            >
              <Loader2
                size={13}
                style={{ animation: "spin 0.8s linear infinite" }}
              />
              {auth.stage === "checking" ? "Checking…" : "Waiting for GitHub…"}
            </span>
          )}
        </div>
      </div>

      {/* Device-code card — shown while authorizing */}
      {auth.stage === "authorizing" && (
        <div
          style={{
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-default)",
            borderRadius: 8,
            padding: "14px 16px",
            display: "flex",
            flexDirection: "column",
            gap: 10,
          }}
        >
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              lineHeight: 1.5,
            }}
          >
            Visit the link below and enter the code to authenticate:
          </div>

          {/* User code */}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <code
              style={{
                fontSize: 22,
                fontFamily: '"SF Mono","Fira Code",monospace',
                fontWeight: 700,
                letterSpacing: "0.15em",
                color: "var(--accent-secondary)",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border-default)",
                borderRadius: 6,
                padding: "6px 14px",
                flex: 1,
                textAlign: "center",
              }}
            >
              {auth.userCode}
            </code>
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(auth.userCode)}
              title="Copy code"
              style={{
                background: "var(--bg-secondary)",
                border: "1px solid var(--border-default)",
                borderRadius: 6,
                padding: "6px 10px",
                fontSize: 12,
                color: "var(--text-secondary)",
                cursor: "pointer",
                fontFamily: "inherit",
                flexShrink: 0,
              }}
            >
              Copy
            </button>
          </div>

          {/* Verification link */}
          <a
            href={auth.verificationUri}
            target="_blank"
            rel="noopener noreferrer"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              fontSize: 13,
              color: "var(--accent-secondary)",
              textDecoration: "none",
              fontWeight: 500,
            }}
          >
            <ExternalLink size={13} />
            {auth.verificationUri}
          </a>

          <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            This page will update automatically once you approve access on
            GitHub.
          </div>
        </div>
      )}

      {/* Error state */}
      {auth.stage === "error" && (
        <div style={{ fontSize: 13, color: "var(--error)" }}>
          ⚠ {auth.message}
        </div>
      )}

      {/* Action buttons */}
      {auth.stage === "authenticated" ? (
        <Btn sm variant="ghost" onClick={startAuth}>
          <RefreshCw size={12} /> Re-authenticate
        </Btn>
      ) : auth.stage === "idle" || auth.stage === "error" ? (
        <Btn sm variant="primary" onClick={startAuth}>
          Connect GitHub Account
        </Btn>
      ) : null}
    </div>
  );
}

// ─── Provider card — matches .provider-card from mockup ───────────────────────

interface ProviderCardProps {
  provider: Provider;
  models: Model[];
  onEdit: () => void;
  onDelete: () => void;
  onSyncModels: () => void;
  syncingModels: boolean;
}

function ProviderCard({
  provider,
  models,
  onEdit,
  onDelete,
  onSyncModels,
  syncingModels,
}: ProviderCardProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [hovered, setHovered] = useState(false);
  const enabledCount = models.filter((m) => m.enabled).length;
  const icon = KIND_ICONS[provider.kind] ?? "🔌";

  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: `1px solid ${hovered ? "var(--border-default)" : "var(--border-subtle)"}`,
        borderRadius: 11,
        padding: "16px 18px",
        transition: "border-color 0.15s",
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* provider-card-header: flex, gap 12, mb 12 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          marginBottom: 12,
        }}
      >
        {/* provider-icon: 40×40, 9px radius, bg-elevated, 20px emoji */}
        <div
          style={{
            width: 40,
            height: 40,
            borderRadius: 9,
            background: "var(--bg-elevated)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 20,
            flexShrink: 0,
          }}
        >
          {icon}
        </div>

        {/* provider-meta */}
        <div style={{ flex: 1, minWidth: 0 }}>
          {/* provider-name: 15px, 600, flex, gap 8 */}
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-primary)",
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            {provider.name}
            <StatusBadge status={provider.enabled ? "connected" : "pending"} />
          </div>
          {/* provider-kind: 11px, text-tertiary, mt 1 */}
          <div
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              marginTop: 1,
            }}
          >
            {provider.kind}
          </div>
          {/* provider-url: 12px, monospace, mt 2, truncate */}
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              fontFamily: '"SF Mono","Fira Code",monospace',
              marginTop: 2,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {provider.base_url}
          </div>
        </div>
      </div>

      {/* Model chips */}
      {models.length > 0 && (
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "4px 6px",
            marginBottom: 10,
          }}
        >
          {models.slice(0, 6).map((m) => (
            <span
              key={m.id}
              style={{
                display: "inline-flex",
                alignItems: "center",
                background: m.enabled
                  ? "rgba(74,82,53,0.35)"
                  : "var(--bg-elevated)",
                border: `1px solid ${m.enabled ? "var(--accent-muted)" : "var(--border-subtle)"}`,
                borderRadius: 5,
                padding: "2px 8px",
                fontSize: 11,
                color: m.enabled
                  ? "var(--text-secondary)"
                  : "var(--text-tertiary)",
                fontFamily: '"SF Mono","Fira Code",monospace',
                opacity: m.enabled ? 1 : 0.6,
              }}
            >
              {m.display_name}
            </span>
          ))}
          {models.length > 6 && (
            <span
              style={{
                fontSize: 11,
                color: "var(--text-tertiary)",
                padding: "2px 4px",
              }}
            >
              +{models.length - 6} more
            </span>
          )}
        </div>
      )}

      {/* provider-actions: flex, gap 8, pt 10, border-top subtle */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          paddingTop: 10,
          borderTop: "1px solid var(--border-subtle)",
        }}
      >
        {/* provider-model-count */}
        <span style={{ flex: 1, fontSize: 12, color: "var(--text-tertiary)" }}>
          {models.length === 0
            ? "No models synced"
            : `${enabledCount} / ${models.length} model${models.length !== 1 ? "s" : ""} enabled`}
        </span>
        <Btn sm variant="ghost" onClick={onSyncModels} disabled={syncingModels}>
          <RefreshCw
            size={12}
            style={
              syncingModels
                ? { animation: "spin 0.8s linear infinite" }
                : undefined
            }
          />
          {syncingModels ? "Syncing…" : "Sync Models"}
        </Btn>
        <Btn sm variant="ghost" onClick={onEdit}>
          <Pencil size={12} />
          Edit
        </Btn>
        {confirmDelete ? (
          <>
            <span style={{ fontSize: 11, color: "var(--error)" }}>Sure?</span>
            <Btn sm variant="danger" onClick={onDelete}>
              Yes, delete
            </Btn>
            <Btn sm variant="ghost" onClick={() => setConfirmDelete(false)}>
              Cancel
            </Btn>
          </>
        ) : (
          <Btn sm variant="danger" onClick={() => setConfirmDelete(true)}>
            <Trash2 size={12} />
            Delete
          </Btn>
        )}
      </div>
    </div>
  );
}

// ─── Providers tab ────────────────────────────────────────────────────────────

interface ProvidersTabProps {
  onDataChanged: () => void;
}

function ProvidersTab({ onDataChanged }: ProvidersTabProps) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<
    Record<string, Model[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<Provider | null>(null);
  const [syncingId, setSyncingId] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);

  const loadProviders = useCallback(async () => {
    setLoading(true);
    try {
      const res = await providersApi.list();
      setProviders(res.data);
      const entries = await Promise.all(
        res.data.map(async (p) => {
          try {
            const mRes = await modelsApi.list(p.id);
            return [p.id, mRes.data] as [string, Model[]];
          } catch {
            return [p.id, []] as [string, Model[]];
          }
        }),
      );
      setModelsByProvider(Object.fromEntries(entries));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadProviders();
  }, [loadProviders]);

  const handleSaved = useCallback(async () => {
    setShowForm(false);
    setEditing(null);
    await loadProviders();
    onDataChanged();
  }, [loadProviders, onDataChanged]);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await providersApi.delete(id);
        await loadProviders();
        onDataChanged();
      } catch {
        /* ignore */
      }
    },
    [loadProviders, onDataChanged],
  );

  const handleSyncModels = useCallback(async (providerId: string) => {
    setSyncingId(providerId);
    setSyncError(null);
    try {
      const res = await modelsApi.sync(providerId);
      // Safely coerce: server returns { data: Model[] } — guard against
      // unexpected shapes (e.g. legacy { data: { models, synced } }).
      const models: Model[] = Array.isArray(res.data)
        ? res.data
        : Array.isArray((res.data as { models?: Model[] }).models)
          ? (res.data as { models: Model[] }).models
          : [];
      setModelsByProvider((prev) => ({ ...prev, [providerId]: models }));
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Sync failed");
    } finally {
      setSyncingId(null);
    }
  }, []);

  const openAdd = useCallback(() => {
    setEditing(null);
    setShowForm(true);
    setSyncError(null);
  }, []);
  const openEdit = useCallback((p: Provider) => {
    setEditing(p);
    setShowForm(true);
  }, []);
  const cancelForm = useCallback(() => {
    setShowForm(false);
    setEditing(null);
  }, []);

  return (
    <div>
      {/* page-header */}
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
            Providers
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            Configure AI model providers and manage API keys
          </div>
        </div>
      </div>

      {showForm && (
        <ProviderFormPanel
          editing={editing}
          onSaved={handleSaved}
          onCancel={cancelForm}
        />
      )}

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
      ) : providers.length === 0 && !showForm ? (
        <div
          style={{
            padding: "60px 20px",
            textAlign: "center",
            color: "var(--text-tertiary)",
          }}
        >
          <div style={{ fontSize: 40, marginBottom: 14, opacity: 0.2 }}>🔌</div>
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginBottom: 6,
            }}
          >
            No providers yet
          </div>
          <div style={{ fontSize: 13, marginBottom: 18 }}>
            Add an AI provider to start using agent-deck.
          </div>
          <Btn variant="primary" onClick={openAdd}>
            <Plus size={14} />
            Add Provider
          </Btn>
        </div>
      ) : (
        <>
          {/* section-heading */}
          {providers.length > 0 && (
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                marginBottom: 10,
              }}
            >
              Connected Providers
            </div>
          )}
          {/* provider-list: flex-col, gap 12 */}
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 12,
              marginBottom: 24,
            }}
          >
            {syncError && (
              <div
                style={{
                  background: "rgba(220,50,50,0.1)",
                  border: "1px solid rgba(220,50,50,0.3)",
                  borderRadius: 8,
                  padding: "8px 12px",
                  fontSize: 12,
                  color: "var(--error, #e05c5c)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 8,
                }}
              >
                <span>⚠ Sync failed: {syncError}</span>
                <button
                  onClick={() => setSyncError(null)}
                  style={{
                    background: "none",
                    border: "none",
                    color: "inherit",
                    cursor: "pointer",
                    fontSize: 14,
                    lineHeight: 1,
                    padding: "0 2px",
                    opacity: 0.7,
                    fontFamily: "inherit",
                  }}
                >
                  ×
                </button>
              </div>
            )}
            {providers.map((p) => (
              <ProviderCard
                key={p.id}
                provider={p}
                models={modelsByProvider[p.id] ?? []}
                onEdit={() => openEdit(p)}
                onDelete={() => handleDelete(p.id)}
                onSyncModels={() => handleSyncModels(p.id)}
                syncingModels={syncingId === p.id}
              />
            ))}
            {!showForm && (
              <Btn variant="ghost" onClick={openAdd}>
                <Plus size={13} />
                Add Provider
              </Btn>
            )}
          </div>
        </>
      )}
    </div>
  );
}

// ─── Persona add/edit form — matches .form-card from mockup ───────────────────

interface PersonaFormPanelProps {
  editing: AgentPersona | null;
  providers: Provider[];
  modelsByProvider: Record<string, Model[]>;
  onSaved: () => void;
  onCancel: () => void;
}

function PersonaFormPanel({
  editing,
  providers,
  modelsByProvider,
  onSaved,
  onCancel,
}: PersonaFormPanelProps) {
  const [emojiPickerOpen, setEmojiPickerOpen] = useState(false);
  const [emojiSearch, setEmojiSearch] = useState("");
  const [emojiCategory, setEmojiCategory] = useState(0);

  const [form, setForm] = useState<PersonaForm>({
    name: editing?.name ?? "",
    emoji: editing?.emoji ?? "🦉",
    system_prompt: editing?.system_prompt ?? "",
    default_provider: editing?.default_provider ?? "",
    default_model: editing?.default_model ?? "",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const setField =
    (k: keyof PersonaForm) =>
    (
      e: React.ChangeEvent<
        HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
      >,
    ) => {
      const val = e.target.value;
      setForm((prev) => {
        const next = { ...prev, [k]: val };
        if (k === "default_provider") next.default_model = "";
        return next;
      });
    };

  const availableModels = form.default_provider
    ? (modelsByProvider[form.default_provider] ?? []).filter((m) => m.enabled)
    : [];

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!form.name.trim()) return setError("Name is required.");
    if (!form.system_prompt.trim())
      return setError("System prompt is required.");
    if (!form.default_provider)
      return setError(
        "A default provider is required so the agent knows which AI to use.",
      );
    if (!form.default_model)
      return setError(
        "A default model is required — pick one from the provider's model list.",
      );
    setSaving(true);
    try {
      if (editing) {
        await personasApi.update(editing.id, {
          name: form.name.trim(),
          emoji: form.emoji,
          system_prompt: form.system_prompt.trim(),
          default_provider: form.default_provider || undefined,
          default_model: form.default_model || undefined,
        });
      } else {
        await personasApi.create({
          name: form.name.trim(),
          emoji: form.emoji,
          system_prompt: form.system_prompt.trim(),
          default_provider: form.default_provider || undefined,
          default_model: form.default_model || undefined,
        });
      }
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed.");
    } finally {
      setSaving(false);
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
      {/* form-title */}
      <div
        style={{
          fontSize: 15,
          fontWeight: 600,
          marginBottom: 18,
          color: "var(--text-primary)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span>{editing ? "Edit Persona" : "New Persona"}</span>
        <Btn sm variant="ghost" onClick={onCancel}>
          ✕ Cancel
        </Btn>
      </div>

      <form onSubmit={handleSave}>
        {/* Emoji picker row */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 6,
            marginBottom: 14,
          }}
        >
          <FieldLabel>Emoji</FieldLabel>
          {/* quick palette + selected preview + expand button */}
          <div
            style={{
              display: "flex",
              gap: 8,
              flexWrap: "wrap",
              marginTop: 4,
              alignItems: "center",
            }}
          >
            {/* selected preview (if not in palette) */}
            {!EMOJI_PALETTE.includes(form.emoji) && (
              <EmojiBtn
                key="custom"
                emoji={form.emoji}
                selected={true}
                onClick={() => setEmojiPickerOpen((o) => !o)}
              />
            )}
            {EMOJI_PALETTE.map((e) => (
              <EmojiBtn
                key={e}
                emoji={e}
                selected={form.emoji === e}
                onClick={() => {
                  setForm((prev) => ({ ...prev, emoji: e }));
                  setEmojiPickerOpen(false);
                }}
              />
            ))}
            {/* expand button */}
            <button
              type="button"
              title="Browse all emojis"
              onClick={() => setEmojiPickerOpen((o) => !o)}
              style={{
                width: 36,
                height: 36,
                borderRadius: 7,
                fontSize: 16,
                background: emojiPickerOpen
                  ? "var(--accent-primary)"
                  : "var(--bg-tertiary)",
                border: `2px solid ${emojiPickerOpen ? "var(--accent-primary)" : "transparent"}`,
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: emojiPickerOpen
                  ? "var(--text-inverse)"
                  : "var(--text-secondary)",
                transition: "all 0.15s",
                flexShrink: 0,
                fontFamily: "inherit",
              }}
            >
              {emojiPickerOpen ? "✕" : "···"}
            </button>
          </div>

          {/* full emoji picker panel */}
          {emojiPickerOpen && (
            <div
              style={{
                marginTop: 10,
                background: "var(--bg-primary)",
                border: "1px solid var(--border-default)",
                borderRadius: 10,
                padding: "10px 12px",
                display: "flex",
                flexDirection: "column",
                gap: 8,
              }}
            >
              {/* search */}
              <input
                type="text"
                placeholder="Type or paste any emoji…"
                value={emojiSearch}
                onChange={(e) => setEmojiSearch(e.target.value)}
                onKeyDown={(e) => {
                  // pressing Enter with a single grapheme cluster selects it
                  const val = e.currentTarget.value.trim();
                  if (e.key === "Enter" && val) {
                    // grab first grapheme (emoji may be multi-codepoint)
                    const seg = [...new Intl.Segmenter().segment(val)];
                    if (seg.length > 0) {
                      setForm((prev) => ({ ...prev, emoji: seg[0].segment }));
                      setEmojiSearch("");
                      setEmojiPickerOpen(false);
                    }
                  }
                }}
                style={{
                  background: "var(--bg-secondary)",
                  border: "1px solid var(--border-subtle)",
                  borderRadius: 7,
                  padding: "7px 10px",
                  color: "var(--text-primary)",
                  fontSize: 13,
                  fontFamily: "inherit",
                  outline: "none",
                  width: "100%",
                  boxSizing: "border-box",
                }}
                autoFocus
              />

              {/* category tabs */}
              {!emojiSearch && (
                <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                  {EMOJI_CATEGORIES.map((cat, i) => (
                    <button
                      key={cat.label}
                      type="button"
                      onClick={() => setEmojiCategory(i)}
                      style={{
                        padding: "3px 8px",
                        borderRadius: 5,
                        fontSize: 11,
                        fontWeight: 500,
                        cursor: "pointer",
                        border: "none",
                        background:
                          emojiCategory === i
                            ? "var(--accent-primary)"
                            : "var(--bg-elevated)",
                        color:
                          emojiCategory === i
                            ? "var(--text-inverse)"
                            : "var(--text-secondary)",
                        fontFamily: "inherit",
                        transition: "background 0.12s",
                      }}
                    >
                      {cat.label}
                    </button>
                  ))}
                </div>
              )}

              {/* emoji grid */}
              <div
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  gap: 4,
                  maxHeight: 180,
                  overflowY: "auto",
                }}
              >
                {(emojiSearch
                  ? EMOJI_CATEGORIES.flatMap((c) => c.emojis).filter((e) =>
                      e.includes(emojiSearch.trim()),
                    )
                  : EMOJI_CATEGORIES[emojiCategory].emojis
                ).map((e) => (
                  <EmojiBtn
                    key={e}
                    emoji={e}
                    selected={form.emoji === e}
                    onClick={() => {
                      setForm((prev) => ({ ...prev, emoji: e }));
                      setEmojiSearch("");
                      setEmojiPickerOpen(false);
                    }}
                  />
                ))}
                {emojiSearch &&
                  EMOJI_CATEGORIES.flatMap((c) => c.emojis).filter((e) =>
                    e.includes(emojiSearch.trim()),
                  ).length === 0 && (
                    <div
                      style={{
                        fontSize: 12,
                        color: "var(--text-tertiary)",
                        padding: "8px 4px",
                      }}
                    >
                      No matches — press Enter to use "{emojiSearch.trim()}"
                      directly.
                    </div>
                  )}
              </div>
            </div>
          )}
        </div>

        {/* form-grid */}
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}
        >
          {/* Name */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Name</FieldLabel>
            <FieldInput
              placeholder="e.g. Aldous"
              value={form.name}
              onChange={setField("name")}
            />
          </div>

          {/* Default Provider */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Default Provider</FieldLabel>
            <FieldSelect
              value={form.default_provider}
              onChange={setField("default_provider")}
            >
              <option value="">None — choose per thread</option>
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </FieldSelect>
          </div>

          {/* Default Model */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Default Model</FieldLabel>
            <FieldSelect
              value={form.default_model}
              onChange={setField("default_model")}
              disabled={!form.default_provider}
            >
              <option value="">None — choose per thread</option>
              {availableModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.display_name}
                </option>
              ))}
            </FieldSelect>
            {form.default_provider && availableModels.length === 0 && (
              <FieldHint>
                No enabled models — sync models for this provider first.
              </FieldHint>
            )}
          </div>

          {/* System Prompt — form-full */}
          <div
            style={{
              gridColumn: "1 / -1",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            <FieldLabel>System Prompt</FieldLabel>
            <FieldTextarea
              placeholder="You are Aldous, a thoughtful and precise assistant. You excel at deep analysis, technical problems, and careful reasoning…"
              value={form.system_prompt}
              onChange={setField("system_prompt")}
              style={{ minHeight: 140 }}
            />
            <FieldHint>
              Defines the agent's core personality and behavior. Always
              prepended to the conversation context.
            </FieldHint>
          </div>
        </div>

        {error && (
          <p style={{ marginTop: 10, fontSize: 12, color: "var(--error)" }}>
            {error}
          </p>
        )}

        {/* form-actions */}
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            marginTop: 18,
            paddingTop: 16,
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
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
              opacity: saving ? 0.6 : 1,
              fontFamily: "inherit",
            }}
          >
            {saving ? "Saving…" : "Save Persona"}
          </button>
        </div>
      </form>
    </div>
  );
}

// emoji-btn: 36×36, 7px radius, bg-tertiary, border 2px transparent, 18px font
function EmojiBtn({
  emoji,
  selected,
  onClick,
}: {
  emoji: string;
  selected: boolean;
  onClick: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        width: 36,
        height: 36,
        borderRadius: 7,
        fontSize: 18,
        background: selected
          ? "var(--accent-muted)"
          : hovered
            ? "var(--bg-elevated)"
            : "var(--bg-tertiary)",
        border: `2px solid ${selected ? "var(--accent-primary)" : "transparent"}`,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "border-color 0.15s, background 0.15s",
        flexShrink: 0,
      }}
    >
      {emoji}
    </button>
  );
}

// ─── Persona card — matches .persona-card from mockup ────────────────────────

interface PersonaCardProps {
  persona: AgentPersona;
  providers: Provider[];
  modelsByProvider: Record<string, Model[]>;
  onEdit: () => void;
  onDelete: () => void;
}

function PersonaCard({
  persona,
  providers,
  modelsByProvider,
  onEdit,
  onDelete,
}: PersonaCardProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [hovered, setHovered] = useState(false);

  const provider = providers.find((p) => p.id === persona.default_provider);
  const allModels = persona.default_provider
    ? (modelsByProvider[persona.default_provider] ?? [])
    : [];
  const model = allModels.find((m) => m.id === persona.default_model);
  const modelLabel =
    model && provider
      ? `${model.display_name} · ${provider.name}`
      : provider
        ? provider.name
        : null;

  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: `1px solid ${hovered ? "var(--border-default)" : "var(--border-subtle)"}`,
        borderRadius: 12,
        padding: 18,
        cursor: "default",
        transition: "border-color 0.15s",
        position: "relative",
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* persona-card-top: flex, gap 12, mb 12 */}
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: 12,
          marginBottom: 12,
        }}
      >
        {/* persona-avatar: 52×52, circle, bg-elevated, 24px emoji */}
        <div
          style={{
            width: 52,
            height: 52,
            borderRadius: "50%",
            background: "var(--bg-elevated)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 24,
            flexShrink: 0,
          }}
        >
          {persona.emoji}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          {/* persona-card-name: 15px, 700 */}
          <div
            style={{
              fontSize: 15,
              fontWeight: 700,
              color: "var(--text-primary)",
              marginBottom: 2,
              display: "flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            {persona.name}
          </div>
          {/* persona-card-model: 11px, text-tertiary */}
          {modelLabel && (
            <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
              {modelLabel}
            </div>
          )}
        </div>
      </div>

      {/* persona-card-prompt: 12px, text-secondary, 2-line clamp, border-top, pt 10 */}
      <div
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          lineHeight: 1.5,
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
          borderTop: "1px solid var(--border-subtle)",
          paddingTop: 10,
          marginBottom: 0,
        }}
      >
        {persona.system_prompt}
      </div>

      {/* persona-card-actions: flex, gap 6, mt 12 */}
      <div style={{ display: "flex", gap: 6, marginTop: 12 }}>
        <Btn sm variant="ghost" onClick={onEdit}>
          <Pencil size={12} /> Edit
        </Btn>
        {confirmDelete ? (
          <>
            <span
              style={{
                fontSize: 11,
                color: "var(--error)",
                alignSelf: "center",
              }}
            >
              Sure?
            </span>
            <Btn sm variant="danger" onClick={onDelete}>
              Yes, delete
            </Btn>
            <Btn sm variant="ghost" onClick={() => setConfirmDelete(false)}>
              Cancel
            </Btn>
          </>
        ) : (
          <Btn
            sm
            variant="danger"
            style={{ marginLeft: "auto" }}
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 size={12} /> Delete
          </Btn>
        )}
      </div>
    </div>
  );
}

// ─── Personas tab ─────────────────────────────────────────────────────────────

interface PersonasTabProps {
  onDataChanged: () => void;
}

function PersonasTab({ onDataChanged }: PersonasTabProps) {
  const [personas, setPersonas] = useState<AgentPersona[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<
    Record<string, Model[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<AgentPersona | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [pRes, provRes] = await Promise.all([
        personasApi.list(),
        providersApi.list(),
      ]);
      setPersonas(pRes.data);
      setProviders(provRes.data);
      const entries = await Promise.all(
        provRes.data.map(async (p) => {
          try {
            const mRes = await modelsApi.list(p.id);
            return [p.id, mRes.data] as [string, Model[]];
          } catch {
            return [p.id, []] as [string, Model[]];
          }
        }),
      );
      setModelsByProvider(Object.fromEntries(entries));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleSaved = useCallback(async () => {
    setShowForm(false);
    setEditing(null);
    await loadData();
    onDataChanged();
  }, [loadData, onDataChanged]);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await personasApi.delete(id);
        await loadData();
        onDataChanged();
      } catch {
        /* ignore */
      }
    },
    [loadData, onDataChanged],
  );

  const openAdd = useCallback(() => {
    setEditing(null);
    setShowForm(true);
  }, []);
  const openEdit = useCallback((p: AgentPersona) => {
    setEditing(p);
    setShowForm(true);
  }, []);

  return (
    <div>
      {/* page-header */}
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
            Personas
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            Manage agent personalities, prompts, and memories
          </div>
        </div>
      </div>

      {showForm && (
        <PersonaFormPanel
          editing={editing}
          providers={providers}
          modelsByProvider={modelsByProvider}
          onSaved={handleSaved}
          onCancel={() => {
            setShowForm(false);
            setEditing(null);
          }}
        />
      )}

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
      ) : personas.length === 0 && !showForm ? (
        <div
          style={{
            padding: "60px 20px",
            textAlign: "center",
            color: "var(--text-tertiary)",
          }}
        >
          <div style={{ fontSize: 40, marginBottom: 14, opacity: 0.2 }}>🤖</div>
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginBottom: 6,
            }}
          >
            No personas yet
          </div>
          <div style={{ fontSize: 13, marginBottom: 18 }}>
            Create a persona to define your AI's name, personality, and default
            model.
          </div>
          <Btn variant="primary" onClick={openAdd}>
            <Plus size={14} />
            New Persona
          </Btn>
        </div>
      ) : (
        <>
          {personas.length > 0 && (
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                marginBottom: 10,
              }}
            >
              Your Personas
            </div>
          )}
          {/* persona-grid: auto-fill minmax(220px,1fr), gap 14 */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
              gap: 14,
              marginBottom: 24,
            }}
          >
            {personas.map((p) => (
              <PersonaCard
                key={p.id}
                persona={p}
                providers={providers}
                modelsByProvider={modelsByProvider}
                onEdit={() => openEdit(p)}
                onDelete={() => handleDelete(p.id)}
              />
            ))}

            {/* Dashed "add" card */}
            {!showForm && <AddPersonaCard onClick={openAdd} />}
          </div>
        </>
      )}
    </div>
  );
}

function AddPersonaCard({ onClick }: { onClick: () => void }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: "transparent",
        border: `2px dashed ${hovered ? "var(--border-strong)" : "var(--border-default)"}`,
        borderRadius: 12,
        minHeight: 160,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        color: hovered ? "var(--text-secondary)" : "var(--text-tertiary)",
        fontSize: 13,
        fontFamily: "inherit",
        transition: "border-color 0.15s, color 0.15s",
      }}
    >
      <span style={{ fontSize: 22 }}>＋</span>
      <span>New Persona</span>
    </button>
  );
}

// ─── Settings nav item — matches .nav-item from mockup ───────────────────────

function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: string;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "8px 16px",
        margin: "0 8px 2px",
        borderRadius: 7,
        fontSize: 13,
        color:
          active || hovered ? "var(--text-primary)" : "var(--text-secondary)",
        cursor: "pointer",
        background: active
          ? "var(--accent-muted)"
          : hovered
            ? "var(--bg-tertiary)"
            : "none",
        border: "none",
        width: "calc(100% - 16px)",
        textAlign: "left",
        transition: "background 0.15s, color 0.15s",
        fontFamily: "inherit",
      }}
    >
      <span style={{ fontSize: 15 }}>{icon}</span>
      {label}
    </button>
  );
}

// ─── Main SettingsModal ───────────────────────────────────────────────────────

export interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialTab?: SettingsTab;
  onDataChanged?: () => void;
}

export function SettingsModal({
  isOpen,
  onClose,
  initialTab = "providers",
  onDataChanged,
}: SettingsModalProps) {
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [prevIsOpen, setPrevIsOpen] = useState(isOpen);

  // Sync tab to initialTab each time the modal transitions closed → open
  if (prevIsOpen !== isOpen) {
    setPrevIsOpen(isOpen);
    if (isOpen) setTab(initialTab);
  }

  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [isOpen, onClose]);

  const handleDataChanged = useCallback(() => {
    onDataChanged?.();
  }, [onDataChanged]);

  const handleBackdrop = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose],
  );

  return (
    <>
      {/* Keyframe for spin animation used by test/sync buttons */}
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>

      {/* Backdrop — separate div so its opacity doesn't bleed into the modal */}
      <div
        onClick={handleBackdrop}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 200,
          background: "rgba(0,0,0,0.45)",
          backdropFilter: "blur(6px)",
          WebkitBackdropFilter: "blur(6px)",
          opacity: isOpen ? 1 : 0,
          pointerEvents: "none",
          transition: "opacity 0.2s",
        }}
      />

      {/* Modal positioner — handles centering and pointer-events independently */}
      <div
        onClick={handleBackdrop}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 201,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          pointerEvents: isOpen ? "auto" : "none",
        }}
        aria-modal="true"
        role="dialog"
        aria-label="Settings"
      >
        {/* Modal shell — 860×600, bg-secondary, border-subtle, 14px radius */}
        <div
          style={{
            position: "relative",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 14,
            width: 860,
            maxWidth: "calc(100vw - 32px)",
            height: 600,
            maxHeight: "calc(100vh - 48px)",
            display: "flex",
            overflow: "hidden",
            boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
            opacity: isOpen ? 1 : 0,
            transform: isOpen ? "scale(1)" : "scale(0.97)",
            transition: "opacity 0.2s, transform 0.2s",
          }}
        >
          {/* ── Settings sidebar — 220px, bg-secondary, border-right, padding 20px 0 ── */}
          <aside
            style={{
              width: 220,
              minWidth: 220,
              background: "var(--bg-secondary)",
              borderRight: "1px solid var(--border-subtle)",
              display: "flex",
              flexDirection: "column",
              padding: "20px 0",
            }}
          >
            {/* sidebar-brand: flex, gap 8, px 16, pb 20, border-bottom, mb 12 */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "0 16px 20px",
                borderBottom: "1px solid var(--border-subtle)",
                marginBottom: 12,
              }}
            >
              <span style={{ fontSize: 18 }}>🤖</span>
              <span
                style={{
                  fontSize: 14,
                  fontWeight: 700,
                  color: "var(--text-primary)",
                  letterSpacing: "-0.01em",
                }}
              >
                agent-deck
              </span>
            </div>

            {/* back-link */}
            <BackLink onClick={onClose} />

            {/* nav-section-label */}
            <div
              style={{
                padding: "4px 16px 6px",
                fontSize: 10,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
              }}
            >
              Settings
            </div>

            <NavItem
              icon="🔌"
              label="Providers"
              active={tab === "providers"}
              onClick={() => setTab("providers")}
            />
            <NavItem
              icon="🎭"
              label="Personas"
              active={tab === "personas"}
              onClick={() => setTab("personas")}
            />
          </aside>

          {/* ── Main content ── */}
          <div
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
            }}
          >
            {/* page-body: flex 1, overflow-y auto, padding 24px 28px */}
            <div
              style={{
                flex: 1,
                overflowY: "auto",
                padding: "24px 28px",
              }}
              className="scrollbar-thin"
            >
              {tab === "providers" && (
                <ProvidersTab onDataChanged={handleDataChanged} />
              )}
              {tab === "personas" && (
                <PersonasTab onDataChanged={handleDataChanged} />
              )}
            </div>
          </div>

          {/* Close × button — top-right */}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close settings"
            style={{
              position: "absolute",
              top: 14,
              right: 14,
              width: 28,
              height: 28,
              borderRadius: 6,
              background: "transparent",
              border: "none",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              transition: "background 0.15s, color 0.15s",
              fontFamily: "inherit",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--bg-elevated)";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-tertiary)";
            }}
          >
            <X size={15} />
          </button>
        </div>
      </div>
    </>
  );
}

// back-link: flex, gap 7, padding 7px 16px, margin 0 8px 12px, 6px radius, 13px
function BackLink({ onClick }: { onClick: () => void }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 7,
        padding: "7px 16px",
        margin: "0 8px 12px",
        borderRadius: 6,
        fontSize: 13,
        color: hovered ? "var(--text-primary)" : "var(--text-secondary)",
        cursor: "pointer",
        background: hovered ? "var(--bg-tertiary)" : "none",
        border: "none",
        transition: "background 0.15s, color 0.15s",
        fontFamily: "inherit",
      }}
    >
      ← Back to Chats
    </button>
  );
}

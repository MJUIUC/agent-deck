import React, { useState } from "react";

// ─── Types ────────────────────────────────────────────────────────────────────

export type SettingsTab =
  | "providers"
  | "credentials"
  | "personas"
  | "mcp-servers"
  | "mobile"
  | "general"
  | "archived-threads";

export type ProviderFormData = {
  name: string;
  kind: "openai" | "anthropic" | "custom" | "copilot" | "";
  base_url: string;
  api_key: string;
};

export type CopilotAuthStep =
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

export type PersonaFormData = {
  name: string;
  emoji: string;
  system_prompt: string;
  default_provider: string;
  default_model: string;
  recall_conversation_cross_thread?: boolean;
};

// ─── Constants ────────────────────────────────────────────────────────────────

export const KIND_DEFAULT_URLS: Record<string, string> = {
  openai: "https://api.openai.com/v1",
  anthropic: "https://api.anthropic.com/v1",
  custom: "",
  copilot: "http://localhost:4141/v1",
};

export const KIND_ICONS: Record<string, string> = {
  openai: "🤖",
  anthropic: "✦",
  custom: "🔑",
  copilot: "🐙",
};

export const KIND_URL_HINTS: Record<string, string> = {
  openai: "OpenAI-compatible endpoint — e.g. https://api.openai.com/v1.",
  anthropic: "Anthropic API endpoint — https://api.anthropic.com/v1.",
  custom:
    "Any OpenAI-compatible endpoint, e.g. a local Ollama or LM Studio URL.",
  copilot: "Managed automatically by the copilot-api sidecar.",
};

export const EMOJI_PALETTE = [
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

export const EMOJI_CATEGORIES: { label: string; emojis: string[] }[] = [
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

// ─── Shared field primitives ──────────────────────────────────────────────────

/** field-label: 11px, 600, text-secondary, uppercase, 0.06em tracking */
export function FieldLabel({ children }: { children: React.ReactNode }) {
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
export const fieldBase: React.CSSProperties = {
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

export function FieldInput(
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

export function FieldSelect(
  props: React.SelectHTMLAttributes<HTMLSelectElement>,
) {
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

export function FieldTextarea(
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

export function FieldHint({ children }: { children: React.ReactNode }) {
  return (
    <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
      {children}
    </span>
  );
}

// ─── Btn ──────────────────────────────────────────────────────────────────────

/** btn: 8px 16px, 7px radius, 13px, 500 weight */
export function Btn({
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

// ─── StatusBadge ──────────────────────────────────────────────────────────────

export function StatusBadge({
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
    },
    pending: {
      bg: "rgba(196,162,74,0.12)",
      color: "var(--warning)",
      dot: "var(--warning)",
      label: "Auth required",
    },
    error: {
      bg: "rgba(196,90,90,0.12)",
      color: "var(--error)",
      dot: "var(--error)",
      label: "Auth error",
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

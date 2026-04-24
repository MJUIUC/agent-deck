import React, { useState } from "react";
import { useThemeStore, type Palette, type Mode } from "@/stores/useThemeStore";

// ─── Palette definitions ──────────────────────────────────────────────────────

const PALETTES: {
  id: Palette;
  label: string;
  accent: string;
}[] = [
  { id: "olive", label: "Olive", accent: "#7c8c5a" },
  { id: "slate", label: "Slate", accent: "#58a6ff" },
  { id: "midnight", label: "Midnight", accent: "#8b7fd4" },
  { id: "rose", label: "Rose", accent: "#c47a8a" },
  { id: "forest", label: "Forest", accent: "#4caf72" },
  { id: "ember", label: "Ember", accent: "#d4853a" },
  { id: "ocean", label: "Ocean", accent: "#2ab8d0" },
  { id: "copper", label: "Copper", accent: "#c07840" },
  { id: "sakura", label: "Sakura", accent: "#e8709a" },
  { id: "noir", label: "Noir", accent: "#e0e0e0" },
];

// ─── ToggleRow ────────────────────────────────────────────────────────────────

function ToggleRow({
  title,
  description,
  value,
  onChange,
}: {
  title: string;
  description: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 16,
        paddingBottom: 14,
        marginBottom: 14,
        borderBottom: "1px solid var(--border-subtle)",
      }}
    >
      <div>
        <div
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
            marginBottom: 2,
          }}
        >
          {title}
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          {description}
        </div>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={value}
        onClick={() => onChange(!value)}
        style={{
          width: 36,
          height: 20,
          borderRadius: 10,
          border: "none",
          background: value ? "var(--accent-primary)" : "var(--bg-elevated)",
          cursor: "pointer",
          position: "relative",
          flexShrink: 0,
          transition: "background 0.2s",
          outline: "none",
        }}
      >
        <span
          style={{
            position: "absolute",
            top: 3,
            left: value ? 19 : 3,
            width: 14,
            height: 14,
            borderRadius: "50%",
            background: value ? "var(--text-inverse)" : "var(--text-tertiary)",
            transition: "left 0.2s, background 0.2s",
          }}
        />
      </button>
    </div>
  );
}

// ─── SectionCard ──────────────────────────────────────────────────────────────

function SectionCard({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <section
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "16px 18px",
        marginBottom: 16,
      }}
    >
      <div style={{ marginBottom: subtitle ? 4 : 14 }}>
        <div
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
            marginBottom: subtitle ? 2 : 0,
          }}
        >
          {title}
        </div>
        {subtitle && (
          <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            {subtitle}
          </div>
        )}
      </div>
      {children}
    </section>
  );
}

// ─── AppearanceSettings ───────────────────────────────────────────────────────

export function AppearanceSettings() {
  const palette = useThemeStore((s) => s.palette);
  const mode = useThemeStore((s) => s.mode);
  const setPalette = useThemeStore((s) => s.setPalette);
  const setMode = useThemeStore((s) => s.setMode);

  // UI preferences — local state only (no persistence endpoint yet)
  const [showStreamingIndicator, setShowStreamingIndicator] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const [showRoutineLabels, setShowRoutineLabels] = useState(true);
  const [pushNotifications, setPushNotifications] = useState(false);

  return (
    <div>
      {/* Section header */}
      <div style={{ marginBottom: 20 }}>
        <div
          style={{
            fontSize: 16,
            fontWeight: 700,
            color: "var(--text-primary)",
            marginBottom: 4,
          }}
        >
          Appearance
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          Theme, color palette, and interface preferences.
        </div>
      </div>

      {/* ── Theme ── */}
      <SectionCard title="Theme">
        {/* Palette swatches */}
        <div style={{ marginBottom: 20 }}>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              marginBottom: 10,
              fontWeight: 500,
            }}
          >
            Palette
          </div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
            {PALETTES.map((p) => {
              const selected = palette === p.id;
              return (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => setPalette(p.id)}
                  title={p.label}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    gap: 5,
                    background: "none",
                    border: "none",
                    cursor: "pointer",
                    padding: 2,
                  }}
                >
                  <div
                    style={{
                      width: 36,
                      height: 36,
                      borderRadius: 8,
                      background: p.accent,
                      boxShadow: selected
                        ? `0 0 0 2px var(--bg-primary), 0 0 0 4px ${p.accent}`
                        : "none",
                      transition: "box-shadow 0.15s",
                    }}
                  />
                  <span
                    style={{
                      fontSize: 10,
                      color: selected
                        ? "var(--text-primary)"
                        : "var(--text-tertiary)",
                      transition: "color 0.15s",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {p.label}
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Mode toggle */}
        <div>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              marginBottom: 10,
              fontWeight: 500,
            }}
          >
            Mode
          </div>
          <div
            style={{
              display: "inline-flex",
              borderRadius: 8,
              overflow: "hidden",
              border: "1px solid var(--border-default)",
            }}
          >
            {(["system", "light", "dark"] as Mode[]).map((m, i) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                style={{
                  padding: "7px 18px",
                  fontSize: 12,
                  fontWeight: 500,
                  border: "none",
                  borderLeft:
                    i > 0 ? "1px solid var(--border-default)" : "none",
                  background:
                    mode === m ? "var(--accent-primary)" : "var(--bg-tertiary)",
                  color:
                    mode === m
                      ? "var(--text-inverse)"
                      : "var(--text-secondary)",
                  cursor: "pointer",
                  transition: "background 0.15s, color 0.15s",
                  fontFamily: "inherit",
                }}
              >
                {m.charAt(0).toUpperCase() + m.slice(1)}
              </button>
            ))}
          </div>
        </div>
      </SectionCard>

      {/* ── Preferences ── */}
      <SectionCard title="Preferences">
        <ToggleRow
          title="Show streaming indicator"
          description="Display animated dots while the agent is generating a response"
          value={showStreamingIndicator}
          onChange={setShowStreamingIndicator}
        />
        <ToggleRow
          title="Auto-scroll to new messages"
          description="Automatically scroll down when new messages arrive"
          value={autoScroll}
          onChange={setAutoScroll}
        />
        <ToggleRow
          title="Show routine message labels"
          description='Display a "Routine" label on messages generated by scheduled routines'
          value={showRoutineLabels}
          onChange={setShowRoutineLabels}
        />
        {/* Push notifications — last row, no bottom border */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 16,
          }}
        >
          <div>
            <div
              style={{
                fontSize: 13,
                fontWeight: 500,
                color: "var(--text-primary)",
                marginBottom: 2,
              }}
            >
              Push notifications
            </div>
            <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              Send push notifications when the mobile app is in the background
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={pushNotifications}
            onClick={() => setPushNotifications((v) => !v)}
            style={{
              width: 36,
              height: 20,
              borderRadius: 10,
              border: "none",
              background: pushNotifications
                ? "var(--accent-primary)"
                : "var(--bg-elevated)",
              cursor: "pointer",
              position: "relative",
              flexShrink: 0,
              transition: "background 0.2s",
              outline: "none",
            }}
          >
            <span
              style={{
                position: "absolute",
                top: 3,
                left: pushNotifications ? 19 : 3,
                width: 14,
                height: 14,
                borderRadius: "50%",
                background: pushNotifications
                  ? "var(--text-inverse)"
                  : "var(--text-tertiary)",
                transition: "left 0.2s, background 0.2s",
              }}
            />
          </button>
        </div>
      </SectionCard>
    </div>
  );
}

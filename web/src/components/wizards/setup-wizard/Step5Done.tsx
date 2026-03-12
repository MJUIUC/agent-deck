import React, { useEffect, useState } from "react";
import type { PersonaConfig } from "./Step4Persona";

// ── Step5Done ─────────────────────────────────────────────────────────────────
// Final step of the setup wizard. Animated ✓ icon, "You're all set!" heading,
// summary of what was configured, and "Open agent-deck →" button.
// No back button on this step.

interface Step5DoneProps {
  displayName: string;
  providerName: string | null;
  persona: PersonaConfig | null;
  saving: boolean;
  onComplete: () => void;
}

export function Step5Done({
  displayName,
  providerName,
  persona,
  saving,
  onComplete,
}: Step5DoneProps) {
  const [checkVisible, setCheckVisible] = useState(false);
  const [contentVisible, setContentVisible] = useState(false);

  // Staggered entrance animation
  useEffect(() => {
    const t1 = setTimeout(() => setCheckVisible(true), 80);
    const t2 = setTimeout(() => setContentVisible(true), 380);
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
    };
  }, []);

  const summaryRows: { label: string; value: string }[] = [
    {
      label: "Your name",
      value: displayName || "—",
    },
    {
      label: "Provider",
      value: providerName ?? "None — add one in Settings",
    },
    {
      label: "First persona",
      value: persona
        ? `${persona.emoji} ${persona.name}`
        : "None — add one in Settings",
    },
  ];

  return (
    <div style={{ textAlign: "center" }}>
      {/* Animated checkmark */}
      <div
        style={{
          width: 72,
          height: 72,
          borderRadius: "50%",
          background: checkVisible
            ? "var(--accent-primary)"
            : "var(--bg-elevated)",
          border: `2px solid ${checkVisible ? "var(--accent-primary)" : "var(--border-default)"}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          margin: "0 auto 20px",
          fontSize: 30,
          color: checkVisible ? "var(--text-inverse)" : "var(--text-tertiary)",
          transition: "background 0.4s ease, border-color 0.4s ease, color 0.4s ease",
          transform: checkVisible ? "scale(1)" : "scale(0.85)",
        }}
      >
        ✓
      </div>

      {/* Heading */}
      <div
        style={{
          opacity: contentVisible ? 1 : 0,
          transform: contentVisible ? "translateY(0)" : "translateY(8px)",
          transition: "opacity 0.35s ease, transform 0.35s ease",
        }}
      >
        <h2
          style={{
            fontSize: 20,
            fontWeight: 700,
            color: "var(--text-primary)",
            letterSpacing: "-0.02em",
            marginBottom: 6,
          }}
        >
          You're all set,{" "}
          <span style={{ color: "var(--accent-secondary)" }}>
            {displayName || "friend"}
          </span>
          !
        </h2>
        <p
          style={{
            fontSize: 13,
            color: "var(--text-secondary)",
            lineHeight: 1.6,
            marginBottom: 24,
          }}
        >
          agent-deck is ready. Here's a summary of what was configured.
        </p>

        {/* Summary list */}
        <div
          style={{
            background: "var(--bg-tertiary)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 10,
            overflow: "hidden",
            marginBottom: 24,
            textAlign: "left",
          }}
        >
          {summaryRows.map((row, i) => (
            <div
              key={row.label}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                padding: "12px 16px",
                borderBottom:
                  i < summaryRows.length - 1
                    ? "1px solid var(--border-subtle)"
                    : "none",
              }}
            >
              <span
                style={{
                  fontSize: 14,
                  color: "var(--accent-secondary)",
                  flexShrink: 0,
                  fontWeight: 700,
                }}
              >
                ✓
              </span>
              <span
                style={{
                  fontSize: 12,
                  color: "var(--text-tertiary)",
                  width: 100,
                  flexShrink: 0,
                }}
              >
                {row.label}
              </span>
              <span
                style={{
                  fontSize: 13,
                  color:
                    row.value.startsWith("None")
                      ? "var(--text-tertiary)"
                      : "var(--text-primary)",
                  fontStyle: row.value.startsWith("None") ? "italic" : "normal",
                  fontWeight: 500,
                  flex: 1,
                  minWidth: 0,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {row.value}
              </span>
            </div>
          ))}
        </div>

        {/* Open agent-deck button */}
        <OpenAppBtn onClick={onComplete} disabled={saving} />
      </div>
    </div>
  );
}

// ── OpenAppBtn ────────────────────────────────────────────────────────────────

function OpenAppBtn({
  onClick,
  disabled,
}: {
  onClick: () => void;
  disabled: boolean;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        padding: "11px 32px",
        borderRadius: 8,
        fontSize: 14,
        fontWeight: 600,
        cursor: disabled ? "default" : "pointer",
        border: "none",
        background: hovered && !disabled
          ? "var(--accent-secondary)"
          : "var(--accent-primary)",
        color: "var(--text-inverse)",
        transition: "background 0.15s",
        opacity: disabled ? 0.6 : 1,
        fontFamily: "inherit",
        letterSpacing: "-0.01em",
      }}
    >
      {disabled ? "Opening…" : "Open agent-deck →"}
    </button>
  );
}

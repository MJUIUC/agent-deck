import React, { useState } from "react";

// ── WizardNavRow ──────────────────────────────────────────────────────────────
// Consistent bottom navigation row for wizard steps.
// Skip link only renders when onSkip is provided.
// All props are optional — render only what's needed per step.

interface WizardNavRowProps {
  onBack?: () => void;
  onNext?: () => void;
  onSkip?: () => void;
  nextLabel?: string;
  backLabel?: string;
  nextDisabled?: boolean;
  /** When true, centers content (e.g. Welcome step with only a "Get Started" btn) */
  centered?: boolean;
}

function NavBtn({
  children,
  variant,
  disabled,
  onClick,
}: {
  children: React.ReactNode;
  variant: "primary" | "ghost";
  disabled?: boolean;
  onClick?: () => void;
}) {
  const [hovered, setHovered] = useState(false);

  const base: React.CSSProperties = {
    padding: "8px 20px",
    borderRadius: 7,
    fontSize: 13,
    fontWeight: 500,
    cursor: disabled ? "default" : "pointer",
    border: "none",
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    transition: "background 0.15s, color 0.15s, opacity 0.15s",
    opacity: disabled ? 0.45 : 1,
    fontFamily: "inherit",
    outline: "none",
  };

  const variantStyle: React.CSSProperties =
    variant === "primary"
      ? {
          background: hovered && !disabled
            ? "var(--accent-secondary)"
            : "var(--accent-primary)",
          color: "var(--text-inverse)",
        }
      : {
          background: hovered ? "var(--bg-elevated)" : "transparent",
          color: hovered ? "var(--text-primary)" : "var(--text-secondary)",
          border: "1px solid var(--border-default)",
        };

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{ ...base, ...variantStyle }}
    >
      {children}
    </button>
  );
}

function SkipLink({ onClick }: { onClick: () => void }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: "none",
        border: "none",
        padding: "4px 8px",
        fontSize: 12,
        color: hovered ? "var(--text-secondary)" : "var(--text-tertiary)",
        cursor: "pointer",
        fontFamily: "inherit",
        transition: "color 0.15s",
        textDecoration: hovered ? "underline" : "none",
      }}
    >
      Skip for now
    </button>
  );
}

export function WizardNavRow({
  onBack,
  onNext,
  onSkip,
  nextLabel = "Next →",
  backLabel = "← Back",
  nextDisabled = false,
  centered = false,
}: WizardNavRowProps) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: centered ? "center" : "space-between",
        marginTop: 24,
        gap: 10,
      }}
    >
      {/* Left side — back button (or spacer to keep layout balanced) */}
      {!centered && (
        <div>
          {onBack && (
            <NavBtn variant="ghost" onClick={onBack}>
              {backLabel}
            </NavBtn>
          )}
        </div>
      )}

      {/* Right side — skip link + next/finish button */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        {onSkip && <SkipLink onClick={onSkip} />}
        {onNext && (
          <NavBtn variant="primary" disabled={nextDisabled} onClick={onNext}>
            {nextLabel}
          </NavBtn>
        )}
      </div>
    </div>
  );
}

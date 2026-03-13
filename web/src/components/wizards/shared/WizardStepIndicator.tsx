import React from "react";
import type { WizardStepMeta } from "./types";

// ── WizardStepIndicator ───────────────────────────────────────────────────────
// Dot/line row progress indicator. Dots before currentStep are marked done (✓),
// the current dot is active, the rest are inactive.
// Accepts 1-based currentStep.

interface WizardStepIndicatorProps {
  steps: WizardStepMeta[];
  currentStep: number; // 1-based
}

export function WizardStepIndicator({
  steps,
  currentStep,
}: WizardStepIndicatorProps) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 0,
        marginBottom: 28,
        position: "relative",
        paddingBottom: 20,
      }}
    >
      {steps.map((step, index) => {
        const stepNum = index + 1;
        const isDone = stepNum < currentStep;
        const isActive = stepNum === currentStep;

        return (
          <React.Fragment key={step.label}>
            {/* Step dot */}
            <div
              style={{
                width: 28,
                height: 28,
                borderRadius: "50%",
                background: isDone
                  ? "var(--accent-primary)"
                  : isActive
                    ? "var(--accent-muted)"
                    : "var(--bg-elevated)",
                border: isDone
                  ? "2px solid var(--accent-primary)"
                  : isActive
                    ? "2px solid var(--accent-secondary)"
                    : "2px solid var(--border-default)",
                color: isDone
                  ? "var(--text-inverse)"
                  : isActive
                    ? "var(--accent-secondary)"
                    : "var(--text-tertiary)",
                fontSize: 11,
                fontWeight: 700,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                transition: "all 0.25s",
                position: "relative",
                zIndex: 1,
                flexShrink: 0,
              }}
            >
              {isDone ? "✓" : stepNum}

              {/* Step label — positioned below dot */}
              <span
                style={{
                  position: "absolute",
                  top: "calc(100% + 6px)",
                  left: "50%",
                  transform: "translateX(-50%)",
                  fontSize: 10,
                  fontWeight: isActive ? 600 : 400,
                  color: isActive
                    ? "var(--accent-secondary)"
                    : isDone
                      ? "var(--text-secondary)"
                      : "var(--text-tertiary)",
                  whiteSpace: "nowrap",
                  letterSpacing: "0.01em",
                  transition: "color 0.25s",
                }}
              >
                {step.label}
              </span>
            </div>

            {/* Connector line between dots (not after the last dot) */}
            {index < steps.length - 1 && (
              <div
                style={{
                  flex: 1,
                  height: 2,
                  background: isDone
                    ? "var(--accent-primary)"
                    : "var(--border-default)",
                  transition: "background 0.25s",
                  minWidth: 24,
                  maxWidth: 60,
                }}
              />
            )}
          </React.Fragment>
        );
      })}
    </div>
  );
}

import React from "react";
import type { WizardStepMeta } from "./types";
import { WizardStepIndicator } from "./WizardStepIndicator";
import { WizardCard } from "./WizardCard";

// ── WizardShell ───────────────────────────────────────────────────────────────
// Full-screen dark overlay with brand header, optional step indicator, and
// a WizardCard wrapping the step content.
//
// Knows nothing about the setup wizard specifically — accepts only generic
// props so it can be reused for any future wizard flow.

interface WizardShellProps {
  steps: WizardStepMeta[];
  currentStep: number; // 1-based
  showIndicator?: boolean;
  children: React.ReactNode;
}

export function WizardShell({
  steps,
  currentStep,
  showIndicator = true,
  children,
}: WizardShellProps) {
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 300,
        background: "var(--bg-primary)",
        color: "var(--text-primary)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        padding: "24px 20px",
        overflowY: "auto",
      }}
    >
      {/* ── Brand header ── */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 10,
          marginBottom: 32,
          flexShrink: 0,
        }}
      >
        <span style={{ fontSize: 26 }}>🤖</span>
        <span
          style={{
            fontSize: 20,
            fontWeight: 700,
            letterSpacing: "-0.02em",
            color: "var(--text-primary)",
          }}
        >
          agent-deck
        </span>
      </div>

      {/* ── Step indicator (hidden on step 1 or when showIndicator=false) ── */}
      {showIndicator && (
        <div style={{ width: "100%", maxWidth: 540, flexShrink: 0 }}>
          <WizardStepIndicator steps={steps} currentStep={currentStep} />
        </div>
      )}

      {/* ── Card ── */}
      <WizardCard>{children}</WizardCard>
    </div>
  );
}

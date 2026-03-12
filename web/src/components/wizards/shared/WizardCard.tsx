import React from "react";

// ── WizardCard ────────────────────────────────────────────────────────────────
// Centered card container for wizard step content.
// Fixed width ~540px, bg-secondary background, subtle border, 14px radius,
// box shadow. Content scrolls internally if it overflows.

interface WizardCardProps {
  children: React.ReactNode;
}

export function WizardCard({ children }: WizardCardProps) {
  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 14,
        width: "100%",
        maxWidth: 540,
        margin: "0 auto",
        boxShadow: "0 24px 64px rgba(0,0,0,0.45)",
        overflowY: "auto",
        maxHeight: "calc(100vh - 200px)",
        padding: "28px 32px",
      }}
    >
      {children}
    </div>
  );
}

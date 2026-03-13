import React from "react";
import { WizardNavRow } from "../shared/WizardNavRow";

// ── Step1Welcome ──────────────────────────────────────────────────────────────
// First step of the setup wizard. Hero icon, title, description, feature grid,
// and a single centered "Get Started →" button. No step indicator on this step.

interface Step1WelcomeProps {
  onNext: () => void;
}

const FEATURES = [
  {
    icon: "🧠",
    title: "Persistent Memory",
    desc: "Agents remember across sessions",
  },
  {
    icon: "⚡",
    title: "Routines",
    desc: "Scheduled automated prompts",
  },
  {
    icon: "🎭",
    title: "Agent Personas",
    desc: "Different agents for different tasks",
  },
  {
    icon: "🔒",
    title: "Private by Design",
    desc: "All data stays on your hardware",
  },
];

export function Step1Welcome({ onNext }: Step1WelcomeProps) {
  return (
    <div>
      {/* Hero */}
      <div
        style={{
          textAlign: "center",
          marginBottom: 28,
        }}
      >
        <div style={{ fontSize: 48, marginBottom: 14, lineHeight: 1 }}>🤖</div>
        <h1
          style={{
            fontSize: 22,
            fontWeight: 700,
            color: "var(--text-primary)",
            letterSpacing: "-0.02em",
            marginBottom: 10,
          }}
        >
          Welcome to agent-deck
        </h1>
        <p
          style={{
            fontSize: 14,
            color: "var(--text-secondary)",
            lineHeight: 1.6,
            maxWidth: 380,
            margin: "0 auto",
          }}
        >
          Your self-hosted AI agent platform. Private, configurable, and always
          on. Let's get you set up in about a minute.
        </p>
      </div>

      {/* Feature grid — 2×2 */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 10,
          marginBottom: 4,
        }}
      >
        {FEATURES.map((f) => (
          <div
            key={f.title}
            style={{
              background: "var(--bg-tertiary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 10,
              padding: "14px 16px",
              display: "flex",
              alignItems: "flex-start",
              gap: 12,
            }}
          >
            <span style={{ fontSize: 20, flexShrink: 0, lineHeight: 1.3 }}>
              {f.icon}
            </span>
            <div>
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 600,
                  color: "var(--text-primary)",
                  marginBottom: 2,
                }}
              >
                {f.title}
              </div>
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-tertiary)",
                  lineHeight: 1.45,
                }}
              >
                {f.desc}
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Nav row — centered, single button */}
      <WizardNavRow onNext={onNext} nextLabel="Get Started →" centered />
    </div>
  );
}

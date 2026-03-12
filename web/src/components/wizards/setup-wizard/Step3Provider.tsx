import React, { useState, useEffect, useCallback } from "react";
import { WizardNavRow } from "../shared/WizardNavRow";
import { FieldInput, FieldLabel, FieldHint } from "../../settings/shared";
import { CopilotAuthSection } from "../../settings/CopilotAuthSection";
import { copilotApi } from "@/api/client";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ProviderKind = "copilot" | "openai" | "anthropic" | "custom";

/** Raw form data collected in Step 3. Nothing is persisted until Step 5. */
export interface ProviderDraft {
  kind: ProviderKind;
  name: string;
  base_url: string;
  api_key: string;
}

// ── Provider option metadata ──────────────────────────────────────────────────

const PROVIDER_OPTIONS: {
  kind: ProviderKind;
  icon: string;
  name: string;
  desc: string;
  badge: string;
  badgeVariant: "free" | "api";
  defaultName: string;
  defaultUrl: string;
}[] = [
  {
    kind: "copilot",
    icon: "🐙",
    name: "GitHub Copilot",
    desc: "Use your existing Copilot subscription",
    badge: "No API key",
    badgeVariant: "free",
    defaultName: "GitHub Copilot",
    defaultUrl: "http://localhost:4141/v1",
  },
  {
    kind: "openai",
    icon: "🤖",
    name: "OpenAI",
    desc: "GPT-4o, GPT-4o-mini, and more",
    badge: "API key",
    badgeVariant: "api",
    defaultName: "OpenAI",
    defaultUrl: "https://api.openai.com/v1",
  },
  {
    kind: "anthropic",
    icon: "✦",
    name: "Anthropic",
    desc: "Claude 3.5 Sonnet, Haiku, and more",
    badge: "API key",
    badgeVariant: "api",
    defaultName: "Anthropic",
    defaultUrl: "https://api.anthropic.com/v1",
  },
  {
    kind: "custom",
    icon: "⚙",
    name: "Custom / Local",
    desc: "Any OpenAI-compatible endpoint",
    badge: "Custom URL",
    badgeVariant: "api",
    defaultName: "Custom",
    defaultUrl: "",
  },
];

// ── Props ─────────────────────────────────────────────────────────────────────

interface Step3ProviderProps {
  /** Re-hydrate form state when user navigates back to this step. */
  initialDraft: ProviderDraft | null;
  onBack: () => void;
  /** Passes the collected draft up (or null if skipped). No API calls here. */
  onNext: (draft: ProviderDraft | null) => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function Step3Provider({
  initialDraft,
  onBack,
  onNext,
}: Step3ProviderProps) {
  const [selectedKind, setSelectedKind] = useState<ProviderKind | null>(
    initialDraft?.kind ?? null,
  );
  const [name, setName] = useState(initialDraft?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(initialDraft?.base_url ?? "");
  const [apiKey, setApiKey] = useState(initialDraft?.api_key ?? "");

  // Copilot auth state — only relevant for validation before advancing
  const [copilotAuthed, setCopilotAuthed] = useState(false);

  // Check Copilot auth status whenever Copilot is selected
  useEffect(() => {
    if (selectedKind !== "copilot") return;
    let cancelled = false;
    copilotApi
      .authStatus()
      .then((res) => {
        if (!cancelled) setCopilotAuthed(res.data.authenticated);
      })
      .catch(() => {
        /* ignore */
      });
    return () => {
      cancelled = true;
    };
  }, [selectedKind]);

  // Poll auth status every 3s while copilot is selected so the Next button
  // activates automatically once the user completes the GitHub flow.
  useEffect(() => {
    if (selectedKind !== "copilot" || copilotAuthed) return;
    const interval = setInterval(async () => {
      try {
        const res = await copilotApi.authStatus();
        if (res.data.authenticated) setCopilotAuthed(true);
      } catch {
        /* ignore */
      }
    }, 3000);
    return () => clearInterval(interval);
  }, [selectedKind, copilotAuthed]);

  // When kind changes, pre-fill config defaults (unless re-hydrating)
  const handleSelectKind = useCallback((kind: ProviderKind) => {
    setSelectedKind(kind);
    const opt = PROVIDER_OPTIONS.find((o) => o.kind === kind)!;
    setName(opt.defaultName);
    setBaseUrl(opt.defaultUrl);
    setApiKey("");
    if (kind === "copilot") {
      // Re-check immediately when switching to copilot
      copilotApi
        .authStatus()
        .then((res) => setCopilotAuthed(res.data.authenticated))
        .catch(() => setCopilotAuthed(false));
    }
  }, []);

  const handleSkip = () => onNext(null);

  const handleNext = () => {
    if (!selectedKind) {
      onNext(null);
      return;
    }
    onNext({
      kind: selectedKind,
      name: name.trim() || selectedKind,
      base_url: baseUrl.trim(),
      api_key: apiKey.trim(),
    });
  };

  // Next is disabled when:
  // - Copilot selected but not yet authenticated
  // - Non-copilot selected but missing required fields
  const isNextDisabled =
    (selectedKind === "copilot" && !copilotAuthed) ||
    (selectedKind !== null &&
      selectedKind !== "copilot" &&
      (!name.trim() || !baseUrl.trim()));

  return (
    <div>
      {/* Step heading */}
      <h2
        style={{
          fontSize: 18,
          fontWeight: 700,
          color: "var(--text-primary)",
          letterSpacing: "-0.01em",
          marginBottom: 6,
        }}
      >
        Connect an AI provider
      </h2>
      <p
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          lineHeight: 1.6,
          marginBottom: 20,
        }}
      >
        Choose where your AI models come from. You can add more providers later
        in Settings.
      </p>

      {/* Provider kind picker cards */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          marginBottom: 16,
        }}
      >
        {PROVIDER_OPTIONS.map((opt) => {
          const isSelected = selectedKind === opt.kind;
          return (
            <button
              key={opt.kind}
              type="button"
              onClick={() => handleSelectKind(opt.kind)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                padding: "12px 14px",
                borderRadius: 10,
                background: isSelected
                  ? "var(--accent-muted)"
                  : "var(--bg-tertiary)",
                border: isSelected
                  ? "1px solid var(--accent-primary)"
                  : "1px solid var(--border-subtle)",
                cursor: "pointer",
                textAlign: "left",
                transition: "background 0.15s, border-color 0.15s",
                fontFamily: "inherit",
                width: "100%",
              }}
            >
              <span style={{ fontSize: 22, flexShrink: 0 }}>{opt.icon}</span>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontSize: 13,
                    fontWeight: 600,
                    color: "var(--text-primary)",
                    marginBottom: 2,
                  }}
                >
                  {opt.name}
                </div>
                <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                  {opt.desc}
                </div>
              </div>
              {/* Badge */}
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  padding: "2px 9px",
                  borderRadius: 20,
                  flexShrink: 0,
                  ...(opt.badgeVariant === "free"
                    ? {
                        background: "rgba(106,158,91,0.15)",
                        color: "var(--success)",
                        border: "1px solid rgba(106,158,91,0.3)",
                      }
                    : {
                        background: "rgba(90,130,196,0.12)",
                        color: "var(--info)",
                        border: "1px solid rgba(90,130,196,0.3)",
                      }),
                }}
              >
                {opt.badge}
              </span>
            </button>
          );
        })}
      </div>

      {/* Dynamic config panel */}
      {selectedKind && (
        <div
          style={{
            background: "var(--bg-tertiary)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 10,
            padding: "16px 18px",
            marginBottom: 4,
            display: "flex",
            flexDirection: "column",
            gap: 14,
          }}
        >
          {selectedKind === "copilot" ? (
            /* Copilot — embed the existing auth section. Auth itself is handled
               by the existing device-code flow in CopilotAuthSection. The token
               is written to disk by the server's auth-poll endpoint. On Step 5,
               SetupWizard will create the provider record once the user row
               exists. */
            <div>
              <CopilotAuthSection />
              {!copilotAuthed && (
                <p
                  style={{
                    fontSize: 12,
                    color: "var(--text-tertiary)",
                    marginTop: 10,
                    lineHeight: 1.5,
                  }}
                >
                  Complete authentication above, then click{" "}
                  <strong style={{ color: "var(--text-secondary)" }}>
                    Next →
                  </strong>{" "}
                  to continue.
                </p>
              )}
            </div>
          ) : (
            /* OpenAI / Anthropic / Custom */
            <>
              <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                <FieldLabel>Provider Name</FieldLabel>
                <FieldInput
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="e.g. OpenAI"
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                <FieldLabel>Base URL</FieldLabel>
                <FieldInput
                  type="text"
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  placeholder="https://api.openai.com/v1"
                  mono
                />
                <FieldHint>
                  OpenAI-compatible endpoint URL for this provider.
                </FieldHint>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                <FieldLabel>API Key</FieldLabel>
                <FieldInput
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-…"
                  mono
                  autoComplete="new-password"
                />
                <FieldHint>
                  Leave blank if using a local model without authentication.
                </FieldHint>
              </div>
            </>
          )}
        </div>
      )}

      <WizardNavRow
        onBack={onBack}
        onNext={handleNext}
        onSkip={handleSkip}
        nextLabel="Next →"
        nextDisabled={isNextDisabled}
      />
    </div>
  );
}

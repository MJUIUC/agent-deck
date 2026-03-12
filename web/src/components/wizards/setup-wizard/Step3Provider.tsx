import React, { useState, useEffect } from "react";
import { WizardNavRow } from "../shared/WizardNavRow";
import { FieldInput, FieldLabel, FieldHint } from "../../settings/shared";
import { CopilotAuthSection } from "../../settings/CopilotAuthSection";
import { providersApi, modelsApi, copilotApi } from "@/api/client";
import type { Provider } from "@/types";

// ── Step3Provider ─────────────────────────────────────────────────────────────
// Third step of the setup wizard. Provider kind picker cards with a dynamic
// config panel below. Copilot uses the embedded CopilotAuthSection. Other
// providers get name/base_url/api_key fields.
// On Next: creates the provider + syncs models, stores provider ID in wizard state.
// Skippable.

export type ProviderKind = "copilot" | "openai" | "anthropic" | "custom" | null;

interface ProviderConfig {
  name: string;
  base_url: string;
  api_key: string;
}

interface Step3ProviderProps {
  onBack: () => void;
  onNext: (providerId: string | null) => void;
}

const PROVIDER_OPTIONS: {
  kind: ProviderKind & string;
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

export function Step3Provider({ onBack, onNext }: Step3ProviderProps) {
  const [selectedKind, setSelectedKind] = useState<ProviderKind>(null);
  const [config, setConfig] = useState<ProviderConfig>({
    name: "",
    base_url: "",
    api_key: "",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // For Copilot: track whether auth has completed so we can enable Next
  const [copilotAuthed, setCopilotAuthed] = useState(false);

  // Check Copilot auth status when Copilot is selected
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

  // When kind changes, pre-fill config defaults
  const handleSelectKind = (kind: ProviderKind) => {
    setSelectedKind(kind);
    setError(null);
    const opt = PROVIDER_OPTIONS.find((o) => o.kind === kind);
    if (opt) {
      setConfig({
        name: opt.defaultName,
        base_url: opt.defaultUrl,
        api_key: "",
      });
    }
    if (kind === "copilot") {
      // Re-check auth when switching to copilot
      copilotApi
        .authStatus()
        .then((res) => setCopilotAuthed(res.data.authenticated))
        .catch(() => setCopilotAuthed(false));
    }
  };

  const handleSkip = () => onNext(null);

  const handleNext = async () => {
    if (!selectedKind) {
      // Nothing selected — treat as skip
      onNext(null);
      return;
    }

    setSaving(true);
    setError(null);

    try {
      if (selectedKind === "copilot") {
        // Check that auth is complete
        const status = await copilotApi.authStatus();
        if (!status.data.authenticated) {
          setError("Please complete GitHub authentication before continuing.");
          setSaving(false);
          return;
        }

        // Check if a Copilot provider already exists
        const existing = await providersApi.list();
        let copilotProvider: Provider | undefined = existing.data.find(
          (p) => p.kind === "copilot",
        );

        if (!copilotProvider) {
          const created = await providersApi.create({
            name: "GitHub Copilot",
            kind: "copilot",
            base_url: "http://localhost:4141/v1",
          });
          copilotProvider = created.data;
        }

        // Sync models
        try {
          await modelsApi.sync(copilotProvider.id);
        } catch {
          // Model sync failure is non-fatal in setup wizard
        }

        onNext(copilotProvider.id);
      } else {
        // API-key / custom provider
        if (!config.name.trim()) {
          setError("Provider name is required.");
          setSaving(false);
          return;
        }
        if (!config.base_url.trim()) {
          setError("Base URL is required.");
          setSaving(false);
          return;
        }

        const created = await providersApi.create({
          name: config.name.trim(),
          kind: "api_key",
          base_url: config.base_url.trim(),
          api_key: config.api_key.trim() || undefined,
        });

        // Sync models (best-effort)
        try {
          await modelsApi.sync(created.data.id);
        } catch {
          // Non-fatal
        }

        onNext(created.data.id);
      }
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to save provider.",
      );
    } finally {
      setSaving(false);
    }
  };

  const isNextDisabled =
    saving ||
    (selectedKind === "copilot" && !copilotAuthed) ||
    (selectedKind !== null &&
      selectedKind !== "copilot" &&
      (!config.name.trim() || !config.base_url.trim()));

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
              onClick={() => handleSelectKind(opt.kind as ProviderKind)}
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
            /* Copilot — embed existing auth section */
            <div>
              <CopilotAuthSection />
              {copilotAuthed ? null : (
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
              {/* Poll auth status periodically so Next button activates automatically */}
              <CopilotAuthPoller onAuthenticated={() => setCopilotAuthed(true)} />
            </div>
          ) : (
            /* OpenAI / Anthropic / Custom — name + base_url + api_key */
            <>
              <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                <FieldLabel>Provider Name</FieldLabel>
                <FieldInput
                  type="text"
                  value={config.name}
                  onChange={(e) =>
                    setConfig((c) => ({ ...c, name: e.target.value }))
                  }
                  placeholder="e.g. OpenAI"
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                <FieldLabel>Base URL</FieldLabel>
                <FieldInput
                  type="text"
                  value={config.base_url}
                  onChange={(e) =>
                    setConfig((c) => ({ ...c, base_url: e.target.value }))
                  }
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
                  value={config.api_key}
                  onChange={(e) =>
                    setConfig((c) => ({ ...c, api_key: e.target.value }))
                  }
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

      {/* Error message */}
      {error && (
        <div
          style={{
            marginTop: 10,
            fontSize: 12,
            color: "var(--error)",
            background: "rgba(196,90,90,0.08)",
            border: "1px solid rgba(196,90,90,0.25)",
            borderRadius: 7,
            padding: "8px 12px",
          }}
        >
          ⚠ {error}
        </div>
      )}

      <WizardNavRow
        onBack={onBack}
        onNext={handleNext}
        onSkip={handleSkip}
        nextLabel={saving ? "Saving…" : "Next →"}
        nextDisabled={isNextDisabled}
      />
    </div>
  );
}

// ── CopilotAuthPoller ─────────────────────────────────────────────────────────
// Polls the Copilot auth status every 3s and calls onAuthenticated when it
// flips to true. Mounts/unmounts with the Copilot config panel.

function CopilotAuthPoller({
  onAuthenticated,
}: {
  onAuthenticated: () => void;
}) {
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const res = await copilotApi.authStatus();
        if (res.data.authenticated) {
          onAuthenticated();
          clearInterval(interval);
        }
      } catch {
        // ignore
      }
    }, 3000);
    return () => clearInterval(interval);
  }, [onAuthenticated]);

  return null;
}

import React, { useState, useEffect, useCallback } from "react";
import { WizardShell } from "../shared/WizardShell";
import type { WizardStepMeta } from "../shared/types";
import { Step1Welcome } from "./Step1Welcome";
import { Step2Name } from "./Step2Name";
import { Step3Provider } from "./Step3Provider";
import { Step4Persona } from "./Step4Persona";
import type { PersonaConfig } from "./Step4Persona";
import { Step5Done } from "./Step5Done";
import { personasApi, modelsApi, setupApi, providersApi } from "@/api/client";
import type { Model } from "@/types";

// ── SetupWizard ───────────────────────────────────────────────────────────────
// Orchestrator for the first-run setup wizard. Owns all wizard state and drives
// step transitions. Renders the appropriate WizardShell + step component for
// each step.
//
// Steps (1-based):
//   1 — Welcome        (no indicator)
//   2 — Your Name
//   3 — Add a Provider
//   4 — Create First Persona
//   5 — Done

const STEPS: WizardStepMeta[] = [
  { label: "Welcome" },
  { label: "Your name" },
  { label: "Provider", skippable: true },
  { label: "Persona", skippable: true },
  { label: "Done" },
];

interface SetupWizardProps {
  /** Called after POST /api/setup/complete succeeds */
  onComplete: () => void;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
  // ── Step state ──────────────────────────────────────────────────────────────
  const [step, setStep] = useState(1);

  // ── Wizard data ─────────────────────────────────────────────────────────────
  const [displayName, setDisplayName] = useState("");
  const [providerId, setProviderId] = useState<string | null>(null);
  const [providerName, setProviderName] = useState<string | null>(null);
  const [persona, setPersona] = useState<PersonaConfig | null>(null);
  const [models, setModels] = useState<Model[]>([]);

  // ── Completion state ────────────────────────────────────────────────────────
  const [saving, setSaving] = useState(false);

  // When a provider is selected in Step 3, load its models for Step 4
  useEffect(() => {
    if (!providerId) {
      setModels([]);
      return;
    }
    modelsApi
      .list(providerId)
      .then((res) => setModels(res.data.filter((m) => m.enabled !== false)))
      .catch(() => setModels([]));
  }, [providerId]);

  // ── Navigation helpers ──────────────────────────────────────────────────────

  const goTo = useCallback((s: number) => {
    setStep(s);
    // Scroll to top of wizard on step change (in case card content is tall)
    window.scrollTo({ top: 0, behavior: "smooth" });
  }, []);

  // ── Step 3 completion ───────────────────────────────────────────────────────

  const handleProviderNext = useCallback(
    async (newProviderId: string | null) => {
      setProviderId(newProviderId);

      // Resolve provider display name for the Step 5 summary
      if (newProviderId) {
        try {
          const provRes = await providersApi.get(newProviderId);
          setProviderName(provRes.data.name);
        } catch {
          setProviderName("Configured");
        }
      } else {
        setProviderName(null);
      }

      goTo(4);
    },
    [goTo],
  );

  // ── Step 4 completion ───────────────────────────────────────────────────────

  const handlePersonaNext = useCallback(
    async (config: PersonaConfig | null) => {
      setPersona(config);

      if (config) {
        try {
          await personasApi.create({
            name: config.name,
            emoji: config.emoji,
            system_prompt: config.system_prompt,
            default_model: config.default_model ?? undefined,
            default_provider: providerId ?? undefined,
          });
        } catch {
          // Non-fatal in wizard flow — user can configure via Settings
        }
      }

      goTo(5);
    },
    [goTo, providerId],
  );

  // ── Step 5 completion ───────────────────────────────────────────────────────

  const handleComplete = useCallback(async () => {
    setSaving(true);
    try {
      await setupApi.complete(displayName.trim() || "User");
      onComplete();
    } catch (err) {
      // If already complete (409 Conflict), treat as success
      const msg = err instanceof Error ? err.message : "";
      if (msg.includes("409") || msg.toLowerCase().includes("already")) {
        onComplete();
      }
      // Otherwise keep saving state to show feedback — don't throw
    } finally {
      setSaving(false);
    }
  }, [displayName, onComplete]);

  // ── Render ──────────────────────────────────────────────────────────────────

  // Step indicator is hidden on step 1 (Welcome)
  const showIndicator = step > 1;

  return (
    <WizardShell steps={STEPS} currentStep={step} showIndicator={showIndicator}>
      {step === 1 && <Step1Welcome onNext={() => goTo(2)} />}

      {step === 2 && (
        <Step2Name
          displayName={displayName}
          onChange={setDisplayName}
          onBack={() => goTo(1)}
          onNext={() => goTo(3)}
        />
      )}

      {step === 3 && (
        <Step3Provider onBack={() => goTo(2)} onNext={handleProviderNext} />
      )}

      {step === 4 && (
        <Step4Persona
          models={models}
          onBack={() => goTo(3)}
          onNext={handlePersonaNext}
        />
      )}

      {step === 5 && (
        <Step5Done
          displayName={displayName}
          providerName={providerName}
          persona={persona}
          saving={saving}
          onComplete={handleComplete}
        />
      )}
    </WizardShell>
  );
}

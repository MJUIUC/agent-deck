import React, { useState, useCallback } from "react";
import { WizardShell } from "../shared/WizardShell";
import type { WizardStepMeta } from "../shared/types";
import { Step1Welcome } from "./Step1Welcome";
import { Step2Name } from "./Step2Name";
import { Step3Provider } from "./Step3Provider";
import type { ProviderDraft } from "./Step3Provider";
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
  /** Called after all setup persistence succeeds */
  onComplete: () => void;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
  // ── Step state ──────────────────────────────────────────────────────────────
  const [step, setStep] = useState(1);

  // ── Wizard data — collected across steps, persisted all at once in Step 5 ──
  const [displayName, setDisplayName] = useState("");
  // providerDraft holds the raw form data; nothing is written to DB until Step 5
  const [providerDraft, setProviderDraft] = useState<ProviderDraft | null>(
    null,
  );
  const [persona, setPersona] = useState<PersonaConfig | null>(null);
  // models are populated after provider creation in Step 5, but we pre-fetch
  // a preview from the draft in Step 4 only when a copilot token already exists
  const [models, setModels] = useState<Model[]>([]);

  // ── Completion state ────────────────────────────────────────────────────────
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // ── Navigation helpers ──────────────────────────────────────────────────────

  const goTo = useCallback((s: number) => {
    setStep(s);
    window.scrollTo({ top: 0, behavior: "smooth" });
  }, []);

  // ── Step 3 completion — just store the draft, no API call yet ───────────────

  const handleProviderNext = useCallback(
    (draft: ProviderDraft | null) => {
      setProviderDraft(draft);
      setModels([]); // models unknown until provider is actually created in Step 5
      goTo(4);
    },
    [goTo],
  );

  // ── Step 4 completion — store persona config, no API call yet ──────────────

  const handlePersonaNext = useCallback(
    (config: PersonaConfig | null) => {
      setPersona(config);
      goTo(5);
    },
    [goTo],
  );

  // ── Step 5 completion — persist everything in order ────────────────────────
  //
  // Order matters:
  //   1. POST /api/setup/complete  → creates the user row (required by all below)
  //   2. POST /api/providers       → creates the provider (if one was configured)
  //   3. POST /api/providers/:id/models  → syncs models for the new provider
  //   4. POST /api/personas        → creates the persona (if one was configured)

  const handleComplete = useCallback(async () => {
    setSaving(true);
    setSaveError(null);

    try {
      // 1 — Create the user row
      await setupApi.complete(displayName.trim() || "User");

      let createdProviderId: string | null = null;

      // 2 — Create provider
      if (providerDraft) {
        try {
          const provRes = await providersApi.create({
            name: providerDraft.name,
            kind: providerDraft.kind,
            base_url: providerDraft.base_url,
            api_key: providerDraft.api_key || undefined,
          });
          createdProviderId = provRes.data.id;

          // 3 — Sync models (best-effort, non-fatal)
          try {
            await modelsApi.sync(createdProviderId);
          } catch {
            // ignore
          }
        } catch {
          // Provider creation failed — non-fatal, user can add via Settings
        }
      }

      // 4 — Create persona
      if (persona) {
        try {
          await personasApi.create({
            name: persona.name,
            emoji: persona.emoji,
            system_prompt: persona.system_prompt,
            default_model: persona.default_model ?? undefined,
            default_provider: createdProviderId ?? undefined,
          });
        } catch {
          // Non-fatal — user can create via Settings
        }
      }

      onComplete();
    } catch (err) {
      // setup/complete itself failed
      const msg = err instanceof Error ? err.message : "";
      // 409 = already complete from a previous attempt, treat as success
      if (msg.includes("409") || msg.toLowerCase().includes("already")) {
        onComplete();
        return;
      }
      setSaveError("Something went wrong. Please try again.");
    } finally {
      setSaving(false);
    }
  }, [displayName, providerDraft, persona, onComplete]);

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
        <Step3Provider
          initialDraft={providerDraft}
          onBack={() => goTo(2)}
          onNext={handleProviderNext}
        />
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
          providerName={providerDraft?.name ?? null}
          persona={persona}
          saving={saving}
          saveError={saveError}
          onComplete={handleComplete}
        />
      )}
    </WizardShell>
  );
}

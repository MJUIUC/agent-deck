import React, { useState, useCallback } from "react";
import { WizardShell } from "../shared/WizardShell";
import type { WizardStepMeta } from "../shared/types";
import { Step1Welcome } from "./Step1Welcome";
import { Step2Name } from "./Step2Name";
import { Step2bAboutYou } from "./Step2bAboutYou";
import type { AboutYouDraft } from "./Step2bAboutYou";
import { Step3Provider } from "./Step3Provider";
import type { ProviderDraft } from "./Step3Provider";
import { Step4Persona } from "./Step4Persona";
import type { PersonaConfig } from "./Step4Persona";
import { Step5Done } from "./Step5Done";
import {
  personasApi,
  setupApi,
  providersApi,
  modelsApi,
  profileApi,
} from "@/api/client";
import type { Model } from "@/types";

// ── SetupWizard ───────────────────────────────────────────────────────────────
// Orchestrator for the first-run setup wizard. Owns all wizard state and drives
// step transitions. Renders the appropriate WizardShell + step component for
// each step.
//
// Steps (1-based):
//   1 — Welcome        (no indicator)
//   2 — Your Name
//   3 — About You      (optional / skippable)
//   4 — Add a Provider
//   5 — Create First Persona
//   6 — Done

const STEPS: WizardStepMeta[] = [
  { label: "Welcome" },
  { label: "Your name" },
  { label: "About you", skippable: true },
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

  // ── Wizard data — collected across steps, persisted all at once in Step 6 ──
  const [displayName, setDisplayName] = useState("");
  // profileDraft holds the "About You" fields collected in Step 3
  const [profileDraft, setProfileDraft] = useState<AboutYouDraft | null>(null);
  // providerDraft holds the raw form data; nothing is written to DB until Step 6
  const [providerDraft, setProviderDraft] = useState<ProviderDraft | null>(
    null,
  );
  // Track whether the user explicitly skipped each optional step
  const [providerSkipped, setProviderSkipped] = useState(false);
  const [personaSkipped, setPersonaSkipped] = useState(false);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [persona, setPersona] = useState<PersonaConfig | null>(null);
  // models are populated after provider creation in Step 6, but we pre-fetch
  // a preview from the draft in Step 5 only when a copilot token already exists
  const [models, setModels] = useState<Model[]>([]);

  // ── Completion state ────────────────────────────────────────────────────────
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // ── Navigation helpers ──────────────────────────────────────────────────────

  const goTo = useCallback((s: number) => {
    setStep(s);
    window.scrollTo({ top: 0, behavior: "smooth" });
  }, []);

  // ── Step 3 completion — store About You draft, proceed to Provider ──────────

  const handleAboutYouNext = useCallback(
    (draft: AboutYouDraft | null) => {
      setProfileDraft(draft);
      goTo(4);
    },
    [goTo],
  );

  // ── Step 4 completion — store draft, then try to pre-fetch models ───────────
  // If draft is null the user skipped — jump straight to Step 6 (Step 5 is
  // meaningless without a provider).
  //
  // For Copilot (and any provider kind that may already exist in the DB from a
  // previous setup attempt), we look up the matching provider and load its
  // models now so the Step 5 dropdown is populated immediately.

  const handleProviderNext = useCallback(
    async (draft: ProviderDraft | null) => {
      setProviderDraft(draft);

      if (draft === null) {
        // Skipped provider — also mark persona as skipped and go straight to done
        setProviderSkipped(true);
        setPersonaSkipped(true);
        setPersona(null);
        setModels([]);
        goTo(6);
        return;
      }

      setProviderSkipped(false);

      // For Copilot, fetch models directly from the sidecar via the public
      // /api/providers/copilot/models endpoint — no auth or DB required, so
      // this works before setup is complete.
      // For other provider kinds there are no models yet (the provider hasn't
      // been created in the DB), so we leave the list empty.
      if (draft.kind === "copilot") {
        setModelsLoading(true);
        try {
          const res = await fetch("/api/providers/copilot/models");
          if (res.ok) {
            const json = await res.json();
            const items: Model[] = (json.data ?? []).map(
              (m: { id: string; display_name: string }) => ({
                id: m.id,
                provider_id: "copilot",
                model_id: m.id,
                display_name: m.display_name,
                enabled: true,
                created_at: "",
                updated_at: "",
              }),
            );
            setModels(items);
          } else {
            setModels([]);
          }
        } catch {
          setModels([]);
        } finally {
          setModelsLoading(false);
        }
      } else {
        setModels([]);
      }

      goTo(5);
    },
    [goTo],
  );

  // ── Step 5 completion — store persona config, no API call yet ──────────────

  const handlePersonaNext = useCallback(
    (config: PersonaConfig | null) => {
      setPersona(config);
      setPersonaSkipped(config === null);
      goTo(6);
    },
    [goTo],
  );

  // ── Step 6 completion — persist everything in order ────────────────────────
  //
  // Order matters:
  //   1. POST /api/setup/complete  → creates the user row (required by all below)
  //   2. PUT  /api/profile         → saves About You fields (best-effort)
  //   3. POST /api/providers       → creates the provider (if one was configured)
  //   4. POST /api/providers/:id/models  → syncs models for the new provider
  //   5. POST /api/personas        → creates the persona (if one was configured)

  const handleComplete = useCallback(async () => {
    setSaving(true);
    setSaveError(null);

    try {
      // 1 — Create the user row
      await setupApi.complete(displayName.trim() || "User");

      // 2 — Save profile (best-effort, non-fatal)
      if (
        profileDraft &&
        (profileDraft.role ||
          profileDraft.organization ||
          profileDraft.location ||
          profileDraft.about ||
          profileDraft.timezone)
      ) {
        try {
          await profileApi.update({
            role: profileDraft.role || null,
            organization: profileDraft.organization || null,
            location: profileDraft.location || null,
            about: profileDraft.about || null,
            timezone: profileDraft.timezone || null,
          });
        } catch {
          // Non-fatal — profile can be set in Settings
        }
      }

      let createdProviderId: string | null = null;
      // DB row ID of the synced model matching the user's selection, or null
      let resolvedModelId: string | null = null;

      // 3 — Create provider
      if (providerDraft) {
        try {
          const provRes = await providersApi.create({
            name: providerDraft.name,
            kind: providerDraft.kind,
            base_url: providerDraft.base_url,
            api_key: providerDraft.api_key || undefined,
          });
          createdProviderId = provRes.data.id;

          // 4 — Sync models then resolve the selected model to its DB row ID.
          // persona.default_model holds the raw sidecar model_id (e.g. "gpt-4o"),
          // which is NOT the DB primary key. We must look it up after sync or
          // the FK constraint on agent_personas.default_model will fail.
          try {
            const syncedModels = await modelsApi.sync(createdProviderId);
            if (persona?.default_model) {
              const match = syncedModels.data.find(
                (m) =>
                  m.model_id === persona.default_model ||
                  m.id === persona.default_model,
              );
              resolvedModelId = match?.id ?? null;
            }
          } catch {
            // ignore — persona will just have no default model
          }
        } catch {
          // Provider creation failed — non-fatal, user can add via Settings
        }
      }

      // 5 — Create persona
      if (persona) {
        try {
          await personasApi.create({
            name: persona.name,
            emoji: persona.emoji,
            system_prompt: persona.system_prompt,
            default_model: resolvedModelId ?? undefined,
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
  }, [displayName, profileDraft, providerDraft, persona, onComplete]);

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
        <Step2bAboutYou
          initialDraft={profileDraft}
          onBack={() => goTo(2)}
          onNext={handleAboutYouNext}
        />
      )}

      {step === 4 && (
        <Step3Provider
          initialDraft={providerDraft}
          onBack={() => goTo(3)}
          onNext={handleProviderNext}
        />
      )}

      {step === 5 && (
        <Step4Persona
          models={models}
          modelsLoading={modelsLoading}
          onBack={() => goTo(4)}
          onNext={handlePersonaNext}
        />
      )}

      {step === 6 && (
        <Step5Done
          displayName={displayName}
          providerName={providerDraft?.name ?? null}
          persona={persona}
          bothSkipped={providerSkipped && personaSkipped}
          saving={saving}
          saveError={saveError}
          onComplete={handleComplete}
        />
      )}
    </WizardShell>
  );
}

// ── Shared wizard types ───────────────────────────────────────────────────────

export interface WizardStepMeta {
  label: string;       // shown in the step indicator
  skippable?: boolean; // whether the skip link appears on this step
}

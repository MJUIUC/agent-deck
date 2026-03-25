import React, { useState } from "react";
import { WizardNavRow } from "../shared/WizardNavRow";
import {
  FieldInput,
  FieldLabel,
  FieldHint,
  FieldTextarea,
} from "../../settings/shared";

// ── Step2bAboutYou ────────────────────────────────────────────────────────────
// Optional "About You" step in the setup wizard.
// Collects role, organization, location, and about.
// Timezone is auto-detected from the browser and silently stored.
// All fields are optional — the step can be skipped entirely.

export interface AboutYouDraft {
  role: string;
  organization: string;
  location: string;
  about: string;
  timezone: string; // auto-detected, not shown as a field
}

interface Step2bAboutYouProps {
  initialDraft: AboutYouDraft | null;
  onBack: () => void;
  /** Passes the collected draft up (or null if skipped). */
  onNext: (draft: AboutYouDraft | null) => void;
}

export function Step2bAboutYou({
  initialDraft,
  onBack,
  onNext,
}: Step2bAboutYouProps) {
  const [role, setRole] = useState(initialDraft?.role ?? "");
  const [organization, setOrganization] = useState(
    initialDraft?.organization ?? "",
  );
  const [location, setLocation] = useState(initialDraft?.location ?? "");
  const [about, setAbout] = useState(initialDraft?.about ?? "");

  // Auto-detect timezone once on mount via lazy initializer
  const [detectedTimezone] = useState<string>(() => {
    if (initialDraft?.timezone) return initialDraft.timezone;
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return "";
    }
  });

  const handleSkip = () => onNext(null);

  const handleNext = () => {
    const draft: AboutYouDraft = {
      role: role.trim(),
      organization: organization.trim(),
      location: location.trim(),
      about: about.trim(),
      timezone: detectedTimezone,
    };
    onNext(draft);
  };

  const ABOUT_MAX = 500;
  const aboutCount = about.length;

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
        About you
      </h2>
      <p
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          lineHeight: 1.6,
          marginBottom: 24,
        }}
      >
        This helps your agents understand who they're talking to from the first
        message. You can update this anytime in Settings.
      </p>

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        {/* Role */}
        <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          <FieldLabel>Role</FieldLabel>
          <FieldInput
            type="text"
            value={role}
            placeholder="e.g. Senior Software Engineer"
            onChange={(e) => setRole(e.target.value)}
          />
        </div>

        {/* Organization */}
        <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          <FieldLabel>Organization</FieldLabel>
          <FieldInput
            type="text"
            value={organization}
            placeholder="e.g. Acme Corp"
            onChange={(e) => setOrganization(e.target.value)}
          />
        </div>

        {/* Location */}
        <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          <FieldLabel>Location</FieldLabel>
          <FieldInput
            type="text"
            value={location}
            placeholder="e.g. San Francisco, CA"
            onChange={(e) => setLocation(e.target.value)}
          />
          {detectedTimezone && (
            <FieldHint>
              Detected timezone: {detectedTimezone} — change in Settings
            </FieldHint>
          )}
        </div>

        {/* About */}
        <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          <FieldLabel>About</FieldLabel>
          <FieldTextarea
            value={about}
            placeholder="e.g. I prefer concise answers without unnecessary preamble. I work primarily in TypeScript and Rust."
            onChange={(e) => {
              if (e.target.value.length <= ABOUT_MAX) setAbout(e.target.value);
            }}
            style={{ minHeight: 90 }}
          />
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <FieldHint>
              Shown to all your personas as background context.
            </FieldHint>
            <span
              style={{
                fontSize: 11,
                color:
                  aboutCount >= ABOUT_MAX
                    ? "var(--error)"
                    : "var(--text-tertiary)",
              }}
            >
              {aboutCount} / {ABOUT_MAX}
            </span>
          </div>
        </div>
      </div>

      <WizardNavRow
        onBack={onBack}
        onNext={handleNext}
        onSkip={handleSkip}
        nextLabel="Next →"
      />
    </div>
  );
}

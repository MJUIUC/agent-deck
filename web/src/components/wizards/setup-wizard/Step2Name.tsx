import React, { useState } from "react";
import { WizardNavRow } from "../shared/WizardNavRow";
import { FieldInput, FieldLabel, FieldHint } from "../../settings/shared";

// ── Step2Name ─────────────────────────────────────────────────────────────────
// Second step of the setup wizard. Single display name input field.
// Next button is disabled until the field is non-empty.
// The display name is lifted up to the SetupWizard orchestrator for use in
// the final POST /api/setup/complete call.

interface Step2NameProps {
  displayName: string;
  onChange: (name: string) => void;
  onBack: () => void;
  onNext: () => void;
}

export function Step2Name({
  displayName,
  onChange,
  onBack,
  onNext,
}: Step2NameProps) {
  const [touched, setTouched] = useState(false);

  const trimmed = displayName.trim();
  const isValid = trimmed.length > 0;
  const showError = touched && !isValid;

  const handleNext = () => {
    setTouched(true);
    if (isValid) onNext();
  };

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
        What should we call you?
      </h2>
      <p
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          lineHeight: 1.6,
          marginBottom: 24,
        }}
      >
        This is used to personalize your experience. It's stored locally and
        never shared.
      </p>

      {/* Name field */}
      <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 4 }}>
        <FieldLabel>Your Name</FieldLabel>
        <FieldInput
          type="text"
          value={displayName}
          placeholder="e.g. Marcus"
          autoComplete="off"
          autoFocus
          style={
            showError
              ? { borderColor: "var(--error)", boxShadow: "0 0 0 3px rgba(196,90,90,0.15)" }
              : undefined
          }
          onChange={(e) => {
            setTouched(true);
            onChange(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleNext();
          }}
        />
        {showError ? (
          <span style={{ fontSize: 11, color: "var(--error)" }}>
            Please enter your name to continue.
          </span>
        ) : (
          <FieldHint>Just a first name or nickname is fine.</FieldHint>
        )}
      </div>

      <WizardNavRow
        onBack={onBack}
        onNext={handleNext}
        nextDisabled={!isValid}
      />
    </div>
  );
}

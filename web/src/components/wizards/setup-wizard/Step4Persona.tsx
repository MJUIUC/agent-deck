import React, { useState } from "react";
import { WizardNavRow } from "../shared/WizardNavRow";
import {
  FieldInput,
  FieldLabel,
  FieldHint,
  FieldTextarea,
} from "../../settings/shared";
import { EmojiPicker } from "../../settings/EmojiPicker";
import { FieldSelect } from "../../settings/shared";
import type { Model } from "@/types";

// ── Step4Persona ──────────────────────────────────────────────────────────────
// Fourth step of the setup wizard. Preset persona template cards: Aldous, Scout,
// Muse, Custom. Selecting a preset fills name, emoji, and system prompt with
// hardcoded defaults. Selecting "Custom" shows editable fields.
// Model dropdown populated from models synced in Step 3.
// Skippable.

export interface PersonaConfig {
  name: string;
  emoji: string;
  system_prompt: string;
  default_model: string | null;
}

interface Step4PersonaProps {
  models: Model[];
  modelsLoading?: boolean;
  onBack: () => void;
  onNext: (config: PersonaConfig | null) => void;
}

type PresetKey = "aldous" | "scout" | "muse" | "custom";

const PRESETS: {
  key: PresetKey;
  emoji: string;
  name: string;
  tagline: string;
  system_prompt: string;
}[] = [
  {
    key: "aldous",
    emoji: "🦉",
    name: "Aldous",
    tagline: "Thoughtful, precise. Deep analysis and code.",
    system_prompt:
      "You are Aldous, a thoughtful and precise assistant specializing in deep analysis, reasoning, and software development. You approach problems methodically, prefer accuracy over speed, and always explain your thinking. When writing code, you favor clarity and correctness. You ask clarifying questions before diving into complex tasks.",
  },
  {
    key: "scout",
    emoji: "🧠",
    name: "Scout",
    tagline: "Quick, organized. Planning and research.",
    system_prompt:
      "You are Scout, a quick and organized assistant focused on planning, research, and getting things done efficiently. You break complex tasks into clear steps, summarize information concisely, and help the user stay on top of their goals. You prefer structured responses with bullet points and headers when appropriate.",
  },
  {
    key: "muse",
    emoji: "🎨",
    name: "Muse",
    tagline: "Creative, expressive. Writing and ideas.",
    system_prompt:
      "You are Muse, a creative and expressive assistant who excels at writing, brainstorming, and generating ideas. You bring imagination and flair to every interaction. You're comfortable with ambiguity and enjoy exploring unconventional angles. You write with voice and personality, adapting your tone to match the user's creative vision.",
  },
  {
    key: "custom",
    emoji: "⚒",
    name: "Custom",
    tagline: "Start from scratch with your own prompt.",
    system_prompt: "",
  },
];

export function Step4Persona({
  models,
  modelsLoading = false,
  onBack,
  onNext,
}: Step4PersonaProps) {
  const [selectedKey, setSelectedKey] = useState<PresetKey>("aldous");
  const [customName, setCustomName] = useState("");
  const [customEmoji, setCustomEmoji] = useState("⚒");
  const [customPrompt, setCustomPrompt] = useState("");

  const [selectedModel, setSelectedModel] = useState<string>("");

  const isCustom = selectedKey === "custom";
  const hasModels = models.length > 0;
  const showModelLoading = modelsLoading;

  // Resolve the persona config to submit
  const getPersonaConfig = (): PersonaConfig => {
    if (isCustom) {
      return {
        name: customName.trim() || "Custom",
        emoji: customEmoji,
        system_prompt: customPrompt.trim(),
        default_model: selectedModel || null,
      };
    }
    const preset = PRESETS.find((p) => p.key === selectedKey)!;
    return {
      name: preset.name,
      emoji: preset.emoji,
      system_prompt: preset.system_prompt,
      default_model: selectedModel || null,
    };
  };

  const isNextDisabled =
    (isCustom && (!customName.trim() || !customPrompt.trim())) ||
    (hasModels && !selectedModel);

  const handleNext = () => {
    onNext(getPersonaConfig());
  };

  const handleSkip = () => {
    onNext(null);
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
        Choose your first agent persona
      </h2>
      <p
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          lineHeight: 1.6,
          marginBottom: 20,
        }}
      >
        Personas define how your agent thinks and responds. You can create more
        in Settings.
      </p>

      {/* Preset persona cards — 2×2 grid */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 8,
          marginBottom: 16,
        }}
      >
        {PRESETS.map((preset) => {
          const isSelected = selectedKey === preset.key;
          return (
            <button
              key={preset.key}
              type="button"
              onClick={() => setSelectedKey(preset.key)}
              style={{
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                gap: 6,
                padding: "14px 12px",
                borderRadius: 10,
                background: isSelected
                  ? "var(--accent-muted)"
                  : "var(--bg-tertiary)",
                border: isSelected
                  ? "1px solid var(--accent-primary)"
                  : "1px solid var(--border-subtle)",
                cursor: "pointer",
                textAlign: "center",
                transition: "background 0.15s, border-color 0.15s",
                fontFamily: "inherit",
              }}
            >
              <span style={{ fontSize: 28, lineHeight: 1 }}>
                {preset.emoji}
              </span>
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 600,
                  color: "var(--text-primary)",
                }}
              >
                {preset.name}
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  lineHeight: 1.4,
                }}
              >
                {preset.tagline}
              </div>
            </button>
          );
        })}
      </div>

      {/* Custom persona fields — shown only when "Custom" is selected */}
      {isCustom && (
        <div
          style={{
            background: "var(--bg-tertiary)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 10,
            padding: "16px 18px",
            marginBottom: 16,
            display: "flex",
            flexDirection: "column",
            gap: 14,
          }}
        >
          {/* Name field */}
          <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
            <FieldLabel>Persona Name</FieldLabel>
            <FieldInput
              type="text"
              value={customName}
              onChange={(e) => setCustomName(e.target.value)}
              placeholder="e.g. Orion"
              autoFocus
            />
          </div>

          {/* Emoji picker */}
          <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
            <FieldLabel>Emoji</FieldLabel>
            <EmojiPicker
              value={customEmoji}
              onChange={(emoji) => setCustomEmoji(emoji)}
            />
          </div>

          {/* System prompt */}
          <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
            <FieldLabel>System Prompt</FieldLabel>
            <FieldTextarea
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              rows={4}
              placeholder="You are a helpful assistant named Orion…"
              style={{ minHeight: 100 }}
            />
            <FieldHint>
              Describes how your agent thinks and responds. Be as specific as
              you like.
            </FieldHint>
          </div>
        </div>
      )}

      {/* Model selector */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 5,
          marginBottom: 4,
        }}
      >
        <FieldLabel>Default Model</FieldLabel>
        {showModelLoading ? (
          <div
            style={{
              background: "var(--bg-tertiary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 7,
              padding: "9px 12px",
              fontSize: 13,
              color: "var(--text-tertiary)",
              fontStyle: "italic",
            }}
          >
            Loading models…
          </div>
        ) : hasModels ? (
          <FieldSelect
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
          >
            <option value="">— None selected —</option>
            {models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.display_name}
              </option>
            ))}
          </FieldSelect>
        ) : (
          <div
            style={{
              background: "var(--bg-tertiary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 7,
              padding: "9px 12px",
              fontSize: 13,
              color: "var(--text-tertiary)",
              fontStyle: "italic",
            }}
          >
            No models available — add a provider in Step 3 first.
          </div>
        )}
        <FieldHint>
          {showModelLoading
            ? "Fetching available models…"
            : hasModels && !selectedModel
              ? "⚠ A model is required to continue. Select one above."
              : hasModels
                ? "The model this persona uses by default. Can be changed per thread."
                : "You can configure a model later in Settings → Providers."}
        </FieldHint>
      </div>

      <WizardNavRow
        onBack={onBack}
        onNext={handleNext}
        onSkip={handleSkip}
        nextLabel="Finish Setup →"
        nextDisabled={isNextDisabled}
      />
    </div>
  );
}

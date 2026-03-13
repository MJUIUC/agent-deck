import React, { useState, type FormEvent } from "react";
import type { AgentPersona, Provider, Model } from "@/types";
import { personasApi } from "@/api/client";
import {
  Btn,
  FieldHint,
  FieldInput,
  FieldLabel,
  FieldSelect,
  FieldTextarea,
  type PersonaFormData,
} from "./shared";
import { EmojiPicker } from "./EmojiPicker";

// ─── PersonaForm ──────────────────────────────────────────────────────────────

export interface PersonaFormProps {
  /** If set, the form is in edit mode for this persona. */
  editing: AgentPersona | null;
  providers: Provider[];
  modelsByProvider: Record<string, Model[]>;
  onSaved: () => void;
  onCancel: () => void;
}

/**
 * Add / edit form for a single persona.
 * Handles its own local state (fields, saving, validation).
 * Emoji selection is delegated to the EmojiPicker component.
 */
export function PersonaForm({
  editing,
  providers,
  modelsByProvider,
  onSaved,
  onCancel,
}: PersonaFormProps) {
  const [form, setForm] = useState<PersonaFormData>({
    name: editing?.name ?? "",
    emoji: editing?.emoji ?? "🦉",
    system_prompt: editing?.system_prompt ?? "",
    default_provider: editing?.default_provider ?? "",
    default_model: editing?.default_model ?? "",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const setField =
    (k: keyof PersonaFormData) =>
    (
      e: React.ChangeEvent<
        HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
      >,
    ) => {
      const val = e.target.value;
      setForm((prev) => {
        const next = { ...prev, [k]: val };
        // Reset model when provider changes
        if (k === "default_provider") next.default_model = "";
        return next;
      });
    };

  const availableModels = form.default_provider
    ? (modelsByProvider[form.default_provider] ?? []).filter((m) => m.enabled)
    : [];

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!form.name.trim()) return setError("Name is required.");
    if (!form.system_prompt.trim())
      return setError("System prompt is required.");
    if (!form.default_provider)
      return setError(
        "A default provider is required so the agent knows which AI to use.",
      );
    if (!form.default_model)
      return setError(
        "A default model is required — pick one from the provider's model list.",
      );
    setSaving(true);
    try {
      const payload = {
        name: form.name.trim(),
        emoji: form.emoji,
        system_prompt: form.system_prompt.trim(),
        default_provider: form.default_provider || undefined,
        default_model: form.default_model || undefined,
      };
      if (editing) {
        await personasApi.update(editing.id, payload);
      } else {
        await personasApi.create(payload);
      }
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border-default)",
        borderRadius: 12,
        padding: "22px 24px",
        marginBottom: 24,
      }}
    >
      {/* form-title */}
      <div
        style={{
          fontSize: 15,
          fontWeight: 600,
          marginBottom: 18,
          color: "var(--text-primary)",
          display: "flex",
          alignItems: "center",
          justifyContent: "flex-start",
        }}
      >
        <span>{editing ? "Edit Persona" : "New Persona"}</span>
      </div>

      <form onSubmit={handleSave}>
        {/* Emoji row */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 6,
            marginBottom: 14,
          }}
        >
          <FieldLabel>Emoji</FieldLabel>
          <EmojiPicker
            value={form.emoji}
            onChange={(emoji) => setForm((prev) => ({ ...prev, emoji }))}
          />
        </div>

        {/* form-grid: 2 cols, 14px gap */}
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}
        >
          {/* Name */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Name</FieldLabel>
            <FieldInput
              placeholder="e.g. Aldous"
              value={form.name}
              onChange={setField("name")}
            />
          </div>

          {/* Default Provider */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Default Provider</FieldLabel>
            <FieldSelect
              value={form.default_provider}
              onChange={setField("default_provider")}
            >
              <option value="">None — choose per thread</option>
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </FieldSelect>
          </div>

          {/* Default Model */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Default Model</FieldLabel>
            <FieldSelect
              value={form.default_model}
              onChange={setField("default_model")}
              disabled={!form.default_provider}
            >
              <option value="">None — choose per thread</option>
              {availableModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.display_name}
                </option>
              ))}
            </FieldSelect>
            {form.default_provider && availableModels.length === 0 && (
              <FieldHint>
                No enabled models — sync models for this provider first.
              </FieldHint>
            )}
          </div>

          {/* System Prompt — full width */}
          <div
            style={{
              gridColumn: "1 / -1",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            <FieldLabel>System Prompt</FieldLabel>
            <FieldTextarea
              placeholder="You are Aldous, a thoughtful and precise assistant. You excel at deep analysis, technical problems, and careful reasoning…"
              value={form.system_prompt}
              onChange={setField("system_prompt")}
              style={{ minHeight: 140 }}
            />
            <FieldHint>
              Defines the agent's core personality and behavior. Always
              prepended to the conversation context.
            </FieldHint>
          </div>
        </div>

        {error && (
          <p style={{ marginTop: 10, fontSize: 12, color: "var(--error)" }}>
            {error}
          </p>
        )}

        {/* form-actions */}
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            marginTop: 18,
            paddingTop: 16,
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          <Btn variant="ghost" onClick={onCancel}>
            Cancel
          </Btn>
          <button
            type="submit"
            disabled={saving}
            style={{
              padding: "8px 16px",
              borderRadius: 7,
              fontSize: 13,
              fontWeight: 500,
              cursor: saving ? "default" : "pointer",
              border: "none",
              background: "var(--accent-primary)",
              color: "var(--text-inverse)",
              opacity: saving ? 0.6 : 1,
              fontFamily: "inherit",
            }}
          >
            {saving ? "Saving…" : "Save Persona"}
          </button>
        </div>
      </form>
    </div>
  );
}

import React, { useState, type FormEvent } from "react";
import { Renew, Flash } from "@carbon/icons-react";
import type { Provider } from "@/types";
import { providersApi } from "@/api/client";
import {
  Btn,
  FieldHint,
  FieldInput,
  FieldLabel,
  FieldSelect,
  KIND_DEFAULT_URLS,
  KIND_URL_HINTS,
  type ProviderFormData,
} from "./shared";
import { CopilotAuthSection } from "./CopilotAuthSection";

// ─── ProviderForm ─────────────────────────────────────────────────────────────

export interface ProviderFormProps {
  /** If set, the form is in edit mode for this provider. */
  editing: Provider | null;
  onSaved: () => void;
  onCancel: () => void;
}

/**
 * Add / edit form for a single provider.
 * Handles its own local state (fields, saving, test result).
 */
export function ProviderForm({
  editing,
  onSaved,
  onCancel,
}: ProviderFormProps) {
  // Normalise legacy kind values saved before the two-kind model
  const normaliseKind = (
    k: string,
  ): "openai" | "anthropic" | "custom" | "copilot" | "" => {
    if (k === "copilot") return "copilot";
    if (k === "openai") return "openai";
    if (k === "anthropic") return "anthropic";
    if (k === "custom") return "custom";
    if (k === "api_key") return "openai"; // legacy fallback
    return "";
  };

  const [form, setForm] = useState<ProviderFormData>({
    name: editing?.name ?? "",
    kind: normaliseKind(editing?.kind ?? ""),
    base_url: editing?.base_url ?? "",
    api_key: "",
  });
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    models: string[];
    error?: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const setField =
    (k: keyof ProviderFormData) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => {
      const val = e.target.value;
      setForm((prev) => {
        const next = { ...prev, [k]: val } as ProviderFormData;
        if (k === "kind" && !editing)
          next.base_url = KIND_DEFAULT_URLS[val] ?? "";
        return next;
      });
      setTestResult(null);
    };

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!form.name.trim()) return setError("Name is required.");
    if (!form.kind) return setError("Provider kind is required.");
    if (form.kind !== "copilot" && !form.base_url.trim())
      return setError("Base URL is required.");
    setSaving(true);
    try {
      const payload = {
        name: form.name.trim(),
        kind: form.kind as string,
        base_url:
          form.kind === "copilot"
            ? KIND_DEFAULT_URLS.copilot
            : form.base_url.trim(),
        ...(form.kind !== "copilot" && form.api_key
          ? { api_key: form.api_key }
          : {}),
      };
      if (editing) {
        await providersApi.update(editing.id, payload);
      } else {
        await providersApi.create(payload);
      }
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed.");
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    if (!editing) return;
    setTesting(true);
    setTestResult(null);
    try {
      const res = await providersApi.test(editing.id);
      setTestResult({ models: res.data.models });
    } catch (err) {
      setTestResult({
        models: [],
        error: err instanceof Error ? err.message : "Connection failed.",
      });
    } finally {
      setTesting(false);
    }
  };

  const isCopilot = form.kind === "copilot";

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
        <span>{editing ? "Edit Provider" : "Add Provider"}</span>
      </div>

      <form onSubmit={handleSave}>
        {/* form-grid: 2 cols, 14px gap */}
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}
        >
          {/* Name */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Name</FieldLabel>
            <FieldInput
              placeholder={isCopilot ? "e.g. GitHub Copilot" : "e.g. My OpenAI"}
              value={form.name}
              onChange={setField("name")}
            />
          </div>

          {/* Kind — two options only */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <FieldLabel>Kind</FieldLabel>
            <FieldSelect
              value={form.kind}
              onChange={setField("kind")}
              disabled={!!editing}
            >
              <option value="">Select provider type…</option>
              <option value="openai">OpenAI</option>
              <option value="anthropic">Anthropic</option>
              <option value="custom">Custom (OpenAI-compatible)</option>
              <option value="copilot">GitHub Copilot</option>
            </FieldSelect>
            {editing && (
              <FieldHint>Kind cannot be changed after creation.</FieldHint>
            )}
          </div>

          {/* API Key provider — base URL + key */}
          {(form.kind === "openai" ||
            form.kind === "anthropic" ||
            form.kind === "custom") && (
            <>
              <div
                style={{
                  gridColumn: "1 / -1",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <FieldLabel>Base URL</FieldLabel>
                <FieldInput
                  mono
                  placeholder="https://api.openai.com/v1"
                  value={form.base_url}
                  onChange={setField("base_url")}
                />
                <FieldHint>
                  {KIND_URL_HINTS[form.kind] ?? KIND_URL_HINTS.custom}
                </FieldHint>
              </div>

              <div
                style={{
                  gridColumn: "1 / -1",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <FieldLabel>API Key</FieldLabel>
                <FieldInput
                  mono
                  type="password"
                  placeholder={
                    editing ? "Leave blank to keep existing" : "sk-…"
                  }
                  value={form.api_key}
                  onChange={setField("api_key")}
                />
                <FieldHint>
                  Encrypted at rest. Never returned in API responses.
                </FieldHint>
              </div>
            </>
          )}

          {/* Copilot provider — no URL/key fields, show auth widget instead */}
          {form.kind === "copilot" && (
            <div style={{ gridColumn: "1 / -1" }}>
              <CopilotAuthSection />
            </div>
          )}
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
          {editing && form.kind !== "copilot" && (
            <Btn
              variant="ghost"
              onClick={handleTest}
              disabled={testing}
              style={{ marginRight: "auto" }}
            >
              {testing ? (
                <Renew
                  size={13}
                  style={{ animation: "spin 0.8s linear infinite" }}
                />
              ) : (
                <Flash size={13} />
              )}
              {testing ? "Testing…" : "Test Connection"}
            </Btn>
          )}
          <Btn variant="ghost" onClick={onCancel}>
            Cancel
          </Btn>
          <button
            type="submit"
            disabled={saving || !form.kind}
            style={{
              padding: "8px 16px",
              borderRadius: 7,
              fontSize: 13,
              fontWeight: 500,
              cursor: saving || !form.kind ? "default" : "pointer",
              border: "none",
              background: "var(--accent-primary)",
              color: "var(--text-inverse)",
              opacity: saving || !form.kind ? 0.5 : 1,
              fontFamily: "inherit",
            }}
          >
            {saving ? "Saving…" : "Save Provider"}
          </button>
        </div>

        {/* Test result panel */}
        {testResult && (
          <div
            style={{
              display: "block",
              background: "var(--bg-tertiary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 8,
              padding: "12px 14px",
              marginTop: 10,
            }}
          >
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                marginBottom: 8,
              }}
            >
              {testResult.error ? "Connection Failed" : "Available Models"}
            </div>
            {testResult.error ? (
              <span style={{ fontSize: 13, color: "var(--error)" }}>
                {testResult.error}
              </span>
            ) : testResult.models.length === 0 ? (
              <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                No models returned.
              </span>
            ) : (
              <div>
                {testResult.models.slice(0, 12).map((m) => (
                  <span
                    key={m}
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      background: "var(--bg-elevated)",
                      border: "1px solid var(--border-subtle)",
                      borderRadius: 5,
                      padding: "3px 9px",
                      fontSize: 12,
                      color: "var(--text-secondary)",
                      margin: "3px 3px 3px 0",
                      fontFamily: '"SF Mono","Fira Code",monospace',
                    }}
                  >
                    {m}
                  </span>
                ))}
                {testResult.models.length > 12 && (
                  <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                    +{testResult.models.length - 12} more
                  </span>
                )}
              </div>
            )}
          </div>
        )}
      </form>
    </div>
  );
}

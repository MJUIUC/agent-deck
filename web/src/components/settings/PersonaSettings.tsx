import React, { useState, useEffect, useCallback } from "react";
import { Plus, Pencil, Trash2 } from "lucide-react";
import type { AgentPersona, Provider, Model } from "@/types";
import { personasApi, providersApi, modelsApi } from "@/api/client";
import { Btn } from "./shared";
import { PersonaForm } from "./PersonaForm";

// ─── PersonaCard ──────────────────────────────────────────────────────────────

interface PersonaCardProps {
  persona: AgentPersona;
  providers: Provider[];
  modelsByProvider: Record<string, Model[]>;
  onEdit: () => void;
  onDelete: () => void;
}

function PersonaCard({
  persona,
  providers,
  modelsByProvider,
  onEdit,
  onDelete,
}: PersonaCardProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [hovered, setHovered] = useState(false);

  const provider = providers.find((p) => p.id === persona.default_provider);
  const allModels = persona.default_provider
    ? (modelsByProvider[persona.default_provider] ?? [])
    : [];
  const model = allModels.find((m) => m.id === persona.default_model);
  const modelLabel =
    model && provider
      ? `${model.display_name} · ${provider.name}`
      : provider
        ? provider.name
        : null;

  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: `1px solid ${hovered ? "var(--border-default)" : "var(--border-subtle)"}`,
        borderRadius: 12,
        padding: 18,
        cursor: "default",
        transition: "border-color 0.15s",
        position: "relative",
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* persona-card-top */}
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: 12,
          marginBottom: 12,
        }}
      >
        {/* persona-avatar: 52×52, circle */}
        <div
          style={{
            width: 52,
            height: 52,
            borderRadius: "50%",
            background: "var(--bg-elevated)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 24,
            flexShrink: 0,
          }}
        >
          {persona.emoji}
        </div>

        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontSize: 15,
              fontWeight: 700,
              color: "var(--text-primary)",
              marginBottom: 2,
              display: "flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            {persona.name}
          </div>
          {modelLabel && (
            <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
              {modelLabel}
            </div>
          )}
        </div>
      </div>

      {/* System prompt preview — 2-line clamp */}
      <div
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          lineHeight: 1.5,
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
          borderTop: "1px solid var(--border-subtle)",
          paddingTop: 10,
          marginBottom: 0,
        }}
      >
        {persona.system_prompt}
      </div>

      {/* persona-card-actions */}
      <div style={{ display: "flex", gap: 6, marginTop: 12 }}>
        <Btn sm variant="ghost" onClick={onEdit}>
          <Pencil size={12} /> Edit
        </Btn>
        {confirmDelete ? (
          <>
            <span
              style={{
                fontSize: 11,
                color: "var(--error)",
                alignSelf: "center",
              }}
            >
              Sure?
            </span>
            <Btn sm variant="danger" onClick={onDelete}>
              Yes, delete
            </Btn>
            <Btn sm variant="ghost" onClick={() => setConfirmDelete(false)}>
              Cancel
            </Btn>
          </>
        ) : (
          <Btn
            sm
            variant="danger"
            style={{ marginLeft: "auto" }}
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 size={12} /> Delete
          </Btn>
        )}
      </div>
    </div>
  );
}

// ─── AddPersonaCard ───────────────────────────────────────────────────────────

function AddPersonaCard({ onClick }: { onClick: () => void }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: "transparent",
        border: `2px dashed ${hovered ? "var(--border-strong)" : "var(--border-default)"}`,
        borderRadius: 12,
        minHeight: 160,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        color: hovered ? "var(--text-secondary)" : "var(--text-tertiary)",
        fontSize: 13,
        fontFamily: "inherit",
        transition: "border-color 0.15s, color 0.15s",
      }}
    >
      <span style={{ fontSize: 22 }}>＋</span>
      <span>New Persona</span>
    </button>
  );
}

// ─── PersonaSettings ──────────────────────────────────────────────────────────

export interface PersonaSettingsProps {
  onDataChanged: () => void;
}

/**
 * Personas tab content — list, add, edit, delete.
 * Also loads providers + models so the PersonaForm can populate its dropdowns.
 * Manages its own data loading and CRUD state.
 */
export function PersonaSettings({ onDataChanged }: PersonaSettingsProps) {
  const [personas, setPersonas] = useState<AgentPersona[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<
    Record<string, Model[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<AgentPersona | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [pRes, provRes] = await Promise.all([
        personasApi.list(),
        providersApi.list(),
      ]);
      setPersonas(pRes.data);
      setProviders(provRes.data);
      const entries = await Promise.all(
        provRes.data.map(async (p) => {
          try {
            const mRes = await modelsApi.list(p.id);
            return [p.id, mRes.data] as [string, Model[]];
          } catch {
            return [p.id, []] as [string, Model[]];
          }
        }),
      );
      setModelsByProvider(Object.fromEntries(entries));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleSaved = useCallback(async () => {
    setShowForm(false);
    setEditing(null);
    await loadData();
    onDataChanged();
  }, [loadData, onDataChanged]);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await personasApi.delete(id);
        await loadData();
        onDataChanged();
      } catch {
        /* ignore */
      }
    },
    [loadData, onDataChanged],
  );

  const openAdd = useCallback(() => {
    setEditing(null);
    setShowForm(true);
  }, []);

  const openEdit = useCallback((p: AgentPersona) => {
    setEditing(p);
    setShowForm(true);
  }, []);

  return (
    <div>
      {/* page-header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 24,
        }}
      >
        <div>
          <div
            style={{ fontSize: 18, fontWeight: 700, letterSpacing: "-0.01em" }}
          >
            Personas
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            Manage agent personalities, prompts, and memories
          </div>
        </div>
      </div>

      {showForm && (
        <PersonaForm
          editing={editing}
          providers={providers}
          modelsByProvider={modelsByProvider}
          onSaved={handleSaved}
          onCancel={() => {
            setShowForm(false);
            setEditing(null);
          }}
        />
      )}

      {loading ? (
        <div
          style={{
            padding: "48px 0",
            textAlign: "center",
            fontSize: 13,
            color: "var(--text-tertiary)",
          }}
        >
          Loading…
        </div>
      ) : personas.length === 0 && !showForm ? (
        /* Empty state */
        <div
          style={{
            padding: "60px 20px",
            textAlign: "center",
            color: "var(--text-tertiary)",
          }}
        >
          <div style={{ fontSize: 40, marginBottom: 14, opacity: 0.2 }}>🤖</div>
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginBottom: 6,
            }}
          >
            No personas yet
          </div>
          <div style={{ fontSize: 13, marginBottom: 18 }}>
            Create a persona to define your AI's name, personality, and default
            model.
          </div>
          <Btn variant="primary" onClick={openAdd}>
            <Plus size={14} />
            New Persona
          </Btn>
        </div>
      ) : (
        <>
          {personas.length > 0 && (
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                marginBottom: 10,
              }}
            >
              Your Personas
            </div>
          )}

          {/* persona-grid: auto-fill minmax(220px,1fr) */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
              gap: 14,
              marginBottom: 24,
            }}
          >
            {personas.map((p) => (
              <PersonaCard
                key={p.id}
                persona={p}
                providers={providers}
                modelsByProvider={modelsByProvider}
                onEdit={() => openEdit(p)}
                onDelete={() => handleDelete(p.id)}
              />
            ))}

            {/* Dashed "add" card — hidden when form is open */}
            {!showForm && <AddPersonaCard onClick={openAdd} />}
          </div>
        </>
      )}
    </div>
  );
}

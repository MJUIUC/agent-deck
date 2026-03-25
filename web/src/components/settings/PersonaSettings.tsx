import React, { useState, useEffect, useCallback } from "react";
import { Plus, Pencil, Trash2 } from "lucide-react";
import type { AgentPersona, Provider, Model, MemoryEntry } from "@/types";
import {
  personasApi,
  providersApi,
  modelsApi,
  memoriesApi,
} from "@/api/client";
import { Btn } from "./shared";
import { PersonaForm } from "./PersonaForm";

// ─── PersonaMemoryViewer ──────────────────────────────────────────────────────

interface PersonaMemoryViewerProps {
  persona: AgentPersona;
  onBack: () => void;
}

function PersonaMemoryViewer({ persona, onBack }: PersonaMemoryViewerProps) {
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const PAGE_SIZE = 20;

  const load = useCallback(
    async (pageNum = 0) => {
      setLoading(true);
      try {
        const res = await memoriesApi.list(persona.id, {
          limit: PAGE_SIZE,
          offset: pageNum * PAGE_SIZE,
        });
        setMemories(res.data.memories);
        setTotal(res.data.total_count);
        setPage(pageNum);
      } catch {
        // ignore
      } finally {
        setLoading(false);
      }
    },
    [persona.id],
  );

  useEffect(() => {
    load(0);
  }, [load]);

  const handleDelete = async (memoryId: string) => {
    setDeletingId(memoryId);
    try {
      await memoriesApi.delete(persona.id, memoryId);
      await load(page);
    } catch {
      // ignore
    } finally {
      setDeletingId(null);
    }
  };

  const MAX_MEMORIES = 500;
  const isNearLimit = total > 400;

  return (
    <div>
      {/* Back button + header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          marginBottom: 20,
        }}
      >
        <button
          onClick={onBack}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            color: "var(--text-secondary)",
            fontSize: 13,
            fontFamily: "inherit",
            padding: "4px 8px 4px 0",
            display: "flex",
            alignItems: "center",
            gap: 4,
          }}
        >
          ← Back
        </button>
        <div
          style={{
            fontSize: 16,
            fontWeight: 700,
            color: "var(--text-primary)",
          }}
        >
          {persona.emoji} {persona.name} — Memories
        </div>
      </div>

      {/* Count + warning */}
      <div
        style={{
          marginBottom: 16,
          fontSize: 12,
          color: isNearLimit
            ? "var(--warning, #f59e0b)"
            : "var(--text-tertiary)",
        }}
      >
        {total} / {MAX_MEMORIES} memories
        {isNearLimit && " · Approaching limit"}
      </div>

      {loading ? (
        <div
          style={{
            padding: "32px 0",
            textAlign: "center",
            fontSize: 13,
            color: "var(--text-tertiary)",
          }}
        >
          Loading…
        </div>
      ) : memories.length === 0 ? (
        <div
          style={{
            padding: "40px 20px",
            textAlign: "center",
            color: "var(--text-tertiary)",
            fontSize: 13,
          }}
        >
          No memories stored for this persona yet. Start a conversation and the
          agent will save things it learns about you.
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {memories.map((m) => (
            <div
              key={m.id}
              style={{
                background: "var(--bg-secondary)",
                borderRadius: 10,
                padding: "12px 14px",
                display: "flex",
                gap: 10,
                alignItems: "flex-start",
              }}
            >
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontSize: 13,
                    color: "var(--text-primary)",
                    lineHeight: 1.5,
                    marginBottom: 4,
                  }}
                >
                  {m.content}
                </div>
                <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
                  {new Date(m.created_at).toLocaleDateString()}
                  {m.thread_title && ` · from "${m.thread_title}"`}
                </div>
              </div>
              <button
                onClick={() => handleDelete(m.id)}
                disabled={deletingId === m.id}
                title="Delete memory"
                style={{
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  color: "var(--text-tertiary)",
                  fontSize: 14,
                  padding: 4,
                  borderRadius: 4,
                  flexShrink: 0,
                  opacity: deletingId === m.id ? 0.5 : 1,
                }}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Pagination */}
      {total > PAGE_SIZE && (
        <div
          style={{
            display: "flex",
            gap: 8,
            marginTop: 16,
            justifyContent: "center",
          }}
        >
          <button
            onClick={() => load(page - 1)}
            disabled={page === 0}
            style={{
              padding: "6px 14px",
              borderRadius: 6,
              border: "1px solid var(--border-default)",
              background: "transparent",
              cursor: "pointer",
              color: "var(--text-secondary)",
              fontSize: 12,
              fontFamily: "inherit",
            }}
          >
            ← Prev
          </button>
          <span
            style={{
              alignSelf: "center",
              fontSize: 12,
              color: "var(--text-tertiary)",
            }}
          >
            Page {page + 1} of {Math.ceil(total / PAGE_SIZE)}
          </span>
          <button
            onClick={() => load(page + 1)}
            disabled={(page + 1) * PAGE_SIZE >= total}
            style={{
              padding: "6px 14px",
              borderRadius: 6,
              border: "1px solid var(--border-default)",
              background: "transparent",
              cursor: "pointer",
              color: "var(--text-secondary)",
              fontSize: 12,
              fontFamily: "inherit",
            }}
          >
            Next →
          </button>
        </div>
      )}
    </div>
  );
}

// ─── PersonaCard ──────────────────────────────────────────────────────────────

interface PersonaCardProps {
  persona: AgentPersona;
  providers: Provider[];
  modelsByProvider: Record<string, Model[]>;
  onEdit: () => void;
  onDelete: () => void;
  onViewMemories: () => void;
}

function PersonaCard({
  persona,
  providers,
  modelsByProvider,
  onEdit,
  onDelete,
  onViewMemories,
}: PersonaCardProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [memCount, setMemCount] = useState(0);

  useEffect(() => {
    if (persona.is_default) return;
    memoriesApi
      .list(persona.id, { limit: 1 })
      .then((r) => setMemCount(r.data.total_count))
      .catch(() => {});
  }, [persona.id, persona.is_default]);

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
              flexWrap: "wrap",
            }}
          >
            {persona.name}
            {!persona.is_default && memCount > 0 && (
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 500,
                  color: "var(--text-tertiary)",
                  background: "var(--bg-elevated)",
                  borderRadius: 10,
                  padding: "1px 7px",
                }}
              >
                {memCount} memories
              </span>
            )}
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
      <div style={{ display: "flex", gap: 6, marginTop: 12, flexWrap: "wrap" }}>
        {!persona.is_default && (
          <Btn sm variant="ghost" onClick={onViewMemories}>
            🗂 Memories
          </Btn>
        )}
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
 * Personas tab content — list, add, edit, delete, and view memories.
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
  const [viewingMemoriesFor, setViewingMemoriesFor] =
    useState<AgentPersona | null>(null);

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

  if (viewingMemoriesFor) {
    return (
      <PersonaMemoryViewer
        persona={viewingMemoriesFor}
        onBack={() => setViewingMemoriesFor(null)}
      />
    );
  }

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
                onViewMemories={() => setViewingMemoriesFor(p)}
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

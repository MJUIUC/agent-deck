import React, { useState, useEffect, useCallback } from "react";
import { Add, Renew, Edit, TrashCan } from "@carbon/icons-react";
import type { Provider, Model } from "@/types";
import { providersApi, modelsApi } from "@/api/client";
import { Btn, KindIcon, StatusBadge } from "./shared";
import { ProviderForm } from "./ProviderForm";

// ─── ProviderCard ─────────────────────────────────────────────────────────────

interface ProviderCardProps {
  provider: Provider;
  models: Model[];
  onEdit: () => void;
  onDelete: () => void;
  onSyncModels: () => void;
  syncingModels: boolean;
  onToggleModel: (
    modelId: string,
    patch: { enabled?: boolean; vision?: boolean },
  ) => void;
  togglingModelId: string | null;
}

function ProviderCard({
  provider,
  models,
  onEdit,
  onDelete,
  onSyncModels,
  syncingModels,
  onToggleModel,
  togglingModelId,
}: ProviderCardProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const enabledCount = models.filter((m) => m.enabled).length;

  return (
    <div
      style={{
        background: "var(--bg-secondary)",
        border: `1px solid ${hovered ? "var(--border-default)" : "var(--border-subtle)"}`,
        borderRadius: 11,
        padding: "16px 18px",
        transition: "border-color 0.15s",
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* provider-card-header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          marginBottom: 12,
        }}
      >
        {/* provider-icon: 40×40, 9px radius */}
        <div
          style={{
            width: 40,
            height: 40,
            borderRadius: 9,
            background: "var(--bg-elevated)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <KindIcon kind={provider.kind} size={20} />
        </div>

        {/* provider-meta */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-primary)",
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            {provider.name}
            <StatusBadge status={provider.enabled ? "connected" : "pending"} />
          </div>
          <div
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              marginTop: 1,
            }}
          >
            {provider.kind}
          </div>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              fontFamily: '"SF Mono","Fira Code",monospace',
              marginTop: 2,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {provider.base_url}
          </div>
        </div>
      </div>

      {/* Model list */}
      {models.length > 0 && (
        <div style={{ marginBottom: 10 }}>
          {!expanded ? (
            /* Compact chip row — up to 6 read-only chips */
            <div style={{ display: "flex", flexWrap: "wrap", gap: "4px 6px" }}>
              {models.slice(0, 6).map((m) => (
                <span
                  key={m.id}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    background: m.enabled
                      ? "rgba(74,82,53,0.35)"
                      : "var(--bg-elevated)",
                    border: `1px solid ${m.enabled ? "var(--accent-muted)" : "var(--border-subtle)"}`,
                    borderRadius: 5,
                    padding: "2px 8px",
                    fontSize: 11,
                    color: m.enabled
                      ? "var(--text-secondary)"
                      : "var(--text-tertiary)",
                    fontFamily: '"SF Mono","Fira Code",monospace',
                    opacity: m.enabled ? 1 : 0.6,
                  }}
                >
                  {m.vision && (
                    <span
                      title="Vision capable"
                      style={{ fontSize: 10, opacity: 0.7 }}
                    >
                      👁
                    </span>
                  )}
                  {m.display_name}
                </span>
              ))}
            </div>
          ) : (
            /* Expanded model rows with toggles */
            <div
              style={{
                background: "var(--bg-tertiary)",
                border: "1px solid var(--border-subtle)",
                borderRadius: 7,
                overflow: "hidden",
              }}
            >
              {models.map((m, idx) => (
                <div
                  key={m.id}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "7px 10px",
                    borderBottom:
                      idx < models.length - 1
                        ? "1px solid var(--border-subtle)"
                        : "none",
                    opacity: togglingModelId === m.id ? 0.5 : 1,
                    transition: "opacity 0.15s",
                  }}
                >
                  <span
                    style={{
                      flex: 1,
                      fontSize: 11,
                      fontFamily: '"SF Mono","Fira Code",monospace',
                      color: m.enabled
                        ? "var(--text-secondary)"
                        : "var(--text-tertiary)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {m.display_name}
                  </span>
                  {/* Vision toggle */}
                  <button
                    type="button"
                    onClick={() => onToggleModel(m.id, { vision: !m.vision })}
                    disabled={togglingModelId === m.id}
                    title={
                      m.vision
                        ? "Vision enabled — click to disable"
                        : "Vision disabled — click to enable"
                    }
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 4,
                      padding: "2px 7px",
                      borderRadius: 4,
                      border: `1px solid ${m.vision ? "var(--accent-muted)" : "var(--border-subtle)"}`,
                      background: m.vision
                        ? "rgba(74,82,53,0.35)"
                        : "transparent",
                      color: m.vision
                        ? "var(--text-secondary)"
                        : "var(--text-tertiary)",
                      fontSize: 10,
                      fontWeight: 600,
                      cursor: "pointer",
                      fontFamily: "inherit",
                      transition: "all 0.15s",
                    }}
                  >
                    👁 Vision
                  </button>
                  {/* Enabled toggle */}
                  <button
                    type="button"
                    onClick={() => onToggleModel(m.id, { enabled: !m.enabled })}
                    disabled={togglingModelId === m.id}
                    title={
                      m.enabled
                        ? "Enabled — click to disable"
                        : "Disabled — click to enable"
                    }
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 4,
                      padding: "2px 7px",
                      borderRadius: 4,
                      border: `1px solid ${m.enabled ? "var(--accent-muted)" : "var(--border-subtle)"}`,
                      background: m.enabled
                        ? "rgba(74,82,53,0.35)"
                        : "transparent",
                      color: m.enabled
                        ? "var(--text-secondary)"
                        : "var(--text-tertiary)",
                      fontSize: 10,
                      fontWeight: 600,
                      cursor: "pointer",
                      fontFamily: "inherit",
                      transition: "all 0.15s",
                    }}
                  >
                    {m.enabled ? "✓ Enabled" : "Disabled"}
                  </button>
                </div>
              ))}
            </div>
          )}
          {models.length > 6 && (
            <button
              onClick={() => setExpanded((v) => !v)}
              style={{
                marginTop: 6,
                background: "none",
                border: "none",
                padding: 0,
                fontSize: 11,
                color: "var(--accent-secondary)",
                cursor: "pointer",
                fontFamily: "inherit",
              }}
            >
              {expanded ? "▲ Show less" : `▼ +${models.length - 6} more`}
            </button>
          )}
        </div>
      )}

      {/* provider-actions */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          paddingTop: 10,
          borderTop: "1px solid var(--border-subtle)",
        }}
      >
        <span style={{ flex: 1, fontSize: 12, color: "var(--text-tertiary)" }}>
          {models.length === 0
            ? "No models synced"
            : `${enabledCount} / ${models.length} model${models.length !== 1 ? "s" : ""} enabled`}
        </span>
        <Btn sm variant="ghost" onClick={onSyncModels} disabled={syncingModels}>
          <Renew
            size={12}
            style={
              syncingModels
                ? { animation: "spin 0.8s linear infinite" }
                : undefined
            }
          />
          {syncingModels ? "Syncing…" : "Sync Models"}
        </Btn>
        <Btn sm variant="ghost" onClick={onEdit}>
          <Edit size={12} />
          Edit
        </Btn>
        {confirmDelete ? (
          <>
            <span style={{ fontSize: 11, color: "var(--error)" }}>Sure?</span>
            <Btn sm variant="danger" onClick={onDelete}>
              Yes, delete
            </Btn>
            <Btn sm variant="ghost" onClick={() => setConfirmDelete(false)}>
              Cancel
            </Btn>
          </>
        ) : (
          <Btn sm variant="danger" onClick={() => setConfirmDelete(true)}>
            <TrashCan size={12} />
            Delete
          </Btn>
        )}
      </div>
    </div>
  );
}

// ─── ProviderSettings ─────────────────────────────────────────────────────────

export interface ProviderSettingsProps {
  onDataChanged: () => void;
}

/**
 * Providers tab content — list, add, edit, delete, sync models, Copilot auth.
 * Manages its own data loading and CRUD state.
 */
export function ProviderSettings({ onDataChanged }: ProviderSettingsProps) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<
    Record<string, Model[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<Provider | null>(null);
  const [syncingId, setSyncingId] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [togglingModelId, setTogglingModelId] = useState<string | null>(null);

  const loadProviders = useCallback(async () => {
    setLoading(true);
    try {
      const res = await providersApi.list();
      setProviders(res.data);
      const entries = await Promise.all(
        res.data.map(async (p) => {
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
    loadProviders();
  }, [loadProviders]);

  const handleSaved = useCallback(async () => {
    setShowForm(false);
    setEditing(null);
    await loadProviders();
    onDataChanged();
  }, [loadProviders, onDataChanged]);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await providersApi.delete(id);
        await loadProviders();
        onDataChanged();
      } catch {
        /* ignore */
      }
    },
    [loadProviders, onDataChanged],
  );

  const handleSyncModels = useCallback(async (providerId: string) => {
    setSyncingId(providerId);
    setSyncError(null);
    try {
      const res = await modelsApi.sync(providerId);
      // Guard against unexpected response shapes
      const models: Model[] = Array.isArray(res.data)
        ? res.data
        : Array.isArray((res.data as { models?: Model[] }).models)
          ? (res.data as { models: Model[] }).models
          : [];
      setModelsByProvider((prev) => ({ ...prev, [providerId]: models }));
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Sync failed");
    } finally {
      setSyncingId(null);
    }
  }, []);

  const handleToggleModel = useCallback(
    async (
      providerId: string,
      modelId: string,
      patch: { enabled?: boolean; vision?: boolean },
    ) => {
      setTogglingModelId(modelId);
      try {
        const res = await modelsApi.update(providerId, modelId, patch);
        setModelsByProvider((prev) => ({
          ...prev,
          [providerId]: (prev[providerId] ?? []).map((m) =>
            m.id === modelId ? res.data : m,
          ),
        }));
      } catch {
        /* ignore */
      } finally {
        setTogglingModelId(null);
      }
    },
    [],
  );

  const openAdd = useCallback(() => {
    setEditing(null);
    setShowForm(true);
    setSyncError(null);
  }, []);

  const openEdit = useCallback((p: Provider) => {
    setEditing(p);
    setShowForm(true);
  }, []);

  const cancelForm = useCallback(() => {
    setShowForm(false);
    setEditing(null);
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
            Providers
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            Configure AI model providers and manage API keys
          </div>
        </div>
      </div>

      {showForm && (
        <ProviderForm
          editing={editing}
          onSaved={handleSaved}
          onCancel={cancelForm}
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
      ) : providers.length === 0 && !showForm ? (
        /* Empty state */
        <div
          style={{
            padding: "60px 20px",
            textAlign: "center",
            color: "var(--text-tertiary)",
          }}
        >
          <div style={{ fontSize: 40, marginBottom: 14, opacity: 0.2 }}>🔌</div>
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginBottom: 6,
            }}
          >
            No providers yet
          </div>
          <div style={{ fontSize: 13, marginBottom: 18 }}>
            Add an AI provider to start using agent-deck.
          </div>
          <Btn variant="primary" onClick={openAdd}>
            <Add size={14} />
            Add Provider
          </Btn>
        </div>
      ) : (
        <>
          {providers.length > 0 && (
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
              Connected Providers
            </div>
          )}

          {/* provider-list */}
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 12,
              marginBottom: 24,
            }}
          >
            {syncError && (
              <div
                style={{
                  background: "rgba(220,50,50,0.1)",
                  border: "1px solid rgba(220,50,50,0.3)",
                  borderRadius: 8,
                  padding: "8px 12px",
                  fontSize: 12,
                  color: "var(--error, #e05c5c)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 8,
                }}
              >
                <span>⚠ Sync failed: {syncError}</span>
                <button
                  onClick={() => setSyncError(null)}
                  style={{
                    background: "none",
                    border: "none",
                    color: "inherit",
                    cursor: "pointer",
                    fontSize: 14,
                    lineHeight: 1,
                    padding: "0 2px",
                    opacity: 0.7,
                    fontFamily: "inherit",
                  }}
                >
                  ×
                </button>
              </div>
            )}

            {providers.map((p) => (
              <ProviderCard
                key={p.id}
                provider={p}
                models={modelsByProvider[p.id] ?? []}
                onEdit={() => openEdit(p)}
                onDelete={() => handleDelete(p.id)}
                onSyncModels={() => handleSyncModels(p.id)}
                syncingModels={syncingId === p.id}
                onToggleModel={(modelId, patch) =>
                  handleToggleModel(p.id, modelId, patch)
                }
                togglingModelId={togglingModelId}
              />
            ))}

            {!showForm && (
              <Btn variant="ghost" onClick={openAdd}>
                <Add size={13} />
                Add Provider
              </Btn>
            )}
          </div>
        </>
      )}
    </div>
  );
}

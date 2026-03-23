import { useState, useEffect, useRef, useCallback } from "react";
import type {
  Thread,
  McpServer,
  McpTool,
  Provider,
  Model,
  Routine,
} from "@/types";
import {
  threadsApi,
  mcpServersApi,
  providersApi,
  modelsApi,
  routinesApi,
} from "@/api/client";
import { X, ChevronRight, Settings } from "lucide-react";
import styles from "./ConfigPane.module.css";
import { CronPicker } from "./CronPicker";

// ─── Sub-types ────────────────────────────────────────────────────────────────

interface ThreadMcpEntry {
  id: string;
  thread_id: string;
  mcp_server_id: string;
  enabled: boolean;
}

interface ProviderWithModels {
  provider: Provider;
  models: Model[];
}

// ─── Toggle switch ────────────────────────────────────────────────────────────

function Toggle({
  checked,
  onChange,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      onClick={() => !disabled && onChange(!checked)}
      disabled={disabled}
      className={[styles.toggleTrack, checked ? styles.toggleOn : ""].join(" ")}
    >
      <span
        className={[
          styles.toggleThumb,
          checked ? styles.toggleThumbOn : "",
        ].join(" ")}
      />
    </button>
  );
}

// ─── Status / type badges ─────────────────────────────────────────────────────

function StatusBadge({ status }: { status: McpServer["status"] }) {
  const map: Record<McpServer["status"], { cls: string; label: string }> = {
    connected: { cls: styles.badgeConnected, label: "connected" },
    connecting: { cls: styles.badgeConnecting, label: "connecting" },
    error: { cls: styles.badgeError, label: "error" },
    inactive: { cls: styles.badgeInactive, label: "inactive" },
  };
  const { cls, label } = map[status] ?? map.inactive;
  return <span className={[styles.badge, cls].join(" ")}>{label}</span>;
}

function TypeBadge({ type }: { type: McpServer["server_type"] }) {
  return (
    <span className={[styles.badge, styles.badgeType].join(" ")}>{type}</span>
  );
}

// ─── MCP server card ──────────────────────────────────────────────────────────

function McpServerCard({
  server,
  tools,
  onRemove,
  onLoadTools,
}: {
  server: McpServer;
  tools: McpTool[] | null;
  onRemove: () => void;
  onLoadTools: () => void;
}) {
  const [toolsOpen, setToolsOpen] = useState(false);
  const [toolsLoading, setToolsLoading] = useState(false);

  const handleToggleTools = async () => {
    if (!toolsOpen && tools === null) {
      setToolsLoading(true);
      await onLoadTools();
      setToolsLoading(false);
    }
    setToolsOpen((o) => !o);
  };

  const url = server.source_url ?? null;
  const displayUrl = url
    ? url.replace(/^https?:\/\//, "").replace(/\/$/, "")
    : null;

  return (
    <div className={styles.mcpCard}>
      <div className={styles.mcpCardTop}>
        <div className={styles.mcpCardMeta}>
          <span className={styles.mcpName}>{server.name}</span>
          <div className={styles.mcpBadges}>
            <StatusBadge status={server.status} />
            <TypeBadge type={server.server_type} />
          </div>
        </div>
        <button
          className={styles.mcpRemoveBtn}
          title="Detach server from thread"
          onClick={onRemove}
        >
          <X size={12} />
        </button>
      </div>

      {server.description && (
        <div className={styles.mcpDesc}>{server.description}</div>
      )}

      {displayUrl && (
        <div className={styles.mcpUrl}>
          <span className={styles.mcpUrlText}>{displayUrl}</span>
          {url && (
            <a
              href={url}
              target="_blank"
              rel="noopener noreferrer"
              className={styles.mcpUrlLink}
              title="Open source URL"
            >
              ↗
            </a>
          )}
        </div>
      )}

      <div className={styles.mcpToolsDisclosure}>
        <button className={styles.mcpToolsToggle} onClick={handleToggleTools}>
          <ChevronRight
            size={10}
            className={[
              styles.mcpToolsArrow,
              toolsOpen ? styles.mcpToolsArrowOpen : "",
            ].join(" ")}
          />
          <span>Tools</span>
          {tools !== null && (
            <span className={styles.mcpToolsCount}>({tools.length})</span>
          )}
          {toolsLoading && <span className={styles.mcpToolsCount}>(…)</span>}
        </button>

        {toolsOpen && tools !== null && (
          <div className={styles.mcpToolsList}>
            {tools.length === 0 ? (
              <span className={styles.mcpToolsEmpty}>No tools reported</span>
            ) : (
              tools.map((t) => (
                <div key={t.name} className={styles.mcpToolItem}>
                  <span className={styles.mcpToolName}>{t.name}</span>
                  <span className={styles.mcpToolDesc}>{t.description}</span>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Provider + Model selector ────────────────────────────────────────────────

function ProviderModelSelector({
  thread,
  onUpdate,
}: {
  thread: Thread;
  // Callers receive provider UUID and model record UUID — server-ready values.
  onUpdate: (providerRecordId: string, modelRecordId: string) => void;
}) {
  // All IDs stored on the thread (active_provider, active_model) and on the
  // persona (default_provider, default_model) are record UUIDs.
  // We resolve them to display strings only for rendering.
  const [data, setData] = useState<ProviderWithModels[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    null,
  );
  const [listOpen, setListOpen] = useState(false);

  // Resolved display strings for the UI label — never written to the server.
  const [effectiveProviderName, setEffectiveProviderName] = useState<
    string | null
  >(null);
  const [effectiveModelLabel, setEffectiveModelLabel] = useState<string | null>(
    null,
  );
  // The currently active record IDs (UUIDs) — used for isActive comparisons.
  const [activeProviderId, setActiveProviderId] = useState<string | null>(
    thread.active_provider ?? null,
  );
  const [activeModelId, setActiveModelId] = useState<string | null>(
    thread.active_model ?? null,
  );

  // Load all providers and their models on mount
  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const { data: providers } = await providersApi.list();
        const enabled = providers.filter((p) => p.enabled);
        const results = await Promise.all(
          enabled.map(async (p) => {
            try {
              const { data: models } = await modelsApi.list(p.id);
              return { provider: p, models: models.filter((m) => m.enabled) };
            } catch {
              return { provider: p, models: [] };
            }
          }),
        );
        if (!cancelled) {
          setData(results);

          // Resolve the active provider/model UUIDs to display strings.
          // Source priority: thread's own active_* fields, then persona defaults.
          const resolvedProviderId =
            thread.active_provider ?? thread.persona?.default_provider ?? null;
          const resolvedModelId =
            thread.active_model ?? thread.persona?.default_model ?? null;

          // Find the provider entry by UUID
          const providerEntry =
            results.find((r) => r.provider.id === resolvedProviderId) ?? null;

          // Find the model entry by UUID across all providers
          let modelEntry: Model | null = null;
          for (const { models } of results) {
            const found = models.find((m) => m.id === resolvedModelId);
            if (found) {
              modelEntry = found;
              break;
            }
          }

          setActiveProviderId(resolvedProviderId);
          setActiveModelId(resolvedModelId);
          setEffectiveProviderName(providerEntry?.provider.name ?? null);
          setEffectiveModelLabel(
            modelEntry?.display_name || modelEntry?.model_id || null,
          );

          // Pre-select the resolved provider's pill
          setSelectedProviderId(
            providerEntry?.provider.id ?? results[0]?.provider.id ?? null,
          );
          setListOpen(false);
        }
      } catch {
        // silently degrade
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thread.id]);

  const selectedEntry =
    data.find((d) => d.provider.id === selectedProviderId) ?? null;

  if (loading) {
    return <div className={styles.selectorLoading}>Loading models…</div>;
  }

  if (data.length === 0) {
    return (
      <div className={styles.selectorEmpty}>
        No providers configured. Add one in Settings → Providers.
      </div>
    );
  }

  // Label shown on the collapsed toggle — resolved display strings
  const collapsedLabel =
    effectiveProviderName && effectiveModelLabel
      ? `${effectiveProviderName} · ${effectiveModelLabel}`
      : "Select a model…";

  return (
    <div className={styles.selectorWrap}>
      {/* Toggle header — always visible */}
      <button
        className={styles.selectorToggle}
        onClick={() => setListOpen((o) => !o)}
      >
        <ChevronRight
          size={11}
          className={[
            styles.selectorArrow,
            listOpen ? styles.selectorArrowOpen : "",
          ].join(" ")}
        />
        <span className={styles.selectorLabel}>{collapsedLabel}</span>
      </button>

      {/* Expanded panel */}
      {listOpen && (
        <div className={styles.selectorPanel}>
          {/* Provider pills */}
          <div className={styles.providerPills}>
            {data.map(({ provider }) => (
              <button
                key={provider.id}
                className={[
                  styles.providerPill,
                  provider.id === selectedProviderId
                    ? styles.providerPillActive
                    : "",
                ].join(" ")}
                onClick={() => setSelectedProviderId(provider.id)}
              >
                {provider.name}
              </button>
            ))}
          </div>

          {/* Model list for selected provider */}
          {selectedEntry && (
            <div className={styles.modelList}>
              {selectedEntry.models.length === 0 ? (
                <div className={styles.modelEmpty}>
                  No models. Sync in Settings → Providers.
                </div>
              ) : (
                selectedEntry.models.map((m) => {
                  const isActive =
                    m.id === activeModelId &&
                    selectedEntry.provider.id === activeProviderId;
                  return (
                    <button
                      key={m.id}
                      className={[
                        styles.modelItem,
                        isActive ? styles.modelItemActive : "",
                      ].join(" ")}
                      onClick={() => {
                        // Update local display state immediately
                        setActiveProviderId(selectedEntry.provider.id);
                        setActiveModelId(m.id);
                        setEffectiveProviderName(selectedEntry.provider.name);
                        setEffectiveModelLabel(m.display_name || m.model_id);
                        // Persist UUIDs to server — store + ChatHeader update reactively
                        onUpdate(selectedEntry.provider.id, m.id);
                        setListOpen(false);
                      }}
                    >
                      {m.display_name || m.model_id}
                      {isActive && (
                        <span className={styles.modelActiveCheck}>✓</span>
                      )}
                    </button>
                  );
                })
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── Attach server picker overlay ─────────────────────────────────────────────

function AttachServerPicker({
  attachedIds,
  onAttach,
  onClose,
}: {
  attachedIds: Set<string>;
  onAttach: (server: McpServer) => void;
  onClose: () => void;
}) {
  const [allServers, setAllServers] = useState<McpServer[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    mcpServersApi.list().then(({ data }) => {
      setAllServers(data);
      setLoading(false);
    });
  }, []);

  // Click-outside to close
  useEffect(() => {
    function handler(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  const available = allServers.filter(
    (s) =>
      !attachedIds.has(s.id) &&
      (query === "" ||
        s.name.toLowerCase().includes(query.toLowerCase()) ||
        (s.description ?? "").toLowerCase().includes(query.toLowerCase())),
  );

  return (
    <div className={styles.pickerOverlay} ref={ref}>
      <div className={styles.pickerHeader}>
        <span className={styles.pickerTitle}>Attach MCP Server</span>
        <button className={styles.pickerClose} onClick={onClose}>
          <X size={12} />
        </button>
      </div>
      <input
        className={styles.pickerSearch}
        type="text"
        placeholder="Search servers…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        autoFocus
      />
      <div className={styles.pickerList}>
        {loading ? (
          <div className={styles.pickerEmpty}>Loading…</div>
        ) : available.length === 0 ? (
          <div className={styles.pickerEmpty}>
            {allServers.length === 0
              ? "No MCP servers configured. Add one in Settings → MCP Servers."
              : "All configured servers are already attached."}
          </div>
        ) : (
          available.map((s) => (
            <button
              key={s.id}
              className={styles.pickerItem}
              onClick={() => onAttach(s)}
            >
              <div className={styles.pickerItemName}>{s.name}</div>
              {s.description && (
                <div className={styles.pickerItemDesc}>{s.description}</div>
              )}
              <div className={styles.pickerItemBadges}>
                <StatusBadge status={s.status} />
                <TypeBadge type={s.server_type} />
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
}

// ─── Main ConfigPane ──────────────────────────────────────────────────────────

interface ConfigPaneProps {
  isOpen: boolean;
  thread: Thread;
  onClose: () => void;
  onThreadUpdated: (thread: Thread) => void;
  onOpenSettings?: () => void;
  onArchiveThread?: (threadId: string) => Promise<void>;
}

export function ConfigPane({
  isOpen,
  thread,
  onClose,
  onThreadUpdated,
  onOpenSettings,
  onArchiveThread,
}: ConfigPaneProps) {
  // ── Addendum ──
  const [addendum, setAddendum] = useState(thread.system_prompt_addendum ?? "");
  const [isSavingAddendum, setIsSavingAddendum] = useState(false);

  // ── Archive confirm ──
  const [showArchiveConfirm, setShowArchiveConfirm] = useState(false);
  const [isArchiving, setIsArchiving] = useState(false);

  // ── Tool activity ──
  const [showToolActivity, setShowToolActivity] = useState(
    thread.show_tool_activity ?? false,
  );
  const [isSavingToolActivity, setIsSavingToolActivity] = useState(false);

  // ── System events ──
  const [showSystemEvents, setShowSystemEvents] = useState(
    thread.show_system_events ?? false,
  );
  const [isSavingSystemEvents, setIsSavingSystemEvents] = useState(false);

  // ── MCP servers ──
  const [attachedEntries, setAttachedEntries] = useState<ThreadMcpEntry[]>([]);
  const [mcpServersMap, setMcpServersMap] = useState<Record<string, McpServer>>(
    {},
  );
  const [toolsMap, setToolsMap] = useState<Record<string, McpTool[] | null>>(
    {},
  );
  const [showAttachPicker, setShowAttachPicker] = useState(false);
  const [mcpLoading, setMcpLoading] = useState(false);

  // ── Routines ──
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routinesLoading, setRoutinesLoading] = useState(false);
  const [showRoutineForm, setShowRoutineForm] = useState(false);
  const [editingRoutine, setEditingRoutine] = useState<Routine | null>(null);
  // Routine form state
  const [routineFormName, setRoutineFormName] = useState("");
  const [routineFormPrompt, setRoutineFormPrompt] = useState("");
  const [routineFormCron, setRoutineFormCron] = useState("0 9 * * *");
  const [routineFormSaving, setRoutineFormSaving] = useState(false);
  const [routineFormError, setRoutineFormError] = useState<string | null>(null);
  const [deletingRoutineId, setDeletingRoutineId] = useState<string | null>(
    null,
  );

  // Sync local state when thread prop changes (different thread selected)
  useEffect(() => {
    setAddendum(thread.system_prompt_addendum ?? "");
    setShowToolActivity(thread.show_tool_activity ?? false);
    setShowSystemEvents(thread.show_system_events ?? false);
    setShowAttachPicker(false);
    setShowArchiveConfirm(false);
    // Reset routines form state on thread switch
    setRoutines([]);
    setShowRoutineForm(false);
    setEditingRoutine(null);
    setRoutineFormError(null);
  }, [
    thread.id,
    thread.system_prompt_addendum,
    thread.show_tool_activity,
    thread.show_system_events,
  ]);

  // Load attached MCP servers whenever the pane opens or thread changes
  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;

    async function loadMcp() {
      setMcpLoading(true);
      try {
        const [{ data: entries }, { data: allServers }] = await Promise.all([
          threadsApi.listMcpServers(thread.id),
          mcpServersApi.list(),
        ]);
        if (cancelled) return;

        const serverById: Record<string, McpServer> = {};
        for (const s of allServers) serverById[s.id] = s;

        setAttachedEntries(entries);
        setMcpServersMap(serverById);
        // Reset tools cache on thread change
        setToolsMap({});
      } catch {
        // silently degrade
      } finally {
        if (!cancelled) setMcpLoading(false);
      }
    }

    loadMcp();
    return () => {
      cancelled = true;
    };
  }, [isOpen, thread.id]);

  // Load routines whenever the pane opens or thread changes
  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;

    async function loadRoutines() {
      setRoutinesLoading(true);
      try {
        const res = await routinesApi.list(thread.id);
        if (!cancelled) setRoutines(res.data);
      } catch {
        /* non-critical */
      } finally {
        if (!cancelled) setRoutinesLoading(false);
      }
    }

    loadRoutines();
    return () => {
      cancelled = true;
    };
  }, [isOpen, thread.id]);

  // ── Handlers ──

  const handleAddendumBlur = async () => {
    const current = addendum.trim();
    const existing = (thread.system_prompt_addendum ?? "").trim();
    if (current === existing) return;
    setIsSavingAddendum(true);
    try {
      const res = await threadsApi.update(thread.id, {
        system_prompt_addendum: current || undefined,
      });
      onThreadUpdated(res.data);
      // Notify connected clients about the addendum change
      threadsApi.notify(thread.id, "addendum_updated").catch(() => {
        /* silently degrade */
      });
    } catch {
      setAddendum(thread.system_prompt_addendum ?? "");
    } finally {
      setIsSavingAddendum(false);
    }
  };

  const handleToolActivityChange = async (value: boolean) => {
    setShowToolActivity(value);
    setIsSavingToolActivity(true);
    try {
      const res = await threadsApi.update(thread.id, {
        show_tool_activity: value,
      });
      onThreadUpdated(res.data);
    } catch {
      setShowToolActivity(!value);
    } finally {
      setIsSavingToolActivity(false);
    }
  };

  const handleSystemEventsChange = async (value: boolean) => {
    setShowSystemEvents(value);
    setIsSavingSystemEvents(true);
    try {
      const res = await threadsApi.update(thread.id, {
        show_system_events: value,
      });
      onThreadUpdated(res.data);
    } catch {
      setShowSystemEvents(!value);
    } finally {
      setIsSavingSystemEvents(false);
    }
  };

  const handleModelUpdate = useCallback(
    async (providerRecordId: string, modelRecordId: string) => {
      try {
        const res = await threadsApi.update(thread.id, {
          active_provider: providerRecordId,
          active_model: modelRecordId,
        });
        // Server returns the updated thread with UUID fields — upserts the store,
        // which reactively updates ChatHeader and any other thread consumers.
        onThreadUpdated(res.data);

        // Notify the thread about the model switch so system events are recorded
        // Look up the display names from the updated thread for the notify payload
        threadsApi
          .notify(thread.id, "model_switched", {
            provider_name: res.data.active_provider ?? "unknown",
            model_name: res.data.active_model ?? "unknown",
          })
          .catch(() => {
            /* silently degrade */
          });
      } catch {
        // silently degrade — UI local state already updated optimistically
      }
    },
    [thread.id, onThreadUpdated],
  );

  const handleDetachServer = async (mcpServerId: string) => {
    const server = mcpServersMap[mcpServerId];
    try {
      await threadsApi.detachMcpServer(thread.id, mcpServerId);
      setAttachedEntries((prev) =>
        prev.filter((e) => e.mcp_server_id !== mcpServerId),
      );
      // Notify thread about detach
      if (server) {
        threadsApi
          .notify(thread.id, "mcp_server_detached", { name: server.name })
          .catch(() => {
            /* silently degrade */
          });
      }
    } catch {
      // silently degrade
    }
  };

  const handleAttachServer = async (server: McpServer) => {
    setShowAttachPicker(false);
    try {
      const { data: entry } = await threadsApi.attachMcpServer(
        thread.id,
        server.id,
      );
      setAttachedEntries((prev) => [...prev, entry]);
      setMcpServersMap((prev) => ({ ...prev, [server.id]: server }));
      // Notify thread about attach
      threadsApi
        .notify(thread.id, "mcp_server_attached", { name: server.name })
        .catch(() => {
          /* silently degrade */
        });
    } catch {
      // silently degrade
    }
  };

  const handleLoadTools = async (serverId: string) => {
    try {
      const { data: tools } = await mcpServersApi.listTools(serverId);
      setToolsMap((prev) => ({ ...prev, [serverId]: tools }));
    } catch {
      setToolsMap((prev) => ({ ...prev, [serverId]: [] }));
    }
  };

  // ── Routine helpers ──

  function openAddForm() {
    setEditingRoutine(null);
    setRoutineFormName("");
    setRoutineFormPrompt("");
    setRoutineFormCron("0 9 * * *");
    setRoutineFormError(null);
    setShowRoutineForm(true);
  }

  function openEditForm(r: Routine) {
    setEditingRoutine(r);
    setRoutineFormName(r.name);
    setRoutineFormPrompt(r.prompt);
    setRoutineFormCron(r.cron_expr);
    setRoutineFormError(null);
    setShowRoutineForm(true);
  }

  function closeRoutineForm() {
    setShowRoutineForm(false);
    setEditingRoutine(null);
    setRoutineFormError(null);
  }

  async function handleRoutineSave() {
    if (!routineFormName.trim()) {
      setRoutineFormError("Name is required");
      return;
    }
    if (!routineFormPrompt.trim()) {
      setRoutineFormError("Prompt is required");
      return;
    }
    if (!routineFormCron.trim()) {
      setRoutineFormError("Schedule is required");
      return;
    }
    setRoutineFormSaving(true);
    setRoutineFormError(null);
    try {
      if (editingRoutine) {
        const res = await routinesApi.update(thread.id, editingRoutine.id, {
          name: routineFormName.trim(),
          prompt: routineFormPrompt.trim(),
          cron_expr: routineFormCron.trim(),
        });
        setRoutines((rs) =>
          rs.map((r) => (r.id === editingRoutine.id ? res.data : r)),
        );
      } else {
        const res = await routinesApi.create(thread.id, {
          name: routineFormName.trim(),
          prompt: routineFormPrompt.trim(),
          cron_expr: routineFormCron.trim(),
        });
        setRoutines((rs) => [...rs, res.data]);
      }
      closeRoutineForm();
    } catch (e) {
      setRoutineFormError(
        e instanceof Error ? e.message : "Failed to save routine",
      );
    } finally {
      setRoutineFormSaving(false);
    }
  }

  async function handleRoutineToggle(routineId: string) {
    try {
      const res = await routinesApi.toggle(thread.id, routineId);
      setRoutines((rs) =>
        rs.map((r) =>
          r.id === routineId ? { ...r, enabled: res.data.enabled } : r,
        ),
      );
    } catch {
      /* non-critical */
    }
  }

  async function handleRoutineDelete(routineId: string) {
    setDeletingRoutineId(routineId);
    try {
      await routinesApi.delete(thread.id, routineId);
      setRoutines((rs) => rs.filter((r) => r.id !== routineId));
    } catch {
      /* non-critical */
    } finally {
      setDeletingRoutineId(null);
    }
  }

  const handleArchiveClick = () => setShowArchiveConfirm(true);

  const handleArchiveConfirm = async () => {
    if (!onArchiveThread) return;
    setIsArchiving(true);
    try {
      await onArchiveThread(thread.id);
      // onArchiveThread updates the store and switches active thread;
      // the pane will unmount or receive a new thread prop automatically.
    } catch {
      setIsArchiving(false);
      setShowArchiveConfirm(false);
    }
  };

  const handleArchiveCancel = () => setShowArchiveConfirm(false);

  const persona = thread.persona;
  const attachedIds = new Set(attachedEntries.map((e) => e.mcp_server_id));

  return (
    <>
      {/* Overlay — click to close */}
      <div
        className={[styles.overlay, isOpen ? styles.overlayVisible : ""].join(
          " ",
        )}
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Pane */}
      <div
        className={[styles.pane, isOpen ? styles.paneOpen : ""].join(" ")}
        role="complementary"
        aria-label="Thread config"
      >
        {/* ── Header ── */}
        <div className={styles.header}>
          <span className={styles.headerTitle}>Thread Config</span>
          <button
            onClick={onClose}
            aria-label="Close thread config"
            className={styles.closeBtn}
          >
            <X size={15} />
          </button>
        </div>

        {/* ── Scrollable body ── */}
        <div className={styles.body}>
          {/* ── Persona ── */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <span className={styles.sectionTitle}>Persona</span>
            </div>
            {persona ? (
              <div
                className={styles.personaCard}
                onClick={onOpenSettings}
                role={onOpenSettings ? "button" : undefined}
                tabIndex={onOpenSettings ? 0 : undefined}
                onKeyDown={
                  onOpenSettings
                    ? (e) => e.key === "Enter" && onOpenSettings()
                    : undefined
                }
                title={onOpenSettings ? "Open persona settings" : undefined}
                style={{ cursor: onOpenSettings ? "pointer" : "default" }}
              >
                <div className={styles.personaAvatar}>{persona.emoji}</div>
                <div className={styles.personaInfo}>
                  <div className={styles.personaName}>{persona.name}</div>
                  <div className={styles.personaDesc}>
                    {persona.system_prompt.length > 60
                      ? persona.system_prompt.slice(0, 60) + "…"
                      : persona.system_prompt}
                  </div>
                </div>
                {onOpenSettings && (
                  <Settings size={13} className={styles.personaSettingsIcon} />
                )}
              </div>
            ) : (
              <div className={styles.emptyHint}>No persona attached</div>
            )}
          </div>

          <div className={styles.divider} />

          {/* ── Model ── */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <span className={styles.sectionTitle}>Model</span>
            </div>
            <ProviderModelSelector
              thread={thread}
              onUpdate={handleModelUpdate}
            />
          </div>

          <div className={styles.divider} />

          {/* ── Routines ── */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <span className={styles.sectionTitle}>Routines</span>
              {!showRoutineForm && (
                <button className={styles.addBtn} onClick={openAddForm}>
                  ＋ Add
                </button>
              )}
            </div>

            {routinesLoading ? (
              <div className={styles.emptyHint}>Loading…</div>
            ) : (
              <div className={styles.routineList}>
                {routines.length === 0 && !showRoutineForm && (
                  <div className={styles.emptyHint}>No routines yet.</div>
                )}

                {routines.map((r) => (
                  <div key={r.id} className={styles.routineCard}>
                    <div className={styles.routineCardTop}>
                      <Toggle
                        checked={r.enabled}
                        onChange={() => handleRoutineToggle(r.id)}
                      />
                      <div className={styles.routineCardInfo}>
                        <div className={styles.routineName}>{r.name}</div>
                        <div className={styles.routineCron}>{r.cron_expr}</div>
                      </div>
                      <div className={styles.routineCardActions}>
                        <button
                          className={styles.routineEditBtn}
                          onClick={() => openEditForm(r)}
                          title="Edit"
                        >
                          ✎
                        </button>
                        <button
                          className={styles.routineDeleteBtn}
                          onClick={() => handleRoutineDelete(r.id)}
                          disabled={deletingRoutineId === r.id}
                          title="Delete"
                        >
                          {deletingRoutineId === r.id ? "…" : "✕"}
                        </button>
                      </div>
                    </div>
                    <div className={styles.routinePromptPreview}>
                      {r.prompt.length > 80
                        ? r.prompt.slice(0, 80) + "…"
                        : r.prompt}
                    </div>
                  </div>
                ))}

                {/* ── Inline add/edit form ── */}
                {showRoutineForm && (
                  <div className={styles.routineForm}>
                    <div className={styles.routineFormTitle}>
                      {editingRoutine ? "Edit Routine" : "New Routine"}
                    </div>

                    <label className={styles.routineFormLabel}>Name</label>
                    <input
                      className={styles.routineFormInput}
                      placeholder="e.g. Morning Briefing"
                      value={routineFormName}
                      onChange={(e) => setRoutineFormName(e.target.value)}
                      disabled={routineFormSaving}
                    />

                    <label className={styles.routineFormLabel}>Prompt</label>
                    <textarea
                      className={styles.routineFormTextarea}
                      placeholder="What should the agent do when this fires?"
                      value={routineFormPrompt}
                      onChange={(e) => setRoutineFormPrompt(e.target.value)}
                      disabled={routineFormSaving}
                      rows={3}
                    />

                    <label className={styles.routineFormLabel}>Schedule</label>
                    <input
                      className={styles.routineFormInput}
                      style={{ fontFamily: "monospace" }}
                      placeholder="0 9 * * *"
                      value={routineFormCron}
                      onChange={(e) => setRoutineFormCron(e.target.value)}
                      disabled={routineFormSaving}
                    />
                    <CronPicker
                      value={routineFormCron}
                      onChange={setRoutineFormCron}
                    />

                    {routineFormError && (
                      <div className={styles.routineFormError}>
                        {routineFormError}
                      </div>
                    )}

                    <div className={styles.routineFormActions}>
                      <button
                        className={styles.routineFormCancel}
                        onClick={closeRoutineForm}
                        disabled={routineFormSaving}
                      >
                        Cancel
                      </button>
                      <button
                        className={styles.routineFormSave}
                        onClick={handleRoutineSave}
                        disabled={routineFormSaving}
                      >
                        {routineFormSaving ? "Saving…" : "Save"}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          <div className={styles.divider} />

          {/* ── MCP Servers ── */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <span className={styles.sectionTitle}>MCP Servers</span>
            </div>

            {mcpLoading ? (
              <div className={styles.emptyHint}>Loading…</div>
            ) : (
              <div className={styles.mcpList}>
                {attachedEntries.length === 0 && (
                  <div className={styles.emptyHint}>No servers attached.</div>
                )}
                {attachedEntries.map((entry) => {
                  const server = mcpServersMap[entry.mcp_server_id];
                  if (!server) return null;
                  return (
                    <McpServerCard
                      key={entry.id}
                      server={server}
                      tools={toolsMap[server.id] ?? null}
                      onRemove={() => handleDetachServer(server.id)}
                      onLoadTools={() => handleLoadTools(server.id)}
                    />
                  );
                })}

                {/* Attach button */}
                <div className={styles.mcpAttachWrap}>
                  <button
                    className={styles.mcpAttachBtn}
                    onClick={() => setShowAttachPicker((o) => !o)}
                  >
                    ＋ Attach server
                  </button>
                  {showAttachPicker && (
                    <AttachServerPicker
                      attachedIds={attachedIds}
                      onAttach={handleAttachServer}
                      onClose={() => setShowAttachPicker(false)}
                    />
                  )}
                </div>
              </div>
            )}

            {/* Tool activity toggle — inside MCP section, after server list */}
            <div className={styles.toolActivityRow}>
              <div className={styles.toolActivityInfo}>
                <div className={styles.toolActivityLabel}>
                  Show tool activity in chat
                </div>
                <div className={styles.toolActivityHint}>
                  Display tool calls and results inline in the conversation
                </div>
              </div>
              <Toggle
                checked={showToolActivity}
                onChange={handleToolActivityChange}
                disabled={isSavingToolActivity}
              />
            </div>

            {/* System events toggle */}
            <div className={styles.toolActivityRow}>
              <div className={styles.toolActivityInfo}>
                <div className={styles.toolActivityLabel}>
                  Show system events in chat
                </div>
                <div className={styles.toolActivityHint}>
                  Display model switches, MCP attach/detach, and other events
                </div>
              </div>
              <Toggle
                checked={showSystemEvents}
                onChange={handleSystemEventsChange}
                disabled={isSavingSystemEvents}
              />
            </div>
          </div>

          <div className={styles.divider} />

          {/* ── System Prompt Addendum ── */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <span className={styles.sectionTitle}>
                System Prompt Addendum
              </span>
              <span className={styles.sectionSubtitle}>
                {isSavingAddendum ? "Saving…" : "editable any time"}
              </span>
            </div>
            <textarea
              className={styles.addendumTextarea}
              placeholder="Additional instructions appended to this persona's system prompt for this thread only…"
              value={addendum}
              onChange={(e) => setAddendum(e.target.value)}
              onBlur={handleAddendumBlur}
              disabled={isSavingAddendum}
            />
            <div className={styles.fieldHint}>
              {persona
                ? `This text is appended to ${persona.name}'s base system prompt for this thread only.`
                : "This text is appended to the persona's system prompt for this thread only."}
            </div>
          </div>

          {/* ── Archive ── */}
          {onArchiveThread && (
            <>
              <div className={styles.divider} />
              <div className={styles.section}>
                <div className={styles.sectionHeader}>
                  <span className={styles.sectionTitle}>Danger Zone</span>
                </div>
                {showArchiveConfirm ? (
                  <div className={styles.archiveConfirm}>
                    <p className={styles.archiveConfirmText}>
                      Archive this thread? It will be hidden from your chat list
                      and moved to <strong>Settings → Archived Threads</strong>.
                    </p>
                    <p className={styles.archiveConfirmWarning}>
                      ⚠️ There is currently no way to restore an archived
                      thread. Restore functionality is planned for a future
                      update.
                    </p>
                    <div className={styles.archiveConfirmActions}>
                      <button
                        className={styles.archiveCancelBtn}
                        onClick={handleArchiveCancel}
                        disabled={isArchiving}
                      >
                        Cancel
                      </button>
                      <button
                        className={styles.archiveConfirmBtn}
                        onClick={handleArchiveConfirm}
                        disabled={isArchiving}
                      >
                        {isArchiving ? "Archiving…" : "Yes, archive it"}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    className={styles.archiveBtn}
                    onClick={handleArchiveClick}
                  >
                    📦 Archive Thread
                  </button>
                )}
              </div>
            </>
          )}
        </div>
        {/* /body */}
      </div>
    </>
  );
}

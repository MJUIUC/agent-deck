// Slide-up bottom sheet rendered over MobileChatView.
// Shows thread configuration: persona, model (interactive selects), routines (with toggles).
// ─────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback } from "react";
import cronstrue from "cronstrue";
import type {
  Thread,
  Routine,
  Provider,
  Model,
  McpServer,
  McpTool,
  ThreadMcpServer,
} from "@/types";
import {
  routinesApi,
  providersApi,
  modelsApi,
  threadsApi,
  mcpServersApi,
} from "@/api/client";
import { useThreadStore } from "@/stores/useThreadStore";
import { useSseStore } from "@/stores/useSseStore";
import styles from "./MobileConfigSheet.module.css";

// ─── Internal types ───────────────────────────────────────────────────────────

interface ProviderWithModels {
  provider: Provider;
  models: Model[];
}

// ─── Props ────────────────────────────────────────────────────────────────────

interface MobileConfigSheetProps {
  thread: Thread;
  isOpen: boolean;
  onClose: () => void;
  onArchive?: () => void;
}

// ─── Toggle sub-component ─────────────────────────────────────────────────────

interface ToggleProps {
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  label: string;
}

function Toggle({ checked, onChange, disabled = false, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={onChange}
      className={styles.toggleBtn}
    >
      <span
        className={[styles.toggleTrack, checked ? styles.toggleOn : ""]
          .filter(Boolean)
          .join(" ")}
      >
        <span
          className={[styles.toggleThumb, checked ? styles.toggleThumbOn : ""]
            .filter(Boolean)
            .join(" ")}
        />
      </span>
    </button>
  );
}

// ─── PickerDrawer sub-component ───────────────────────────────────────────────

interface PickerDrawerProps {
  label: string;
  value: string | null; // currently selected option id
  displayValue: string; // text to show on the collapsed row
  options: { id: string; label: string }[];
  onChange: (id: string) => void;
  disabled?: boolean;
}

function PickerDrawer({
  label,
  value,
  displayValue,
  options,
  onChange,
  disabled = false,
}: PickerDrawerProps) {
  const [open, setOpen] = useState(false);

  return (
    <div className={styles.pickerWrap}>
      {/* Collapsed trigger row */}
      <button
        type="button"
        className={cx(styles.pickerTrigger, open && styles.pickerTriggerOpen)}
        onClick={() => !disabled && setOpen((o) => !o)}
        disabled={disabled}
        aria-expanded={open}
        aria-label={`${label}: ${displayValue}`}
      >
        <span className={styles.pickerTriggerValue}>{displayValue}</span>
        <span
          className={cx(styles.pickerChevron, open && styles.pickerChevronOpen)}
          aria-hidden="true"
        >
          ›
        </span>
      </button>

      {/* Expandable drawer */}
      <div
        className={cx(styles.pickerDrawer, open && styles.pickerDrawerOpen)}
        aria-hidden={!open}
      >
        <div className={styles.pickerList}>
          {options.map((opt) => (
            <button
              key={opt.id}
              type="button"
              className={cx(
                styles.pickerItem,
                opt.id === value && styles.pickerItemActive,
              )}
              onClick={() => {
                onChange(opt.id);
                setOpen(false);
              }}
            >
              <span className={styles.pickerItemLabel}>{opt.label}</span>
              {opt.id === value && (
                <span className={styles.pickerItemCheck} aria-hidden="true">
                  ✓
                </span>
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Converts a cron expression to a human-readable string, or returns "" on failure. */
function cronToHuman(expr: string): string {
  try {
    return cronstrue.toString(expr, {
      verbose: false,
      throwExceptionOnParseError: true,
    });
  } catch {
    return "";
  }
}

/** Joins class names, filtering out falsy values. */
function cx(...classes: Array<string | undefined | false>): string {
  return classes.filter(Boolean).join(" ");
}

function mcpStatusDotClass(status: McpServer["status"]): string {
  switch (status) {
    case "connected":
      return styles.statusConnected;
    case "connecting":
      return styles.statusConnecting;
    case "error":
      return styles.statusError;
    default:
      return styles.statusInactive;
  }
}

// ─── Component ────────────────────────────────────────────────────────────────

export function MobileConfigSheet({
  thread,
  isOpen,
  onClose,
  onArchive,
}: MobileConfigSheetProps) {
  // ── Routines state ───────────────────────────────────────────────────────
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routinesLoading, setRoutinesLoading] = useState(false);
  const [togglingId, setTogglingId] = useState<string | null>(null);

  // ── Model/provider state ─────────────────────────────────────────────────
  const [providers, setProviders] = useState<Provider[]>([]);
  const [allModels, setAllModels] = useState<ProviderWithModels[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    null,
  );
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);

  // ── MCP state ─────────────────────────────────────────────────────────────
  const [attachedEntries, setAttachedEntries] = useState<ThreadMcpServer[]>([]);
  const [expandedMcpId, setExpandedMcpId] = useState<string | null>(null);
  const [mcpToolsCache, setMcpToolsCache] = useState<Record<string, McpTool[]>>(
    {},
  );
  const [mcpServersMap, setMcpServersMap] = useState<Record<string, McpServer>>(
    {},
  );
  const [mcpLoading, setMcpLoading] = useState(false);
  const [showMcpPicker, setShowMcpPicker] = useState(false);

  // ── Archive state ─────────────────────────────────────────────────────────
  const [showArchiveConfirm, setShowArchiveConfirm] = useState(false);
  const [isArchiving, setIsArchiving] = useState(false);

  // ── SSE ───────────────────────────────────────────────────────────────────
  const lastMcpStatusChange = useSseStore((s) => s.lastMcpStatusChange);

  // ── Refs ─────────────────────────────────────────────────────────────────
  const sheetRef = useRef<HTMLDivElement>(null);
  // clientY recorded at touchstart on the drag handle
  const dragStartY = useRef<number | null>(null);
  // Whether we've ever opened — avoids unmounting during the close animation
  const hasOpenedRef = useRef(false);

  // Mark as opened so the render guard doesn't strip the sheet during close animation
  useEffect(() => {
    if (isOpen) {
      hasOpenedRef.current = true;
    } else {
      // Reset archive confirm state when sheet closes
      setShowArchiveConfirm(false);
    }
  }, [isOpen]);

  // ── Fetch routines when sheet opens ──────────────────────────────────────
  useEffect(() => {
    if (!isOpen) return;

    setRoutinesLoading(true);
    routinesApi
      .list(thread.id)
      .then((res) => {
        setRoutines(res.data);
      })
      .catch(() => {
        setRoutines([]);
      })
      .finally(() => {
        setRoutinesLoading(false);
      });
  }, [isOpen, thread.id]);

  // ── Fetch providers + models when sheet opens ─────────────────────────────
  useEffect(() => {
    if (!isOpen) return;

    setModelsLoading(true);

    providersApi
      .list()
      .then(async (res) => {
        const enabledProviders = res.data.filter((p) => p.enabled);
        setProviders(enabledProviders);

        // Fetch models for each enabled provider in parallel
        const entries = await Promise.all(
          enabledProviders.map((p) =>
            modelsApi
              .list(p.id)
              .then((r) => ({
                provider: p,
                models: r.data.filter((m) => m.enabled),
              }))
              .catch(() => ({ provider: p, models: [] as Model[] })),
          ),
        );
        setAllModels(entries);

        // Initialise selections: prefer thread active values, fall back to persona defaults
        const initProvider =
          thread.active_provider ??
          thread.persona?.default_provider ??
          enabledProviders[0]?.id ??
          null;

        const initModel =
          thread.active_model ?? thread.persona?.default_model ?? null;

        setSelectedProviderId(initProvider);

        // If we have an init model, use it; otherwise pick the first model of the provider
        if (initModel) {
          setSelectedModelId(initModel);
        } else {
          const entry = entries.find((e) => e.provider.id === initProvider);
          setSelectedModelId(entry?.models[0]?.id ?? null);
        }
      })
      .catch(() => {
        setProviders([]);
        setAllModels([]);
      })
      .finally(() => {
        setModelsLoading(false);
      });
  }, [isOpen, thread.id]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Lock body scroll while sheet is open ─────────────────────────────────
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [isOpen]);

  // ── Fetch MCP data when sheet opens ──────────────────────────────────────
  useEffect(() => {
    if (!isOpen) return;
    setMcpLoading(true);
    Promise.all([threadsApi.listMcpServers(thread.id), mcpServersApi.list()])
      .then(([entriesRes, serversRes]) => {
        setAttachedEntries(entriesRes.data);
        const map: Record<string, McpServer> = {};
        for (const server of serversRes.data) {
          map[server.id] = server;
        }
        setMcpServersMap(map);
      })
      .catch(() => {
        setAttachedEntries([]);
        setMcpServersMap({});
      })
      .finally(() => {
        setMcpLoading(false);
      });
  }, [isOpen, thread.id]);

  // ── Live MCP status updates via SSE ──────────────────────────────────────
  useEffect(() => {
    if (!lastMcpStatusChange) return;
    const { mcp_server_id, status } = lastMcpStatusChange;
    setMcpServersMap((prev) => {
      if (!prev[mcp_server_id]) return prev;
      return {
        ...prev,
        [mcp_server_id]: {
          ...prev[mcp_server_id],
          status: status as McpServer["status"],
        },
      };
    });
  }, [lastMcpStatusChange]);

  // ── Routine toggle ────────────────────────────────────────────────────────
  const handleToggleRoutine = useCallback(
    async (routine: Routine) => {
      if (togglingId !== null) return; // debounce concurrent taps
      setTogglingId(routine.id);
      try {
        const res = await routinesApi.toggle(thread.id, routine.id);
        setRoutines((prev) =>
          prev.map((r) =>
            r.id === routine.id ? { ...r, enabled: res.data.enabled } : r,
          ),
        );
      } catch {
        // Silent fail on mobile — no toast needed here
      } finally {
        setTogglingId(null);
      }
    },
    [thread.id, togglingId],
  );

  // ── Provider/model change handlers ────────────────────────────────────────
  const handleProviderChange = useCallback(
    (provId: string) => {
      setSelectedProviderId(provId);
      const entry = allModels.find((e) => e.provider.id === provId);
      const firstModel = entry?.models[0] ?? null;
      setSelectedModelId(firstModel?.id ?? null);
      if (firstModel) {
        threadsApi
          .update(thread.id, {
            active_provider: provId,
            active_model: firstModel.id,
          })
          .then((res) => useThreadStore.getState().upsertThread(res.data))
          .catch(() => {});
      }
    },
    [allModels, thread.id],
  );

  const handleModelChange = useCallback(
    (modelId: string) => {
      setSelectedModelId(modelId);
      if (selectedProviderId) {
        threadsApi
          .update(thread.id, {
            active_provider: selectedProviderId,
            active_model: modelId,
          })
          .then((res) => useThreadStore.getState().upsertThread(res.data))
          .catch(() => {});
      }
    },
    [selectedProviderId, thread.id],
  );

  // ── Drag-to-dismiss (touch) ───────────────────────────────────────────────

  const handleTouchStart = useCallback(
    (e: React.TouchEvent<HTMLDivElement>) => {
      dragStartY.current = e.touches[0].clientY;
    },
    [],
  );

  const handleTouchMove = useCallback((e: React.TouchEvent<HTMLDivElement>) => {
    if (dragStartY.current === null || !sheetRef.current) return;
    const dy = e.touches[0].clientY - dragStartY.current;
    if (dy > 0) {
      // Drag sheet down live; disable transition so it follows the finger
      sheetRef.current.style.transition = "none";
      sheetRef.current.style.transform = `translateY(${dy}px)`;
    }
  }, []);

  const handleTouchEnd = useCallback(
    (e: React.TouchEvent<HTMLDivElement>) => {
      if (dragStartY.current === null || !sheetRef.current) return;
      const dy = e.changedTouches[0].clientY - dragStartY.current;

      // Re-enable transition before snapping back or closing
      sheetRef.current.style.transition = "";
      sheetRef.current.style.transform = "";

      if (dy > 80) {
        onClose();
      }

      dragStartY.current = null;
    },
    [onClose],
  );

  // ── MCP attach/detach handlers ────────────────────────────────────────────

  const handleDetachServer = useCallback(
    async (mcpServerId: string) => {
      const server = mcpServersMap[mcpServerId];
      try {
        await threadsApi.detachMcpServer(thread.id, mcpServerId);
        setAttachedEntries((prev) =>
          prev.filter((e) => e.mcp_server_id !== mcpServerId),
        );
        if (server) {
          threadsApi
            .notify(thread.id, "mcp_server_detached", { name: server.name })
            .catch(() => {});
        }
      } catch {
        // Silent fail on mobile
      }
    },
    [thread.id, mcpServersMap],
  );

  // ── Archive handler ───────────────────────────────────────────────────────
  const handleArchiveConfirm = useCallback(async () => {
    setIsArchiving(true);
    try {
      await useThreadStore.getState().archiveThread(thread.id);
      onClose();
      onArchive?.();
    } catch {
      // Silent fail — store will not update, user can retry
    } finally {
      setIsArchiving(false);
      setShowArchiveConfirm(false);
    }
  }, [thread.id, onClose, onArchive]);

  const handleAttachServer = useCallback(
    async (server: McpServer) => {
      try {
        const { data: entry } = await threadsApi.attachMcpServer(
          thread.id,
          server.id,
        );
        setAttachedEntries((prev) => [...prev, entry]);
        setMcpServersMap((prev) => ({ ...prev, [server.id]: server }));
        setShowMcpPicker(false);
        threadsApi
          .notify(thread.id, "mcp_server_attached", { name: server.name })
          .catch(() => {});
      } catch {
        // Silent fail on mobile
      }
    },
    [thread.id],
  );

  // ── Render guard — don't mount until the sheet has been opened once ───────
  // This prevents the closed sheet from flash-rendering on first load.
  if (!hasOpenedRef.current && !isOpen) {
    return null;
  }

  const persona = thread.persona;

  return (
    <>
      {/* ── Backdrop ─────────────────────────────────────────────────────── */}
      <div
        className={cx(styles.backdrop, isOpen && styles.backdropVisible)}
        onClick={onClose}
        aria-hidden="true"
      />

      {/* ── Bottom sheet ─────────────────────────────────────────────────── */}
      <div
        ref={sheetRef}
        role="dialog"
        aria-modal="true"
        aria-label="Thread Config"
        className={cx(styles.sheet, !isOpen && styles.sheetClosed)}
      >
        {/* ── Drag handle ──────────────────────────────────────────────── */}
        <div
          className={styles.handle}
          onTouchStart={handleTouchStart}
          onTouchMove={handleTouchMove}
          onTouchEnd={handleTouchEnd}
          aria-hidden="true"
        >
          <div className={styles.handleBar} />
        </div>

        {/* ── Header ───────────────────────────────────────────────────── */}
        <div className={styles.header}>
          <span className={styles.title}>Thread Config</span>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Close thread config"
          >
            ✕
          </button>
        </div>

        {/* ── Scrollable body ───────────────────────────────────────────── */}
        <div className={styles.body}>
          {/* ── Persona ─────────────────────────────────────────────── */}
          <section
            className={styles.section}
            aria-labelledby="mcs-label-persona"
          >
            <div id="mcs-label-persona" className={styles.sectionLabel}>
              Persona
            </div>

            <div className={styles.personaRow}>
              <div className={styles.personaAvatar} aria-hidden="true">
                {persona?.emoji ?? "🤖"}
              </div>

              <div className={styles.personaInfo}>
                <div className={styles.personaName}>
                  {persona?.name ?? "Default"}
                </div>
                <div className={styles.personaDesc}>
                  {persona?.system_prompt
                    ? persona.system_prompt.length > 72
                      ? persona.system_prompt.slice(0, 72) + "…"
                      : persona.system_prompt
                    : "No persona description"}
                </div>
              </div>
            </div>
          </section>

          <div className={styles.divider} aria-hidden="true" />

          {/* ── Model ────────────────────────────────────────────────── */}
          <section className={styles.section} aria-labelledby="mcs-label-model">
            <div id="mcs-label-model" className={styles.sectionLabel}>
              Model
            </div>

            {modelsLoading ? (
              <div
                className={styles.stateText}
                role="status"
                aria-live="polite"
              >
                Loading models…
              </div>
            ) : providers.length === 0 ? (
              <div className={styles.stateText}>No providers configured.</div>
            ) : (
              <div className={styles.modelPickerGroup}>
                <PickerDrawer
                  label="Provider"
                  value={selectedProviderId}
                  displayValue={
                    providers.find((p) => p.id === selectedProviderId)?.name ??
                    "Select provider…"
                  }
                  options={providers.map((p) => ({ id: p.id, label: p.name }))}
                  onChange={handleProviderChange}
                />
                <PickerDrawer
                  label="Model"
                  value={selectedModelId}
                  displayValue={(() => {
                    const models =
                      allModels.find(
                        (e) => e.provider.id === selectedProviderId,
                      )?.models ?? [];
                    const m = models.find((m) => m.id === selectedModelId);
                    return m ? m.display_name || m.model_id : "Select model…";
                  })()}
                  options={(
                    allModels.find((e) => e.provider.id === selectedProviderId)
                      ?.models ?? []
                  ).map((m) => ({
                    id: m.id,
                    label: m.display_name || m.model_id,
                  }))}
                  onChange={handleModelChange}
                  disabled={!selectedProviderId}
                />
              </div>
            )}
          </section>

          <div className={styles.divider} aria-hidden="true" />

          {/* ── Routines ─────────────────────────────────────────────── */}
          <section
            className={styles.section}
            aria-labelledby="mcs-label-routines"
          >
            <div id="mcs-label-routines" className={styles.sectionLabel}>
              Routines
            </div>

            {routinesLoading ? (
              <div
                className={styles.stateText}
                role="status"
                aria-live="polite"
              >
                Loading routines…
              </div>
            ) : routines.length === 0 ? (
              <div className={styles.stateText}>No routines configured.</div>
            ) : (
              <div className={styles.routinesList}>
                {routines.map((routine) => {
                  const humanLabel = cronToHuman(routine.cron_expr);
                  const isToggling = togglingId === routine.id;

                  return (
                    <div key={routine.id} className={styles.routineRow}>
                      {/* Icon */}
                      <div
                        className={cx(
                          styles.routineIcon,
                          routine.enabled && styles.routineIconActive,
                        )}
                        aria-hidden="true"
                      >
                        ⏰
                      </div>

                      {/* Info */}
                      <div className={styles.routineInfo}>
                        <div className={styles.routineName}>{routine.name}</div>

                        <div className={styles.routineSchedule}>
                          <span className={styles.routineCron}>
                            {routine.cron_expr}
                          </span>
                          {humanLabel && (
                            <span className={styles.routineHuman}>
                              {humanLabel}
                            </span>
                          )}
                        </div>

                        <div
                          className={cx(
                            styles.routineStatus,
                            routine.enabled
                              ? styles.routineStatusOn
                              : styles.routineStatusOff,
                          )}
                          aria-live="polite"
                        >
                          <span
                            className={styles.statusDot}
                            aria-hidden="true"
                          />
                          {routine.enabled ? "Enabled" : "Disabled"}
                        </div>
                      </div>

                      {/* Toggle */}
                      <Toggle
                        checked={routine.enabled}
                        onChange={() => handleToggleRoutine(routine)}
                        disabled={isToggling}
                        label={`${routine.enabled ? "Disable" : "Enable"} routine: ${routine.name}`}
                      />
                    </div>
                  );
                })}
              </div>
            )}
          </section>

          <div className={styles.divider} aria-hidden="true" />

          {/* ── Tools ────────────────────────────────────────────────── */}
          <section className={styles.section} aria-labelledby="mcs-label-tools">
            <div id="mcs-label-tools" className={styles.sectionLabel}>
              Tools
            </div>

            {mcpLoading ? (
              <div
                className={styles.stateText}
                role="status"
                aria-live="polite"
              >
                Loading…
              </div>
            ) : (
              <div className={styles.toolsList}>
                {attachedEntries.map((entry) => {
                  const server = mcpServersMap[entry.mcp_server_id];
                  if (!server) return null;
                  return (
                    <div
                      key={entry.id}
                      className={cx(styles.toolRow, styles.toolRowExpanded)}
                    >
                      {/* Main row */}
                      <div className={styles.toolRowInner}>
                        <span
                          className={cx(
                            styles.mcpStatusDot,
                            mcpStatusDotClass(server.status),
                          )}
                          aria-hidden="true"
                        />
                        <span className={styles.toolName}>{server.name}</span>
                        <button
                          type="button"
                          className={styles.toolsExpandBtn}
                          onClick={async () => {
                            if (expandedMcpId === entry.mcp_server_id) {
                              setExpandedMcpId(null);
                            } else {
                              setExpandedMcpId(entry.mcp_server_id);
                              if (!mcpToolsCache[entry.mcp_server_id]) {
                                try {
                                  const res = await mcpServersApi.listTools(
                                    entry.mcp_server_id,
                                  );
                                  setMcpToolsCache((prev) => ({
                                    ...prev,
                                    [entry.mcp_server_id]: res.data,
                                  }));
                                } catch {
                                  setMcpToolsCache((prev) => ({
                                    ...prev,
                                    [entry.mcp_server_id]: [],
                                  }));
                                }
                              }
                            }
                          }}
                        >
                          Tools{" "}
                          {expandedMcpId === entry.mcp_server_id ? "▲" : "▼"}
                        </button>
                        <button
                          type="button"
                          className={styles.detachBtn}
                          onClick={() =>
                            handleDetachServer(entry.mcp_server_id)
                          }
                          aria-label={`Detach ${server.name}`}
                        >
                          ✕
                        </button>
                      </div>

                      {/* Tools accordion */}
                      {expandedMcpId === entry.mcp_server_id && (
                        <div className={styles.toolRowAccordion}>
                          {/* Timeout */}
                          <div className={styles.toolRowTimeoutRow}>
                            <span className={styles.toolRowTimeoutLabel}>
                              Timeout (s):
                            </span>
                            <input
                              type="number"
                              min="1"
                              className={styles.toolRowTimeoutInput}
                              value={entry.tool_call_timeout_secs ?? ""}
                              placeholder="inherit"
                              onChange={async (e) => {
                                const val = e.target.value.trim();
                                const timeout = val ? parseInt(val, 10) : null;
                                try {
                                  const { data: updated } =
                                    await threadsApi.updateThreadMcpServer(
                                      entry.thread_id,
                                      entry.mcp_server_id,
                                      { tool_call_timeout_secs: timeout },
                                    );
                                  setAttachedEntries((prev) =>
                                    prev.map((e) =>
                                      e.id === updated.id ? updated : e,
                                    ),
                                  );
                                } catch {
                                  /* silently degrade */
                                }
                              }}
                            />
                          </div>
                          {/* Tool toggles */}
                          {server.status !== "connected" &&
                            !mcpToolsCache[entry.mcp_server_id] && (
                              <span className={styles.toolRowEmpty}>
                                Connect the server to load tools.
                              </span>
                            )}
                          {(mcpToolsCache[entry.mcp_server_id] ?? []).map(
                            (tool) => (
                              <label
                                key={tool.name}
                                className={styles.toolToggleLabel}
                              >
                                <input
                                  type="checkbox"
                                  checked={
                                    !entry.disabled_tools.includes(tool.name)
                                  }
                                  onChange={async () => {
                                    const isDisabled =
                                      entry.disabled_tools.includes(tool.name);
                                    const newList = isDisabled
                                      ? entry.disabled_tools.filter(
                                          (n) => n !== tool.name,
                                        )
                                      : [...entry.disabled_tools, tool.name];
                                    try {
                                      const { data: updated } =
                                        await threadsApi.updateThreadMcpServer(
                                          entry.thread_id,
                                          entry.mcp_server_id,
                                          { disabled_tools: newList },
                                        );
                                      setAttachedEntries((prev) =>
                                        prev.map((e) =>
                                          e.id === updated.id ? updated : e,
                                        ),
                                      );
                                    } catch {
                                      /* silently degrade */
                                    }
                                  }}
                                />
                                <span
                                  className={cx(
                                    styles.toolToggleName,
                                    entry.disabled_tools.includes(tool.name) &&
                                      styles.toolToggleNameDisabled,
                                  )}
                                >
                                  {tool.name}
                                </span>
                              </label>
                            ),
                          )}
                          {mcpToolsCache[entry.mcp_server_id]?.length === 0 && (
                            <span className={styles.toolRowEmpty}>
                              No tools reported.
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}

                <button
                  type="button"
                  className={styles.attachRow}
                  onClick={() => setShowMcpPicker(true)}
                >
                  + Attach Server
                </button>
              </div>
            )}

            {/* Picker overlay */}
            {showMcpPicker &&
              (() => {
                const attachedIds = new Set(
                  attachedEntries.map((e) => e.mcp_server_id),
                );
                const available = Object.values(mcpServersMap).filter(
                  (s) => !attachedIds.has(s.id),
                );
                return (
                  <div
                    className={styles.pickerOverlay}
                    role="dialog"
                    aria-label="Attach MCP Server"
                  >
                    <div className={styles.pickerOverlayHeader}>
                      <span className={styles.pickerOverlayTitle}>
                        Attach MCP Server
                      </span>
                      <button
                        type="button"
                        className={styles.closeBtn}
                        onClick={() => setShowMcpPicker(false)}
                        aria-label="Close picker"
                      >
                        ✕
                      </button>
                    </div>
                    <div className={styles.pickerOverlayBody}>
                      {available.length === 0 ? (
                        <div className={styles.stateText}>
                          All available servers are attached.
                        </div>
                      ) : (
                        available.map((server) => (
                          <button
                            key={server.id}
                            type="button"
                            className={styles.pickerServerRow}
                            onClick={() => handleAttachServer(server)}
                          >
                            <span
                              className={cx(
                                styles.mcpStatusDot,
                                mcpStatusDotClass(server.status),
                              )}
                              aria-hidden="true"
                            />
                            <span className={styles.pickerServerName}>
                              {server.name}
                            </span>
                          </button>
                        ))
                      )}
                    </div>
                  </div>
                );
              })()}
          </section>

          {/* ── Archive ──────────────────────────────────────────────── */}
          <section className={styles.section}>
            {showArchiveConfirm ? (
              <div className={styles.archiveConfirm}>
                <p className={styles.archiveConfirmText}>
                  Archive this thread?
                </p>
                <p className={styles.archiveConfirmHint}>
                  It will be moved to Archived Threads in Settings and can be
                  restored at any time.
                </p>
                <div className={styles.archiveConfirmActions}>
                  <button
                    type="button"
                    className={styles.archiveCancelBtn}
                    onClick={() => setShowArchiveConfirm(false)}
                    disabled={isArchiving}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    className={styles.archiveConfirmBtn}
                    onClick={handleArchiveConfirm}
                    disabled={isArchiving}
                  >
                    {isArchiving ? "Archiving…" : "Archive"}
                  </button>
                </div>
              </div>
            ) : (
              <button
                type="button"
                className={styles.archiveBtn}
                onClick={() => setShowArchiveConfirm(true)}
              >
                Archive Thread
              </button>
            )}
          </section>
        </div>
        {/* /body */}
      </div>
      {/* /sheet */}
    </>
  );
}

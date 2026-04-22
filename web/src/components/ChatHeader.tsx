import { useState, useEffect, useRef } from "react";
import type { Thread, Provider, Model } from "@/types";
import { useThreadStore } from "@/stores/useThreadStore";
import { providersApi, modelsApi, threadsApi } from "@/api/client";
import { Menu, FolderOpen } from "lucide-react";
import styles from "./ChatHeader.module.css";

interface ChatHeaderProps {
  thread: Thread;
  /** Optional — when undefined the settings button is rendered disabled (draft mode). */
  onToggleConfig?: () => void;
  onMobileMenuOpen?: () => void;
  onOpenExplorer?: () => void;
  onTitleUpdate?: (updated: Thread) => void;
}

// Module-level cache so all ChatHeader instances share one fetch per session.
let cachedProviders: Provider[] | null = null;
const cachedModelsByProvider: Record<string, Model[]> = {};

export async function resolveDisplayNames(
  providerUuid: string | null,
  modelUuid: string | null,
): Promise<{ providerName: string | null; modelName: string | null }> {
  if (!providerUuid && !modelUuid) {
    return { providerName: null, modelName: null };
  }

  // Load providers once
  if (!cachedProviders) {
    try {
      const { data } = await providersApi.list();
      cachedProviders = data;
    } catch {
      return { providerName: null, modelName: null };
    }
  }

  const provider = cachedProviders.find((p) => p.id === providerUuid) ?? null;
  const providerName = provider?.name ?? null;

  if (!modelUuid || !provider) {
    return { providerName, modelName: null };
  }

  // Load models for this provider once
  if (!cachedModelsByProvider[provider.id]) {
    try {
      const { data } = await modelsApi.list(provider.id);
      cachedModelsByProvider[provider.id] = data;
    } catch {
      cachedModelsByProvider[provider.id] = [];
    }
  }

  const model =
    cachedModelsByProvider[provider.id].find((m) => m.id === modelUuid) ?? null;
  const modelName = model?.display_name || model?.model_id || null;

  return { providerName, modelName };
}

export function ChatHeader({
  thread,
  onToggleConfig,
  onMobileMenuOpen,
  onOpenExplorer,
  onTitleUpdate,
}: ChatHeaderProps) {
  const persona = thread.persona;
  const emoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

  const [providerName, setProviderName] = useState<string | null>(null);
  const [modelName, setModelName] = useState<string | null>(null);

  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState("");
  const [saving, setSaving] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    // Prefer thread's own active values; fall back to persona defaults (UUIDs)
    const providerUuid =
      thread.active_provider ?? thread.persona?.default_provider ?? null;
    const modelUuid =
      thread.active_model ?? thread.persona?.default_model ?? null;
    resolveDisplayNames(providerUuid, modelUuid).then(
      ({ providerName: pn, modelName: mn }) => {
        if (!cancelled) {
          setProviderName(pn);
          setModelName(mn);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [
    thread.active_provider,
    thread.active_model,
    thread.persona?.default_provider,
    thread.persona?.default_model,
  ]);

  useEffect(() => {
    if (editing) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [editing]);

  const parts: string[] = [personaName];
  if (providerName) parts.push(providerName);
  if (modelName) parts.push(modelName);
  const subtitle = parts.join(" · ");

  function startEditing() {
    setEditValue(thread.title ?? "");
    setEditing(true);
  }

  function cancelEditing() {
    setEditing(false);
    setEditValue("");
  }

  async function commitEdit() {
    const trimmed = editValue.trim();
    if (!trimmed || trimmed === thread.title) {
      cancelEditing();
      return;
    }

    setSaving(true);
    try {
      const res = await threadsApi.update(thread.id, { title: trimmed });
      useThreadStore.getState().upsertThread(res.data);
      onTitleUpdate?.(res.data);
    } catch {
      // Leave editing mode even on failure — the caller's state will not update,
      // so the title will revert to the previous value on next render.
    } finally {
      setSaving(false);
      setEditing(false);
      setEditValue("");
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitEdit();
    } else if (e.key === "Escape") {
      cancelEditing();
    }
  }

  // Prevent the blur-triggered commitEdit from double-firing after Enter/Escape
  // already closed editing. We track whether we initiated the blur ourselves.
  function handleBlur() {
    if (editing) {
      commitEdit();
    }
  }

  return (
    <div className={styles.header}>
      {/* Left: optional hamburger + avatar + info */}
      <div className={styles.left}>
        {onMobileMenuOpen && (
          <button
            onClick={onMobileMenuOpen}
            aria-label="Open sidebar"
            className={styles.menuBtn}
          >
            <Menu size={18} />
          </button>
        )}

        <div className={styles.avatar}>{emoji}</div>

        <div className={styles.meta}>
          {editing ? (
            <input
              ref={inputRef}
              className={styles.titleInput}
              value={editValue}
              disabled={saving}
              onChange={(e) => setEditValue(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={handleBlur}
              aria-label="Edit thread title"
            />
          ) : (
            <div
              className={`${styles.title} ${styles.titleEditable}`}
              onClick={startEditing}
              title="Click to rename"
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  startEditing();
                }
              }}
            >
              {thread.title}
            </div>
          )}
          <div className={styles.subtitle}>{subtitle}</div>
        </div>
      </div>

      {/* Right: action buttons */}
      <div className={styles.right}>
        {onOpenExplorer && (
          <button
            onClick={onOpenExplorer}
            aria-label="Open file explorer"
            title="File explorer"
            className={styles.iconBtn}
          >
            <FolderOpen size={16} />
          </button>
        )}
        <button
          onClick={onToggleConfig}
          disabled={!onToggleConfig}
          aria-label="Thread settings"
          title={
            onToggleConfig
              ? "Thread settings"
              : "Settings unavailable in draft mode"
          }
          className={styles.iconBtn}
        >
          <Menu size={16} />
        </button>
      </div>
    </div>
  );
}

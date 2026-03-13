import { useState, useEffect } from "react";
import type { Thread, Provider, Model } from "@/types";
import { providersApi, modelsApi } from "@/api/client";
import { Settings, Menu } from "lucide-react";
import styles from "./ChatHeader.module.css";

interface ChatHeaderProps {
  thread: Thread;
  onToggleConfig: () => void;
  onMobileMenuOpen?: () => void;
}

// Module-level cache so all ChatHeader instances share one fetch per session.
let cachedProviders: Provider[] | null = null;
let cachedModelsByProvider: Record<string, Model[]> = {};

async function resolveDisplayNames(
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
}: ChatHeaderProps) {
  const persona = thread.persona;
  const emoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

  const [providerName, setProviderName] = useState<string | null>(null);
  const [modelName, setModelName] = useState<string | null>(null);

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

  const parts: string[] = [personaName];
  if (providerName) parts.push(providerName);
  if (modelName) parts.push(modelName);
  const subtitle = parts.join(" · ");

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
          <div className={styles.title}>{thread.title}</div>
          <div className={styles.subtitle}>{subtitle}</div>
        </div>
      </div>

      {/* Right: action buttons */}
      <div className={styles.right}>
        <button
          onClick={onToggleConfig}
          aria-label="Thread settings"
          title="Thread settings"
          className={styles.iconBtn}
        >
          <Settings size={16} />
        </button>
      </div>
    </div>
  );
}

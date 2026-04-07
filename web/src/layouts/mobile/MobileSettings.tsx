import { useEffect, useState, type ReactNode } from "react";
import {
  useThemeStore,
  type Palette,
  type Mode,
} from "../../stores/useThemeStore";

import {
  pushApi,
  providersApi,
  credentialsApi,
  mcpServersApi,
  personasApi,
  type Credential,
  type CredentialType,
  type McpServerConfig,
} from "../../api/client";
import { usePlatform } from "../../hooks/usePlatform";
import { useSseStore } from "../../stores/useSseStore";
import type { Provider, McpServer, AgentPersona } from "../../types";
import styles from "./MobileSettings.module.css";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function urlBase64ToUint8Array(base64String: string): Uint8Array<ArrayBuffer> {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const rawData = window.atob(base64);
  const output = new Uint8Array(new ArrayBuffer(rawData.length));
  for (let i = 0; i < rawData.length; i++) {
    output[i] = rawData.charCodeAt(i);
  }
  return output;
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

// ─── Notification status ──────────────────────────────────────────────────────

type NotifState =
  | "loading"
  | "enabled"
  | "not-enabled"
  | "blocked"
  | "unavailable";

function useNotificationStatus(): {
  state: NotifState;
  recheck: () => void;
} {
  const [state, setState] = useState<NotifState>("loading");
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    async function check(): Promise<void> {
      if (typeof Notification === "undefined") {
        setState("unavailable");
        return;
      }

      const permission = Notification.permission;

      if (permission === "denied") {
        setState("blocked");
        return;
      }

      if (!navigator.serviceWorker) {
        setState("unavailable");
        return;
      }

      let subscription: PushSubscription | null = null;
      try {
        const reg = await navigator.serviceWorker.ready;
        subscription = await reg.pushManager.getSubscription();
      } catch {
        setState("unavailable");
        return;
      }

      if (permission === "granted" && subscription !== null) {
        setState("enabled");
      } else {
        setState("not-enabled");
      }
    }

    check().catch(() => setState("unavailable"));
  }, [refreshKey]);

  return { state, recheck: () => setRefreshKey((k) => k + 1) };
}

// ─── Install steps ────────────────────────────────────────────────────────────

const IOS_STEPS: ReactNode[] = [
  <>
    Open this page in <strong>Safari</strong> (not Chrome or Firefox)
  </>,
  <>
    Tap the <strong>Share</strong> button at the bottom of the screen
  </>,
  <>
    Scroll down and tap <strong>"Add to Home Screen"</strong>
  </>,
  <>
    Tap <strong>"Add"</strong> — agent-deck will appear on your home screen
  </>,
];

const ANDROID_STEPS: ReactNode[] = [
  <>
    Open this page in <strong>Chrome</strong>
  </>,
  <>
    Tap the <strong>⋮ menu</strong> in the top-right corner
  </>,
  <>
    Tap <strong>"Add to Home Screen"</strong> or <strong>"Install app"</strong>
  </>,
  <>
    Tap <strong>"Add"</strong> — agent-deck will appear in your app drawer
  </>,
];

// ─── Theme palettes ───────────────────────────────────────────────────────────

const PALETTES: { id: Palette; label: string; accent: string }[] = [
  { id: "olive", label: "Olive", accent: "#7c8c5a" },
  { id: "slate", label: "Slate", accent: "#58a6ff" },
  { id: "midnight", label: "Midnight", accent: "#8b7fd4" },
  { id: "rose", label: "Rose", accent: "#c47a8a" },
  { id: "forest", label: "Forest", accent: "#4caf72" },
  { id: "ember", label: "Ember", accent: "#d4853a" },
  { id: "ocean", label: "Ocean", accent: "#2ab8d0" },
  { id: "copper", label: "Copper", accent: "#c07840" },
  { id: "sakura", label: "Sakura", accent: "#e8709a" },
  { id: "noir", label: "Noir", accent: "#e0e0e0" },
];

// ─── MobileSettings ───────────────────────────────────────────────────────────

export function MobileSettings() {
  const platform = usePlatform();
  const { state: notifState, recheck } = useNotificationStatus();
  const [notifLoading, setNotifLoading] = useState(false);

  const palette = useThemeStore((s) => s.palette);
  const mode = useThemeStore((s) => s.mode);
  const setPalette = useThemeStore((s) => s.setPalette);
  const setMode = useThemeStore((s) => s.setMode);

  // ── Providers ────────────────────────────────────────────────────────────────
  const [providers, setProviders] = useState<Provider[]>([]);
  const [providerDrawerOpen, setProviderDrawerOpen] = useState(false);
  const [pendingDeleteProviderId, setPendingDeleteProviderId] = useState<
    string | null
  >(null);
  const [providerName, setProviderName] = useState("");
  const [providerKind, setProviderKind] = useState<
    "copilot" | "openai" | "anthropic" | "custom"
  >("openai");
  const [providerBaseUrl, setProviderBaseUrl] = useState("");
  const [providerApiKey, setProviderApiKey] = useState("");
  const [providerNameError, setProviderNameError] = useState("");
  const [providerSaving, setProviderSaving] = useState(false);

  // ── Credentials ──────────────────────────────────────────────────────────────
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [credentialDrawerOpen, setCredentialDrawerOpen] = useState(false);
  const [pendingDeleteCredentialKey, setPendingDeleteCredentialKey] = useState<
    string | null
  >(null);
  const [credKey, setCredKey] = useState("");
  const [credDisplayName, setCredDisplayName] = useState("");
  const [credService, setCredService] = useState("");
  const [credType, setCredType] = useState<CredentialType>("api_key");
  const [credSecret, setCredSecret] = useState("");
  const [credKeyError, setCredKeyError] = useState("");
  const [credDisplayNameError, setCredDisplayNameError] = useState("");
  const [credSaving, setCredSaving] = useState(false);

  // ── MCP Servers ──────────────────────────────────────────────────────────────
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [mcpDrawerOpen, setMcpDrawerOpen] = useState(false);
  const [pendingDeleteMcpId, setPendingDeleteMcpId] = useState<string | null>(
    null,
  );
  const [mcpName, setMcpName] = useState("");
  const [mcpType, setMcpType] = useState<"local" | "remote">("local");
  const [mcpExecutable, setMcpExecutable] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpUrl, setMcpUrl] = useState("");
  const [mcpCredentialKey, setMcpCredentialKey] = useState("");
  const [mcpNameError, setMcpNameError] = useState("");
  const [mcpSaving, setMcpSaving] = useState(false);

  // ── Personas ─────────────────────────────────────────────────────────────────
  const [personas, setPersonas] = useState<AgentPersona[]>([]);
  const [personaDrawerOpen, setPersonaDrawerOpen] = useState(false);
  const [editingPersona, setEditingPersona] = useState<AgentPersona | null>(
    null,
  );
  const [pendingDeletePersonaId, setPendingDeletePersonaId] = useState<
    string | null
  >(null);
  const [personaName, setPersonaName] = useState("");
  const [personaEmoji, setPersonaEmoji] = useState("");
  const [personaSystemPrompt, setPersonaSystemPrompt] = useState("");
  const [personaNameError, setPersonaNameError] = useState("");
  const [personaSaving, setPersonaSaving] = useState(false);

  // ── SSE ──────────────────────────────────────────────────────────────────────
  const lastMcpStatusChange = useSseStore((s) => s.lastMcpStatusChange);

  // ── Load on mount ─────────────────────────────────────────────────────────────
  useEffect(() => {
    providersApi
      .list()
      .then((res) => setProviders(res.data))
      .catch(() => {});
    credentialsApi
      .list()
      .then((res) => setCredentials(res))
      .catch(() => {});
    mcpServersApi
      .list()
      .then((res) => setMcpServers(res.data))
      .catch(() => {});
    personasApi
      .list()
      .then((res) => setPersonas(res.data))
      .catch(() => {});
  }, []);

  // ── Live MCP status via SSE ───────────────────────────────────────────────────
  useEffect(() => {
    if (!lastMcpStatusChange) return;
    const { mcp_server_id, status } = lastMcpStatusChange;
    setMcpServers((prev) =>
      prev.map((server) =>
        server.id === mcp_server_id
          ? { ...server, status: status as McpServer["status"] }
          : server,
      ),
    );
  }, [lastMcpStatusChange]);

  // ── Populate persona form when editing ───────────────────────────────────────
  useEffect(() => {
    if (editingPersona) {
      setPersonaName(editingPersona.name);
      setPersonaEmoji(editingPersona.emoji);
      setPersonaSystemPrompt(editingPersona.system_prompt);
    } else {
      setPersonaName("");
      setPersonaEmoji("");
      setPersonaSystemPrompt("");
    }
  }, [editingPersona]);

  // ── Notification helpers ──────────────────────────────────────────────────────
  const steps =
    platform === "ios"
      ? IOS_STEPS
      : platform === "android"
        ? ANDROID_STEPS
        : null;

  const dotClass = (() => {
    switch (notifState) {
      case "enabled":
        return styles.statusDotGreen;
      case "not-enabled":
        return styles.statusDotYellow;
      case "blocked":
        return styles.statusDotRed;
      default:
        return styles.statusDotGrey;
    }
  })();

  const statusMessage = (() => {
    switch (notifState) {
      case "enabled":
        return "Notifications enabled";
      case "not-enabled":
        return "Notifications not enabled";
      case "blocked":
        return "Notifications blocked — enable in your browser settings";
      case "unavailable":
        return "Notifications unavailable in this browser";
      default:
        return "";
    }
  })();

  async function handleEnable() {
    setNotifLoading(true);
    try {
      const permission = await Notification.requestPermission();
      if (permission !== "granted") {
        recheck();
        setNotifLoading(false);
        return;
      }
      const reg = await navigator.serviceWorker.ready;
      const { data } = await pushApi.getVapidPublicKey();
      const sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToUint8Array(data.public_key),
      });
      const subJson = sub.toJSON() as {
        endpoint: string;
        keys: { p256dh: string; auth: string };
      };
      await pushApi.subscribe({
        endpoint: subJson.endpoint,
        p256dh: subJson.keys.p256dh,
        auth: subJson.keys.auth,
        user_agent: navigator.userAgent,
      });
      recheck();
    } catch (err) {
      console.error("Enable notifications failed:", err);
    } finally {
      setNotifLoading(false);
    }
  }

  async function handleDisable() {
    setNotifLoading(true);
    try {
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) {
        await sub.unsubscribe();
        await pushApi.unsubscribe(sub.endpoint);
      }
      recheck();
    } catch (err) {
      console.error("Disable notifications failed:", err);
    } finally {
      setNotifLoading(false);
    }
  }

  // ── Provider handlers ─────────────────────────────────────────────────────────
  function resetProviderForm() {
    setProviderName("");
    setProviderKind("openai");
    setProviderBaseUrl("");
    setProviderApiKey("");
    setProviderNameError("");
  }

  async function handleSaveProvider() {
    if (!providerName.trim()) {
      setProviderNameError("Name is required");
      return;
    }
    setProviderSaving(true);
    try {
      await providersApi.create({
        name: providerName.trim(),
        kind: providerKind,
        base_url: providerBaseUrl,
        api_key: providerApiKey || undefined,
      });
      const refreshed = await providersApi.list();
      setProviders(refreshed.data);
      setProviderDrawerOpen(false);
      resetProviderForm();
    } catch (err) {
      console.error("Save provider failed:", err);
    } finally {
      setProviderSaving(false);
    }
  }

  async function handleToggleProvider(provider: Provider) {
    try {
      await providersApi.update(provider.id, { enabled: !provider.enabled });
      const refreshed = await providersApi.list();
      setProviders(refreshed.data);
    } catch (err) {
      console.error("Toggle provider failed:", err);
    }
  }

  async function handleDeleteProvider(id: string) {
    try {
      await providersApi.delete(id);
      const refreshed = await providersApi.list();
      setProviders(refreshed.data);
      setPendingDeleteProviderId(null);
    } catch (err) {
      console.error("Delete provider failed:", err);
    }
  }

  // ── Credential handlers ───────────────────────────────────────────────────────
  function resetCredentialForm() {
    setCredKey("");
    setCredDisplayName("");
    setCredService("");
    setCredType("api_key");
    setCredSecret("");
    setCredKeyError("");
    setCredDisplayNameError("");
  }

  async function handleSaveCredential() {
    let hasError = false;
    if (!credKey.trim()) {
      setCredKeyError("Key is required");
      hasError = true;
    }
    if (!credDisplayName.trim()) {
      setCredDisplayNameError("Display name is required");
      hasError = true;
    }
    if (hasError) return;

    setCredSaving(true);
    try {
      await credentialsApi.create({
        key: credKey.trim(),
        display_name: credDisplayName.trim(),
        credential_type: credType,
        secret: credSecret || undefined,
      });
      const refreshed = await credentialsApi.list();
      setCredentials(refreshed);
      setCredentialDrawerOpen(false);
      resetCredentialForm();
    } catch (err) {
      console.error("Save credential failed:", err);
    } finally {
      setCredSaving(false);
    }
  }

  async function handleDeleteCredential(id: string) {
    try {
      await credentialsApi.delete(id);
      const refreshed = await credentialsApi.list();
      setCredentials(refreshed);
      setPendingDeleteCredentialKey(null);
    } catch (err) {
      console.error("Delete credential failed:", err);
    }
  }

  // ── MCP Server handlers ───────────────────────────────────────────────────────
  function resetMcpForm() {
    setMcpName("");
    setMcpType("local");
    setMcpExecutable("");
    setMcpArgs("");
    setMcpUrl("");
    setMcpCredentialKey("");
    setMcpNameError("");
  }

  async function handleSaveMcpServer() {
    if (!mcpName.trim()) {
      setMcpNameError("Name is required");
      return;
    }
    setMcpSaving(true);
    try {
      let config: McpServerConfig;
      if (mcpType === "local") {
        config = {
          executable: mcpExecutable,
          args: mcpArgs.split(" ").filter(Boolean),
          env: {},
        };
      } else {
        config = {
          url: mcpUrl,
          ...(mcpCredentialKey ? { credential_key: mcpCredentialKey } : {}),
        };
      }
      await mcpServersApi.create({
        name: mcpName.trim(),
        server_type: mcpType,
        config,
      });
      const refreshed = await mcpServersApi.list();
      setMcpServers(refreshed.data);
      setMcpDrawerOpen(false);
      resetMcpForm();
    } catch (err) {
      console.error("Save MCP server failed:", err);
    } finally {
      setMcpSaving(false);
    }
  }

  async function handleToggleMcpServer(server: McpServer) {
    try {
      await mcpServersApi.update(server.id, { enabled: !server.enabled });
      setMcpServers((prev) =>
        prev.map((s) =>
          s.id === server.id ? { ...s, enabled: !server.enabled } : s,
        ),
      );
    } catch (err) {
      console.error("Toggle MCP server failed:", err);
    }
  }

  async function handleDeleteMcpServer(id: string) {
    try {
      await mcpServersApi.delete(id);
      const refreshed = await mcpServersApi.list();
      setMcpServers(refreshed.data);
      setPendingDeleteMcpId(null);
    } catch (err) {
      console.error("Delete MCP server failed:", err);
    }
  }

  // ── Persona handlers ──────────────────────────────────────────────────────────
  function resetPersonaForm() {
    setPersonaName("");
    setPersonaEmoji("");
    setPersonaSystemPrompt("");
    setPersonaNameError("");
    setEditingPersona(null);
  }

  async function handleSavePersona() {
    if (!personaName.trim()) {
      setPersonaNameError("Name is required");
      return;
    }
    setPersonaSaving(true);
    try {
      if (editingPersona === null) {
        await personasApi.create({
          name: personaName.trim(),
          emoji: personaEmoji,
          system_prompt: personaSystemPrompt,
        });
      } else {
        await personasApi.update(editingPersona.id, {
          name: personaName.trim(),
          emoji: personaEmoji,
          system_prompt: personaSystemPrompt,
        });
      }
      const refreshed = await personasApi.list();
      setPersonas(refreshed.data);
      setPersonaDrawerOpen(false);
      resetPersonaForm();
    } catch (err) {
      console.error("Save persona failed:", err);
    } finally {
      setPersonaSaving(false);
    }
  }

  async function handleDeletePersona(id: string) {
    try {
      await personasApi.delete(id);
      const refreshed = await personasApi.list();
      setPersonas(refreshed.data);
      setPendingDeletePersonaId(null);
    } catch (err) {
      console.error("Delete persona failed:", err);
    }
  }

  return (
    <div className={styles.container}>
      {/* ── Nav bar ── */}
      <div className={styles.navBar}>
        <h1 className={styles.navTitle}>Settings</h1>
      </div>

      {/* ── Scrollable body ── */}
      <div className={`${styles.body} scrollbar-thin`}>
        {/* ── Theme ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Theme</div>
          <div className={styles.sectionCard}>
            {/* Palette swatches row */}
            <div className={styles.row}>
              <div className={styles.rowTitle}>Palette</div>
              <div className={styles.paletteGrid}>
                {PALETTES.map((p) => {
                  const selected = palette === p.id;
                  return (
                    <button
                      key={p.id}
                      type="button"
                      className={[
                        styles.paletteSwatch,
                        selected ? styles.paletteSwatchActive : "",
                      ].join(" ")}
                      style={
                        { "--swatch-color": p.accent } as React.CSSProperties
                      }
                      onClick={() => setPalette(p.id)}
                      aria-label={p.label}
                      aria-pressed={selected}
                      title={p.label}
                    >
                      <span className={styles.paletteSwatchDot} />
                      <span className={styles.paletteSwatchLabel}>
                        {p.label}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
            {/* Appearance mode */}
            <div className={styles.row}>
              <div className={styles.rowTitle}>Appearance</div>
              <div className={styles.modeSegmented}>
                {(["system", "light", "dark"] as Mode[]).map((m) => (
                  <button
                    key={m}
                    type="button"
                    className={[
                      styles.modeBtn,
                      mode === m ? styles.modeBtnActive : "",
                    ].join(" ")}
                    onClick={() => setMode(m)}
                  >
                    {m.charAt(0).toUpperCase() + m.slice(1)}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </section>

        {/* ── Install as App ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Install as App</div>
          <div className={styles.sectionCard}>
            {steps !== null ? (
              steps.map((text, i) => (
                <div key={i} className={`${styles.row} ${styles.stepRow}`}>
                  <span className={styles.stepNum}>{i + 1}</span>
                  <span className={styles.stepText}>{text}</span>
                </div>
              ))
            ) : (
              <div className={styles.row}>
                <div className={styles.rowDesc}>
                  Open this page on your iOS or Android device to install
                  agent-deck as an app.
                </div>
              </div>
            )}
          </div>
        </section>

        {/* ── Notifications ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Notifications</div>
          <div className={styles.sectionCard}>
            <div className={styles.row}>
              {notifState !== "loading" && (
                <>
                  <div className={styles.statusRow}>
                    <span className={`${styles.statusDot} ${dotClass}`} />
                    <span className={styles.statusText}>{statusMessage}</span>
                  </div>
                  {notifState === "not-enabled" && (
                    <div style={{ marginTop: 12 }}>
                      <button
                        className={styles.enableBtn}
                        onClick={handleEnable}
                        disabled={notifLoading}
                      >
                        Enable Notifications
                      </button>
                    </div>
                  )}
                  {notifState === "enabled" && (
                    <div style={{ marginTop: 12 }}>
                      <button
                        className={styles.disableBtn}
                        onClick={handleDisable}
                        disabled={notifLoading}
                      >
                        Disable Notifications
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </section>

        {/* ── Providers ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Providers</div>
          <div className={styles.sectionCard}>
            {providers.map((provider) => (
              <div key={provider.id} className={styles.listRow}>
                <div className={styles.listRowLabel}>
                  <div className={styles.listRowName}>{provider.name}</div>
                  <div className={styles.listRowSub}>{provider.base_url}</div>
                </div>
                <input
                  type="checkbox"
                  checked={provider.enabled}
                  onChange={() => handleToggleProvider(provider)}
                />
                {pendingDeleteProviderId === provider.id ? (
                  <div className={styles.confirmRow}>
                    <span className={styles.confirmText}>Delete?</span>
                    <button
                      className={styles.confirmBtn}
                      onClick={() => handleDeleteProvider(provider.id)}
                    >
                      Confirm
                    </button>
                    <button
                      className={styles.cancelBtn}
                      onClick={() => setPendingDeleteProviderId(null)}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    className={styles.deleteButton}
                    onClick={() => setPendingDeleteProviderId(provider.id)}
                  >
                    ✕
                  </button>
                )}
              </div>
            ))}
            <button
              className={styles.addButton}
              onClick={() => {
                resetProviderForm();
                setProviderDrawerOpen(true);
              }}
            >
              + Add Provider
            </button>
          </div>
        </section>

        {/* ── Credentials ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Credentials</div>
          <div className={styles.sectionCard}>
            {credentials.map((credential) => (
              <div key={credential.id} className={styles.listRow}>
                <div className={styles.listRowLabel}>
                  <div className={styles.listRowName}>
                    {credential.display_name}
                  </div>
                  <div className={styles.listRowSub}>
                    {credential.key}
                    {credential.service && (
                      <span className={styles.badge}>{credential.service}</span>
                    )}
                  </div>
                </div>
                {pendingDeleteCredentialKey === credential.id ? (
                  <div className={styles.confirmRow}>
                    <span className={styles.confirmText}>Delete?</span>
                    <button
                      className={styles.confirmBtn}
                      onClick={() => handleDeleteCredential(credential.id)}
                    >
                      Confirm
                    </button>
                    <button
                      className={styles.cancelBtn}
                      onClick={() => setPendingDeleteCredentialKey(null)}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    className={styles.deleteButton}
                    onClick={() => setPendingDeleteCredentialKey(credential.id)}
                  >
                    ✕
                  </button>
                )}
              </div>
            ))}
            <button
              className={styles.addButton}
              onClick={() => {
                resetCredentialForm();
                setCredentialDrawerOpen(true);
              }}
            >
              + Add Credential
            </button>
          </div>
        </section>

        {/* ── MCP Servers ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>MCP Servers</div>
          <div className={styles.sectionCard}>
            {mcpServers.map((server) => (
              <div key={server.id} className={styles.listRow}>
                <span
                  className={`${styles.mcpStatusDot} ${mcpStatusDotClass(server.status)}`}
                />
                <div className={styles.listRowLabel}>
                  <div className={styles.listRowName}>
                    {server.name}
                    <span className={styles.badge}>{server.server_type}</span>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={server.enabled}
                  onChange={() => handleToggleMcpServer(server)}
                />
                {pendingDeleteMcpId === server.id ? (
                  <div className={styles.confirmRow}>
                    <span className={styles.confirmText}>Delete?</span>
                    <button
                      className={styles.confirmBtn}
                      onClick={() => handleDeleteMcpServer(server.id)}
                    >
                      Confirm
                    </button>
                    <button
                      className={styles.cancelBtn}
                      onClick={() => setPendingDeleteMcpId(null)}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    className={styles.deleteButton}
                    onClick={() => setPendingDeleteMcpId(server.id)}
                  >
                    ✕
                  </button>
                )}
              </div>
            ))}
            <button
              className={styles.addButton}
              onClick={() => {
                resetMcpForm();
                setMcpDrawerOpen(true);
              }}
            >
              + Add Server
            </button>
          </div>
        </section>

        {/* ── Personas ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Personas</div>
          <div className={styles.sectionCard}>
            {personas.map((persona) => (
              <div
                key={persona.id}
                className={styles.listRow}
                onClick={() => {
                  setEditingPersona(persona);
                  setPersonaDrawerOpen(true);
                }}
                style={{ cursor: "pointer" }}
              >
                <div className={styles.listRowLabel}>
                  <div className={styles.listRowName}>
                    {persona.emoji} {persona.name}
                  </div>
                </div>
                {pendingDeletePersonaId === persona.id ? (
                  <div
                    className={styles.confirmRow}
                    onClick={(e) => e.stopPropagation()}
                  >
                    <span className={styles.confirmText}>Delete?</span>
                    <button
                      className={styles.confirmBtn}
                      onClick={() => handleDeletePersona(persona.id)}
                    >
                      Confirm
                    </button>
                    <button
                      className={styles.cancelBtn}
                      onClick={() => setPendingDeletePersonaId(null)}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    className={styles.deleteButton}
                    disabled={persona.is_default}
                    title={
                      persona.is_default
                        ? "Cannot delete default persona"
                        : undefined
                    }
                    onClick={(e) => {
                      e.stopPropagation();
                      setPendingDeletePersonaId(persona.id);
                    }}
                  >
                    ✕
                  </button>
                )}
              </div>
            ))}
            <button
              className={styles.addButton}
              onClick={() => {
                resetPersonaForm();
                setPersonaDrawerOpen(true);
              }}
            >
              + Add Persona
            </button>
          </div>
        </section>
      </div>

      {/* ── Provider Drawer ── */}
      <div
        className={`${styles.drawer} ${providerDrawerOpen ? styles.drawerOpen : ""}`}
      >
        <div className={styles.drawerHeader}>
          <span className={styles.drawerTitle}>Add Provider</span>
          <button
            className={styles.drawerCloseBtn}
            onClick={() => {
              setProviderDrawerOpen(false);
              resetProviderForm();
            }}
          >
            ✕
          </button>
        </div>
        <div className={styles.drawerBody}>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Name</label>
            <input
              className={styles.formInput}
              type="text"
              value={providerName}
              onChange={(e) => {
                setProviderName(e.target.value);
                if (providerNameError) setProviderNameError("");
              }}
              placeholder="My Provider"
            />
            {providerNameError && (
              <div className={styles.formError}>{providerNameError}</div>
            )}
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Kind</label>
            <div className={styles.segmentedControl}>
              {(["copilot", "openai", "anthropic", "custom"] as const).map(
                (kind) => (
                  <button
                    key={kind}
                    className={`${styles.segmentedBtn} ${providerKind === kind ? styles.segmentedBtnActive : ""}`}
                    onClick={() => setProviderKind(kind)}
                  >
                    {kind.charAt(0).toUpperCase() + kind.slice(1)}
                  </button>
                ),
              )}
            </div>
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Base URL</label>
            <input
              className={styles.formInput}
              type="text"
              value={providerBaseUrl}
              onChange={(e) => setProviderBaseUrl(e.target.value)}
              placeholder="https://api.example.com"
            />
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>API Key</label>
            <input
              className={styles.formInput}
              type="password"
              value={providerApiKey}
              onChange={(e) => setProviderApiKey(e.target.value)}
              autoComplete="new-password"
            />
          </div>
        </div>
        <div className={styles.drawerFooter}>
          <button
            className={styles.primaryBtn}
            onClick={handleSaveProvider}
            disabled={providerSaving}
          >
            Save
          </button>
        </div>
      </div>

      {/* ── Credential Drawer ── */}
      <div
        className={`${styles.drawer} ${credentialDrawerOpen ? styles.drawerOpen : ""}`}
      >
        <div className={styles.drawerHeader}>
          <span className={styles.drawerTitle}>Add Credential</span>
          <button
            className={styles.drawerCloseBtn}
            onClick={() => {
              setCredentialDrawerOpen(false);
              resetCredentialForm();
            }}
          >
            ✕
          </button>
        </div>
        <div className={styles.drawerBody}>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Key</label>
            <input
              className={styles.formInput}
              type="text"
              value={credKey}
              onChange={(e) => {
                setCredKey(e.target.value);
                if (credKeyError) setCredKeyError("");
              }}
              placeholder="MY_API_KEY"
            />
            {credKeyError && (
              <div className={styles.formError}>{credKeyError}</div>
            )}
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Display Name</label>
            <input
              className={styles.formInput}
              type="text"
              value={credDisplayName}
              onChange={(e) => {
                setCredDisplayName(e.target.value);
                if (credDisplayNameError) setCredDisplayNameError("");
              }}
              placeholder="My API Key"
            />
            {credDisplayNameError && (
              <div className={styles.formError}>{credDisplayNameError}</div>
            )}
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Service (optional)</label>
            <input
              className={styles.formInput}
              type="text"
              value={credService}
              onChange={(e) => setCredService(e.target.value)}
              placeholder="e.g. github, openai"
            />
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Type</label>
            <select
              className={styles.formSelect}
              value={credType}
              onChange={(e) => setCredType(e.target.value as CredentialType)}
            >
              <option value="api_key">API Key</option>
              <option value="pat">Personal Access Token</option>
              <option value="bearer_token">Bearer Token</option>
            </select>
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Secret</label>
            <input
              className={styles.formInput}
              type="password"
              value={credSecret}
              onChange={(e) => setCredSecret(e.target.value)}
              autoComplete="new-password"
            />
          </div>
        </div>
        <div className={styles.drawerFooter}>
          <button
            className={styles.primaryBtn}
            onClick={handleSaveCredential}
            disabled={credSaving}
          >
            Save
          </button>
        </div>
      </div>

      {/* ── MCP Server Drawer ── */}
      <div
        className={`${styles.drawer} ${mcpDrawerOpen ? styles.drawerOpen : ""}`}
      >
        <div className={styles.drawerHeader}>
          <span className={styles.drawerTitle}>Add MCP Server</span>
          <button
            className={styles.drawerCloseBtn}
            onClick={() => {
              setMcpDrawerOpen(false);
              resetMcpForm();
            }}
          >
            ✕
          </button>
        </div>
        <div className={styles.drawerBody}>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Name</label>
            <input
              className={styles.formInput}
              type="text"
              value={mcpName}
              onChange={(e) => {
                setMcpName(e.target.value);
                if (mcpNameError) setMcpNameError("");
              }}
              placeholder="My MCP Server"
            />
            {mcpNameError && (
              <div className={styles.formError}>{mcpNameError}</div>
            )}
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Type</label>
            <div className={styles.segmentedControl}>
              <button
                className={`${styles.segmentedBtn} ${mcpType === "local" ? styles.segmentedBtnActive : ""}`}
                onClick={() => setMcpType("local")}
              >
                Local
              </button>
              <button
                className={`${styles.segmentedBtn} ${mcpType === "remote" ? styles.segmentedBtnActive : ""}`}
                onClick={() => setMcpType("remote")}
              >
                Remote
              </button>
            </div>
          </div>
          {mcpType === "local" ? (
            <>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>Executable</label>
                <input
                  className={styles.formInput}
                  type="text"
                  value={mcpExecutable}
                  onChange={(e) => setMcpExecutable(e.target.value)}
                  placeholder="e.g. /usr/local/bin/my-server"
                />
              </div>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>Args</label>
                <input
                  className={styles.formInput}
                  type="text"
                  value={mcpArgs}
                  onChange={(e) => setMcpArgs(e.target.value)}
                  placeholder="space-separated arguments"
                />
              </div>
            </>
          ) : (
            <>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>URL</label>
                <input
                  className={styles.formInput}
                  type="text"
                  value={mcpUrl}
                  onChange={(e) => setMcpUrl(e.target.value)}
                  placeholder="https://my-mcp-server.example.com"
                />
              </div>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>
                  Credential Key (optional)
                </label>
                <input
                  className={styles.formInput}
                  type="text"
                  value={mcpCredentialKey}
                  onChange={(e) => setMcpCredentialKey(e.target.value)}
                  placeholder="MY_CREDENTIAL_KEY"
                />
              </div>
            </>
          )}
        </div>
        <div className={styles.drawerFooter}>
          <button
            className={styles.primaryBtn}
            onClick={handleSaveMcpServer}
            disabled={mcpSaving}
          >
            Save
          </button>
        </div>
      </div>

      {/* ── Persona Drawer ── */}
      <div
        className={`${styles.drawer} ${personaDrawerOpen ? styles.drawerOpen : ""}`}
      >
        <div className={styles.drawerHeader}>
          <span className={styles.drawerTitle}>
            {editingPersona ? "Edit Persona" : "Add Persona"}
          </span>
          <button
            className={styles.drawerCloseBtn}
            onClick={() => {
              setPersonaDrawerOpen(false);
              resetPersonaForm();
            }}
          >
            ✕
          </button>
        </div>
        <div className={styles.drawerBody}>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Name</label>
            <input
              className={styles.formInput}
              type="text"
              value={personaName}
              onChange={(e) => {
                setPersonaName(e.target.value);
                if (personaNameError) setPersonaNameError("");
              }}
              placeholder="My Persona"
            />
            {personaNameError && (
              <div className={styles.formError}>{personaNameError}</div>
            )}
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Emoji</label>
            <input
              className={styles.formInput}
              type="text"
              value={personaEmoji}
              onChange={(e) => setPersonaEmoji(e.target.value)}
              placeholder="🤖"
              maxLength={2}
            />
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>System Prompt</label>
            <textarea
              className={styles.formTextarea}
              value={personaSystemPrompt}
              onChange={(e) => setPersonaSystemPrompt(e.target.value)}
              rows={6}
              placeholder="You are a helpful assistant..."
            />
          </div>
        </div>
        <div className={styles.drawerFooter}>
          <button
            className={styles.primaryBtn}
            onClick={handleSavePersona}
            disabled={personaSaving}
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

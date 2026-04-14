import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import QRCode from "qrcode";
import { authApi, profileApi } from "@/api/client";
import { TailscaleStatusCard } from "./TailscaleStatusCard";
import type { UserProfile } from "@/types";
import {
  Btn,
  FieldHint,
  FieldInput,
  FieldLabel,
  FieldTextarea,
} from "./shared";
import { View, ViewOff } from "@carbon/icons-react";
import { useThemeStore, type Palette, type Mode } from "@/stores/useThemeStore";

// ─── Section card wrapper ─────────────────────────────────────────────────────

function SectionCard({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "18px 20px",
        marginBottom: 16,
      }}
    >
      <div style={{ marginBottom: 16 }}>
        <div
          style={{
            fontSize: 14,
            fontWeight: 700,
            color: "var(--text-primary)",
            marginBottom: subtitle ? 3 : 0,
          }}
        >
          {title}
        </div>
        {subtitle && (
          <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            {subtitle}
          </div>
        )}
      </div>
      {children}
    </div>
  );
}

// ─── ProfileField ─────────────────────────────────────────────────────────────

function ProfileField({
  label,
  field,
  value: initialValue,
  placeholder,
  saving,
  onBlurSave,
}: {
  label: string;
  field: keyof UserProfile;
  value: string;
  placeholder?: string;
  saving: boolean;
  onBlurSave: (field: keyof UserProfile, value: string) => void;
}) {
  const [localValue, setLocalValue] = useState(initialValue);

  // Sync when profile loads (initialValue changes)
  useEffect(() => {
    setLocalValue(initialValue);
  }, [initialValue]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <FieldLabel>{label}</FieldLabel>
      <FieldInput
        type="text"
        value={localValue}
        placeholder={placeholder}
        onChange={(e) => setLocalValue(e.target.value)}
        onBlur={() => onBlurSave(field, localValue)}
        style={saving ? { opacity: 0.6 } : undefined}
      />
    </div>
  );
}

// ─── AboutField ───────────────────────────────────────────────────────────────

function AboutField({
  value: initialValue,
  saving,
  onBlurSave,
}: {
  value: string;
  saving: boolean;
  onBlurSave: (field: keyof UserProfile, value: string) => void;
}) {
  const [localValue, setLocalValue] = useState(initialValue);

  useEffect(() => {
    setLocalValue(initialValue);
  }, [initialValue]);

  const ABOUT_MAX = 500;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <FieldLabel>About</FieldLabel>
      <FieldTextarea
        value={localValue}
        placeholder="Background context shown to all your personas on every turn..."
        onChange={(e) => {
          if (e.target.value.length <= ABOUT_MAX) setLocalValue(e.target.value);
        }}
        onBlur={() => onBlurSave("about", localValue)}
        style={{ minHeight: 80, ...(saving ? { opacity: 0.6 } : {}) }}
      />
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <FieldHint>
          Shown to all your personas as background context. Be direct — this is
          read by the model on every turn.
        </FieldHint>
        <span
          style={{
            fontSize: 11,
            color:
              localValue.length >= ABOUT_MAX
                ? "var(--error)"
                : "var(--text-tertiary)",
            flexShrink: 0,
            marginLeft: 8,
          }}
        >
          {localValue.length} / {ABOUT_MAX}
        </span>
      </div>
    </div>
  );
}

// ─── Warning box ──────────────────────────────────────────────────────────────

function WarningBox({
  onConfirm,
  onCancel,
  rotating,
}: {
  onConfirm: () => void;
  onCancel: () => void;
  rotating: boolean;
}) {
  return (
    <div
      style={{
        background: "rgba(196,162,74,0.08)",
        border: "1px solid rgba(196,162,74,0.35)",
        borderRadius: 8,
        padding: "14px 16px",
        marginTop: 14,
      }}
    >
      <div
        style={{
          fontSize: 13,
          color: "var(--warning)",
          fontWeight: 600,
          marginBottom: 6,
        }}
      >
        ⚠ Warning
      </div>
      <div
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          lineHeight: 1.55,
          marginBottom: 10,
        }}
      >
        Rotating the token will immediately invalidate all existing sessions.
        You'll need to:
        <ul
          style={{
            margin: "6px 0 0",
            paddingLeft: 18,
            display: "flex",
            flexDirection: "column",
            gap: 4,
          }}
        >
          <li>
            Re-enter the new token on any remote browsers accessing via
            Tailscale
          </li>
          <li>Re-scan the QR code on the mobile app</li>
        </ul>
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <Btn variant="danger" sm onClick={onConfirm} disabled={rotating}>
          {rotating ? "Rotating…" : "Rotate Token"}
        </Btn>
        <Btn variant="ghost" sm onClick={onCancel} disabled={rotating}>
          Cancel
        </Btn>
      </div>
    </div>
  );
}

// ─── Toggle row ───────────────────────────────────────────────────────────────

function ToggleRow({
  title,
  description,
  value,
  onChange,
}: {
  title: string;
  description: string;
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 16,
        paddingBottom: 12,
        marginBottom: 12,
        borderBottom: "1px solid var(--border-subtle)",
      }}
    >
      <div>
        <div
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
            marginBottom: 2,
          }}
        >
          {title}
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          {description}
        </div>
      </div>
      {/* Toggle pill */}
      <button
        type="button"
        role="switch"
        aria-checked={value}
        onClick={() => onChange(!value)}
        style={{
          width: 36,
          height: 20,
          borderRadius: 10,
          border: "none",
          background: value ? "var(--accent-primary)" : "var(--bg-elevated)",
          cursor: "pointer",
          position: "relative",
          flexShrink: 0,
          transition: "background 0.2s",
          outline: "none",
        }}
      >
        <span
          style={{
            position: "absolute",
            top: 3,
            left: value ? 19 : 3,
            width: 14,
            height: 14,
            borderRadius: "50%",
            background: value ? "var(--text-inverse)" : "var(--text-tertiary)",
            transition: "left 0.2s, background 0.2s",
          }}
        />
      </button>
    </div>
  );
}

// ─── QR canvas ───────────────────────────────────────────────────────────────

function QrCanvas({ url }: { url: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!canvasRef.current) return;
    QRCode.toCanvas(canvasRef.current, url, {
      width: 200,
      color: {
        dark: "#F0EDE4",
        light: "#242422",
      },
    }).catch((err) => console.error("QR generation error:", err));
  }, [url]);

  return <canvas ref={canvasRef} width={200} height={200} />;
}

// ─── Server info row ──────────────────────────────────────────────────────────

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span style={{ fontSize: 13, color: "var(--text-tertiary)" }}>
        {label}
      </span>
      <span
        style={{
          fontSize: 13,
          color: "var(--text-primary)",
          fontFamily: '"SF Mono","Fira Code",monospace',
        }}
      >
        {value}
      </span>
    </>
  );
}

// ─── GeneralSettings ──────────────────────────────────────────────────────────

const PALETTES: {
  id: Palette;
  label: string;
  dark: string;
  light: string;
  accent: string;
}[] = [
  {
    id: "olive",
    label: "Olive",
    dark: "#1c1c1a",
    light: "#f5f3ef",
    accent: "#7c8c5a",
  },
  {
    id: "slate",
    label: "Slate",
    dark: "#0f1117",
    light: "#f6f8fa",
    accent: "#58a6ff",
  },
  {
    id: "midnight",
    label: "Midnight",
    dark: "#0d0d1a",
    light: "#f7f7fb",
    accent: "#8b7fd4",
  },
  {
    id: "rose",
    label: "Rose",
    dark: "#1a1218",
    light: "#fdf6f8",
    accent: "#c47a8a",
  },
  {
    id: "forest",
    label: "Forest",
    dark: "#0d1410",
    light: "#f4faf6",
    accent: "#4caf72",
  },
  {
    id: "ember",
    label: "Ember",
    dark: "#1a1208",
    light: "#fefaf4",
    accent: "#d4853a",
  },
  {
    id: "ocean",
    label: "Ocean",
    dark: "#080f18",
    light: "#f4fafd",
    accent: "#2ab8d0",
  },
  {
    id: "copper",
    label: "Copper",
    dark: "#181210",
    light: "#fdf8f4",
    accent: "#c07840",
  },
  {
    id: "sakura",
    label: "Sakura",
    dark: "#180f14",
    light: "#fff5f8",
    accent: "#e8709a",
  },
  {
    id: "noir",
    label: "Noir",
    dark: "#0a0a0a",
    light: "#ffffff",
    accent: "#e0e0e0",
  },
];

export function GeneralSettings() {
  // Token state
  const [token, setToken] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [showWarning, setShowWarning] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [rotateError, setRotateError] = useState<string | null>(null);

  // Server info state
  const [serverInfo, setServerInfo] = useState<{
    version: string;
    database_path: string;
  } | null>(null);

  // Profile state
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [profileSaving, setProfileSaving] = useState<string | null>(null); // field key being saved

  // Preferences — local state only for now (no persistence endpoint yet)
  const [showStreamingIndicator, setShowStreamingIndicator] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const [showRoutineLabels, setShowRoutineLabels] = useState(true);
  const [pushNotifications, setPushNotifications] = useState(false);

  const palette = useThemeStore((s) => s.palette);
  const mode = useThemeStore((s) => s.mode);
  const setPalette = useThemeStore((s) => s.setPalette);
  const setMode = useThemeStore((s) => s.setMode);
  const [themeOpen, setThemeOpen] = useState(true);

  // Auto-detect timezone for pre-fill
  const detectedTimezone = useMemo(() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return "";
    }
  }, []);

  // Load server info on mount
  const loadServerInfo = useCallback(async () => {
    try {
      const res = await authApi.getConfig();
      setServerInfo({
        version: res.data.version,
        database_path:
          (res.data as unknown as Record<string, string>).database_path ?? "",
      });
    } catch {
      // non-critical — server info card will just not render
    }
  }, []);

  // Load profile on mount
  const loadProfile = useCallback(async () => {
    try {
      const res = await profileApi.get();
      setProfile(res.data);
    } catch {
      // non-critical
    }
  }, []);

  useEffect(() => {
    loadServerInfo();
    loadProfile();
  }, [loadServerInfo, loadProfile]);

  // Save a profile field on blur
  const handleProfileBlur = async (field: keyof UserProfile, value: string) => {
    const finalValue = value.trim() || null;
    try {
      setProfileSaving(field);
      const res = await profileApi.update({ [field]: finalValue });
      setProfile(res.data);
    } catch {
      // ignore
    } finally {
      setProfileSaving(null);
    }
  };

  // Copy token to clipboard
  const handleCopy = async () => {
    if (!token) return;
    try {
      await navigator.clipboard.writeText(token);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard not available
    }
  };

  // Rotate token
  const handleRotateConfirm = async () => {
    setRotating(true);
    setRotateError(null);
    try {
      const res = await authApi.rotateToken();
      setToken(res.data.token);
      setRevealed(true); // show the new token immediately
      setShowWarning(false);
    } catch (e) {
      setRotateError(
        e instanceof Error ? e.message : "Failed to rotate token.",
      );
    } finally {
      setRotating(false);
    }
  };

  // Masked display value
  const displayValue = token
    ? revealed
      ? token
      : token.slice(0, 4) +
        "••••••••••••••••••••••••••••••••••••••••••••••••••••••••" +
        token.slice(-4)
    : "••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••";

  return (
    <div>
      {/* Section header */}
      <div style={{ marginBottom: 20 }}>
        <div
          style={{
            fontSize: 16,
            fontWeight: 700,
            color: "var(--text-primary)",
            marginBottom: 4,
          }}
        >
          General
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          Auth token management, preferences, and server information.
        </div>
      </div>

      {/* ── Theme ──────────────────────────────────────────────────────── */}
      <section style={{ marginBottom: 36 }}>
        <button
          type="button"
          onClick={() => setThemeOpen((o) => !o)}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            width: "100%",
            background: "none",
            border: "none",
            borderBottom: "1px solid var(--border-subtle)",
            paddingBottom: 8,
            marginBottom: themeOpen ? 16 : 0,
            cursor: "pointer",
            fontFamily: "inherit",
          }}
        >
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Theme
          </span>
          <span
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              transform: themeOpen ? "rotate(180deg)" : "rotate(0deg)",
              transition: "transform 0.2s",
              display: "inline-block",
            }}
          >
            ▾
          </span>
        </button>

        {themeOpen && (
          <>
            {/* Palette swatches */}
            <div style={{ marginBottom: 20 }}>
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  marginBottom: 10,
                  fontWeight: 500,
                }}
              >
                Palette
              </div>
              <div
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  gap: 10,
                }}
              >
                {PALETTES.map((p) => {
                  const selected = palette === p.id;
                  return (
                    <button
                      key={p.id}
                      type="button"
                      onClick={() => setPalette(p.id)}
                      title={p.label}
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        alignItems: "center",
                        gap: 5,
                        background: "none",
                        border: "none",
                        cursor: "pointer",
                        padding: 2,
                      }}
                    >
                      <div
                        style={{
                          width: 36,
                          height: 36,
                          borderRadius: 8,
                          background: p.accent,
                          boxShadow: selected
                            ? `0 0 0 2px var(--bg-primary), 0 0 0 4px ${p.accent}`
                            : "none",
                          transition: "box-shadow 0.15s",
                        }}
                      />
                      <span
                        style={{
                          fontSize: 10,
                          color: selected
                            ? "var(--text-primary)"
                            : "var(--text-tertiary)",
                          transition: "color 0.15s",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {p.label}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Mode toggle */}
            <div>
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  marginBottom: 10,
                  fontWeight: 500,
                }}
              >
                Appearance
              </div>
              <div
                style={{
                  display: "inline-flex",
                  borderRadius: 8,
                  overflow: "hidden",
                  border: "1px solid var(--border-default)",
                }}
              >
                {(["system", "light", "dark"] as Mode[]).map((m, i) => (
                  <button
                    key={m}
                    type="button"
                    onClick={() => setMode(m)}
                    style={{
                      padding: "7px 18px",
                      fontSize: 12,
                      fontWeight: 500,
                      border: "none",
                      borderLeft:
                        i > 0 ? "1px solid var(--border-default)" : "none",
                      background:
                        mode === m
                          ? "var(--accent-primary)"
                          : "var(--bg-tertiary)",
                      color:
                        mode === m
                          ? "var(--text-inverse)"
                          : "var(--text-secondary)",
                      cursor: "pointer",
                      transition: "background 0.15s, color 0.15s",
                      fontFamily: "inherit",
                    }}
                  >
                    {m.charAt(0).toUpperCase() + m.slice(1)}
                  </button>
                ))}
              </div>
            </div>
          </>
        )}
      </section>

      {/* ── Profile card ── */}
      <SectionCard
        title="Profile"
        subtitle="Shared with all your personas as background context"
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {/* display_name */}
          <ProfileField
            label="Name"
            field="display_name"
            value={profile?.display_name ?? ""}
            saving={profileSaving === "display_name"}
            onBlurSave={handleProfileBlur}
          />
          {/* pronouns */}
          <ProfileField
            label="Pronouns"
            field="pronouns"
            value={profile?.pronouns ?? ""}
            placeholder="e.g. she/her"
            saving={profileSaving === "pronouns"}
            onBlurSave={handleProfileBlur}
          />
          {/* role */}
          <ProfileField
            label="Role"
            field="role"
            value={profile?.role ?? ""}
            placeholder="e.g. Senior Software Engineer"
            saving={profileSaving === "role"}
            onBlurSave={handleProfileBlur}
          />
          {/* organization */}
          <ProfileField
            label="Organization"
            field="organization"
            value={profile?.organization ?? ""}
            placeholder="e.g. Acme Corp"
            saving={profileSaving === "organization"}
            onBlurSave={handleProfileBlur}
          />
          {/* location */}
          <ProfileField
            label="Location"
            field="location"
            value={profile?.location ?? ""}
            placeholder="e.g. San Francisco, CA"
            saving={profileSaving === "location"}
            onBlurSave={handleProfileBlur}
          />
          {/* timezone */}
          <ProfileField
            label="Timezone"
            field="timezone"
            value={profile?.timezone ?? detectedTimezone}
            placeholder="e.g. America/Los_Angeles"
            saving={profileSaving === "timezone"}
            onBlurSave={handleProfileBlur}
          />
          {/* about — textarea with counter */}
          <AboutField
            value={profile?.about ?? ""}
            saving={profileSaving === "about"}
            onBlurSave={handleProfileBlur}
          />
        </div>
      </SectionCard>

      {/* ── Auth Token card ── */}
      <SectionCard
        title="Auth Token"
        subtitle="Used to authenticate remote devices over Tailscale"
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <FieldLabel>Current Token</FieldLabel>

          {/* Token field + actions */}
          <div
            style={{
              display: "flex",
              gap: 6,
              alignItems: "center",
            }}
          >
            <input
              type={revealed ? "text" : "password"}
              readOnly
              value={displayValue}
              style={{
                flex: 1,
                background: "var(--bg-elevated)",
                border: "1px solid var(--border-default)",
                borderRadius: 7,
                padding: "9px 12px",
                color: "var(--text-primary)",
                fontSize: 12,
                fontFamily: '"SF Mono","Fira Code",monospace',
                letterSpacing: revealed ? "0.03em" : "0.12em",
                outline: "none",
                minWidth: 0,
              }}
            />
            {token && (
              <Btn
                variant="ghost"
                sm
                onClick={() => setRevealed((r) => !r)}
                title={revealed ? "Hide token" : "Reveal token"}
                style={{ flexShrink: 0 }}
              >
                {revealed ? <ViewOff size={14} /> : <View size={14} />}
              </Btn>
            )}
            {token && (
              <Btn
                variant="ghost"
                sm
                onClick={handleCopy}
                style={{ flexShrink: 0 }}
              >
                {copied ? "✓ Copied" : "Copy"}
              </Btn>
            )}
          </div>

          <FieldHint>
            This token is printed to the server terminal on startup. Copy it to
            authenticate from remote browsers or the mobile app.
          </FieldHint>
        </div>

        {/* Rotate warning / confirm */}
        {showWarning && (
          <WarningBox
            onConfirm={handleRotateConfirm}
            onCancel={() => {
              setShowWarning(false);
              setRotateError(null);
            }}
            rotating={rotating}
          />
        )}

        {/* Rotate error */}
        {rotateError && (
          <div
            style={{
              marginTop: 10,
              padding: "8px 12px",
              borderRadius: 7,
              background: "rgba(196,90,90,0.1)",
              border: "1px solid rgba(196,90,90,0.25)",
              color: "var(--error)",
              fontSize: 12,
            }}
          >
            {rotateError}
          </div>
        )}

        {/* Rotate button — hidden while warning is showing */}
        {!showWarning && (
          <div style={{ marginTop: 14 }}>
            <Btn variant="ghost" onClick={() => setShowWarning(true)}>
              ⟳ Rotate Token
            </Btn>
          </div>
        )}
      </SectionCard>

      {/* ── Preferences card ── */}
      <SectionCard title="Preferences">
        <ToggleRow
          title="Show streaming indicator"
          description="Display animated dots while the agent is generating a response"
          value={showStreamingIndicator}
          onChange={setShowStreamingIndicator}
        />
        <ToggleRow
          title="Auto-scroll to new messages"
          description="Automatically scroll down when new messages arrive"
          value={autoScroll}
          onChange={setAutoScroll}
        />
        <ToggleRow
          title="Show routine message labels"
          description='Display a "Routine" label on messages generated by scheduled routines'
          value={showRoutineLabels}
          onChange={setShowRoutineLabels}
        />
        {/* Last row — no bottom border */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 16,
          }}
        >
          <div>
            <div
              style={{
                fontSize: 13,
                fontWeight: 500,
                color: "var(--text-primary)",
                marginBottom: 2,
              }}
            >
              Push notifications (Android)
            </div>
            <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              Send FCM push notifications when the mobile app is in the
              background
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={pushNotifications}
            onClick={() => setPushNotifications((v) => !v)}
            style={{
              width: 36,
              height: 20,
              borderRadius: 10,
              border: "none",
              background: pushNotifications
                ? "var(--accent-primary)"
                : "var(--bg-elevated)",
              cursor: "pointer",
              position: "relative",
              flexShrink: 0,
              transition: "background 0.2s",
              outline: "none",
            }}
          >
            <span
              style={{
                position: "absolute",
                top: 3,
                left: pushNotifications ? 19 : 3,
                width: 14,
                height: 14,
                borderRadius: "50%",
                background: pushNotifications
                  ? "var(--text-inverse)"
                  : "var(--text-tertiary)",
                transition: "left 0.2s, background 0.2s",
              }}
            />
          </button>
        </div>
      </SectionCard>

      {/* ── Server Info card ── */}
      {serverInfo && (
        <SectionCard title="Server Info">
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "120px 1fr",
              gap: "8px 16px",
              alignItems: "center",
            }}
          >
            <InfoRow label="Version" value={serverInfo.version} />
            <InfoRow label="Database" value={serverInfo.database_path} />
          </div>
        </SectionCard>
      )}

      {/* ── Tailscale card ── */}
      <TailscaleStatusCard />

      {/* ── Open on Phone card ── */}
      <SectionCard
        title="Open on Phone"
        subtitle="Scan to open agent-deck in your phone browser. Make sure Tailscale is running on both devices first."
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 8,
          }}
        >
          <QrCanvas url={window.location.origin} />
          <span
            style={{
              fontSize: 11,
              fontFamily: '"SF Mono","Fira Code",monospace',
              color: "var(--text-tertiary)",
              wordBreak: "break-all",
              marginTop: 10,
            }}
          >
            {window.location.origin}
          </span>
          <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            Once on your phone, go through Share → Add to Home Screen (iOS) or
            menu → Install app (Android) to install as a PWA.
          </span>
        </div>
      </SectionCard>
    </div>
  );
}

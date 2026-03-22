import React, { useCallback, useEffect, useState } from "react";
import { authApi } from "@/api/client";
import { Btn, FieldHint, FieldLabel } from "./shared";

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

  // Preferences — local state only for now (no persistence endpoint yet)
  const [showStreamingIndicator, setShowStreamingIndicator] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const [showRoutineLabels, setShowRoutineLabels] = useState(true);
  const [pushNotifications, setPushNotifications] = useState(false);

  // Load server info on mount
  const loadServerInfo = useCallback(async () => {
    try {
      const res = await authApi.getConfig();
      setServerInfo({
        version: res.data.version,
        database_path: res.data.database_path,
      });
    } catch {
      // non-critical — server info card will just not render
    }
  }, []);

  useEffect(() => {
    loadServerInfo();
  }, [loadServerInfo]);

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
                {revealed ? "🙈" : "👁"}
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
    </div>
  );
}

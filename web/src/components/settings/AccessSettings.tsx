import React, { useCallback, useEffect, useState } from "react";
import { View, ViewOff } from "@carbon/icons-react";
import { authApi } from "@/api/client";
import { Btn, FieldHint, FieldLabel } from "./shared";

// ─── SectionCard ─────────────────────────────────────────────────────────────

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
        padding: "16px 18px",
        marginBottom: 16,
      }}
    >
      <div style={{ marginBottom: subtitle ? 4 : 14 }}>
        <div
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
            marginBottom: subtitle ? 2 : 0,
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

// ─── WarningBox ───────────────────────────────────────────────────────────────

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
        background: "rgba(196,90,90,0.08)",
        border: "1px solid rgba(196,90,90,0.25)",
        borderRadius: 8,
        padding: "12px 14px",
        marginTop: 14,
      }}
    >
      <div
        style={{
          fontSize: 13,
          color: "var(--error)",
          fontWeight: 600,
          marginBottom: 6,
        }}
      >
        Rotate auth token?
      </div>
      <div
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          lineHeight: 1.55,
          marginBottom: 12,
        }}
      >
        All existing sessions and saved tokens on remote devices will be
        invalidated immediately. You will need to re-authenticate everywhere.
      </div>
      <ul
        style={{
          margin: "0 0 12px",
          paddingLeft: 16,
          display: "flex",
          flexDirection: "column",
          gap: 4,
        }}
      >
        {[
          "Mobile browsers using a saved token",
          "Any other devices you have logged in",
        ].map((item) => (
          <li
            key={item}
            style={{ fontSize: 12, color: "var(--text-secondary)" }}
          >
            {item}
          </li>
        ))}
      </ul>
      <div style={{ display: "flex", gap: 8 }}>
        <Btn variant="danger" sm onClick={onConfirm} disabled={rotating}>
          {rotating ? "Rotating…" : "Yes, rotate it"}
        </Btn>
        <Btn variant="ghost" sm onClick={onCancel} disabled={rotating}>
          Cancel
        </Btn>
      </div>
    </div>
  );
}

// ─── InfoRow ──────────────────────────────────────────────────────────────────

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
        {label}
      </span>
      <span
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          fontFamily: '"SF Mono","Fira Code",monospace',
        }}
      >
        {value}
      </span>
    </>
  );
}

// ─── AccessSettings ───────────────────────────────────────────────────────────

export function AccessSettings() {
  const [token, setToken] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [showWarning, setShowWarning] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [rotateError, setRotateError] = useState<string | null>(null);
  const [serverInfo, setServerInfo] = useState<{
    version: string;
    database_path: string;
  } | null>(null);

  const loadServerInfo = useCallback(async () => {
    try {
      const res = await authApi.getConfig();
      setServerInfo({
        version: res.data.version,
        database_path:
          (res.data as unknown as Record<string, string>).database_path ?? "",
      });
    } catch {
      // non-critical
    }
  }, []);

  useEffect(() => {
    loadServerInfo();
    const stored = authApi.getToken();
    if (stored) setToken(stored);
  }, [loadServerInfo]);

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

  const handleRotateConfirm = async () => {
    setRotating(true);
    setRotateError(null);
    try {
      const res = await authApi.rotateToken();
      setToken(res.data.token);
      setRevealed(true);
      setShowWarning(false);
    } catch (e) {
      setRotateError(
        e instanceof Error ? e.message : "Failed to rotate token.",
      );
    } finally {
      setRotating(false);
    }
  };

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
          Access
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          Auth token management and server information.
        </div>
      </div>

      {/* ── Auth Token ── */}
      <SectionCard
        title="Auth Token"
        subtitle="Used to authenticate remote devices over Tailscale"
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <FieldLabel>Current Token</FieldLabel>

          <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
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

        {!showWarning && (
          <div style={{ marginTop: 14 }}>
            <Btn variant="ghost" onClick={() => setShowWarning(true)}>
              ⟳ Rotate Token
            </Btn>
          </div>
        )}
      </SectionCard>

      {/* ── Server Info ── */}
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

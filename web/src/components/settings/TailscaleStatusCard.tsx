import React, { useCallback, useEffect, useState } from "react";
import { tailscaleApi } from "@/api/client";
import type { TailscaleStatus } from "@/types";

interface TailscaleStatusCardProps {
  /** Called when the user clicks [Enable Funnel] or [Disable Funnel] */
  onFunnelToggle?: () => void;
}

const DOT_GREEN = "#4caf7d";
const DOT_AMBER = "#f5a623";
const DOT_RED = "#e05252";

function StatusDot({ color }: { color: string }) {
  return (
    <span
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: color,
        flexShrink: 0,
        marginTop: 1,
      }}
    />
  );
}

function ActionButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      style={{
        background: "var(--accent-primary)",
        border: "none",
        borderRadius: 6,
        padding: "6px 12px",
        color: "var(--text-inverse, #fff)",
        fontSize: 12,
        fontWeight: 500,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.6 : 1,
        transition: "opacity 0.15s",
      }}
    >
      {children}
    </button>
  );
}

function GhostButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      style={{
        background: "transparent",
        border: "1px solid var(--border-default)",
        borderRadius: 6,
        padding: "6px 12px",
        color: "var(--text-secondary)",
        fontSize: 12,
        fontWeight: 500,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.6 : 1,
        transition: "opacity 0.15s",
      }}
    >
      {children}
    </button>
  );
}

export function TailscaleStatusCard({ onFunnelToggle }: TailscaleStatusCardProps) {
  const [status, setStatus] = useState<TailscaleStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const fetchStatus = useCallback(async () => {
    try {
      const result = await tailscaleApi.getStatus();
      setStatus(result.data);
      if (result.data.auth_url) {
        setAuthUrl(result.data.auth_url);
      }
    } catch {
      // leave previous status in place
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  const handleRefresh = useCallback(async () => {
    setLoading(true);
    await fetchStatus();
  }, [fetchStatus]);

  const handleConnect = useCallback(async () => {
    setActionLoading(true);
    try {
      const result = await tailscaleApi.connect();
      setStatus(result.data);
      if (result.data.auth_url) {
        setAuthUrl(result.data.auth_url);
      }
    } catch {
      // leave status unchanged
    } finally {
      setActionLoading(false);
    }
  }, []);

  const handleEnableFunnel = useCallback(async () => {
    setActionLoading(true);
    try {
      const result = await tailscaleApi.enableFunnel();
      setStatus(result.data);
      onFunnelToggle?.();
    } catch {
      // leave status unchanged
    } finally {
      setActionLoading(false);
    }
  }, [onFunnelToggle]);

  const handleDisableFunnel = useCallback(async () => {
    setActionLoading(true);
    try {
      const result = await tailscaleApi.disableFunnel();
      setStatus(result.data);
      onFunnelToggle?.();
    } catch {
      // leave status unchanged
    } finally {
      setActionLoading(false);
    }
  }, [onFunnelToggle]);

  const handleCopyWebhookUrl = useCallback(async () => {
    if (!status?.funnel_url) return;
    const webhookUrl = `${status.funnel_url}/api/webhooks`;
    try {
      await navigator.clipboard.writeText(webhookUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard unavailable
    }
  }, [status?.funnel_url]);

  const cardStyle: React.CSSProperties = {
    background: "var(--bg-tertiary)",
    border: "1px solid var(--border-subtle)",
    borderRadius: 10,
    padding: "14px 16px",
    marginBottom: 16,
  };

  const headerStyle: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    marginBottom: loading || !status ? 0 : 12,
  };

  const titleStyle: React.CSSProperties = {
    fontSize: 13,
    fontWeight: 600,
    color: "var(--text-primary)",
  };

  const refreshBtnStyle: React.CSSProperties = {
    background: "transparent",
    border: "none",
    color: "var(--text-tertiary)",
    fontSize: 12,
    cursor: "pointer",
    padding: "2px 6px",
    borderRadius: 4,
  };

  if (loading) {
    return (
      <div style={cardStyle}>
        <div style={headerStyle}>
          <span style={titleStyle}>Tailscale</span>
          <button type="button" style={refreshBtnStyle} onClick={handleRefresh}>
            Refresh ⟳
          </button>
        </div>
        {/* Loading skeleton */}
        <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 6 }}>
          <div
            style={{
              height: 12,
              width: "60%",
              borderRadius: 4,
              background: "var(--border-subtle)",
              animation: "pulse 1.5s ease-in-out infinite",
            }}
          />
          <div
            style={{
              height: 10,
              width: "80%",
              borderRadius: 4,
              background: "var(--border-subtle)",
              animation: "pulse 1.5s ease-in-out infinite",
            }}
          />
        </div>
      </div>
    );
  }

  if (!status || !status.installed) {
    return (
      <div style={cardStyle}>
        <div style={headerStyle}>
          <span style={titleStyle}>Tailscale</span>
          <button type="button" style={refreshBtnStyle} onClick={handleRefresh}>
            Refresh ⟳
          </button>
        </div>
        <div style={{ display: "flex", alignItems: "flex-start", gap: 8, marginTop: 4 }}>
          <StatusDot color={DOT_AMBER} />
          <div>
            <div style={{ fontSize: 13, fontWeight: 500, color: "var(--text-primary)" }}>
              Tailscale not found
            </div>
            <div style={{ fontSize: 12, color: "var(--text-tertiary)", marginTop: 3, lineHeight: 1.5 }}>
              Install via the agent-deck install script, or visit{" "}
              <a
                href="https://tailscale.com"
                target="_blank"
                rel="noopener noreferrer"
                style={{ color: "var(--accent-primary)" }}
              >
                tailscale.com
              </a>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (!status.connected) {
    return (
      <div style={cardStyle}>
        <div style={headerStyle}>
          <span style={titleStyle}>Tailscale</span>
          <button type="button" style={refreshBtnStyle} onClick={handleRefresh}>
            Refresh ⟳
          </button>
        </div>
        <div style={{ display: "flex", alignItems: "flex-start", gap: 8, marginBottom: 12 }}>
          <StatusDot color={DOT_RED} />
          <div style={{ fontSize: 13, fontWeight: 500, color: "var(--text-primary)" }}>
            Not connected
          </div>
        </div>
        <ActionButton onClick={handleConnect} disabled={actionLoading}>
          {actionLoading ? "Connecting…" : "Connect to Tailscale"}
        </ActionButton>
        {authUrl && (
          <div style={{ marginTop: 10 }}>
            <div style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 4 }}>
              Open this link to authenticate:
            </div>
            <a
              href={authUrl}
              target="_blank"
              rel="noopener noreferrer"
              style={{
                fontSize: 12,
                color: "var(--accent-primary)",
                wordBreak: "break-all",
              }}
            >
              {authUrl}
            </a>
          </div>
        )}
      </div>
    );
  }

  // Connected
  const webhookUrl = status.funnel_url ? `${status.funnel_url}/api/webhooks` : null;

  return (
    <div style={cardStyle}>
      <div style={headerStyle}>
        <span style={titleStyle}>Tailscale</span>
        <button type="button" style={refreshBtnStyle} onClick={handleRefresh}>
          Refresh ⟳
        </button>
      </div>

      {/* Connected status row */}
      <div style={{ display: "flex", alignItems: "flex-start", gap: 8, marginBottom: 8 }}>
        <StatusDot color={DOT_GREEN} />
        <div>
          <div style={{ fontSize: 13, fontWeight: 500, color: "var(--text-primary)" }}>
            Connected
          </div>
          {(status.hostname || status.ip_address) && (
            <div style={{ fontSize: 12, color: "var(--text-tertiary)", marginTop: 2 }}>
              {[status.hostname, status.ip_address].filter(Boolean).join(" · ")}
            </div>
          )}
        </div>
      </div>

      {/* Funnel section */}
      {status.funnel_enabled ? (
        <div style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 6 }}>
            Funnel:{" "}
            <span style={{ color: DOT_GREEN, fontWeight: 500 }}>enabled</span>
          </div>
          {webhookUrl && (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                background: "var(--bg-elevated, var(--bg-secondary))",
                border: "1px solid var(--border-subtle)",
                borderRadius: 6,
                padding: "6px 10px",
                marginBottom: 10,
              }}
            >
              <span
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  fontFamily: '"SF Mono","Fira Code",monospace',
                  flex: 1,
                  wordBreak: "break-all",
                }}
              >
                {webhookUrl}
              </span>
              <button
                type="button"
                onClick={handleCopyWebhookUrl}
                style={{
                  background: "transparent",
                  border: "1px solid var(--border-default)",
                  borderRadius: 4,
                  padding: "3px 8px",
                  color: "var(--text-secondary)",
                  fontSize: 11,
                  cursor: "pointer",
                  flexShrink: 0,
                }}
              >
                {copied ? "Copied!" : "Copy"}
              </button>
            </div>
          )}
          <GhostButton onClick={handleDisableFunnel} disabled={actionLoading}>
            {actionLoading ? "Disabling…" : "Disable Funnel"}
          </GhostButton>
        </div>
      ) : (
        <div style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 12, color: "var(--text-tertiary)", marginBottom: 8 }}>
            Funnel: not enabled —{" "}
            <span style={{ color: DOT_AMBER }}>webhooks will not work</span>
          </div>
          <ActionButton onClick={handleEnableFunnel} disabled={actionLoading}>
            {actionLoading ? "Enabling…" : "Enable Funnel"}
          </ActionButton>
        </div>
      )}
    </div>
  );
}

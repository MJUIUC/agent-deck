import React, { useCallback, useEffect, useRef, useState } from "react";
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

export function TailscaleStatusCard({
  onFunnelToggle,
}: TailscaleStatusCardProps) {
  const [status, setStatus] = useState<TailscaleStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pollingRef = useRef<ReturnType<typeof setInterval> | undefined>(
    undefined,
  );

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

  useEffect(() => {
    if (authUrl && !status?.connected) {
      pollingRef.current = setInterval(async () => {
        try {
          const result = await tailscaleApi.getStatus();
          setStatus(result.data);
          if (result.data.connected) {
            setAuthUrl(null);
            clearInterval(pollingRef.current);
          }
        } catch {
          // keep polling
        }
      }, 4000);
    } else {
      clearInterval(pollingRef.current);
    }
    return () => clearInterval(pollingRef.current);
  }, [authUrl, status?.connected]);

  const handleRefresh = useCallback(async () => {
    setLoading(true);
    await fetchStatus();
  }, [fetchStatus]);

  const handleConnect = useCallback(async () => {
    setActionLoading(true);
    setError(null);
    try {
      const result = await tailscaleApi.connect();
      setStatus(result.data);
      if (result.data.auth_url) {
        setAuthUrl(result.data.auth_url);
      } else if (!result.data.connected && result.data.message) {
        setError(result.data.message);
      }
    } catch {
      setError("Could not reach the server. Check the logs.");
    } finally {
      setActionLoading(false);
    }
  }, []);

  const handleEnableFunnel = useCallback(async () => {
    setActionLoading(true);
    setError(null);
    try {
      const result = await tailscaleApi.enableFunnel();
      setStatus(result.data);
      onFunnelToggle?.();
    } catch {
      setError("Failed to update Tailscale Funnel. Check the logs.");
    } finally {
      setActionLoading(false);
    }
  }, [onFunnelToggle]);

  const handleDisableFunnel = useCallback(async () => {
    setActionLoading(true);
    setError(null);
    try {
      const result = await tailscaleApi.disableFunnel();
      setStatus(result.data);
      onFunnelToggle?.();
    } catch {
      setError("Failed to update Tailscale Funnel. Check the logs.");
    } finally {
      setActionLoading(false);
    }
  }, [onFunnelToggle]);

  const handleStartServe = useCallback(async () => {
    setActionLoading(true);
    setError(null);
    try {
      const result = await tailscaleApi.startServe();
      setStatus(result.data);
    } catch {
      setError("Failed to start serving. Check the logs.");
    } finally {
      setActionLoading(false);
    }
  }, []);

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
        <div
          style={{
            marginTop: 10,
            display: "flex",
            flexDirection: "column",
            gap: 6,
          }}
        >
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
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: 8,
            marginTop: 4,
          }}
        >
          <StatusDot color={DOT_AMBER} />
          <div>
            <div
              style={{
                fontSize: 13,
                fontWeight: 500,
                color: "var(--text-primary)",
              }}
            >
              Tailscale not found
            </div>
            <div
              style={{
                fontSize: 12,
                color: "var(--text-tertiary)",
                marginTop: 3,
                lineHeight: 1.5,
              }}
            >
              Install via the agent-deck install script, or visit{" "}
              <a
                href="https://tailscale.com/download"
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
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: 8,
            marginBottom: 12,
          }}
        >
          <StatusDot color={DOT_RED} />
          <div
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: "var(--text-primary)",
            }}
          >
            Not connected
          </div>
        </div>
        <ActionButton onClick={handleConnect} disabled={actionLoading}>
          {actionLoading ? "Connecting…" : "Connect to Tailscale"}
        </ActionButton>
        {error && (
          <div style={{ marginTop: 8, fontSize: 12, color: "#e05252" }}>
            {error}
          </div>
        )}
        {authUrl && (
          <div
            style={{
              marginTop: 12,
              padding: "10px 12px",
              background: "var(--bg-secondary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 8,
            }}
          >
            <div
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "var(--text-primary)",
                marginBottom: 4,
              }}
            >
              Authentication required
            </div>
            <div
              style={{
                fontSize: 12,
                color: "var(--text-secondary)",
                marginBottom: 8,
                lineHeight: 1.5,
              }}
            >
              Open the link below in your browser to sign in to Tailscale. This
              card will update automatically once connected.
            </div>
            <a
              href={authUrl}
              target="_blank"
              rel="noopener noreferrer"
              style={{
                display: "inline-block",
                fontSize: 12,
                color: "var(--accent-primary)",
                fontWeight: 500,
                wordBreak: "break-all",
              }}
            >
              Open Tailscale login →
            </a>
          </div>
        )}
      </div>
    );
  }

  // Connected
  const webhookUrl = status.funnel_url
    ? `${status.funnel_url}/api/webhooks`
    : null;

  return (
    <div style={cardStyle}>
      <div style={headerStyle}>
        <span style={titleStyle}>Tailscale</span>
        <button type="button" style={refreshBtnStyle} onClick={handleRefresh}>
          Refresh ⟳
        </button>
      </div>

      {/* Serve status row */}
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: 8,
          marginBottom: 8,
        }}
      >
        <StatusDot color={status.serving ? DOT_GREEN : DOT_AMBER} />
        <div>
          <div
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: "var(--text-primary)",
            }}
          >
            {status.serving ? "Serving" : "Connected — not serving"}
          </div>
          {status.serving && status.serve_url && (
            <div
              style={{
                fontSize: 12,
                color: "var(--text-tertiary)",
                marginTop: 2,
              }}
            >
              {status.serve_url}
            </div>
          )}
        </div>
      </div>
      {!status.serving && (
        <div style={{ marginBottom: 8 }}>
          <ActionButton onClick={handleStartServe} disabled={actionLoading}>
            {actionLoading ? "Starting…" : "Start Serving"}
          </ActionButton>
        </div>
      )}
      {error && (
        <div style={{ marginTop: 8, fontSize: 12, color: "#e05252" }}>
          {error}
        </div>
      )}

      {/* Tailscale connection info row */}
      {status.serving && (status.hostname || status.ip_address) && (
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: 8,
            marginTop: 8,
            marginBottom: 12,
          }}
        >
          <StatusDot color={DOT_GREEN} />
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              marginTop: 1,
            }}
          >
            {[status.hostname, status.ip_address].filter(Boolean).join(" · ")}
          </div>
        </div>
      )}

      {/* Funnel section */}
      {status.funnel_enabled ? (
        <div style={{ marginBottom: 10 }}>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              marginBottom: 6,
            }}
          >
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
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              marginBottom: 8,
            }}
          >
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

import React, { useCallback, useEffect, useRef, useState } from "react";
import { tailscaleApi } from "@/api/client";
import type { TailscaleStatus } from "@/types";
import { WizardNavRow } from "../shared/WizardNavRow";

interface StepTailscaleProps {
  onNext: () => void;
  onSkip: () => void;
}

const DOT_GREEN = "#4caf7d";
const DOT_AMBER = "#f5a623";
const DOT_RED = "#e05252";

function StatusDot({ color }: { color: string }) {
  return (
    <span
      style={{
        display: "inline-block",
        width: 12,
        height: 12,
        borderRadius: "50%",
        background: color,
        flexShrink: 0,
        marginTop: 2,
      }}
    />
  );
}

function AnimatedEllipsis() {
  const [dots, setDots] = useState(".");

  useEffect(() => {
    const timer = setInterval(() => {
      setDots((prev) => {
        if (prev === "...") return ".";
        return prev + ".";
      });
    }, 500);
    return () => clearInterval(timer);
  }, []);

  return (
    <span style={{ display: "inline-block", minWidth: 16 }}>{dots}</span>
  );
}

export function StepTailscale({ onNext, onSkip }: StepTailscaleProps) {
  const [status, setStatus] = useState<TailscaleStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [connectLoading, setConnectLoading] = useState(false);
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const autoAdvanceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const result = await tailscaleApi.getStatus();
      setStatus(result.data);
      if (result.data.auth_url) {
        setAuthUrl(result.data.auth_url);
      }
      return result.data;
    } catch {
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const stopPolling = useCallback(() => {
    if (pollIntervalRef.current !== null) {
      clearInterval(pollIntervalRef.current);
      pollIntervalRef.current = null;
    }
  }, []);

  const startPolling = useCallback(() => {
    stopPolling();
    pollIntervalRef.current = setInterval(async () => {
      const result = await fetchStatus();
      if (result?.connected) {
        stopPolling();
      }
    }, 3000);
  }, [fetchStatus, stopPolling]);

  // On mount: fetch and start polling
  useEffect(() => {
    const initialize = async () => {
      const result = await fetchStatus();
      if (result?.connected) {
        // Already connected — schedule auto-advance
        autoAdvanceTimerRef.current = setTimeout(() => {
          onNext();
        }, 1500);
      } else {
        startPolling();
      }
    };

    initialize();

    return () => {
      stopPolling();
      if (autoAdvanceTimerRef.current !== null) {
        clearTimeout(autoAdvanceTimerRef.current);
        autoAdvanceTimerRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // When status transitions to connected, stop polling and auto-advance
  useEffect(() => {
    if (status?.connected) {
      stopPolling();
      if (autoAdvanceTimerRef.current === null) {
        autoAdvanceTimerRef.current = setTimeout(() => {
          onNext();
        }, 1500);
      }
    }
  }, [status?.connected, stopPolling, onNext]);

  const handleRefresh = useCallback(async () => {
    setLoading(true);
    await fetchStatus();
    setLoading(false);
  }, [fetchStatus]);

  const handleConnect = useCallback(async () => {
    setConnectLoading(true);
    try {
      const result = await tailscaleApi.connect();
      setStatus(result.data);
      if (result.data.auth_url) {
        setAuthUrl(result.data.auth_url);
      }
    } catch {
      // leave state unchanged
    } finally {
      setConnectLoading(false);
    }
  }, []);

  const handleCopyUrl = useCallback(async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard unavailable
    }
  }, []);

  const infoCardStyle: React.CSSProperties = {
    background: "var(--bg-tertiary)",
    border: "1px solid var(--border-subtle)",
    borderRadius: 10,
    padding: "14px 16px",
    marginBottom: 16,
  };

  const skipButtonStyle: React.CSSProperties = {
    color: "var(--text-tertiary)",
    background: "none",
    border: "none",
    cursor: "pointer",
    fontSize: 12,
    padding: "8px 0",
    display: "block",
    margin: "8px auto 0",
  };

  const primaryButtonStyle: React.CSSProperties = {
    background: "var(--accent-primary)",
    border: "none",
    borderRadius: 7,
    padding: "9px 18px",
    color: "var(--text-inverse, #fff)",
    fontSize: 13,
    fontWeight: 500,
    cursor: "pointer",
    transition: "opacity 0.15s",
  };

  const secondaryButtonStyle: React.CSSProperties = {
    background: "transparent",
    border: "1px solid var(--border-default)",
    borderRadius: 7,
    padding: "7px 14px",
    color: "var(--text-secondary)",
    fontSize: 12,
    fontWeight: 500,
    cursor: "pointer",
    transition: "opacity 0.15s",
  };

  if (loading) {
    return (
      <div>
        <div style={{ textAlign: "center", marginBottom: 24 }}>
          <h2
            style={{
              fontSize: 20,
              fontWeight: 700,
              color: "var(--text-primary)",
              letterSpacing: "-0.02em",
              marginBottom: 8,
            }}
          >
            Checking Tailscale…
          </h2>
        </div>
        <div style={{ ...infoCardStyle, display: "flex", flexDirection: "column", gap: 8 }}>
          <div
            style={{
              height: 12,
              width: "55%",
              borderRadius: 4,
              background: "var(--border-subtle)",
            }}
          />
          <div
            style={{
              height: 10,
              width: "75%",
              borderRadius: 4,
              background: "var(--border-subtle)",
            }}
          />
        </div>
      </div>
    );
  }

  // ── STATE 1: Not installed ────────────────────────────────────────────────
  if (!status || !status.installed) {
    return (
      <div>
        <div style={{ textAlign: "center", marginBottom: 24 }}>
          <h2
            style={{
              fontSize: 20,
              fontWeight: 700,
              color: "var(--text-primary)",
              letterSpacing: "-0.02em",
              marginBottom: 8,
            }}
          >
            Set Up Secure Access
          </h2>
          <p
            style={{
              fontSize: 13,
              color: "var(--text-secondary)",
              lineHeight: 1.6,
              maxWidth: 380,
              margin: "0 auto",
            }}
          >
            Tailscale lets you securely access agent-deck from anywhere, on any
            device.
          </p>
        </div>

        <div style={infoCardStyle}>
          <div style={{ display: "flex", alignItems: "flex-start", gap: 10, marginBottom: 10 }}>
            <StatusDot color={DOT_AMBER} />
            <div>
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 600,
                  color: "var(--text-primary)",
                  marginBottom: 4,
                }}
              >
                Tailscale not found
              </div>
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  lineHeight: 1.55,
                }}
              >
                agent-deck works best with Tailscale for secure remote access.
                Run the agent-deck install script to install Tailscale along with
                all other dependencies automatically.
              </div>
            </div>
          </div>

          <button
            type="button"
            style={secondaryButtonStyle}
            onClick={handleRefresh}
          >
            Refresh ⟳
          </button>
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 12,
            color: "var(--text-tertiary)",
            marginBottom: 20,
          }}
        >
          <span>Waiting for Tailscale</span>
          <AnimatedEllipsis />
        </div>

        <button type="button" style={skipButtonStyle} onClick={onSkip}>
          I'll set this up later →
        </button>
      </div>
    );
  }

  // ── STATE 2: Installed, not connected ─────────────────────────────────────
  if (!status.connected) {
    return (
      <div>
        <div style={{ textAlign: "center", marginBottom: 24 }}>
          <h2
            style={{
              fontSize: 20,
              fontWeight: 700,
              color: "var(--text-primary)",
              letterSpacing: "-0.02em",
              marginBottom: 8,
            }}
          >
            Set Up Secure Access
          </h2>
          <p
            style={{
              fontSize: 13,
              color: "var(--text-secondary)",
              lineHeight: 1.6,
              maxWidth: 380,
              margin: "0 auto",
            }}
          >
            Tailscale is installed. Connect to enable private remote access from
            any device.
          </p>
        </div>

        <div style={infoCardStyle}>
          <div style={{ display: "flex", alignItems: "flex-start", gap: 10, marginBottom: 14 }}>
            <StatusDot color={DOT_RED} />
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "var(--text-primary)",
              }}
            >
              Tailscale installed — not connected
            </div>
          </div>

          <button
            type="button"
            style={{
              ...primaryButtonStyle,
              opacity: connectLoading ? 0.6 : 1,
              cursor: connectLoading ? "not-allowed" : "pointer",
            }}
            onClick={handleConnect}
            disabled={connectLoading}
          >
            {connectLoading ? "Connecting…" : "Connect to Tailscale"}
          </button>

          {authUrl && (
            <div style={{ marginTop: 12 }}>
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  marginBottom: 6,
                }}
              >
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
                  lineHeight: 1.5,
                }}
              >
                {authUrl}
              </a>
            </div>
          )}
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 12,
            color: "var(--text-tertiary)",
            marginBottom: 20,
          }}
        >
          <span>Waiting for connection</span>
          <AnimatedEllipsis />
        </div>

        <button type="button" style={skipButtonStyle} onClick={onSkip}>
          I'll set this up later →
        </button>
      </div>
    );
  }

  // ── STATE 3: Connected ────────────────────────────────────────────────────
  const mobileUrl = status.hostname ? `http://${status.hostname}:7474` : null;

  return (
    <div>
      <div style={{ textAlign: "center", marginBottom: 24 }}>
        <h2
          style={{
            fontSize: 20,
            fontWeight: 700,
            color: "var(--text-primary)",
            letterSpacing: "-0.02em",
            marginBottom: 8,
          }}
        >
          Tailscale Connected
        </h2>
        <p
          style={{
            fontSize: 13,
            color: "var(--text-secondary)",
            lineHeight: 1.6,
            maxWidth: 380,
            margin: "0 auto",
          }}
        >
          Secure remote access is active. You can reach agent-deck from any of
          your Tailscale devices.
        </p>
      </div>

      <div style={infoCardStyle}>
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: 10,
            marginBottom: 12,
          }}
        >
          <StatusDot color={DOT_GREEN} />
          <div>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "var(--text-primary)",
                marginBottom: 2,
              }}
            >
              Connected to Tailscale
            </div>
            {(status.hostname || status.ip_address) && (
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-tertiary)",
                  marginTop: 2,
                }}
              >
                {[status.hostname, status.ip_address].filter(Boolean).join(" · ")}
              </div>
            )}
          </div>
        </div>

        {mobileUrl && (
          <div
            style={{
              background: "var(--bg-elevated, var(--bg-secondary))",
              border: "1px solid var(--border-subtle)",
              borderRadius: 7,
              padding: "10px 12px",
            }}
          >
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                marginBottom: 6,
              }}
            >
              Connect from another device:
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  fontFamily: '"SF Mono","Fira Code",monospace',
                  flex: 1,
                  wordBreak: "break-all",
                }}
              >
                {mobileUrl}
              </span>
              <button
                type="button"
                onClick={() => handleCopyUrl(mobileUrl)}
                style={{
                  background: "transparent",
                  border: "1px solid var(--border-default)",
                  borderRadius: 5,
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
          </div>
        )}
      </div>

      <div
        style={{
          fontSize: 12,
          color: "var(--text-tertiary)",
          textAlign: "center",
          marginBottom: 16,
        }}
      >
        Advancing in 1s… —{" "}
        <button
          type="button"
          onClick={onNext}
          style={{
            background: "none",
            border: "none",
            color: "var(--accent-primary)",
            fontSize: 12,
            cursor: "pointer",
            padding: 0,
            fontWeight: 500,
          }}
        >
          Next →
        </button>
      </div>

      <WizardNavRow onNext={onNext} nextLabel="Next →" />
    </div>
  );
}

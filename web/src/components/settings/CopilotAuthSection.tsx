import React, { useState, useEffect, useCallback, useRef } from "react";
import { RefreshCw, ExternalLink, CheckCircle2, Loader2 } from "lucide-react";
import { copilotApi } from "@/api/client";
import { Btn, type CopilotAuthStep } from "./shared";

/**
 * Inline GitHub device-auth flow widget.
 * Checks auth status on mount, then allows the user to connect (or re-connect)
 * their GitHub account via the Copilot device flow.
 *
 * Completely self-contained — owns all auth state and polling logic.
 */
export function CopilotAuthSection() {
  const [auth, setAuth] = useState<CopilotAuthStep>({ stage: "idle" });
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Keep a ref so the polling interval callback always reads the latest stage
  // without a stale closure.
  const authRef = useRef<CopilotAuthStep>(auth);
  useEffect(() => {
    authRef.current = auth;
  }, [auth]);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  // Check whether a token is already stored on mount
  useEffect(() => {
    let cancelled = false;
    setAuth({ stage: "checking" });
    copilotApi
      .authStatus()
      .then((res) => {
        if (cancelled) return;
        setAuth(
          res.data.authenticated
            ? { stage: "authenticated" }
            : { stage: "idle" },
        );
      })
      .catch(() => {
        if (!cancelled) setAuth({ stage: "idle" });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Clean up polling on unmount
  useEffect(() => () => stopPolling(), [stopPolling]);

  const startAuth = useCallback(async () => {
    stopPolling();
    setAuth({ stage: "checking" });
    try {
      const res = await copilotApi.authStart();
      const d = res.data;
      const expiresAt = Date.now() + d.expires_in * 1000;

      const authorizingState: CopilotAuthStep = {
        stage: "authorizing",
        userCode: d.user_code,
        verificationUri: d.verification_uri,
        deviceCode: d.device_code,
        expiresAt,
      };
      setAuth(authorizingState);
      authRef.current = authorizingState;

      const intervalMs = (d.interval ?? 5) * 1000;
      pollRef.current = setInterval(async () => {
        const current = authRef.current;
        // Stop if we've already moved out of the authorizing stage
        if (current.stage !== "authorizing") {
          stopPolling();
          return;
        }

        if (Date.now() > current.expiresAt) {
          stopPolling();
          setAuth({
            stage: "error",
            message: "Authentication timed out. Please try again.",
          });
          return;
        }

        try {
          const pollRes = await copilotApi.authPoll(current.deviceCode);
          if (pollRes.data.authenticated) {
            stopPolling();
            setAuth({ stage: "authenticated" });
          }
          // "authorization_pending" or "slow_down" — keep waiting
        } catch {
          // network hiccup — keep polling
        }
      }, intervalMs);
    } catch (err) {
      setAuth({
        stage: "error",
        message:
          err instanceof Error
            ? err.message
            : "Failed to start authentication.",
      });
    }
  }, [stopPolling]);

  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "16px 18px",
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      {/* Header row */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ fontSize: 22 }}>🐙</span>
        <div>
          <div
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            GitHub Copilot
          </div>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              marginTop: 1,
            }}
          >
            Authenticate with your GitHub account to use Copilot models.
          </div>
        </div>

        {/* Status badge */}
        <div style={{ marginLeft: "auto" }}>
          {auth.stage === "authenticated" && (
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                background: "rgba(106,158,91,0.15)",
                color: "var(--success)",
                border: "1px solid rgba(106,158,91,0.3)",
                borderRadius: 20,
                padding: "3px 10px",
                fontSize: 12,
                fontWeight: 600,
              }}
            >
              <CheckCircle2 size={13} /> Authenticated
            </span>
          )}
          {(auth.stage === "checking" || auth.stage === "authorizing") && (
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                color: "var(--text-tertiary)",
                fontSize: 12,
              }}
            >
              <Loader2
                size={13}
                style={{ animation: "spin 0.8s linear infinite" }}
              />
              {auth.stage === "checking" ? "Checking…" : "Waiting for GitHub…"}
            </span>
          )}
        </div>
      </div>

      {/* Device-code card — shown while authorizing */}
      {auth.stage === "authorizing" && (
        <div
          style={{
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-default)",
            borderRadius: 8,
            padding: "14px 16px",
            display: "flex",
            flexDirection: "column",
            gap: 10,
          }}
        >
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              lineHeight: 1.5,
            }}
          >
            Visit the link below and enter the code to authenticate:
          </div>

          {/* User code */}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <code
              style={{
                fontSize: 22,
                fontFamily: '"SF Mono","Fira Code",monospace',
                fontWeight: 700,
                letterSpacing: "0.15em",
                color: "var(--accent-secondary)",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border-default)",
                borderRadius: 6,
                padding: "6px 14px",
                flex: 1,
                textAlign: "center",
              }}
            >
              {auth.userCode}
            </code>
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(auth.userCode)}
              title="Copy code"
              style={{
                background: "var(--bg-secondary)",
                border: "1px solid var(--border-default)",
                borderRadius: 6,
                padding: "6px 10px",
                fontSize: 12,
                color: "var(--text-secondary)",
                cursor: "pointer",
                fontFamily: "inherit",
                flexShrink: 0,
              }}
            >
              Copy
            </button>
          </div>

          {/* Verification link */}
          <a
            href={auth.verificationUri}
            target="_blank"
            rel="noopener noreferrer"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              fontSize: 13,
              color: "var(--accent-secondary)",
              textDecoration: "none",
              fontWeight: 500,
            }}
          >
            <ExternalLink size={13} />
            {auth.verificationUri}
          </a>

          <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            This page will update automatically once you approve access on
            GitHub.
          </div>
        </div>
      )}

      {/* Error state */}
      {auth.stage === "error" && (
        <div style={{ fontSize: 13, color: "var(--error)" }}>
          ⚠ {auth.message}
        </div>
      )}

      {/* Action buttons */}
      {auth.stage === "authenticated" ? (
        <Btn sm variant="ghost" onClick={startAuth}>
          <RefreshCw size={12} /> Re-authenticate
        </Btn>
      ) : auth.stage === "idle" || auth.stage === "error" ? (
        <Btn sm variant="primary" onClick={startAuth}>
          Connect GitHub Account
        </Btn>
      ) : null}
    </div>
  );
}

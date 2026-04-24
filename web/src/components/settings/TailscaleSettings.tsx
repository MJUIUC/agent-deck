import React, { useRef, useEffect, useState } from "react";
import { TailscaleStatusCard } from "./TailscaleStatusCard";
import { tailscaleApi } from "@/api/client";
import type { TailscaleStatus } from "@/types";
import QRCode from "qrcode";

// ─── QR canvas ────────────────────────────────────────────────────────────────

function QrCanvas({ url }: { url: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (canvasRef.current && url) {
      QRCode.toCanvas(canvasRef.current, url, {
        width: 140,
        color: { dark: "#000000", light: "#ffffff" },
      });
    }
  }, [url]);

  return <canvas ref={canvasRef} style={{ borderRadius: 8 }} />;
}

// ─── TailscaleSettings ────────────────────────────────────────────────────────

export function TailscaleSettings() {
  const [tailscaleStatus, setTailscaleStatus] = useState<TailscaleStatus | null>(null);

  useEffect(() => {
    tailscaleApi.getStatus().then((res) => setTailscaleStatus(res.data)).catch(() => {});
  }, []);

  const mobileUrl = tailscaleStatus?.hostname
    ? `http://${tailscaleStatus.hostname}:7474`
    : null;

  const qrUrl = mobileUrl ?? window.location.origin;

  return (
    <div>
      {/* ── Header ── */}
      <div style={{ marginBottom: 24 }}>
        <div
          style={{
            fontSize: 16,
            fontWeight: 700,
            color: "var(--text-primary)",
            marginBottom: 4,
          }}
        >
          Tailscale
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          VPN status, Funnel, and remote access configuration.
        </div>
      </div>

      {/* ── Status card ── */}
      <TailscaleStatusCard onFunnelToggle={() => {
        tailscaleApi.getStatus().then((res) => setTailscaleStatus(res.data)).catch(() => {});
      }} />

      {/* ── Open on Phone ── */}
      <div
        style={{
          background: "var(--bg-tertiary)",
          border: "1px solid var(--border-subtle)",
          borderRadius: 10,
          padding: "14px 16px",
          marginBottom: 16,
        }}
      >
        {/* Section header */}
        <div
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--text-primary)",
            marginBottom: 4,
          }}
        >
          Open on Phone
        </div>
        <div
          style={{
            fontSize: 12,
            color: "var(--text-tertiary)",
            marginBottom: 16,
            lineHeight: 1.5,
          }}
        >
          {tailscaleStatus?.connected
            ? "Scan to open agent-deck on any device connected to your Tailscale network."
            : "Connect Tailscale above to get your private network URL, then scan to open on any device."}
        </div>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 12,
          }}
        >
          <QrCanvas url={qrUrl} />

          <span
            style={{
              fontSize: 11,
              fontFamily: '"SF Mono","Fira Code",monospace',
              color: "var(--text-secondary)",
              wordBreak: "break-all",
              textAlign: "center",
            }}
          >
            {qrUrl}
          </span>

          {tailscaleStatus?.connected && (
            <div
              style={{
                fontSize: 11,
                color: "var(--text-tertiary)",
                textAlign: "center",
                lineHeight: 1.6,
                maxWidth: 380,
              }}
            >
              On iPhone: open this URL in Safari, then tap{" "}
              <strong style={{ color: "var(--text-secondary)" }}>
                Share → Add to Home Screen
              </strong>{" "}
              to install as a PWA.
              <br />
              On Android: tap the browser menu and choose{" "}
              <strong style={{ color: "var(--text-secondary)" }}>
                Install app
              </strong>
              .
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

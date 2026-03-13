import React, { useEffect, useRef, useState } from "react";
import QRCode from "qrcode";
import { pairingApi } from "@/api/client";
import { Btn } from "./shared";

// ─── QR canvas renderer ───────────────────────────────────────────────────────

function QrCanvas({ value }: { value: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!canvasRef.current) return;
    QRCode.toCanvas(canvasRef.current, value, {
      width: 220,
      margin: 2,
      color: {
        dark: "#F0EDE4", // text_primary — warm white dots
        light: "#242422", // bg_secondary — dark background
      },
    });
  }, [value]);

  return (
    <canvas
      ref={canvasRef}
      style={{
        borderRadius: 10,
        display: "block",
      }}
    />
  );
}

// ─── Step row ─────────────────────────────────────────────────────────────────

function StepRow({
  num,
  children,
}: {
  num: number;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        gap: 12,
        alignItems: "flex-start",
        paddingBottom: 10,
        marginBottom: 10,
        borderBottom: "1px solid var(--border-subtle)",
      }}
    >
      <div
        style={{
          width: 22,
          height: 22,
          borderRadius: "50%",
          background: "var(--accent-muted)",
          color: "var(--accent-primary)",
          fontSize: 11,
          fontWeight: 700,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          marginTop: 1,
        }}
      >
        {num}
      </div>
      <div
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          lineHeight: 1.55,
        }}
      >
        {children}
      </div>
    </div>
  );
}

// ─── MobileSettings ───────────────────────────────────────────────────────────

export function MobileSettings() {
  const [qrValue, setQrValue] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generated, setGenerated] = useState(false);

  const generate = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await pairingApi.generate();
      setQrValue(JSON.stringify(res.data.pairing_payload));
      setGenerated(true);
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Failed to generate pairing code.",
      );
    } finally {
      setLoading(false);
    }
  };

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
          Mobile Pairing
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          Pair your Android device by scanning this QR code in the agent-deck
          mobile app. The token is valid until it's rotated — re-scan after any
          token rotation.
        </div>
      </div>

      {/* QR + action — centered */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 20,
          marginBottom: 24,
        }}
      >
        {/* QR display area */}
        <div
          style={{
            width: 220,
            height: 220,
            borderRadius: 10,
            background: "var(--bg-tertiary)",
            border: "1px solid var(--border-subtle)",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: 8,
            overflow: "hidden",
          }}
        >
          {qrValue ? (
            <QrCanvas value={qrValue} />
          ) : (
            <>
              <span
                style={{
                  fontSize: 40,
                  color: "var(--text-tertiary)",
                  lineHeight: 1,
                }}
              >
                ▦
              </span>
              <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                QR code renders here
              </span>
              <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
                Contains server URL + auth token
              </span>
            </>
          )}
        </div>

        {/* Error */}
        {error && (
          <div
            style={{
              padding: "8px 14px",
              borderRadius: 7,
              background: "rgba(196,90,90,0.1)",
              border: "1px solid rgba(196,90,90,0.25)",
              color: "var(--error)",
              fontSize: 12,
            }}
          >
            {error}
          </div>
        )}

        {/* Action buttons */}
        <div style={{ display: "flex", gap: 8 }}>
          {!generated ? (
            <Btn variant="primary" onClick={generate} disabled={loading}>
              {loading ? "Generating…" : "Generate QR Code"}
            </Btn>
          ) : (
            <Btn variant="ghost" onClick={generate} disabled={loading}>
              {loading ? "Refreshing…" : "⟳ Refresh"}
            </Btn>
          )}
        </div>
      </div>

      {/* How to pair steps */}
      <div
        style={{
          background: "var(--bg-tertiary)",
          border: "1px solid var(--border-subtle)",
          borderRadius: 10,
          padding: "16px 18px",
          maxWidth: 480,
          margin: "0 auto",
        }}
      >
        <div
          style={{
            fontSize: 11,
            fontWeight: 600,
            color: "var(--text-tertiary)",
            textTransform: "uppercase",
            letterSpacing: "0.07em",
            marginBottom: 14,
          }}
        >
          How to pair
        </div>

        <StepRow num={1}>
          Install the{" "}
          <strong style={{ color: "var(--text-primary)" }}>agent-deck</strong>{" "}
          app on your Android device.
        </StepRow>

        <StepRow num={2}>
          Open the app and tap{" "}
          <code
            style={{
              fontFamily: '"SF Mono","Fira Code",monospace',
              fontSize: 12,
              background: "var(--bg-elevated)",
              padding: "1px 5px",
              borderRadius: 3,
            }}
          >
            Scan QR to connect
          </code>{" "}
          on the pairing screen.
        </StepRow>

        <StepRow num={3}>
          Scan the QR code displayed above. The app will store your server URL
          and auth token automatically.
        </StepRow>

        <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
          <div
            style={{
              width: 22,
              height: 22,
              borderRadius: "50%",
              background: "var(--accent-muted)",
              color: "var(--accent-primary)",
              fontSize: 11,
              fontWeight: 700,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              flexShrink: 0,
              marginTop: 1,
            }}
          >
            4
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-secondary)",
              lineHeight: 1.55,
            }}
          >
            Make sure both devices are on the same{" "}
            <strong style={{ color: "var(--text-primary)" }}>Tailscale</strong>{" "}
            network.
          </div>
        </div>
      </div>
    </div>
  );
}

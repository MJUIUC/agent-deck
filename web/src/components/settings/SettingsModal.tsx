import React, { useState, useEffect, useCallback } from "react";

import type { SettingsTab } from "./shared";
import { SettingsSidebar } from "./SettingsNav";
import { ProviderSettings } from "./ProviderSettings";
import { PersonaSettings } from "./PersonaSettings";
import { CredentialsSettings } from "./CredentialsSettings";
import { McpServerSettings } from "./McpServerSettings";
import { WebhookSettings } from "./WebhookSettings";
import { ProfileSettings } from "./ProfileSettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { TailscaleSettings } from "./TailscaleSettings";
import { AccessSettings } from "./AccessSettings";
import { ArchivedThreadsSettings } from "./ArchivedThreadsSettings";

export interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialTab?: SettingsTab;
  onDataChanged?: () => void;
}

export function SettingsModal({
  isOpen,
  onClose,
  initialTab = "providers",
  onDataChanged,
}: SettingsModalProps) {
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [prevIsOpen, setPrevIsOpen] = useState(isOpen);

  // Sync tab to initialTab each time the modal transitions closed → open
  if (prevIsOpen !== isOpen) {
    setPrevIsOpen(isOpen);
    if (isOpen) setTab(initialTab);
  }

  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [isOpen, onClose]);

  const handleDataChanged = useCallback(() => {
    onDataChanged?.();
  }, [onDataChanged]);

  const handleBackdrop = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose],
  );

  return (
    <>
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>

      {/* Backdrop */}
      <div
        onClick={handleBackdrop}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 200,
          background: "rgba(0,0,0,0.45)",
          backdropFilter: "blur(6px)",
          WebkitBackdropFilter: "blur(6px)",
          opacity: isOpen ? 1 : 0,
          pointerEvents: "none",
          transition: "opacity 0.2s",
        }}
      />

      {/* Positioner */}
      <div
        onClick={handleBackdrop}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 201,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          pointerEvents: isOpen ? "auto" : "none",
        }}
        aria-modal="true"
        role="dialog"
        aria-label="Settings"
      >
        {/* Modal shell — 860×600 */}
        <div
          style={{
            position: "relative",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 14,
            width: 860,
            maxWidth: "calc(100vw - 32px)",
            height: 600,
            maxHeight: "calc(100vh - 48px)",
            display: "flex",
            overflow: "hidden",
            boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
            opacity: isOpen ? 1 : 0,
            transform: isOpen ? "scale(1)" : "scale(0.97)",
            transition: "opacity 0.2s, transform 0.2s",
          }}
        >
          <SettingsSidebar tab={tab} onTabChange={setTab} />

          {/* Close button */}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close settings"
            style={{
              position: "absolute",
              top: 14,
              right: 16,
              width: 28,
              height: 28,
              borderRadius: 7,
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 18,
              lineHeight: 1,
              zIndex: 10,
              transition: "background 0.15s, color 0.15s",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--bg-tertiary)";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-tertiary)";
            }}
          >
            ×
          </button>

          {/* Main content */}
          <div
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
            }}
          >
            <div
              style={{ flex: 1, overflowY: "auto", padding: "24px 28px" }}
              className="scrollbar-thin"
            >
              {tab === "providers" && (
                <ProviderSettings onDataChanged={handleDataChanged} />
              )}
              {tab === "credentials" && <CredentialsSettings />}
              {tab === "personas" && (
                <PersonaSettings onDataChanged={handleDataChanged} />
              )}
              {tab === "mcp-servers" && <McpServerSettings />}
              {tab === "webhooks" && <WebhookSettings />}
              {tab === "profile" && <ProfileSettings />}
              {tab === "appearance" && <AppearanceSettings />}
              {tab === "tailscale" && <TailscaleSettings />}
              {tab === "access" && <AccessSettings />}
              {tab === "archived-threads" && <ArchivedThreadsSettings />}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

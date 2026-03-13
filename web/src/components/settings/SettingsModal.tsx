import React, { useState, useEffect, useCallback } from "react";
import { X } from "lucide-react";
import type { SettingsTab } from "./shared";
import { SettingsSidebar } from "./SettingsNav";
import { ProviderSettings } from "./ProviderSettings";
import { PersonaSettings } from "./PersonaSettings";
import { McpServerSettings } from "./McpServerSettings";
import { MobileSettings } from "./MobileSettings";
import { GeneralSettings } from "./GeneralSettings";
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
          <SettingsSidebar tab={tab} onTabChange={setTab} onClose={onClose} />

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
              {tab === "personas" && (
                <PersonaSettings onDataChanged={handleDataChanged} />
              )}
              {tab === "mcp-servers" && <McpServerSettings />}
              {tab === "mobile" && <MobileSettings />}
              {tab === "general" && <GeneralSettings />}
              {tab === "archived-threads" && <ArchivedThreadsSettings />}
            </div>
          </div>

          {/* Close × button */}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close settings"
            style={{
              position: "absolute",
              top: 14,
              right: 14,
              width: 28,
              height: 28,
              borderRadius: 6,
              background: "transparent",
              border: "none",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              transition: "background 0.15s, color 0.15s",
              fontFamily: "inherit",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--bg-elevated)";
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
            <X size={15} />
          </button>
        </div>
      </div>
    </>
  );
}

import React, { useState } from "react";
import {
  CloudApp,
  Password,
  UserAvatar,
  ToolKit,
  Settings,
  Archive,
  Ai,
} from "@carbon/icons-react";
import type { SettingsTab } from "./shared";

// ─── NavItem ──────────────────────────────────────────────────────────────────

export function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "8px 16px",
        margin: "0 8px 2px",
        borderRadius: 7,
        fontSize: 13,
        color:
          active || hovered ? "var(--text-primary)" : "var(--text-secondary)",
        cursor: "pointer",
        background: active
          ? "var(--accent-muted)"
          : hovered
            ? "var(--bg-tertiary)"
            : "none",
        border: "none",
        width: "calc(100% - 16px)",
        textAlign: "left",
        transition: "background 0.15s, color 0.15s",
        fontFamily: "inherit",
      }}
    >
      <span style={{ display: "flex", alignItems: "center" }}>{icon}</span>
      {label}
    </button>
  );
}

// ─── BackLink ─────────────────────────────────────────────────────────────────

export function BackLink({ onClick }: { onClick: () => void }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 7,
        padding: "7px 16px",
        margin: "0 8px 12px",
        borderRadius: 6,
        fontSize: 13,
        color: hovered ? "var(--text-primary)" : "var(--text-secondary)",
        cursor: "pointer",
        background: hovered ? "var(--bg-tertiary)" : "none",
        border: "none",
        transition: "background 0.15s, color 0.15s",
        fontFamily: "inherit",
      }}
    >
      ← Back to Chats
    </button>
  );
}

// ─── SettingsSidebar ──────────────────────────────────────────────────────────

/**
 * The 220px left sidebar inside SettingsModal.
 * Renders the brand header, back link, nav section label, and tab nav items.
 */
export function SettingsSidebar({
  tab,
  onTabChange,
  onClose,
}: {
  tab: SettingsTab;
  onTabChange: (t: SettingsTab) => void;
  onClose: () => void;
}) {
  return (
    <aside
      style={{
        width: 220,
        minWidth: 220,
        background: "var(--bg-secondary)",
        borderRight: "1px solid var(--border-subtle)",
        display: "flex",
        flexDirection: "column",
        padding: "20px 0",
      }}
    >
      {/* Brand */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 16px 20px",
          borderBottom: "1px solid var(--border-subtle)",
          marginBottom: 12,
        }}
      >
        <Ai size={20} />
        <span
          style={{
            fontSize: 14,
            fontWeight: 700,
            color: "var(--text-primary)",
            letterSpacing: "-0.01em",
          }}
        >
          agent-deck
        </span>
      </div>

      <BackLink onClick={onClose} />

      <div
        style={{
          padding: "4px 16px 6px",
          fontSize: 10,
          fontWeight: 600,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.08em",
        }}
      >
        Settings
      </div>

      <NavItem
        icon={<CloudApp size={16} />}
        label="Providers"
        active={tab === "providers"}
        onClick={() => onTabChange("providers")}
      />
      <NavItem
        icon={<Password size={16} />}
        label="Credentials"
        active={tab === "credentials"}
        onClick={() => onTabChange("credentials")}
      />
      <NavItem
        icon={<UserAvatar size={16} />}
        label="Personas"
        active={tab === "personas"}
        onClick={() => onTabChange("personas")}
      />
      <NavItem
        icon={<ToolKit size={16} />}
        label="MCP Servers"
        active={tab === "mcp-servers"}
        onClick={() => onTabChange("mcp-servers")}
      />

      <NavItem
        icon={<Settings size={16} />}
        label="General"
        active={tab === "general"}
        onClick={() => onTabChange("general")}
      />
      <NavItem
        icon={<Archive size={16} />}
        label="Archived Threads"
        active={tab === "archived-threads"}
        onClick={() => onTabChange("archived-threads")}
      />
    </aside>
  );
}

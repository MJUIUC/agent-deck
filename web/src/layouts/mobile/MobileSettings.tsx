import { useCallback } from "react";
import styles from "./MobileSettings.module.css";

// ─── Props ────────────────────────────────────────────────────────────────────

interface MobileSettingsProps {
  onOpenFullSettings?: () => void;
}

// ─── MobileSettings ───────────────────────────────────────────────────────────

export function MobileSettings({ onOpenFullSettings }: MobileSettingsProps) {
  const handleFullSettings = useCallback(() => {
    onOpenFullSettings?.();
    window.dispatchEvent(new CustomEvent("agent-deck:open-settings"));
  }, [onOpenFullSettings]);

  return (
    <div className={styles.container}>
      {/* ── Nav bar ── */}
      <div className={styles.navBar}>
        <h1 className={styles.navTitle}>Settings</h1>
      </div>

      {/* ── Scrollable body ── */}
      <div className={`${styles.body} scrollbar-thin`}>

        {/* ── Install as App ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Install as App</div>
          <div className={styles.sectionCard}>
            <div className={styles.row}>
              <div className={styles.rowTitle}>Add to Home Screen</div>
              <div className={styles.rowDesc}>
                Install agent-deck as a Progressive Web App on your iOS or
                Android device for a native-like experience. Setup instructions
                coming soon.
              </div>
            </div>
          </div>
        </section>

        {/* ── Notifications ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Notifications</div>
          <div className={styles.sectionCard}>
            <div className={styles.row}>
              <div className={styles.rowTitle}>Push Notifications</div>
              <div className={styles.rowDesc}>
                Get alerted when your agent responds or a scheduled routine
                completes. Push notification setup coming soon.
              </div>
            </div>
          </div>
        </section>

        {/* ── Connection ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Connection</div>
          <div className={styles.sectionCard}>
            <div className={styles.row}>
              <div className={styles.rowDesc}>
                Configure AI providers, personas, credentials, MCP servers, and
                other server settings.
              </div>
            </div>
            <div className={styles.rowAction}>
              <button
                type="button"
                className={styles.fullSettingsBtn}
                onClick={handleFullSettings}
              >
                ⚙︎ Full Settings
              </button>
            </div>
          </div>
        </section>

      </div>
    </div>
  );
}

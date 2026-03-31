import { useEffect, useState, type ReactNode } from "react";

import { usePlatform } from "../../hooks/usePlatform";
import styles from "./MobileSettings.module.css";

// ─── Notification status ──────────────────────────────────────────────────────

type NotifState =
  | "loading"
  | "enabled"
  | "not-enabled"
  | "blocked"
  | "unavailable";

function useNotificationStatus(): NotifState {
  const [state, setState] = useState<NotifState>("loading");

  useEffect(() => {
    async function check(): Promise<void> {
      if (typeof Notification === "undefined") {
        setState("unavailable");
        return;
      }

      const permission = Notification.permission;

      if (permission === "denied") {
        setState("blocked");
        return;
      }

      if (!navigator.serviceWorker) {
        setState("unavailable");
        return;
      }

      let subscription: PushSubscription | null = null;
      try {
        const reg = await navigator.serviceWorker.ready;
        subscription = await reg.pushManager.getSubscription();
      } catch {
        setState("unavailable");
        return;
      }

      if (permission === "granted" && subscription !== null) {
        setState("enabled");
      } else {
        setState("not-enabled");
      }
    }

    check().catch(() => setState("unavailable"));
  }, []);

  return state;
}

// ─── Install steps ────────────────────────────────────────────────────────────

const IOS_STEPS: ReactNode[] = [
  <>
    Open this page in <strong>Safari</strong> (not Chrome or Firefox)
  </>,
  <>
    Tap the <strong>Share</strong> button at the bottom of the screen
  </>,
  <>
    Scroll down and tap <strong>"Add to Home Screen"</strong>
  </>,
  <>
    Tap <strong>"Add"</strong> — agent-deck will appear on your home screen
  </>,
];

const ANDROID_STEPS: ReactNode[] = [
  <>
    Open this page in <strong>Chrome</strong>
  </>,
  <>
    Tap the <strong>⋮ menu</strong> in the top-right corner
  </>,
  <>
    Tap <strong>"Add to Home Screen"</strong> or <strong>"Install app"</strong>
  </>,
  <>
    Tap <strong>"Add"</strong> — agent-deck will appear in your app drawer
  </>,
];

// ─── MobileSettings ───────────────────────────────────────────────────────────

export function MobileSettings() {
  const platform = usePlatform();
  const notifState = useNotificationStatus();

  const steps =
    platform === "ios"
      ? IOS_STEPS
      : platform === "android"
        ? ANDROID_STEPS
        : null;

  const dotClass = (() => {
    switch (notifState) {
      case "enabled":
        return styles.statusDotGreen;
      case "not-enabled":
        return styles.statusDotYellow;
      case "blocked":
        return styles.statusDotRed;
      default:
        return styles.statusDotGrey;
    }
  })();

  const statusMessage = (() => {
    switch (notifState) {
      case "enabled":
        return "Notifications enabled";
      case "not-enabled":
        return "Notifications not enabled";
      case "blocked":
        return "Notifications blocked — enable in your browser settings";
      case "unavailable":
        return "Notifications unavailable in this browser";
      default:
        return "";
    }
  })();

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
            {steps !== null ? (
              steps.map((text, i) => (
                <div key={i} className={`${styles.row} ${styles.stepRow}`}>
                  <span className={styles.stepNum}>{i + 1}</span>
                  <span className={styles.stepText}>{text}</span>
                </div>
              ))
            ) : (
              <div className={styles.row}>
                <div className={styles.rowDesc}>
                  Open this page on your iOS or Android device to install
                  agent-deck as an app.
                </div>
              </div>
            )}
          </div>
        </section>

        {/* ── Notifications ── */}
        <section className={styles.section}>
          <div className={styles.sectionLabel}>Notifications</div>
          <div className={styles.sectionCard}>
            <div className={styles.row}>
              {notifState !== "loading" && (
                <>
                  <div className={styles.statusRow}>
                    <span className={`${styles.statusDot} ${dotClass}`} />
                    <span className={styles.statusText}>{statusMessage}</span>
                  </div>
                  {notifState === "not-enabled" && (
                    <div style={{ marginTop: 12 }}>
                      <button className={styles.enableBtn} disabled>
                        Enable Notifications
                      </button>
                      <div className={styles.comingSoonLabel}>
                        (coming soon)
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

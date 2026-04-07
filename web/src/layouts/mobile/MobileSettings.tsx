import { useEffect, useState, type ReactNode } from "react";

import { pushApi } from "../../api/client";
import { usePlatform } from "../../hooks/usePlatform";
import styles from "./MobileSettings.module.css";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function urlBase64ToUint8Array(base64String: string): Uint8Array<ArrayBuffer> {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const rawData = window.atob(base64);
  const output = new Uint8Array(new ArrayBuffer(rawData.length));
  for (let i = 0; i < rawData.length; i++) {
    output[i] = rawData.charCodeAt(i);
  }
  return output;
}

// ─── Notification status ──────────────────────────────────────────────────────

type NotifState =
  | "loading"
  | "enabled"
  | "not-enabled"
  | "blocked"
  | "unavailable";

function useNotificationStatus(): {
  state: NotifState;
  recheck: () => void;
} {
  const [state, setState] = useState<NotifState>("loading");
  const [refreshKey, setRefreshKey] = useState(0);

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
  }, [refreshKey]);

  return { state, recheck: () => setRefreshKey((k) => k + 1) };
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
  const { state: notifState, recheck } = useNotificationStatus();
  const [notifLoading, setNotifLoading] = useState(false);

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

  async function handleEnable() {
    setNotifLoading(true);
    try {
      const permission = await Notification.requestPermission();
      if (permission !== "granted") {
        recheck();
        setNotifLoading(false);
        return;
      }
      const reg = await navigator.serviceWorker.ready;
      const { data } = await pushApi.getVapidPublicKey();
      const sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToUint8Array(data.public_key),
      });
      const subJson = sub.toJSON() as {
        endpoint: string;
        keys: { p256dh: string; auth: string };
      };
      await pushApi.subscribe({
        endpoint: subJson.endpoint,
        p256dh: subJson.keys.p256dh,
        auth: subJson.keys.auth,
        user_agent: navigator.userAgent,
      });
      recheck();
    } catch (err) {
      console.error("Enable notifications failed:", err);
    } finally {
      setNotifLoading(false);
    }
  }

  async function handleDisable() {
    setNotifLoading(true);
    try {
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) {
        await sub.unsubscribe();
        await pushApi.unsubscribe(sub.endpoint);
      }
      recheck();
    } catch (err) {
      console.error("Disable notifications failed:", err);
    } finally {
      setNotifLoading(false);
    }
  }

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
                      <button
                        className={styles.enableBtn}
                        onClick={handleEnable}
                        disabled={notifLoading}
                      >
                        Enable Notifications
                      </button>
                    </div>
                  )}
                  {notifState === "enabled" && (
                    <div style={{ marginTop: 12 }}>
                      <button
                        className={styles.disableBtn}
                        onClick={handleDisable}
                        disabled={notifLoading}
                      >
                        Disable Notifications
                      </button>
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

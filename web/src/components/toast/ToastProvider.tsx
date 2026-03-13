import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
} from "react";
import styles from "./ToastProvider.module.css";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ToastVariant = "success" | "error" | "neutral";

export interface Toast {
  id: string;
  message: string;
  variant: ToastVariant;
}

interface ToastContextValue {
  toast: (message: string, variant?: ToastVariant) => void;
}

// ── Context ───────────────────────────────────────────────────────────────────

const ToastContext = createContext<ToastContextValue | null>(null);

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used inside <ToastProvider>");
  }
  return ctx;
}

// ── Auto-dismiss duration ─────────────────────────────────────────────────────
// TODO: Make this configurable via an environment variable or app config.
const TOAST_DURATION_MS = 3000;

// ── Provider ──────────────────────────────────────────────────────────────────

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  // Track per-toast timers so we can clear them if needed.
  const timers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const timer = timers.current.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.current.delete(id);
    }
  }, []);

  const toast = useCallback(
    (message: string, variant: ToastVariant = "neutral") => {
      const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
      const entry: Toast = { id, message, variant };

      // Append to the bottom of the stack (FIFO — oldest at top, newest at bottom).
      setToasts((prev) => [...prev, entry]);

      // Auto-dismiss after TOAST_DURATION_MS
      const timer = setTimeout(() => {
        dismiss(id);
      }, TOAST_DURATION_MS);

      timers.current.set(id, timer);
    },
    [dismiss],
  );

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}

      {/* Toast stack — rendered at the top-centre of the viewport */}
      {toasts.length > 0 && (
        <div className={styles.stack} role="region" aria-label="Notifications" aria-live="polite">
          {toasts.map((t) => (
            <div
              key={t.id}
              className={[
                styles.toast,
                t.variant === "success"
                  ? styles.success
                  : t.variant === "error"
                    ? styles.error
                    : styles.neutral,
              ].join(" ")}
              role="alert"
            >
              <span className={styles.icon}>
                {t.variant === "success"
                  ? "✓"
                  : t.variant === "error"
                    ? "✕"
                    : "ℹ"}
              </span>
              <span className={styles.message}>{t.message}</span>
              <button
                className={styles.close}
                onClick={() => dismiss(t.id)}
                aria-label="Dismiss notification"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}
    </ToastContext.Provider>
  );
}

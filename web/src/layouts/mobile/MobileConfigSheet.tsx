// Slide-up bottom sheet rendered over MobileChatView.
// Shows thread configuration: persona, model (read-only), routines (with toggles).
// ─────────────────────────────────────────────────────────────────────────────

import { useState, useEffect, useRef, useCallback } from "react";
import cronstrue from "cronstrue";
import type { Thread, Routine } from "@/types";
import { routinesApi } from "@/api/client";
import styles from "./MobileConfigSheet.module.css";

// ─── Props ────────────────────────────────────────────────────────────────────

interface MobileConfigSheetProps {
  thread: Thread;
  isOpen: boolean;
  onClose: () => void;
}

// ─── Toggle sub-component ─────────────────────────────────────────────────────

interface ToggleProps {
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  label: string;
}

function Toggle({ checked, onChange, disabled = false, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={onChange}
      className={styles.toggleBtn}
    >
      <span
        className={[styles.toggleTrack, checked ? styles.toggleOn : ""]
          .filter(Boolean)
          .join(" ")}
      >
        <span
          className={[styles.toggleThumb, checked ? styles.toggleThumbOn : ""]
            .filter(Boolean)
            .join(" ")}
        />
      </span>
    </button>
  );
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Converts a cron expression to a human-readable string, or returns "" on failure. */
function cronToHuman(expr: string): string {
  try {
    return cronstrue.toString(expr, {
      verbose: false,
      throwExceptionOnParseError: true,
    });
  } catch {
    return "";
  }
}

/** Joins class names, filtering out falsy values. */
function cx(...classes: Array<string | undefined | false>): string {
  return classes.filter(Boolean).join(" ");
}

// ─── Component ────────────────────────────────────────────────────────────────

export function MobileConfigSheet({
  thread,
  isOpen,
  onClose,
}: MobileConfigSheetProps) {
  // ── State ────────────────────────────────────────────────────────────────
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routinesLoading, setRoutinesLoading] = useState(false);
  const [togglingId, setTogglingId] = useState<string | null>(null);

  // ── Refs ─────────────────────────────────────────────────────────────────
  const sheetRef = useRef<HTMLDivElement>(null);
  // clientY recorded at touchstart on the drag handle
  const dragStartY = useRef<number | null>(null);
  // Whether we've ever opened — avoids unmounting during the close animation
  const hasOpenedRef = useRef(false);

  // Mark as opened so the render guard doesn't strip the sheet during close animation
  useEffect(() => {
    if (isOpen) {
      hasOpenedRef.current = true;
    }
  }, [isOpen]);

  // ── Fetch routines when sheet opens ──────────────────────────────────────
  useEffect(() => {
    if (!isOpen) return;

    setRoutinesLoading(true);
    routinesApi
      .list(thread.id)
      .then((res) => {
        setRoutines(res.data);
      })
      .catch(() => {
        setRoutines([]);
      })
      .finally(() => {
        setRoutinesLoading(false);
      });
  }, [isOpen, thread.id]);

  // ── Lock body scroll while sheet is open ─────────────────────────────────
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [isOpen]);

  // ── Routine toggle ────────────────────────────────────────────────────────
  const handleToggleRoutine = useCallback(
    async (routine: Routine) => {
      if (togglingId !== null) return; // debounce concurrent taps
      setTogglingId(routine.id);
      try {
        const res = await routinesApi.toggle(thread.id, routine.id);
        setRoutines((prev) =>
          prev.map((r) =>
            r.id === routine.id ? { ...r, enabled: res.data.enabled } : r,
          ),
        );
      } catch {
        // Silent fail on mobile — no toast needed here
      } finally {
        setTogglingId(null);
      }
    },
    [thread.id, togglingId],
  );

  // ── Open-settings dispatcher ──────────────────────────────────────────────
  const openSettings = useCallback(
    (tab?: string) => {
      window.dispatchEvent(
        new CustomEvent("agent-deck:open-settings", {
          detail: tab ? { tab } : undefined,
        }),
      );
      onClose();
    },
    [onClose],
  );

  // ── Drag-to-dismiss (touch) ───────────────────────────────────────────────

  const handleTouchStart = useCallback(
    (e: React.TouchEvent<HTMLDivElement>) => {
      dragStartY.current = e.touches[0].clientY;
    },
    [],
  );

  const handleTouchMove = useCallback((e: React.TouchEvent<HTMLDivElement>) => {
    if (dragStartY.current === null || !sheetRef.current) return;
    const dy = e.touches[0].clientY - dragStartY.current;
    if (dy > 0) {
      // Drag sheet down live; disable transition so it follows the finger
      sheetRef.current.style.transition = "none";
      sheetRef.current.style.transform = `translateY(${dy}px)`;
    }
  }, []);

  const handleTouchEnd = useCallback(
    (e: React.TouchEvent<HTMLDivElement>) => {
      if (dragStartY.current === null || !sheetRef.current) return;
      const dy = e.changedTouches[0].clientY - dragStartY.current;

      // Re-enable transition before snapping back or closing
      sheetRef.current.style.transition = "";
      sheetRef.current.style.transform = "";

      if (dy > 80) {
        onClose();
      }

      dragStartY.current = null;
    },
    [onClose],
  );

  // ── Render guard — don't mount until the sheet has been opened once ───────
  // This prevents the closed sheet from flash-rendering on first load.
  if (!hasOpenedRef.current && !isOpen) {
    return null;
  }

  const persona = thread.persona;

  return (
    <>
      {/* ── Backdrop ─────────────────────────────────────────────────────── */}
      <div
        className={cx(styles.backdrop, isOpen && styles.backdropVisible)}
        onClick={onClose}
        aria-hidden="true"
      />

      {/* ── Bottom sheet ─────────────────────────────────────────────────── */}
      <div
        ref={sheetRef}
        role="dialog"
        aria-modal="true"
        aria-label="Thread Config"
        className={cx(styles.sheet, !isOpen && styles.sheetClosed)}
      >
        {/* ── Drag handle ──────────────────────────────────────────────── */}
        <div
          className={styles.handle}
          onTouchStart={handleTouchStart}
          onTouchMove={handleTouchMove}
          onTouchEnd={handleTouchEnd}
          aria-hidden="true"
        >
          <div className={styles.handleBar} />
        </div>

        {/* ── Header ───────────────────────────────────────────────────── */}
        <div className={styles.header}>
          <span className={styles.title}>Thread Config</span>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Close thread config"
          >
            ✕
          </button>
        </div>

        {/* ── Scrollable body ───────────────────────────────────────────── */}
        <div className={styles.body}>
          {/* ── Persona ─────────────────────────────────────────────── */}
          <section
            className={styles.section}
            aria-labelledby="mcs-label-persona"
          >
            <div id="mcs-label-persona" className={styles.sectionLabel}>
              Persona
            </div>

            <div className={styles.personaRow}>
              <div className={styles.personaAvatar} aria-hidden="true">
                {persona?.emoji ?? "🤖"}
              </div>

              <div className={styles.personaInfo}>
                <div className={styles.personaName}>
                  {persona?.name ?? "Default"}
                </div>
                <div className={styles.personaDesc}>
                  {persona?.system_prompt
                    ? persona.system_prompt.length > 72
                      ? persona.system_prompt.slice(0, 72) + "…"
                      : persona.system_prompt
                    : "No persona description"}
                </div>
              </div>

              {/* "Full settings →" badge-link */}
              <button
                type="button"
                className={styles.personaBadge}
                onClick={() => openSettings("personas")}
                aria-label="Open persona settings"
              >
                Full settings →
              </button>
            </div>
          </section>

          <div className={styles.divider} aria-hidden="true" />

          {/* ── Model (read-only) ────────────────────────────────────── */}
          <section className={styles.section} aria-labelledby="mcs-label-model">
            <div id="mcs-label-model" className={styles.sectionLabel}>
              Model
            </div>

            <div className={styles.modelCurrent}>
              <div className={styles.modelIcon} aria-hidden="true">
                🤖
              </div>
              <div className={styles.modelInfo}>
                <div className={styles.modelName}>
                  {thread.active_model ?? "Default model"}
                </div>
                <div className={styles.modelProvider}>
                  {thread.active_provider ?? "Default provider"}
                </div>
              </div>
            </div>

            <p className={styles.modelNote}>
              Change model in{" "}
              <button
                type="button"
                className={styles.inlineLink}
                onClick={() => openSettings("models")}
              >
                full settings
              </button>
              .
            </p>
          </section>

          <div className={styles.divider} aria-hidden="true" />

          {/* ── Routines ─────────────────────────────────────────────── */}
          <section
            className={styles.section}
            aria-labelledby="mcs-label-routines"
          >
            <div id="mcs-label-routines" className={styles.sectionLabel}>
              Routines
            </div>

            {/* View-only notice */}
            <div className={styles.viewOnlyNotice} role="note">
              <span className={styles.viewOnlyIcon} aria-hidden="true">
                ℹ️
              </span>
              <span className={styles.viewOnlyText}>
                Toggle routines below. To add or edit schedules,{" "}
                <button
                  type="button"
                  className={styles.noticeLink}
                  onClick={() => openSettings("routines")}
                >
                  open full settings
                </button>
                .
              </span>
            </div>

            {routinesLoading ? (
              <div
                className={styles.stateText}
                role="status"
                aria-live="polite"
              >
                Loading routines…
              </div>
            ) : routines.length === 0 ? (
              <div className={styles.stateText}>No routines configured.</div>
            ) : (
              <div className={styles.routinesList}>
                {routines.map((routine) => {
                  const humanLabel = cronToHuman(routine.cron_expr);
                  const isToggling = togglingId === routine.id;

                  return (
                    <div key={routine.id} className={styles.routineRow}>
                      {/* Icon */}
                      <div
                        className={cx(
                          styles.routineIcon,
                          routine.enabled && styles.routineIconActive,
                        )}
                        aria-hidden="true"
                      >
                        ⏰
                      </div>

                      {/* Info */}
                      <div className={styles.routineInfo}>
                        <div className={styles.routineName}>{routine.name}</div>

                        <div className={styles.routineSchedule}>
                          <span className={styles.routineCron}>
                            {routine.cron_expr}
                          </span>
                          {humanLabel && (
                            <span className={styles.routineHuman}>
                              {humanLabel}
                            </span>
                          )}
                        </div>

                        <div
                          className={cx(
                            styles.routineStatus,
                            routine.enabled
                              ? styles.routineStatusOn
                              : styles.routineStatusOff,
                          )}
                          aria-live="polite"
                        >
                          <span
                            className={styles.statusDot}
                            aria-hidden="true"
                          />
                          {routine.enabled ? "Enabled" : "Disabled"}
                        </div>
                      </div>

                      {/* Toggle */}
                      <Toggle
                        checked={routine.enabled}
                        onChange={() => handleToggleRoutine(routine)}
                        disabled={isToggling}
                        label={`${routine.enabled ? "Disable" : "Enable"} routine: ${routine.name}`}
                      />
                    </div>
                  );
                })}
              </div>
            )}
          </section>

          <div className={styles.divider} aria-hidden="true" />

          {/* ── Full settings link ───────────────────────────────────── */}
          <section className={styles.section}>
            <button
              type="button"
              className={styles.settingsLink}
              onClick={() => openSettings()}
            >
              <span aria-hidden="true">⚙️</span>
              Full settings →
            </button>
          </section>
        </div>
        {/* /body */}
      </div>
      {/* /sheet */}
    </>
  );
}

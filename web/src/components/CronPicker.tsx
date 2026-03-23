import { useState, useEffect } from "react";
import cronstrue from "cronstrue";
import styles from "./CronPicker.module.css";

// ── Types ─────────────────────────────────────────────────────────────────────

type Frequency = "daily" | "weekdays" | "weekends" | "custom";

interface PickerState {
  frequency: Frequency;
  customDays: boolean[]; // index 0 = Sunday … 6 = Saturday
  hour: number; // 1–12
  minute: number; // 0 | 15 | 30 | 45
  ampm: "AM" | "PM";
}

export interface CronPickerProps {
  /** Current 5-field cron expression, e.g. "0 9 * * 1-5" */
  value: string;
  /** Called whenever the picker generates a new expression */
  onChange: (expr: string) => void;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const DAY_LABELS = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"] as const;
const HOURS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] as const;
const MINUTES = [0, 15, 30, 45] as const;

const FREQ_OPTIONS: { key: Frequency; label: string }[] = [
  { key: "daily", label: "Every day" },
  { key: "weekdays", label: "Weekdays" },
  { key: "weekends", label: "Weekends" },
  { key: "custom", label: "Custom" },
];

// ── State helpers ─────────────────────────────────────────────────────────────

function defaultState(): PickerState {
  return {
    frequency: "daily",
    customDays: Array(7).fill(true) as boolean[],
    hour: 9,
    minute: 0,
    ampm: "AM",
  };
}

function parseCron(expr: string): PickerState {
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return defaultState();

  const [minStr, hourStr, , , dowStr] = parts;
  const minute = parseInt(minStr, 10);
  const hour24 = parseInt(hourStr, 10);

  if (isNaN(minute) || isNaN(hour24)) return defaultState();

  // Convert 24 h → 12 h + AM/PM
  const ampm: "AM" | "PM" = hour24 < 12 ? "AM" : "PM";
  const hour12 = hour24 === 0 ? 12 : hour24 > 12 ? hour24 - 12 : hour24;

  // Snap minute to nearest 0 / 15 / 30 / 45
  const snappedMinute = ([0, 15, 30, 45] as const).reduce((prev, curr) =>
    Math.abs(curr - minute) < Math.abs(prev - minute) ? curr : prev,
  );

  // Detect frequency from DOW field
  let frequency: Frequency;
  let customDays: boolean[];

  if (dowStr === "*") {
    frequency = "daily";
    customDays = Array(7).fill(true) as boolean[];
  } else if (dowStr === "1-5") {
    frequency = "weekdays";
    customDays = [false, true, true, true, true, true, false];
  } else if (dowStr === "0,6" || dowStr === "6,0") {
    frequency = "weekends";
    customDays = [true, false, false, false, false, false, true];
  } else {
    frequency = "custom";
    customDays = Array(7).fill(false) as boolean[];
    dowStr.split(",").forEach((d) => {
      const idx = parseInt(d, 10);
      if (idx >= 0 && idx <= 6) customDays[idx] = true;
    });
  }

  return {
    frequency,
    customDays,
    hour: hour12,
    minute: snappedMinute,
    ampm,
  };
}

function buildCron(state: PickerState): string {
  const hour24 =
    state.ampm === "AM"
      ? state.hour === 12
        ? 0
        : state.hour
      : state.hour === 12
        ? 12
        : state.hour + 12;

  let dow: string;
  switch (state.frequency) {
    case "daily":
      dow = "*";
      break;
    case "weekdays":
      dow = "1-5";
      break;
    case "weekends":
      dow = "0,6";
      break;
    case "custom": {
      const active = state.customDays
        .map((on, i) => (on ? i : -1))
        .filter((i) => i >= 0);
      dow = active.length === 0 ? "*" : active.join(",");
      break;
    }
    default:
      dow = "*";
  }

  return `${state.minute} ${hour24} * * ${dow}`;
}

/** Returns the 7-element active-day array for the current mode. */
function getActiveDays(state: PickerState): boolean[] {
  switch (state.frequency) {
    case "daily":
      return Array(7).fill(true) as boolean[];
    case "weekdays":
      return [false, true, true, true, true, true, false];
    case "weekends":
      return [true, false, false, false, false, false, true];
    case "custom":
      return state.customDays;
    default:
      return Array(7).fill(false) as boolean[];
  }
}

/** Tiny className helper — filters out falsy values. */
function cx(...args: (string | false | undefined | null)[]): string {
  return args.filter(Boolean).join(" ");
}

// ── Component ─────────────────────────────────────────────────────────────────

export function CronPicker({ value, onChange }: CronPickerProps) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<PickerState>(() => parseCron(value));

  // Track what we last pushed to the parent so we can skip re-parsing our own
  // onChange emissions and avoid an infinite loop.
  const prevValueRef = useRef<string>(value);

  // ── Sync incoming prop → internal state (controlled) ──────────────────────
  useEffect(() => {
    if (value !== prevValueRef.current) {
      prevValueRef.current = value;
      setState(parseCron(value));
    }
  }, [value]);

  // ── State mutator — updates local state AND notifies parent ───────────────
  function updateState(next: PickerState) {
    const expr = buildCron(next);
    prevValueRef.current = expr; // mark as our own emission
    setState(next);
    onChange(expr);
  }

  // ── Human-readable description ─────────────────────────────────────────────
  let description = "";
  try {
    description = cronstrue.toString(buildCron(state));
  } catch {
    description = "";
  }

  const activeDays = getActiveDays(state);
  const isCustom = state.frequency === "custom";

  // ── Render ─────────────────────────────────────────────────────────────────
  return (
    <div className={styles.wrap}>
      {/* ── Trigger button ── */}
      <button
        type="button"
        className={cx(styles.triggerBtn, open && styles.triggerBtnOpen)}
        onClick={() => setOpen((o) => !o)}
        aria-label="Open schedule picker"
        aria-expanded={open}
        title={open ? "Hide schedule builder" : "Show schedule builder"}
      >
        📅 {open ? "Hide schedule builder" : "Schedule builder"}
      </button>

      {/* ── Inline panel ── */}
      {open && (
        <div className={styles.panel} role="group" aria-label="Schedule picker">
          {/* 1. Frequency pills */}
          <div className={styles.frequencyRow}>
            {FREQ_OPTIONS.map(({ key, label }) => (
              <button
                key={key}
                type="button"
                className={cx(
                  styles.freqPill,
                  state.frequency === key && styles.freqPillActive,
                )}
                onClick={() => {
                  // When switching away from custom, preserve customDays so
                  // they are still there if the user flips back.
                  updateState({ ...state, frequency: key });
                }}
              >
                {label}
              </button>
            ))}
          </div>

          {/* 2. Day toggles */}
          <div className={styles.daysRow}>
            {DAY_LABELS.map((label, idx) => (
              <button
                key={idx}
                type="button"
                className={cx(
                  styles.dayPill,
                  activeDays[idx] && styles.dayPillActive,
                  !isCustom && styles.dayPillDisabled,
                )}
                disabled={!isCustom}
                aria-pressed={activeDays[idx]}
                aria-label={label}
                onClick={() => {
                  if (!isCustom) return;
                  const newDays = [...state.customDays];
                  newDays[idx] = !newDays[idx];
                  updateState({ ...state, customDays: newDays });
                }}
              >
                {label}
              </button>
            ))}
          </div>

          {/* 3. Time row */}
          <div className={styles.timeRow}>
            {/* Hour */}
            <select
              className={styles.timeSelect}
              value={state.hour}
              aria-label="Hour"
              onChange={(e) =>
                updateState({ ...state, hour: parseInt(e.target.value, 10) })
              }
            >
              {HOURS.map((h) => (
                <option key={h} value={h}>
                  {h}
                </option>
              ))}
            </select>

            <span className={styles.colon} aria-hidden>
              :
            </span>

            {/* Minute */}
            <select
              className={styles.timeSelect}
              value={state.minute}
              aria-label="Minute"
              onChange={(e) =>
                updateState({
                  ...state,
                  minute: parseInt(e.target.value, 10),
                })
              }
            >
              {MINUTES.map((m) => (
                <option key={m} value={m}>
                  {String(m).padStart(2, "0")}
                </option>
              ))}
            </select>

            {/* AM / PM toggle */}
            <button
              type="button"
              className={cx(
                styles.ampmBtn,
                state.ampm === "PM" && styles.ampmBtnActive,
              )}
              aria-label={`Switch to ${state.ampm === "AM" ? "PM" : "AM"}`}
              onClick={() =>
                updateState({
                  ...state,
                  ampm: state.ampm === "AM" ? "PM" : "AM",
                })
              }
            >
              {state.ampm}
            </button>
          </div>

          {/* 4. Human-readable description */}
          {description && <p className={styles.description}>{description}</p>}
        </div>
      )}
    </div>
  );
}

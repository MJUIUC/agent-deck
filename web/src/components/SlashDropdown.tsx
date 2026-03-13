import { useEffect, useRef } from "react";
import styles from "./SlashDropdown.module.css";

// ── Command definitions ───────────────────────────────────────────────────────

export interface SlashCommand {
  command: string; // e.g. "model"
  argHint: string; // e.g. "list | switch <name>"
  description: string;
  icon: string;
  badge: string;
  badgeVariant: "model" | "system" | "memory";
}

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    command: "model",
    argHint: "list | switch <name>",
    description: "List or switch the active model for this thread.",
    icon: "🔄",
    badge: "Model",
    badgeVariant: "model",
  },
  {
    command: "routine",
    argHint: "list | add",
    description: "List routines attached to this thread or open the editor.",
    icon: "⚡",
    badge: "System",
    badgeVariant: "system",
  },
  {
    command: "memory",
    argHint: "list",
    description: "Show recent memories for this thread's persona.",
    icon: "🗂",
    badge: "Memory",
    badgeVariant: "memory",
  },
  {
    command: "help",
    argHint: "",
    description: "Show all available commands.",
    icon: "❓",
    badge: "System",
    badgeVariant: "system",
  },
];

// ── Props ─────────────────────────────────────────────────────────────────────

interface SlashDropdownProps {
  /** The raw value of the input, e.g. "/mod" */
  query: string;
  /** Currently highlighted index into the *visible* items list */
  highlightIndex: number;
  onHighlight: (index: number) => void;
  /** Called when the user selects a command (Enter, Tab, or click) */
  onSelect: (command: SlashCommand) => void;
  /** Called when Escape is pressed */
  onDismiss: () => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function SlashDropdown({
  query,
  highlightIndex,
  onHighlight,
  onSelect,
  onDismiss,
}: SlashDropdownProps) {
  const listRef = useRef<HTMLDivElement>(null);

  // Derive the filter text from everything after the leading "/".
  // "/mod" → "mod", "/" → "", "/model sw" → "model sw"
  const filter = query.startsWith("/") ? query.slice(1).toLowerCase() : query.toLowerCase();

  // Only filter on the first word so "/model " still shows the model item.
  const filterWord = filter.split(/\s+/)[0];

  const visible = SLASH_COMMANDS.filter((cmd) =>
    filterWord === "" ? true : cmd.command.startsWith(filterWord),
  );

  // Clamp highlight to visible range whenever the list changes.
  const clampedIndex = visible.length > 0
    ? Math.min(highlightIndex, visible.length - 1)
    : 0;

  // Scroll highlighted item into view.
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const items = list.querySelectorAll<HTMLDivElement>("[data-slash-item]");
    const el = items[clampedIndex];
    if (el) {
      el.scrollIntoView({ block: "nearest" });
    }
  }, [clampedIndex]);

  if (visible.length === 0) return null;

  return (
    <div className={styles.dropdown} role="listbox" aria-label="Slash commands">
      {/* Header */}
      <div className={styles.header}>
        <span className={styles.title}>Commands</span>
        <span className={styles.hint}>
          <kbd className={styles.kbd}>↑</kbd>
          <kbd className={styles.kbd}>↓</kbd>
          {" "}navigate{" "}
          <kbd className={styles.kbd}>↵</kbd>
          {" "}select{" "}
          <kbd className={styles.kbd}>Esc</kbd>
          {" "}dismiss
        </span>
      </div>

      {/* Command list */}
      <div ref={listRef} className={styles.list}>
        {visible.map((cmd, idx) => {
          const isHighlighted = idx === clampedIndex;
          return (
            <div
              key={cmd.command}
              data-slash-item
              role="option"
              aria-selected={isHighlighted}
              className={[
                styles.item,
                isHighlighted ? styles.highlighted : "",
              ].join(" ")}
              onMouseEnter={() => onHighlight(idx)}
              onMouseDown={(e) => {
                // Prevent the textarea from losing focus on click.
                e.preventDefault();
              }}
              onClick={() => onSelect(cmd)}
            >
              {/* Icon */}
              <div className={[styles.icon, isHighlighted ? styles.iconHighlighted : ""].join(" ")}>
                {cmd.icon}
              </div>

              {/* Text block */}
              <div className={styles.text}>
                <div className={styles.cmdLine}>
                  <span className={[styles.cmdName, isHighlighted ? styles.cmdNameHighlighted : ""].join(" ")}>
                    /{cmd.command}
                  </span>
                  {cmd.argHint && (
                    <span className={styles.cmdArg}>{cmd.argHint}</span>
                  )}
                </div>
                <div className={styles.desc}>{cmd.description}</div>
              </div>

              {/* Badge */}
              <span
                className={[
                  styles.badge,
                  cmd.badgeVariant === "model"
                    ? styles.badgeModel
                    : cmd.badgeVariant === "memory"
                      ? styles.badgeMemory
                      : styles.badgeSystem,
                ].join(" ")}
              >
                {cmd.badge}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Keyboard handler helper ───────────────────────────────────────────────────
// Exported so MessageInput can call it from its onKeyDown handler.

export interface SlashDropdownKeyResult {
  /** Whether the event was consumed by the dropdown */
  consumed: boolean;
  /** If the user selected a command, this is the filled text to put in the input */
  fill?: string;
  dismiss?: boolean;
}

export function handleSlashDropdownKey(
  e: React.KeyboardEvent,
  visible: SlashCommand[],
  highlightIndex: number,
  onHighlight: (i: number) => void,
  onDismiss: () => void,
): SlashDropdownKeyResult {
  if (visible.length === 0) return { consumed: false };

  const clamped = Math.min(highlightIndex, visible.length - 1);

  if (e.key === "ArrowDown") {
    e.preventDefault();
    onHighlight((clamped + 1) % visible.length);
    return { consumed: true };
  }

  if (e.key === "ArrowUp") {
    e.preventDefault();
    onHighlight((clamped - 1 + visible.length) % visible.length);
    return { consumed: true };
  }

  if (e.key === "Escape") {
    e.preventDefault();
    onDismiss();
    return { consumed: true, dismiss: true };
  }

  if ((e.key === "Enter" && !e.shiftKey) || e.key === "Tab") {
    const selected = visible[clamped];
    if (selected) {
      e.preventDefault();
      // Fill the command name + a trailing space into the input.
      return { consumed: true, fill: `/${selected.command} ` };
    }
  }

  return { consumed: false };
}

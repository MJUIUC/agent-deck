import { useRef, useState, useCallback, useEffect } from "react";
import { SendHorizonal } from "lucide-react";
import {
  SlashDropdown,
  SLASH_COMMANDS,
  handleSlashDropdownKey,
} from "./SlashDropdown";
import styles from "./MessageInput.module.css";

interface MessageInputProps {
  threadId: string;
  personaName?: string;
  modelName?: string;
  isSending: boolean;
  onSend: (content: string) => void;
  onCommand: (input: string) => void;
}

export function MessageInput({
  threadId,
  personaName = "Agent",
  modelName,
  isSending,
  onSend,
  onCommand,
}: MessageInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [value, setValue] = useState("");
  const [dropdownVisible, setDropdownVisible] = useState(false);
  const [highlightIndex, setHighlightIndex] = useState(0);

  // Reset input and dropdown when thread changes
  useEffect(() => {
    setValue("");
    setDropdownVisible(false);
    setHighlightIndex(0);
    if (textareaRef.current) {
      textareaRef.current.style.height = "22px";
    }
  }, [threadId]);

  const resize = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 160) + "px";
  }, []);

  // Derive visible commands for the current query so the key handler
  // can work with the same filtered list the dropdown renders.
  const getVisibleCommands = useCallback((query: string) => {
    const filter = query.startsWith("/")
      ? query.slice(1).toLowerCase()
      : query.toLowerCase();
    const filterWord = filter.split(/\s+/)[0];
    return SLASH_COMMANDS.filter((cmd) =>
      filterWord === "" ? true : cmd.command.startsWith(filterWord),
    );
  }, []);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const next = e.target.value;
      setValue(next);
      resize();

      if (next.startsWith("/")) {
        setDropdownVisible(true);
        setHighlightIndex(0);
      } else {
        setDropdownVisible(false);
      }
    },
    [resize],
  );

  const handleSubmit = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || isSending) return;

    // If the dropdown is visible and an item is highlighted, pressing Enter
    // should fill the command rather than submit — that is handled in
    // handleKeyDown before handleSubmit is ever called. If we reach here
    // with a slash-command, the dropdown has been dismissed (Escape) or the
    // user typed a full command and intentionally submitted.
    if (trimmed.startsWith("/")) {
      onCommand(trimmed);
    } else {
      onSend(trimmed);
    }

    setValue("");
    setDropdownVisible(false);
    setHighlightIndex(0);
    if (textareaRef.current) {
      textareaRef.current.style.height = "22px";
    }
  }, [value, isSending, onSend, onCommand]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // When the dropdown is open, let it handle navigation keys first.
      if (dropdownVisible) {
        const visible = getVisibleCommands(value);
        const result = handleSlashDropdownKey(
          e,
          visible,
          highlightIndex,
          setHighlightIndex,
          () => setDropdownVisible(false),
        );

        if (result.consumed) {
          if (result.fill) {
            // Fill the command name into the input and keep focus.
            setValue(result.fill);
            // Keep dropdown open so the user sees the selected command.
            setDropdownVisible(true);
            // Reset highlight to 0 after fill — the filter will re-run.
            setHighlightIndex(0);
            // Re-trigger resize for the new value.
            requestAnimationFrame(() => {
              const el = textareaRef.current;
              if (!el) return;
              el.style.height = "auto";
              el.style.height = Math.min(el.scrollHeight, 160) + "px";
              // Move cursor to end
              el.setSelectionRange(result.fill!.length, result.fill!.length);
            });
          }
          return;
        }
      }

      // Normal Enter = send
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [dropdownVisible, value, highlightIndex, getVisibleCommands, handleSubmit],
  );

  const handleSelect = useCallback(
    (cmd: import("./SlashDropdown").SlashCommand) => {
      const filled = `/${cmd.command} `;
      setValue(filled);
      setDropdownVisible(true);
      setHighlightIndex(0);
      requestAnimationFrame(() => {
        const el = textareaRef.current;
        if (!el) return;
        el.style.height = "auto";
        el.style.height = Math.min(el.scrollHeight, 160) + "px";
        el.focus();
        el.setSelectionRange(filled.length, filled.length);
      });
    },
    [],
  );

  const handleBlur = useCallback(() => {
    // Delay hide so a click on a dropdown item registers before we close.
    setTimeout(() => setDropdownVisible(false), 150);
  }, []);

  const handleFocus = useCallback(() => {
    if (value.startsWith("/")) {
      setDropdownVisible(true);
    }
  }, [value]);

  const isEmpty = value.trim().length === 0;
  const isSlashCommand = value.trim().startsWith("/");

  const placeholder = isSlashCommand
    ? "Type a command…"
    : `Message ${personaName}… (type / for commands)`;

  return (
    <div ref={wrapRef} className={styles.wrap} style={{ position: "relative" }}>
      {/* Slash command dropdown — floats above the input */}
      {dropdownVisible && (
        <SlashDropdown
          query={value}
          highlightIndex={highlightIndex}
          onHighlight={setHighlightIndex}
          onSelect={handleSelect}
          onDismiss={() => setDropdownVisible(false)}
        />
      )}

      {/* Input row */}
      <div className={styles.inputRow}>
        <textarea
          ref={textareaRef}
          rows={1}
          placeholder={placeholder}
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onBlur={handleBlur}
          onFocus={handleFocus}
          disabled={isSending}
          aria-label="Message input"
          aria-expanded={dropdownVisible}
          aria-haspopup="listbox"
          autoComplete="off"
          autoCorrect="off"
          spellCheck
          className={styles.textarea}
          style={{ minHeight: "22px", maxHeight: "160px" }}
        />
        <button
          onClick={handleSubmit}
          disabled={isEmpty || isSending}
          aria-label="Send message"
          title="Send"
          className={styles.sendBtn}
        >
          <SendHorizonal size={15} strokeWidth={2} />
        </button>
      </div>

      {/* Hints row */}
      <div className={styles.hints}>
        <span className={styles.hint}>
          ↵ send · Shift+↵ newline · / for commands
        </span>
        {modelName && <span className={styles.hint}>{modelName}</span>}
      </div>
    </div>
  );
}

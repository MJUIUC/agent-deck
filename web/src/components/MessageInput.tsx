import { useRef, useState, useCallback, useEffect } from "react";
import { SendHorizonal } from "lucide-react";
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
  const [value, setValue] = useState("");

  // Reset input when thread changes
  useEffect(() => {
    setValue("");
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

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setValue(e.target.value);
      resize();
    },
    [resize],
  );

  const handleSubmit = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || isSending) return;

    if (trimmed.startsWith("/")) {
      onCommand(trimmed);
    } else {
      onSend(trimmed);
    }

    setValue("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "22px";
    }
  }, [value, isSending, onSend, onCommand]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  const isEmpty = value.trim().length === 0;
  const isSlashCommand = value.trim().startsWith("/");

  const placeholder = isSlashCommand
    ? "Type a command…"
    : `Message ${personaName}… (type / for commands)`;

  return (
    <div className={styles.wrap}>
      {/* Input row */}
      <div className={styles.inputRow}>
        <textarea
          ref={textareaRef}
          rows={1}
          placeholder={placeholder}
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          disabled={isSending}
          aria-label="Message input"
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

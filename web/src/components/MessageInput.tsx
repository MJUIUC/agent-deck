import { useRef, useState, useCallback, useEffect } from "react";
import { SendHorizonal } from "lucide-react";
import styles from "./MessageInput.module.css";

interface MessageInputProps {
  threadId: string;
  personaName?: string;
  isSending: boolean;
  onSend: (content: string) => void;
}

export function MessageInput({
  threadId,
  personaName = "Agent",
  isSending,
  onSend,
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
    onSend(trimmed);
    setValue("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "22px";
    }
  }, [value, isSending, onSend]);

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

  return (
    <div className={styles.wrap}>
      {/* Input row */}
      <div className={styles.inputRow}>
        <textarea
          ref={textareaRef}
          rows={1}
          placeholder={`Message ${personaName}…`}
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
        <span className={styles.hint}>↵ send · Shift+↵ newline</span>
      </div>
    </div>
  );
}

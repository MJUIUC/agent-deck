import { useRef, useState, useCallback, useEffect } from "react";
import { SendHorizonal, Square } from "lucide-react";
import styles from "./MessageInput.module.css";

interface MessageInputProps {
  threadId: string;
  personaName?: string;
  isSending: boolean;
  isStreaming?: boolean;
  onSend: (content: string) => void;
  onCancel?: () => void;
  queuedCount?: number;
}

export function MessageInput({
  threadId,
  personaName = "Agent",
  isSending,
  isStreaming = false,
  onSend,
  onCancel,
  queuedCount = 0,
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
    // Allow sending while streaming (queues behind current run)
    // Only block while isSending (optimistic phase, before SSE started)
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

  const handleCancel = useCallback(() => {
    if (onCancel) onCancel();
  }, [onCancel]);

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

        {isStreaming ? (
          /* Stop button — shown while the agent is streaming */
          <button
            onClick={handleCancel}
            aria-label="Stop generation"
            title="Stop"
            className={styles.stopBtn}
          >
            <Square size={14} strokeWidth={2} fill="currentColor" />
          </button>
        ) : (
          /* Send button */
          <button
            onClick={handleSubmit}
            disabled={isEmpty || isSending}
            aria-label="Send message"
            title="Send"
            className={styles.sendBtn}
          >
            <SendHorizonal size={15} strokeWidth={2} />
          </button>
        )}
      </div>

      {/* Hints row */}
      <div className={styles.hints}>
        <span className={styles.hint}>↵ send · Shift+↵ newline</span>
        {queuedCount > 0 && (
          <span className={styles.queuedIndicator}>
            {queuedCount} message{queuedCount !== 1 ? "s" : ""} queued
          </span>
        )}
      </div>
    </div>
  );
}

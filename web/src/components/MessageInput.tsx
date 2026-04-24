import { useRef, useState, useCallback, useEffect } from "react";
import {
  Attachment,
  Close,
  SendAlt,
  StopFilled,
  InProgress,
} from "@carbon/icons-react";
import { uploadsApi } from "@/api/client";
import type { MessageAttachment } from "@/types";
import styles from "./MessageInput.module.css";

interface MessageInputProps {
  threadId: string;
  personaName?: string;
  isSending: boolean;
  isStreaming?: boolean;
  onSend: (content: string, attachments: MessageAttachment[]) => void;
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
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState("");
  const [pendingFiles, setPendingFiles] = useState<File[]>([]);
  const [uploading, setUploading] = useState(false);

  // Reset input and focus when thread changes
  useEffect(() => {
    setValue("");
    setPendingFiles([]);
    if (textareaRef.current) {
      textareaRef.current.style.height = "22px";
      textareaRef.current.focus();
    }
  }, [threadId]);

  // Restore focus when streaming ends (true → false transition)
  const wasStreamingRef = useRef(false);
  useEffect(() => {
    if (wasStreamingRef.current && !isStreaming) {
      textareaRef.current?.focus();
    }
    wasStreamingRef.current = isStreaming ?? false;
  }, [isStreaming]);

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

  const handleSubmit = useCallback(async () => {
    const trimmed = value.trim();
    if ((!trimmed && pendingFiles.length === 0) || isSending || uploading)
      return;

    setUploading(true);
    const uploaded: MessageAttachment[] = [];
    try {
      for (const file of pendingFiles) {
        const res = await uploadsApi.upload(threadId, file);
        uploaded.push({
          path: res.data.path,
          filename: res.data.filename,
          content_type: res.data.content_type,
        });
      }
    } catch {
      // upload failed — still send the message without attachments
    } finally {
      setUploading(false);
    }

    onSend(trimmed, uploaded);
    setValue("");
    setPendingFiles([]);
    if (textareaRef.current) textareaRef.current.style.height = "22px";
  }, [value, pendingFiles, isSending, uploading, onSend, threadId]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleCancel = useCallback(() => {
    if (onCancel) onCancel();
  }, [onCancel]);

  const removeFile = useCallback((idx: number) => {
    setPendingFiles((prev) => prev.filter((_, i) => i !== idx));
  }, []);

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const files = Array.from(e.target.files ?? []);
      setPendingFiles((prev) => [...prev, ...files]);
      // Reset input so same file can be re-selected
      e.target.value = "";
    },
    [],
  );

  const isEmpty = value.trim().length === 0 && pendingFiles.length === 0;

  return (
    <div
      className={styles.wrap}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        const files = Array.from(e.dataTransfer.files);
        if (files.length) setPendingFiles((prev) => [...prev, ...files]);
      }}
    >
      {/* Attachment chips */}
      {pendingFiles.length > 0 && (
        <div className={styles.attachmentChips}>
          {pendingFiles.map((file, idx) => (
            <div key={idx} className={styles.chip}>
              {file.type.startsWith("image/") ? (
                <img
                  src={URL.createObjectURL(file)}
                  alt={file.name}
                  className={styles.chipThumb}
                />
              ) : (
                <Attachment size={14} />
              )}
              <span className={styles.chipName}>{file.name}</span>
              {uploading ? (
                <InProgress size={14} />
              ) : (
                <button
                  type="button"
                  className={styles.chipRemove}
                  onClick={() => removeFile(idx)}
                  aria-label={`Remove ${file.name}`}
                >
                  <Close size={12} />
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Input row */}
      <div className={styles.inputRow}>
        {/* Paperclip button */}
        <button
          type="button"
          onClick={() => fileInputRef.current?.click()}
          disabled={isSending || uploading}
          aria-label="Attach file"
          title="Attach file"
          className={styles.attachBtn}
        >
          <Attachment size={15} />
        </button>

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
            <StopFilled size={14} />
          </button>
        ) : (
          /* Send button */
          <button
            onClick={() => void handleSubmit()}
            disabled={isEmpty || isSending || uploading}
            aria-label="Send message"
            title="Send"
            className={styles.sendBtn}
          >
            <SendAlt size={15} />
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

      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept="image/*,application/pdf,text/*,.md,.csv,.json,.txt,.ts,.tsx,.js,.jsx,.py,.rs"
        style={{ display: "none" }}
        onChange={handleFileChange}
      />
    </div>
  );
}

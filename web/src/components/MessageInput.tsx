import { useRef, useState, useCallback, useEffect } from "react";
import { SendHorizonal } from "lucide-react";

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
    <div className="px-[18px] pt-3 pb-4 bg-bg-primary border-t border-border-subtle shrink-0">
      {/* Input row */}
      <div className="flex items-end gap-2.5 bg-bg-tertiary border border-border-default rounded-[10px] px-3 py-2.5 transition-colors focus-within:border-accent-muted">
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
          className="flex-1 bg-transparent border-none outline-none resize-none text-[14px] text-text-primary placeholder:text-text-tertiary leading-[1.5] min-h-[22px] max-h-[160px] disabled:opacity-60 disabled:cursor-default font-[inherit]"
          style={{ minHeight: "22px", maxHeight: "160px" }}
        />
        <button
          onClick={handleSubmit}
          disabled={isEmpty || isSending}
          aria-label="Send message"
          title="Send"
          className="w-8 h-8 rounded-[7px] flex items-center justify-center shrink-0 transition-colors bg-accent-primary text-text-inverse hover:bg-accent-secondary disabled:bg-bg-elevated disabled:text-text-tertiary disabled:cursor-default"
        >
          <SendHorizonal size={15} strokeWidth={2} />
        </button>
      </div>

      {/* Hints row */}
      <div className="flex justify-between items-center pt-1.5 px-0.5">
        <span className="text-[11px] text-text-tertiary">
          ↵ send · Shift+↵ newline · / for commands
        </span>
        {modelName && (
          <span className="text-[11px] text-text-tertiary">{modelName}</span>
        )}
      </div>
    </div>
  );
}

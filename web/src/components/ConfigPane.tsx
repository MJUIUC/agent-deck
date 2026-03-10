import { useState, useEffect } from "react";
import type { Thread } from "@/types";
import { threadsApi } from "@/api/client";
import { X } from "lucide-react";

interface ConfigPaneProps {
  isOpen: boolean;
  thread: Thread;
  onClose: () => void;
  onThreadUpdated: (thread: Thread) => void;
}

export function ConfigPane({
  isOpen,
  thread,
  onClose,
  onThreadUpdated,
}: ConfigPaneProps) {
  const [addendum, setAddendum] = useState(thread.system_prompt_addendum ?? "");
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setAddendum(thread.system_prompt_addendum ?? "");
  }, [thread.id, thread.system_prompt_addendum]);

  const persona = thread.persona;

  const handleAddendumBlur = async () => {
    const current = addendum.trim();
    const existing = (thread.system_prompt_addendum ?? "").trim();
    if (current === existing) return;

    setIsSaving(true);
    try {
      const res = await threadsApi.update(thread.id, {
        system_prompt_addendum: current || undefined,
      });
      onThreadUpdated(res.data);
    } catch {
      setAddendum(thread.system_prompt_addendum ?? "");
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div
      className={[
        "absolute top-0 right-0 bottom-0 w-[320px]",
        "bg-bg-secondary border-l border-border-subtle",
        "z-10 flex flex-col overflow-y-auto scrollbar-thin",
        "config-pane",
        isOpen ? "config-pane-open" : "",
      ].join(" ")}
      role="complementary"
      aria-label="Thread settings"
    >
      {/* ── Header ── */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border-subtle shrink-0">
        <span className="text-[14px] font-semibold text-text-primary">
          Thread Settings
        </span>
        <button
          onClick={onClose}
          aria-label="Close thread settings"
          className="w-7 h-7 flex items-center justify-center rounded text-text-tertiary hover:bg-bg-elevated hover:text-text-primary transition-colors"
        >
          <X size={15} />
        </button>
      </div>

      {/* ── Persona ── */}
      <div className="px-4 py-3.5 border-b border-border-subtle">
        <div className="text-[10px] font-semibold text-text-tertiary uppercase tracking-[0.07em] mb-2">
          Persona
        </div>
        {persona ? (
          <div className="flex items-center gap-2.5">
            <div className="w-9 h-9 rounded-full bg-bg-elevated flex items-center justify-center text-[18px] shrink-0">
              {persona.emoji}
            </div>
            <div className="min-w-0">
              <div className="text-[13px] font-semibold text-text-primary">
                {persona.name}
              </div>
              <div className="text-[11px] text-text-tertiary mt-0.5 leading-[1.4] truncate max-w-[200px]">
                {persona.system_prompt.length > 50
                  ? persona.system_prompt.slice(0, 50) + "…"
                  : persona.system_prompt}
              </div>
            </div>
          </div>
        ) : (
          <div className="text-[13px] text-text-tertiary">
            No persona attached
          </div>
        )}
      </div>

      {/* ── Model ── */}
      {(thread.active_model || thread.active_provider) && (
        <div className="px-4 py-3.5 border-b border-border-subtle">
          <div className="text-[10px] font-semibold text-text-tertiary uppercase tracking-[0.07em] mb-2">
            Model
          </div>
          <div className="text-[13px] text-text-primary bg-bg-tertiary border border-border-default rounded-[6px] px-2.5 py-[7px]">
            {[thread.active_model, thread.active_provider]
              .filter(Boolean)
              .join(" · ")}
          </div>
        </div>
      )}

      {/* ── System Prompt Addendum ── */}
      <div className="px-4 py-3.5 flex-1">
        <div className="flex items-center justify-between mb-2">
          <div className="text-[10px] font-semibold text-text-tertiary uppercase tracking-[0.07em]">
            System Prompt Addendum
          </div>
          {isSaving && (
            <span className="text-[10px] text-accent-primary">Saving…</span>
          )}
        </div>
        <textarea
          className="w-full bg-bg-tertiary border border-border-default text-text-primary text-[12px] px-2.5 py-2 rounded-[6px] outline-none resize-y min-h-[80px] leading-[1.55] font-[inherit] placeholder:text-text-tertiary focus:border-accent-muted transition-colors disabled:opacity-60"
          placeholder="Add thread-specific instructions…"
          value={addendum}
          onChange={(e) => setAddendum(e.target.value)}
          onBlur={handleAddendumBlur}
          disabled={isSaving}
        />
        <p className="text-[11px] text-text-tertiary mt-1.5 leading-[1.5]">
          Appended to the persona's system prompt for this thread only. Saved
          automatically when you click away.
        </p>
      </div>
    </div>
  );
}

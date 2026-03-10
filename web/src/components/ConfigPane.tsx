import { useState, useEffect } from "react";
import type { Thread } from "@/types";
import { threadsApi } from "@/api/client";
import { X } from "lucide-react";
import styles from "./ConfigPane.module.css";

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
      className={[styles.pane, isOpen ? styles.paneOpen : ""].join(" ")}
      role="complementary"
      aria-label="Thread settings"
    >
      {/* ── Header ── */}
      <div className={styles.header}>
        <span className={styles.headerTitle}>Thread Settings</span>
        <button
          onClick={onClose}
          aria-label="Close thread settings"
          className={styles.closeBtn}
        >
          <X size={15} />
        </button>
      </div>

      {/* ── Persona ── */}
      <div className={styles.section}>
        <div className={styles.sectionLabel}>Persona</div>
        {persona ? (
          <div className={styles.personaRow}>
            <div className={styles.personaAvatar}>{persona.emoji}</div>
            <div>
              <div className={styles.personaName}>{persona.name}</div>
              <div className={styles.personaPrompt}>
                {persona.system_prompt.length > 50
                  ? persona.system_prompt.slice(0, 50) + "…"
                  : persona.system_prompt}
              </div>
            </div>
          </div>
        ) : (
          <div className={styles.noPersona}>No persona attached</div>
        )}
      </div>

      {/* ── Model ── */}
      {(thread.active_model || thread.active_provider) && (
        <div className={styles.section}>
          <div className={styles.sectionLabel}>Model</div>
          <div className={styles.modelChip}>
            {[thread.active_model, thread.active_provider]
              .filter(Boolean)
              .join(" · ")}
          </div>
        </div>
      )}

      {/* ── System Prompt Addendum ── */}
      <div className={styles.addendumSection}>
        <div className={styles.addendumHeader}>
          <div className={styles.sectionLabel}>System Prompt Addendum</div>
          {isSaving && <span className={styles.saving}>Saving…</span>}
        </div>
        <textarea
          className={styles.textarea}
          placeholder="Add thread-specific instructions…"
          value={addendum}
          onChange={(e) => setAddendum(e.target.value)}
          onBlur={handleAddendumBlur}
          disabled={isSaving}
        />
        <p className={styles.hint}>
          Appended to the persona's system prompt for this thread only. Saved
          automatically when you click away.
        </p>
      </div>
    </div>
  );
}

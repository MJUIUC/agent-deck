import { useState, useEffect, useCallback } from "react";
import type { AgentPersona } from "@/types";
import styles from "./PersonaPickerModal.module.css";

interface PersonaPickerModalProps {
  isOpen: boolean;
  personas: AgentPersona[];
  onConfirm: (personaId: string) => void;
  onCancel: () => void;
  isLoading?: boolean;
}

export function PersonaPickerModal({
  isOpen,
  personas,
  onConfirm,
  onCancel,
  isLoading = false,
}: PersonaPickerModalProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const sorted = [...personas].sort((a, b) => {
    if (a.is_default && !b.is_default) return -1;
    if (!a.is_default && b.is_default) return 1;
    return a.created_at < b.created_at ? -1 : 1;
  });

  // Pre-select the Default persona when modal opens
  useEffect(() => {
    if (isOpen && personas.length > 0 && !selectedId) {
      const def = personas.find((p) => p.is_default);
      setSelectedId(def ? def.id : personas[0].id);
    }
  }, [isOpen, personas, selectedId]);

  // Reset on close
  useEffect(() => {
    if (!isOpen) setSelectedId(null);
  }, [isOpen]);

  const handleConfirm = useCallback(() => {
    if (selectedId) onConfirm(selectedId);
  }, [selectedId, onConfirm]);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) onCancel();
    },
    [onCancel],
  );

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
      if (e.key === "Enter" && selectedId) handleConfirm();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onCancel, handleConfirm, selectedId]);

  return (
    <div
      className={[
        styles.backdrop,
        isOpen ? styles.backdropOpen : styles.backdropClosed,
      ].join(" ")}
      onClick={handleBackdropClick}
    >
      <div
        className={[
          styles.modal,
          isOpen ? styles.modalOpen : styles.modalClosed,
        ].join(" ")}
      >
        <div className={styles.title}>Start a new chat</div>

        {personas.length === 0 ? (
          <div className={styles.empty}>
            <div className={styles.emptyIcon}>🤖</div>
            <div className={styles.emptyTitle}>No personas configured</div>
            <div className={styles.emptyDesc}>
              Set up a persona in Settings before starting a chat.
            </div>
          </div>
        ) : (
          <div className={styles.grid}>
            {sorted.map((persona) => {
              const isDefault = persona.is_default;
              return (
                <div
                  key={persona.id}
                  onClick={() => setSelectedId(persona.id)}
                  className={[
                    styles.card,
                    selectedId === persona.id ? styles.cardSelected : "",
                  ].join(" ")}
                >
                  <div className={styles.cardEmoji}>
                    {isDefault ? "💬" : persona.emoji}
                  </div>
                  <div className={styles.cardName}>
                    {isDefault ? "None" : persona.name}
                  </div>
                  {isDefault ? (
                    <div className={styles.cardHint}>
                      Long-term memory not available
                    </div>
                  ) : (
                    <div className={styles.cardPrompt}>
                      {persona.system_prompt.length > 60
                        ? persona.system_prompt.slice(0, 60) + "…"
                        : persona.system_prompt}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        <div className={styles.footer}>
          <button
            onClick={onCancel}
            disabled={isLoading}
            className={styles.btnCancel}
          >
            Cancel
          </button>
          <button
            onClick={handleConfirm}
            disabled={!selectedId || isLoading || personas.length === 0}
            className={styles.btnConfirm}
          >
            {isLoading ? "Starting…" : "Start Chat"}
          </button>
        </div>
      </div>
    </div>
  );
}

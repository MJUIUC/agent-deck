import { useState, useEffect, useCallback } from "react";
import type { AgentPersona } from "@/types";

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

  // Pre-select first persona when modal opens
  useEffect(() => {
    if (isOpen && personas.length > 0 && !selectedId) {
      setSelectedId(personas[0].id);
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
        "fixed inset-0 flex items-center justify-center z-[100]",
        "bg-black/60 transition-opacity duration-200",
        isOpen
          ? "opacity-100 pointer-events-auto"
          : "opacity-0 pointer-events-none",
      ].join(" ")}
      onClick={handleBackdropClick}
    >
      <div
        className={[
          "bg-bg-secondary border border-border-default rounded-[12px]",
          "w-[440px] max-w-[calc(100vw-40px)] max-h-[80vh] overflow-y-auto p-5",
          "transition-transform duration-200",
          isOpen ? "scale-100" : "scale-[0.97]",
        ].join(" ")}
      >
        <div className="text-[15px] font-semibold text-text-primary mb-4">
          Choose a persona for this chat
        </div>

        {personas.length === 0 ? (
          <div className="py-8 text-center">
            <div className="text-3xl opacity-30 mb-3">🤖</div>
            <div className="text-[13px] font-semibold text-text-secondary mb-1.5">
              No personas configured
            </div>
            <div className="text-[12px] text-text-tertiary leading-relaxed">
              Set up a persona in Settings before starting a chat.
            </div>
          </div>
        ) : (
          <div className="grid grid-cols-2 gap-2.5 mb-4 max-sm:grid-cols-1">
            {personas.map((persona) => (
              <div
                key={persona.id}
                onClick={() => setSelectedId(persona.id)}
                className={[
                  "bg-bg-tertiary rounded-[9px] p-3.5 cursor-pointer",
                  "border-2 transition-colors duration-150",
                  selectedId === persona.id
                    ? "border-accent-primary"
                    : "border-transparent hover:bg-bg-elevated",
                ].join(" ")}
              >
                <div className="text-[28px] mb-[7px]">{persona.emoji}</div>
                <div className="text-[13px] font-semibold text-text-primary">
                  {persona.name}
                </div>
                <div className="text-[11px] text-text-tertiary mt-0.5 leading-[1.45]">
                  {persona.system_prompt.length > 60
                    ? persona.system_prompt.slice(0, 60) + "…"
                    : persona.system_prompt}
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="flex justify-end gap-2 mt-1">
          <button
            onClick={onCancel}
            disabled={isLoading}
            className="px-4 py-2 rounded-[7px] text-[13px] font-medium bg-transparent text-text-secondary border border-border-default hover:bg-bg-elevated hover:text-text-primary transition-colors disabled:opacity-50 disabled:cursor-default"
          >
            Cancel
          </button>
          <button
            onClick={handleConfirm}
            disabled={!selectedId || isLoading || personas.length === 0}
            className="px-4 py-2 rounded-[7px] text-[13px] font-medium bg-accent-primary text-text-inverse hover:bg-accent-secondary transition-colors disabled:opacity-50 disabled:cursor-default"
          >
            {isLoading ? "Starting…" : "Start Chat"}
          </button>
        </div>
      </div>
    </div>
  );
}

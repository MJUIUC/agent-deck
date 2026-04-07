import { useState } from "react";
import {
  CheckmarkFilled,
  CloseFilled,
  ChevronRight,
  ChevronDown,
} from "@carbon/icons-react";
import type { ProcessingRound, ToolCallEntry } from "@/types";
import styles from "./ProcessingBlock.module.css";

// ── ProcessingBlock ──────────────────────────────────────────────────────────

interface ProcessingBlockProps {
  rounds: ProcessingRound[];
  streaming?: boolean;
}

export function ProcessingBlock({
  rounds,
  streaming = false,
}: ProcessingBlockProps) {
  const anyRoundInProgress = rounds.some((r) => r.status === "in_progress");
  // Keep the spinner active while the parent streaming phase is still live
  // (e.g. all tool rounds finished but the final text message hasn't arrived yet).
  const isActive = streaming || anyRoundInProgress;

  // null means the user hasn't manually toggled yet — follow isActive automatically.
  // Once the user clicks, their choice is stored and takes precedence.
  const [manualExpanded, setManualExpanded] = useState<boolean | null>(null);
  const expanded = manualExpanded !== null ? manualExpanded : isActive;
  const allCancelled =
    rounds.every((r) => r.status === "cancelled") && rounds.length > 0;
  let label: string;
  if (isActive) {
    const activeRound = [...rounds]
      .reverse()
      .find((r) => r.status === "in_progress");
    const currentRound = activeRound ?? rounds[rounds.length - 1];
    const toolNames = currentRound?.tools.map((t) => t.tool_name) ?? [];
    const toolLabel =
      toolNames.length === 0
        ? "Processing…"
        : toolNames.length === 1
          ? toolNames[0]
          : `${toolNames[0]} +${toolNames.length - 1}`;
    const roundLabel =
      rounds.length > 1 ? ` · Round ${currentRound?.round ?? 1}` : "";
    label = `${toolLabel}${roundLabel}`;
  } else if (allCancelled) {
    label = "Stopped";
  } else {
    label = "Processing complete";
  }

  return (
    <div className={`${styles.block} ${allCancelled ? styles.cancelled : ""}`}>
      <button
        className={styles.header}
        onClick={() => setManualExpanded((v) => (v !== null ? !v : !expanded))}
        aria-expanded={expanded}
      >
        <span className={styles.statusIcon}>
          {isActive ? (
            <span className={styles.cogSpinner} />
          ) : allCancelled ? (
            <CloseFilled size={14} />
          ) : (
            <CheckmarkFilled size={14} className={styles.done} />
          )}
        </span>
        <span className={styles.label}>{label}</span>
        <span className={styles.chevron}>
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </span>
      </button>

      {expanded && (
        <div className={styles.body}>
          {rounds.map((round) => (
            <div key={round.round} className={styles.round}>
              {rounds.length > 1 && (
                <div className={styles.roundHeader}>
                  Round {round.round}
                  {round.status === "completed" && (
                    <CheckmarkFilled size={11} className={styles.roundDone} />
                  )}
                </div>
              )}
              {round.tools.map((tool) => (
                <ToolRow
                  key={tool.tool_call_id}
                  tool={
                    round.status === "completed" &&
                    tool.status === "in_progress"
                      ? { ...tool, status: "completed" }
                      : tool
                  }
                />
              ))}
              {round.reasoning && (
                <div className={styles.reasoning}>
                  <div className={styles.reasoningLabel}>Reasoning</div>
                  <div className={styles.reasoningText}>{round.reasoning}</div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ToolRow({ tool }: { tool: ToolCallEntry }) {
  const previewValue = Object.values(tool.input_preview)[0] ?? "";
  const [resultOpen, setResultOpen] = useState(false);
  const [showFull, setShowFull] = useState(false);
  const hasResult = tool.status === "completed" && tool.result_content !== null;
  const TRUNCATE_AT = 300;
  const resultText = tool.result_content ?? "";
  const displayText =
    !showFull && resultText.length > TRUNCATE_AT
      ? resultText.slice(0, TRUNCATE_AT) + "…"
      : resultText;

  return (
    <div className={styles.toolRowWrapper}>
      <div className={styles.toolRow}>
        <span className={styles.toolStatus}>
          {tool.status === "in_progress" ? (
            <span className={styles.toolSpinner} />
          ) : tool.status === "cancelled" ? (
            <CloseFilled size={12} className={styles.mutedIcon} />
          ) : (
            <CheckmarkFilled size={12} className={styles.done} />
          )}
        </span>
        <span className={styles.toolName}>{tool.tool_name}</span>
        {previewValue && (
          <>
            <span className={styles.toolSep}>·</span>
            <span className={styles.toolPreview}>{previewValue}</span>
          </>
        )}
        {hasResult && (
          <button
            className={styles.resultToggle}
            onClick={() => setResultOpen((v) => !v)}
            aria-expanded={resultOpen}
          >
            {resultOpen ? "▼ output" : "▶ output"}
          </button>
        )}
      </div>
      {hasResult && resultOpen && (
        <div className={styles.resultSection}>
          <pre className={styles.resultContent}>{displayText}</pre>
          {resultText.length > TRUNCATE_AT && (
            <button
              className={styles.resultToggle}
              onClick={() => setShowFull((v) => !v)}
            >
              {showFull ? "show less" : "show more"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ── ProcessingBubble ─────────────────────────────────────────────────────────

interface ProcessingBubbleProps {
  rounds: ProcessingRound[];
  personaEmoji: string;
  personaName?: string;
  streaming?: boolean;
}

export function ProcessingBubble({
  rounds,
  personaEmoji,
  personaName,
  streaming = false,
}: ProcessingBubbleProps) {
  return (
    <div className={styles.row}>
      <div className={styles.avatar}>{personaEmoji}</div>
      <div className={styles.col}>
        <div className={styles.bubble}>
          <ProcessingBlock rounds={rounds} streaming={streaming} />
        </div>
        <div className={styles.bubbleMeta}>{personaName ?? "Agent"}</div>
      </div>
    </div>
  );
}

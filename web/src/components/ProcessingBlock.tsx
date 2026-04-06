import { useState } from "react";
import {
  InProgress,
  CheckmarkFilled,
  CloseFilled,
  ChevronRight,
  ChevronDown,
} from "@carbon/icons-react";
import type { ProcessingRound, ToolCallEntry } from "@/types";
import { AgentAvatar } from "./MessageBubble";
import bubbleStyles from "./MessageBubble.module.css";
import styles from "./ProcessingBlock.module.css";

// ── ProcessingBlock ──────────────────────────────────────────────────────────

interface ProcessingBlockProps {
  rounds: ProcessingRound[];
}

export function ProcessingBlock({ rounds }: ProcessingBlockProps) {
  const [expanded, setExpanded] = useState(false);

  const anyInProgress = rounds.some((r) => r.status === "in_progress");
  const allCancelled =
    rounds.every((r) => r.status === "cancelled") && rounds.length > 0;
  const totalTools = rounds.reduce((sum, r) => sum + r.tools.length, 0);
  const multiRound = rounds.length > 1;

  let label: string;
  if (anyInProgress) {
    label = "Processing…";
  } else if (allCancelled) {
    label = "Stopped";
  } else if (totalTools === 0) {
    label = "Processing";
  } else if (multiRound) {
    label = `Processing · ${totalTools} tools, ${rounds.length} rounds`;
  } else {
    label = `Processing · ${totalTools} tool${totalTools === 1 ? "" : "s"}`;
  }

  return (
    <div className={`${styles.block} ${allCancelled ? styles.cancelled : ""}`}>
      <button
        className={styles.header}
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <span className={styles.statusIcon}>
          {anyInProgress ? (
            <InProgress size={14} className={styles.spinning} />
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
          {rounds.map((round, roundIdx) => (
            <div key={round.round} className={styles.round}>
              {multiRound && (
                <div className={styles.roundHeader}>
                  Round {round.round}
                  {round.status === "completed" && (
                    <CheckmarkFilled size={11} className={styles.roundDone} />
                  )}
                </div>
              )}
              {round.tools.map((tool) => (
                <ToolRow key={tool.tool_call_id} tool={tool} />
              ))}
              {round.reasoning && roundIdx < rounds.length - 1 && (
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
  return (
    <div className={styles.toolRow}>
      <span className={styles.toolStatus}>
        {tool.status === "in_progress" ? (
          <InProgress size={12} className={styles.spinning} />
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
    </div>
  );
}

// ── ProcessingBubble ─────────────────────────────────────────────────────────

interface ProcessingBubbleProps {
  rounds: ProcessingRound[];
  personaEmoji: string;
  personaName?: string;
}

export function ProcessingBubble({
  rounds,
  personaEmoji,
  personaName,
}: ProcessingBubbleProps) {
  return (
    <div className={bubbleStyles.rowAgent}>
      <AgentAvatar emoji={personaEmoji} />
      <div className={bubbleStyles.col}>
        <div className={bubbleStyles.bubbleAgent}>
          <ProcessingBlock rounds={rounds} />
        </div>
        <div className={bubbleStyles.meta}>{personaName ?? "Agent"}</div>
      </div>
    </div>
  );
}

import type { SlashCommandResponse } from "@/types";
import { formatMessageTime } from "@/hooks/useTimeFormat";
import styles from "./EphemeralBubble.module.css";

// ── Ephemeral message types ───────────────────────────────────────────────────

/** A command echo — the raw input the user typed, e.g. "/model list" */
export interface EphemeralEcho {
  kind: "echo";
  id: string;
  input: string;
  timestamp: string;
}

/** A command result — the server's response rendered inline */
export interface EphemeralResult {
  kind: "result";
  id: string;
  response: SlashCommandResponse;
  timestamp: string;
}

/** An ephemeral error — shown when a command fails */
export interface EphemeralError {
  kind: "error";
  id: string;
  message: string;
  timestamp: string;
}

export type EphemeralMessage = EphemeralEcho | EphemeralResult | EphemeralError;

// ── Echo bubble ───────────────────────────────────────────────────────────────

function EchoBubble({ msg }: { msg: EphemeralEcho }) {
  const timeStr = formatMessageTime(msg.timestamp);
  return (
    <div className={`${styles.row} ${styles.rowUser}`}>
      <div className={styles.col}>
        <div className={`${styles.bubble} ${styles.echo}`}>
          <div className={styles.tag}>
            <span>⚡</span> Slash Command
          </div>
          <span className={styles.echoInput}>{msg.input}</span>
        </div>
        <div className={`${styles.meta} ${styles.metaUser}`}>
          {timeStr} · <em>ephemeral — not saved</em>
        </div>
      </div>
    </div>
  );
}

// ── Result content renderers ──────────────────────────────────────────────────

function ModelListContent({ payload }: { payload: SlashCommandResponse["payload"] }) {
  const models = payload?.models as Array<{ id: string; display_name: string }> | undefined;
  if (!models || models.length === 0) {
    return <p className={styles.resultText}>No models available.</p>;
  }
  return (
    <ul className={styles.resultList}>
      {models.map((m) => (
        <li key={m.id} className={styles.resultItem}>
          <span className={styles.resultName}>{m.display_name}</span>
          <code className={styles.resultId}>{m.id}</code>
        </li>
      ))}
    </ul>
  );
}

function ModelSwitchedContent({ response }: { response: SlashCommandResponse }) {
  return (
    <div>
      <div className={styles.resultHeader}>
        <span className={styles.resultBadge}>Model changed</span>
        <span className={styles.resultTitle}>
          {String(response.payload?.display_name ?? "")}
        </span>
      </div>
      <p className={styles.resultText}>{response.message}</p>
    </div>
  );
}

function RoutineListContent({ payload }: { payload: SlashCommandResponse["payload"] }) {
  const routines = payload?.routines as Array<{
    id: string;
    name: string;
    cron_expr: string;
    enabled: boolean;
  }> | undefined;
  if (!routines || routines.length === 0) {
    return <p className={styles.resultText}>No routines attached to this thread.</p>;
  }
  return (
    <ul className={styles.resultList}>
      {routines.map((r) => (
        <li key={r.id} className={styles.resultItem}>
          <span className={styles.resultName}>{r.name}</span>
          <code className={styles.resultId}>{r.cron_expr}</code>
          {!r.enabled && (
            <span className={styles.resultDisabled}>disabled</span>
          )}
        </li>
      ))}
    </ul>
  );
}

function MemoryListContent({ response }: { response: SlashCommandResponse }) {
  const memories = response.payload?.memories as Array<{
    id: string;
    content: string;
    thread_title?: string;
    created_at: string;
  }> | undefined;
  if (!memories || memories.length === 0) {
    return <p className={styles.resultText}>No memories found for this persona.</p>;
  }
  return (
    <div>
      <p className={styles.resultText}>{response.message}</p>
      <ul className={styles.memoryList}>
        {memories.map((m) => (
          <li key={m.id} className={styles.memoryItem}>
            <em className={styles.memoryContent}>"{m.content}"</em>
            {m.thread_title && (
              <span className={styles.memoryMeta}> — {m.thread_title}</span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function HelpContent({ payload }: { payload: SlashCommandResponse["payload"] }) {
  const commands = payload?.commands as Array<{
    command: string;
    description: string;
  }> | undefined;
  if (!commands || commands.length === 0) {
    return <p className={styles.resultText}>No commands available.</p>;
  }
  return (
    <ul className={styles.resultList}>
      {commands.map((c) => (
        <li key={c.command} className={styles.resultItem}>
          <code className={styles.resultCmd}>{c.command}</code>
          <span className={styles.resultDesc}>{c.description}</span>
        </li>
      ))}
    </ul>
  );
}

function DefaultContent({ response }: { response: SlashCommandResponse }) {
  return <p className={styles.resultText}>{response.message}</p>;
}

// ── Result bubble ─────────────────────────────────────────────────────────────

function ResultBubble({ msg }: { msg: EphemeralResult }) {
  const timeStr = formatMessageTime(msg.timestamp);
  const { response } = msg;

  const content = (() => {
    switch (response.type) {
      case "model_list":
        return <ModelListContent payload={response.payload} />;
      case "model_switched":
        return <ModelSwitchedContent response={response} />;
      case "routine_list":
        return <RoutineListContent payload={response.payload} />;
      case "open_add_routine_modal":
        return <DefaultContent response={response} />;
      case "memory_list":
        return <MemoryListContent response={response} />;
      case "help":
        return <HelpContent payload={response.payload} />;
      default:
        return <DefaultContent response={response} />;
    }
  })();

  return (
    <div className={`${styles.row} ${styles.rowResult}`}>
      <div className={`${styles.avatar} ${styles.avatarResult}`}>⚙</div>
      <div className={styles.col}>
        <div className={`${styles.bubble} ${styles.result}`}>
          {content}
        </div>
        <div className={styles.meta}>
          {timeStr} · <em>ephemeral</em>
        </div>
      </div>
    </div>
  );
}

// ── Error bubble ──────────────────────────────────────────────────────────────

function ErrorBubble({ msg }: { msg: EphemeralError }) {
  const timeStr = formatMessageTime(msg.timestamp);
  return (
    <div className={`${styles.row} ${styles.rowResult}`}>
      <div className={`${styles.avatar} ${styles.avatarError}`}>✕</div>
      <div className={styles.col}>
        <div className={`${styles.bubble} ${styles.error}`}>
          <div className={styles.tag}>
            <span>⚠</span> Command Error
          </div>
          <p className={styles.resultText}>{msg.message}</p>
        </div>
        <div className={styles.meta}>
          {timeStr} · <em>ephemeral</em>
        </div>
      </div>
    </div>
  );
}

// ── Public export ─────────────────────────────────────────────────────────────

export function EphemeralBubble({ msg }: { msg: EphemeralMessage }) {
  if (msg.kind === "echo") return <EchoBubble msg={msg} />;
  if (msg.kind === "result") return <ResultBubble msg={msg} />;
  return <ErrorBubble msg={msg} />;
}

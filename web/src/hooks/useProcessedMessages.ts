import { useMemo } from "react";
import type { Message, ProcessingRound, ToolCallEntry } from "@/types";
import { formatDateDivider } from "@/hooks/useTimeFormat";

export type ProcessedItem =
  | { type: "date_divider"; label: string; key: string }
  | { type: "message"; message: Message }
  | { type: "tool_group"; executionId: string; rounds: ProcessingRound[] };

export function useProcessedMessages(visibleMessages: Message[]): ProcessedItem[] {
  return useMemo((): ProcessedItem[] => {
    // First pass: group tool messages into runs, then add date dividers.
    //
    // Messages WITH execution_id are pre-collected by that id so that
    // chat_segment entries between rounds don't split the group.
    //
    // Messages WITHOUT execution_id (runs from before the execution_id
    // backend fix) are assigned a synthetic slot key: a contiguous run of
    // source:"tool" messages, possibly separated only by chat_segment entries,
    // all share one slot key and collapse into a single ProcessingBlock.
    type RawItem =
      | { type: "message"; message: Message }
      | { type: "tool_group"; executionId: string; rounds: ProcessingRound[] };

    const toolMsgsByExecId = new Map<string, Message[]>();
    const nullSlotKeys = new Map<string, string>(); // msg.id → slot key
    // Reasoning text collected from inter-round chat_segment messages,
    // keyed by the same slot/execution_id used for tool grouping.
    const reasoningByKey = new Map<string, string>();
    // IDs of chat_segments that follow tool messages in the same run —
    // these are suppressed as standalone bubbles and shown only inside
    // the ProcessingBlock.
    const interRoundSegIds = new Set<string>();
    let slotCounter = 0;
    let openSlotKey: string | null = null;
    let openSlotSeenTool = false;
    // Track which execution_ids have already received at least one tool message
    const execIdsWithTools = new Set<string>();

    for (const m of visibleMessages) {
      if (m.source === "tool") {
        let key: string;
        if (m.execution_id) {
          key = m.execution_id;
          execIdsWithTools.add(key);
          openSlotKey = null;
          openSlotSeenTool = false;
        } else {
          if (openSlotKey === null) {
            openSlotKey = `__slot_${slotCounter++}`;
          }
          key = openSlotKey;
          nullSlotKeys.set(m.id, key);
          openSlotSeenTool = true;
        }
        const arr = toolMsgsByExecId.get(key);
        if (arr) arr.push(m);
        else toolMsgsByExecId.set(key, [m]);
      } else if (m.event_type === "chat_segment") {
        // Classify as inter-round if it follows at least one tool message
        // in the same run; keep the current slot open either way.
        if (m.execution_id && execIdsWithTools.has(m.execution_id)) {
          interRoundSegIds.add(m.id);
          reasoningByKey.set(
            m.execution_id,
            (
              (reasoningByKey.get(m.execution_id) ?? "") +
              "\n" +
              m.content
            ).trim(),
          );
        } else if (!m.execution_id && openSlotKey && openSlotSeenTool) {
          interRoundSegIds.add(m.id);
          reasoningByKey.set(
            openSlotKey,
            ((reasoningByKey.get(openSlotKey) ?? "") + "\n" + m.content).trim(),
          );
        }
      } else {
        // User message or final assistant text: close the current null slot.
        openSlotKey = null;
        openSlotSeenTool = false;
      }
    }

    const emittedExecIds = new Set<string>();
    const rawItems: RawItem[] = [];

    for (const msg of visibleMessages) {
      if (msg.source === "tool") {
        const key = msg.execution_id ?? nullSlotKeys.get(msg.id) ?? msg.id;
        if (emittedExecIds.has(key)) continue;
        emittedExecIds.add(key);

        const group = toolMsgsByExecId.get(key) ?? [];
        const calls = group.filter((m) => m.role === "assistant");
        const results = group.filter((m) => (m.role as string) === "tool");
        const roundEntries = calls.map((call, idx): ToolCallEntry => {
          const nameMatch = call.content.match(/\*\*Tool call:\*\* `([^`]+)`/);
          const toolName = nameMatch?.[1] ?? "tool";
          const result = results[idx] ?? null;
          return {
            tool_call_id: call.id,
            tool_name: toolName,
            input_preview: {},
            status: "completed",
            call_message_id: call.id,
            result_message_id: result?.id ?? null,
            result_content: result?.content ?? null,
          };
        });
        const reasoning = reasoningByKey.get(key) ?? "";
        const rounds: ProcessingRound[] =
          roundEntries.length > 0
            ? [
                {
                  round: 1,
                  tools: roundEntries,
                  status: "completed",
                  reasoning,
                },
              ]
            : [];
        rawItems.push({ type: "tool_group", executionId: key, rounds });
      } else if (interRoundSegIds.has(msg.id)) {
        // Suppress — content is shown as reasoning inside the ProcessingBlock.
        continue;
      } else {
        rawItems.push({ type: "message", message: msg });
      }
    }

    // Second pass: insert date dividers before the first message of each new day
    const result: ProcessedItem[] = [];
    let lastDateKey = "";
    for (const item of rawItems) {
      if (item.type === "message") {
        const d = new Date(item.message.created_at);
        const dateKey = isNaN(d.getTime())
          ? ""
          : `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
        if (dateKey !== lastDateKey) {
          lastDateKey = dateKey;
          result.push({
            type: "date_divider",
            label: formatDateDivider(item.message.created_at),
            key: dateKey,
          });
        }
        result.push(item);
      } else {
        result.push(item);
      }
    }
    return result;
  }, [visibleMessages]);
}

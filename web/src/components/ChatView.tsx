import { useEffect, useState, useCallback } from "react";
import type { Thread } from "@/types";
import { useMessageStore } from "@/stores/useMessageStore";
import { useSseStore } from "@/stores/useSseStore";
import { useThreadStore } from "@/stores/useThreadStore";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { groupByDate } from "@/hooks/useTimeFormat";
import { MessageBubble, StreamingBubble } from "./MessageBubble";
import { MessageInput } from "./MessageInput";
import { ChatHeader } from "./ChatHeader";
import { ConfigPane } from "./ConfigPane";

interface ChatViewProps {
  thread: Thread;
  onMobileMenuOpen?: () => void;
}

export function ChatView({ thread, onMobileMenuOpen }: ChatViewProps) {
  const [configOpen, setConfigOpen] = useState(false);

  const messages = useMessageStore((s) => s.messagesByThread[thread.id] ?? []);
  const streamingContent = useMessageStore(
    (s) => s.streamingContent[thread.id] ?? "",
  );
  const isStreaming = useMessageStore((s) => s.isStreaming[thread.id] ?? false);
  const isSending = useMessageStore((s) => s.isSending[thread.id] ?? false);
  const isLoadingMessages = useMessageStore((s) => s.isLoadingMessages);
  const messageError = useMessageStore((s) => s.error);
  const loadMessages = useMessageStore((s) => s.loadMessages);
  const sendMessage = useMessageStore((s) => s.sendMessage);
  const sendCommand = useMessageStore((s) => s.sendCommand);

  const connectThread = useSseStore((s) => s.connectThread);
  const disconnectThread = useSseStore((s) => s.disconnectThread);

  const upsertThread = useThreadStore((s) => s.upsertThread);

  useEffect(() => {
    loadMessages(thread.id);
    connectThread(thread.id);
    return () => {
      disconnectThread();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thread.id]);

  const { containerRef } = useAutoScroll([
    thread.id,
    messages.length,
    streamingContent,
  ]);

  const visibleMessages = messages.filter((m) => m.visibility !== "hidden");
  const grouped = groupByDate(visibleMessages);

  const persona = thread.persona;
  const personaEmoji = persona?.emoji ?? "🤖";
  const personaName = persona?.name ?? "Agent";

  const handleSend = useCallback(
    (content: string) => sendMessage(thread.id, content),
    [thread.id, sendMessage],
  );

  const handleCommand = useCallback(
    (input: string) => sendCommand(thread.id, input),
    [thread.id, sendCommand],
  );

  const handleThreadUpdated = useCallback(
    (updated: Thread) => {
      upsertThread({ ...updated, persona: thread.persona });
    },
    [upsertThread, thread.persona],
  );

  const modelName = thread.active_model ?? undefined;

  return (
    <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
      {/* ── Header ── */}
      <ChatHeader
        thread={thread}
        onToggleConfig={() => setConfigOpen((o) => !o)}
        onMobileMenuOpen={onMobileMenuOpen}
      />

      {/* ── Messages area ── */}
      <div
        ref={containerRef}
        className="flex-1 overflow-y-auto scrollbar-thin flex flex-col pt-6 pb-3"
      >
        {isLoadingMessages ? (
          <div className="flex-1 flex items-center justify-center text-text-tertiary text-[13px]">
            Loading messages…
          </div>
        ) : visibleMessages.length === 0 && !isStreaming ? (
          /* Empty thread */
          <div className="flex-1 flex flex-col items-center justify-center gap-3 px-10">
            <div className="text-[40px] opacity-20">{personaEmoji}</div>
            <div className="text-[15px] font-semibold text-text-secondary">
              Start a conversation with {personaName}
            </div>
            <div className="text-[13px] text-text-tertiary text-center max-w-[320px] leading-relaxed">
              Send a message below to begin. Use{" "}
              <code className="bg-bg-elevated px-[5px] py-px rounded-[3px] font-mono text-[12px]">
                /help
              </code>{" "}
              to see available slash commands.
            </div>
          </div>
        ) : (
          <>
            {grouped.map(({ dateLabel, items }) => (
              <div key={dateLabel}>
                {/* Date divider */}
                <div className="date-divider">{dateLabel}</div>

                {items.map((message) => (
                  <MessageBubble
                    key={message.id}
                    message={message}
                    personaEmoji={personaEmoji}
                    personaName={personaName}
                  />
                ))}
              </div>
            ))}

            {/* Streaming bubble */}
            {isStreaming && (
              <StreamingBubble
                personaEmoji={personaEmoji}
                personaName={personaName}
                content={streamingContent}
              />
            )}
          </>
        )}

        {/* Error banner */}
        {messageError && !isStreaming && (
          <div className="mx-[18px] my-2 px-3.5 py-2.5 bg-error/10 border border-error/30 rounded-[8px] text-[13px] text-error leading-snug">
            ⚠ {messageError}
          </div>
        )}
      </div>

      {/* ── Input bar ── */}
      <MessageInput
        threadId={thread.id}
        personaName={personaName}
        modelName={modelName}
        isSending={isSending || isStreaming}
        onSend={handleSend}
        onCommand={handleCommand}
      />

      {/* ── Config pane (slides in from right) ── */}
      <ConfigPane
        isOpen={configOpen}
        thread={thread}
        onClose={() => setConfigOpen(false)}
        onThreadUpdated={handleThreadUpdated}
      />
    </div>
  );
}

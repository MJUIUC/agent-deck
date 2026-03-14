// ── Token buffer ──────────────────────────────────────────────────────────────
// Accumulates incoming SSE tokens and flushes them in a single batch on the
// next animation frame. This keeps React re-renders at display rate (~60fps)
// regardless of how fast the server streams tokens, and prevents ReactMarkdown
// from re-parsing the growing content on every individual token.
//
// Kept in its own module so useSseStore and useMessageStore can both reference
// it without creating a circular import between the two store files.

const tokenBuffers = new Map<string, string>();
const rafHandles = new Map<string, number>();

type FlushCallback = (threadId: string, buffered: string) => void;

export function bufferToken(
  threadId: string,
  token: string,
  onFlush: FlushCallback,
): void {
  tokenBuffers.set(threadId, (tokenBuffers.get(threadId) ?? "") + token);

  if (!rafHandles.has(threadId)) {
    const handle = requestAnimationFrame(() => {
      rafHandles.delete(threadId);
      const buffered = tokenBuffers.get(threadId) ?? "";
      tokenBuffers.delete(threadId);
      if (buffered) {
        onFlush(threadId, buffered);
      }
    });
    rafHandles.set(threadId, handle);
  }
}

// Cancel any pending RAF flush and discard buffered tokens for a thread.
// Called by finalizeStream so that leftover tokens from the last animation
// frame do not re-open the streaming phase after message_complete arrives.
export function cancelTokenBuffer(threadId: string): void {
  const handle = rafHandles.get(threadId);
  if (handle !== undefined) {
    cancelAnimationFrame(handle);
    rafHandles.delete(threadId);
  }
  tokenBuffers.delete(threadId);
}

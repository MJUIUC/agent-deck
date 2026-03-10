import { useEffect, useRef, useCallback } from "react";

/**
 * useAutoScroll — keeps a scrollable container pinned to the bottom
 * as new content arrives, unless the user has manually scrolled up.
 *
 * Returns a ref to attach to the scrollable container.
 */
export function useAutoScroll(deps: unknown[]) {
  const containerRef = useRef<HTMLDivElement>(null);
  const isPinnedRef = useRef(true);
  const isUserScrollingRef = useRef(false);

  const scrollToBottom = useCallback((smooth = false) => {
    const el = containerRef.current;
    if (!el) return;
    el.scrollTo({
      top: el.scrollHeight,
      behavior: smooth ? "smooth" : "instant",
    });
  }, []);

  // Detect when the user scrolls away from the bottom
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const handleScroll = () => {
      const distanceFromBottom =
        el.scrollHeight - el.scrollTop - el.clientHeight;
      // Re-pin if within 80px of the bottom
      isPinnedRef.current = distanceFromBottom < 80;
    };

    el.addEventListener("scroll", handleScroll, { passive: true });
    return () => el.removeEventListener("scroll", handleScroll);
  }, []);

  // Scroll to bottom when deps change (new messages / tokens)
  useEffect(() => {
    if (isPinnedRef.current) {
      scrollToBottom(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  // Jump to bottom immediately when the thread changes (first element of deps)
  const prevThreadIdRef = useRef<unknown>(null);
  useEffect(() => {
    const threadId = deps[0];
    if (threadId !== prevThreadIdRef.current) {
      prevThreadIdRef.current = threadId;
      isPinnedRef.current = true;
      // Small delay to let the DOM render the messages first
      requestAnimationFrame(() => scrollToBottom(false));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deps[0]]);

  return { containerRef, scrollToBottom, isUserScrollingRef };
}

import { useEffect, useRef, useCallback } from "react";

/**
 * useAutoScroll — keeps a scrollable container pinned to the bottom
 * as new content arrives, unless the user has manually scrolled up.
 *
 * Thread-switch / initial-load scrolling is handled by ChatView directly
 * via a sentinel div + scrollIntoView. This hook only concerns itself with
 * keeping the view pinned during streaming.
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

  // Scroll to bottom when deps change, but only while pinned
  useEffect(() => {
    if (isPinnedRef.current) {
      scrollToBottom(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { containerRef, scrollToBottom, isPinnedRef, isUserScrollingRef };
}

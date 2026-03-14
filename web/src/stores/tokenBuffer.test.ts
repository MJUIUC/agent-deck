import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";
import { bufferToken, cancelTokenBuffer } from "./tokenBuffer";

// ── Fake timers (controls requestAnimationFrame / cancelAnimationFrame) ────────

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Advance fake timers so all pending rAF callbacks fire. */
function flushRaf(): void {
  vi.runAllTimers();
}

// ── bufferToken ───────────────────────────────────────────────────────────────

describe("bufferToken", () => {
  it("concatenates multiple tokens and flushes them as a single string", () => {
    const onFlush = vi.fn();
    const threadId = "thread-concat";

    bufferToken(threadId, "Hello", onFlush);
    bufferToken(threadId, ", ", onFlush);
    bufferToken(threadId, "world", onFlush);
    bufferToken(threadId, "!", onFlush);

    flushRaf();

    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith(threadId, "Hello, world!");
  });

  it("calls onFlush with the correct threadId and full concatenated string", () => {
    const onFlush = vi.fn();
    const threadId = "thread-callback-args";

    bufferToken(threadId, "foo", onFlush);
    bufferToken(threadId, "bar", onFlush);

    flushRaf();

    expect(onFlush).toHaveBeenCalledWith("thread-callback-args", "foobar");
  });

  it("schedules only one RAF regardless of how many tokens arrive before the flush", () => {
    const rafSpy = vi.spyOn(window, "requestAnimationFrame");
    const onFlush = vi.fn();
    const threadId = "thread-single-raf";

    bufferToken(threadId, "a", onFlush);
    bufferToken(threadId, "b", onFlush);
    bufferToken(threadId, "c", onFlush);
    bufferToken(threadId, "d", onFlush);

    expect(rafSpy).toHaveBeenCalledTimes(1);

    flushRaf();

    rafSpy.mockRestore();
  });

  it("buffers and flushes tokens for different threads independently", () => {
    const onFlushA = vi.fn();
    const onFlushB = vi.fn();
    const threadA = "thread-independent-a";
    const threadB = "thread-independent-b";

    bufferToken(threadA, "Hello from A", onFlushA);
    bufferToken(threadB, "Hello from B", onFlushB);
    bufferToken(threadA, " — extra A", onFlushA);

    flushRaf();

    expect(onFlushA).toHaveBeenCalledTimes(1);
    expect(onFlushA).toHaveBeenCalledWith(threadA, "Hello from A — extra A");

    expect(onFlushB).toHaveBeenCalledTimes(1);
    expect(onFlushB).toHaveBeenCalledWith(threadB, "Hello from B");
  });

  it("schedules a fresh RAF after the previous one has already fired", () => {
    const onFlush = vi.fn();
    const threadId = "thread-new-raf-after-flush";

    bufferToken(threadId, "first", onFlush);
    flushRaf();

    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith(threadId, "first");

    // The handle should have been cleaned up — a new token must schedule a new RAF.
    const rafSpy = vi.spyOn(window, "requestAnimationFrame");
    bufferToken(threadId, "second", onFlush);

    expect(rafSpy).toHaveBeenCalledTimes(1);

    flushRaf();

    expect(onFlush).toHaveBeenCalledTimes(2);
    expect(onFlush).toHaveBeenLastCalledWith(threadId, "second");

    rafSpy.mockRestore();
  });

  it("does not call onFlush when the buffered string is empty", () => {
    // This can't happen through the public API (empty string tokens are
    // valid but result in an empty buffer only if that's all that was added),
    // but we verify the guard branch: if somehow the buffer is empty the
    // callback is skipped.
    const onFlush = vi.fn();
    const threadId = "thread-empty-token";

    // Buffering an empty string leaves the buffer as "".
    bufferToken(threadId, "", onFlush);

    flushRaf();

    expect(onFlush).not.toHaveBeenCalled();
  });
});

// ── cancelTokenBuffer ─────────────────────────────────────────────────────────

describe("cancelTokenBuffer", () => {
  it("prevents onFlush from being called when cancelled before the RAF fires", () => {
    const onFlush = vi.fn();
    const threadId = "thread-cancel-before-flush";

    bufferToken(threadId, "should not flush", onFlush);

    cancelTokenBuffer(threadId);
    flushRaf();

    expect(onFlush).not.toHaveBeenCalled();
  });

  it("clears the buffer so a subsequent bufferToken call starts fresh", () => {
    const onFlush = vi.fn();
    const threadId = "thread-fresh-after-cancel";

    bufferToken(threadId, "stale data", onFlush);
    cancelTokenBuffer(threadId);

    // Now buffer a new token — the stale data must not be included.
    bufferToken(threadId, "fresh", onFlush);
    flushRaf();

    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith(threadId, "fresh");
  });

  it("cancels the RAF so only one cancelAnimationFrame call is made", () => {
    const cancelSpy = vi.spyOn(window, "cancelAnimationFrame");
    const onFlush = vi.fn();
    const threadId = "thread-cancel-raf-spy";

    bufferToken(threadId, "token", onFlush);
    cancelTokenBuffer(threadId);

    expect(cancelSpy).toHaveBeenCalledTimes(1);

    cancelSpy.mockRestore();
  });

  it("is a no-op when called for a thread that has no pending buffer", () => {
    const cancelSpy = vi.spyOn(window, "cancelAnimationFrame");

    // No bufferToken call for this thread — cancelling should not throw and
    // should not invoke cancelAnimationFrame.
    expect(() => cancelTokenBuffer("thread-noop-cancel")).not.toThrow();
    expect(cancelSpy).not.toHaveBeenCalled();

    cancelSpy.mockRestore();
  });

  it("does not affect pending buffers for other threads", () => {
    const onFlushA = vi.fn();
    const onFlushB = vi.fn();
    const threadA = "thread-cancel-isolation-a";
    const threadB = "thread-cancel-isolation-b";

    bufferToken(threadA, "cancel me", onFlushA);
    bufferToken(threadB, "keep me", onFlushB);

    cancelTokenBuffer(threadA);
    flushRaf();

    expect(onFlushA).not.toHaveBeenCalled();
    expect(onFlushB).toHaveBeenCalledTimes(1);
    expect(onFlushB).toHaveBeenCalledWith(threadB, "keep me");
  });
});

import { vi, describe, it, expect, beforeEach } from "vitest";
import type { Thread } from "@/types";

// ── Mock the API module before importing the store ────────────────────────────
// archiveThread calls threadsApi.archive(); we don't want real HTTP in unit tests.

vi.mock("@/api/client", () => ({
  threadsApi: {
    list: vi.fn(),
    archive: vi.fn().mockResolvedValue({ data: { id: "t1", status: "archived" } }),
  },
  personasApi: {
    list: vi.fn(),
  },
}));

import { useThreadStore } from "./useThreadStore";

// ── Fixtures ──────────────────────────────────────────────────────────────────

function makeThread(id: string, updatedAt: string): Thread {
  return {
    id,
    user_id: "u1",
    persona_id: "p1",
    title: `Thread ${id}`,
    active_model: null,
    active_provider: null,
    system_prompt_addendum: null,
    status: "active",
    show_tool_activity: false,
    created_at: "2024-01-01T00:00:00.000Z",
    updated_at: updatedAt,
  };
}

// ── Reset store state before each test ───────────────────────────────────────
// Zustand exposes setState on the store object directly; we use it to seed
// a known state without going through any async actions.

beforeEach(() => {
  useThreadStore.setState({
    threads: [],
    personas: [],
    activeThreadId: null,
    isLoading: false,
    isCreating: false,
    error: null,
  });
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("archiveThread", () => {
  it("removes the archived thread and switches active to the next most-recent thread", async () => {
    // t2 is more recently updated, so it sits at index 0 in the sorted list.
    const t1 = makeThread("t1", "2024-06-01T10:00:00.000Z");
    const t2 = makeThread("t2", "2024-06-02T10:00:00.000Z");

    useThreadStore.setState({ threads: [t2, t1], activeThreadId: "t1" });

    await useThreadStore.getState().archiveThread("t1");

    const { threads, activeThreadId } = useThreadStore.getState();

    expect(threads).toHaveLength(1);
    expect(threads[0].id).toBe("t2");
    expect(activeThreadId).toBe("t2");
  });

  it("sets activeThreadId to null when the last thread is archived", async () => {
    const t1 = makeThread("t1", "2024-06-01T10:00:00.000Z");

    useThreadStore.setState({ threads: [t1], activeThreadId: "t1" });

    await useThreadStore.getState().archiveThread("t1");

    const { threads, activeThreadId } = useThreadStore.getState();

    expect(threads).toHaveLength(0);
    expect(activeThreadId).toBeNull();
  });
});

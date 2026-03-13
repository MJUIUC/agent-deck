import { create } from "zustand";
import { threadsApi, personasApi } from "@/api/client";
import type { Thread, AgentPersona } from "@/types";

interface ThreadStore {
  // State
  threads: Thread[];
  personas: AgentPersona[];
  activeThreadId: string | null;
  isLoading: boolean;
  isCreating: boolean;
  error: string | null;

  // Derived
  activeThread: Thread | null;

  // Actions
  loadThreads: () => Promise<void>;
  loadPersonas: () => Promise<void>;
  setActiveThread: (threadId: string | null) => void;
  createThread: (personaId: string) => Promise<Thread>;
  archiveThread: (threadId: string) => Promise<void>;
  updateThreadPreview: (
    threadId: string,
    preview: string,
    updatedAt: string,
  ) => void;
  upsertThread: (thread: Thread) => void;
  clearError: () => void;
}

export const useThreadStore = create<ThreadStore>((set, get) => ({
  // ── Initial state ───────────────────────────────────────────────────────────
  threads: [],
  personas: [],
  activeThreadId: null,
  isLoading: false,
  isCreating: false,
  error: null,

  // ── Derived ─────────────────────────────────────────────────────────────────
  get activeThread() {
    const { threads, activeThreadId } = get();
    return threads.find((t) => t.id === activeThreadId) ?? null;
  },

  // ── Actions ─────────────────────────────────────────────────────────────────

  loadThreads: async () => {
    set({ isLoading: true, error: null });
    try {
      const [threadsRes, personasRes] = await Promise.all([
        threadsApi.list("active"),
        personasApi.list(),
      ]);

      const personaMap = new Map(personasRes.data.map((p) => [p.id, p]));

      // Join persona data onto each thread for display convenience
      const threadsWithPersonas = threadsRes.data.map((t) => ({
        ...t,
        persona: personaMap.get(t.persona_id),
      }));

      set({
        threads: threadsWithPersonas,
        personas: personasRes.data,
        isLoading: false,
      });
    } catch (err) {
      set({
        isLoading: false,
        error: err instanceof Error ? err.message : "Failed to load threads",
      });
    }
  },

  loadPersonas: async () => {
    try {
      const res = await personasApi.list();
      set({ personas: res.data });
    } catch (err) {
      set({
        error: err instanceof Error ? err.message : "Failed to load personas",
      });
    }
  },

  setActiveThread: (threadId) => {
    set({ activeThreadId: threadId });
  },

  createThread: async (personaId) => {
    set({ isCreating: true, error: null });
    try {
      const { personas } = get();
      const persona = personas.find((p) => p.id === personaId);

      const res = await threadsApi.create({ persona_id: personaId });
      const newThread: Thread = {
        ...res.data,
        persona,
      };

      set((state) => ({
        threads: [newThread, ...state.threads],
        isCreating: false,
      }));

      return newThread;
    } catch (err) {
      set({
        isCreating: false,
        error: err instanceof Error ? err.message : "Failed to create thread",
      });
      throw err;
    }
  },

  archiveThread: async (threadId) => {
    await threadsApi.archive(threadId);
    set((state) => {
      const remaining = state.threads.filter((t) => t.id !== threadId);
      // Pick the next active thread: the most recently updated one after removal,
      // or null if none remain.
      const nextActive =
        state.activeThreadId === threadId
          ? (remaining[0]?.id ?? null)
          : state.activeThreadId;
      return { threads: remaining, activeThreadId: nextActive };
    });
  },

  updateThreadPreview: (threadId, preview, updatedAt) => {
    set((state) => {
      const threads = state.threads.map((t) =>
        t.id === threadId
          ? { ...t, last_message_preview: preview, updated_at: updatedAt }
          : t,
      );

      // Re-sort: most recently updated first
      threads.sort(
        (a, b) =>
          new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      );

      return { threads };
    });
  },

  upsertThread: (thread) => {
    set((state) => {
      const existing = state.threads.find((t) => t.id === thread.id);
      const persona =
        thread.persona ??
        state.personas.find((p) => p.id === thread.persona_id);
      const enriched = { ...thread, persona };

      let threads: Thread[];
      if (existing) {
        threads = state.threads.map((t) => (t.id === thread.id ? enriched : t));
      } else {
        threads = [enriched, ...state.threads];
      }

      // Re-sort
      threads.sort(
        (a, b) =>
          new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      );

      return { threads };
    });
  },

  clearError: () => set({ error: null }),
}));

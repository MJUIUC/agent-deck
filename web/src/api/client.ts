// ── Typed API client — wraps all server endpoints ────────────────────────────

import type {
  AgentPersona,
  Thread,
  Message,
  Provider,
  Model,
  SlashCommandResponse,
} from "@/types";

// Base fetch helper — throws on non-OK responses with the error body
async function apiFetch<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const res = await fetch(path, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers ?? {}),
    },
  });

  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`;
    try {
      const body = await res.json();
      if (body?.error) message = body.error;
    } catch {
      // ignore parse errors
    }
    throw new Error(message);
  }

  return res.json() as Promise<T>;
}

// ── Personas ──────────────────────────────────────────────────────────────────

export const personasApi = {
  list(): Promise<{ data: AgentPersona[] }> {
    return apiFetch("/api/personas");
  },

  get(id: string): Promise<{ data: AgentPersona }> {
    return apiFetch(`/api/personas/${id}`);
  },

  create(payload: {
    name: string;
    emoji: string;
    system_prompt: string;
    default_model?: string;
    default_provider?: string;
  }): Promise<{ data: AgentPersona }> {
    return apiFetch("/api/personas", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },
};

// ── Threads ───────────────────────────────────────────────────────────────────

export const threadsApi = {
  list(status = "active"): Promise<{ data: Thread[] }> {
    return apiFetch(`/api/threads?status=${status}`);
  },

  get(id: string): Promise<{ data: Thread }> {
    return apiFetch(`/api/threads/${id}`);
  },

  create(payload: {
    persona_id: string;
    title?: string;
    active_model?: string;
    active_provider?: string;
  }): Promise<{ data: Thread }> {
    return apiFetch("/api/threads", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },

  update(
    id: string,
    payload: {
      title?: string;
      active_model?: string;
      active_provider?: string;
      system_prompt_addendum?: string;
    }
  ): Promise<{ data: Thread }> {
    return apiFetch(`/api/threads/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  archive(id: string): Promise<{ data: { id: string; status: string } }> {
    return apiFetch(`/api/threads/${id}/archive`, { method: "POST" });
  },
};

// ── Messages ──────────────────────────────────────────────────────────────────

export const messagesApi = {
  list(
    threadId: string,
    opts: { limit?: number; before?: string } = {}
  ): Promise<{ data: Message[] }> {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before) params.set("before", opts.before);
    const qs = params.toString();
    return apiFetch(`/api/threads/${threadId}/messages${qs ? `?${qs}` : ""}`);
  },

  send(
    threadId: string,
    content: string
  ): Promise<{ data: Message }> {
    return apiFetch(`/api/threads/${threadId}/messages`, {
      method: "POST",
      body: JSON.stringify({ content }),
    });
  },

  sendCommand(
    threadId: string,
    command: string,
    args?: string
  ): Promise<{ data: SlashCommandResponse }> {
    return apiFetch(`/api/threads/${threadId}/command`, {
      method: "POST",
      body: JSON.stringify({ command, args: args ?? "" }),
    });
  },
};

// ── Providers ─────────────────────────────────────────────────────────────────

export const providersApi = {
  list(): Promise<{ data: Provider[] }> {
    return apiFetch("/api/providers");
  },
};

// ── Models ────────────────────────────────────────────────────────────────────

export const modelsApi = {
  list(providerId: string): Promise<{ data: Model[] }> {
    return apiFetch(`/api/providers/${providerId}/models`);
  },
};

// ── Setup ─────────────────────────────────────────────────────────────────────

export const setupApi = {
  status(): Promise<{ data: { complete: boolean } }> {
    return apiFetch("/api/setup/status");
  },
};

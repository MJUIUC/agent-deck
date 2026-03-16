// ── Typed API client — wraps all server endpoints ────────────────────────────

import type {
  AgentPersona,
  Thread,
  Message,
  Provider,
  Model,
  McpServer,
  McpTool,
  SlashCommandResponse,
} from "@/types";

// ── Credential types ──────────────────────────────────────────────────────────

export type CredentialType =
  | "api_key"
  | "pat"
  | "bearer_token"
  | "key_secret_pair"
  | "service_account";

export interface Credential {
  id: string;
  key: string;
  display_name: string;
  service: string;
  credential_type: CredentialType;
  service_url?: string;
  username?: string;
  email?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateCredentialPayload {
  key: string;
  display_name: string;
  credential_type: CredentialType;
  service_url?: string;
  username?: string;
  email?: string;
  secret?: string;
  password?: string;
}

export interface UpdateCredentialPayload {
  display_name?: string;
  service?: string;
  credential_type?: CredentialType;
  service_url?: string;
  username?: string;
  email?: string;
  secret?: string;
  password?: string;
}

// Base fetch helper — throws on non-OK responses with the error body
async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
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

  // 204 No Content (and any other empty response) has no body — parsing it
  // throws, which silently breaks callers. Return null instead.
  const contentLength = res.headers.get("content-length");
  const contentType = res.headers.get("content-type") ?? "";
  if (
    res.status === 204 ||
    contentLength === "0" ||
    !contentType.includes("application/json")
  ) {
    return null as T;
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

  update(
    id: string,
    payload: {
      name?: string;
      emoji?: string;
      system_prompt?: string;
      default_model?: string;
      default_provider?: string;
    },
  ): Promise<{ data: AgentPersona }> {
    return apiFetch(`/api/personas/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  delete(id: string): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/personas/${id}`, { method: "DELETE" });
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
      show_tool_activity?: boolean;
      show_system_events?: boolean;
    },
  ): Promise<{ data: Thread }> {
    return apiFetch(`/api/threads/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  notify(
    id: string,
    event_type: string,
    payload?: Record<string, unknown>,
  ): Promise<{
    data: {
      event_type: string;
      persisted: boolean;
      triggered: boolean;
      message_id: string | null;
    };
  }> {
    return apiFetch(`/api/threads/${id}/notify`, {
      method: "POST",
      body: JSON.stringify({ event_type, payload }),
    });
  },

  generateTitle(threadId: string): Promise<{ data: { title: string } }> {
    return apiFetch(`/api/threads/${threadId}/generate-title`, {
      method: "POST",
    });
  },

  deleteEmpty(threadId: string): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/threads/${threadId}`, { method: "DELETE" });
  },

  archive(id: string): Promise<{ data: { id: string; status: string } }> {
    return apiFetch(`/api/threads/${id}/archive`, { method: "POST" });
  },

  unarchive(id: string): Promise<{ data: { id: string; status: string } }> {
    return apiFetch(`/api/threads/${id}/unarchive`, { method: "POST" });
  },

  listMcpServers(threadId: string): Promise<{
    data: Array<{
      id: string;
      thread_id: string;
      mcp_server_id: string;
      enabled: boolean;
    }>;
  }> {
    return apiFetch(`/api/threads/${threadId}/mcp-servers`);
  },

  attachMcpServer(
    threadId: string,
    mcpServerId: string,
  ): Promise<{
    data: {
      id: string;
      thread_id: string;
      mcp_server_id: string;
      enabled: boolean;
    };
  }> {
    return apiFetch(`/api/threads/${threadId}/mcp-servers`, {
      method: "POST",
      body: JSON.stringify({ mcp_server_id: mcpServerId }),
    });
  },

  detachMcpServer(
    threadId: string,
    mcpServerId: string,
  ): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/threads/${threadId}/mcp-servers/${mcpServerId}`, {
      method: "DELETE",
    });
  },
};

// ── Messages ──────────────────────────────────────────────────────────────────

export const messagesApi = {
  list(
    threadId: string,
    opts: { limit?: number; before?: string } = {},
  ): Promise<{ data: Message[] }> {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before) params.set("before", opts.before);
    const qs = params.toString();
    return apiFetch(`/api/threads/${threadId}/messages${qs ? `?${qs}` : ""}`);
  },

  send(threadId: string, content: string): Promise<{ data: Message }> {
    return apiFetch(`/api/threads/${threadId}/messages`, {
      method: "POST",
      body: JSON.stringify({ content }),
    });
  },

  cancel(threadId: string): Promise<{ data: { cancelled: boolean } }> {
    return apiFetch(`/api/threads/${threadId}/cancel`, {
      method: "POST",
    });
  },

  sendCommand(
    threadId: string,
    command: string,
    args: string[],
  ): Promise<{ data: SlashCommandResponse }> {
    return apiFetch(`/api/threads/${threadId}/command`, {
      method: "POST",
      body: JSON.stringify({ command, args }),
    });
  },
};

// ── Providers ─────────────────────────────────────────────────────────────────

export const providersApi = {
  list(): Promise<{ data: Provider[] }> {
    return apiFetch("/api/providers");
  },

  get(id: string): Promise<{ data: Provider }> {
    return apiFetch(`/api/providers/${id}`);
  },

  create(payload: {
    name: string;
    kind: string;
    base_url: string;
    api_key?: string;
  }): Promise<{ data: Provider }> {
    return apiFetch("/api/providers", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },

  update(
    id: string,
    payload: {
      name?: string;
      kind?: string;
      base_url?: string;
      api_key?: string;
      enabled?: boolean;
    },
  ): Promise<{ data: Provider }> {
    return apiFetch(`/api/providers/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  delete(id: string): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/providers/${id}`, { method: "DELETE" });
  },

  test(
    id: string,
  ): Promise<{ data: { models: string[]; connected: boolean } }> {
    return apiFetch(`/api/providers/${id}/test`, { method: "POST" });
  },
};

// ── Models ────────────────────────────────────────────────────────────────────

export const modelsApi = {
  list(providerId: string): Promise<{ data: Model[] }> {
    return apiFetch(`/api/providers/${providerId}/models`);
  },

  sync(providerId: string): Promise<{ data: Model[] }> {
    return apiFetch(`/api/providers/${providerId}/models`, { method: "POST" });
  },

  update(
    providerId: string,
    modelId: string,
    payload: { enabled?: boolean; display_name?: string },
  ): Promise<{ data: Model }> {
    return apiFetch(`/api/providers/${providerId}/models/${modelId}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  delete(
    providerId: string,
    modelId: string,
  ): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/providers/${providerId}/models/${modelId}`, {
      method: "DELETE",
    });
  },
};

// ── Copilot auth ──────────────────────────────────────────────────────────────

export const copilotApi = {
  authStatus(): Promise<{
    data: {
      process_status: string;
      authenticated: boolean;
      reason?: string;
    };
  }> {
    return apiFetch("/api/providers/copilot/auth-status");
  },

  authStart(): Promise<{
    data: {
      device_code: string;
      user_code: string;
      verification_uri: string;
      expires_in: number;
      interval: number;
    };
  }> {
    return apiFetch("/api/providers/copilot/auth-start", { method: "POST" });
  },

  authPoll(deviceCode: string): Promise<{
    data: {
      authenticated: boolean;
      reason?: string;
    };
  }> {
    return apiFetch("/api/providers/copilot/auth-poll", {
      method: "POST",
      body: JSON.stringify({ device_code: deviceCode }),
    });
  },
};

// ── MCP Servers ───────────────────────────────────────────────────────────────

export type McpServerConfig =
  | { executable: string; args: string[]; env: Record<string, string> }
  | { url: string; auth_header?: string; credential_key?: string };

export const mcpServersApi = {
  list(): Promise<{ data: McpServer[] }> {
    return apiFetch("/api/mcp-servers");
  },

  get(id: string): Promise<{ data: McpServer }> {
    return apiFetch(`/api/mcp-servers/${id}`);
  },

  create(payload: {
    name: string;
    description?: string;
    source_url?: string;
    server_type: "local" | "remote";
    config: McpServerConfig;
  }): Promise<{ data: McpServer }> {
    return apiFetch("/api/mcp-servers", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },

  update(
    id: string,
    payload: {
      name?: string;
      description?: string;
      source_url?: string;
      config?: McpServerConfig;
      enabled?: boolean;
    },
  ): Promise<{ data: McpServer }> {
    return apiFetch(`/api/mcp-servers/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  delete(id: string): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/mcp-servers/${id}`, { method: "DELETE" });
  },

  listTools(id: string): Promise<{ data: McpTool[] }> {
    return apiFetch(`/api/mcp-servers/${id}/tools`);
  },
};

// ── Auth ─────────────────────────────────────────────────────────────────────

export const authApi = {
  rotateToken(): Promise<{
    data: { token: string; message: string };
  }> {
    return apiFetch("/api/auth/token/rotate", { method: "POST" });
  },

  getConfig(): Promise<{
    data: { setup_complete: boolean; port: number; version: string };
  }> {
    return apiFetch("/api/config");
  },
};

// ── Pairing ───────────────────────────────────────────────────────────────────

export const pairingApi = {
  generate(): Promise<{
    data: {
      pairing_payload: { server_url: string; token: string };
      hint: string;
    };
  }> {
    return apiFetch("/api/pairing/generate", { method: "POST" });
  },
};

// ── Credentials ───────────────────────────────────────────────────────────────

export const credentialsApi = {
  list(): Promise<Credential[]> {
    return apiFetch("/api/credentials");
  },

  get(id: string): Promise<Credential> {
    return apiFetch(`/api/credentials/${id}`);
  },

  create(payload: CreateCredentialPayload): Promise<Credential> {
    return apiFetch("/api/credentials", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },

  update(id: string, payload: UpdateCredentialPayload): Promise<Credential> {
    return apiFetch(`/api/credentials/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },

  delete(id: string): Promise<{ deleted: boolean; warnings?: string[] }> {
    return apiFetch(`/api/credentials/${id}`, { method: "DELETE" });
  },
};

// ── Setup ─────────────────────────────────────────────────────────────────────

export const setupApi = {
  status(): Promise<{ data: { complete: boolean } }> {
    return apiFetch("/api/setup/status");
  },

  complete(displayName: string): Promise<{
    data: { complete: boolean; user: { id: string; display_name: string } };
  }> {
    return apiFetch("/api/setup/complete", {
      method: "POST",
      body: JSON.stringify({ display_name: displayName }),
    });
  },
};

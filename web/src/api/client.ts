// ── Typed API client — wraps all server endpoints ────────────────────────────

import type {
  AgentPersona,
  Thread,
  Message,
  Provider,
  Model,
  McpServer,
  McpTool,
  Routine,
  SlashCommandResponse,
  MemoryListResponse,
  UserProfile,
  FsEntry,
  FsFileContent,
  TailscaleStatus,
  ThreadMcpServer,
  UploadedFile,
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

// Get auth token from localStorage or session
function getAuthToken(): string | null {
  try {
    // Check localStorage first (set after successful login)
    const token = localStorage.getItem("agent_deck_auth_token");
    if (token) return token;
  } catch {
    // localStorage might not be available
  }
  return null;
}

// Base fetch helper — throws on non-OK responses with the error body
async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers: Record<string, string> = {
    ...(!(options?.body instanceof FormData)
      ? { "Content-Type": "application/json" }
      : {}),
    ...(options.headers ?? {}),
  } as Record<string, string>;

  // Include auth token if available
  const token = getAuthToken();
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(path, {
    ...options,
    headers,
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
      show_system_events?: boolean;
      auto_summarize?: boolean;
      auto_retitle?: boolean;
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

  listMcpServers(threadId: string): Promise<{ data: ThreadMcpServer[] }> {
    return apiFetch(`/api/threads/${threadId}/mcp-servers`);
  },

  attachMcpServer(
    threadId: string,
    mcpServerId: string,
  ): Promise<{ data: ThreadMcpServer }> {
    return apiFetch(`/api/threads/${threadId}/mcp-servers`, {
      method: "POST",
      body: JSON.stringify({ mcp_server_id: mcpServerId }),
    });
  },

  updateThreadMcpServer(
    threadId: string,
    mcpServerId: string,
    payload: {
      disabled_tools?: string[];
      tool_call_timeout_secs?: number | null;
    },
  ): Promise<{ data: ThreadMcpServer }> {
    return apiFetch(`/api/threads/${threadId}/mcp-servers/${mcpServerId}`, {
      method: "PATCH",
      body: JSON.stringify(payload),
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
    opts: { limit?: number; before?: string; include_hidden?: boolean } = {},
  ): Promise<{ data: Message[]; has_more: boolean }> {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before) params.set("before", opts.before);
    if (opts.include_hidden) params.set("include_hidden", "true");
    const qs = params.toString();
    return apiFetch(`/api/threads/${threadId}/messages${qs ? `?${qs}` : ""}`);
  },

  send(
    threadId: string,
    content: string,
    attachments?: import("@/types").MessageAttachment[],
  ): Promise<{ data: Message }> {
    return apiFetch(`/api/threads/${threadId}/messages`, {
      method: "POST",
      body: JSON.stringify({
        content,
        ...(attachments?.length ? { attachments } : {}),
      }),
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
    vision?: boolean;
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
      vision?: boolean;
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
    payload: { enabled?: boolean; display_name?: string; vision?: boolean },
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
    tool_call_timeout_secs?: number | null;
    disabled_tools?: string[];
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
      tool_call_timeout_secs?: number | null;
      disabled_tools?: string[];
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

  restart(id: string): Promise<{ data: { restarted: boolean } }> {
    return apiFetch(`/api/mcp-servers/${id}/restart`, { method: "POST" });
  },
};

// ── Uploads ───────────────────────────────────────────────────────────────────

export const uploadsApi = {
  upload(threadId: string, file: File): Promise<{ data: UploadedFile }> {
    const form = new FormData();
    form.append("file", file);
    // Note: do NOT set Content-Type header — let the browser set multipart boundary
    return apiFetch(`/api/threads/${threadId}/upload`, {
      method: "POST",
      body: form,
    });
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
    data: {
      setup_complete: boolean;
      port: number;
      version: string;
      database_path: string;
      system_timezone: string;
    };
  }> {
    return apiFetch("/api/config");
  },

  // Login with token (for mobile/remote access)
  login(token: string): void {
    try {
      localStorage.setItem("agent_deck_auth_token", token);
    } catch {
      // localStorage might not be available, that's ok
      console.warn("Could not store auth token in localStorage");
    }
  },

  // Get stored auth token
  getToken(): string | null {
    return getAuthToken();
  },

  // Clear auth token (logout)
  logout(): void {
    try {
      localStorage.removeItem("agent_deck_auth_token");
    } catch {
      // localStorage might not be available
    }
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

// ── Routines ──────────────────────────────────────────────────────────────────

export const routinesApi = {
  list(threadId: string): Promise<{ data: Routine[] }> {
    return apiFetch(`/api/threads/${threadId}/routines`);
  },
  get(threadId: string, routineId: string): Promise<{ data: Routine }> {
    return apiFetch(`/api/threads/${threadId}/routines/${routineId}`);
  },
  create(
    threadId: string,
    payload: { name: string; prompt: string; cron_expr: string; timezone?: string },
  ): Promise<{ data: Routine }> {
    return apiFetch(`/api/threads/${threadId}/routines`, {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },
  update(
    threadId: string,
    routineId: string,
    payload: {
      name?: string;
      prompt?: string;
      cron_expr?: string;
      timezone?: string;
      enabled?: boolean;
    },
  ): Promise<{ data: Routine }> {
    return apiFetch(`/api/threads/${threadId}/routines/${routineId}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  },
  delete(
    threadId: string,
    routineId: string,
  ): Promise<{ data: { deleted: boolean } }> {
    return apiFetch(`/api/threads/${threadId}/routines/${routineId}`, {
      method: "DELETE",
    });
  },
  toggle(
    threadId: string,
    routineId: string,
  ): Promise<{ data: { id: string; enabled: boolean } }> {
    return apiFetch(`/api/threads/${threadId}/routines/${routineId}/toggle`, {
      method: "PATCH",
    });
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

export const memoriesApi = {
  list(
    personaId: string,
    params: { limit?: number; offset?: number; thread_id?: string } = {},
  ) {
    const qs = new URLSearchParams();
    if (params.limit != null) qs.set("limit", String(params.limit));
    if (params.offset != null) qs.set("offset", String(params.offset));
    if (params.thread_id) qs.set("thread_id", params.thread_id);
    const query = qs.toString() ? `?${qs.toString()}` : "";
    return apiFetch<{ data: MemoryListResponse }>(
      `/api/personas/${personaId}/memory${query}`,
    );
  },
  delete(personaId: string, memoryId: string) {
    return apiFetch<{ data: { deleted: boolean } }>(
      `/api/personas/${personaId}/memory/${memoryId}`,
      { method: "DELETE" },
    );
  },
};

export const profileApi = {
  get() {
    return apiFetch<{ data: UserProfile }>("/api/profile");
  },

  update(
    fields: Partial<{
      display_name: string;
      pronouns: string | null;
      role: string | null;
      organization: string | null;
      location: string | null;
      timezone: string | null;
      about: string | null;
    }>,
  ) {
    return apiFetch<{ data: UserProfile }>("/api/profile", {
      method: "PUT",
      body: JSON.stringify(fields),
    });
  },
};

export const pushApi = {
  /** Fetch the server's VAPID public key. Public endpoint — no auth required. */
  getVapidPublicKey(): Promise<{ data: { public_key: string } }> {
    return apiFetch("/api/push/vapid-public-key");
  },

  /** Register a new browser push subscription on the server (upsert by endpoint). */
  subscribe(payload: {
    endpoint: string;
    p256dh: string;
    auth: string;
    user_agent?: string;
  }): Promise<{ data: { subscribed: boolean } }> {
    return apiFetch("/api/push/subscribe", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },

  /** Remove a push subscription from the server by endpoint URL. */
  unsubscribe(endpoint: string): Promise<{ data: { deleted: boolean } }> {
    return apiFetch("/api/push/subscribe", {
      method: "DELETE",
      body: JSON.stringify({ endpoint }),
    });
  },
};

export const fsApi = {
  /** List the direct children of a directory on the headless machine. */
  list(path: string): Promise<{ data: { path: string; entries: FsEntry[] } }> {
    return apiFetch(`/api/fs/list?path=${encodeURIComponent(path)}`);
  },
  /** Fetch the content of a file on the headless machine for preview. */
  read(path: string): Promise<{ data: FsFileContent }> {
    return apiFetch(`/api/fs/read?path=${encodeURIComponent(path)}`);
  },
  /** Get (and create if absent) the workspace directory path for a thread. */
  workspace(
    threadId: string,
    title?: string,
  ): Promise<{ data: { path: string } }> {
    const params = new URLSearchParams({ thread_id: threadId });
    if (title) params.set("thread_title", title);
    return apiFetch(`/api/fs/workspace?${params.toString()}`);
  },
  /** Build a download URL for a file. Pure URL builder — no fetch call. */
  downloadUrl(path: string): string {
    return `/api/fs/download?path=${encodeURIComponent(path)}`;
  },
};

export const tailscaleApi = {
  getStatus: (): Promise<{ data: TailscaleStatus }> =>
    apiFetch("/api/tailscale/status"),
  connect: (): Promise<{ data: TailscaleStatus }> =>
    apiFetch("/api/tailscale/connect", { method: "POST" }),
  enableFunnel: (): Promise<{ data: TailscaleStatus }> =>
    apiFetch("/api/tailscale/funnel/enable", { method: "POST" }),
  disableFunnel: (): Promise<{ data: TailscaleStatus }> =>
    apiFetch("/api/tailscale/funnel/disable", { method: "POST" }),
  startServe: (): Promise<{ data: TailscaleStatus }> =>
    apiFetch("/api/tailscale/serve", { method: "POST" }),
};

// ── Webhook Bindings ──────────────────────────────────────────────────────────

export interface WebhookBinding {
  id: string;
  name: string;
  source: string;
  signature_header?: string | null;
  prompt: string;
  enabled: boolean;
  created_at: string;
  // only on create response:
  webhook_url?: string;
  secret?: string;
}

export interface ThreadWebhookBinding {
  id: string; // attachment id
  webhook_binding_id: string;
  name: string;
  source: string;
  enabled: boolean; // from global binding
  prompt?: string | null;
  created_at: string;
}

export const webhookBindingsApi = {
  list: () => apiFetch<{ data: WebhookBinding[] }>("/api/webhook-bindings"),

  create: (payload: {
    name: string;
    source: string;
    signature_header?: string;
    prompt: string;
  }) =>
    apiFetch<{
      data: WebhookBinding & { webhook_url: string; secret: string };
      warning?: string;
    }>("/api/webhook-bindings", {
      method: "POST",
      body: JSON.stringify(payload),
    }),

  update: (
    id: string,
    payload: {
      name?: string;
      prompt?: string;
      signature_header?: string;
      enabled?: boolean;
    },
  ) =>
    apiFetch<{ data: WebhookBinding }>(`/api/webhook-bindings/${id}`, {
      method: "PATCH",
      body: JSON.stringify(payload),
    }),

  delete: (id: string) =>
    apiFetch<{ data: { deleted: boolean } }>(`/api/webhook-bindings/${id}`, {
      method: "DELETE",
    }),

  toggle: (id: string) =>
    apiFetch<{ data: WebhookBinding }>(`/api/webhook-bindings/${id}/toggle`, {
      method: "PATCH",
    }),
};

export const threadWebhookBindingsApi = {
  list: (threadId: string) =>
    apiFetch<{ data: ThreadWebhookBinding[] }>(
      `/api/threads/${threadId}/webhook-bindings`,
    ),

  attach: (threadId: string, webhookBindingId: string, prompt?: string) =>
    apiFetch<{ data: ThreadWebhookBinding }>(
      `/api/threads/${threadId}/webhook-bindings`,
      {
        method: "POST",
        body: JSON.stringify({ webhook_binding_id: webhookBindingId, prompt }),
      },
    ),

  detach: (threadId: string, attachmentId: string) =>
    apiFetch<{ data: { detached: boolean } }>(
      `/api/threads/${threadId}/webhook-bindings/${attachmentId}`,
      { method: "DELETE" },
    ),

  updatePrompt: (
    threadId: string,
    attachmentId: string,
    prompt: string | null,
  ) =>
    apiFetch<{ data: ThreadWebhookBinding }>(
      `/api/threads/${threadId}/webhook-bindings/${attachmentId}`,
      { method: "PATCH", body: JSON.stringify({ prompt }) },
    ),
};

// ── Shared TypeScript types matching the Rust server models ──────────────────

export interface AgentPersona {
  id: string;
  user_id: string;
  name: string;
  emoji: string;
  avatar_path: string | null;
  system_prompt: string;
  default_model: string | null;
  default_provider: string | null;
  created_at: string;
  updated_at: string;
}

export interface Thread {
  id: string;
  user_id: string;
  persona_id: string;
  title: string;
  active_model: string | null;
  active_provider: string | null;
  system_prompt_addendum: string | null;
  status: string;
  created_at: string;
  updated_at: string;
  // Joined client-side for display convenience
  persona?: AgentPersona;
  last_message_preview?: string;
}

export interface Message {
  id: string;
  thread_id: string;
  role: "user" | "assistant" | "system";
  content: string;
  source: "chat" | "routine" | "tool";
  routine_id: string | null;
  visibility: "visible" | "hidden";
  execution_id: string | null;
  created_at: string;
}

export interface Provider {
  id: string;
  user_id: string;
  name: string;
  kind: string;
  base_url: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface Model {
  id: string;
  provider_id: string;
  model_id: string;
  display_name: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface McpServer {
  id: string;
  user_id: string;
  name: string;
  description: string | null;
  source_url: string | null;
  server_type: "local" | "remote";
  config: string; // JSON string
  status: "inactive" | "connecting" | "connected" | "error";
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface McpTool {
  name: string;
  description: string;
}

export interface Routine {
  id: string;
  thread_id: string;
  name: string;
  prompt: string;
  cron_expr: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

// ── SSE event payloads ────────────────────────────────────────────────────────

export interface SseTokenEvent {
  event: "token";
  token: string;
}

export interface SseMessageCompleteEvent {
  event: "message_complete";
  id: string;
  thread_id: string;
  role: string;
  content: string;
  created_at: string;
}

export interface SseRoutineMessageEvent {
  event: "routine_message";
  id: string;
  thread_id: string;
  role: string;
  content: string;
  routine_id: string;
  created_at: string;
}

export interface SseErrorEvent {
  // Named "stream_error" on the wire to avoid collision with EventSource's
  // built-in "error" connection event. The client handler also accepts the
  // legacy "error" name for backwards compatibility during the transition.
  event: "stream_error" | "error";
  code: string;
  message: string;
}

export type SseThreadEvent =
  | SseTokenEvent
  | SseMessageCompleteEvent
  | SseRoutineMessageEvent
  | SseErrorEvent;

// Global SSE event from copilot.rs GlobalEvent
export interface SseThreadUpdatedEvent {
  event: "thread_updated";
  thread_id: string;
  last_message: string;
  updated_at: string;
}

export type SseGlobalEvent = SseThreadUpdatedEvent;

// ── API response wrappers ─────────────────────────────────────────────────────

export interface ApiResponse<T> {
  data: T;
}

export interface ApiError {
  error: string;
  code?: string;
}

// ── Slash command response ────────────────────────────────────────────────────

export interface SlashCommandPayload {
  model_id?: string;
  display_name?: string;
}

export interface SlashCommandResponse {
  type: string;
  message: string;
  payload?: SlashCommandPayload;
}

// ── Streaming state ───────────────────────────────────────────────────────────

export interface StreamingMessage {
  thread_id: string;
  content: string;
  started_at: string;
}

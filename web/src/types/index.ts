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
  is_default: boolean;
  recall_conversation_cross_thread: boolean;
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
  show_system_events: boolean;
  // Summarization fields (server-managed, read-only from client)
  summary: string | null;
  summary_updated_at: string | null;
  summary_message_count: number;
  auto_summarize: boolean;
  auto_retitle: boolean;
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
  source: "chat" | "routine" | "tool" | "system_event";
  routine_id: string | null;
  visibility: "visible" | "hidden";
  execution_id: string | null;
  event_type?: string;
  stopped?: boolean;
  attachments?: MessageAttachment[] | null;
  created_at: string;
}

export interface MessageAttachment {
  path: string;
  filename: string;
  content_type: string;
}

export interface UploadedFile {
  path: string;
  filename: string;
  size: number;
  content_type: string;
  is_image: boolean;
}

export interface Provider {
  id: string;
  user_id: string;
  name: string;
  kind: string;
  base_url: string;
  enabled: boolean;
  vision: boolean;
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
  tag: string;
  description: string | null;
  source_url: string | null;
  server_type: "local" | "remote";
  config: string; // JSON string
  status: "inactive" | "connecting" | "connected" | "error";
  enabled: boolean;
  /** Per-server timeout for tool calls in seconds. null = no timeout. */
  tool_call_timeout_secs: number | null;
  /** Tool names that are disabled for this server and won't be offered to the model. */
  disabled_tools: string[];
  created_at: string;
  updated_at: string;
}

export interface McpTool {
  name: string;
  description: string;
}

export interface ThreadMcpServer {
  id: string;
  thread_id: string;
  mcp_server_id: string;
  enabled: boolean;
  /** Per-thread disabled tool names. Tools in this list won't be offered to the model for this thread. */
  disabled_tools: string[];
  /** Per-thread timeout override in seconds. null = inherit from the server's global setting. */
  tool_call_timeout_secs: number | null;
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

// ── File system types ─────────────────────────────────────────────────────────

export interface FsEntry {
  name: string;
  path: string;
  kind: "file" | "dir";
  size: number | null;
  modified: string | null;
  extension: string | null;
}

export interface FsFileContent {
  path: string;
  previewable: boolean;
  // present when previewable === true and is_image !== true
  content?: string;
  // present when previewable === true and is_image === true
  is_image?: boolean;
  image_data?: string;
  extension?: string | null;
  size: number;
  // present when previewable === false
  reason?: "binary" | "too_large";
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
  stopped?: boolean;
}

export interface SseSystemEventEvent {
  event: "system_event";
  event_type: string;
  content: string;
}

export interface SseCancelledEvent {
  event: "cancelled";
  thread_id: string;
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

export interface SseRetryEvent {
  event: "retry";
  attempt: number;
  max_attempts: number;
  reason: string;
}

export interface SseToolStartEvent {
  event: "tool_start";
  tool_name: string;
  tool_call_id: string;
  round: number;
  input_preview: Record<string, string>;
}

export interface SseToolActivityEvent {
  event: "tool_activity";
  id: string;
  role: string;
  content: string;
  created_at: string;
  tool_call_id: string;
  round: number;
}

export interface SseToolRoundCompleteEvent {
  event: "tool_round_complete";
  round: number;
  tool_count: number;
}

export interface SseChatSegmentEvent {
  event: "chat_segment";
  id: string;
  thread_id: string;
  content: string;
  created_at: string;
}

export type SseThreadEvent =
  | SseTokenEvent
  | SseMessageCompleteEvent
  | SseRoutineMessageEvent
  | SseErrorEvent
  | SseSystemEventEvent
  | SseCancelledEvent
  | SseRetryEvent
  | SseToolStartEvent
  | SseToolActivityEvent
  | SseToolRoundCompleteEvent
  | SseChatSegmentEvent;

// Global SSE event from copilot.rs GlobalEvent
export interface SseThreadUpdatedEvent {
  event: "thread_updated";
  thread_id: string;
  last_message: string;
  updated_at: string;
}

export interface SseMcpStatusChangedEvent {
  event: "mcp_status_changed";
  mcp_server_id: string;
  status: "inactive" | "connecting" | "connected" | "error";
}

export type SseGlobalEvent = SseThreadUpdatedEvent | SseMcpStatusChangedEvent;

// ── API response wrappers ─────────────────────────────────────────────────────

export interface ApiResponse<T> {
  data: T;
}

export interface ApiError {
  error: string;
  code?: string;
}

// ── Slash command response ────────────────────────────────────────────────────

// TODO: Tighten to a discriminated union per command type (model_list,
// memory_list, routine_list, model_switched, etc.) once the command surface
// stabilises. For now, keep it loose and pattern-match on `type` at render time.
export interface SlashCommandPayload {
  model_id?: string;
  display_name?: string;
  [key: string]: unknown;
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

// ── Thread state machine ──────────────────────────────────────────────────────

export interface ToolCallEntry {
  tool_call_id: string;
  tool_name: string;
  input_preview: Record<string, string>;
  status: "in_progress" | "completed" | "cancelled";
  call_message_id: string | null;
  result_message_id: string | null;
  result_content: string | null; // tool output from tool_activity SSE
}

export interface ProcessingRound {
  round: number;
  tools: ToolCallEntry[];
  status: "in_progress" | "completed" | "cancelled";
  reasoning: string;
}

export type StreamingEntry =
  | { type: "text"; content: string }
  | { type: "processing"; rounds: ProcessingRound[] };

export type ThreadPhase =
  | { status: "idle" }
  | { status: "sending"; optimisticId: string }
  | { status: "streaming"; entries: StreamingEntry[] }
  | { status: "error"; message: string; recoverable: boolean };

export interface ThreadState {
  messages: Message[];
  phase: ThreadPhase;
  /** Number of messages queued behind the currently active run. */
  queuedCount?: number;
  // ── Pagination ──────────────────────────────────────────────────────────────
  /** ID of the oldest loaded message — used as `before` cursor for load-more */
  oldestLoadedId?: string | null;
  /** Whether older messages exist on the server beyond what's loaded */
  hasMore?: boolean;
  /** True while a load-more fetch is in progress (prevents double-fetch) */
  isLoadingMore?: boolean;
  /** Processing rounds from the most recent streaming turn. Preserved across
   *  phase transitions so the ProcessingBubble survives finalizeStream/cancel. */
  lastProcessingRounds?: ProcessingRound[] | null;
}

export type ThreadMap = Record<string, ThreadState>;

// ── Memory ────────────────────────────────────────────────────────────────────

export interface MemoryEntry {
  id: string;
  content: string;
  thread_id: string | null;
  thread_title: string | null;
  created_at: string;
}

export interface MemoryListResponse {
  memories: MemoryEntry[];
  total_count: number;
}

// ── User Profile ──────────────────────────────────────────────────────────────

export interface UserProfile {
  display_name: string;
  pronouns: string | null;
  role: string | null;
  organization: string | null;
  location: string | null;
  timezone: string | null;
  about: string | null;
  profile_updated_at: string | null;
}

// ── Tailscale ─────────────────────────────────────────────────────────────────

export interface TailscaleStatus {
  installed: boolean;
  connected: boolean;
  needs_service: boolean;
  hostname: string | null;
  funnel_enabled: boolean;
  funnel_url: string | null;
  auth_url: string | null;
  version: string | null;
  ip_address: string | null;
  serving: boolean;
  serve_url: string | null;
  message?: string;
}

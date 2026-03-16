//! Notify route — Story 5.1 Part C
//!
//! `POST /api/threads/:id/notify`
//!
//! Allows clients to emit structured system events into a thread.  Each event
//! type can independently:
//!   - `persist`: insert a hidden `system_event` message into the DB and
//!     broadcast it on the thread's SSE stream so connected clients can
//!     optionally surface it in the UI.
//!   - `trigger`: fire the agent run-loop with the event content as the
//!     user message (used for `routine_fired` events).

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

use crate::{
    error::{AppError, AppResult},
    routes::{
        messages::{get_user_id, verify_thread_ownership},
        AppState,
    },
};

// ─── Event type registry ──────────────────────────────────────────────────────

/// All supported system event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEventType {
    ModelSwitched,
    ProviderSwitched,
    McpServerAttached,
    McpServerDetached,
    AddendumUpdated,
    RoutineFired,
}

impl SystemEventType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "model_switched" => Some(Self::ModelSwitched),
            "provider_switched" => Some(Self::ProviderSwitched),
            "mcp_server_attached" => Some(Self::McpServerAttached),
            "mcp_server_detached" => Some(Self::McpServerDetached),
            "addendum_updated" => Some(Self::AddendumUpdated),
            "routine_fired" => Some(Self::RoutineFired),
            _ => None,
        }
    }

    /// Whether this event type should be persisted as a hidden system message.
    pub fn persist(&self) -> bool {
        !matches!(self, Self::RoutineFired)
    }

    /// Whether this event type should trigger an agent run.
    pub fn trigger(&self) -> bool {
        matches!(self, Self::RoutineFired)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ModelSwitched => "model_switched",
            Self::ProviderSwitched => "provider_switched",
            Self::McpServerAttached => "mcp_server_attached",
            Self::McpServerDetached => "mcp_server_detached",
            Self::AddendumUpdated => "addendum_updated",
            Self::RoutineFired => "routine_fired",
        }
    }

    /// Format a human-readable content string for this event using the
    /// optional payload fields.
    pub fn format_content(&self, payload: &Option<Value>) -> String {
        let p = payload.as_ref().and_then(|v| v.as_object());
        match self {
            Self::ModelSwitched => {
                let provider = p
                    .and_then(|m| m.get("provider_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let model = p
                    .and_then(|m| m.get("model_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("Model switched to {} · {}", provider, model)
            }
            Self::ProviderSwitched => {
                let provider = p
                    .and_then(|m| m.get("provider_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("Provider switched to {}", provider)
            }
            Self::McpServerAttached => {
                let name = p
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("MCP server '{}' attached to this thread", name)
            }
            Self::McpServerDetached => {
                let name = p
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("MCP server '{}' detached from this thread", name)
            }
            Self::AddendumUpdated => "System prompt addendum updated".to_string(),
            Self::RoutineFired => {
                // For routine_fired, the payload itself is the agent instruction
                payload
                    .as_ref()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            }
        }
    }
}

// ─── Request / Response shapes ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NotifyRequest {
    pub event_type: String,
    pub payload: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct NotifyResponse {
    pub event_type: String,
    pub persisted: bool,
    pub triggered: bool,
    pub message_id: Option<String>,
}

// ─── Handler ──────────────────────────────────────────────────────────────────

/// `POST /api/threads/:id/notify`
///
/// Emits a structured system event into the thread.  Depending on the event
/// type the handler may:
///   1. Insert a hidden `system_event` message into the DB.
///   2. Broadcast a `system_event` SSE event to connected clients.
///   3. Trigger the agent run-loop (for `routine_fired`).
pub async fn notify(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(body): Json<NotifyRequest>,
) -> AppResult<impl IntoResponse> {
    let user_id = get_user_id(&state).await?;
    verify_thread_ownership(&state, &thread_id, &user_id).await?;

    let event_type = SystemEventType::from_str(&body.event_type)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown event_type: {}", body.event_type)))?;

    let content = event_type.format_content(&body.payload);
    let mut message_id: Option<String> = None;

    // ── Persist ───────────────────────────────────────────────────────────────
    if event_type.persist() {
        let msg_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        sqlx::query(
            "INSERT INTO messages
                 (id, thread_id, role, content, source, routine_id, visibility,
                  execution_id, event_type, stopped, created_at)
             VALUES (?, ?, 'system', ?, 'system_event', NULL, 'hidden', NULL, ?, 0, ?)",
        )
        .bind(&msg_id)
        .bind(&thread_id)
        .bind(&content)
        .bind(event_type.as_str())
        .bind(&now)
        .execute(&state.pool)
        .await?;

        message_id = Some(msg_id);

        // Broadcast system_event SSE so connected clients can react immediately
        state.send_thread_event(
            &thread_id,
            crate::routes::sse::ThreadEvent::SystemEvent {
                event_type: event_type.as_str().to_string(),
                content: content.clone(),
            },
        );
    }

    // ── Trigger ───────────────────────────────────────────────────────────────
    if event_type.trigger() {
        let run_state = state.get_run_state(&thread_id);

        // Routine runs are not depth-limited — they originate from the server
        // itself rather than from user-initiated sends.
        run_state.depth.fetch_add(1, Ordering::SeqCst);

        let state_clone = state.clone();
        let tid = thread_id.clone();
        let trigger_content = content.clone();

        tokio::spawn(async move {
            let new_token = tokio_util::sync::CancellationToken::new();
            {
                let mut lock = run_state.cancel_token.lock().await;
                *lock = new_token.clone();
            }

            // Acquire semaphore — queues behind any in-progress run on this thread
            let _permit = run_state.semaphore.acquire().await.unwrap();

            crate::services::agent::run(state_clone, tid, trigger_content, new_token).await;

            run_state.depth.fetch_sub(1, Ordering::SeqCst);
        });
    }

    Ok(Json(json!({
        "data": {
            "event_type": event_type.as_str(),
            "persisted": event_type.persist(),
            "triggered": event_type.trigger(),
            "message_id": message_id,
        }
    })))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── from_str ──────────────────────────────────────────────────────────────

    #[test]
    fn unknown_event_type_returns_none() {
        assert!(SystemEventType::from_str("totally_unknown").is_none());
        assert!(SystemEventType::from_str("").is_none());
        assert!(SystemEventType::from_str("MODEL_SWITCHED").is_none()); // case-sensitive
    }

    #[test]
    fn all_known_event_types_parse_from_str() {
        assert_eq!(
            SystemEventType::from_str("model_switched"),
            Some(SystemEventType::ModelSwitched)
        );
        assert_eq!(
            SystemEventType::from_str("provider_switched"),
            Some(SystemEventType::ProviderSwitched)
        );
        assert_eq!(
            SystemEventType::from_str("mcp_server_attached"),
            Some(SystemEventType::McpServerAttached)
        );
        assert_eq!(
            SystemEventType::from_str("mcp_server_detached"),
            Some(SystemEventType::McpServerDetached)
        );
        assert_eq!(
            SystemEventType::from_str("addendum_updated"),
            Some(SystemEventType::AddendumUpdated)
        );
        assert_eq!(
            SystemEventType::from_str("routine_fired"),
            Some(SystemEventType::RoutineFired)
        );
    }

    // ── persist / trigger flags ───────────────────────────────────────────────

    #[test]
    fn model_switched_has_persist_true_trigger_false() {
        let et = SystemEventType::ModelSwitched;
        assert!(et.persist());
        assert!(!et.trigger());
    }

    #[test]
    fn provider_switched_has_persist_true_trigger_false() {
        let et = SystemEventType::ProviderSwitched;
        assert!(et.persist());
        assert!(!et.trigger());
    }

    #[test]
    fn mcp_server_attached_has_persist_true_trigger_false() {
        let et = SystemEventType::McpServerAttached;
        assert!(et.persist());
        assert!(!et.trigger());
    }

    #[test]
    fn mcp_server_detached_has_persist_true_trigger_false() {
        let et = SystemEventType::McpServerDetached;
        assert!(et.persist());
        assert!(!et.trigger());
    }

    #[test]
    fn addendum_updated_has_persist_true_trigger_false() {
        let et = SystemEventType::AddendumUpdated;
        assert!(et.persist());
        assert!(!et.trigger());
    }

    #[test]
    fn routine_fired_has_persist_false_trigger_true() {
        let et = SystemEventType::RoutineFired;
        assert!(!et.persist());
        assert!(et.trigger());
    }

    #[test]
    fn all_registered_types_have_at_least_one_flag_true() {
        let all = [
            SystemEventType::ModelSwitched,
            SystemEventType::ProviderSwitched,
            SystemEventType::McpServerAttached,
            SystemEventType::McpServerDetached,
            SystemEventType::AddendumUpdated,
            SystemEventType::RoutineFired,
        ];
        for et in &all {
            assert!(
                et.persist() || et.trigger(),
                "{:?} has both persist=false and trigger=false — every type must have at least one flag set",
                et
            );
        }
    }

    // ── as_str roundtrip ──────────────────────────────────────────────────────

    #[test]
    fn as_str_roundtrips_for_all_variants() {
        let all = [
            SystemEventType::ModelSwitched,
            SystemEventType::ProviderSwitched,
            SystemEventType::McpServerAttached,
            SystemEventType::McpServerDetached,
            SystemEventType::AddendumUpdated,
            SystemEventType::RoutineFired,
        ];
        for et in &all {
            let s = et.as_str();
            let parsed = SystemEventType::from_str(s).unwrap_or_else(|| {
                panic!(
                    "as_str() produced '{}' which does not round-trip via from_str",
                    s
                )
            });
            assert_eq!(*et, parsed, "roundtrip failed for {:?}", et);
        }
    }

    // ── format_content ────────────────────────────────────────────────────────

    #[test]
    fn model_switched_content_includes_provider_and_model() {
        let payload = Some(json!({
            "provider_name": "OpenAI",
            "model_name": "gpt-4o"
        }));
        let content = SystemEventType::ModelSwitched.format_content(&payload);
        assert!(
            content.contains("OpenAI"),
            "expected provider name in content: {}",
            content
        );
        assert!(
            content.contains("gpt-4o"),
            "expected model name in content: {}",
            content
        );
    }

    #[test]
    fn model_switched_content_uses_unknown_when_payload_absent() {
        let content = SystemEventType::ModelSwitched.format_content(&None);
        assert!(
            content.contains("unknown"),
            "expected 'unknown' placeholder when payload absent: {}",
            content
        );
    }

    #[test]
    fn provider_switched_content_includes_provider_name() {
        let payload = Some(json!({ "provider_name": "Anthropic" }));
        let content = SystemEventType::ProviderSwitched.format_content(&payload);
        assert!(
            content.contains("Anthropic"),
            "expected provider name in content: {}",
            content
        );
    }

    #[test]
    fn mcp_server_attached_content_includes_server_name() {
        let payload = Some(json!({ "name": "filesystem" }));
        let content = SystemEventType::McpServerAttached.format_content(&payload);
        assert!(
            content.contains("filesystem"),
            "expected server name in content: {}",
            content
        );
    }

    #[test]
    fn mcp_server_detached_content_includes_server_name() {
        let payload = Some(json!({ "name": "brave-search" }));
        let content = SystemEventType::McpServerDetached.format_content(&payload);
        assert!(
            content.contains("brave-search"),
            "expected server name in content: {}",
            content
        );
    }

    #[test]
    fn addendum_updated_content_is_fixed_string() {
        // The content should be the same fixed string regardless of payload.
        let with_payload =
            SystemEventType::AddendumUpdated.format_content(&Some(json!({ "anything": true })));
        let without_payload = SystemEventType::AddendumUpdated.format_content(&None);
        assert_eq!(with_payload, without_payload);
        assert!(!with_payload.is_empty(), "fixed string should not be empty");
    }

    #[test]
    fn routine_fired_content_is_payload_string() {
        let payload = Some(Value::String("Run the daily report".to_string()));
        let content = SystemEventType::RoutineFired.format_content(&payload);
        assert_eq!(content, "Run the daily report");
    }

    #[test]
    fn routine_fired_content_is_empty_when_no_payload() {
        let content = SystemEventType::RoutineFired.format_content(&None);
        assert_eq!(content, "");
    }
}

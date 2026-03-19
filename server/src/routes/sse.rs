//! SSE infrastructure — Story 2.3
//!
//! Two endpoints:
//!
//! `GET /api/threads/:id/stream` — per-thread stream
//!   Backed by a per-connection `tokio::sync::mpsc` channel.  When the client
//!   connects a new sender is registered in `AppState::thread_senders`.  When
//!   it disconnects the sender is dropped and the registry entry is cleaned up.
//!
//! `GET /api/events` — global stream
//!   Backed by a `tokio::sync::broadcast` channel held in `AppState`.  Each
//!   connected client subscribes via `global_tx.subscribe()`.
//!
//! Both streams emit a 30-second keepalive ping.

use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use futures::stream::{Stream, StreamExt};
use std::{convert::Infallible, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};

use crate::{error::AppResult, routes::AppState, services::copilot::GlobalEvent};

// ─── Per-thread event types ───────────────────────────────────────────────────

/// Events emitted on a per-thread SSE stream.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ThreadEvent {
    /// A single streamed LLM token.
    Token { token: String },
    /// Full assistant message once streaming is complete.
    MessageComplete {
        id: String,
        thread_id: String,
        role: String,
        content: String,
        created_at: String,
        stopped: bool,
    },
    /// A system event notification (model switched, MCP attached, etc.)
    SystemEvent { event_type: String, content: String },
    /// A message produced by a routine firing on this thread.
    RoutineMessage {
        id: String,
        thread_id: String,
        role: String,
        content: String,
        routine_id: String,
        created_at: String,
    },
    /// A streaming or provider error.
    Error { code: String, message: String },
}

impl ThreadEvent {
    /// The SSE `event:` field name for this variant.
    pub fn event_name(&self) -> &'static str {
        match self {
            ThreadEvent::Token { .. } => "token",
            ThreadEvent::MessageComplete { .. } => "message_complete",
            ThreadEvent::RoutineMessage { .. } => "routine_message",
            ThreadEvent::SystemEvent { .. } => "system_event",
            ThreadEvent::Error { .. } => "stream_error",
        }
    }
}

// ─── AppState helper methods ──────────────────────────────────────────────────

impl AppState {
    /// Register a new per-thread SSE sender and return the corresponding receiver.
    ///
    /// Multiple clients may connect to the same thread stream simultaneously — each
    /// gets its own channel.  The `SenderGuard` returned by the caller must be kept
    /// alive for the duration of the connection; dropping it removes the sender from
    /// the registry automatically via `SenderGuard`'s `Drop` impl.
    pub fn subscribe_thread(
        &self,
        thread_id: &str,
    ) -> (mpsc::Sender<ThreadEvent>, mpsc::Receiver<ThreadEvent>) {
        let (tx, rx) = mpsc::channel::<ThreadEvent>(4096);
        let mut map = self.thread_senders.lock().unwrap();
        map.entry(thread_id.to_string())
            .or_default()
            .push(tx.clone());
        (tx, rx)
    }

    /// Remove a specific sender from the per-thread registry.
    ///
    /// Called when the SSE connection is closed so we don't accumulate dead senders.
    pub fn unsubscribe_thread(&self, thread_id: &str, tx: &mpsc::Sender<ThreadEvent>) {
        let mut map = self.thread_senders.lock().unwrap();
        if let Some(senders) = map.get_mut(thread_id) {
            senders.retain(|s| !s.same_channel(tx));
            if senders.is_empty() {
                map.remove(thread_id);
            }
        }
    }

    /// Broadcast a `ThreadEvent` to all clients currently connected to the given thread.
    ///
    /// Senders whose receivers have been dropped (i.e. the client disconnected) are
    /// silently removed.  Senders whose channels are temporarily full are kept — a
    /// full channel means the client is briefly behind, not gone.  Evicting on Full
    /// would permanently lose the sender (and all subsequent events, including
    /// `message_complete`) whenever a tool call causes a brief pause in draining.
    pub fn send_thread_event(&self, thread_id: &str, event: ThreadEvent) {
        use tokio::sync::mpsc::error::TrySendError;
        let mut map = self.thread_senders.lock().unwrap();
        if let Some(senders) = map.get_mut(thread_id) {
            senders.retain(|tx| match tx.try_send(event.clone()) {
                Ok(_) => true,
                Err(TrySendError::Full(_)) => true, // keep — receiver is just slow
                Err(TrySendError::Closed(_)) => false, // drop — receiver is gone
            });
            if senders.is_empty() {
                map.remove(thread_id);
            }
        }
    }

    /// Broadcast a `GlobalEvent` to all connected global-stream clients.
    ///
    /// Returns `Ok(n)` where `n` is the number of active receivers, or `Err` if
    /// there are none (which is fine — events fired before any client connects are
    /// simply discarded).
    pub fn send_global_event(
        &self,
        event: GlobalEvent,
    ) -> std::result::Result<usize, tokio::sync::broadcast::error::SendError<GlobalEvent>> {
        self.global_tx.send(event)
    }

    /// Subscribe to the global event stream.
    pub fn subscribe_global(&self) -> tokio::sync::broadcast::Receiver<GlobalEvent> {
        self.global_tx.subscribe()
    }

    /// Returns `true` if at least one SSE client is currently subscribed to the
    /// given thread stream.
    pub fn has_thread_subscriber(&self, thread_id: &str) -> bool {
        let map = self.thread_senders.lock().unwrap();
        map.get(thread_id).map_or(false, |v| !v.is_empty())
    }

    /// Wait until at least one SSE client subscribes to `thread_id`, or until
    /// `timeout` elapses.  This lets the agent start streaming immediately after
    /// the client's EventSource connection is established, rather than firing
    /// tokens into the void before the browser is ready.
    pub async fn wait_for_subscriber(&self, thread_id: &str, timeout: std::time::Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.has_thread_subscriber(thread_id) {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}

// ─── Route handlers ───────────────────────────────────────────────────────────

/// `GET /api/threads/:id/stream`
///
/// Per-thread SSE stream.  Each connected client gets its own mpsc channel.
pub async fn thread_stream(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let (tx, rx) = state.subscribe_thread(&thread_id);

    // Clone identifiers for the cleanup closure.
    let state_for_cleanup = state.clone();
    let thread_id_for_cleanup = thread_id.clone();
    let tx_for_cleanup = tx.clone();

    // Wrap the receiver as a Stream<Item = Result<Event, Infallible>>.
    let event_stream = ReceiverStream::new(rx).map(move |thread_event| {
        let name = thread_event.event_name();
        let data = serde_json::to_string(&thread_event).unwrap_or_else(|_| "{}".to_string());
        Ok::<Event, Infallible>(Event::default().event(name).data(data))
    });

    // Append a "cleanup" stream that fires once the main stream ends (client disconnected).
    // We use a channel that we immediately drop on the server side so the stream ends,
    // then run cleanup logic in an async block that completes after disconnection.
    let cleanup_state = state_for_cleanup;
    let cleanup_tid = thread_id_for_cleanup;
    let cleanup_tx = tx_for_cleanup;

    // Wrap in a stream that runs cleanup on termination.
    let guarded_stream = CleanupStream {
        inner: event_stream,
        cleanup: Some((cleanup_state, cleanup_tid, cleanup_tx)),
    };

    Ok(Sse::new(guarded_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    ))
}

/// `GET /api/events`
///
/// Global SSE stream backed by a `broadcast` channel.
pub async fn global_stream(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let rx = state.subscribe_global();

    // Wrap the broadcast receiver as a Stream using tokio_stream's BroadcastStream,
    // then map each item into an SSE Event.  This avoids the futures-core version
    // conflict that async_stream::stream! triggers.
    let handshake = futures::stream::once(futures::future::ready(Ok::<Event, Infallible>(
        Event::default()
            .event("connected")
            .data(r#"{"service":"agent-deck"}"#),
    )));

    let broadcast_stream = BroadcastStream::new(rx).filter_map(|item| {
        futures::future::ready(match item {
            Ok(global_event) => {
                let event_name = match &global_event {
                    GlobalEvent::ProviderStatus { .. } => "provider_status",
                    GlobalEvent::ThreadUpdated { .. } => "thread_updated",
                    GlobalEvent::RoutineFired { .. } => "routine_fired",
                    GlobalEvent::TitleUpdated { .. } => "title_updated",
                    GlobalEvent::McpStatusChanged { .. } => "mcp_status_changed",
                };
                let data =
                    serde_json::to_string(&global_event).unwrap_or_else(|_| "{}".to_string());
                Some(Ok::<Event, Infallible>(
                    Event::default().event(event_name).data(data),
                ))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("global SSE stream lagged by {} events", n);
                None
            }
        })
    });

    let event_stream = handshake.chain(broadcast_stream);

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    ))
}

// ─── CleanupStream ────────────────────────────────────────────────────────────

/// Wraps an inner `Stream` and runs deregistration logic when the stream is
/// dropped (i.e., when the SSE connection is closed by the client or the server).
struct CleanupStream<S> {
    inner: S,
    cleanup: Option<(AppState, String, mpsc::Sender<ThreadEvent>)>,
}

impl<S> Drop for CleanupStream<S> {
    fn drop(&mut self) {
        if let Some((state, thread_id, tx)) = self.cleanup.take() {
            state.unsubscribe_thread(&thread_id, &tx);
        }
    }
}

impl<S> Stream for CleanupStream<S>
where
    S: Stream<Item = Result<Event, Infallible>> + Unpin,
{
    type Item = Result<Event, Infallible>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::AppState;
    use crate::services::copilot::GlobalEvent;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::broadcast;

    fn make_state() -> AppState {
        let (global_tx, _) = broadcast::channel(64);
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let (mcp_tx, _) = tokio::sync::broadcast::channel(1);
        let mcp = crate::services::mcp::McpConnectionManager::new(
            pool.clone(),
            "test-master-key".to_string(),
            mcp_tx,
            std::path::PathBuf::from("/tmp/test-deck/mcp"),
        );
        AppState {
            pool,
            config: crate::config::Config {
                port: 7474,
                data_dir: std::path::PathBuf::from("/tmp/test-deck"),
                mcp_dir: std::path::PathBuf::from("/tmp/test-deck/mcp"),
                personas_dir: std::path::PathBuf::from("/tmp/test-deck/personas"),
                database_url: "sqlite::memory:".to_string(),
                public_dir: "./public".to_string(),
                fcm_service_account_json: None,
            },
            machine_secret: "test-secret".to_string(),
            credential_master_key: "test-master-key".to_string(),
            auth_token: "test-token".to_string(),
            global_tx,
            thread_senders: Arc::new(Mutex::new(HashMap::new())),
            run_states: dashmap::DashMap::new(),
            copilot: None,
            mcp,
            built_in_tools: std::sync::Arc::new(vec![]),
        }
    }

    // ── Thread sender registry ─────────────────────────────────────────────────

    #[tokio::test]
    async fn subscribe_thread_registers_sender() {
        let state = make_state();
        let (_tx, _rx) = state.subscribe_thread("thread-abc");
        let map = state.thread_senders.lock().unwrap();
        assert!(map.contains_key("thread-abc"));
        assert_eq!(map["thread-abc"].len(), 1);
    }

    #[tokio::test]
    async fn multiple_subscribers_same_thread() {
        let state = make_state();
        let (_tx1, _rx1) = state.subscribe_thread("t1");
        let (_tx2, _rx2) = state.subscribe_thread("t1");
        let map = state.thread_senders.lock().unwrap();
        assert_eq!(map["t1"].len(), 2);
    }

    #[tokio::test]
    async fn unsubscribe_thread_removes_sender() {
        let state = make_state();
        let (tx, _rx) = state.subscribe_thread("t2");
        state.unsubscribe_thread("t2", &tx);
        let map = state.thread_senders.lock().unwrap();
        assert!(!map.contains_key("t2"));
    }

    #[tokio::test]
    async fn send_thread_event_delivers_to_receiver() {
        let state = make_state();
        let (tx, mut rx) = state.subscribe_thread("t3");
        // We need the tx in the registry, but subscribe_thread already added it.
        // Drop our local clone — the registry still holds one.
        drop(tx);

        // Manually insert a fresh pair to test delivery
        let (tx2, mut rx2) = mpsc::channel(8);
        {
            let mut map = state.thread_senders.lock().unwrap();
            map.entry("t3".to_string()).or_default().push(tx2);
        }

        state.send_thread_event(
            "t3",
            ThreadEvent::Token {
                token: "hello".to_string(),
            },
        );

        let event = rx2.try_recv().expect("should have received event");
        match event {
            ThreadEvent::Token { token } => assert_eq!(token, "hello"),
            _ => panic!("wrong event type"),
        }

        // First rx should have received the empty-token handshake from subscribe
        // (sent via try_send in the route handler, but here we test send_thread_event directly)
        drop(rx);
    }

    #[tokio::test]
    async fn send_thread_event_cleans_dead_senders() {
        let state = make_state();
        let (tx, rx) = mpsc::channel::<ThreadEvent>(1);
        {
            let mut map = state.thread_senders.lock().unwrap();
            map.entry("t4".to_string()).or_default().push(tx);
        }
        // Drop the receiver — sender is now dead.
        drop(rx);

        // This should not panic; it should clean up the dead sender.
        state.send_thread_event(
            "t4",
            ThreadEvent::Token {
                token: "x".to_string(),
            },
        );

        let map = state.thread_senders.lock().unwrap();
        assert!(
            !map.contains_key("t4"),
            "dead sender should have been cleaned up"
        );
    }

    // ── Global broadcast ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn send_global_event_delivers_to_subscriber() {
        let state = make_state();
        let mut rx = state.subscribe_global();

        let event = GlobalEvent::ProviderStatus {
            provider_id: "copilot".to_string(),
            status: "connected".to_string(),
            reason: None,
        };
        state.send_global_event(event).unwrap();

        let received = rx.try_recv().expect("should have received event");
        match received {
            GlobalEvent::ProviderStatus {
                provider_id,
                status,
                ..
            } => {
                assert_eq!(provider_id, "copilot");
                assert_eq!(status, "connected");
            }
            _ => panic!("wrong event"),
        }
    }

    #[tokio::test]
    async fn send_global_no_receivers_is_ok() {
        let state = make_state();
        // No subscribers — should not panic, just return an error we ignore.
        let result = state.send_global_event(GlobalEvent::ThreadUpdated {
            thread_id: "t1".to_string(),
            last_message: "hello".to_string(),
            updated_at: "2025-01-01T00:00:00.000Z".to_string(),
        });
        // May be Ok(0) or Err depending on broadcast semantics — either is fine.
        let _ = result;
    }

    // ── ThreadEvent serialisation ──────────────────────────────────────────────

    #[test]
    fn thread_event_token_event_name() {
        let e = ThreadEvent::Token {
            token: "hi".to_string(),
        };
        assert_eq!(e.event_name(), "token");
    }

    #[test]
    fn thread_event_message_complete_event_name() {
        let e = ThreadEvent::MessageComplete {
            id: "m1".to_string(),
            thread_id: "t1".to_string(),
            role: "assistant".to_string(),
            content: "hello".to_string(),
            created_at: "2025-01-01".to_string(),
            stopped: false,
        };
        assert_eq!(e.event_name(), "message_complete");
    }

    #[test]
    fn thread_event_error_event_name() {
        let e = ThreadEvent::Error {
            code: "PROVIDER_UNAVAILABLE".to_string(),
            message: "copilot-api is down".to_string(),
        };
        // Named "stream_error" to avoid collision with EventSource's built-in
        // "error" event (which fires for connection errors, not server errors).
        assert_eq!(e.event_name(), "stream_error");
    }

    #[test]
    fn thread_event_routine_message_event_name() {
        let e = ThreadEvent::RoutineMessage {
            id: "m1".to_string(),
            thread_id: "t1".to_string(),
            role: "assistant".to_string(),
            content: "good morning".to_string(),
            routine_id: "r1".to_string(),
            created_at: "2025-01-01".to_string(),
        };
        assert_eq!(e.event_name(), "routine_message");
    }

    #[test]
    fn thread_event_serialises_correctly() {
        let e = ThreadEvent::Token {
            token: "world".to_string(),
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["event"], "token");
        assert_eq!(json["token"], "world");
    }
}

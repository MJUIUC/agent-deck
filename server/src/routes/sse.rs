use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use std::time::Duration;

use crate::{error::AppResult, routes::AppState};

/// GET /api/threads/:id/stream
///
/// Per-thread Server-Sent Events stream. Clients connect here to receive:
/// - `token` — a single streamed LLM token during generation
/// - `message_complete` — the full assistant message once streaming is done
/// - `routine_message` — a message produced by a routine firing
/// - `error` — a streaming or provider error
///
/// Phase 1 stub: emits a single `connected` event then keeps the connection
/// alive with 30-second heartbeat pings. Real token streaming is wired up
/// in Phase 2 when the agent run-loop is implemented.
pub async fn thread_stream(
    State(_state): State<AppState>,
    Path(thread_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let stream = stream::once(async move {
        Ok::<Event, Infallible>(
            Event::default()
                .event("connected")
                .data(format!(r#"{{"thread_id":"{}"}}"#, thread_id)),
        )
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    ))
}

/// GET /api/events
///
/// Global Server-Sent Events stream. Clients connect here to receive:
/// - `thread_updated` — a thread's last message or unread count changed
/// - `routine_fired` — a routine ran (includes thread_id for navigation)
///
/// Phase 1 stub: emits a single `connected` event then keeps the connection
/// alive with 30-second heartbeat pings. Real event broadcasting is wired
/// up in Phase 2 alongside the agent run-loop and in Phase 4 for routines.
pub async fn global_stream(State(_state): State<AppState>) -> AppResult<impl IntoResponse> {
    let stream = stream::once(async {
        Ok::<Event, Infallible>(
            Event::default()
                .event("connected")
                .data(r#"{"service":"agent-deck"}"#),
        )
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    ))
}

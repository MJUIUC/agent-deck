//! Web Push notification dispatch.
//!
//! Called after a routine agent run completes.  Sends a push notification to
//! every registered subscription for the user, **unless** an SSE client is
//! currently connected to the thread (in which case the notification is
//! suppressed — the client is already live and will receive the update via SSE).

use anyhow::anyhow;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};
use web_push::{ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder};

use crate::models::push_subscription::PushSubscription;
use crate::routes::AppState;

/// Send a Web Push notification to every registered subscription for `user_id`,
/// suppressing the notification if an SSE client is already connected to `thread_id`.
///
/// All errors are logged and swallowed — a push failure must never abort a routine run.
pub async fn send_routine_push_notifications(
    state: &AppState,
    thread_id: &str,
    user_id: &str,
    title: &str,
    body_text: &str,
) {
    // ── Guard: SSE client is already watching — no push needed ────────────────
    if state.has_thread_subscriber(thread_id) {
        debug!(thread_id = %thread_id, "push: SSE client connected — skipping notification");
        return;
    }

    // ── Load subscriptions ────────────────────────────────────────────────────
    let subs: Vec<PushSubscription> = match sqlx::query_as(
        "SELECT id, user_id, endpoint, p256dh, auth, user_agent, created_at, updated_at
         FROM push_subscriptions
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!(thread_id = %thread_id, error = %e, "push: failed to load subscriptions");
            return;
        }
    };

    if subs.is_empty() {
        debug!(thread_id = %thread_id, "push: no subscriptions registered — skipping");
        return;
    }

    // ── Build payload JSON ────────────────────────────────────────────────────
    let payload_json = serde_json::json!({
        "title": title,
        "body": body_text,
        "data": { "thread_id": thread_id }
    })
    .to_string();
    let payload_bytes = payload_json.as_bytes().to_vec();

    let client = reqwest::Client::new();

    // ── Dispatch to each subscription ─────────────────────────────────────────
    for sub in &subs {
        match send_one(
            &client,
            &state.pool,
            &state.vapid_private_pem,
            sub,
            &payload_bytes,
        )
        .await
        {
            Ok(()) => {
                info!(
                    thread_id = %thread_id,
                    endpoint = %sub.endpoint,
                    "push: notification sent"
                );
            }
            Err(e) => {
                warn!(
                    thread_id = %thread_id,
                    endpoint = %sub.endpoint,
                    error = %e,
                    "push: failed to send notification (subscription kept)"
                );
            }
        }
    }
}

/// Returns `true` when a push notification should be sent for the given thread.
/// A notification is warranted when no SSE client is currently connected.
///
/// This thin wrapper exists so the decision logic can be unit-tested without
/// spinning up a full server.
pub fn should_notify(has_sse_subscriber: bool) -> bool {
    !has_sse_subscriber
}

// ── Private helpers ────────────────────────────────────────────────────────────

/// Build and dispatch a single push notification.  Returns `Ok(())` on success
/// **and** on 410 Gone (subscription is deleted before returning).
/// Returns `Err` for all other failure modes.
async fn send_one(
    client: &reqwest::Client,
    pool: &SqlitePool,
    vapid_private_pem: &str,
    sub: &PushSubscription,
    payload_bytes: &[u8],
) -> anyhow::Result<()> {
    let sub_info = SubscriptionInfo::new(&sub.endpoint, &sub.p256dh, &sub.auth);

    let sig = VapidSignatureBuilder::from_pem(
        std::io::Cursor::new(vapid_private_pem.as_bytes()),
        &sub_info,
    )
    .map_err(|e| anyhow!("vapid sig builder: {}", e))?
    .build()
    .map_err(|e| anyhow!("vapid sig build: {}", e))?;

    let mut builder = WebPushMessageBuilder::new(&sub_info);
    builder.set_payload(ContentEncoding::Aes128Gcm, payload_bytes);
    builder.set_vapid_signature(sig);
    builder.set_ttl(86400); // 24-hour TTL

    let message = builder
        .build()
        .map_err(|e| anyhow!("web push build: {}", e))?;

    // Assemble the reqwest request from WebPushMessage's public fields.
    // (web-push uses http 0.2, reqwest uses http 1.x — they cannot share types
    //  directly, so we extract URL, TTL, and crypto headers manually.)
    let url = message.endpoint.to_string();
    let mut req = client.post(&url).header("TTL", message.ttl.to_string());

    if let Some(payload) = message.payload {
        // web-push 0.11 only puts `Authorization` in crypto_headers for Aes128Gcm;
        // `Content-Encoding` and `Content-Type` are not included and must be set
        // explicitly, otherwise FCM forwards the blob without encoding metadata and
        // Chrome cannot decrypt the payload (event.data arrives as null in the SW).
        req = req
            .header("Content-Encoding", payload.content_encoding.to_str())
            .header("Content-Type", "application/octet-stream");
        for (name, value) in &payload.crypto_headers {
            req = req.header(*name, value);
        }
        req = req.body(payload.content);
    }

    let response = req.send().await.map_err(|e| anyhow!("push send: {}", e))?;
    let status = response.status().as_u16();

    match status {
        200..=299 => {
            info!(endpoint = %sub.endpoint, status = %status, "push: FCM accepted (2xx)");
            Ok(())
        }
        410 => {
            // Subscription is gone — remove the stale row.
            warn!(
                endpoint = %sub.endpoint,
                "push: endpoint returned 410 Gone — removing stale subscription"
            );
            if let Err(e) = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
                .bind(&sub.endpoint)
                .execute(pool)
                .await
            {
                warn!(endpoint = %sub.endpoint, error = %e, "push: failed to delete stale subscription");
            }
            Ok(())
        }
        other => {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            Err(anyhow!(
                "push: endpoint returned status {} — body: {}",
                other,
                body
            ))
        }
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_notify_when_no_sse_subscriber() {
        assert!(
            should_notify(false),
            "must notify when no SSE client is connected"
        );
    }

    #[test]
    fn should_not_notify_when_sse_subscriber_present() {
        assert!(
            !should_notify(true),
            "must NOT notify when SSE client is connected"
        );
    }
}

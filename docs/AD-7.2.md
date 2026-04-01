# AD-7.2 — Web Push Dispatch from Server

**Story:** 7.2 — Web Push dispatch from server
**Branch:** `feature/phase7-web-push-dispatch`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 7

---

## Summary

Wires Web Push notification dispatch into the routine execution path. After a routine
agent run completes, the server checks whether any SSE client is currently connected
to the thread. If no client is connected, it dispatches a Web Push notification to
every `push_subscriptions` row belonging to the user, using VAPID signing (private key
loaded from `app_config`) and the `web-push` crate for message construction. If a push
endpoint returns HTTP 410 (Gone), the stale subscription row is deleted automatically.
No new external dependencies are required — `web-push` and `reqwest` are already in the
workspace.

---

## Current State

### What Story 7.1 delivered (all on this branch — not yet merged to `dev`):

- **`server/src/services/vapid.rs`** — `get_or_create_vapid_keys(pool) -> (private_pem, public_b64url)`.
- **`server/src/db/migrations/011_push_subscriptions.sql`** — the `push_subscriptions` table (indexed on `(user_id, endpoint)`).
- **`server/src/models/push_subscription.rs`** — `PushSubscription`, `CreatePushSubscription`, `DeletePushSubscription`.
- **`server/src/routes/push.rs`** — `get_vapid_public_key`, `subscribe`, `unsubscribe` handlers.
- **`AppState`** (`routes/mod.rs`) — has `vapid_public_key: String` (cached public key). The private PEM is **not** stored in AppState yet — it is loaded from `app_config` in `build_router` but immediately discarded with `let (_vapid_private_pem, vapid_public_key) = ...`.
- **`has_thread_subscriber(thread_id)`** on `AppState` (`routes/sse.rs`) — returns `true` if at least one SSE client is connected to the given thread stream.
- **`web-push = { version = "0.11.0", default-features = false }`** — workspace dep. The `default-features = false` flag disables the bundled isahc client; the low-level message-building API (`SubscriptionInfo`, `VapidSignatureBuilder`, `WebPushMessageBuilder`, `ContentEncoding`) and the public struct fields on `WebPushMessage` / `WebPushPayload` are all still available without any feature flags.
- **`reqwest`** — already in workspace deps. Used as the HTTP client to send the built push request.

### HTTP client design note

`web-push` uses `http = "0.2"` internally; `reqwest = "0.12"` uses `http = "1.x"`.
`request_builder::build_request` returns an `http 0.2` `Request`, which cannot be fed
directly to reqwest. Instead, `WebPushMessage` exposes its fields publicly:
- `message.endpoint` (`http 0.2 Uri`) — `.to_string()` gives the endpoint URL string.
- `message.ttl` — added as `TTL` header.
- `message.payload` (`Option<WebPushPayload>`) — `.content: Vec<u8>` is the body;
  `.crypto_headers: Vec<(&'static str, String)>` are the encryption headers to forward.

This means the push request can be assembled manually with reqwest from these fields,
and `web_push::request_builder::parse_response` is not needed (we handle 410 directly).

### Routine completion hook

`server/src/services/agent.rs` `run_inner` already:
1. Updates `routine_executions` status to `'completed'`.
2. Fetches the routine name and emits `GlobalEvent::RoutineFired`.
3. Emits `GlobalEvent::ThreadUpdated`.

The push dispatch call belongs **between steps 1 and 2** — after the DB write confirms
success, before global events fire.

---

## Implementation Plan

### Task 1 — Add `vapid_private_pem` to `AppState` and store it at startup

- **Files:** `server/src/routes/mod.rs`
- **Change:**
  1. Add a new field to `AppState` immediately after `vapid_public_key`:
     ```rust
     /// PKCS8 PEM-encoded VAPID private key. Loaded from app_config at startup.
     /// Used by the push dispatch service. Never logged or returned by any endpoint.
     pub vapid_private_pem: String,
     ```
  2. In `build_router`, change the existing destructuring from:
     ```rust
     let (_vapid_private_pem, vapid_public_key) =
         crate::services::vapid::get_or_create_vapid_keys(&pool).await?;
     ```
     to:
     ```rust
     let (vapid_private_pem, vapid_public_key) =
         crate::services::vapid::get_or_create_vapid_keys(&pool).await?;
     ```
  3. Add `vapid_private_pem` to the `AppState { ... }` struct literal in `build_router`.
  4. Update the three `AppState` construction sites in `mod tests` to include
     `vapid_private_pem: String::new()`.
- **Why:** The push dispatch function in Task 2 needs the private key. Loading it once
  at startup (already done by `vapid::get_or_create_vapid_keys`) and caching it in
  AppState avoids a DB query on every routine fire.

### Task 2 — Create `services/push.rs` (push dispatch logic)

- **Files:** `server/src/services/push.rs` (new), `server/src/services/mod.rs`

- **Public API:**
  ```rust
  /// Send a Web Push notification to every registered subscription for `user_id`,
  /// unless an SSE client is currently connected to `thread_id`.
  ///
  /// Errors are logged and swallowed — a push failure must never abort a routine run.
  pub async fn send_routine_push_notifications(
      state: &AppState,
      thread_id: &str,
      user_id: &str,
      title: &str,      // e.g. "🦉 Aldous"
      body_text: &str,  // first 100 chars of routine response
  )
  ```

- **Logic:**
  1. Call `state.has_thread_subscriber(thread_id)`. If `true`, log at debug level
     ("push: SSE client connected — skipping notification") and return.
  2. Load all subscriptions for `user_id`:
     ```sql
     SELECT id, user_id, endpoint, p256dh, auth, user_agent, created_at, updated_at
     FROM push_subscriptions
     WHERE user_id = ?
     ```
     If the list is empty, return immediately.
  3. Build the JSON payload string:
     ```json
     { "title": "<title>", "body": "<body_text>", "data": { "thread_id": "<thread_id>" } }
     ```
  4. Create a `reqwest::Client::new()` (one per call — cheap enough for background
     routine invocations).
  5. For each subscription, call the private helper `send_one(...)`. Errors are
     warned and skipped; 410 causes the subscription row to be deleted.

- **Private helper `async fn send_one`:**
  ```rust
  async fn send_one(
      client: &reqwest::Client,
      pool: &sqlx::SqlitePool,
      vapid_private_pem: &str,
      sub: &PushSubscription,
      payload_bytes: &[u8],
  ) -> anyhow::Result<()>
  ```
  Implementation steps:
  1. Build `SubscriptionInfo::new(&sub.endpoint, &sub.p256dh, &sub.auth)`.
  2. Build VAPID signature:
     ```rust
     let sig = VapidSignatureBuilder::from_pem(
         std::io::Cursor::new(vapid_private_pem.as_bytes()),
         &sub_info,
     )?.build()?;
     ```
  3. Build message:
     ```rust
     let mut builder = WebPushMessageBuilder::new(&sub_info);
     builder.set_payload(ContentEncoding::Aes128Gcm, payload_bytes);
     builder.set_vapid_signature(sig);
     builder.set_ttl(86400); // 24-hour TTL
     let message = builder.build()?;
     ```
  4. Assemble reqwest request from `WebPushMessage` public fields:
     ```rust
     let url = message.endpoint.to_string();
     let mut req = client.post(&url).header("TTL", message.ttl.to_string());
     if let Some(payload) = message.payload {
         for (name, value) in &payload.crypto_headers {
             req = req.header(*name, value);
         }
         req = req.body(payload.content);
     }
     let response = req.send().await?;
     let status = response.status().as_u16();
     ```
  5. Handle response:
     - `200..=299` → success, log at debug.
     - `410` → subscription revoked; `DELETE FROM push_subscriptions WHERE endpoint = ?`;
       log a warning; return `Ok(())` (not an error — subscription was cleaned up).
     - anything else → return `Err(anyhow!("push: endpoint returned {}", status))`.

- **Wire-in:** Add `pub mod push;` to `server/src/services/mod.rs`.

### Task 3 — Wire push dispatch into `agent.rs`

- **Files:** `server/src/services/agent.rs`
- **Change:** In `run_inner`, inside the
  `if let (Some(ref rid), Some(ref exec_id)) = (&routine_id_opt, &execution_id)` block,
  **after** the `UPDATE routine_executions SET status = 'completed'` query and
  **before** the `state.send_global_event(GlobalEvent::RoutineFired {...})` call:

  ```rust
  // ── Web Push dispatch ─────────────────────────────────────────────────────
  // Send a push notification if no SSE client is watching this thread.
  // The notification body is the first 100 chars of the assistant response.
  let push_title = format!("{} {}", persona.emoji, persona.name);
  let push_body: String = gen_result.content.chars().take(100).collect();
  crate::services::push::send_routine_push_notifications(
      state,
      thread_id,
      user_id,
      &push_title,
      &push_body,
  )
  .await;
  ```

  Note: `persona`, `user_id`, and `gen_result` are all already in scope at this point.

### Schema changes

None. The `push_subscriptions` table was created in Story 7.1 (migration `011`).
No new migrations are required. No `cargo sqlx prepare` run is needed.

### Parallelisation note

- Task 1 (AppState edit) must complete before Task 2 (push service) is written,
  because `send_routine_push_notifications` reads `state.vapid_private_pem`.
- Task 2 must complete before Task 3 (agent.rs wire-in), because agent.rs calls
  the push function by path.
- **All three tasks are sequential.** Assign to a single sub-agent.

---

## Acceptance Criteria

*(Copied verbatim from PLAN_3.md §Story 7.2)*

- [ ] Notification is sent when no SSE client is connected for the thread
- [ ] Notification is NOT sent when an SSE client is connected
- [ ] 410 responses from push endpoints cause the subscription to be deleted
- [ ] Routine execution wires up the dispatch (fulfils the TODO from Story 5.4 step 10)
- [ ] Unit test for the "should notify" decision logic

---

## Human Review Instructions

*Written after implementation is complete — see Step 5. Leave blank until then.*

---

## Approval

- [ ] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off
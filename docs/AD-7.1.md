# AD-7.1 — VAPID Key Generation and Server Endpoints

**Story:** 7.1 — VAPID key generation and server endpoints
**Branch:** `feature/phase7-vapid-server`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 7

---

## Summary

Lays the server-side foundation for Web Push notifications. On startup the server generates a VAPID P-256 key pair and persists both keys in `app_config`. Three new endpoints expose the public key to clients, and allow authenticated users to register or deregister browser push subscriptions stored in a new `push_subscriptions` table. No notifications are dispatched yet — that is Story 7.2.

---

## Current State

- `app_config` table stores arbitrary key/value pairs; `models/app_config.rs` has a `keys` module with well-known constants.
- `services/credentials.rs` and `services/auth.rs` both demonstrate the "get or create on first run" pattern against `app_config`.
- `server/Cargo.toml` inherits all deps from the workspace `Cargo.toml`. Neither `p256` nor `web-push` are currently declared.
- The last migration is `010_phase5_summarization.sql`. The next file must be `011_push_subscriptions.sql`.
- `routes/mod.rs` declares all route modules, registers every route, and defines `AppState`. It is ~420 lines and must be edited carefully.

---

## Implementation Plan

### Task 1 — Cargo dependencies

- Files: `Cargo.toml` (workspace root), `server/Cargo.toml`
- Add to `[workspace.dependencies]`:
  ```toml
  p256 = { version = "0.13", features = ["pkcs8", "pem"] }
  web-push = { version = "0.11.0", default-features = false }
  ```
  `default-features = false` omits the `isahc-client` HTTP transport — we do not need it in Story 7.1 and will decide on the dispatch transport in Story 7.2.
- Add to `server/Cargo.toml` under `[dependencies]`:
  ```toml
  p256 = { workspace = true }
  web-push = { workspace = true }
  ```

### Task 2 — app_config keys

- Files: `server/src/models/app_config.rs`
- Add two constants to the `keys` module:
  ```rust
  /// PKCS8 PEM-encoded VAPID private key. Generated once on first server run.
  /// Never logged or returned by any API endpoint.
  pub const VAPID_PRIVATE_KEY: &str = "vapid_private_key";

  /// Base64url-encoded (no padding) uncompressed P-256 public key (65 raw bytes).
  /// Safe to expose publicly — returned by GET /api/push/vapid-public-key.
  pub const VAPID_PUBLIC_KEY: &str = "vapid_public_key";
  ```

### Task 3 — Migration: push_subscriptions table

- Files: `server/src/db/migrations/011_push_subscriptions.sql` (new)
- Schema:
  ```sql
  CREATE TABLE IF NOT EXISTS push_subscriptions (
      id          TEXT PRIMARY KEY,
      user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      endpoint    TEXT NOT NULL,
      p256dh      TEXT NOT NULL,
      auth        TEXT NOT NULL,
      user_agent  TEXT NOT NULL DEFAULT '',
      created_at  TEXT NOT NULL,
      updated_at  TEXT NOT NULL,
      UNIQUE(user_id, endpoint)
  );
  ```
  The `UNIQUE(user_id, endpoint)` constraint enables upsert-by-endpoint within a user.

### Task 4 — Push subscription model

- Files: `server/src/models/push_subscription.rs` (new), `server/src/models/mod.rs`
- `PushSubscription` — `sqlx::FromRow`, full DB row, all fields public.
- `CreatePushSubscription` — `serde::Deserialize`, fields: `endpoint: String`, `p256dh: String`, `auth: String`, `user_agent: Option<String>`.
- `DeletePushSubscription` — `serde::Deserialize`, field: `endpoint: String`.
- `impl PushSubscription { pub fn new(user_id, payload: CreatePushSubscription) -> Self }` — sets `id` (UUID v4), `created_at`, `updated_at`.
- Add `pub mod push_subscription;` to `models/mod.rs`.

### Task 5 — VAPID key service

- Files: `server/src/services/vapid.rs` (new), `server/src/services/mod.rs`
- One public function:
  ```rust
  pub async fn get_or_create_vapid_keys(pool: &SqlitePool) -> Result<(String, String)>
  // Returns (private_key_pem, public_key_b64url)
  ```
- Logic:
  1. Check `app_config` for both `VAPID_PRIVATE_KEY` and `VAPID_PUBLIC_KEY`. If both exist, return them.
  2. Otherwise generate a fresh P-256 key pair using `p256`:
     ```rust
     use p256::{SecretKey, pkcs8::EncodePrivateKey};
     use p256::elliptic_curve::sec1::ToEncodedPoint;
     use base64::engine::general_purpose::URL_SAFE_NO_PAD;
     use base64::Engine;

     let secret = SecretKey::random(&mut rand::thread_rng());
     let pem = secret.to_pkcs8_pem(Default::default())?.to_string();
     let point = secret.public_key().to_encoded_point(false); // uncompressed
     let public_b64 = URL_SAFE_NO_PAD.encode(point.as_bytes()); // 65 bytes → 87 chars
     ```
  3. Persist both with `INSERT INTO app_config ... ON CONFLICT DO NOTHING` to be safe against races.
  4. Log `"vapid: generated new key pair (first run)"` — never log the key values.
  5. Return `(pem, public_b64)`.
- Add `pub mod vapid;` to `services/mod.rs`.
- Unit tests in a `mod tests` block (use `sqlite::memory:` + migrations):
  - `test_vapid_keys_stable_across_calls` — call twice, assert returned strings are identical.
  - `test_vapid_public_key_is_valid_base64url` — assert the 87-char no-pad base64url string decodes to exactly 65 bytes and the first byte is `0x04` (uncompressed point marker).
  - `test_vapid_private_key_is_pem` — assert the PEM string starts with `"-----BEGIN"`.

### Task 6 — Push route handlers

- Files: `server/src/routes/push.rs` (new)
- Three handlers, following the exact pattern in `routes/tokens.rs` (use `get_user_id` helper, `State(state): State<Arc<AppState>>`, return `AppResult<impl IntoResponse>`):

  **`pub async fn get_vapid_public_key`**
  - No auth required (registered in `public_api`).
  - Reads `state.vapid_public_key` directly from AppState (no DB round-trip).
  - Returns `200 { "data": { "public_key": "<base64url>" } }`.

  **`pub async fn subscribe`**
  - Authenticated (registered in `protected_api`).
  - Accepts `Json(payload): Json<CreatePushSubscription>`.
  - Validates `endpoint` is non-empty; returns `400` otherwise.
  - Upserts: `INSERT INTO push_subscriptions ... ON CONFLICT(user_id, endpoint) DO UPDATE SET p256dh=excluded.p256dh, auth=excluded.auth, user_agent=excluded.user_agent, updated_at=excluded.updated_at`.
  - Returns `200 { "data": { "subscribed": true } }`.

  **`pub async fn unsubscribe`**
  - Authenticated (registered in `protected_api`).
  - Accepts `Json(payload): Json<DeletePushSubscription>`.
  - `DELETE FROM push_subscriptions WHERE endpoint = ? AND user_id = ?`.
  - If `rows_affected() == 0`, returns `404`.
  - Returns `200 { "data": { "deleted": true } }`.

### Task 7 — Wire everything into routes/mod.rs and AppState

- Files: `server/src/routes/mod.rs`
- Add `pub mod push;` alongside the other module declarations.
- Add `vapid_public_key: String` field to `AppState` (after `scheduler_tx`). No `Arc` needed — it is a cheap clone of a short string.
- In `build_router`, after `get_or_create_master_key` and before the router is constructed:
  ```rust
  let (_vapid_private_pem, vapid_public_key) =
      crate::services::vapid::get_or_create_vapid_keys(&pool).await?;
  tracing::info!("vapid: public key loaded ({}…)", &vapid_public_key[..8]);
  ```
  The private PEM is loaded but not stored in AppState for this story (it will be loaded fresh in Story 7.2 when the dispatch service is initialised).
- Add `vapid_public_key` to the `AppState { ... }` initialiser.
- In `public_api`, add:
  ```rust
  .route("/api/push/vapid-public-key", get(push::get_vapid_public_key))
  ```
- In `protected_api`, add:
  ```rust
  .route(
      "/api/push/subscribe",
      axum::routing::post(push::subscribe).delete(push::unsubscribe),
  )
  ```

### Schema changes

Migration `011_push_subscriptions.sql` is a new file. `cargo sqlx prepare` must be run after implementation and the updated `.sqlx/` directory committed.

### Parallelisation note

- **Tasks 1 + 2** (Cargo.toml + app_config keys) — share no files with each other and can run simultaneously with Tasks 3–5. Start all in parallel.
- **Tasks 3 + 4 + 5** (migration, model, VAPID service) — no shared files; run in parallel with each other and with Tasks 1 + 2.
- **Tasks 6 + 7** (route handler + wiring) — depend on Tasks 1–5 being complete. Run after all prior tasks finish. These two touch different files (`push.rs` is new; `mod.rs` is an edit) and can themselves be parallelised.

Recommended agent split:
- **Sub-agent A**: Tasks 1 + 2 (Cargo.toml workspace, server/Cargo.toml, app_config.rs)
- **Sub-agent B**: Tasks 3 + 4 + 5 (migration SQL, push_subscription model + models/mod.rs, vapid service + services/mod.rs)
- After A and B complete → **Sub-agent C**: Tasks 6 + 7 (routes/push.rs, routes/mod.rs)

---

## Acceptance Criteria

- [ ] VAPID keys are generated once and persist across server restarts
- [ ] Private key is never logged or returned by any endpoint
- [ ] All three endpoints work correctly
- [ ] `push_subscriptions` table is created by migration `011_push_subscriptions.sql`
- [ ] Unit tests for key generation idempotency (calling generate twice returns the same key)

---

## Human Review Instructions

**Prerequisites:** Server built and running on port 7474. Auth token visible in server startup log.

**Step 1 — Start the server and note the auth token**

```
cd agent-deck && cargo run
```

Look for the startup banner in the terminal output:
```
║  Auth token: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx  ║
║             xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx  ║
```

Copy the full 64-character token (both lines joined). You will need it for authenticated requests below.

**Step 2 — GET /api/push/vapid-public-key (public, no auth)**

```
curl -s http://localhost:7474/api/push/vapid-public-key | jq .
```

**Expected:** HTTP 200 with a JSON body like:
```json
{ "data": { "public_key": "BK3x..." } }
```
The `public_key` value should be an 87-character base64url string (no `=` padding).
**Failure sign:** Empty body, 500 error, or a key shorter/longer than 87 chars.

**Step 3 — Verify VAPID keys persist across restarts**

Note the `public_key` value from Step 2. Stop the server (`Ctrl-C`), restart it (`cargo run`), then run the same curl again. The `public_key` must be **identical** to what you saw before the restart.
**Failure sign:** A different public key value after restart.

**Step 4 — POST /api/push/subscribe (authenticated)**

Replace `<TOKEN>` with your auth token from Step 1:

```
curl -s -X POST http://localhost:7474/api/push/subscribe \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"endpoint":"https://push.example.com/test-sub-1","p256dh":"BNbxBq1mNMQ8test","auth":"abc123test","user_agent":"Test/1.0"}' \
  | jq .
```

**Expected:** HTTP 200, `{ "data": { "subscribed": true } }`.
**Failure sign:** 401 (bad token), 400 (missing endpoint), or 500.

**Step 5 — POST /api/push/subscribe upsert (same endpoint, different keys)**

Run the same curl again with different `p256dh`/`auth` values to confirm upsert works without a duplicate-key error:

```
curl -s -X POST http://localhost:7474/api/push/subscribe \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"endpoint":"https://push.example.com/test-sub-1","p256dh":"UPDATED_KEY","auth":"UPDATED_AUTH"}' \
  | jq .
```

**Expected:** HTTP 200, `{ "data": { "subscribed": true } }` (no error — row updated in place).

**Step 6 — DELETE /api/push/subscribe (authenticated)**

```
curl -s -X DELETE http://localhost:7474/api/push/subscribe \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"endpoint":"https://push.example.com/test-sub-1"}' \
  | jq .
```

**Expected:** HTTP 200, `{ "data": { "deleted": true } }`.
**Failure sign:** 404 (subscription not found), 401, or 500.

**Step 7 — DELETE non-existent subscription returns 404**

Run the same DELETE again (the row was just deleted):

```
curl -s -o /dev/null -w "%{http_code}" -X DELETE http://localhost:7474/api/push/subscribe \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"endpoint":"https://push.example.com/test-sub-1"}'
```

**Expected:** `404`.

**Step 8 — Private key is not exposed**

Confirm the private key is not returned by the public key endpoint (it should only contain `public_key`):

```
curl -s http://localhost:7474/api/push/vapid-public-key | jq 'keys'
```

**Expected:** `["data"]` with only `public_key` inside — no `private_key` field anywhere.

---

## Approval

- [x] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off

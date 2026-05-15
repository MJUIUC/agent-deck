# Integration Testing Guide

## TL;DR

**Prefer in-process tests over live-server curl tests.**  
All route modules have a `mod tests` block that spins up an in-memory SQLite
router via `tower::ServiceExt::oneshot`.  These tests are self-contained,
repeatable, and fast — run them with:

```bash
cargo test -p server                        # all tests
cargo test -p server routines               # filter by module
cargo test -p server test_create_routine    # filter by name
```

---

## In-Process Test Pattern

Every route file follows the same setup:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}, Router};
    use tower::ServiceExt;

    /// Fresh in-memory router + auth token.
    async fn test_app() -> (Router, String) { ... }

    /// Router + user + thread (most tests need this).
    async fn setup_app() -> (Router, String, String) { ... }

    #[tokio::test]
    async fn test_something() {
        let (app, token, thread_id) = setup_app().await;
        let resp = app.oneshot(Request::builder()
            .method("POST")
            .uri(format!("/api/threads/{}/routines", thread_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"x","prompt":"y","cron_expr":"0 9 * * *"}"#))
            .unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
}
```

Key properties:
- Uses `sqlite::memory:` — no file on disk, no cleanup needed
- Runs all migrations on startup (`sqlx::migrate!`)
- Each test gets its own isolated DB — tests cannot interfere
- No network required, no running server

---

## When You Need a Live Server

Occasionally you need to test things the in-process harness can't reach
(scheduler firing, SSE streams, push notifications). When that's the case:

### Starting the server

```bash
# From repo root — starts on port 7474
PUBLIC_DIR="$(pwd)/server/public" cargo run --bin server &> /tmp/adeck.log &
disown

# Wait for ready
until grep -q "Listening on port" /tmp/adeck.log 2>/dev/null; do sleep 1; done
echo "Server ready"

# Extract auth token
TOKEN=$(grep -a "Auth token:" /tmp/adeck.log \
  | head -1 \
  | grep -oP '[a-f0-9]{32}' \
  | tr -d '\n')
# Token is split across two log lines — concatenate both hex strings:
TOKEN=$(grep -a "║  [a-f0-9]" /tmp/adeck.log \
  | grep -oP '[a-f0-9]{32}' \
  | tr -d '\n')
echo "TOKEN=$TOKEN"

BASE="http://localhost:7474/api"
```

### Stopping the server

```bash
kill $(lsof -ti :7474) 2>/dev/null
```

### Rules for live-server tests

1. **Always clean up** created resources (DELETE after assertions).
2. **Never depend on existing data** — create everything you need at test start.
3. **Token is stable** between restarts for the same `~/.agent-deck` data dir.
4. **Live DB is not reset between tests** — use unique names to avoid collisions.

---

## Adding Tests for a New Feature

When you add a new field or endpoint:

1. Find the `mod tests` block in the relevant route file.
2. Add tests covering: happy path, missing optional field defaults, invalid value → 400.
3. Use the existing `setup_app()` / `create_*` helpers — don't duplicate setup logic.
4. Run `cargo test -p server <module_name>` to confirm all pass before committing.

Example for a new field `foo` on routines:

```rust
#[tokio::test]
async fn test_create_routine_with_foo() { ... }

#[tokio::test]
async fn test_create_routine_foo_defaults() { ... }

#[tokio::test]
async fn test_create_routine_invalid_foo() { ... }
```

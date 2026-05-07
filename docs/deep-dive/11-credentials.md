# 11 — Credentials

---

## Overview

Vestry has a two-layer encryption scheme:

1. **Machine secret** — a machine-specific random value stored in `app_config`. Used to encrypt provider API keys.
2. **Credential master key** — a separate AES-256-GCM key stored in `app_config`. Used to encrypt the credential store.

Both keys are stored in the SQLite database. The encryption is symmetric (AES-256-GCM) via `services/encryption.rs`.

---

## Credential store schema

```sql
CREATE TABLE credentials (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    key        TEXT NOT NULL UNIQUE,   -- lookup identifier e.g. "schwab"
    kind       TEXT NOT NULL,          -- api_key | oauth_token | service_account | basic_auth
    api_key    TEXT,                   -- AES-256-GCM encrypted
    secret_key TEXT,                   -- AES-256-GCM encrypted
    password   TEXT,                   -- AES-256-GCM encrypted
    token      TEXT,                   -- AES-256-GCM encrypted
    service_account_json TEXT,         -- AES-256-GCM encrypted
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

---

## Credential injection

Used in MCP server env var values and remote MCP config. The syntax (using angle bracket notation below to avoid triggering runtime injection):

**Basic form** — resolves `secret_key` field:
```
MY_API_TOKEN: <cred:github_pat>
```

**Field-specific form** — resolves a named field:
```
CLIENT_ID: <cred:schwab:api_key>
CLIENT_SECRET: <cred:schwab:secret_key>
```

### Supported field names
- `api_key`
- `secret_key`
- `password`
- `token`
- `service_account_json`

The bare form (no field selector) is backwards-compatible and resolves `secret_key`.

---

## Resolution functions (`services/credentials.rs`)

```rust
// Resolve the "secret_key" field by credential key name.
// Used for the bare placeholder form.
pub async fn resolve_secret(pool, master_key, cred_key) -> Result<String>

// Resolve a specific named field.
// Used for the field-specific placeholder form.
pub async fn resolve_field(pool, master_key, cred_key, field) -> Result<String>

// Inject credential placeholders in a serde_json::Value tree.
// Recursively walks the value; replaces string values matching placeholder pattern.
// Used before forwarding tool call args to MCP servers.
pub async fn inject_credentials(value: Value, pool, master_key) -> Result<Value>
```

---

## Injection flow in MCP tool calls

When `execute_mcp_tool` dispatches a tool call, it injects credentials into the args JSON before forwarding to the MCP server:

```rust
// Raw placeholder text is stored in the hidden tool call message (DB + SSE)
// Only the RESOLVED value is sent to the MCP server
let args = credentials::inject_credentials(
    args,
    &state.pool,
    &state.credential_master_key,
).await?;
state.mcp.call_tool(&s.id, tool_name, args).await
```

Secrets never reach the database.

---

## Encryption implementation

`services/encryption.rs` uses AES-256-GCM:
- Random 12-byte nonce prepended to ciphertext
- Result is hex-encoded for storage in SQLite TEXT columns

```rust
pub fn encrypt(plaintext: &str, key: &str) -> Result<String>
pub fn decrypt(ciphertext: &str, key: &str) -> Result<String>
```

---

## Credential master key lifecycle

```rust
// In build_router:
let master_key = credentials_service::get_or_create_master_key(&pool).await?;
```

On first run: generates 32 random bytes, hex-encodes, stores in `app_config`. Subsequent runs read from DB.

**Security model:** The SQLite database is on the local machine. Physical access is assumed to be controlled by the owner. The key at rest is as secure as the machine's filesystem.

---

## API

### `POST /api/credentials`
```json
{
  "key": "schwab",
  "kind": "api_key",
  "api_key": "CLIENT_ID_HERE",
  "secret_key": "CLIENT_SECRET_HERE"
}
```
Each value field is individually encrypted before storage.

### `GET /api/credentials`
Returns metadata only: `id`, `key`, `kind`, `created_at`. Value fields are **never** returned by the API.

### `PUT /api/credentials/:id`
Pass only the fields to update. Unset fields are left unchanged. Provided values are re-encrypted.

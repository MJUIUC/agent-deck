# Credential Store

The credential store is a server-side encrypted vault for API keys, tokens, and other secrets. Secrets are encrypted with AES-256-GCM at rest and are **never decrypted into the message stream or any tool call response** — the server resolves them internally at connection time. As an agent you work entirely with credential *keys* (short identifiers) and the public metadata fields. You never see, handle, or transmit the raw secret values.

## What you can and cannot do

| You can | You cannot |
|---|---|
| List and read credential metadata | Read or write the database directly |
| Create a new credential (user provides the secret in the UI, or you POST it if the user pastes it to you explicitly) | Retrieve a decrypted secret via any API call |
| Reference a credential by key inside an MCP server config | Put a raw secret value in a message, argument, or config field |
| Update metadata or rotate a secret | Bypass the placeholder system to inject secrets manually |
| Delete a credential | |

---

## Data model

| Field | Notes |
|---|---|
| `id` | UUID. Use this for GET/PUT/DELETE by record. |
| `key` | Unique machine-readable identifier, e.g. `github_pat` or `openai_key`. This is what you use in MCP config placeholders. |
| `display_name` | Human-readable label shown in the UI. |
| `service` | Optional. The service this credential belongs to, e.g. `openai`, `github`. |
| `credential_type` | One of: `api_key`, `pat`, `bearer_token`, `key_secret_pair`, `service_account`. |
| `service_url` | Optional. Base URL for the service, if relevant. |
| `username` / `email` | Optional. Non-secret identity fields, returned in API responses. |
| `created_at` / `updated_at` | ISO 8601 timestamps. |

The `encrypted_data`, `secret`, and `password` fields are **never present in any API response**. `secret` and `password` are write-only — supplied on create/update and immediately encrypted.

---

## API reference

### List all credentials

```
GET /api/credentials
```

Returns an array of public credential records. No secret data is included.

**Example response**
```json
[
  {
    "id": "a1b2c3d4-...",
    "key": "github_pat",
    "display_name": "GitHub Personal Access Token",
    "service": "github",
    "credential_type": "pat",
    "service_url": null,
    "username": "marcusjefferson",
    "email": null,
    "created_at": "2025-04-22T10:00:00Z",
    "updated_at": "2025-04-22T10:00:00Z"
  }
]
```

---

### Get a single credential

```
GET /api/credentials/:id
```

Returns one public record by ID. Useful for confirming a credential exists after creation.

---

### Create a credential

```
POST /api/credentials
Content-Type: application/json
```

**Body fields**

| Field | Required | Notes |
|---|---|---|
| `key` | ✅ | Must be unique. Use `snake_case`. Cannot be changed after creation. |
| `display_name` | ✅ | |
| `credential_type` | ✅ | `api_key`, `pat`, `bearer_token`, `key_secret_pair`, or `service_account` |
| `service` | — | Recommended. e.g. `openai`, `anthropic`, `github` |
| `service_url` | — | |
| `username` / `email` | — | Non-secret identity fields. Returned in API responses. |
| `secret` | — | The primary secret (API key, token, etc.). Write-only — encrypted immediately, never returned. |
| `password` | — | Secondary secret for `key_secret_pair` credentials. Write-only. Independent of `secret` — you can supply one without the other. |

Returns `201 Created` with the public record. Never echoes `secret` or `password` back.

**Example — single API key**
```json
{
  "key": "openai_key",
  "display_name": "OpenAI API Key",
  "service": "openai",
  "credential_type": "api_key",
  "secret": "<value the user pasted>"
}
```

**Example — key/secret pair (e.g. AWS, Schwab)**
```json
{
  "key": "schwab_trading",
  "display_name": "Schwab Trading API",
  "service": "schwab",
  "credential_type": "key_secret_pair",
  "secret": "<app key>",
  "password": "<app secret>"
}
```

> **Important:** Only POST a secret if the user has explicitly pasted it into the conversation. Never ask the user to send secrets in chat if you can direct them to enter it via the UI instead.

---

### Update a credential

```
PUT /api/credentials/:id
Content-Type: application/json
```

All fields are optional. Only supplied fields are changed.

`secret` and `password` are updated independently — supplying one preserves the other. For example, sending only `"secret"` re-encrypts the primary secret while leaving any stored `password` untouched, and vice versa. The `key` field cannot be changed after creation.

**Example — rotate the primary secret only** (existing `password` is preserved)
```json
{
  "secret": "<new value>"
}
```

**Example — update metadata only** (no re-encryption)
```json
{
  "display_name": "OpenAI Key (production)",
  "service_url": "https://api.openai.com"
}
```

---

### Delete a credential

```
DELETE /api/credentials/:id
```

Returns `204 No Content` if nothing referenced this credential.

If one or more MCP servers reference it by key, returns `200` with a warning instead of deleting silently:

```json
{
  "deleted": true,
  "warnings": [
    "MCP server 'schwab-mcp' referenced this credential"
  ]
}
```

Always check for warnings and inform the user so they can update the affected MCP server configs before the connection breaks.

---

## Referencing credentials in MCP server config

When configuring an MCP server, use placeholder strings in the `env` or `headers` fields of the config JSON. The server resolves these to the decrypted value at connection time — the agent never sees the plaintext.

### Placeholder syntax

| Placeholder | Resolves to |
|---|---|
| `{credential:<key>}` | The primary `secret` field of the credential |
| `{credential:<key>:secret}` | Same as above — explicit form |
| `{credential:<key>:password}` | The `password` field — for `key_secret_pair` credentials |

### Example — single API key in env var

```json
{
  "command": "npx",
  "args": ["-y", "@openai/mcp-server"],
  "env": {
    "OPENAI_API_KEY": "{credential:openai_key}"
  }
}
```

### Example — key/secret pair in env vars

```json
{
  "command": "npx",
  "args": ["-y", "@schwab/mcp-server"],
  "env": {
    "SCHWAB_APP_KEY": "{credential:schwab_trading:secret}",
    "SCHWAB_APP_SECRET": "{credential:schwab_trading:password}"
  }
}
```

### Example — bearer token in HTTP header (remote MCP server)

```json
{
  "url": "https://mcp.example.com/sse",
  "headers": {
    "Authorization": "Bearer {credential:example_token}"
  }
}
```

---

## Typical workflow

1. **Check what credentials already exist** — `GET /api/credentials`. Look for a `key` that matches the service you need.
2. **If the credential is missing**, ask the user to provide the secret, then `POST /api/credentials` with the value they supply. Prefer directing the user to the Credentials UI (Settings → Credentials) so the secret never passes through the conversation at all.
3. **Reference the credential by key** in the MCP server config using the `{credential:<key>}` placeholder. Never hardcode a raw secret value in the config.
4. **When deleting a credential**, check the response for `warnings` and tell the user which MCP servers need to be updated.
# Skill: Configure an MCP Server

You can add, update, enable, disable, and remove MCP servers on behalf of the user by calling the agent-deck REST API directly. MCP (Model Context Protocol) servers extend the agent with external tools — file system access, web search, database queries, API integrations, and more.

You can also **build a custom MCP server** from scratch and register it with the platform — see the dedicated section below.

---

## When to use this skill

Use this skill when the user asks you to:
- Add or connect a new MCP server
- Build a custom MCP server and register it with the platform
- Remove or disconnect an MCP server
- Enable or disable an existing MCP server
- Update an MCP server's configuration (URL, command, args, env vars)
- Store an API key securely and wire it into an MCP server
- Help troubleshoot why an MCP server isn't connecting

---

## Core concepts

| Concept | Description |
|---|---|
| **tag** | Short slug used as the server's identifier and config directory name (e.g. `filesystem`, `brave-search`). Lowercase letters, digits, and hyphens only. |
| **server_type** | `"local"` (stdio subprocess) or `"remote"` (HTTP/SSE) |
| **config** | JSON object describing how to connect — shape differs by `server_type` |
| **mcp_dir** | Root directory for all MCP implementations: `~/.agent-deck/mcp/` |
| **config.json** | Per-server file at `~/.agent-deck/mcp/<tag>/config.json` — used for persistence and startup sync |
| **credential** | An encrypted secret stored in the platform's credential store — the right way to handle API keys |

---

## Filesystem layout

Every MCP server known to the platform has a directory under `~/.agent-deck/mcp/`:

```
~/.agent-deck/mcp/
  tavily/
    config.json        ← platform registration metadata
    server.py          ← implementation (if custom-built)
    requirements.txt
  github/
    config.json
  my-custom-mcp/
    config.json
    index.js
    package.json
```

The `config.json` in each directory is the source of truth for that server's registration. On every server startup the platform scans this directory tree and syncs any new or changed `config.json` files into its database automatically.

---

## config.json schema

### Local (stdio) server

```json
{
  "id": "<uuid>",
  "name": "My Custom MCP",
  "tag": "my-custom-mcp",
  "server_type": "local",
  "config": {
    "executable": "node",
    "args": ["index.js"],
    "env": {
      "API_KEY": "{credential:my_api_key}"
    },
    "working_dir": null
  }
}
```

| Field | Required | Description |
|---|---|---|
| `id` | Yes | UUID — used to match the DB row across restarts. Generate one with `uuidgen` or any UUID tool. |
| `name` | Yes | Human-readable display name |
| `tag` | Yes | Short slug — must match the directory name |
| `server_type` | Yes | `"local"` for stdio subprocess servers |
| `config.executable` | Yes | Binary to launch (e.g. `node`, `python3`, `uvx`) |
| `config.args` | No | Array of arguments passed to the executable |
| `config.env` | No | Environment variables. Use `"{credential:<key>}"` placeholders for secrets — never paste raw keys. |
| `config.working_dir` | No | Override the working directory. Defaults to `~/.agent-deck/mcp/<tag>/` when null or absent. |

### Remote (HTTP/SSE) server

```json
{
  "id": "<uuid>",
  "name": "My Remote MCP",
  "tag": "my-remote-mcp",
  "server_type": "remote",
  "config": {
    "url": "https://mcp.example.com/mcp/",
    "credential_key": "my_api_key",
    "auth_header": "Authorization",
    "auth_format": "Bearer {value}",
    "headers": {}
  }
}
```

| Field | Required | Description |
|---|---|---|
| `id` | Yes | UUID |
| `name` | Yes | Human-readable display name |
| `tag` | Yes | Short slug — must match the directory name |
| `server_type` | Yes | `"remote"` for HTTP/SSE servers |
| `config.url` | Yes | Full URL of the MCP endpoint |
| `config.credential_key` | No | Key of a stored credential whose decrypted value is sent as the auth header |
| `config.auth_header` | No | Header name. Defaults to `Authorization` |
| `config.auth_format` | No | Header value template. Defaults to `Bearer {value}`. Use `{value}` as the placeholder for the decrypted secret. |
| `config.headers` | No | Additional static headers sent on every request |

---

## Credentials — storing and using API keys securely

**Never put raw API keys in a `config.json` or in env var values.** Use the platform's encrypted credential store instead. Credentials are encrypted with AES-256-GCM at rest using a per-installation master key that never leaves the server.

### Step 1 — Create the credential

**`POST /api/credentials`**

```json
{
  "key": "tavily_api_key",
  "display_name": "Tavily API Key",
  "service": "tavily",
  "credential_type": "api_key",
  "secret": "tvly-abc123..."
}
```

| Field | Description |
|---|---|
| `key` | Unique machine-readable identifier — this is what you reference in MCP configs. Use a descriptive slug like `tavily_api_key` or `github_pat`. |
| `display_name` | Human-readable label shown in the UI |
| `service` | Optional service name (e.g. `"tavily"`, `"github"`) |
| `credential_type` | One of: `api_key`, `pat`, `bearer_token`, `key_secret_pair`, `service_account` |
| `secret` | The raw secret value — encrypted before storage, never returned by the API |

The response returns the public record (no `secret` or `encrypted_data` field):

```json
{
  "id": "...",
  "key": "tavily_api_key",
  "display_name": "Tavily API Key",
  "service": "tavily",
  "credential_type": "api_key",
  "created_at": "..."
}
```

### Step 2 — Reference the credential in the MCP config

**For local (stdio) servers — env var placeholder:**

In the `env` map, use the syntax `"{credential:<key>}"` as the value. The platform resolves it to the decrypted secret at launch time — the raw key is never written to disk.

```json
"env": {
  "TAVILY_API_KEY": "{credential:tavily_api_key}"
}
```

**For remote (HTTP/SSE) servers — credential_key field:**

Set `credential_key` to the credential's `key` value. The platform decrypts it at connection time and injects it into the auth header using `auth_format`.

```json
"credential_key": "tavily_api_key",
"auth_header": "Authorization",
"auth_format": "Bearer {value}"
```

If the remote server expects the key in a non-standard header (e.g. `X-Api-Key: <value>`):

```json
"credential_key": "my_service_key",
"auth_header": "X-Api-Key",
"auth_format": "{value}"
```

---

## Building a custom MCP server

When a user asks you to build a custom MCP, follow this sequence:

### 1. Create the credential first

If the MCP needs an API key, store it via `POST /api/credentials` before writing any code so the key never appears in source files.

### 2. Build the implementation

Create the implementation at `~/.agent-deck/mcp/<tag>/`. The working directory is set to this path by default, so relative imports and file paths work without extra configuration.

Example layout for a Python MCP:
```
~/.agent-deck/mcp/my-search/
  server.py
  requirements.txt
```

The server must implement the MCP stdio transport (JSON-RPC over stdin/stdout). Use `fastmcp`, `mcp` (the official Python SDK), or any compatible library.

### 3. Write config.json

Create `~/.agent-deck/mcp/<tag>/config.json` using the schema above. Generate a fresh UUID for the `id` field.

### 4. Register with the platform

Write the `config.json` to disk first, then call `POST /api/mcp-servers` to register and connect immediately without requiring a server restart:

```json
{
  "name": "My Search MCP",
  "tag": "my-search",
  "server_type": "local",
  "config": {
    "executable": "python3",
    "args": ["server.py"],
    "env": {
      "SEARCH_API_KEY": "{credential:search_api_key}"
    }
  }
}
```

The platform will:
1. Insert the server into its database
2. Write (or update) `config.json` on disk
3. Attempt to connect immediately

The `config.json` on disk means the server will also be picked up automatically on every subsequent platform restart via `startup_sync`.

### 5. Attach to the thread

After registration, attach the server to the current thread so its tools are available:

```
POST /api/threads/<thread_id>/mcp-servers
{ "mcp_server_id": "<id from step 4 response>" }
```

---

## Startup sync

On every platform startup, the server scans `~/.agent-deck/mcp/` for subdirectories containing a `config.json` and syncs them into the database:

- **New config found** → inserts a new enabled row
- **Existing config changed** → updates the DB row with the new values
- **Config deleted** → disables the corresponding DB row

This means a custom MCP written to disk will be automatically discovered and registered on the next restart even without a `POST /api/mcp-servers` call. However, calling the API is preferred since it connects the server immediately without a restart.

---

## API reference

### List all MCP servers

**`GET /api/mcp-servers`**

Returns all registered servers with their current connection status.

---

### Create a new MCP server

**`POST /api/mcp-servers`**

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | Yes | Display name |
| `tag` | string | No | Slug identifier. Defaults to lowercased `name`. |
| `server_type` | `"local"` or `"remote"` | Yes | Transport type |
| `config` | object | Yes | Connection config (see schema above) |
| `description` | string | No | Optional description |
| `source_url` | string | No | Optional link to docs or source repo |

---

### Update an existing MCP server

**`PUT /api/mcp-servers/:id`**

All fields are optional. The server reconnects automatically when connection-relevant fields change.

---

### Delete an MCP server

**`DELETE /api/mcp-servers/:id`**

Disconnects and removes the server. Does not delete the `~/.agent-deck/mcp/<tag>/` directory or the implementation files.

---

### List available tools

**`GET /api/mcp-servers/:id/tools`**

Returns the tools the connected server exposes. Only works when `status` is `"connected"`.

---

### Attach to a thread

**`POST /api/threads/:thread_id/mcp-servers`**
```json
{ "mcp_server_id": "<uuid>" }
```

### Detach from a thread

**`DELETE /api/threads/:thread_id/mcp-servers/:mcp_server_id`**

---

## Common off-the-shelf MCP servers

| Server | Transport | Command | Credential env var |
|---|---|---|---|
| `@modelcontextprotocol/server-filesystem` | local | `npx -y @modelcontextprotocol/server-filesystem <path>` | — |
| `@modelcontextprotocol/server-brave-search` | local | `npx -y @modelcontextprotocol/server-brave-search` | `BRAVE_API_KEY` |
| `@modelcontextprotocol/server-github` | local | `npx -y @modelcontextprotocol/server-github` | `GITHUB_PERSONAL_ACCESS_TOKEN` |
| `@modelcontextprotocol/server-postgres` | local | `npx -y @modelcontextprotocol/server-postgres <connection-string>` | — |
| Tavily (hosted MCP) | remote | — | `credential_key` → `Authorization: Bearer {value}` |

For the npx-based servers, the `config.json` looks like:

```json
{
  "id": "<uuid>",
  "name": "Brave Search",
  "tag": "brave-search",
  "server_type": "local",
  "config": {
    "executable": "npx",
    "args": ["-y", "@modelcontextprotocol/server-brave-search"],
    "env": {
      "BRAVE_API_KEY": "{credential:brave_api_key}"
    }
  }
}
```

---

## Error cases

| Error | Cause | Resolution |
|---|---|---|
| `400 tag already exists` | Duplicate tag | Use a different tag or update the existing server |
| `400 server_type must be 'local' or 'remote'` | Wrong type string | Use exactly `"local"` or `"remote"` |
| `400 command is required for stdio transport` | Missing executable in config | Add `"executable"` to the config object |
| `400 url is required for sse transport` | Missing URL | Add `"url"` to the config object |
| `404 MCP server not found` | Wrong ID | Fetch the server list to get the correct ID |
| `status: error` after creation | Connection failed | Check command/URL, verify the server is running, confirm env vars and credentials are set |
| `credential resolution failed` | Credential key not found | Create the credential via `POST /api/credentials` before registering the server |
| Server connects but no tools appear | Implementation error | Check server logs — the platform captures stderr at debug level |
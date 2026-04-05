# Skill: Configure an MCP Server

You can add, update, enable, disable, and remove MCP servers on behalf of the user by calling the agent-deck REST API directly. MCP (Model Context Protocol) servers extend the agent with external tools — file system access, web search, database queries, API integrations, and more.

---

## When to use this skill

Use this skill when the user asks you to:
- Add or connect a new MCP server
- Remove or disconnect an MCP server
- Enable or disable an existing MCP server
- Update an MCP server's configuration (URL, command, args, env vars)
- Help troubleshoot why an MCP server isn't connecting

---

## Core concepts

| Concept | Description |
|---|---|
| **tag** | Short slug used as the server's identifier and config directory name (e.g. `filesystem`, `brave-search`). Lowercase, hyphens only. |
| **transport** | How the agent connects to the server: `sse` (HTTP Server-Sent Events, for remote servers) or `stdio` (local subprocess) |
| **url** | For `sse` transport: the full HTTP URL of the running MCP server |
| **command / args** | For `stdio` transport: the executable and arguments to launch the local process |
| **env** | Optional key-value pairs injected as environment variables when launching a `stdio` server |

Config files for `stdio` servers are stored at `~/.agent-deck/mcp/<tag>/config.json` on the server host.

---

## API contract

### List all MCP servers

**`GET /api/mcp-servers`**

Returns all registered servers with their current connection status.

---

### Create a new MCP server

**`POST /api/mcp-servers`**

**Required fields:**

| Field | Type | Description |
|---|---|---|
| `name` | string | Human-readable display name (e.g. "Filesystem") |
| `tag` | string | Unique slug identifier (e.g. `filesystem`) |
| `transport` | `"sse"` or `"stdio"` | Connection type |

**For `sse` transport, also required:**

| Field | Type | Description |
|---|---|---|
| `url` | string | Full URL of the SSE endpoint (e.g. `http://localhost:3000/sse`) |

**For `stdio` transport, also required:**

| Field | Type | Description |
|---|---|---|
| `command` | string | Executable to run (e.g. `npx`, `python`, `/usr/local/bin/my-mcp`) |
| `args` | array of strings | Arguments passed to the command (e.g. `["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]`) |
| `env` | object | Optional environment variables (e.g. `{"API_KEY": "abc123"}`) |

**Response envelope:**
```json
{
  "data": {
    "id": "<uuid>",
    "name": "Filesystem",
    "tag": "filesystem",
    "transport": "stdio",
    "url": null,
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    "env": {},
    "enabled": true,
    "status": "connecting",
    "created_at": "...",
    "updated_at": "..."
  }
}
```

---

### Update an existing MCP server

**`PUT /api/mcp-servers/:id`**

Send only the fields to change. All fields are optional. The server will reconnect automatically if connection-related fields change.

---

### Delete an MCP server

**`DELETE /api/mcp-servers/:id`**

Disconnects and removes the server permanently.

---

### List available tools on a server

**`GET /api/mcp-servers/:id/tools`**

Returns the tools the connected server exposes. Only works when `status` is `connected`.

---

## Step-by-step: adding a new server

1. **Ask the user for the details you need:**
   - Transport type (`sse` or `stdio`)
   - For `sse`: the URL
   - For `stdio`: the command, args, and any required environment variables (API keys etc.)
   - A display name and tag if not obvious from context

2. **Handle secrets carefully.** If the server requires an API key or token, remind the user that env vars passed via this API are stored in the server's config file on disk — not in the encrypted credential store. Advise them not to use this for highly sensitive credentials if the machine is shared.

3. **Call `POST /api/mcp-servers`** with the collected details.

4. **Check the status.** After creation the server will attempt to connect. Call `GET /api/mcp-servers` to check whether `status` has moved to `connected`. If it stays `error` or `disconnected`, investigate with the user:
   - For `sse`: is the remote server running and reachable?
   - For `stdio`: is the command installed? Are args correct? Are required env vars set?

5. **Confirm success.** Tell the user the server is connected and list its available tools via `GET /api/mcp-servers/:id/tools` so they know what capabilities are now available.

---

## Common MCP servers

| Server | Transport | Command | Notes |
|---|---|---|---|
| `@modelcontextprotocol/server-filesystem` | stdio | `npx -y @modelcontextprotocol/server-filesystem <path>` | Exposes file read/write tools for a given directory |
| `@modelcontextprotocol/server-brave-search` | stdio | `npx -y @modelcontextprotocol/server-brave-search` | Requires `BRAVE_API_KEY` env var |
| `@modelcontextprotocol/server-github` | stdio | `npx -y @modelcontextprotocol/server-github` | Requires `GITHUB_PERSONAL_ACCESS_TOKEN` env var |
| `@modelcontextprotocol/server-postgres` | stdio | `npx -y @modelcontextprotocol/server-postgres <connection-string>` | Full DB query access — use with caution |

---

## Attaching a server to a thread

Creating a server makes it available globally. To use its tools in a specific thread, the server must also be **attached to that thread**:

**`POST /api/threads/:thread_id/mcp-servers`**
```json
{ "mcp_server_id": "<uuid>" }
```

To detach:

**`DELETE /api/threads/:thread_id/mcp-servers/:mcp_server_id`**

After attaching, the thread's next agent run will have access to all tools the server exposes.

---

## Error cases

| Error | Cause | Resolution |
|---|---|---|
| `400 tag already exists` | Duplicate tag | Use a different tag or update the existing server |
| `400 url is required for sse transport` | Missing URL | Provide the SSE endpoint URL |
| `400 command is required for stdio transport` | Missing command | Provide the executable path or command |
| `404 MCP server not found` | Wrong ID | Fetch the server list to get the correct ID |
| `status: error` after creation | Connection failed | Check command/URL, verify the server is running, check env vars |
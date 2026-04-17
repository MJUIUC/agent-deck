# agent-deck

A self-hosted AI agent platform that runs on your own hardware. Talk to multiple AI personas, give them persistent memory, connect them to MCP servers and external tools, and access everything privately from any device over Tailscale.

---

## Install

> Requires macOS or Linux with internet access. Everything else is installed automatically.

```bash
curl -fsSL https://github.com/MJUIUC/agent-deck/archive/refs/heads/main.tar.gz | tar xz && cd agent-deck-main && ./install.sh
```

The installer will:

1. Install **Homebrew** — macOS only (if absent)
2. Install **Rust** via rustup (if absent)
3. Install **nvm** and **Node 24** via `.nvmrc` (if absent)
4. Install **Tailscale** via Homebrew on macOS, or the official install script on Linux (if absent)
5. Build the Rust server and React frontend from source
6. Deploy everything to `~/.agent-deck/`
7. Start the server and print your auth token

Once installed, open **http://localhost:7474**.

> This downloads the source and builds locally — the same as `git clone`, just without needing `git`. On modern multi-core hardware the compile takes about a minute; older or single-core machines may take longer. Pre-built binary releases (no build step) are planned once a CI release pipeline is in place.

### Build from source

If you already have `git`:

```bash
git clone git@github.com:MJUIUC/agent-deck.git && cd agent-deck && ./install.sh
```

---

## CLI

The installer registers an `agent-deck` command in your shell:

```
agent-deck start    Start the server
agent-deck stop     Stop the server
agent-deck status   Show server and Tailscale status
agent-deck logs     Tail the server log
agent-deck open     Open in the default browser
```

---

## Data directory

Everything lives under `~/.agent-deck/`:

```
~/.agent-deck/
  mcp/          MCP server configs and local binaries
  personas/     Persona avatars and assets
  skills/       Skill definitions
  workspaces/   Per-thread file workspaces
  .process/     Internal runtime files (binary, database, logs, assets)
```

The four top-level directories are the ones you'll interact with directly. `.process/` is managed by the installer and server — you shouldn't need to touch it.

---

## What it does

### Personas
Create multiple AI personas, each with its own name, emoji, system prompt, and model assignment. Personas have independent memory — what one persona knows doesn't bleed into another.

### Persistent memory
Each persona has a searchable memory store (SQLite FTS5). The agent can call `save_memory` and `recall_memory` tools automatically during conversation. Memory persists across threads.

### MCP servers
Connect any [Model Context Protocol](https://modelcontextprotocol.io) server — local (child processes via stdio) or remote (HTTP/SSE). Tools are namespaced per server and injected into the agent's tool list automatically.

### Scheduled routines
Attach cron-based routines to threads. At the scheduled time the agent runs autonomously, executes tool calls, and saves results as messages. Useful for daily summaries, monitoring, or recurring tasks.

### Providers
Connect any OpenAI-compatible LLM provider. Multiple providers can be configured simultaneously and assigned to individual personas. GitHub Copilot is supported via a bundled `copilot-api` sidecar.

### Workspaces
Each thread gets an isolated filesystem workspace at `~/.agent-deck/workspaces/<thread-id>/`. The agent can read and write files there, and you can browse the workspace from the file explorer in the UI.

### Credentials store
API keys and secrets are stored encrypted at rest (AES-256-GCM) and decrypted only when needed by the MCP connection manager. They are never written to disk in plaintext.

### Tailscale access
Connect over your private Tailscale network to access agent-deck from any device — phone, tablet, or another computer. Enable Tailscale Funnel to expose a public HTTPS endpoint for incoming webhooks.

---

## Authentication

**Localhost (127.0.0.1 / ::1):** Auth is bypassed entirely. The browser on the same machine as the server never needs a token.

**Remote devices:** On first visit from a remote browser, a token entry screen is shown. The token is printed to the server terminal on startup (and visible in `agent-deck logs`). Once entered it's stored as an `httpOnly` session cookie — you won't need to re-enter it per browser.

To rotate the token: **Settings → Auth Token → Rotate**.

---

## Updating

Pull the latest code and re-run the installer:

```bash
cd agent-deck && git pull && ./install.sh
```

The installer stops any running instance, rebuilds, redeploys, and restarts.

---

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `AGENT_DECK_HOME` | `~/.agent-deck` | Root data and install directory |
| `AGENT_DECK_PORT` | `7474` | Server listen port |
| `FCM_SERVICE_ACCOUNT_JSON` | *(unset)* | Firebase service account path for push notifications |
| `RUST_LOG` | `info` | Log level (`info`, `debug`, `trace`) |

---

## Architecture

The server is a single Rust binary (Axum + SQLite) that serves the React SPA, runs the agent loop, manages MCP connections, and handles all API routes. There are no external services required — just the binary and a SQLite database.

```
Rust server :7474
├── Axum HTTP + SSE
├── Agent run-loop (streaming, tool calls, cancellation)
├── Cron scheduler
├── MCP connection manager (local stdio + remote HTTP/SSE)
├── Provider abstraction (OpenAI-compatible + Copilot)
├── Credential store (AES-256-GCM)
└── Static file server → React SPA

copilot-api sidecar :4141   (spawned automatically, only if Copilot is configured)
SQLite (WAL mode)            ~/.agent-deck/.process/.database/agent-deck.db
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for sequence diagrams and deeper detail.

---

## License

Private. All rights reserved.
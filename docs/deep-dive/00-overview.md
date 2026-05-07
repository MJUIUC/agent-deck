# Vestry (agent-deck) — Deep-Dive Architecture Documentation

> **Audience:** Senior engineers onboarding to the codebase who need full system fluency within a week.  
> **Codebase language:** Rust (server) + TypeScript/React (web client)  
> **Runtime target:** Single-user, always-on local server (Mac mini), accessed by browser and mobile clients over a private Tailscale network.

---

## Document Index

| File | What it covers |
|---|---|
| `00-overview.md` | You are here — system map, stack summary, reading order |
| `01-startup-and-config.md` | Server boot sequence, `Config`, directory layout |
| `02-database.md` | SQLite schema, migration system, all 21 migrations |
| `03-appstate-and-router.md` | `AppState`, router construction, auth middleware |
| `04-agent-runloop.md` | Full agent execution model, generation loop, tool dispatch |
| `05-context-assembly.md` | How system prompts, history, and tools are assembled |
| `06-provider-abstraction.md` | `LlmProvider` trait, `OpenAiProvider`, `CopilotProvider` |
| `07-mcp-integration.md` | MCP connection manager, stdio + HTTP transports, credential injection |
| `08-sse-and-realtime.md` | SSE event system, per-thread streams, global stream |
| `09-api-endpoints.md` | Every REST endpoint, grouped by domain |
| `10-memory-system.md` | Per-persona memory store, FTS5 search, built-in tools |
| `11-credentials.md` | AES-256-GCM credential store, injection syntax |
| `12-scheduler-and-routines.md` | Cron scheduler, routine execution lifecycle |
| `13-tailscale-integration.md` | Tailscale serve/funnel, status caching |
| `14-frontend-architecture.md` | React SPA, Zustand stores, SSE consumption |

---

## System Summary

Vestry is a **private AI agent host** built on Rust + Axum. It runs as a headless server on a local machine and exposes a React SPA and REST API. Agents are LLM-backed, persona-scoped, and tool-capable via:

1. **Built-in tools** — `save_memory`, `recall_memory`, `delete_memory`, `recall_conversation`, `tailscale_status`, `set_mcp_timeout`, `describe_image`
2. **MCP (Model Context Protocol) tools** — dynamically discovered from local child processes or remote HTTP endpoints

### High-level stack

```
+------------------------------------------------------------------+
|                        Clients                                   |
|   Browser (React SPA)          Mobile (React Native)             |
+------------------------------------------------------------------+
                       | HTTP + SSE (Tailscale private network)
+------------------------------------------------------------------+
|               Rust Server  :7474  (Axum + Tokio)                 |
|                                                                  |
|  +----------+ +----------+ +----------+ +------------------+    |
|  |  Router  | |  Agent   | |Scheduler | | MCP Connection   |    |
|  | (Axum)   | | Run-Loop | | (cron)   | | Manager          |    |
|  +----------+ +----------+ +----------+ +------------------+    |
|                                                                  |
|  +----------+ +----------+ +----------+ +------------------+    |
|  |Provider  | |Credential| |  Memory  | |  SSE Broadcast   |    |
|  |Abstraction| |  Store   | |  Store   | |  (per-thread +   |    |
|  |          | |AES-256-GCM| |  FTS5   | |   global)        |    |
|  +----------+ +----------+ +----------+ +------------------+    |
|                                                                  |
|  +------------------------------------------------------------+  |
|  |                 SQLite (WAL mode, serialized)              |  |
|  +------------------------------------------------------------+  |
+------------------------------------------------------------------+
         |                              |
         v                              v
  LLM Providers                  MCP Servers
  (OpenAI/Anthropic/              (local child procs
   Copilot/custom)                 or remote HTTP)
```

### Key design decisions

| Decision | Rationale |
|---|---|
| Single SQLite database, serialized connection pool | Single-user; avoids WAL contention between handler and agent writes |
| `Arc<AppState>` everywhere | `DashMap` deep-clones on struct clone — `Arc` ensures `run_states` is shared across requests (required for cancel to find the running turn) |
| Cooperative cancellation via `watch::channel<bool>` | Clean, Rust-idiomatic; avoids `tokio::task::abort()` which can leave DB writes half-done |
| MCP tools namespaced as `{tag}__{tool_name}` | Prevents name collisions across servers; tag is validated alphanumeric |
| Credential injection at tool-call time, not storage time | Secrets never reach the database; raw placeholder text stored in tool call logs |

---

## How to read this documentation

If you are a senior engineer coming in cold, suggested reading order:

1. **`01-startup-and-config.md`** — understand the directory layout and how the server initializes
2. **`03-appstate-and-router.md`** — understand the shared state and all API routes
3. **`04-agent-runloop.md`** — the core of the system; spend the most time here
4. **`07-mcp-integration.md`** — tool infrastructure
5. **`09-api-endpoints.md`** — reference for every HTTP endpoint
6. The remaining files as needed for specific features

---

## Top-level directory structure

```
agent-deck/
+-- server/                  Rust server (Axum + SQLx + Tokio)
|   +-- src/
|   |   +-- main.rs          Entry point, startup sequence
|   |   +-- config.rs        Config struct + env parsing
|   |   +-- error.rs         AppError enum + Into<Response>
|   |   +-- db/
|   |   |   +-- mod.rs       DB init, migrations, schema check
|   |   |   +-- threads.rs   Thread DB helpers
|   |   |   +-- migrations/  001-021 SQL migration files
|   |   +-- models/          sqlx::FromRow structs
|   |   +-- routes/          Axum route handlers + AppState
|   |   +-- services/        Business logic
|   +-- Cargo.toml
+-- web/                     React SPA (TypeScript + Vite)
|   +-- src/
|       +-- App.tsx
|       +-- api/client.ts    Typed API client
|       +-- components/      UI components
|       +-- stores/          Zustand stores
|       +-- types/index.ts   Shared types
+-- vendor/copilot-api/      Git submodule: Bun-based GitHub Copilot proxy
+-- docs/                    Documentation (you are here)
+-- Makefile
```

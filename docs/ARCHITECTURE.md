# Agent-Deck — System Architecture

## System Overview

```mermaid
graph TB
    subgraph tailscale["Tailscale Private Network"]

        subgraph clients["Clients"]
            browser["Browser<br/><i>React SPA</i><br/>(any device)"]
            android["Android App<br/><i>React Native</i>"]
        end

        subgraph macmini["Mac mini — Always-On Server"]

            subgraph rust["Rust Server :7474"]
                axum["Axum HTTP<br/>+ SSE"]
                agent["Agent<br/>Run-Loop"]
                scheduler["Cron<br/>Scheduler"]
                mcp_mgr["MCP Connection<br/>Manager"]
                provider["Provider<br/>Abstraction Layer"]
                static["Static File Server<br/><i>(React SPA build)</i>"]
                credential["Credential Store<br/><i>AES-256-GCM</i>"]
                run_mgr["Run Manager<br/><i>Semaphore + Cancel</i>"]
            end

            sqlite[("SQLite<br/>WAL mode")]
            copilot["copilot-api<br/><i>Bun :4141</i><br/>GitHub Copilot proxy"]

            subgraph mcp_servers["MCP Servers"]
                local_mcp["Local MCP<br/><i>child processes</i>"]
                remote_mcp["Remote MCP<br/><i>HTTP/SSE endpoints</i>"]
            end

        end

    end

    subgraph external["External Services"]
        firebase["Google Firebase<br/><i>FCM push notifications</i>"]
        llm_providers["LLM Providers<br/><i>OpenAI · Anthropic · Custom</i>"]
    end

    %% Client connections
    browser -- "HTTP + SSE" --> axum
    android -- "HTTP + SSE" --> axum

    %% Internal server connections
    axum --> agent
    axum --> static
    scheduler -- "POST /notify<br/>routine_fired" --> agent
    agent --> run_mgr
    agent --> provider
    agent --> mcp_mgr
    mcp_mgr --> credential
    mcp_mgr --> local_mcp
    mcp_mgr --> remote_mcp
    agent --> sqlite
    scheduler --> sqlite
    axum --> sqlite
    credential --> sqlite

    %% External connections
    provider -- "OpenAI-compatible API" --> llm_providers
    provider -- "localhost:4141" --> copilot
    rust -- "HTTPS (FCM v1)" --> firebase
    firebase -. "Push" .-> android
```

## Agent Run-Loop Detail

```mermaid
flowchart TD
    trigger["Trigger<br/><i>User message · Routine · Notify</i>"]
    lock{"Acquire<br/>semaphore"}
    reject["429 Too Many Requests<br/><i>(user only; routines exempt)</i>"]
    context["Assemble Context<br/><i>persona prompt + addendum<br/>+ memory instructions<br/>+ last N messages</i>"]
    tools["Build Tool List<br/><i>MCP tools (namespaced)<br/>+ memory tools (if persona)</i>"]
    llm_call["Call LLM Provider<br/><i>streaming</i>"]

    subgraph gen_loop["Generation Loop (max 20 rounds)"]
        stream["Stream tokens → SSE"]
        check_cancel{"Cancelled?"}
        tool_req{"Tool calls<br/>requested?"}
        pre_tool_cancel{"Cancelled?"}
        exec_tool["Execute tool call<br/><i>MCP or built-in</i>"]
        tool_result["Persist hidden<br/>tool result message"]
        next_round["Next LLM call<br/>with tool results"]
    end

    persist["Persist assistant message"]
    stopped["Persist partial message<br/><i>stopped = 1</i>"]
    complete["Emit message_complete SSE<br/>Release semaphore"]

    trigger --> lock
    lock -- "depth > 3" --> reject
    lock -- "acquired" --> context
    context --> tools
    tools --> llm_call
    llm_call --> stream
    stream --> check_cancel
    check_cancel -- "yes" --> stopped
    check_cancel -- "no" --> tool_req
    tool_req -- "no" --> persist
    tool_req -- "yes" --> pre_tool_cancel
    pre_tool_cancel -- "yes" --> stopped
    pre_tool_cancel -- "no" --> exec_tool
    exec_tool --> tool_result
    tool_result --> next_round
    next_round --> llm_call
    persist --> complete
    stopped --> complete
```

## Data Flow — Routine Execution

```mermaid
sequenceDiagram
    participant Cron as Cron Scheduler
    participant Notify as /notify endpoint
    participant Lock as Run Manager
    participant Agent as Agent Run-Loop
    participant LLM as LLM Provider
    participant MCP as MCP Servers
    participant DB as SQLite
    participant SSE as SSE Stream
    participant FCM as Firebase (FCM)

    Cron->>Notify: routine_fired (trigger: true)
    Notify->>Lock: acquire semaphore
    Lock->>Agent: start run (AgentSink::Silent)

    Note over DB: routine_executions<br/>status: running

    Agent->>LLM: chat completion (streaming)
    LLM-->>Agent: tokens (buffered, no SSE)

    opt Tool calls needed
        Agent->>MCP: call_tool (namespaced)
        MCP-->>Agent: tool result
        Agent->>DB: persist hidden tool message
        Agent->>LLM: continue with tool results
        LLM-->>Agent: final response
    end

    Agent->>DB: persist visible message (source: routine)
    Agent->>DB: routine_executions → completed

    Agent->>SSE: routine_message event
    Agent->>SSE: thread_updated (global)

    alt No SSE clients connected
        Agent->>FCM: push notification (Phase 7)
    end

    Agent->>Lock: release semaphore
```

## Memory System

```mermaid
flowchart LR
    subgraph persona_a["Persona: Aldous 🦉"]
        thread1["Thread A"]
        thread2["Thread B"]
        mem_a[("Memory Store<br/><i>FTS5 keyword search<br/>max 500 entries</i>")]
    end

    subgraph persona_b["Persona: Atlas 🤖"]
        thread3["Thread C"]
        mem_b[("Memory Store<br/><i>independent from Aldous</i>")]
    end

    subgraph no_persona["No Persona 💬"]
        thread4["Thread D"]
        no_mem["No memory<br/><i>tools not injected</i>"]
    end

    thread1 -- "save_memory" --> mem_a
    thread2 -- "save_memory" --> mem_a
    thread1 -- "recall_memory" --> mem_a
    thread2 -- "recall_memory" --> mem_a

    thread3 -- "save_memory" --> mem_b
    thread3 -- "recall_memory" --> mem_b

    thread4 -. "no memory tools" .-> no_mem
```

## Shared State (`Arc<AppState>`)

All route handlers and the auth middleware receive the application state via Axum's `State` extractor. Axum **clones** the state on every request, so the state type must be `Arc<AppState>` — not a bare `AppState` struct.

### Why `Arc` is required

| Field | Type | Sharing mechanism |
|---|---|---|
| `pool` | `SqlitePool` | Internally `Arc`-based — safe to clone |
| `run_states` | `DashMap<String, Arc<RunState>>` | `DashMap::clone()` is a **deep copy** — without `Arc`, each request gets its own disconnected map |
| `thread_senders` | `Arc<Mutex<HashMap<…>>>` | Explicitly `Arc`-wrapped |
| `global_tx` | `broadcast::Sender` | Reference-counted internally |
| `built_in_tools` | `Arc<Vec<…>>` | Explicitly `Arc`-wrapped |
| `auth_token` | `Arc<RwLock<String>>` | Must be readable by every request **and** writable by `rotate_token` |
| `mcp`, `copilot` | internally `Arc`-based | Safe to clone |

Without `Arc<AppState>`, the `run_states` DashMap deep-clones on every request, meaning the `RunState` inserted by a `POST /messages` handler (which registers the running turn) is **never visible** to `POST /cancel` — cancellation silently fires into a disconnected copy of the map.

### `auth_token: Arc<RwLock<String>>`

The auth middleware compares incoming Bearer tokens and session cookies against `state.auth_token`. The `POST /auth/token/rotate` endpoint generates a new token, persists it to the database, **and** writes it back through the `RwLock` so the middleware immediately enforces the new token without a server restart:

```rust
// routes/auth.rs — rotate_token
let new_token = auth_service::rotate_auth_token(&state.pool).await?;
*state.auth_token.write().unwrap() = new_token.clone();
```

### Router setup

```rust
// routes/mod.rs — build_router
let state = Arc::new(AppState { … });

let protected_api = Router::new()
    // … routes …
    .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

let app = Router::new()
    .merge(public_api)
    .merge(protected_api)
    .with_state(state);   // Arc clone — cheap reference-count bump
```

Every handler signature uses `State<Arc<AppState>>`. Helper functions that only need a read reference accept `&AppState`; Rust's `Deref` coercion from `&Arc<AppState>` to `&AppState` means call sites need no changes.

## MCP Integration

```mermaid
flowchart TB
    subgraph agent_context["Agent Run Context"]
        builtin["Built-in Tools<br/><i>save_memory<br/>recall_memory</i>"]
        mcp_tools["MCP Tools<br/><i>github__create_issue<br/>gmail__send_email<br/>fs__read_file</i>"]
    end

    subgraph mcp_layer["MCP Connection Manager"]
        pool["Connection Pool<br/><i>keyed by server_id</i>"]
        cred_resolve["Credential Resolution<br/><i>decrypt at connect time</i>"]
    end

    subgraph servers["MCP Servers"]
        local1["Local: filesystem<br/><i>child process · stdio</i>"]
        local2["Local: code-tools<br/><i>child process · stdio</i>"]
        remote1["Remote: gmail<br/><i>HTTP/SSE · bearer token</i>"]
        remote2["Remote: context7<br/><i>HTTP/SSE · API key</i>"]
    end

    cred_store[("Credential Store<br/><i>AES-256-GCM encrypted</i>")]

    agent_context --> pool
    pool --> local1
    pool --> local2
    pool --> remote1
    pool --> remote2
    cred_resolve --> cred_store
    remote1 -. "auth header" .-> cred_resolve
    remote2 -. "auth header" .-> cred_resolve
```

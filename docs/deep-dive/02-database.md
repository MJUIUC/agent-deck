# 02 — Database

## Overview

Vestry uses a single **SQLite** database in **WAL mode** with a **serialized, single-connection pool**. This is an intentional design: the system is single-user and single-process. Multiple concurrent writers on WAL were causing HTTP handlers to block until the agent's writes completed, making POST responses appear to hang.

```rust
let pool = SqlitePoolOptions::new()
    .max_connections(1)
    .connect_with(connect_options.serialized(true))
    .await?;

sqlx::query("PRAGMA journal_mode=WAL;").execute(&pool).await?;
sqlx::query("PRAGMA foreign_keys=ON;").execute(&pool).await?;
```

---

## Migration System

Migrations live in `server/src/db/migrations/` and are embedded in the binary via `sqlx::migrate!`. They run on every startup (idempotent — sqlx tracks which have been applied in `_sqlx_migrations`).

After migrations run, a **schema integrity check** verifies that critical columns exist:

```rust
let checks: &[(&str, &str)] = &[
    ("threads", "auto_retitle"),
    ("threads", "summary"),
    ("threads", "auto_summarize"),
    ("mcp_servers", "tool_call_timeout_secs"),
    ("mcp_servers", "disabled_tools"),
    ...
];
```

This catches the case where `_sqlx_migrations` has a stale record but the `ALTER TABLE` never landed (e.g. after a crash during migration).

---

## Schema Reference

### `users`
Single-user system — exactly one row created during setup.

| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| display_name | TEXT NOT NULL | |
| pronouns/role/organization/location/timezone/about | TEXT | Optional profile fields injected into system prompt |
| profile_updated_at | TEXT | ISO-8601 |
| created_at | TEXT | ISO-8601 |

### `agent_personas`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| user_id | TEXT FK | |
| name | TEXT | Display name |
| emoji | TEXT | Default: robot emoji |
| avatar_path | TEXT | Relative to personas_dir |
| system_prompt | TEXT | Base system prompt injected for all threads |
| default_model | TEXT FK | UUID reference to models table |
| default_provider | TEXT FK | UUID reference to providers table |
| is_default | BOOLEAN | Default persona has no memory tools |
| recall_conversation_cross_thread | BOOLEAN | Reserved for future use |

### `threads`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| user_id / persona_id | TEXT FK | |
| title | TEXT | Auto-generated after first reply |
| active_provider / active_model | TEXT | Per-thread overrides for persona defaults |
| system_prompt_addendum | TEXT | Appended to persona's system prompt |
| show_tool_activity | BOOLEAN | Show hidden tool messages in UI |
| show_system_events | BOOLEAN | Show system event messages |
| auto_retitle | BOOLEAN | Auto-generate title after first exchange |
| auto_summarize | BOOLEAN | Enable rolling summarization |
| summary | TEXT | Current rolling summary text |
| summary_message_count | INTEGER | Message offset: history loaded OFFSET this value |
| archived | BOOLEAN | |

### `messages`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| thread_id | TEXT FK CASCADE | |
| role | TEXT | user, assistant, system, tool |
| content | TEXT | Message body (markdown) |
| source | TEXT | chat, tool, routine |
| routine_id | TEXT | Set for routine-triggered messages |
| visibility | TEXT | visible or hidden (tool activity) |
| execution_id | TEXT | Groups all messages from one routine execution |
| event_type | TEXT | For system event messages |
| stopped | BOOLEAN | True when stream was cancelled mid-response |
| attachments | TEXT | JSON array of `MessageAttachment` structs |
| tool_call_id | TEXT | Correlates tool call/result pairs |
| tool_round | INTEGER | Round number in multi-turn tool loop |

### `providers`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| kind | TEXT | openai, anthropic, custom, copilot |
| base_url | TEXT | e.g. `https://api.openai.com/v1` |
| api_key | TEXT | AES-256-GCM encrypted |
| enabled | BOOLEAN | |

### `models`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| provider_id | TEXT FK CASCADE | |
| model_id | TEXT | Provider model string e.g. `gpt-4o` |
| display_name | TEXT | Human readable |
| vision | BOOLEAN | Whether model supports image input |
| enabled | BOOLEAN | |

### `mcp_servers`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| tag | TEXT UNIQUE | Short alphanumeric ID used as tool prefix |
| server_type | TEXT | local or remote |
| config | TEXT | JSON: `LocalConfig` or `RemoteConfig` struct |
| status | TEXT | inactive, connecting, connected, error |
| tool_call_timeout_secs | INTEGER | Per-server timeout override |
| disabled_tools | TEXT | JSON array of disabled tool names |

**LocalConfig JSON shape:**
```json
{
  "executable": "docker",
  "args": ["run", "--rm", "-i", "mcp-server"],
  "env": {
    "GITHUB_TOKEN": "literal-value"
  },
  "working_dir": "/optional/path"
}
```

**RemoteConfig JSON shape:**
```json
{
  "url": "https://mcp.example.com/mcp",
  "credential_key": "my_api_key",
  "auth_header": "Authorization",
  "auth_format": "Bearer {value}",
  "headers": {
    "X-Custom": "value"
  }
}
```

### `credentials`
| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | UUID |
| key | TEXT UNIQUE | Lookup key for credential injection syntax |
| kind | TEXT | api_key, oauth_token, service_account, basic_auth, ... |
| api_key / secret_key / password / token / service_account_json | TEXT | AES-256-GCM encrypted fields |

Credential injection syntax used in MCP env vars and remote config:
- `GITHUB_TOKEN: "{cred-placeholder:my_github}"` resolves the `secret` field
- `HEADER: "{cred-placeholder:schwab:api_key}"` resolves the `api_key` field specifically

(Note: actual runtime syntax uses curly-brace-credential-colon-key pattern — see `07-mcp-integration.md` for full details.)

### `memories`
```sql
CREATE TABLE memories (
    id         TEXT PRIMARY KEY,
    persona_id TEXT NOT NULL REFERENCES agent_personas(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id),
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE memories_fts USING fts5(
    content,
    content='memories',
    content_rowid='rowid'
);
```
Max 500 entries per persona. FTS5 with prefix search. See `10-memory-system.md`.

### `routines`
| Column | Type | Notes |
|---|---|---|
| cron_expr | TEXT | Standard 5-field cron: `0 9 * * 1-5` |
| instructions | TEXT | Injected as the user message for the routine run |

### `routine_executions`
| Column | Type | Notes |
|---|---|---|
| status | TEXT | running, completed, failed |
| output_message_id | TEXT | FK to final assistant message |

---

## Migration history

| # | File | What it adds |
|---|---|---|
| 001 | `001_initial_schema.sql` | Core tables: users, providers, models, personas, threads, messages, memories, memories_fts |
| 002 | `002_plan_v1_4_alignment.sql` | message visibility, source, event_type, stopped; thread auto_retitle |
| 003 | `003_thread_show_tool_activity.sql` | thread.show_tool_activity |
| 004 | `004_phase4_credentials.sql` | credentials table |
| 005 | `005_credential_type_service_account.sql` | service_account_json field on credentials |
| 006 | `006_mcp_tag.sql` | mcp_servers.tag, thread_mcp_servers join table |
| 007 | `007_phase5_run_management.sql` | routines, routine_executions, messages.execution_id |
| 008 | `008_phase5_memory_tools.sql` | memories.user_id |
| 009 | `009_phase5_user_profile.sql` | users profile fields |
| 010 | `010_phase5_summarization.sql` | threads.summary, auto_summarize, summary_message_count |
| 011 | `011_push_subscriptions.sql` | push_subscriptions, device_tokens |
| 012 | `012_tool_call_grouping.sql` | messages.tool_call_id, messages.tool_round |
| 013 | `013_auto_retitle.sql` | threads.auto_retitle default fix |
| 014 | `014_webhook_bindings.sql` | webhook_bindings table |
| 015 | `015_webhook_binding_prompt.sql` | webhook_binding_attachments.prompt |
| 016 | `016_webhook_bindings_refactor.sql` | Webhook binding schema refactor |
| 017 | `017_webhook_bindings_schema_v2.sql` | Final webhook bindings schema v2 |
| 018 | `018_mcp_tool_settings.sql` | mcp_servers timeout + disabled_tools |
| 019 | `019_thread_mcp_tool_settings.sql` | thread_mcp_servers timeout + disabled_tools override |
| 020 | `020_provider_vision_message_attachments.sql` | messages.attachments JSON column |
| 021 | `021_model_vision.sql` | models.vision boolean |

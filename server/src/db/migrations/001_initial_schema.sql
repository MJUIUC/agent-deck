-- Migration 001: Initial schema
-- Enables WAL mode and foreign keys at the connection level (done in db/mod.rs),
-- but we set them here too as a safety net for tooling that runs migrations directly.
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- ─── users ────────────────────────────────────────────────────────────────────
-- One row — always the same single user. Exists for future multi-user expansion.
CREATE TABLE IF NOT EXISTS users (
  id           TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── app_config ───────────────────────────────────────────────────────────────
-- Single-row key/value store for application settings (auth token, setup flag, etc.)
CREATE TABLE IF NOT EXISTS app_config (
  key          TEXT PRIMARY KEY,
  value        TEXT NOT NULL,
  updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── providers ────────────────────────────────────────────────────────────────
-- Model provider configurations. Each provider is an OpenAI-compatible endpoint.
CREATE TABLE IF NOT EXISTS providers (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,
  kind         TEXT NOT NULL,       -- 'copilot' | 'openai' | 'anthropic' | 'custom'
  base_url     TEXT NOT NULL,
  api_key      TEXT,                -- encrypted at rest, null for copilot
  enabled      INTEGER NOT NULL DEFAULT 1,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);

-- ─── models ───────────────────────────────────────────────────────────────────
-- Available models per provider. Populated by calling the provider's /v1/models endpoint.
CREATE TABLE IF NOT EXISTS models (
  id           TEXT PRIMARY KEY,
  provider_id  TEXT NOT NULL,
  model_id     TEXT NOT NULL,       -- e.g. "gpt-4o", "claude-3-5-sonnet-20241022"
  display_name TEXT NOT NULL,
  enabled      INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
);

-- ─── agent_personas ───────────────────────────────────────────────────────────
-- Reusable agent configurations. A persona defines who the agent is.
CREATE TABLE IF NOT EXISTS agent_personas (
  id               TEXT PRIMARY KEY,
  user_id          TEXT NOT NULL,
  name             TEXT NOT NULL,
  emoji            TEXT NOT NULL,
  avatar_path      TEXT,
  system_prompt    TEXT NOT NULL,
  default_model    TEXT,
  default_provider TEXT,
  created_at       TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id)          REFERENCES users(id),
  FOREIGN KEY (default_model)    REFERENCES models(id)    ON DELETE SET NULL,
  FOREIGN KEY (default_provider) REFERENCES providers(id) ON DELETE SET NULL
);

-- ─── skills ───────────────────────────────────────────────────────────────────
-- Skill definitions. Stored locally, used as tool calls at runtime.
CREATE TABLE IF NOT EXISTS skills (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,        -- slug, e.g. "weekly-report"
  display_name TEXT NOT NULL,
  description  TEXT NOT NULL,
  instructions TEXT NOT NULL,        -- full SKILL.md body content
  enabled      INTEGER NOT NULL DEFAULT 1,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);

-- ─── mcp_servers ──────────────────────────────────────────────────────────────
-- MCP server configurations.
CREATE TABLE IF NOT EXISTS mcp_servers (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,
  command      TEXT NOT NULL,
  args         TEXT NOT NULL DEFAULT '[]',   -- JSON array of string args
  env          TEXT NOT NULL DEFAULT '{}',   -- JSON object of env vars
  enabled      INTEGER NOT NULL DEFAULT 1,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);

-- ─── threads ──────────────────────────────────────────────────────────────────
-- A chat session. Locked to one persona at creation time.
CREATE TABLE IF NOT EXISTS threads (
  id                     TEXT PRIMARY KEY,
  user_id                TEXT NOT NULL,
  persona_id             TEXT NOT NULL,
  title                  TEXT NOT NULL,
  active_model           TEXT,
  active_provider        TEXT,
  system_prompt_addendum TEXT,
  status                 TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'archived'
  created_at             TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at             TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id)        REFERENCES users(id),
  FOREIGN KEY (persona_id)     REFERENCES agent_personas(id),
  FOREIGN KEY (active_model)   REFERENCES models(id)    ON DELETE SET NULL,
  FOREIGN KEY (active_provider) REFERENCES providers(id) ON DELETE SET NULL
);

-- ─── thread_skills ────────────────────────────────────────────────────────────
-- Skills attached to a thread.
CREATE TABLE IF NOT EXISTS thread_skills (
  id         TEXT PRIMARY KEY,
  thread_id  TEXT NOT NULL,
  skill_id   TEXT NOT NULL,
  enabled    INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
  FOREIGN KEY (skill_id)  REFERENCES skills(id)  ON DELETE CASCADE,
  UNIQUE(thread_id, skill_id)
);

-- ─── thread_mcp_servers ───────────────────────────────────────────────────────
-- MCP servers enabled for a specific thread.
CREATE TABLE IF NOT EXISTS thread_mcp_servers (
  id            TEXT PRIMARY KEY,
  thread_id     TEXT NOT NULL,
  mcp_server_id TEXT NOT NULL,
  enabled       INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (thread_id)     REFERENCES threads(id)     ON DELETE CASCADE,
  FOREIGN KEY (mcp_server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE,
  UNIQUE(thread_id, mcp_server_id)
);

-- ─── routines ─────────────────────────────────────────────────────────────────
-- Scheduled prompts attached to a thread.
CREATE TABLE IF NOT EXISTS routines (
  id           TEXT PRIMARY KEY,
  thread_id    TEXT NOT NULL,
  name         TEXT NOT NULL,
  prompt       TEXT NOT NULL,
  cron_expr    TEXT NOT NULL,
  enabled      INTEGER NOT NULL DEFAULT 1,
  run_count    INTEGER NOT NULL DEFAULT 0,
  last_run_at  TEXT,
  next_run_at  TEXT,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

-- ─── messages ─────────────────────────────────────────────────────────────────
-- All messages in all threads.
CREATE TABLE IF NOT EXISTS messages (
  id           TEXT PRIMARY KEY,
  thread_id    TEXT NOT NULL,
  role         TEXT NOT NULL,   -- 'user' | 'assistant' | 'system' | 'tool'
  content      TEXT NOT NULL,
  source       TEXT NOT NULL DEFAULT 'chat',  -- 'chat' | 'routine'
  routine_id   TEXT,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (thread_id)  REFERENCES threads(id)   ON DELETE CASCADE,
  FOREIGN KEY (routine_id) REFERENCES routines(id)  ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_thread_created
  ON messages(thread_id, created_at);

-- ─── device_tokens ────────────────────────────────────────────────────────────
-- FCM device tokens for push notifications.
CREATE TABLE IF NOT EXISTS device_tokens (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL,
  token        TEXT NOT NULL UNIQUE,
  platform     TEXT NOT NULL DEFAULT 'android',
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);

-- ─── memory ───────────────────────────────────────────────────────────────────
-- Persistent memory entries scoped to a user + persona (cross-thread, per-agent).
CREATE TABLE IF NOT EXISTS memory (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL,
  persona_id   TEXT NOT NULL,
  thread_id    TEXT,             -- provenance: where the memory was created (nullable)
  content      TEXT NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id)    REFERENCES users(id),
  FOREIGN KEY (persona_id) REFERENCES agent_personas(id) ON DELETE CASCADE,
  FOREIGN KEY (thread_id)  REFERENCES threads(id)        ON DELETE SET NULL
);

-- FTS5 virtual table for memory recall via full-text search.
-- content='' means it's a "contentless" FTS5 table — we manage the index manually
-- by inserting into memory_fts whenever we insert into memory.
-- We use content='memory' with content_rowid='rowid' so SQLite can retrieve
-- the original text for snippet/highlight functions.
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
  content,
  content='memory',
  content_rowid='rowid'
);

-- Triggers to keep the FTS index in sync with the memory table.
-- INSERT trigger: add new memory content to the FTS index.
CREATE TRIGGER IF NOT EXISTS memory_fts_insert
  AFTER INSERT ON memory
BEGIN
  INSERT INTO memory_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- DELETE trigger: remove deleted memory content from the FTS index.
CREATE TRIGGER IF NOT EXISTS memory_fts_delete
  AFTER DELETE ON memory
BEGIN
  INSERT INTO memory_fts(memory_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;

-- UPDATE trigger: update the FTS index when memory content changes.
CREATE TRIGGER IF NOT EXISTS memory_fts_update
  AFTER UPDATE OF content ON memory
BEGIN
  INSERT INTO memory_fts(memory_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
  INSERT INTO memory_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- Migration 002: Align schema with PLAN.md v1.4
-- Removes skills system, updates mcp_servers, updates messages,
-- and adds credentials, routine_executions, persona_default_mcp_servers

-- 1. Remove skills tables
DROP TABLE IF EXISTS thread_skills;
DROP TABLE IF EXISTS skills;

-- 2. Update mcp_servers table
--    SQLite does not support DROP COLUMN before 3.35.0 and ALTER COLUMN at all.
--    Recreate the table with the new schema.
CREATE TABLE mcp_servers_new (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL,
  name         TEXT NOT NULL,
  description  TEXT,
  source_url   TEXT,
  server_type  TEXT NOT NULL DEFAULT 'local' CHECK (server_type IN ('local', 'remote')),
  config       TEXT NOT NULL DEFAULT '{}',
  status       TEXT NOT NULL DEFAULT 'inactive',
  enabled      INTEGER NOT NULL DEFAULT 1,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (user_id) REFERENCES users(id)
);

-- Migrate existing mcp_server rows (map old columns into new config JSON)
INSERT INTO mcp_servers_new (id, user_id, name, server_type, config, enabled, created_at, updated_at)
SELECT
  id,
  user_id,
  name,
  'local',
  json_object('executable', command, 'args', json(args), 'env', json(env)),
  enabled,
  created_at,
  created_at
FROM mcp_servers;

DROP TABLE mcp_servers;
ALTER TABLE mcp_servers_new RENAME TO mcp_servers;

-- 3. Add visibility and execution_id to messages
--    routine_executions does not exist yet, so we add execution_id as a plain TEXT
--    reference and the FK constraint is enforced by the application layer for now.
--    (SQLite does not validate FK references against tables created later in the same
--    migration run when PRAGMA foreign_keys=ON is set at connection time.)
ALTER TABLE messages ADD COLUMN visibility TEXT NOT NULL DEFAULT 'visible'
  CHECK (visibility IN ('visible', 'hidden'));
ALTER TABLE messages ADD COLUMN execution_id TEXT;

-- 4. Add credentials table
CREATE TABLE IF NOT EXISTS credentials (
  id              TEXT PRIMARY KEY,
  key             TEXT NOT NULL UNIQUE,
  display_name    TEXT NOT NULL,
  provider        TEXT NOT NULL,
  credential_type TEXT NOT NULL CHECK (credential_type IN ('oauth2', 'api_key', 'custom')),
  owner_type      TEXT NOT NULL CHECK (owner_type IN ('user', 'persona')),
  persona_id      TEXT REFERENCES agent_personas(id),
  encrypted_data  TEXT NOT NULL,
  scopes          TEXT,
  expires_at      TEXT,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 5. Add routine_executions table
CREATE TABLE IF NOT EXISTS routine_executions (
  id                TEXT PRIMARY KEY,
  routine_id        TEXT NOT NULL REFERENCES routines(id),
  thread_id         TEXT NOT NULL REFERENCES threads(id),
  fired_at          TEXT NOT NULL,
  status            TEXT NOT NULL DEFAULT 'running'
    CHECK (status IN ('running', 'completed', 'failed')),
  output_message_id TEXT REFERENCES messages(id),
  error             TEXT,
  completed_at      TEXT
);

-- 6. Add persona_default_mcp_servers table
CREATE TABLE IF NOT EXISTS persona_default_mcp_servers (
  persona_id    TEXT NOT NULL REFERENCES agent_personas(id) ON DELETE CASCADE,
  mcp_server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
  PRIMARY KEY (persona_id, mcp_server_id)
);

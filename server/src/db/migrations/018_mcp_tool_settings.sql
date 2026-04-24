-- Migration 018: Per-server MCP tool settings
-- Adds a configurable tool-call timeout and a list of disabled tools to mcp_servers.

ALTER TABLE mcp_servers ADD COLUMN tool_call_timeout_secs INTEGER;
ALTER TABLE mcp_servers ADD COLUMN disabled_tools TEXT NOT NULL DEFAULT '[]';

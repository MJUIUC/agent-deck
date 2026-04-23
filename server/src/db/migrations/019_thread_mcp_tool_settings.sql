-- Migration 019: Per-thread MCP tool settings
-- Adds per-thread disabled-tools and tool-call-timeout overrides to thread_mcp_servers.
-- The global mcp_servers values act as defaults; these override them per thread.

ALTER TABLE thread_mcp_servers ADD COLUMN disabled_tools TEXT NOT NULL DEFAULT '[]';
ALTER TABLE thread_mcp_servers ADD COLUMN tool_call_timeout_secs INTEGER;

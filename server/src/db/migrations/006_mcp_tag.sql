-- Migration 006: Add tag column to mcp_servers
-- The tag is a short identifier used to namespace tool names (e.g. "github" → "github__create_issue").
-- Defaults to the server's name so existing rows stay valid immediately.

ALTER TABLE mcp_servers ADD COLUMN tag TEXT NOT NULL DEFAULT '';

-- Back-fill tag = name for any pre-existing rows.
UPDATE mcp_servers SET tag = name WHERE tag = '';

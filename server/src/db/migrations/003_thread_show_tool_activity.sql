-- Migration 003: Add show_tool_activity column to threads table
ALTER TABLE threads ADD COLUMN show_tool_activity INTEGER NOT NULL DEFAULT 0;

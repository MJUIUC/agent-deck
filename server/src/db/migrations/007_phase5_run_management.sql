-- Migration 007: Phase 5 run management
-- Add event_type and stopped to messages, show_system_events to threads

ALTER TABLE messages ADD COLUMN event_type TEXT;
ALTER TABLE messages ADD COLUMN stopped INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN show_system_events INTEGER NOT NULL DEFAULT 0;

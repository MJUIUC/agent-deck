-- Migration 010: Phase 5.8 — Context Window Summarization and Conversation Recall
--
-- Adds rolling summary support to threads, an append-only summary log table,
-- and a per-persona cross-thread conversation recall toggle.

-- New columns on threads
ALTER TABLE threads ADD COLUMN summary               TEXT;
ALTER TABLE threads ADD COLUMN summary_updated_at    TEXT;
ALTER TABLE threads ADD COLUMN summary_message_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN auto_summarize        INTEGER NOT NULL DEFAULT 1;

-- Append-only log of every summarization run
CREATE TABLE thread_summaries (
    id                TEXT PRIMARY KEY,
    thread_id         TEXT NOT NULL REFERENCES threads(id),
    summary           TEXT NOT NULL,
    from_message_seq  INTEGER NOT NULL,
    to_message_seq    INTEGER NOT NULL,
    from_date         TEXT NOT NULL,
    to_date           TEXT NOT NULL,
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_thread_summaries_thread_id ON thread_summaries(thread_id);
CREATE INDEX idx_thread_summaries_to_date   ON thread_summaries(to_date);

-- Per-persona toggle: when 1, recall_conversation searches all threads using this persona;
-- when 0, restricts to the current thread only.
ALTER TABLE agent_personas ADD COLUMN recall_conversation_cross_thread INTEGER NOT NULL DEFAULT 1;

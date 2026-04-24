-- Drop the old per-thread table (introduced in 014, altered in 015)
DROP TABLE IF EXISTS webhook_bindings;

-- Global webhook binding registry.
-- One row per webhook configured on the external service.
-- The secret here is what you give to GitHub/GitLab once and never change.
CREATE TABLE webhook_bindings (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    name       TEXT NOT NULL,
    source     TEXT NOT NULL DEFAULT 'github',
    event_type TEXT NOT NULL DEFAULT '*',
    secret     TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_webhook_bindings_source ON webhook_bindings(source);

-- Per-thread attachment.
-- Attaching a binding to a thread means that binding's events will trigger
-- the agent in that thread, with an optional per-attachment response prompt.
CREATE TABLE thread_webhook_bindings (
    id                 TEXT PRIMARY KEY,
    thread_id          TEXT NOT NULL REFERENCES threads(id),
    webhook_binding_id TEXT NOT NULL REFERENCES webhook_bindings(id),
    prompt             TEXT,
    created_at         TEXT NOT NULL,
    UNIQUE(thread_id, webhook_binding_id)
);
CREATE INDEX idx_thread_webhook_bindings_thread  ON thread_webhook_bindings(thread_id);
CREATE INDEX idx_thread_webhook_bindings_binding ON thread_webhook_bindings(webhook_binding_id);

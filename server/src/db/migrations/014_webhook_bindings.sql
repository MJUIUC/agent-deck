CREATE TABLE webhook_bindings (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    thread_id  TEXT NOT NULL REFERENCES threads(id),
    source     TEXT NOT NULL DEFAULT 'github',
    event_type TEXT NOT NULL,
    secret     TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_webhook_bindings_source ON webhook_bindings(source);
CREATE INDEX idx_webhook_bindings_thread ON webhook_bindings(thread_id);

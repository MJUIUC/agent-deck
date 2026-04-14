-- Migration 011: push_subscriptions table for Web Push (VAPID) notifications.
-- Each row represents one browser push subscription belonging to a user.
-- A user may have multiple subscriptions (one per browser/device).

CREATE TABLE IF NOT EXISTS push_subscriptions (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint    TEXT NOT NULL,
    p256dh      TEXT NOT NULL,
    auth        TEXT NOT NULL,
    user_agent  TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE(user_id, endpoint)
);

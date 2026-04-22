-- v2 schema changes:
-- • removes event_type (filtering managed on the external service side)
-- • adds prompt — default response instructions; used as fallback when the
--   thread attachment has no prompt of its own
-- • adds signature_header — required for source='other', names the HTTP header
--   the external service uses to send its HMAC signature (e.g. X-Linear-Signature)
-- • renames source value 'generic' → 'other'

CREATE TABLE webhook_bindings_v2 (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id),
    name             TEXT NOT NULL,
    source           TEXT NOT NULL DEFAULT 'github',
    signature_header TEXT,
    prompt           TEXT NOT NULL DEFAULT '',
    secret           TEXT NOT NULL DEFAULT '',
    enabled          INTEGER NOT NULL DEFAULT 1,
    created_at       TEXT NOT NULL
);

INSERT INTO webhook_bindings_v2 (id, user_id, name, source, prompt, secret, enabled, created_at)
SELECT
    id,
    user_id,
    name,
    CASE WHEN source = 'generic' THEN 'other' ELSE source END,
    '',
    secret,
    enabled,
    created_at
FROM webhook_bindings;

DROP TABLE webhook_bindings;
ALTER TABLE webhook_bindings_v2 RENAME TO webhook_bindings;

CREATE INDEX idx_webhook_bindings_source ON webhook_bindings(source);

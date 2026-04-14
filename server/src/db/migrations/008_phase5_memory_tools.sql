-- Migration 008: Phase 5.2 — Memory tools / Default persona
--
-- 1. Add `is_default` flag to agent_personas.
--    Default personas cannot be edited or deleted via the API.
--
-- 2. Seed the Default persona for any users that already exist
--    (i.e. existing installations upgrading from an older schema).
--    For fresh installs the Default persona is created by
--    POST /api/setup/complete so that the correct user_id is available.

ALTER TABLE agent_personas ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0;

-- Seed one Default persona per existing user that does not already have one.
-- lower(hex(randomblob(16))) produces a 32-char hex string that satisfies the
-- TEXT PRIMARY KEY constraint.  The single-user constraint means this INSERT
-- fires at most once per database.
INSERT INTO agent_personas
    (id, user_id, name, emoji, avatar_path, system_prompt, is_default,
     default_model, default_provider, created_at, updated_at)
SELECT
    lower(hex(randomblob(16))),
    id,
    'Default',
    '💬',
    NULL,
    '',
    1,
    NULL,
    NULL,
    datetime('now'),
    datetime('now')
FROM users
WHERE NOT EXISTS (
    SELECT 1 FROM agent_personas
    WHERE user_id = users.id AND is_default = 1
);

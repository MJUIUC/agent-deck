-- Migration 004: Phase 4 credential store
-- Replaces the Phase 3 credentials table (oauth2/provider/owner_type schema)
-- with the Phase 4 schema (service/service_url/username/email, expanded types).
-- Also adds credential_key to providers for linked key resolution.

-- 1. Drop the old credentials table (introduced in migration 002).
--    No data migration needed — the old table was never populated via the UI.
DROP TABLE IF EXISTS credentials;

-- 2. Create the Phase 4 credentials table.
CREATE TABLE credentials (
  id              TEXT PRIMARY KEY,
  key             TEXT NOT NULL UNIQUE,
  display_name    TEXT NOT NULL,
  service         TEXT NOT NULL,
  credential_type TEXT NOT NULL CHECK (credential_type IN ('api_key', 'pat', 'bearer_token', 'key_secret_pair', 'service_account')),
  service_url     TEXT,
  username        TEXT,
  email           TEXT,
  encrypted_data  TEXT NOT NULL,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 3. Add credential_key column to providers.
--    Stores the credentials.key value that supplies the API key for this provider.
--    NULL means the provider has no linked credential (e.g. Copilot uses device auth).
ALTER TABLE providers ADD COLUMN credential_key TEXT;

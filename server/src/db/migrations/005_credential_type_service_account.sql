-- Migration 005: Add 'service_account' to credentials credential_type CHECK constraint.
-- SQLite does not support ALTER TABLE ... MODIFY COLUMN, so we recreate the table
-- with the updated constraint and copy all existing rows across.

-- 1. Create the updated table under a temporary name.
CREATE TABLE credentials_new (
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

-- 2. Copy all existing rows.
INSERT INTO credentials_new
  SELECT id, key, display_name, service, credential_type,
         service_url, username, email, encrypted_data, created_at, updated_at
  FROM credentials;

-- 3. Drop the old table.
DROP TABLE credentials;

-- 4. Rename the new table into place.
ALTER TABLE credentials_new RENAME TO credentials;

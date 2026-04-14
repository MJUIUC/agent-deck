-- Migration 009: Phase 5.7 — User Profile
--
-- Add profile fields to the users table. All columns default to NULL.
-- profile_updated_at is set server-side on every PUT /api/profile.

ALTER TABLE users ADD COLUMN pronouns           TEXT;
ALTER TABLE users ADD COLUMN role               TEXT;
ALTER TABLE users ADD COLUMN organization       TEXT;
ALTER TABLE users ADD COLUMN location           TEXT;
ALTER TABLE users ADD COLUMN timezone           TEXT;
ALTER TABLE users ADD COLUMN about              TEXT;
ALTER TABLE users ADD COLUMN profile_updated_at TEXT;

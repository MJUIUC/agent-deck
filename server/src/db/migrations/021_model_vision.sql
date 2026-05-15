-- Move vision capability flag from providers to individual models.
-- provider.vision is kept in place (non-destructive) but is no longer used by the run loop.
ALTER TABLE models ADD COLUMN vision INTEGER NOT NULL DEFAULT 0;

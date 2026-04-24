-- Add vision capability flag to providers
ALTER TABLE providers ADD COLUMN vision INTEGER NOT NULL DEFAULT 0;

-- Add attachments column to messages (JSON array of MessageAttachment)
ALTER TABLE messages ADD COLUMN attachments TEXT;

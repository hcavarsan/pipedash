-- Add version column to providers table for optimistic locking
ALTER TABLE providers ADD COLUMN version INTEGER DEFAULT 1 NOT NULL;

-- Create index for faster version lookups
CREATE INDEX idx_providers_version ON providers(id, version);

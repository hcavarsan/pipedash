-- Add pinned column to pipelines_cache for menu bar feature
-- Using INTEGER (0/1) for consistency with SQLite and simpler generic decoding
ALTER TABLE pipelines_cache ADD COLUMN IF NOT EXISTS pinned INTEGER NOT NULL DEFAULT 0;

-- Create partial index for efficient querying of pinned pipelines
CREATE INDEX IF NOT EXISTS idx_pipelines_pinned ON pipelines_cache(pinned) WHERE pinned = 1;

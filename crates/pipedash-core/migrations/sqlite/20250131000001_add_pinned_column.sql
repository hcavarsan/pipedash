-- Add pinned column to pipelines_cache for menu bar feature
ALTER TABLE pipelines_cache ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;

-- Create index for efficient querying of pinned pipelines
CREATE INDEX IF NOT EXISTS idx_pipelines_pinned ON pipelines_cache(pinned) WHERE pinned = 1;

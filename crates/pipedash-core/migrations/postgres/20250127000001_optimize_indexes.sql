-- Add provider_type column to pipelines_cache for JOIN optimization
ALTER TABLE pipelines_cache ADD COLUMN IF NOT EXISTS provider_type TEXT;

-- Create index on provider_type
CREATE INDEX IF NOT EXISTS idx_pipelines_provider_type ON pipelines_cache(provider_type);

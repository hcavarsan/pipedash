-- Index already exists from initial schema, but adding for consistency
CREATE INDEX IF NOT EXISTS idx_pipelines_provider ON pipelines_cache(provider_id);

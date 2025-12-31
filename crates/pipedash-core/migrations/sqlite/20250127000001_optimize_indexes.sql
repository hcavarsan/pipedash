-- Performance Optimization: Database Index Improvements
-- This migration optimizes indexes for pagination and common query patterns

-- 1. Fix run history pagination index
-- Current index uses fetched_at which is not used in ORDER BY
-- Replace with run_number DESC which is actually used for ordering
DROP INDEX IF EXISTS idx_run_history_pipeline_fetched;
CREATE INDEX IF NOT EXISTS idx_run_history_pipeline_run
    ON run_history_cache(pipeline_id, run_number DESC);

-- 2. Covering index for pagination queries
-- Includes run_data so queries don't need to access the table
CREATE INDEX IF NOT EXISTS idx_run_history_covering
    ON run_history_cache(pipeline_id, run_number DESC, run_data);

-- 3. Add provider_type column to pipelines_cache to avoid JOINs
-- This eliminates the need to JOIN with providers table on every query
ALTER TABLE pipelines_cache ADD COLUMN provider_type TEXT;

-- 4. Covering index for common pipeline queries
-- Includes all frequently accessed columns to avoid table lookups
CREATE INDEX IF NOT EXISTS idx_pipelines_covering
    ON pipelines_cache(provider_id, status, last_updated DESC, id, name, provider_type);

-- 5. Index for provider type queries
CREATE INDEX IF NOT EXISTS idx_providers_type
    ON providers(provider_type);

-- 6. Backfill provider_type for existing pipelines_cache entries
-- This ensures existing data has the provider_type populated
UPDATE pipelines_cache
SET provider_type = (
    SELECT provider_type
    FROM providers
    WHERE providers.id = pipelines_cache.provider_id
)
WHERE provider_type IS NULL;

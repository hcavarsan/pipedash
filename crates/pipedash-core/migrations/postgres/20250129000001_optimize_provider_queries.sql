-- Performance Optimization: Provider Status Queries
-- This migration adds an index to optimize provider status and fetch time queries

-- Add index for provider fetch status queries
-- Used by queries that filter or sort by last_fetch_status and last_fetch_at
CREATE INDEX IF NOT EXISTS idx_providers_fetch_status
    ON providers(last_fetch_status, last_fetch_at DESC);

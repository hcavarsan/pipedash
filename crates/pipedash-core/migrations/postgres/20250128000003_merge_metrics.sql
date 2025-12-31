-- Migration: Merge metrics tables into main database (public schema)
-- This replaces the separate metrics schema with tables in the public schema.
-- Previous metrics data will be migrated by the application on startup.

-- Global metrics configuration (singleton row)
CREATE TABLE IF NOT EXISTS metrics_global_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled BOOLEAN NOT NULL DEFAULT false,
    default_retention_days INTEGER NOT NULL DEFAULT 7,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO metrics_global_config (id, enabled, default_retention_days, updated_at)
VALUES (1, false, 7, NOW())
ON CONFLICT (id) DO NOTHING;

-- Per-pipeline metrics configuration
CREATE TABLE IF NOT EXISTS metrics_config (
    pipeline_id TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT false,
    retention_days INTEGER NOT NULL DEFAULT 7,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Pipeline metrics data
CREATE TABLE IF NOT EXISTS pipeline_metrics (
    id BIGSERIAL PRIMARY KEY,
    pipeline_id TEXT NOT NULL,
    run_number INTEGER,
    timestamp TIMESTAMPTZ NOT NULL,
    metric_type TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    metadata_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    run_hash TEXT
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_pipeline_time
    ON pipeline_metrics(pipeline_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_type_time
    ON pipeline_metrics(metric_type, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_timestamp
    ON pipeline_metrics(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_run
    ON pipeline_metrics(pipeline_id, run_number);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_hash
    ON pipeline_metrics(run_hash);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_dedup
    ON pipeline_metrics(pipeline_id, run_hash, metric_type);

-- Processing state tracking (to resume from last processed run)
CREATE TABLE IF NOT EXISTS metrics_processing_state (
    pipeline_id TEXT PRIMARY KEY,
    last_processed_run_number INTEGER NOT NULL DEFAULT 0,
    last_processed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_metrics_processing_updated
    ON metrics_processing_state(updated_at);

-- Storage statistics (singleton row)
CREATE TABLE IF NOT EXISTS metrics_storage_info (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    total_metrics_count INTEGER NOT NULL DEFAULT 0,
    estimated_size_bytes BIGINT NOT NULL DEFAULT 0,
    last_cleanup_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO metrics_storage_info (id, total_metrics_count, estimated_size_bytes, updated_at)
VALUES (1, 0, 0, NOW())
ON CONFLICT (id) DO NOTHING;

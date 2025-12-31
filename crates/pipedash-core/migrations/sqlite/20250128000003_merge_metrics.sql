-- Migration: Merge metrics tables into main database
-- This replaces the separate metrics.db file with tables in the main database.
-- Previous metrics data will be migrated by the application on startup.

-- Global metrics configuration (singleton row)
CREATE TABLE IF NOT EXISTS metrics_global_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    default_retention_days INTEGER NOT NULL DEFAULT 7,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO metrics_global_config (id, enabled, default_retention_days, updated_at)
VALUES (1, 0, 7, datetime('now'));

-- Per-pipeline metrics configuration
CREATE TABLE IF NOT EXISTS metrics_config (
    pipeline_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 0,
    retention_days INTEGER NOT NULL DEFAULT 7,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Pipeline metrics data
CREATE TABLE IF NOT EXISTS pipeline_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pipeline_id TEXT NOT NULL,
    run_number INTEGER,
    timestamp TEXT NOT NULL,
    metric_type TEXT NOT NULL,
    value REAL NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    run_hash TEXT
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_pipeline_time
    ON pipeline_metrics(pipeline_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_type_time
    ON pipeline_metrics(metric_type, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_timestamp
    ON pipeline_metrics(timestamp DESC);

-- Unique index for deduplication (only for valid run_number and run_hash)
CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_pipeline_run_metric_hash
    ON pipeline_metrics(pipeline_id, run_number, metric_type, run_hash)
    WHERE run_number > 0 AND run_hash IS NOT NULL;

-- Index for hash lookups
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_hash
    ON pipeline_metrics(run_hash)
    WHERE run_hash IS NOT NULL;

-- Processing state tracking (to resume from last processed run)
CREATE TABLE IF NOT EXISTS metrics_processing_state (
    pipeline_id TEXT PRIMARY KEY,
    last_processed_run_number INTEGER NOT NULL DEFAULT 0,
    last_processed_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_processing_state_updated
    ON metrics_processing_state(updated_at DESC);

-- Storage statistics (singleton row)
CREATE TABLE IF NOT EXISTS metrics_storage_info (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    total_metrics_count INTEGER NOT NULL DEFAULT 0,
    estimated_size_bytes INTEGER NOT NULL DEFAULT 0,
    last_cleanup_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO metrics_storage_info (id, total_metrics_count, estimated_size_bytes, updated_at)
VALUES (1, 0, 0, datetime('now'));

CREATE TABLE IF NOT EXISTS metrics_global_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    default_retention_days INTEGER NOT NULL DEFAULT 7,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO metrics_global_config (id, enabled, default_retention_days, updated_at)
VALUES (1, 0, 7, datetime('now'));

CREATE TABLE IF NOT EXISTS metrics_config (
    pipeline_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 0,
    retention_days INTEGER NOT NULL DEFAULT 7,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS pipeline_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pipeline_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    metric_type TEXT NOT NULL,
    value REAL NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_pipeline_time
    ON pipeline_metrics(pipeline_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_type_time
    ON pipeline_metrics(metric_type, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_timestamp
    ON pipeline_metrics(timestamp DESC);

CREATE TABLE IF NOT EXISTS metrics_storage_info (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    total_metrics_count INTEGER NOT NULL DEFAULT 0,
    estimated_size_bytes INTEGER NOT NULL DEFAULT 0,
    last_cleanup_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO metrics_storage_info (id, total_metrics_count, estimated_size_bytes, updated_at)
VALUES (1, 0, 0, datetime('now'));

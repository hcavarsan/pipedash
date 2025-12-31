CREATE TABLE IF NOT EXISTS metrics_global_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled BOOLEAN NOT NULL DEFAULT false,
    default_retention_days INTEGER NOT NULL DEFAULT 7,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO metrics_global_config (id, enabled, default_retention_days, updated_at)
VALUES (1, false, 7, NOW())
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS metrics_config (
    pipeline_id TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT false,
    retention_days INTEGER NOT NULL DEFAULT 7,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS pipeline_metrics (
    id BIGSERIAL PRIMARY KEY,
    pipeline_id TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    metric_type TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    metadata_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
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
    estimated_size_bytes BIGINT NOT NULL DEFAULT 0,
    last_cleanup_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO metrics_storage_info (id, total_metrics_count, estimated_size_bytes, updated_at)
VALUES (1, 0, 0, NOW())
ON CONFLICT (id) DO NOTHING;

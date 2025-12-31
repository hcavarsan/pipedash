ALTER TABLE pipeline_metrics ADD COLUMN IF NOT EXISTS run_number INTEGER;

CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_run ON pipeline_metrics(pipeline_id, run_number);

CREATE TABLE IF NOT EXISTS metrics_processing_state (
    pipeline_id TEXT PRIMARY KEY,
    last_processed_run_number INTEGER NOT NULL,
    last_processed_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_metrics_processing_updated ON metrics_processing_state(updated_at);

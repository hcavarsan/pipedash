ALTER TABLE pipeline_metrics ADD COLUMN IF NOT EXISTS run_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_hash ON pipeline_metrics(run_hash);

CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_dedup
    ON pipeline_metrics(pipeline_id, run_hash, metric_type);

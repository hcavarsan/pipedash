ALTER TABLE pipeline_metrics ADD COLUMN run_hash TEXT;

DROP INDEX IF EXISTS idx_unique_pipeline_run_metric;

CREATE UNIQUE INDEX idx_unique_pipeline_run_metric_hash
    ON pipeline_metrics(pipeline_id, run_number, metric_type, run_hash)
    WHERE run_number > 0 AND run_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_pipeline_metrics_hash
    ON pipeline_metrics(run_hash)
    WHERE run_hash IS NOT NULL;

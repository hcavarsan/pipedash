ALTER TABLE pipeline_metrics ADD COLUMN run_number INTEGER;

UPDATE pipeline_metrics SET run_number = 0 WHERE run_number IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_pipeline_run_metric
    ON pipeline_metrics(pipeline_id, run_number, metric_type)
    WHERE run_number > 0;

CREATE TABLE IF NOT EXISTS metrics_processing_state (
    pipeline_id TEXT PRIMARY KEY,
    last_processed_run_number INTEGER NOT NULL,
    last_processed_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_processing_state_updated
    ON metrics_processing_state(updated_at DESC);

INSERT OR IGNORE INTO metrics_processing_state (pipeline_id, last_processed_run_number, last_processed_at)
SELECT pipeline_id, MAX(run_number), MAX(timestamp)
FROM pipeline_metrics
WHERE run_number > 0
GROUP BY pipeline_id;

PRAGMA auto_vacuum = INCREMENTAL;

DELETE FROM pipeline_metrics
WHERE id NOT IN (
    SELECT MAX(id)
    FROM pipeline_metrics
    WHERE run_number > 0
    GROUP BY pipeline_id, run_number, metric_type
)
AND run_number > 0;

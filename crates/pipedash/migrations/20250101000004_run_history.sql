CREATE TABLE IF NOT EXISTS run_history_cache (
    pipeline_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    run_data TEXT NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (pipeline_id, run_number)
);

CREATE INDEX IF NOT EXISTS idx_run_history_pipeline_fetched
    ON run_history_cache(pipeline_id, fetched_at DESC);

ALTER TABLE workflow_parameters_cache ADD COLUMN expires_at TEXT;

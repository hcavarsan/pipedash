CREATE TABLE IF NOT EXISTS run_history_cache (
    pipeline_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    run_data TEXT NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (pipeline_id, run_number)
);

CREATE INDEX IF NOT EXISTS idx_run_history_fetched_at ON run_history_cache(fetched_at);
CREATE INDEX IF NOT EXISTS idx_run_history_pipeline ON run_history_cache(pipeline_id);

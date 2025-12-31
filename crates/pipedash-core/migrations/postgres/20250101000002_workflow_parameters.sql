CREATE TABLE IF NOT EXISTS workflow_parameters_cache (
    workflow_id TEXT PRIMARY KEY,
    parameters_json TEXT NOT NULL,
    cached_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_workflow_params_cached_at ON workflow_parameters_cache(cached_at);

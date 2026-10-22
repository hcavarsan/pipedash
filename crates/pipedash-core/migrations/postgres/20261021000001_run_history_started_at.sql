ALTER TABLE run_history_cache ADD COLUMN IF NOT EXISTS started_at_ms BIGINT NOT NULL DEFAULT 0;

UPDATE run_history_cache
SET started_at_ms = COALESCE(
    FLOOR(EXTRACT(EPOCH FROM (run_data::jsonb ->> 'started_at')::timestamptz) * 1000)::BIGINT,
    0
)
WHERE pipeline_id LIKE 'fluxcd__%';

CREATE INDEX IF NOT EXISTS idx_run_history_pipeline_started
    ON run_history_cache (pipeline_id, started_at_ms DESC, run_number DESC);

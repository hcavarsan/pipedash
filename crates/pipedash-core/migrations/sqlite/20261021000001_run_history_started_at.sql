ALTER TABLE run_history_cache ADD COLUMN started_at_ms INTEGER NOT NULL DEFAULT 0;

UPDATE run_history_cache
SET started_at_ms = COALESCE(
    CAST(strftime('%s', substr(json_extract(run_data, '$.started_at'), 1, 19) || 'Z') AS INTEGER) * 1000
    + CASE WHEN substr(json_extract(run_data, '$.started_at'), 20, 1) = '.'
        THEN CAST(substr(rtrim(substr(json_extract(run_data, '$.started_at'), 21), 'Z') || '000', 1, 3) AS INTEGER)
        ELSE 0
      END,
    0
)
WHERE pipeline_id LIKE 'fluxcd__%';

CREATE INDEX IF NOT EXISTS idx_run_history_pipeline_started
    ON run_history_cache(pipeline_id, started_at_ms DESC, run_number DESC);

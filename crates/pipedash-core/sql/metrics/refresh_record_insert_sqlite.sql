INSERT INTO pipeline_metrics (pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash)
SELECT ?, ?, ?, ?, ?, ?, ?, ?
WHERE NOT EXISTS (
    SELECT 1 FROM pipeline_metrics
    WHERE pipeline_id = ? AND run_number = ? AND metric_type = ?
)
INSERT INTO pipeline_metrics (pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash)
SELECT $1, $2, $3, $4, $5, $6, $7, $8
WHERE NOT EXISTS (
    SELECT 1 FROM pipeline_metrics
    WHERE pipeline_id = $1 AND run_number = $2 AND metric_type = $4
)
SELECT pipeline_id, run_number, outcome_recorded, timing_recorded, attempt, started_at, concluded_at, status
FROM run_evidence
WHERE concluded_at >= $1 AND concluded_at <= $2
ORDER BY concluded_at, pipeline_id, run_number

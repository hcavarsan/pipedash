SELECT pipeline_id, run_number, outcome_recorded, timing_recorded, attempt, started_at, concluded_at, status
FROM run_evidence
WHERE concluded_at >= ? AND concluded_at <= ?
ORDER BY concluded_at, pipeline_id, run_number

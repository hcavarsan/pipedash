INSERT INTO run_evidence (pipeline_id, run_number, outcome_recorded, timing_recorded, attempt, started_at, concluded_at, status)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (pipeline_id, run_number) DO UPDATE SET
    outcome_recorded = excluded.outcome_recorded,
    timing_recorded = excluded.timing_recorded,
    attempt = excluded.attempt,
    started_at = excluded.started_at,
    concluded_at = excluded.concluded_at,
    status = excluded.status

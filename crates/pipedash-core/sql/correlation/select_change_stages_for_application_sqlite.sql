SELECT s.change_id, s.stage, s.group_key, s.pipeline_id, s.run_number, s.started_at, s.concluded_at, s.status,
       s.outcome_recorded, s.timing_recorded, s.attempt
FROM change_stages s
JOIN changes c ON c.id = s.change_id
WHERE c.application_id = ?
ORDER BY s.change_id, s.stage, s.group_key, s.pipeline_id, s.run_number

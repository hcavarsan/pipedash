INSERT INTO log_scans (pipeline_id, run_number, run_hash, scanned_at)
VALUES ($1, $2, $3, $4)
ON CONFLICT (pipeline_id, run_number) DO UPDATE SET
    run_hash = excluded.run_hash,
    scanned_at = excluded.scanned_at

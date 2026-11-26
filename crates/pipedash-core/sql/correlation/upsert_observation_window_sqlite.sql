INSERT INTO observation_windows (provider_id, started_at, ended_at)
VALUES (?, ?, ?)
ON CONFLICT (provider_id, started_at) DO UPDATE SET
    ended_at = excluded.ended_at

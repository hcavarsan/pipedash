ALTER TABLE run_history_cache ADD COLUMN run_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_run_history_hash
    ON run_history_cache(run_hash)
    WHERE run_hash IS NOT NULL;

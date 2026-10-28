ALTER TABLE providers ADD COLUMN IF NOT EXISTS last_success_at TIMESTAMPTZ;

UPDATE providers SET last_success_at = last_fetch_at WHERE last_fetch_status = 'success';

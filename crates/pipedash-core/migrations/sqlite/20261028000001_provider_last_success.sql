ALTER TABLE providers ADD COLUMN last_success_at TEXT;

UPDATE providers SET last_success_at = last_fetch_at WHERE last_fetch_status = 'success';

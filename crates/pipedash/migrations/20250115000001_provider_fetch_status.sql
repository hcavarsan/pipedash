-- Add fetch status tracking to providers table
ALTER TABLE providers ADD COLUMN last_fetch_at TEXT;
ALTER TABLE providers ADD COLUMN last_fetch_status TEXT DEFAULT 'never';
ALTER TABLE providers ADD COLUMN last_fetch_error TEXT;

CREATE INDEX IF NOT EXISTS idx_providers_fetch_status ON providers(last_fetch_status);

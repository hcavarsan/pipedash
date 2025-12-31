ALTER TABLE providers ADD COLUMN IF NOT EXISTS last_fetch_at TIMESTAMPTZ;
ALTER TABLE providers ADD COLUMN IF NOT EXISTS last_fetch_status TEXT;
ALTER TABLE providers ADD COLUMN IF NOT EXISTS last_fetch_error TEXT;

CREATE INDEX IF NOT EXISTS idx_providers_fetch_status ON providers(last_fetch_status);
CREATE INDEX IF NOT EXISTS idx_providers_fetch_at ON providers(last_fetch_at);

CREATE TABLE IF NOT EXISTS subscriptions (
    id_hash TEXT PRIMARY KEY,
    device_token TEXT NOT NULL,
    environment TEXT NOT NULL CHECK (environment IN ('production', 'sandbox')),
    bundle_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_active_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_device_token ON subscriptions(device_token);
CREATE INDEX IF NOT EXISTS idx_subscriptions_last_active ON subscriptions(last_active_at);

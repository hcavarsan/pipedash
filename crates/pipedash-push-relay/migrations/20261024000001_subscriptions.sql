CREATE TABLE IF NOT EXISTS subscriptions (
    id_hash TEXT PRIMARY KEY,
    device_token TEXT NOT NULL,
    environment TEXT NOT NULL CHECK (environment IN ('production', 'sandbox')),
    bundle_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS push_subscriptions (
    id TEXT PRIMARY KEY,
    relay_subscription_id TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_success_at TEXT
);

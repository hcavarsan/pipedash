CREATE TABLE IF NOT EXISTS device_alerts (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    alert_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    pipeline_id TEXT,
    provider_id INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_device_alerts_created_at ON device_alerts(created_at);

CREATE TABLE IF NOT EXISTS device_push_subscriptions (
    device_id TEXT PRIMARY KEY,
    relay_subscription_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_success_at TEXT,
    undelivered_seq INTEGER
);

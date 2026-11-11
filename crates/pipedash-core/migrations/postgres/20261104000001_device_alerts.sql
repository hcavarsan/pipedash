CREATE TABLE IF NOT EXISTS device_alerts (
    seq BIGSERIAL PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    alert_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    pipeline_id TEXT,
    provider_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_device_alerts_created_at ON device_alerts(created_at);

CREATE TABLE IF NOT EXISTS device_push_subscriptions (
    device_id TEXT PRIMARY KEY,
    relay_subscription_id TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_success_at TIMESTAMPTZ,
    undelivered_seq BIGINT,
    undelivered_max_seq BIGINT
);

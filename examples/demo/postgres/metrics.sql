CREATE TABLE metrics_global_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled BOOLEAN NOT NULL DEFAULT false,
    default_retention_days INTEGER NOT NULL DEFAULT 7,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO metrics_global_config (id, enabled, default_retention_days, updated_at)
VALUES (1, true, 30, NOW());

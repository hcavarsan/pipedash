CREATE TABLE IF NOT EXISTS provider_permissions (
    provider_id BIGINT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    permission_name TEXT NOT NULL,
    granted BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (provider_id, permission_name)
);

CREATE INDEX IF NOT EXISTS idx_permissions_provider ON provider_permissions(provider_id);

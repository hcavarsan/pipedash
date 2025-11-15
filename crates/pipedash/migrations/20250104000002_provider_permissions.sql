-- Add provider_permissions table to track token permissions
CREATE TABLE IF NOT EXISTS provider_permissions (
    provider_id INTEGER NOT NULL,
    permission_name TEXT NOT NULL,
    granted BOOLEAN NOT NULL,
    checked_at TEXT NOT NULL,
    PRIMARY KEY (provider_id, permission_name),
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_provider_permissions_provider_id
    ON provider_permissions(provider_id);

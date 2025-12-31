-- Storage metadata table
-- Tracks the current storage backend configuration and state
CREATE TABLE IF NOT EXISTS storage_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1), -- Single row table
    backend_type TEXT NOT NULL, -- 'sqlite', 'postgres', 's3_sqlite', 'hybrid'
    token_backend TEXT NOT NULL, -- 'memory', 'stronghold', 'keyring', 'env'
    cache_backend TEXT NOT NULL, -- 'local', 's3', 'gcs'
    config_json TEXT NOT NULL, -- Full StorageConfig as JSON
    checksum TEXT NOT NULL, -- SHA-256 hash of config for integrity
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Storage migrations table
-- Tracks all migration operations for audit trail
CREATE TABLE IF NOT EXISTS storage_migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_backend TEXT NOT NULL,
    to_backend TEXT NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'in_progress', 'completed', 'failed', 'rolled_back'
    steps_completed TEXT, -- JSON array of completed MigrationStep
    backup_path TEXT,
    error_message TEXT,
    providers_migrated INTEGER DEFAULT 0,
    tokens_migrated INTEGER DEFAULT 0,
    cache_entries_migrated INTEGER DEFAULT 0,
    permissions_migrated INTEGER DEFAULT 0,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    duration_ms INTEGER
);

-- S3 sync state table
-- Tracks S3 synchronization state for hybrid backend
CREATE TABLE IF NOT EXISTS s3_sync_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    database_name TEXT NOT NULL UNIQUE, -- 'pipedash' or 'metrics'
    last_sync_at TEXT,
    last_upload_at TEXT,
    last_download_at TEXT,
    local_checksum TEXT, -- SHA-256 of local database
    remote_checksum TEXT, -- SHA-256 of remote database
    remote_etag TEXT, -- S3 ETag for conflict detection
    sync_status TEXT NOT NULL DEFAULT 'idle', -- 'idle', 'uploading', 'downloading', 'conflict'
    conflict_resolution TEXT, -- 'last_write_wins', 'prefer_local', 'prefer_remote', 'skip'
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_storage_migrations_status ON storage_migrations(status);
CREATE INDEX IF NOT EXISTS idx_storage_migrations_started_at ON storage_migrations(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_s3_sync_state_database ON s3_sync_state(database_name);
CREATE INDEX IF NOT EXISTS idx_s3_sync_state_status ON s3_sync_state(sync_status);

-- Insert default storage metadata if not exists
INSERT OR IGNORE INTO storage_metadata (id, backend_type, token_backend, cache_backend, config_json, checksum)
VALUES (
    1,
    'sqlite',
    'stronghold',
    'local',
    '{"data_dir":"~/.local/share/pipedash","token_backend":"stronghold","config_backend":"sqlite","cache_backend":"local","settings":{}}',
    '' -- Will be populated by application
);

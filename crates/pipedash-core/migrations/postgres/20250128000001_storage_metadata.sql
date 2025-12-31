-- Storage metadata table
-- Tracks the current storage backend configuration and state
CREATE TABLE IF NOT EXISTS storage_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1), -- Single row table
    backend_type TEXT NOT NULL, -- 'sqlite', 'postgres', 's3_sqlite', 'hybrid'
    token_backend TEXT NOT NULL, -- 'memory', 'stronghold', 'keyring', 'env', 'postgres'
    cache_backend TEXT NOT NULL, -- 'local', 's3', 'gcs'
    config_json TEXT NOT NULL, -- Full StorageConfig as JSON
    checksum TEXT NOT NULL, -- SHA-256 hash of config for integrity
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Storage migrations table
-- Tracks all migration operations for audit trail
CREATE TABLE IF NOT EXISTS storage_migrations (
    id BIGSERIAL PRIMARY KEY,
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
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    duration_ms BIGINT
);

-- S3 sync state table
-- Tracks S3 synchronization state for hybrid backend
CREATE TABLE IF NOT EXISTS s3_sync_state (
    id BIGSERIAL PRIMARY KEY,
    database_name TEXT NOT NULL UNIQUE, -- 'pipedash' or 'metrics'
    last_sync_at TIMESTAMPTZ,
    last_upload_at TIMESTAMPTZ,
    last_download_at TIMESTAMPTZ,
    local_checksum TEXT, -- SHA-256 of local database
    remote_checksum TEXT, -- SHA-256 of remote database
    remote_etag TEXT, -- S3 ETag for conflict detection
    sync_status TEXT NOT NULL DEFAULT 'idle', -- 'idle', 'uploading', 'downloading', 'conflict'
    conflict_resolution TEXT, -- 'last_write_wins', 'prefer_local', 'prefer_remote', 'skip'
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_storage_migrations_status ON storage_migrations(status);
CREATE INDEX IF NOT EXISTS idx_storage_migrations_started ON storage_migrations(started_at);
CREATE INDEX IF NOT EXISTS idx_s3_sync_status ON s3_sync_state(sync_status);

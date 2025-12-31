CREATE TABLE IF NOT EXISTS providers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    provider_type TEXT NOT NULL,
    token_encrypted TEXT NOT NULL,
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS pipelines_cache (
    id TEXT PRIMARY KEY,
    provider_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    repository TEXT NOT NULL,
    branch TEXT,
    workflow_file TEXT,
    last_run TEXT,
    last_updated TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pipelines_provider ON pipelines_cache(provider_id);
CREATE INDEX IF NOT EXISTS idx_pipelines_status ON pipelines_cache(status);

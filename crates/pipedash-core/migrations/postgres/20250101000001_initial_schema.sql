CREATE TABLE IF NOT EXISTS providers (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    provider_type TEXT NOT NULL,
    token_encrypted TEXT NOT NULL,
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS pipelines_cache (
    id TEXT PRIMARY KEY,
    provider_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    repository TEXT NOT NULL,
    branch TEXT,
    workflow_file TEXT,
    last_run TIMESTAMPTZ,
    last_updated TIMESTAMPTZ NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pipelines_provider ON pipelines_cache(provider_id);
CREATE INDEX IF NOT EXISTS idx_pipelines_status ON pipelines_cache(status);

CREATE TABLE IF NOT EXISTS table_preferences (
    id BIGSERIAL PRIMARY KEY,
    provider_id BIGINT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    table_id TEXT NOT NULL,
    preferences_json TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider_id, table_id)
);

CREATE INDEX IF NOT EXISTS idx_table_prefs_provider ON table_preferences(provider_id);

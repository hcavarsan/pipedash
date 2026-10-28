CREATE TABLE IF NOT EXISTS pipeline_views (
    id BIGSERIAL PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('all','pinned','hidden','custom')),
    name TEXT NOT NULL,
    parent_id BIGINT REFERENCES pipeline_views(id) ON DELETE SET NULL,
    position BIGINT NOT NULL DEFAULT 0,
    rules_json TEXT NOT NULL DEFAULT '{}',
    display_json TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX IF NOT EXISTS pipeline_views_system_kind ON pipeline_views(kind) WHERE kind <> 'custom';
CREATE TABLE IF NOT EXISTS pipeline_view_members (
    view_id BIGINT NOT NULL REFERENCES pipeline_views(id) ON DELETE CASCADE,
    provider_id BIGINT NOT NULL,
    pipeline_id TEXT NOT NULL,
    position BIGINT NOT NULL DEFAULT 0,
    excluded BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (view_id, pipeline_id)
);
CREATE INDEX IF NOT EXISTS pipeline_view_members_provider ON pipeline_view_members(provider_id);
CREATE TABLE IF NOT EXISTS pipeline_view_pins (
    id BIGSERIAL PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('view','repository')),
    ref TEXT NOT NULL,
    position BIGINT NOT NULL DEFAULT 0,
    UNIQUE (kind, ref)
);
INSERT INTO pipeline_views (kind, name, position, display_json) VALUES
  ('all', 'All pipelines', 0, '{"sort":"name","direction":"asc","sections":"repository","layout":"table"}'),
  ('pinned', 'Pinned', 0, '{"sort":"manual","direction":"asc","sections":"none","layout":"cards"}'),
  ('hidden', 'Hidden', 0, '{}');

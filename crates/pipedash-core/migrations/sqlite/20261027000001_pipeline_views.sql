CREATE TABLE IF NOT EXISTS pipeline_views (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('all','pinned','hidden','custom')),
    name TEXT NOT NULL,
    parent_id INTEGER REFERENCES pipeline_views(id) ON DELETE SET NULL,
    position INTEGER NOT NULL DEFAULT 0,
    rules_json TEXT NOT NULL DEFAULT '{}',
    display_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS pipeline_views_system_kind ON pipeline_views(kind) WHERE kind <> 'custom';
CREATE TABLE IF NOT EXISTS pipeline_view_members (
    view_id INTEGER NOT NULL REFERENCES pipeline_views(id) ON DELETE CASCADE,
    provider_id INTEGER NOT NULL,
    pipeline_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    excluded INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (view_id, pipeline_id)
);
CREATE INDEX IF NOT EXISTS pipeline_view_members_provider ON pipeline_view_members(provider_id);
CREATE TABLE IF NOT EXISTS pipeline_view_pins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('view','repository')),
    ref TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    UNIQUE (kind, ref)
);
INSERT INTO pipeline_views (kind, name, position, display_json) VALUES
  ('all', 'All pipelines', 0, '{"sort":"name","direction":"asc","sections":"repository","layout":"table"}'),
  ('pinned', 'Pinned', 0, '{"sort":"manual","direction":"asc","sections":"none","layout":"cards"}'),
  ('hidden', 'Hidden', 0, '{}');

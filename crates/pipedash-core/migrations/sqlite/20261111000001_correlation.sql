CREATE TABLE IF NOT EXISTS source_observations (
    provider_id INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    repo TEXT NOT NULL,
    path_scopes_json TEXT NOT NULL DEFAULT '[]',
    commit_sha TEXT,
    PRIMARY KEY (subject_id, run_number, ordinal)
);
CREATE INDEX IF NOT EXISTS source_observations_repo ON source_observations(repo);
CREATE INDEX IF NOT EXISTS source_observations_repo_commit ON source_observations(repo, commit_sha);
CREATE INDEX IF NOT EXISTS source_observations_provider ON source_observations(provider_id);
CREATE TABLE IF NOT EXISTS target_observations (
    provider_id INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    cluster TEXT NOT NULL,
    namespace TEXT,
    name TEXT NOT NULL,
    lifetime TEXT NOT NULL CHECK (lifetime IN ('standing','short_lived')),
    PRIMARY KEY (subject_id, run_number, ordinal)
);
CREATE INDEX IF NOT EXISTS target_observations_cluster ON target_observations(cluster, namespace);
CREATE INDEX IF NOT EXISTS target_observations_provider ON target_observations(provider_id);
CREATE TABLE IF NOT EXISTS artifact_observations (
    provider_id INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    relation TEXT NOT NULL CHECK (relation IN ('produced','consumed')),
    ordinal INTEGER NOT NULL,
    repository TEXT NOT NULL,
    digest TEXT,
    tag TEXT,
    target_ordinal INTEGER,
    origin TEXT NOT NULL CHECK (origin IN ('provider','log','attestation')),
    PRIMARY KEY (subject_id, run_number, relation, ordinal)
);
CREATE INDEX IF NOT EXISTS artifact_observations_digest ON artifact_observations(digest);
CREATE INDEX IF NOT EXISTS artifact_observations_repository ON artifact_observations(repository);
CREATE INDEX IF NOT EXISTS artifact_observations_provider ON artifact_observations(provider_id);
CREATE TABLE IF NOT EXISTS subject_identities (
    subject_id TEXT PRIMARY KEY,
    provider_id INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    unit_id TEXT,
    unit_parent TEXT,
    unit_depends_on_json TEXT NOT NULL DEFAULT '[]',
    app_ref_authority TEXT,
    app_ref_scope TEXT,
    app_ref_id TEXT,
    app_label TEXT
);
CREATE INDEX IF NOT EXISTS subject_identities_provider ON subject_identities(provider_id);
CREATE TABLE IF NOT EXISTS run_evidence (
    pipeline_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    outcome_recorded INTEGER NOT NULL,
    timing_recorded INTEGER NOT NULL,
    attempt INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    concluded_at TEXT,
    status TEXT NOT NULL,
    PRIMARY KEY (pipeline_id, run_number)
);
CREATE TABLE IF NOT EXISTS correlation_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_kind TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_kind TEXT NOT NULL,
    to_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    confidence TEXT NOT NULL CHECK (confidence IN ('exact','suggested')),
    rule TEXT NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '{}',
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    UNIQUE (from_kind, from_id, to_kind, to_id, relation, rule)
);
CREATE INDEX IF NOT EXISTS correlation_edges_to ON correlation_edges(to_kind, to_id);
CREATE INDEX IF NOT EXISTS correlation_edges_from ON correlation_edges(from_kind, from_id);
CREATE TABLE IF NOT EXISTS applications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_kind TEXT NOT NULL CHECK (identity_kind IN ('unit','app_ref','user','config')),
    identity_key TEXT NOT NULL,
    name TEXT NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (identity_kind, identity_key)
);
CREATE TABLE IF NOT EXISTS application_members (
    application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    pipeline_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('exact','override','config')),
    edge_id INTEGER REFERENCES correlation_edges(id) ON DELETE SET NULL,
    PRIMARY KEY (application_id, pipeline_id)
);
CREATE INDEX IF NOT EXISTS application_members_pipeline ON application_members(pipeline_id);
CREATE TABLE IF NOT EXISTS application_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id INTEGER REFERENCES applications(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('link','unlink','create','merge','split','rename','hide','stage','target_name','target_group','direction')),
    subject TEXT,
    target TEXT,
    value_json TEXT NOT NULL DEFAULT '{}',
    author TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS application_overrides_application ON application_overrides(application_id);
CREATE TABLE IF NOT EXISTS application_targets (
    application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    target_key TEXT NOT NULL,
    name TEXT NOT NULL,
    group_key TEXT NOT NULL,
    lifetime TEXT NOT NULL CHECK (lifetime IN ('standing','short_lived')),
    PRIMARY KEY (application_id, target_key)
);
CREATE TABLE IF NOT EXISTS target_directions (
    application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    from_group TEXT NOT NULL,
    to_group TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('provider','override','config')),
    PRIMARY KEY (application_id, from_group, to_group)
);
CREATE TABLE IF NOT EXISTS artifacts (
    digest TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    label_source TEXT,
    label_revision TEXT,
    registry_checked_at TEXT,
    log_scanned_at TEXT
);
CREATE INDEX IF NOT EXISTS artifacts_repository ON artifacts(repository);
CREATE TABLE IF NOT EXISTS changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    repo TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    authored_at TEXT,
    UNIQUE (application_id, repo, commit_sha)
);
CREATE INDEX IF NOT EXISTS changes_commit ON changes(repo, commit_sha);
CREATE TABLE IF NOT EXISTS change_stages (
    change_id INTEGER NOT NULL REFERENCES changes(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    group_key TEXT NOT NULL DEFAULT '',
    pipeline_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    concluded_at TEXT,
    status TEXT NOT NULL,
    outcome_recorded INTEGER NOT NULL,
    timing_recorded INTEGER NOT NULL,
    attempt INTEGER NOT NULL,
    PRIMARY KEY (change_id, stage, group_key, pipeline_id, run_number)
);
CREATE INDEX IF NOT EXISTS change_stages_pipeline ON change_stages(pipeline_id, run_number);
CREATE TABLE IF NOT EXISTS observed_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pipeline_id TEXT NOT NULL,
    provider_id INTEGER NOT NULL,
    at TEXT NOT NULL,
    status_before TEXT,
    status_after TEXT NOT NULL,
    health_before TEXT,
    health_after TEXT,
    revision TEXT,
    digests_json TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX IF NOT EXISTS observed_transitions_pipeline ON observed_transitions(pipeline_id, at);
CREATE TABLE IF NOT EXISTS observation_windows (
    provider_id INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    PRIMARY KEY (provider_id, started_at)
);
CREATE TABLE IF NOT EXISTS log_scans (
    pipeline_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    run_hash TEXT NOT NULL,
    scanned_at TEXT NOT NULL,
    PRIMARY KEY (pipeline_id, run_number)
);

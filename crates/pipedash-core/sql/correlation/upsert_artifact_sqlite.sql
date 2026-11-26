INSERT INTO artifacts (digest, repository, first_seen, label_source, label_revision, registry_checked_at, log_scanned_at)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (digest) DO UPDATE SET
    repository = excluded.repository,
    first_seen = CASE WHEN excluded.first_seen < artifacts.first_seen THEN excluded.first_seen ELSE artifacts.first_seen END,
    label_source = excluded.label_source,
    label_revision = excluded.label_revision,
    registry_checked_at = excluded.registry_checked_at,
    log_scanned_at = excluded.log_scanned_at

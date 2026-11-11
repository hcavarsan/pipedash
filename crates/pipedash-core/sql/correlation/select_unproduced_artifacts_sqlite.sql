SELECT a.digest, a.repository, a.first_seen, a.label_source, a.label_revision, a.registry_checked_at, a.log_scanned_at
FROM artifacts a
WHERE EXISTS (
    SELECT 1 FROM artifact_observations o WHERE o.digest = a.digest AND o.relation = 'consumed'
)
AND NOT EXISTS (
    SELECT 1 FROM artifact_observations o WHERE o.digest = a.digest AND o.relation = 'produced'
)
ORDER BY a.digest

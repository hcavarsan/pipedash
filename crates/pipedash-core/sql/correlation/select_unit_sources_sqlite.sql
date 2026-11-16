SELECT s.provider_id, s.subject_id, s.run_number, s.ordinal, s.repo, s.path_scopes_json, s.commit_sha
FROM source_observations s
JOIN subject_identities i ON i.subject_id = s.subject_id
WHERE i.unit_id IS NOT NULL AND s.commit_sha IS NOT NULL AND s.run_number = -1
ORDER BY s.subject_id, s.ordinal

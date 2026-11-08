SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type, metadata_json, resource_state_json
FROM pipelines_cache
WHERE provider_id = $1
INSERT INTO pipelines_cache
(id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json, provider_type, resource_state_json)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
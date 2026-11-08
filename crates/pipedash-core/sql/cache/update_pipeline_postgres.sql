UPDATE pipelines_cache
SET provider_id = $1, name = $2, status = $3, repository = $4, branch = $5,
    workflow_file = $6, last_run = $7, last_updated = $8, metadata_json = $9, provider_type = $10,
    resource_state_json = $11
WHERE id = $12
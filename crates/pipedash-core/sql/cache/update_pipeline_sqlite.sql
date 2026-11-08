UPDATE pipelines_cache
SET provider_id = ?, name = ?, status = ?, repository = ?, branch = ?,
    workflow_file = ?, last_run = ?, last_updated = ?, metadata_json = ?, provider_type = ?,
    resource_state_json = ?
WHERE id = ?
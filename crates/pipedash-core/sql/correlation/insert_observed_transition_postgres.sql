INSERT INTO observed_transitions (pipeline_id, provider_id, at, status_before, status_after, health_before, health_after, revision, digests_json)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
RETURNING id

INSERT INTO application_members (application_id, pipeline_id, stage, source, edge_id)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT (application_id, pipeline_id) DO UPDATE SET
    stage = excluded.stage,
    source = excluded.source,
    edge_id = excluded.edge_id

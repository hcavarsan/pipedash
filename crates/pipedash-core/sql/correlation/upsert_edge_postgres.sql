INSERT INTO correlation_edges (from_kind, from_id, to_kind, to_id, relation, confidence, rule, evidence_json, first_seen, last_seen)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
ON CONFLICT (from_kind, from_id, to_kind, to_id, relation, rule) DO UPDATE SET
    confidence = excluded.confidence,
    evidence_json = excluded.evidence_json,
    last_seen = excluded.last_seen
RETURNING id

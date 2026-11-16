INSERT INTO application_overrides (application_id, kind, subject, target, value_json, author, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id

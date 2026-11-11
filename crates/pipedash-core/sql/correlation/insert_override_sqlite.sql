INSERT INTO application_overrides (application_id, kind, subject, target, value_json, author, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?)
RETURNING id

INSERT INTO applications (identity_kind, identity_key, name, hidden, archived_at, created_at)
VALUES ($1, $2, $3, $4, $5, $6)
ON CONFLICT (identity_kind, identity_key) DO UPDATE SET
    name = excluded.name,
    hidden = excluded.hidden,
    archived_at = excluded.archived_at
RETURNING id

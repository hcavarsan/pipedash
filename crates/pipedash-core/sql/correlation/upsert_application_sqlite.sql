INSERT INTO applications (identity_kind, identity_key, name, hidden, archived_at, created_at)
VALUES (?, ?, ?, ?, ?, ?)
ON CONFLICT (identity_kind, identity_key) DO UPDATE SET
    name = excluded.name,
    hidden = excluded.hidden,
    archived_at = excluded.archived_at
RETURNING id

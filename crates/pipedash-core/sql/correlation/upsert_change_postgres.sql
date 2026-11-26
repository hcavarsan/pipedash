INSERT INTO changes (application_id, repo, commit_sha, authored_at)
VALUES ($1, $2, $3, $4)
ON CONFLICT (application_id, repo, commit_sha) DO UPDATE SET
    authored_at = COALESCE(excluded.authored_at, changes.authored_at)
RETURNING id

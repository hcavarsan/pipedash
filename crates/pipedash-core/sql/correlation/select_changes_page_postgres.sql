SELECT id, application_id, repo, commit_sha, authored_at
FROM changes
WHERE application_id = $1
ORDER BY authored_at IS NULL, authored_at DESC, id DESC
LIMIT $2 OFFSET $3

CREATE TABLE IF NOT EXISTS vault_verifier (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

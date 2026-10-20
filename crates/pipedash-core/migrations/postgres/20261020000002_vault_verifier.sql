CREATE TABLE IF NOT EXISTS vault_verifier (
    id SMALLINT PRIMARY KEY CHECK (id = 1),
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

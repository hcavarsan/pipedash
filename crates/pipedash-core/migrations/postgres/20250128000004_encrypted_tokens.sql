-- Encrypted token storage for PostgreSQL backend
CREATE TABLE IF NOT EXISTS encrypted_tokens (
    provider_id BIGINT PRIMARY KEY REFERENCES providers(id) ON DELETE CASCADE,
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_encrypted_tokens_updated ON encrypted_tokens(updated_at);

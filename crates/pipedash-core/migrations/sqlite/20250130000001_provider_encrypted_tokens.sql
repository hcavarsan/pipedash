-- Add encrypted token columns to providers table
-- This enables AES-256-GCM encrypted token storage directly in the providers table,
-- replacing the Stronghold-based token storage for faster initialization.

ALTER TABLE providers ADD COLUMN encrypted_token BLOB;
ALTER TABLE providers ADD COLUMN token_nonce BLOB;

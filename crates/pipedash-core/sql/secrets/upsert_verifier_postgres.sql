INSERT INTO vault_verifier (id, nonce, ciphertext, kdf_salt)
VALUES (1, $1, $2, $3)
ON CONFLICT (id) DO UPDATE SET
    nonce = EXCLUDED.nonce,
    ciphertext = EXCLUDED.ciphertext,
    kdf_salt = EXCLUDED.kdf_salt
WHERE vault_verifier.kdf_salt IS NULL
